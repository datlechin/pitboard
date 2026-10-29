//! Where an account's login waits while another is signed in. Nothing here decides what to
//! delete: a park no account refers to is listed in `State::discarded` and purged from there.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::permit::Permit;
use crate::provider::{LoginKind, Provider, ProviderId};
use crate::state::{Aside, Dropped, Key, Park, ParkKind, SetAside, State};
use crate::store;
use serde_json::Value;

const PREFIX: &str = "pitboard-park-";

pub(crate) enum Orphan {
    Nothing,
    Unreadable,
    // a login whose own record names the account `id`
    Login {
        id: String,
        park: Box<Park>,
        document: Value,
    },
    Unnamed,
}

pub(crate) trait Parks: Send + Sync {
    fn taken(&self, ctx: &Context, name: &str) -> std::result::Result<bool, store::Error>;
    fn orphan(&self, ctx: &Context, state: &State, name: &str) -> Orphan;
    fn list(&self, ctx: &Context) -> std::result::Result<Option<Vec<String>>, store::Error>;
    fn remove(
        &self,
        ctx: &Context,
        permit: Permit,
        dropped: &Dropped,
    ) -> std::result::Result<(), store::Error>;
    fn shared(&self, ctx: &Context) -> bool;
}

pub(crate) fn parks(kind: ParkKind) -> &'static dyn Parks {
    match kind {
        ParkKind::Vault => &VaultParks,
        ParkKind::Folder => &FolderParks,
    }
}

pub(crate) const EVERY_KIND: [ParkKind; 2] = [ParkKind::Vault, ParkKind::Folder];

struct VaultParks;

impl Parks for VaultParks {
    fn taken(&self, ctx: &Context, name: &str) -> std::result::Result<bool, store::Error> {
        Ok(store::vault_read(ctx, name)?.is_some())
    }

    // The tool is the account's whose id the name carries: a vault item names none itself.
    fn orphan(&self, ctx: &Context, state: &State, name: &str) -> Orphan {
        let raw = match store::vault_read(ctx, name) {
            Ok(None) => return Orphan::Nothing,
            Err(_) => return Orphan::Unreadable,
            Ok(Some(raw)) => raw,
        };
        let Some((id, at_millis)) = parts_of(name) else {
            return Orphan::Unnamed;
        };
        let (Ok(document), Some(account)) = (
            serde_json::from_str::<Value>(&raw),
            state.owner_of_park(&id),
        ) else {
            return Orphan::Unnamed;
        };
        let tool = match crate::provider::of(account.provider()) {
            LoginKind::Document(tool) => tool,
            // A vault item whose id is a folder tool's account is not that account's.
            LoginKind::Folder(_) => return Orphan::Unnamed,
        };
        let park = describe(tool, name, at_millis / 1000, &document);
        if park.login.is_empty() {
            return Orphan::Unnamed;
        }
        Orphan::Login {
            id,
            park: Box::new(park),
            document,
        }
    }

    fn list(&self, ctx: &Context) -> std::result::Result<Option<Vec<String>>, store::Error> {
        store::vault_list(ctx)
    }

    fn remove(
        &self,
        ctx: &Context,
        permit: Permit,
        dropped: &Dropped,
    ) -> std::result::Result<(), store::Error> {
        store::vault_delete(ctx, permit, &dropped.service)
    }

    fn shared(&self, ctx: &Context) -> bool {
        store::vault_is_shared(ctx)
    }
}

struct FolderParks;

impl Parks for FolderParks {
    // Only "not found" says a name is free.
    fn taken(&self, ctx: &Context, name: &str) -> std::result::Result<bool, store::Error> {
        match std::fs::symlink_metadata(store::folder::at(ctx, name)) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(store::Error::Unreadable(e.to_string())),
        }
    }

    // `Login` only where the name, the manifest, this Mac, the items, the keys and the jar agree.
    fn orphan(&self, ctx: &Context, _: &State, name: &str) -> Orphan {
        match store::folder::holds_anything(ctx, name) {
            Ok(false) => return Orphan::Nothing,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Orphan::Nothing,
            Err(_) => return Orphan::Unreadable,
            Ok(true) => {}
        }
        let manifest = match store::folder::manifest(ctx, name) {
            Ok(Some(manifest)) => manifest,
            Ok(None) => return Orphan::Unnamed,
            Err(_) => return Orphan::Unreadable,
        };
        let Some(tool) = ProviderId::parse(&manifest.tool) else {
            return Orphan::Unnamed;
        };
        let app = match crate::provider::of(tool) {
            LoginKind::Folder(app) => app,
            LoginKind::Document(_) => return Orphan::Unnamed,
        };
        let id = match parts_of(name) {
            Some((id, _)) => Some(id),
            None => manifest.account.clone(),
        };
        let (Some(id), Some(login)) = (id, manifest.login.clone()) else {
            return Orphan::Unnamed;
        };
        let machine = ctx.this_machine();
        if manifest.account.as_deref() != Some(id.as_str())
            || machine == crate::state::UNKNOWN_MACHINE
            || manifest.machine != machine
        {
            return Orphan::Unnamed;
        }
        match store::folder::held_items(ctx, name, &manifest.items) {
            Ok(held) if held == manifest.items => {}
            Ok(_) => return Orphan::Unnamed,
            Err(_) => return Orphan::Unreadable,
        }
        let keys = match store::folder::keys(ctx, name) {
            Ok(keys) => keys,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Orphan::Unnamed,
            Err(_) => return Orphan::Unreadable,
        };
        let named = |keys: &crate::provider::folder::Keys| {
            keys.get(app.naming_key())
                .and_then(|value| serde_json::from_str::<String>(value.get()).ok())
        };
        match keys {
            Ok(keys) if keys.digests() == manifest.keys && named(&keys) == manifest.named => {}
            _ => return Orphan::Unnamed,
        }
        let jar = store::folder::item(ctx, name, app.items()[0]);
        match app.session(ctx, &jar) {
            Ok(Some(session)) if session.login == login => Orphan::Login {
                id,
                park: Box::new(Park {
                    service: name.to_string(),
                    kind: ParkKind::Folder,
                    parked_at: manifest.parked_at,
                    login,
                    access_expires_at: None,
                    refresh_expires_at: None,
                    session_expires_at: manifest.session_expires_at,
                }),
                document: Value::Null,
            },
            Ok(_) => Orphan::Unnamed,
            Err(_) => Orphan::Unreadable,
        }
    }

    fn list(&self, ctx: &Context) -> std::result::Result<Option<Vec<String>>, store::Error> {
        store::folder::list(ctx).map_err(|e| store::Error::Unreadable(e.to_string()))
    }

    // A folder that holds a login goes only once its account is forgotten.
    fn remove(
        &self,
        ctx: &Context,
        permit: Permit,
        dropped: &Dropped,
    ) -> std::result::Result<(), store::Error> {
        let removed = if dropped.forgotten {
            store::folder::delete(ctx, permit, &dropped.service)
        } else {
            store::folder::remove_emptied(ctx, permit, &dropped.service)
        };
        removed.map_err(|e| store::Error::Write(e.to_string()))
    }

    fn shared(&self, _: &Context) -> bool {
        false
    }
}

// Never deleted on a guess: a folder no account can take is kept in `state.aside`.
pub(crate) fn set_aside_orphan(
    ctx: &Context,
    state: &mut State,
    name: &str,
    found: &Orphan,
    at: i64,
) {
    if state.aside.iter().any(|aside| aside.name == name) {
        return;
    }
    let (account, login) = match found {
        Orphan::Login { id, park, .. } => (Some(id.clone()), Some(park.login.clone())),
        Orphan::Nothing | Orphan::Unreadable | Orphan::Unnamed => (None, None),
    };
    state.aside.push(Aside {
        name: name.to_string(),
        tool: folder_tool(ctx, name),
        at,
        why: SetAside::Orphan,
        account,
        login,
    });
}

// The tool a folder's manifest names, or the one folder tool where it names none.
fn folder_tool(ctx: &Context, name: &str) -> ProviderId {
    let folders = || {
        ProviderId::ALL
            .iter()
            .copied()
            .filter(|&tool| match crate::provider::of(tool) {
                LoginKind::Folder(_) => true,
                LoginKind::Document(_) => false,
            })
    };
    store::folder::manifest(ctx, name)
        .ok()
        .flatten()
        .and_then(|manifest| ProviderId::parse(&manifest.tool))
        .filter(|tool| folders().any(|folder| folder == *tool))
        .or_else(|| folders().next())
        .expect("a tool whose login is a folder")
}

pub(crate) fn service_name(id: &str, at_millis: i64) -> String {
    format!("{PREFIX}{id}-{at_millis}")
}

/// A name Pitboard made, rather than one Claude Code did. Nothing deletes an item without
/// this being true of its name.
pub(crate) fn is_park_name(service: &str) -> bool {
    parts_of(service).is_some()
}

/// The account id and the moment a name carries. An account id contains dashes, so the
/// moment is taken from the end.
pub(crate) fn parts_of(service: &str) -> Option<(String, i64)> {
    let rest = service.strip_prefix(PREFIX)?;
    let (id, millis) = rest.rsplit_once('-')?;
    let at_millis: i64 = millis.parse().ok()?;
    (!id.is_empty()).then(|| (id.to_string(), at_millis))
}

/// Claim a free name before writing to it, so the caller can record it first and recovery
/// can find a park left by a run that died. Reusing a name would destroy the park there.
pub(crate) fn reserve(ctx: &Context, permit: Permit, id: &str) -> Result<String> {
    crate::state::record_vault(ctx, permit)?;
    let service = free_name(ctx, id)?;
    // Written down before anything is written into it, so a run killed between the two
    // leaves a name the next command can resolve rather than a login nothing on the machine
    // can see.
    crate::pending::reserve(ctx, permit, &service)?;
    Ok(service)
}

/// A name for `id` that nothing in the vault holds yet, for a caller that writes it down
/// itself before it writes into it.
pub(crate) fn free_name(ctx: &Context, id: &str) -> Result<String> {
    let start = ctx.now_millis();
    for offset in 0..1_000 {
        let candidate = service_name(id, start + offset);
        if !parks(ParkKind::Vault).taken(ctx, &candidate)? {
            return Ok(candidate);
        }
    }
    Err(Error::ParkSlotExhausted)
}

// A folder for `id`'s login under `parks`, written down before it is made.
pub(crate) fn reserve_folder(
    ctx: &Context,
    permit: Permit,
    state: &State,
    id: &str,
) -> Result<String> {
    free_folder(ctx, permit, state, |at_millis| service_name(id, at_millis))
}

// A folder under `parks` for what a move sets aside, written down before it is made.
pub(crate) fn reserve_aside(ctx: &Context, permit: Permit, state: &State) -> Result<String> {
    free_folder(ctx, permit, state, |at_millis| {
        format!("{}{at_millis}", store::folder::ASIDE_PREFIX)
    })
}

// Never a name `state` holds: a move names a folder before it makes it.
fn free_folder(
    ctx: &Context,
    permit: Permit,
    state: &State,
    name: impl Fn(i64) -> String,
) -> Result<String> {
    let start = ctx.now_millis();
    for offset in 0..1_000 {
        let candidate = name(start + offset);
        if !state.names(&candidate) && !parks(ParkKind::Folder).taken(ctx, &candidate)? {
            crate::pending::reserve_folder(ctx, permit, &candidate)?;
            return Ok(candidate);
        }
    }
    Err(Error::ParkSlotExhausted)
}

/// Whether a login can be parked at all on this machine, and how.
///
/// On macOS a park goes into the keychain through `security`, whose stdin takes 4032 bytes.
/// A Claude Code slice is a few hundred, but a Codex login is its whole `auth.json`, over
/// four kilobytes before it is hex-encoded, so every Codex park is past the ceiling. Above
/// it the write goes on the argument line, which is said, or is refused where
/// `PITBOARD_NO_ARGV` forbids that, before anything has moved rather than halfway through.
pub(crate) fn price(
    ctx: &Context,
    provider: ProviderId,
    label: &str,
    service: &str,
    document: &Value,
) -> Result<Option<crate::warning::Warning>> {
    let body = serde_json::to_string(document).expect("a credential slice is always serialisable");
    let Some(price) = store::vault_cost(ctx, service, &body) else {
        return Ok(None);
    };
    if price.refused() {
        return Err(Error::CredentialTooLarge {
            tool: provider,
            label: label.to_string(),
            bytes: price.needs,
            limit: price.limit,
            store: price.store,
        });
    }
    Ok(price
        .on_the_second_route()
        .then_some(crate::warning::Warning::WrittenOnTheCommandLine {
            tool: provider,
            bytes: price.needs,
            limit: price.limit,
        }))
}

/// A new park of `document`, a login of the account filed under `id`, which `label` names:
/// what it costs is asked first, then a free name is reserved and written. The caller
/// records it, and deletes it where that fails. What parking it warns of comes with it.
pub(crate) fn keep(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
    label: &str,
    id: &str,
    document: &Value,
) -> Result<(Park, Option<crate::warning::Warning>)> {
    let parking = price(
        ctx,
        tool.id(),
        label,
        &service_name(id, ctx.now_millis()),
        document,
    )?;
    let service = reserve(ctx, permit, id)?;
    let park = store_at(ctx, permit, tool, &service, document)?;
    Ok((park, parking))
}

/// Write a login into a reserved name and prove it reads back.
pub(crate) fn store_at(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
    service: &str,
    document: &Value,
) -> Result<Park> {
    let park = describe(tool, service, ctx.now(), document);
    if park.login.is_empty() {
        return Err(Error::LiveCredentialShapeUnexpected {
            tool: tool.id(),
            detail: "it has no refresh token, so it could never be restored".into(),
        });
    }
    let body = serde_json::to_string(document).expect("a credential slice is always serialisable");
    store::vault_write(ctx, permit, service, &body)?;
    Ok(park)
}

/// What the account index records about a login: nothing secret.
pub(crate) fn describe(
    tool: &'static dyn Provider,
    service: &str,
    parked_at: i64,
    document: &Value,
) -> Park {
    // Through the provider: where the dates are and what unit they are in is a fact about
    // the tool, and the three disagree on both.
    let expiry = tool.expiry(document);
    Park {
        service: service.to_string(),
        parked_at,
        kind: ParkKind::Vault,
        login: tool.fingerprint(document),
        access_expires_at: expiry.access_expires_at,
        refresh_expires_at: expiry.refresh_expires_at,
        session_expires_at: None,
    }
}

/// Takes the account so a failure names it, not an item the user has never seen.
pub(crate) fn load(
    ctx: &Context,
    tool: &'static dyn Provider,
    key: &Key,
    park: &Park,
) -> Result<Value> {
    let label = key.typed();
    let raw =
        store::vault_read(ctx, &park.service)?.ok_or_else(|| Error::ParkedCredentialMissing {
            label: label.clone(),
        })?;
    let value: Value = serde_json::from_str(&raw).map_err(|e| Error::ParkedCredentialCorrupt {
        label: label.clone(),
        detail: e.to_string(),
    })?;
    let found = tool.fingerprint(&value);
    if park.login.is_empty() || found != park.login {
        return Err(Error::ParkedCredentialCorrupt {
            label,
            detail: "it does not match the fingerprint Pitboard recorded".into(),
        });
    }
    Ok(value)
}

/// Whether this parked document is a second copy of the login signed in now, for a tool
/// whose park may never be one.
///
/// Answered from the two refresh tokens' fingerprints, so it needs no network. For a tool
/// whose own sign-out revokes what it finds, keeping such a copy is keeping a token the
/// person's next sign-out would end in both places; it is discarded instead. For a tool
/// that tolerates a copy this is never true, and what was always done still is.
pub(crate) fn is_live_twin(ctx: &Context, tool: &'static dyn Provider, document: &Value) -> bool {
    if tool.park_semantics() != crate::provider::ParkSemantics::MoveOnly {
        return false;
    }
    let parked = tool.fingerprint(document);
    !parked.is_empty() && live_fingerprint(ctx, tool).is_some_and(|live| live == parked)
}

/// The fingerprint of the refresh token the tool has in use now, where there is one and it
/// can be read.
fn live_fingerprint(ctx: &Context, tool: &'static dyn Provider) -> Option<String> {
    let live = tool.read_live(ctx).ok().flatten()?;
    Some(tool.fingerprint(&live.raw)).filter(|found| !found.is_empty())
}

// One refresh token in two places, which a renewal would spend. A folder login is never copied.
pub(crate) fn live_twins(ctx: &Context, state: &State) -> Vec<String> {
    let mut twins = Vec::new();
    for &provider in ProviderId::ALL {
        let held: Vec<&Park> = state
            .accounts
            .iter()
            .filter(|a| a.provider() == provider)
            .filter_map(|a| a.parked.as_ref())
            .collect();
        if held.is_empty() || (provider == ProviderId::Claude && crate::settings::custom_oauth(ctx))
        {
            continue;
        }
        let tool = match crate::provider::of(provider) {
            LoginKind::Document(tool) => tool,
            LoginKind::Folder(_) => continue,
        };
        let Some(live) = live_fingerprint(ctx, tool) else {
            continue;
        };
        twins.extend(
            held.into_iter()
                .filter(|park| park.login == live)
                .map(|park| park.service.clone()),
        );
    }
    twins
}

/// Delete every discarded item, keeping listed only those that resisted. Returns how many
/// remain.
pub(crate) fn purge(ctx: &Context, permit: Permit, state: &mut State) -> usize {
    state
        .discarded
        .retain(|dropped| parks(dropped.kind).remove(ctx, permit, dropped).is_err());
    state.discarded.len()
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::host::memory::MemoryHost;
    use crate::store::memory::Fault;
    use crate::time::FixedClock;
    use serde_json::json;
    use std::sync::Arc;

    fn work() -> Key {
        Key::new(ProviderId::Claude, "work")
    }

    fn oauth(token: &str) -> Value {
        json!({
            "refreshToken": token,
            "accessToken": "a",
            "expiresAt": 1_790_000_000_000i64,
            "refreshTokenExpiresAt": 1_792_000_000_000i64
        })
    }

    /// A machine whose stores are in memory, whose clock stands still, and whose home is a
    /// scratch directory: reserving a name writes it down before it is used.
    fn machine() -> (Context, Arc<MemoryHost>, tempfile::TempDir) {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let mem = MemoryHost::new();
        let clock = Arc::new(FixedClock::at(1_760_000_000));
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root)
            .with_memory_stores(Arc::clone(&mem))
            .with_clock(clock as Arc<dyn crate::time::Clock>);
        (ctx, mem, scratch)
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn a_folder_that_holds_an_item_is_not_removed_unless_forgotten() {
        use crate::host::fs::testing;
        let (ctx, mem, _scratch) = machine();
        let permit = Permit::for_a_test();
        let parks = store::folder::parks_dir(&ctx);
        let (held, emptied) = ("pitboard-park-desktop_a-1", "pitboard-park-desktop_a-2");
        std::fs::create_dir_all(parks.join(held).join("items")).unwrap();
        std::fs::write(parks.join(held).join("items/Cookies"), b"a jar").unwrap();
        std::fs::write(parks.join(held).join("keys.json"), b"{}").unwrap();
        std::fs::create_dir_all(parks.join(emptied).join("items/IndexedDB")).unwrap();
        std::fs::write(parks.join(emptied).join("manifest.json"), b"{}").unwrap();
        testing::make_private(&crate::home::dir(&ctx)).unwrap();
        testing::make_private(&parks).unwrap();

        let mut state = State::default();
        state.discard(held, ParkKind::Folder);
        state.discard(emptied, ParkKind::Folder);
        assert_eq!(
            purge(&ctx, permit, &mut state),
            1,
            "the one that holds an item stays"
        );
        assert_eq!(state.discarded[0].service, held);
        assert!(parks.join(held).join("items/Cookies").exists());
        assert!(!parks.join(emptied).exists());

        state.discarded[0].forgotten = true;
        assert_eq!(purge(&ctx, permit, &mut state), 0);
        assert!(!parks.join(held).exists());
        assert!(
            mem.trail().iter().all(|touch| touch.store != "vault"),
            "no folder is asked of the vault: {:?}",
            mem.trail()
        );
    }

    #[test]
    fn a_folder_name_is_free_only_where_nothing_is_seen_there() {
        use crate::host::fs::testing;
        let (ctx, _mem, _scratch) = machine();
        let folders = parks(ParkKind::Folder);
        let dir = store::folder::parks_dir(&ctx);
        let (held, free) = ("pitboard-park-desktop_a-1", "pitboard-park-desktop_a-2");
        std::fs::create_dir_all(dir.join(held)).unwrap();
        assert!(folders.taken(&ctx, held).unwrap());
        assert!(!folders.taken(&ctx, free).unwrap());

        testing::deny_reading(&dir).unwrap();
        let unseen = folders.taken(&ctx, free);
        // The system's administrator is denied nothing, so there is nothing to see there.
        let denied = std::fs::symlink_metadata(dir.join(held)).is_err();
        testing::make_private(&dir).unwrap();
        if denied {
            assert!(
                matches!(unseen, Err(store::Error::Unreadable(_))),
                "{unseen:?}"
            );
        }
    }

    /// Park, then read it back through the same rules a switch uses. Before the stores were
    /// a seam this needed a real keychain, so it only ran on one platform and only against
    /// whatever the machine happened to hold.
    #[test]
    fn a_parked_login_reads_back_through_its_fingerprint() {
        let (ctx, mem, _scratch) = machine();
        let name = reserve(&ctx, Permit::for_a_test(), "acc").expect("a free name");
        let park = store_at(
            &ctx,
            Permit::for_a_test(),
            crate::provider::document(ProviderId::Claude),
            &name,
            &oauth("r"),
        )
        .expect("stored");

        assert_eq!(mem.vault().services(), vec![name.clone()]);
        assert_eq!(
            load(
                &ctx,
                crate::provider::document(ProviderId::Claude),
                &work(),
                &park
            )
            .expect("loads"),
            oauth("r")
        );
    }

    /// The distinction the whole store layer exists to keep. A park that is gone is gone and
    /// the account needs signing in to again; a park that cannot be read says nothing about
    /// whether it is there, and telling someone to sign in again would be wrong.
    #[test]
    fn a_park_that_vanished_and_one_that_cannot_be_read_are_different_answers() {
        let (ctx, mem, _scratch) = machine();
        let name = reserve(&ctx, Permit::for_a_test(), "acc").expect("a free name");
        let park = store_at(
            &ctx,
            Permit::for_a_test(),
            crate::provider::document(ProviderId::Claude),
            &name,
            &oauth("r"),
        )
        .expect("stored");

        mem.vault().fault(&name, Fault::Vanish);
        assert!(matches!(
            load(
                &ctx,
                crate::provider::document(ProviderId::Claude),
                &work(),
                &park
            ),
            Err(Error::ParkedCredentialMissing { .. })
        ));

        mem.vault()
            .fault(&name, Fault::Unreadable("the keychain is locked".into()));
        assert!(
            matches!(
                load(
                    &ctx,
                    crate::provider::document(ProviderId::Claude),
                    &work(),
                    &park
                ),
                Err(Error::Store(store::Error::Unreadable(_)))
            ),
            "a store that could not answer must never read as an absent login"
        );
    }

    /// Two parks of one account in the same millisecond must not share a name: reusing one
    /// would destroy the login already there.
    #[test]
    fn a_reserved_name_steps_past_one_that_is_taken() {
        let (ctx, mem, _scratch) = machine();
        let first = reserve(&ctx, Permit::for_a_test(), "acc").expect("a free name");
        mem.vault().plant(&first, "{}");
        let second = reserve(&ctx, Permit::for_a_test(), "acc").expect("another free name");
        assert_ne!(first, second, "the clock has not moved, so the name must");
    }

    /// A login with nothing to restore is refused before it is written, so the vault never
    /// holds a park that could not be used.
    #[test]
    fn a_login_with_no_refresh_token_is_never_parked() {
        let (ctx, mem, _scratch) = machine();
        let name = reserve(&ctx, Permit::for_a_test(), "acc").expect("a free name");
        assert!(matches!(
            store_at(
                &ctx,
                Permit::for_a_test(),
                crate::provider::document(ProviderId::Claude),
                &name,
                &json!({"accessToken": "a"})
            ),
            Err(Error::LiveCredentialShapeUnexpected { .. })
        ));
        assert!(mem.vault().services().is_empty());
    }

    #[test]
    fn a_park_records_when_its_login_stops_working() {
        let park = describe(
            crate::provider::document(ProviderId::Claude),
            "pitboard-park-x-1",
            50,
            &serde_json::json!({
                "refreshToken": "r",
                "expiresAt": 1_790_000_000_123i64,
                "refreshTokenExpiresAt": 1_792_000_000_999i64
            }),
        );
        assert_eq!(park.access_expires_at, Some(1_790_000_000));
        assert_eq!(park.refresh_expires_at, Some(1_792_000_000));
        assert_eq!(
            park.login,
            crate::provider::claude::document::fingerprint_of(&serde_json::json!({
                "refreshToken": "r"
            }))
        );
    }

    #[test]
    fn service_names_carry_the_account_and_the_moment() {
        let s = service_name("1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0", 1789935600123);
        assert_eq!(
            s,
            "pitboard-park-1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0-1789935600123"
        );
        assert!(
            s.starts_with("pitboard-park-"),
            "must never collide with a Claude Code item"
        );
    }

    /// Its fingerprint would be the empty string, which every later check would match.
    #[test]
    fn a_credential_with_no_refresh_token_is_refused_rather_than_parked() {
        let refused = store_at(
            &Context::for_unit_test(),
            Permit::for_a_test(),
            crate::provider::document(ProviderId::Claude),
            "pitboard-park-test-no-refresh",
            &serde_json::json!({"accessToken": "a"}),
        );
        assert!(refused.is_err());
    }
}
