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

/// Resolve every name nothing refers to any more. Returns how many logins were found that
/// the state did not name.
///
/// Runs after the journal has had its say, so a switch's own park is already accounted for
/// by then. An item that cannot be read is left listed: a store that could not answer says
/// nothing about what is in it, and deleting on that basis is how a login is lost.
pub fn sweep(ctx: &Context, state: &mut State) -> Result<usize> {
    let listed = read(ctx);
    if listed.is_empty() {
        return Ok(0);
    }
    let mut keep = Vec::new();
    let mut found = 0;
    let mut adopted = false;
    for service in listed {
        if state.names(&service) {
            continue;
        }
        match store::vault_read(ctx, &service) {
            // Claimed but never written to. Nothing was ever there.
            Ok(None) => {}
            // Cannot tell. Try again next time rather than guess.
            Err(_) => keep.push(service),
            Ok(Some(raw)) => {
                found += 1;
                if adopt(ctx, state, &service, &raw) {
                    adopted = true;
                } else {
                    // pitboard's own name, pitboard's own item, and nothing wants it.
                    discard(ctx, state, &service);
                }
            }
        }
    }
    store_list(ctx, &keep)?;
    if adopted || found > 0 {
        crate::state::save(ctx, state)?;
    }
    Ok(found)
}

/// Give an orphan back to the account whose name it carries, where that account is enrolled
/// and holds nothing. Anything else is refused: an account that already holds a park has a
/// login pitboard renews, and a second copy of one refresh chain is the state that ends a
/// login for both holders.
fn adopt(ctx: &Context, state: &mut State, service: &str, raw: &str) -> bool {
    let Some((uuid, at_millis)) = park::parts_of(service) else {
        return false;
    };
    let Ok(oauth) = serde_json::from_str::<serde_json::Value>(raw) else {
        return false;
    };
    let Some(label) = state
        .by_uuid(&uuid)
        .filter(|a| a.parked.is_none())
        .map(|a| a.label.clone())
    else {
        return false;
    };
    let park = park::describe(service, at_millis / 1000, &oauth);
    if park.refresh_fingerprint.is_empty() || !park.restorable_at(ctx.now()) {
        return false;
    }
    state.park(&label, park);
    crate::audit::record(ctx, "reclaim", &label, "ok");
    true
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
    use crate::store::memory::MemoryPlatform;
    use crate::time::{Clock, FixedClock};
    use serde_json::json;
    use std::sync::Arc;

    const NOW: i64 = 1_760_000_000;

    fn machine(name: &str) -> (Context, Arc<MemoryPlatform>, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-pending-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mem = MemoryPlatform::new();
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

        assert_eq!(sweep(&ctx, &mut state).expect("swept"), 0);
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
        assert_eq!(sweep(&ctx, &mut state).expect("swept"), 1);

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

        assert_eq!(sweep(&ctx, &mut state).expect("swept"), 1);
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
        assert_eq!(sweep(&ctx, &mut state).expect("swept"), 0);
        assert_eq!(outstanding(&ctx), vec![service.to_string()]);
        assert!(state.discarded.is_empty(), "nothing is deleted on a guess");
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

        assert_eq!(sweep(&ctx, &mut state).expect("swept"), 0);
        assert!(outstanding(&ctx).is_empty());
        assert!(state.discarded.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}
