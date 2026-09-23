//! Names pitboard is about to write a login into, written down before the login is.
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
use crate::state::State;
use crate::{atomic, home, park, store};
use std::path::PathBuf;

fn path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("pending")
}

fn read(ctx: &Context) -> Vec<String> {
    std::fs::read_to_string(path(ctx))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn store_list(ctx: &Context, names: &[String]) -> Result<()> {
    let path = path(ctx);
    if names.is_empty() {
        // Nothing outstanding: leave no file rather than an empty one, so the ordinary
        // state of a machine is the absence of this.
        match std::fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(source) => return Err(Error::HomeUnwritable { path, source }),
        }
    }
    let mut body = names.join("\n");
    body.push('\n');
    atomic::write(&path, body.as_bytes(), atomic::Perms::Secret)
        .map_err(|source| Error::HomeUnwritable { path, source })
}

/// Write a name down before anything is written into it. A name here that never receives a
/// login costs one line and is dropped by the next sweep.
pub fn reserve(ctx: &Context, service: &str) -> Result<()> {
    home::ensure(ctx).map_err(|source| Error::HomeUnwritable {
        path: home::dir(ctx),
        source,
    })?;
    let mut names = read(ctx);
    if !names.iter().any(|n| n == service) {
        names.push(service.to_string());
    }
    store_list(ctx, &names)
}

/// What resolving unnamed logins did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reclaimed {
    /// Given back to the account whose name they carry: (label, service).
    pub given_back: Vec<(String, String)>,
    /// Listed for deletion. Only ever a name this pitboard wrote down itself and nothing
    /// recorded, which is the one case where being sure is possible.
    pub deleted: Vec<String>,
    /// Found in the store, belonging to no account here and never written down here.
    /// Reported and not touched: the store is shared by the whole machine and pitboard's
    /// records are not, so an item pitboard cannot account for may be another pitboard's.
    pub strangers: Vec<String>,
    /// Left exactly as they are, because the store could not be read.
    pub unreadable: Vec<String>,
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
    }
}

/// Resolve every name pitboard wrote down and nothing refers to any more.
///
/// Runs in every `settle`, after the journal has had its say, so a switch's own park is
/// already accounted for by then. It reads pitboard's own list rather than asking the
/// store, because it is on the path of every change and a keychain dump is not free.
pub fn sweep(ctx: &Context, state: &mut State) -> Result<Reclaimed> {
    let ours = read(ctx);
    resolve(ctx, state, ours.clone(), &ours)
}

/// The same, asking the store what is actually there rather than trusting pitboard's list.
/// This is what finds a login whose name was lost with the list, or written by a version
/// that had no list. `pitboard repair` runs it; nothing else does, because on macOS it
/// dumps the keychain.
pub fn reclaim(ctx: &Context, state: &mut State) -> Result<Reclaimed> {
    let ours = read(ctx);
    let Some(stored) = store::vault_list(ctx)? else {
        // A store that cannot be enumerated: pitboard's own list is all there is.
        return resolve(ctx, state, ours.clone(), &ours);
    };
    let mut names = stored;
    for listed in &ours {
        if !names.contains(listed) {
            names.push(listed.clone());
        }
    }
    resolve(ctx, state, names, &ours)
}

/// `ours` is the list of names this pitboard wrote down before creating them. It is what
/// separates an item that may be deleted from one that may only be reported.
///
/// The asymmetry matters more than anything else here. On macOS the keychain belongs to the
/// whole login session while pitboard's records belong to one `PITBOARD_HOME`, so an item
/// this pitboard cannot account for is not evidence of an orphan: it may be another
/// pitboard's parked login, and deleting it would end that account's session for someone
/// who never ran this command. Giving a login back is additive and safe to do on a guess;
/// deleting one is not, and is done only where being sure is possible.
fn resolve(
    ctx: &Context,
    state: &mut State,
    names: Vec<String>,
    ours: &[String],
) -> Result<Reclaimed> {
    let mut keep = Vec::new();
    let mut out = Reclaimed::default();
    for service in names {
        if state.names(&service) {
            continue;
        }
        let written_here = ours.contains(&service);
        match store::vault_read(ctx, &service) {
            // Claimed but never written to. Nothing was ever there.
            Ok(None) => {}
            // Cannot tell. Try again next time rather than guess.
            Err(_) => {
                out.unreadable.push(service.clone());
                if written_here {
                    keep.push(service);
                }
            }
            Ok(Some(raw)) => match adopt(ctx, state, &service, &raw) {
                Some(label) => out.given_back.push((label, service)),
                None if written_here => {
                    // This pitboard wrote this name down, wrote a login into it, and
                    // nothing here recorded it. That is an orphan and nothing else can be.
                    discard(ctx, state, &service);
                    out.deleted.push(service);
                }
                None => out.strangers.push(service),
            },
        }
    }
    if keep != ours {
        store_list(ctx, &keep)?;
    }
    if !out.given_back.is_empty() || !out.deleted.is_empty() {
        crate::state::save(ctx, state)?;
    }
    Ok(out)
}

/// Give an orphan back to the account whose name it carries, where that account is enrolled
/// and holds nothing. Anything else is refused: an account that already holds a park has a
/// login pitboard renews, and a second copy of one refresh chain is the state that ends a
/// login for both holders.
fn adopt(ctx: &Context, state: &mut State, service: &str, raw: &str) -> Option<String> {
    let (uuid, at_millis) = park::parts_of(service)?;
    let oauth = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    let label = state
        .by_uuid(&uuid)
        .filter(|a| a.parked.is_none())
        .map(|a| a.label.clone())?;
    let park = park::describe(service, at_millis / 1000, &oauth);
    if park.refresh_fingerprint.is_empty() || !park.restorable_at(ctx.now()) {
        return None;
    }
    state.park(&label, park);
    crate::audit::record(ctx, "reclaim", &label, "ok");
    Some(label)
}

/// List it for deletion the way every other unwanted park is listed, so a delete that fails
/// is retried rather than forgotten.
fn discard(ctx: &Context, state: &mut State, service: &str) {
    debug_assert!(park::is_park_name(service), "only pitboard's own names");
    state.discard(service);
    crate::audit::record(ctx, "reclaim", service, "discarded");
}

/// Forget everything listed, without touching the vault. Only `uninstall` does this, once
/// it has deleted what the names refer to.
pub fn clear(ctx: &Context) {
    let _ = std::fs::remove_file(path(ctx));
}

/// Every name still outstanding, for `doctor` to report.
pub fn outstanding(ctx: &Context) -> Vec<String> {
    read(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::memory::MemoryHost;
    use crate::time::{Clock, FixedClock};
    use serde_json::json;
    use std::sync::Arc;

    const NOW: i64 = 1_760_000_000;

    fn machine(name: &str) -> (Context, Arc<MemoryHost>, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-pending-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mem = MemoryHost::new();
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.clone())
            .with_memory_stores(Arc::clone(&mem))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        home::ensure(&ctx).expect("a home");
        (ctx, mem, root)
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
            label: label.into(),
            account_uuid: uuid.into(),
            email: format!("{uuid}@example.com"),
            organization_uuid: "org".into(),
            oauth_account: json!({}),
            parked: None,
        }
    }

    #[test]
    fn a_name_claimed_but_never_written_to_is_simply_dropped() {
        let (ctx, _mem, root) = machine("never-written");
        reserve(&ctx, "pitboard-park-acc-1760000000000").expect("reserved");
        let mut state = State::default();

        assert_eq!(sweep(&ctx, &mut state).expect("swept").found(), 0);
        assert!(outstanding(&ctx).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_orphan_goes_back_to_the_account_whose_name_it_carries() {
        let (ctx, mem, root) = machine("adopt");
        let service = "pitboard-park-acc-1760000000000";
        reserve(&ctx, service).expect("reserved");
        mem.vault().plant(service, &oauth("r").to_string());

        let mut state = State::default();
        state.accounts.push(account("work", "acc"));
        assert_eq!(sweep(&ctx, &mut state).expect("swept").found(), 1);

        let park = state
            .get("work")
            .expect("account")
            .parked
            .clone()
            .expect("the orphan was taken back");
        assert_eq!(park.service, service);
        assert_eq!(
            park.parked_at, 1_760_000_000,
            "when it was parked is in its own name"
        );
        assert!(outstanding(&ctx).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Two copies of one refresh chain is the state that ends a login for both holders, so
    /// the copy nothing names loses.
    #[test]
    fn an_orphan_for_an_account_that_already_holds_one_is_deleted() {
        let (ctx, mem, root) = machine("already-held");
        let held = "pitboard-park-acc-1750000000000";
        let orphan = "pitboard-park-acc-1760000000000";
        mem.vault().plant(held, &oauth("held").to_string());
        mem.vault().plant(orphan, &oauth("orphan").to_string());
        reserve(&ctx, orphan).expect("reserved");

        let mut state = State::default();
        state.accounts.push(account("work", "acc"));
        state.park("work", park::describe(held, NOW, &oauth("held")));

        assert_eq!(sweep(&ctx, &mut state).expect("swept").found(), 1);
        assert!(state.names(held), "the recorded copy is untouched");
        assert_eq!(
            state
                .get("work")
                .expect("account")
                .parked
                .as_ref()
                .map(|p| p.service.as_str()),
            Some(held),
            "the recorded copy is the one pitboard renews and the one it keeps"
        );
        assert!(state.discarded.iter().any(|s| s == orphan));
        assert!(outstanding(&ctx).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    /// A store that could not answer says nothing about what is in it.
    #[test]
    fn an_item_that_cannot_be_read_stays_listed_rather_than_being_guessed_at() {
        let (ctx, mem, root) = machine("unreadable");
        let service = "pitboard-park-acc-1760000000000";
        reserve(&ctx, service).expect("reserved");
        mem.vault().plant(service, &oauth("r").to_string());
        mem.vault().fault(
            service,
            crate::store::memory::Fault::Unreadable("the keychain is locked".into()),
        );

        let mut state = State::default();
        assert_eq!(sweep(&ctx, &mut state).expect("swept").found(), 0);
        assert_eq!(outstanding(&ctx), vec![service.to_string()]);
        assert!(state.discarded.is_empty(), "nothing is deleted on a guess");
        let _ = std::fs::remove_dir_all(root);
    }

    /// The case the list alone cannot answer: a login in the vault that pitboard never
    /// wrote a name down for, because the home was lost, or because it was parked by a
    /// version that kept no list. Only asking the store finds it.
    #[test]
    fn asking_the_store_finds_a_login_the_list_never_knew_about() {
        let (ctx, mem, root) = machine("reclaim");
        let service = "pitboard-park-acc-1760000000000";
        mem.vault().plant(service, &oauth("r").to_string());
        assert!(
            outstanding(&ctx).is_empty(),
            "nothing was ever written down"
        );

        let mut state = State::default();
        state.accounts.push(account("work", "acc"));

        // The cheap sweep cannot see it, because it only reads pitboard's own list.
        assert_eq!(sweep(&ctx, &mut state).expect("swept").found(), 0);
        assert!(state.get("work").expect("account").parked.is_none());

        let reclaimed = reclaim(&ctx, &mut state).expect("reclaimed");
        assert_eq!(reclaimed.given_back.len(), 1);
        assert_eq!(reclaimed.given_back[0].0, "work");
        assert_eq!(
            state
                .get("work")
                .expect("account")
                .parked
                .as_ref()
                .map(|p| p.service.as_str()),
            Some(service)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// A login for an account this machine has never heard of. Keeping it would be keeping
    /// a credential nothing can ever use, renew or name.
    /// The rule that matters most here. A keychain belongs to a login session; pitboard's
    /// records belong to one PITBOARD_HOME. So a parked login this pitboard cannot account
    /// for is not evidence of an orphan, it is evidence of another pitboard, and deleting
    /// it would end that account's session for somebody who never ran this command.
    #[test]
    fn a_login_this_pitboard_never_wrote_down_is_reported_and_not_touched() {
        let (ctx, mem, root) = machine("stranger");
        let service = "pitboard-park-stranger-1760000000000";
        mem.vault().plant(service, &oauth("r").to_string());

        let mut state = State::default();
        let reclaimed = reclaim(&ctx, &mut state).expect("reclaimed");

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
        let _ = std::fs::remove_dir_all(root);
    }

    /// The one case where being sure is possible: this pitboard wrote the name down, wrote
    /// a login into it, and nothing here recorded it.
    #[test]
    fn a_login_this_pitboard_wrote_down_and_nothing_wants_is_deleted() {
        let (ctx, mem, root) = machine("our-orphan");
        let service = "pitboard-park-stranger-1760000000000";
        mem.vault().plant(service, &oauth("r").to_string());
        reserve(&ctx, service).expect("written down first");

        let mut state = State::default();
        let reclaimed = reclaim(&ctx, &mut state).expect("reclaimed");

        assert_eq!(reclaimed.deleted, vec![service.to_string()]);
        assert!(reclaimed.strangers.is_empty());
        assert!(state.discarded.iter().any(|s| s == service));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_name_the_state_already_carries_is_not_swept_at_all() {
        let (ctx, mem, root) = machine("already-named");
        let service = "pitboard-park-acc-1760000000000";
        reserve(&ctx, service).expect("reserved");
        mem.vault().plant(service, &oauth("r").to_string());

        let mut state = State::default();
        state.accounts.push(account("work", "acc"));
        state.park("work", park::describe(service, NOW, &oauth("r")));

        assert_eq!(sweep(&ctx, &mut state).expect("swept").found(), 0);
        assert!(outstanding(&ctx).is_empty());
        assert!(state.discarded.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}
