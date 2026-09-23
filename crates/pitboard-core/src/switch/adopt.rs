//! Taking over a pitboard directory that another machine wrote.
//!
//! The machine stamp is right and the reason for it is sound: a parked login is a refresh
//! token, and two machines taking turns presenting one makes Claude Code drop the login on
//! both. What was wrong was the shape of the refusal. Every command reads the state, so a
//! stamp that does not match stopped all of them, including `uninstall` and, in effect,
//! `doctor`. A person who buys a new Mac and lets Migration Assistant bring their home and
//! their keychain across arrives at a tool that will not do anything at all, with an error
//! telling them to enrol their accounts again and no command that makes that possible.
//!
//! So this is that command. It keeps everything that is a fact about an account rather than
//! about a machine: the label, the email, the account and organisation uuid, and the
//! numbers pitboard remembers. It drops every parked login, because a login is the one
//! thing that does not move. The way back is one sign-in per account.
//!
//! What it deliberately does not do is ask Anthropic whether a parked token still works.
//! Finding out means exchanging it, and exchanging it is exactly the act that would rotate
//! it past the other machine's copy. Until someone has measured, on a throwaway account and
//! two machines, whether an exchange invalidates the copy the other machine holds, there is
//! no safe probe and the honest default is to drop the login and say so.

use super::{exclusive, purge};
use crate::context::Context;
use crate::error::Result;
use crate::{audit, state};

/// What taking over found.
#[derive(Debug, PartialEq, Eq)]
pub struct Adopted {
    /// The accounts kept, in the order they were enrolled.
    pub accounts: Vec<String>,
    /// Accounts that arrived holding a parked login, now dropped.
    pub logins_dropped: Vec<String>,
}

/// Take over this pitboard directory. `None` when it was already this machine's, which is
/// the ordinary case and not an error: running it when there is nothing to do says so.
pub fn adopt(ctx: &Context) -> Result<Option<Adopted>> {
    let _exclusive = exclusive(ctx)?;
    let (mut state, here) = state::load_any_machine(ctx)?;
    if here {
        return Ok(None);
    }

    let accounts: Vec<String> = state.accounts.iter().map(|a| a.label.clone()).collect();
    let mut logins_dropped = Vec::new();
    for account in &mut state.accounts {
        if let Some(park) = account.parked.take() {
            logins_dropped.push(account.label.clone());
            // Listed rather than deleted outright, so a delete that fails is retried. On a
            // machine that did not receive the keychain there is nothing there to delete,
            // and deleting what is not there succeeds.
            state.discarded.push(park.service);
        }
    }
    state.active = None;
    state.slot = None;
    state.machine = state::machine_id();
    state::save(ctx, &state)?;
    purge(ctx, &mut state);

    audit::record(ctx, "adopt", "", "ok");
    Ok(Some(Adopted {
        accounts,
        logins_dropped,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Account, State};
    use crate::store::memory::MemoryHost;
    use crate::time::{Clock, FixedClock};
    use serde_json::json;
    use std::sync::Arc;

    const NOW: i64 = 1_760_000_000;

    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn machine(name: &str) -> (Context, Arc<MemoryHost>, Scratch) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-adopt-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mem = MemoryHost::new();
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.clone())
            .with_memory_stores(Arc::clone(&mem))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        crate::home::ensure(&ctx).expect("a home");
        (ctx, mem, Scratch(root))
    }

    fn oauth() -> serde_json::Value {
        json!({
            "refreshToken": "r",
            "accessToken": "a",
            "expiresAt": (NOW + 3600) * 1000,
            "refreshTokenExpiresAt": (NOW + 30 * 86_400) * 1000
        })
    }

    /// A home that came from somewhere else, with a login in the keychain that came with it.
    fn from_elsewhere(ctx: &Context, mem: &MemoryHost) -> String {
        let service = "pitboard-park-acc-1750000000000";
        mem.vault().plant(service, &oauth().to_string());
        let mut state = State {
            machine: "a hash from another computer".into(),
            ..State::default()
        };
        state.accounts.push(Account {
            last_used_at: None,
            label: "work".into(),
            account_uuid: "acc".into(),
            email: "me@example.com".into(),
            organization_uuid: "org".into(),
            oauth_account: json!({"accountUuid": "acc"}),
            parked: Some(crate::park::describe(service, NOW, &oauth())),
        });
        state.active = Some("work".into());
        let raw = serde_json::to_string(&state).expect("serialisable");
        std::fs::write(crate::home::dir(ctx).join("state.json"), raw).expect("written");
        service.to_string()
    }

    #[test]
    fn a_home_from_another_machine_keeps_its_accounts_and_loses_its_logins() {
        let (ctx, mem, _scratch) = machine("takeover");
        let service = from_elsewhere(&ctx, &mem);

        assert!(
            matches!(
                state::load(&ctx),
                Err(crate::error::Error::StateWrongMachine { .. })
            ),
            "every other command still refuses it"
        );

        let adopted = adopt(&ctx)
            .expect("adopting")
            .expect("there was work to do");
        assert_eq!(adopted.accounts, vec!["work".to_string()]);
        assert_eq!(adopted.logins_dropped, vec!["work".to_string()]);

        let state = state::load(&ctx).expect("now it is this machine's");
        let account = state.get("work").expect("the account is kept");
        assert_eq!(account.email, "me@example.com");
        assert_eq!(account.account_uuid, "acc");
        assert!(
            account.parked.is_none(),
            "the login is the one thing that does not move"
        );
        assert!(
            mem.vault().peek(&service).is_none(),
            "and the copy that came with it is deleted rather than left to be presented"
        );
        assert!(
            state.active.is_none(),
            "who was signed in was true elsewhere"
        );
    }

    #[test]
    fn adopting_a_home_that_is_already_this_machines_does_nothing() {
        let (ctx, _mem, _scratch) = machine("nothing-to-do");
        let mut state = State::default();
        state.accounts.push(Account {
            last_used_at: None,
            label: "work".into(),
            account_uuid: "acc".into(),
            email: "me@example.com".into(),
            organization_uuid: "org".into(),
            oauth_account: json!({}),
            parked: None,
        });
        state::save(&ctx, &state).expect("saved");

        assert_eq!(adopt(&ctx).expect("adopting"), None);
        assert_eq!(
            state::load(&ctx).expect("still readable").accounts.len(),
            1,
            "and it changed nothing"
        );
    }

    #[test]
    fn adopting_an_empty_home_is_not_an_error() {
        let (ctx, _mem, _scratch) = machine("empty");
        assert_eq!(adopt(&ctx).expect("adopting"), None);
    }
}
