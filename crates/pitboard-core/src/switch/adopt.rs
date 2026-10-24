//! Taking over a Pitboard directory another machine wrote. It keeps what is about an account,
//! not a machine: the label, the email, the account and organisation uuid and the numbers. It
//! drops each login parked in a store and asks Anthropic nothing, since two machines taking
//! turns with one refresh token make Claude Code drop the login on both, and whether an
//! exchange revokes the other machine's copy is not measured (docs/facts/not-measured.md).

use super::{exclusive, purge};
use crate::context::Context;
use crate::error::Result;
use crate::permit::Permit;
use crate::state::{Aside, ParkKind, SetAside};
use crate::{audit, state, store};

/// What taking over found.
#[derive(Debug, PartialEq, Eq)]
pub struct Adopted {
    /// The accounts kept, in the order they were enrolled, named the way they are typed:
    /// bare for Claude Code, `codex/work` for another tool.
    pub accounts: Vec<String>,
    /// Accounts that arrived holding a parked login, now dropped, named the same way.
    pub logins_dropped: Vec<String>,
    // Parked on another Mac, where Claude's key differs: set aside, never deleted.
    pub set_aside: Vec<String>,
}

/// Take over this Pitboard directory. `None` when it was already this machine's, which is
/// the ordinary case and not an error: running it when there is nothing to do says so.
pub(crate) fn adopt(ctx: &Context, permit: Permit) -> Result<Option<Adopted>> {
    let _exclusive = exclusive(ctx, permit)?;
    let (mut state, here) = state::load_any_machine(ctx)?;
    if here {
        return Ok(None);
    }

    let accounts: Vec<String> = state.accounts.iter().map(|a| a.key().typed()).collect();
    let mut logins_dropped = Vec::new();
    let mut parks = Vec::new();
    let mut elsewhere = Vec::new();
    let machine = ctx.this_machine();
    for account in &state.accounts {
        let Some(park) = &account.parked else {
            continue;
        };
        match park.kind {
            ParkKind::Vault => {
                logins_dropped.push(account.key().typed());
                parks.push(park.service.clone());
            }
            // One parked here stays its account's.
            ParkKind::Folder => {
                let here = store::folder::manifest(ctx, &park.service)
                    .ok()
                    .flatten()
                    .is_some_and(|manifest| machine != "unknown" && manifest.machine == machine);
                if !here {
                    elsewhere.push((account.key(), park.clone()));
                }
            }
        }
    }
    let set_aside = elsewhere.iter().map(|(key, _)| key.typed()).collect();
    for (key, park) in elsewhere {
        state.set_aside(&key, park, SetAside::AnotherMachine, ctx.now());
    }
    // A move or a sign-in that waited was about the other computer's Claude: every folder it
    // names that no account holds is kept aside.
    for (code, moving) in std::mem::take(&mut state.moving) {
        let Some(tool) = crate::provider::ProviderId::parse(&code) else {
            continue;
        };
        for name in std::iter::once(&moving.park).chain(moving.asides.keys()) {
            let held = state
                .accounts
                .iter()
                .any(|a| a.parked.as_ref().is_some_and(|park| park.service == *name));
            if held || !store::folder::at(ctx, name).is_dir() {
                continue;
            }
            state.keep_aside(Aside {
                name: name.clone(),
                tool,
                at: ctx.now(),
                why: SetAside::AnotherMachine,
                account: (*name == moving.park).then(|| moving.id.clone()),
                login: None,
            });
        }
    }
    state.awaiting.clear();
    // Listed rather than deleted outright, so a delete that fails is retried. On a machine
    // that did not receive the keychain there is nothing there to delete, and deleting what
    // is not there succeeds. Released rather than discarded, because one `repair` gave back
    // may be another Pitboard's, and that keychain may have come across too.
    for service in &parks {
        state.release(service);
    }
    // Whose login each tool had stored, and which logins sign-ins replaced there, were facts
    // about the other computer's stores.
    for account in &mut state.accounts {
        account.replaced_at = None;
    }
    state.forget_in_use();
    state.machine = state::machine_id();
    state::save(ctx, permit, &state)?;
    purge(ctx, permit, &mut state);

    audit::record(ctx, permit, "adopt", "", "ok");
    Ok(Some(Adopted {
        accounts,
        logins_dropped,
        set_aside,
    }))
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::host::memory::MemoryHost;
    use crate::in_use::InUse;
    use crate::provider::ProviderId;
    use crate::state::{Account, State};
    use crate::time::{Clock, FixedClock};
    use serde_json::json;
    use std::sync::Arc;

    const NOW: i64 = 1_760_000_000;

    fn machine() -> (Context, Arc<MemoryHost>, tempfile::TempDir) {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let mem = MemoryHost::new();
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root)
            .with_memory_stores(Arc::clone(&mem))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        crate::home::ensure(&ctx, Permit::for_a_test()).expect("a home");
        (ctx, mem, scratch)
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
            replaced_at: None,
            plan: None,
            label: "work".into(),
            id: "acc".into(),
            account_uuid: "acc".into(),
            email: "me@example.com".into(),
            detail: state::Detail::Claude {
                organization_uuid: "org".into(),
                oauth_account: json!({"accountUuid": "acc"}),
            },
            parked: Some(crate::park::describe(
                crate::provider::document(ProviderId::Claude),
                service,
                NOW,
                &oauth(),
            )),
        });
        let found = InUse::of(&state.accounts[0], "r", NOW);
        state.identified(ProviderId::Claude, found, NOW);
        let raw = serde_json::to_string(&state).expect("serialisable");
        std::fs::write(crate::home::dir(ctx).join("state.json"), raw).expect("written");
        service.to_string()
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn adopting_sets_aside_the_folder_parks_of_another_mac_and_deletes_none() {
        let (ctx, mem, _scratch) = machine();
        from_elsewhere(&ctx, &mem);
        let path = crate::home::dir(&ctx).join("state.json");
        let mut state: State =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let manifest = |machine: &str| {
            json!({
                "v": 1, "tool": "desktop", "account": null, "named": null, "login": null,
                "parked_at": NOW, "machine": machine, "items": [], "keys": {}
            })
        };
        let mut desktop = |label: &str, machine: &str| {
            let service = format!("pitboard-park-desktop_{label}-1");
            let folder = store::folder::at(&ctx, &service);
            std::fs::create_dir_all(folder.join("items")).unwrap();
            std::fs::write(folder.join("items/Cookies"), b"a jar").unwrap();
            std::fs::write(folder.join("manifest.json"), manifest(machine).to_string()).unwrap();
            state.accounts.push(Account {
                last_used_at: None,
                replaced_at: None,
                plan: None,
                label: label.into(),
                id: format!("desktop_{label}"),
                account_uuid: label.into(),
                email: String::new(),
                detail: state::Detail::Desktop {},
                parked: Some(state::Park {
                    service: service.clone(),
                    kind: ParkKind::Folder,
                    parked_at: NOW,
                    login: format!("login-{label}"),
                    access_expires_at: None,
                    refresh_expires_at: None,
                    session_expires_at: None,
                }),
            });
            folder
        };
        let elsewhere = desktop("work", "a hash from another computer");
        let here = desktop("home", &ctx.this_machine());
        std::fs::write(&path, serde_json::to_string(&state).unwrap()).unwrap();

        let adopted = adopt(&ctx, Permit::for_a_test())
            .expect("adopting")
            .expect("there was work to do");
        assert_eq!(adopted.set_aside, ["desktop/work"]);
        assert_eq!(adopted.logins_dropped, ["work"], "the vault's, as before");

        let state = state::load(&ctx).expect("now it is this machine's");
        let key = |label: &str| state::Key::new(ProviderId::Desktop, label);
        assert!(state.get(&key("work")).unwrap().parked.is_none());
        assert!(
            state.get(&key("home")).unwrap().parked.is_some(),
            "parked here: kept"
        );
        assert_eq!(state.aside.len(), 1);
        assert_eq!(state.aside[0].why, SetAside::AnotherMachine);
        assert_eq!(state.aside[0].account.as_deref(), Some("desktop_work"));
        assert!(state.discarded.iter().all(|d| d.kind == ParkKind::Vault));
        assert!(elsewhere.join("items/Cookies").exists(), "deleted none");
        assert!(here.join("items/Cookies").exists());
        assert!(
            mem.trail()
                .iter()
                .all(|touch| !touch.service.starts_with("pitboard-park-desktop_")),
            "{:?}",
            mem.trail()
        );
    }

    // A move or a sign-in waited for on the other Mac: each folder it names is kept aside, once.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn adopting_keeps_aside_every_folder_a_move_from_another_mac_names() {
        let folder = |ctx: &Context, name: &str| {
            let at = store::folder::at(ctx, name);
            std::fs::create_dir_all(at.join("items")).unwrap();
            std::fs::write(at.join("items/Cookies"), b"a jar").unwrap();
            let manifest = json!({
                "v": 1, "tool": "desktop", "account": null, "named": null, "login": null,
                "parked_at": NOW, "machine": "a hash from another computer", "items": [],
                "keys": {}
            });
            std::fs::write(at.join("manifest.json"), manifest.to_string()).unwrap();
            at
        };
        let work = |parked: Option<state::Park>| Account {
            last_used_at: None,
            replaced_at: None,
            plan: None,
            label: "work".into(),
            id: "desktop_work".into(),
            account_uuid: "work".into(),
            email: String::new(),
            detail: state::Detail::Desktop {},
            parked,
        };
        let moving = |half: state::Half, park: &str, asides: &[(&str, SetAside)]| state::Moving {
            half,
            started_at: NOW,
            id: "desktop_work".into(),
            label: "work".into(),
            park: park.into(),
            login: "login-work".into(),
            items: vec!["Cookies".into()],
            keys: std::collections::BTreeMap::new(),
            asides: asides
                .iter()
                .map(|(n, why)| ((*n).to_string(), *why))
                .collect(),
            slot: String::new(),
            awaited: None,
        };
        let adopted = |ctx: &Context, state: &State| {
            let path = crate::home::dir(ctx).join("state.json");
            std::fs::write(&path, serde_json::to_string(state).unwrap()).unwrap();
            adopt(ctx, Permit::for_a_test()).expect("adopting");
            state::load(ctx).expect("now it is this machine's")
        };

        // A leave: its park and the aside it made, never one it only named.
        let (ctx, mem, _scratch) = machine();
        from_elsewhere(&ctx, &mem);
        let path = crate::home::dir(&ctx).join("state.json");
        let mut state: State =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        state.accounts.push(work(None));
        let (park, made) = ("pitboard-park-desktop_work-2", "pitboard-aside-9");
        let (park_at, made_at) = (folder(&ctx, park), folder(&ctx, made));
        state.moving.insert(
            "desktop".into(),
            moving(
                state::Half::Leave,
                park,
                &[
                    (made, SetAside::GivenUp),
                    ("pitboard-aside-10", SetAside::InTheWay),
                ],
            ),
        );
        state.awaiting.insert(
            "desktop".into(),
            state::Awaited {
                since: NOW,
                label: "travel".into(),
                account: None,
                put_back: "desktop_work".into(),
            },
        );
        let state = adopted(&ctx, &state);
        assert!(state.moving.is_empty() && state.awaiting.is_empty());
        let kept: Vec<(&str, SetAside, Option<&str>)> = state
            .aside
            .iter()
            .map(|aside| (aside.name.as_str(), aside.why, aside.account.as_deref()))
            .collect();
        assert_eq!(
            kept,
            [
                (park, SetAside::AnotherMachine, Some("desktop_work")),
                (made, SetAside::AnotherMachine, None),
            ]
        );
        assert!(park_at.join("items/Cookies").exists() && made_at.join("items/Cookies").exists());

        // An arrive of a park the account holds from the other Mac: set aside once.
        let (ctx, mem, _scratch) = machine();
        from_elsewhere(&ctx, &mem);
        let mut state: State = serde_json::from_str(
            &std::fs::read_to_string(crate::home::dir(&ctx).join("state.json")).unwrap(),
        )
        .unwrap();
        let held = "pitboard-park-desktop_work-1";
        folder(&ctx, held);
        state.accounts.push(work(Some(state::Park {
            service: held.into(),
            kind: ParkKind::Folder,
            parked_at: NOW,
            login: "login-work".into(),
            access_expires_at: None,
            refresh_expires_at: None,
            session_expires_at: None,
        })));
        state
            .moving
            .insert("desktop".into(), moving(state::Half::Arrive, held, &[]));
        let state = adopted(&ctx, &state);
        let named: Vec<&str> = state
            .aside
            .iter()
            .map(|aside| aside.name.as_str())
            .collect();
        assert_eq!(named, [held]);
        assert_eq!(state.aside[0].account.as_deref(), Some("desktop_work"));
        assert!(state.moving.is_empty());
    }

    #[test]
    fn a_home_from_another_machine_keeps_its_accounts_and_loses_its_logins() {
        let (ctx, mem, _scratch) = machine();
        let service = from_elsewhere(&ctx, &mem);

        assert!(
            matches!(
                state::load(&ctx),
                Err(crate::error::Error::StateWrongMachine { .. })
            ),
            "every other command still refuses it"
        );

        let adopted = adopt(&ctx, Permit::for_a_test())
            .expect("adopting")
            .expect("there was work to do");
        assert_eq!(adopted.accounts, vec!["work".to_string()]);
        assert_eq!(adopted.logins_dropped, vec!["work".to_string()]);

        let state = state::load(&ctx).expect("now it is this machine's");
        let account = state
            .get(&state::Key::new(ProviderId::Claude, "work"))
            .expect("the account is kept");
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
            state.in_use.is_empty(),
            "whose login was stored was true elsewhere"
        );
    }

    /// A sign-in outside Pitboard replaced a login in the other computer's store. Here no
    /// account has a login yet, and each has the same way back, so none is said to have had
    /// its login replaced.
    #[test]
    fn a_login_replaced_on_another_machine_is_not_said_here() {
        let (ctx, mem, _scratch) = machine();
        from_elsewhere(&ctx, &mem);
        let path = crate::home::dir(&ctx).join("state.json");
        let mut written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("written")).expect("JSON");
        let mut replaced = written["accounts"][0].clone();
        replaced["label"] = json!("personal");
        replaced["id"] = json!("acc-personal");
        replaced["parked"] = serde_json::Value::Null;
        replaced["replaced_at"] = json!(NOW - 60);
        written["accounts"]
            .as_array_mut()
            .expect("accounts")
            .push(replaced);
        std::fs::write(&path, written.to_string()).expect("written");

        adopt(&ctx, Permit::for_a_test())
            .expect("adopting")
            .expect("there was work to do");

        let state = state::load(&ctx).expect("now it is this machine's");
        assert!(
            state.accounts.iter().all(|a| a.replaced_at.is_none()),
            "{:?}",
            state.accounts
        );
    }

    #[test]
    fn adopting_a_home_that_is_already_this_machines_does_nothing() {
        let (ctx, _mem, _scratch) = machine();
        let mut state = State::default();
        state.accounts.push(Account {
            last_used_at: None,
            replaced_at: None,
            plan: None,
            label: "work".into(),
            id: "acc".into(),
            account_uuid: "acc".into(),
            email: "me@example.com".into(),
            detail: state::Detail::Claude {
                organization_uuid: "org".into(),
                oauth_account: json!({}),
            },
            parked: None,
        });
        state::save(&ctx, Permit::for_a_test(), &state).expect("saved");

        assert_eq!(adopt(&ctx, Permit::for_a_test()).expect("adopting"), None);
        assert_eq!(
            state::load(&ctx).expect("still readable").accounts.len(),
            1,
            "and it changed nothing"
        );
    }

    #[test]
    fn adopting_an_empty_home_is_not_an_error() {
        let (ctx, _mem, _scratch) = machine();
        assert_eq!(adopt(&ctx, Permit::for_a_test()).expect("adopting"), None);
    }
}
