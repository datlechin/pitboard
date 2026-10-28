//! Names Pitboard is about to write a login into, written down before the login is.
//!
//! The switch got this right and nothing else did. `enroll --sign-in` and the renewal that
//! runs inside every `pitboard status` both claimed a name, wrote a login into it, and only
//! then recorded it, with cleanup that runs when a step returns an error and never when a
//! run is killed. Killed in that window, the machine keeps a live refresh token that no
//! entry in `state.json` names: never renewed, never offered, not removed by
//! `pitboard uninstall`, and on macOS not listable by any tool the user has, because
//! `security find-generic-password` takes no wildcard.
//!
//! So every name is written here first and resolved by the next command. This is an index,
//! not a vault: it holds names, never a token.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::park::Orphan;
use crate::permit::Permit;
use crate::provider::LoginKind;
use crate::state::{Key, Park, ParkKind, State};
use crate::{atomic, home, park};
use std::path::PathBuf;

fn path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("pending")
}

fn read(ctx: &Context) -> Vec<(ParkKind, String)> {
    std::fs::read_to_string(path(ctx))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| match line.strip_prefix(FOLDER) {
            Some(name) => (ParkKind::Folder, name.to_owned()),
            None => (ParkKind::Vault, line.to_owned()),
        })
        .collect()
}

// A name holds no space, so a folder's line splits exactly.
const FOLDER: &str = "folder ";

fn store_list(ctx: &Context, permit: Permit, names: &[(ParkKind, String)]) -> Result<()> {
    let path = path(ctx);
    if names.is_empty() {
        // Nothing outstanding: leave no file rather than an empty one, so the ordinary
        // state of a machine is the absence of this.
        match crate::host::fs::remove_file(permit, &path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(source) => return Err(Error::HomeUnwritable { path, source }),
        }
    }
    let lines: Vec<String> = names
        .iter()
        .map(|(kind, name)| match kind {
            ParkKind::Vault => name.clone(),
            ParkKind::Folder => format!("{FOLDER}{name}"),
        })
        .collect();
    let mut body = lines.join("\n");
    body.push('\n');
    atomic::write(permit, &path, body.as_bytes(), atomic::Perms::Secret)
        .map_err(|source| Error::HomeUnwritable { path, source })
}

/// Write a name down before anything is written into it. A name here that never receives a
/// login costs one line and is dropped by the next sweep.
pub(crate) fn reserve(ctx: &Context, permit: Permit, service: &str) -> Result<()> {
    write_down(ctx, permit, (ParkKind::Vault, service.to_string()))
}

// A folder under `parks`, written down before it is made.
pub(crate) fn reserve_folder(ctx: &Context, permit: Permit, name: &str) -> Result<()> {
    write_down(ctx, permit, (ParkKind::Folder, name.to_string()))
}

fn write_down(ctx: &Context, permit: Permit, line: (ParkKind, String)) -> Result<()> {
    home::ensure(ctx, permit).map_err(|source| Error::HomeUnwritable {
        path: home::dir(ctx),
        source,
    })?;
    let mut names = read(ctx);
    if !names.contains(&line) {
        names.push(line);
    }
    store_list(ctx, permit, &names)
}

/// What resolving unnamed logins did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reclaimed {
    /// Given back to the account whose name they carry: (label, service).
    pub given_back: Vec<(String, String)>,
    /// Listed for deletion. Only ever a name this Pitboard wrote down itself and nothing
    /// recorded, which is the one case where being sure is possible.
    pub deleted: Vec<String>,
    /// Found in the store, belonging to no account here and never written down here.
    /// Reported and not touched: the store is shared by the whole machine and Pitboard's
    /// records are not, so an item Pitboard cannot account for may be another Pitboard's.
    pub strangers: Vec<String>,
    /// Left exactly as they are, because the store could not be read.
    pub unreadable: Vec<String>,
    // Folders under `parks` no account here can take, kept in `state.aside`, never deleted.
    pub set_aside: Vec<String>,
}

impl Reclaimed {
    pub fn found(&self) -> usize {
        self.given_back.len() + self.deleted.len()
    }

    pub fn is_empty(&self) -> bool {
        self.given_back.is_empty()
            && self.deleted.is_empty()
            && self.strangers.is_empty()
            && self.unreadable.is_empty()
            && self.set_aside.is_empty()
    }
}

/// Resolve every name Pitboard wrote down and nothing refers to any more.
///
/// Runs in every `settle`, after the journal has had its say, so a switch's own park is
/// already accounted for by then. It reads Pitboard's own list rather than asking the
/// store, because it is on the path of every change and a keychain dump is not free.
pub(crate) fn sweep(ctx: &Context, permit: Permit, state: &mut State) -> Result<Reclaimed> {
    let ours = read(ctx);
    resolve(ctx, permit, state, ours.clone(), &ours)
}

/// The same, asking the store what is actually there rather than trusting Pitboard's list.
/// This is what finds a login whose name was lost with the list, or written by a version
/// that had no list. `pitboard repair` runs it; nothing else does, because on macOS it
/// dumps the keychain.
pub(crate) fn reclaim(ctx: &Context, permit: Permit, state: &mut State) -> Result<Reclaimed> {
    let ours = read(ctx);
    let mut names = Vec::new();
    for kind in park::EVERY_KIND {
        // A store that cannot be enumerated: Pitboard's own list is all there is.
        if let Some(stored) = park::parks(kind).list(ctx)? {
            names.extend(stored.into_iter().map(|name| (kind, name)));
        }
    }
    for listed in &ours {
        if !names.contains(listed) {
            names.push(listed.clone());
        }
    }
    resolve(ctx, permit, state, names, &ours)
}

/// `ours` is the list of names this Pitboard wrote down before creating them. It is what
/// separates an item that may be deleted from one that may only be reported.
///
/// The asymmetry matters more than anything else here. On macOS the keychain belongs to the
/// whole login session while Pitboard's records belong to one `PITBOARD_HOME`, so an item
/// this Pitboard cannot account for is not evidence of an orphan: it may be another
/// Pitboard's parked login, and deleting it would end that account's session for someone
/// who never ran this command. Giving a login back is additive and safe to do on a guess;
/// deleting one is not, and is done only where being sure is possible. Where the vault is
/// shared that way, a login given back that this Pitboard did not write down is recorded
/// as such, so letting it go later never deletes it either: only using it does.
fn resolve(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    names: Vec<(ParkKind, String)>,
    ours: &[(ParkKind, String)],
) -> Result<Reclaimed> {
    let mut keep = Vec::new();
    let mut out = Reclaimed::default();
    for (kind, service) in names {
        if state.names(&service) {
            continue;
        }
        let written_here = ours.iter().any(|(k, name)| *k == kind && *name == service);
        let found = park::parks(kind).orphan(ctx, state, &service);
        match kind {
            ParkKind::Folder => resolve_folder(
                ctx,
                permit,
                state,
                &service,
                found,
                written_here,
                &mut out,
                &mut keep,
            ),
            ParkKind::Vault => match found {
                // Claimed but never written to. Nothing was ever there.
                Orphan::Nothing => {}
                // Cannot tell. Try again next time rather than guess.
                Orphan::Unreadable => {
                    out.unreadable.push(service.clone());
                    if written_here {
                        keep.push((kind, service));
                    }
                }
                found => match adopt(ctx, permit, state, found, written_here) {
                    Some(label) => out.given_back.push((label, service)),
                    None if written_here => {
                        // This Pitboard wrote this name down, wrote a login into it, and
                        // nothing here recorded it. That is an orphan and nothing else can be.
                        release(ctx, permit, state, &service);
                        out.deleted.push(service);
                    }
                    None => out.strangers.push(service),
                },
            },
        }
    }
    // The state first, so a run killed between the two leaves no login neither file names.
    if !out.given_back.is_empty() || !out.deleted.is_empty() || !out.set_aside.is_empty() {
        crate::state::save(ctx, permit, state)?;
    }
    crate::fault::point("pending.recorded");
    if keep != ours {
        store_list(ctx, permit, &keep)?;
    }
    Ok(out)
}

// Given back to the account that takes it, and otherwise set aside, written here or not.
#[allow(clippy::too_many_arguments)]
fn resolve_folder(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    name: &str,
    found: Orphan,
    written_here: bool,
    out: &mut Reclaimed,
    keep: &mut Vec<(ParkKind, String)>,
) {
    let taker = match &found {
        Orphan::Login { id, .. } => takes(ctx, state, id),
        Orphan::Nothing | Orphan::Unreadable | Orphan::Unnamed => None,
    };
    match (found, taker) {
        // Reserved and never filled: what is there holds nothing.
        (Orphan::Nothing, _) => {
            let _ = crate::store::folder::remove_emptied(ctx, permit, name);
        }
        (Orphan::Unreadable, _) => {
            out.unreadable.push(name.to_string());
            if written_here {
                keep.push((ParkKind::Folder, name.to_string()));
            }
        }
        (Orphan::Login { park, .. }, Some(key)) => give_back(ctx, permit, state, &key, *park, out),
        (found, _) => {
            park::set_aside_orphan(ctx, state, name, &found, ctx.now());
            out.set_aside.push(name.to_string());
        }
    }
}

// Enrolled, holding no park that is there, and not in use: an older login of the account in use
// stays aside.
pub(crate) fn takes(ctx: &Context, state: &State, id: &str) -> Option<Key> {
    let account = state.owner_of_park(id)?;
    let tool = account.provider();
    let in_use = state.in_use(tool).and_then(|record| record.account(tool));
    let holds = account
        .parked
        .as_ref()
        .is_some_and(|held| !gone(ctx, state, held));
    (!holds && in_use.as_deref() != Some(id)).then(|| account.key())
}

// A recorded folder park whose folder is not found, as a `state.json` restored from a backup names.
fn gone(ctx: &Context, state: &State, park: &Park) -> bool {
    park.kind == ParkKind::Folder
        && !state.moves(&park.service)
        && matches!(
            park::parks(ParkKind::Folder).taken(ctx, &park.service),
            Ok(false)
        )
}

pub(crate) fn give_back(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    key: &Key,
    park: Park,
    out: &mut Reclaimed,
) {
    let name = park.service.clone();
    let held = state.get(key).and_then(|account| account.parked.clone());
    if let Some(held) = held.filter(|held| gone(ctx, state, held)) {
        state.discard(&held.service, ParkKind::Folder);
    }
    state.park(key, park);
    let label = key.typed();
    crate::audit::record(ctx, permit, "reclaim", &label, "ok");
    out.given_back.push((label, name));
}

/// Give an orphan back to the account whose name it carries, where that account is enrolled
/// and holds nothing, or holds an older copy this one replaces.
///
/// The second case is what a renewal leaves when it is killed after writing the fresh login
/// and before recording it: the service has already spent the older copy's refresh token,
/// so the account holds a dead login and the orphan is its only live one. Keeping the older
/// and deleting the newer, which is what happened, lost the login. Only a name this Pitboard
/// wrote down itself is trusted that far; a stranger replaces nothing. The older copy is
/// released rather than discarded, because an enrolment killed at the same point leaves the
/// same shape with an older copy nothing spent, and an older copy `repair` gave back may be
/// another Pitboard's.
///
/// Anything else is refused: an account that already holds a newer park has a login
/// Pitboard renews, and a second copy of one refresh chain is the state that ends a login
/// for both holders.
fn adopt(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    found: Orphan,
    written_here: bool,
) -> Option<String> {
    let Orphan::Login { id, park, document } = found else {
        return None;
    };
    let account = state.owner_of_park(&id)?;
    let key = account.key();
    let tool = match crate::provider::of(key.provider) {
        LoginKind::Document(tool) => tool,
        // A vault line whose id is a folder tool's account is not given back.
        LoginKind::Folder(_) => return None,
    };
    if !park.restorable_at(ctx.now()) {
        return None;
    }
    let replaces_an_older_copy =
        |held: &Park| written_here && held.login != park.login && park.parked_at >= held.parked_at;
    if account
        .parked
        .as_ref()
        .is_some_and(|held| !replaces_an_older_copy(held))
    {
        return None;
    }
    // A copy of the login signed in now is not something to give back, for a tool whose
    // park may never be a copy.
    if park::is_live_twin(ctx, tool, &document) {
        return None;
    }
    // A vault of files lives inside this home, so whatever is in it is this Pitboard's
    // whether or not it was written down, and is deleted like any other once let go.
    if written_here || !park::parks(ParkKind::Vault).shared(ctx) {
        state.park(&key, *park);
    } else {
        state.park_foreign(&key, *park);
    }
    let label = key.typed();
    crate::audit::record(ctx, permit, "reclaim", &label, "ok");
    Some(label)
}

/// List it for deletion the way every other unwanted park is listed, so a delete that fails
/// is retried rather than forgotten.
fn release(ctx: &Context, permit: Permit, state: &mut State, service: &str) {
    debug_assert!(park::is_park_name(service), "only Pitboard's own names");
    state.release(service);
    crate::audit::record(ctx, permit, "reclaim", service, "discarded");
}

/// Forget everything listed, without touching the vault. Only `uninstall` does this, once
/// it has deleted what the names refer to.
pub(crate) fn clear(ctx: &Context, permit: Permit) {
    let _ = crate::host::fs::remove_file(permit, &path(ctx));
}

// Every name written down for a vault item, which a move of the parks takes to the new form.
pub(crate) fn vault_names(ctx: &Context) -> Vec<String> {
    read(ctx)
        .into_iter()
        .filter(|(kind, _)| *kind == ParkKind::Vault)
        .map(|(_, service)| service)
        .collect()
}

/// Every name still outstanding, for `doctor` to report: written down, and named by nothing
/// in `state`. A change records its park in `state.json` once the login is written, and the
/// name stays on the list until the next change's sweep drops it; until then it is
/// accounted for, not outstanding. Where the state could not be read, every name is.
pub(crate) fn outstanding(ctx: &Context, state: Option<&State>) -> Vec<String> {
    read(ctx)
        .into_iter()
        .map(|(_, service)| service)
        .filter(|service| !state.is_some_and(|state| state.names(service)))
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::host::memory::MemoryHost;
    use crate::provider::ProviderId;
    use crate::store;
    use crate::time::{Clock, FixedClock};
    use serde_json::json;
    use std::sync::Arc;

    const NOW: i64 = 1_760_000_000;

    fn work() -> Key {
        Key::new(ProviderId::Claude, "work")
    }

    fn machine() -> (Context, Arc<MemoryHost>, tempfile::TempDir) {
        machine_on(MemoryHost::new())
    }

    fn machine_on(mem: Arc<MemoryHost>) -> (Context, Arc<MemoryHost>, tempfile::TempDir) {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path().join("root");
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root)
            .with_memory_stores(Arc::clone(&mem))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        home::ensure(&ctx, Permit::for_a_test()).expect("a home");
        (ctx, mem, scratch)
    }

    fn oauth(refresh: &str) -> serde_json::Value {
        json!({
            "refreshToken": refresh,
            "accessToken": "a",
            "expiresAt": (NOW + 3600) * 1000,
            "refreshTokenExpiresAt": (NOW + 30 * 86_400) * 1000
        })
    }

    fn account(label: &str, uuid: &str) -> crate::state::Account {
        crate::state::Account {
            last_used_at: None,
            replaced_at: None,
            plan: None,
            label: label.into(),
            id: uuid.into(),
            account_uuid: uuid.into(),
            email: format!("{uuid}@example.com"),
            detail: crate::state::Detail::Claude {
                organization_uuid: "org".into(),
                oauth_account: json!({}),
            },
            parked: None,
        }
    }

    #[test]
    fn a_name_claimed_but_never_written_to_is_simply_dropped() {
        let (ctx, _mem, _root) = machine();
        reserve(
            &ctx,
            Permit::for_a_test(),
            "pitboard-park-acc-1760000000000",
        )
        .expect("reserved");
        let mut state = State::default();

        assert_eq!(
            sweep(&ctx, Permit::for_a_test(), &mut state)
                .expect("swept")
                .found(),
            0
        );
        assert!(outstanding(&ctx, None).is_empty());
    }

    #[test]
    fn a_name_the_state_records_is_not_outstanding() {
        let (ctx, _mem, _root) = machine();
        let service = "pitboard-park-acc-1760000000000";
        reserve(&ctx, Permit::for_a_test(), service).expect("reserved");
        let mut state = State::default();
        state.accounts.push(account("work", "acc"));
        state.park(
            &work(),
            park::describe(
                crate::provider::document(ProviderId::Claude),
                service,
                NOW,
                &oauth("r"),
            ),
        );

        assert!(outstanding(&ctx, Some(&state)).is_empty());
        assert_eq!(outstanding(&ctx, None), [service]);
        assert_eq!(outstanding(&ctx, Some(&State::default())), [service]);
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn a_folder_name_in_pending_is_never_asked_of_the_vault() {
        let (ctx, mem, _root) = machine();
        let (gone, unread) = ("pitboard-park-desktop_a-1", "pitboard-aside-2");
        let folder = store::folder::at(&ctx, unread);
        std::fs::create_dir_all(folder.join("items")).unwrap();
        std::fs::write(folder.join("items/Cookies"), b"a jar").unwrap();
        std::fs::write(folder.join("manifest.json"), b"not json").unwrap();
        std::fs::write(
            path(&ctx),
            format!("folder {gone}\npitboard-park-acc-1\nfolder {unread}\n"),
        )
        .unwrap();
        assert_eq!(
            read(&ctx),
            [
                (ParkKind::Folder, gone.to_string()),
                (ParkKind::Vault, "pitboard-park-acc-1".to_string()),
                (ParkKind::Folder, unread.to_string()),
            ]
        );

        let mut state = State::default();
        let swept = sweep(&ctx, Permit::for_a_test(), &mut state).expect("swept");
        assert_eq!(swept.unreadable, [unread]);
        let asked: Vec<String> = mem.trail().into_iter().map(|touch| touch.service).collect();
        assert_eq!(asked, ["pitboard-park-acc-1"], "the vault's line alone");
        assert_eq!(
            std::fs::read_to_string(path(&ctx)).unwrap(),
            format!("folder {unread}\n"),
            "a folder's line is written back as a folder's"
        );
        assert!(folder.join("items/Cookies").exists());
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn an_orphan_folder_is_set_aside_and_a_vault_orphan_is_deleted_as_before() {
        let (ctx, mem, _root) = machine();
        let (vault, folder) = (
            "pitboard-park-acc-1760000000000",
            "pitboard-park-desktop_a-1",
        );
        reserve(&ctx, Permit::for_a_test(), vault).expect("reserved");
        mem.vault().plant(vault, &oauth("r").to_string());
        let kept = store::folder::at(&ctx, folder);
        std::fs::create_dir_all(kept.join("items")).unwrap();
        std::fs::write(kept.join("items/Cookies"), b"a jar").unwrap();
        let mut listed = std::fs::read_to_string(path(&ctx)).unwrap();
        listed.push_str(&format!("folder {folder}\n"));
        std::fs::write(path(&ctx), listed).unwrap();

        let mut state = State::default();
        let swept = sweep(&ctx, Permit::for_a_test(), &mut state).expect("swept");
        assert_eq!(
            swept.deleted,
            [vault],
            "nobody's, written here: deleted as before"
        );
        assert_eq!(swept.set_aside, [folder]);
        assert_eq!(
            state.discarded,
            [crate::state::Dropped {
                service: vault.into(),
                kind: ParkKind::Vault,
                forgotten: false,
            }]
        );
        assert_eq!(state.aside.len(), 1);
        assert_eq!(state.aside[0].name, folder);
        assert_eq!(state.aside[0].why, crate::state::SetAside::Orphan);
        assert_eq!(state.aside[0].tool, ProviderId::Desktop);
        assert!(kept.join("items/Cookies").exists(), "never deleted");
        assert!(!path(&ctx).exists(), "both lines resolved");
        let saved = crate::state::load(&ctx).expect("saved");
        assert_eq!(saved.aside, state.aside);
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn a_sweep_killed_between_its_two_writes_leaves_no_login_unnamed() {
        let (ctx, mem, _root) = machine();
        let (vault, folder) = (
            "pitboard-park-acc-1760000000000",
            "pitboard-park-desktop_a-1",
        );
        reserve(&ctx, Permit::for_a_test(), vault).expect("reserved");
        mem.vault().plant(vault, &oauth("r").to_string());
        let kept = store::folder::at(&ctx, folder);
        std::fs::create_dir_all(kept.join("items")).unwrap();
        std::fs::write(kept.join("items/Cookies"), b"a jar").unwrap();
        let mut listed = std::fs::read_to_string(path(&ctx)).unwrap();
        listed.push_str(&format!("folder {folder}\n"));
        std::fs::write(path(&ctx), &listed).unwrap();

        let mut state = State::default();
        let died = crate::fault::killing("pending.recorded", || {
            sweep(&ctx, Permit::for_a_test(), &mut state)
        });
        assert_eq!(died.unwrap_err(), "pending.recorded");
        let saved = crate::state::load(&ctx).expect("saved");
        assert!(saved.names(vault), "listed to be deleted");
        assert!(saved.names(folder), "set aside");
        assert_eq!(
            std::fs::read_to_string(path(&ctx)).unwrap(),
            listed,
            "the lines go only once the state names them"
        );

        let mut state = saved;
        let again = sweep(&ctx, Permit::for_a_test(), &mut state).expect("swept");
        assert!(again.is_empty(), "{again:?}");
        assert!(!path(&ctx).exists(), "both lines dropped");
        assert_eq!(state.aside.len(), 1);
        assert!(kept.join("items/Cookies").exists());
    }

    #[test]
    fn an_orphan_goes_back_to_the_account_whose_name_it_carries() {
        let (ctx, mem, _root) = machine();
        let service = "pitboard-park-acc-1760000000000";
        reserve(&ctx, Permit::for_a_test(), service).expect("reserved");
        mem.vault().plant(service, &oauth("r").to_string());

        let mut state = State::default();
        state.accounts.push(account("work", "acc"));
        assert_eq!(
            sweep(&ctx, Permit::for_a_test(), &mut state)
                .expect("swept")
                .found(),
            1
        );

        let park = state
            .get(&Key::new(ProviderId::Claude, "work"))
            .expect("account")
            .parked
            .clone()
            .expect("the orphan was taken back");
        assert_eq!(park.service, service);
        assert_eq!(
            park.parked_at, 1_760_000_000,
            "when it was parked is in its own name"
        );
        assert!(state.foreign.is_empty(), "this Pitboard wrote it");
        assert!(outstanding(&ctx, None).is_empty());
    }

    /// The account a name was written for, holding `held`, with `orphan` in the vault under
    /// a name this Pitboard wrote down and nothing recorded.
    fn holding(
        held: (&str, &str, i64),
        orphan: (&str, &str),
    ) -> (Context, State, tempfile::TempDir) {
        let (ctx, mem, root) = machine();
        mem.vault().plant(held.0, &oauth(held.1).to_string());
        mem.vault().plant(orphan.0, &oauth(orphan.1).to_string());
        reserve(&ctx, Permit::for_a_test(), orphan.0).expect("reserved");
        let mut state = State::default();
        state.accounts.push(account("work", "acc"));
        state.park(
            &work(),
            park::describe(
                crate::provider::document(ProviderId::Claude),
                held.0,
                held.2,
                &oauth(held.1),
            ),
        );
        (ctx, state, root)
    }

    fn parked(state: &State) -> Option<String> {
        state
            .get(&work())
            .and_then(|a| a.parked.as_ref())
            .map(|p| p.service.clone())
    }

    /// Two copies of one refresh chain is the state that ends a login for both holders, so
    /// the copy nothing names loses.
    #[test]
    fn a_second_copy_of_the_chain_an_account_holds_is_deleted() {
        let held = "pitboard-park-acc-1750000000000";
        let orphan = "pitboard-park-acc-1760000000000";
        let (ctx, mut state, _root) = holding((held, "r", NOW), (orphan, "r"));

        assert_eq!(
            sweep(&ctx, Permit::for_a_test(), &mut state)
                .expect("swept")
                .found(),
            1
        );
        assert_eq!(parked(&state).as_deref(), Some(held));
        assert!(state.discarded.iter().any(|d| d.service == orphan));
        assert!(outstanding(&ctx, None).is_empty());
    }

    /// An orphan older than what the account holds is left over from before it, and the
    /// recorded copy is the one Pitboard renews.
    #[test]
    fn an_older_copy_than_the_one_an_account_holds_is_deleted() {
        let held = "pitboard-park-acc-1760000000000";
        let orphan = "pitboard-park-acc-1750000000000";
        let (ctx, mut state, _root) = holding((held, "held", NOW), (orphan, "orphan"));

        assert_eq!(
            sweep(&ctx, Permit::for_a_test(), &mut state)
                .expect("swept")
                .found(),
            1
        );
        assert_eq!(parked(&state).as_deref(), Some(held));
        assert!(state.discarded.iter().any(|d| d.service == orphan));
    }

    /// A newer copy this Pitboard wrote and never recorded is what a renewal killed between
    /// writing and recording leaves. The service spent the older copy's chain when it
    /// answered, so the newer one is the account's only working login and replaces it.
    #[test]
    fn a_newer_copy_written_here_replaces_the_one_an_account_holds() {
        let held = "pitboard-park-acc-1750000000000";
        let orphan = "pitboard-park-acc-1760000000000";
        let (ctx, mut state, _root) = holding((held, "spent", 1_750_000_000), (orphan, "fresh"));

        let reclaimed = sweep(&ctx, Permit::for_a_test(), &mut state).expect("swept");
        assert_eq!(reclaimed.given_back.len(), 1);
        assert_eq!(parked(&state).as_deref(), Some(orphan));
        assert!(
            state.discarded.iter().any(|d| d.service == held),
            "the spent copy goes"
        );
    }

    /// A store that could not answer says nothing about what is in it.
    #[test]
    fn an_item_that_cannot_be_read_stays_listed_rather_than_being_guessed_at() {
        let (ctx, mem, _root) = machine();
        let service = "pitboard-park-acc-1760000000000";
        reserve(&ctx, Permit::for_a_test(), service).expect("reserved");
        mem.vault().plant(service, &oauth("r").to_string());
        mem.vault().fault(
            service,
            store::memory::Fault::Unreadable("the keychain is locked".into()),
        );

        let mut state = State::default();
        assert_eq!(
            sweep(&ctx, Permit::for_a_test(), &mut state)
                .expect("swept")
                .found(),
            0
        );
        assert_eq!(outstanding(&ctx, None), vec![service.to_string()]);
        assert!(state.discarded.is_empty(), "nothing is deleted on a guess");
    }

    // Windows' vault is this home's alone, and a park there that will not open is still kept.
    #[test]
    fn a_sealed_park_that_will_not_open_is_never_deleted() {
        let (ctx, mem, _root) = machine_on(MemoryHost::sealed());
        assert_eq!(store::vault_kind(&ctx), store::Backend::SealedFile);
        assert!(!store::vault_is_shared(&ctx));
        let ours = "pitboard-park-acc-1760000000000";
        let stranger = "pitboard-park-other-1760000000001";
        reserve(&ctx, Permit::for_a_test(), ours).expect("written down");
        for service in [ours, stranger] {
            mem.vault().plant(service, &oauth("r").to_string());
            mem.vault().fault(
                service,
                store::memory::Fault::Unreadable("it does not open for this account".into()),
            );
        }
        let mut state = State::default();
        state.accounts.push(account("work", "acc"));
        state.accounts.push(account("other", "other"));

        let reclaimed = reclaim(&ctx, Permit::for_a_test(), &mut state).expect("reclaimed");
        assert_eq!(reclaimed.unreadable, [ours, stranger]);
        assert!(reclaimed.deleted.is_empty() && reclaimed.given_back.is_empty());

        // A readable park no account refers to, discarded: purge deletes only it, and leaves
        // the two unreadable parks in the vault.
        let gone = "pitboard-park-gone-1760000000002";
        mem.vault().plant(gone, &oauth("g").to_string());
        state.discarded.push(crate::state::Dropped {
            service: gone.to_string(),
            kind: ParkKind::Vault,
            forgotten: false,
        });
        assert_eq!(park::purge(&ctx, Permit::for_a_test(), &mut state), 0);
        assert!(state.discarded.is_empty(), "the readable park is purged");
        assert!(
            mem.vault().peek(gone).is_none(),
            "the discarded park is gone"
        );
        assert_eq!(
            outstanding(&ctx, Some(&state)),
            [ours],
            "asked again next time"
        );
        for service in [ours, stranger] {
            assert!(
                mem.vault().peek(service).is_some(),
                "{service} is still there"
            );
        }
    }

    /// The case the list alone cannot answer: a login in the vault that Pitboard never
    /// wrote a name down for, because the home was lost, or because it was parked by a
    /// version that kept no list. Only asking the store finds it.
    #[test]
    fn asking_the_store_finds_a_login_the_list_never_knew_about() {
        let (ctx, mem, _root) = machine();
        let service = "pitboard-park-acc-1760000000000";
        mem.vault().plant(service, &oauth("r").to_string());
        assert!(
            outstanding(&ctx, None).is_empty(),
            "nothing was ever written down"
        );

        let mut state = State::default();
        state.accounts.push(account("work", "acc"));

        // The cheap sweep cannot see it, because it only reads Pitboard's own list.
        assert_eq!(
            sweep(&ctx, Permit::for_a_test(), &mut state)
                .expect("swept")
                .found(),
            0
        );
        assert!(state.get(&work()).expect("account").parked.is_none());

        let reclaimed = reclaim(&ctx, Permit::for_a_test(), &mut state).expect("reclaimed");
        assert_eq!(reclaimed.given_back.len(), 1);
        assert_eq!(reclaimed.given_back[0].0, "work");
        assert_eq!(
            state
                .get(&Key::new(ProviderId::Claude, "work"))
                .expect("account")
                .parked
                .as_ref()
                .map(|p| p.service.as_str()),
            Some(service)
        );
        assert_eq!(
            state.foreign,
            vec![service.to_string()],
            "nothing says this Pitboard wrote it, so it may be another's"
        );
    }

    /// A login for an account this machine has never heard of. Keeping it would be keeping
    /// a credential nothing can ever use, renew or name.
    /// The rule that matters most here. A keychain belongs to a login session; Pitboard's
    /// records belong to one PITBOARD_HOME. So a parked login this Pitboard cannot account
    /// for is not evidence of an orphan, it is evidence of another Pitboard, and deleting
    /// it would end that account's session for somebody who never ran this command.
    #[test]
    fn a_login_this_pitboard_never_wrote_down_is_reported_and_not_touched() {
        let (ctx, mem, _root) = machine();
        let service = "pitboard-park-stranger-1760000000000";
        mem.vault().plant(service, &oauth("r").to_string());

        let mut state = State::default();
        let reclaimed = reclaim(&ctx, Permit::for_a_test(), &mut state).expect("reclaimed");

        assert!(reclaimed.given_back.is_empty());
        assert!(
            reclaimed.deleted.is_empty(),
            "nothing may be deleted on a guess"
        );
        assert_eq!(reclaimed.strangers, vec![service.to_string()]);
        assert!(state.discarded.is_empty());
        assert_eq!(
            mem.vault().peek(service).as_deref(),
            Some(oauth("r").to_string().as_str()),
            "it is still there"
        );
    }

    /// The one case where being sure is possible: this Pitboard wrote the name down, wrote
    /// a login into it, and nothing here recorded it.
    #[test]
    fn a_login_this_pitboard_wrote_down_and_nothing_wants_is_deleted() {
        let (ctx, mem, _root) = machine();
        let service = "pitboard-park-stranger-1760000000000";
        mem.vault().plant(service, &oauth("r").to_string());
        reserve(&ctx, Permit::for_a_test(), service).expect("written down first");

        let mut state = State::default();
        let reclaimed = reclaim(&ctx, Permit::for_a_test(), &mut state).expect("reclaimed");

        assert_eq!(reclaimed.deleted, vec![service.to_string()]);
        assert!(reclaimed.strangers.is_empty());
        assert!(state.discarded.iter().any(|d| d.service == service));
    }

    #[test]
    fn a_name_the_state_already_carries_is_not_swept_at_all() {
        let (ctx, mem, _root) = machine();
        let service = "pitboard-park-acc-1760000000000";
        reserve(&ctx, Permit::for_a_test(), service).expect("reserved");
        mem.vault().plant(service, &oauth("r").to_string());

        let mut state = State::default();
        state.accounts.push(account("work", "acc"));
        state.park(
            &work(),
            park::describe(
                crate::provider::document(ProviderId::Claude),
                service,
                NOW,
                &oauth("r"),
            ),
        );

        assert_eq!(
            sweep(&ctx, Permit::for_a_test(), &mut state)
                .expect("swept")
                .found(),
            0
        );
        assert!(outstanding(&ctx, None).is_empty());
        assert!(state.discarded.is_empty());
    }
}
