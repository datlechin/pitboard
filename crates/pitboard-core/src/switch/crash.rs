//! The crash matrix: every durable step of every change, killed, recovered, and checked.
//!
//! What this project promises is that an interrupted change leaves a machine a later run
//! can make sense of, and nobody loses a login. Until now that was tested by planting a
//! journal file and a vault item describing a crash that never happened, which tests
//! `reconcile` and not the sequence that produced what `reconcile` is handed. The two
//! orphan windows this roadmap found, in enrolling and in renewing, were exactly that
//! shape and survived every one of those tests.
//!
//! So each case here kills a real change at a real point with [`crate::fault`], runs
//! recovery, and asserts invariants rather than a particular outcome. There is more than
//! one right answer to being killed halfway; there is only one set of things that must be
//! true afterwards.
//!
//! Every case then runs recovery a second time, because recovery that is not idempotent is
//! a machine that cannot be fixed by running the command again, which is the only
//! instruction a person is ever given.

use super::*;
use crate::api::scripted::ScriptedApi;
use crate::api::{Api, Owner};
use crate::fault;
use crate::state::Account;
use crate::store::memory::MemoryPlatform;
use crate::time::{Clock, FixedClock};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

const NOW: i64 = 1_760_000_000;

/// Every place a change can be killed, named the way the code names it.
const POINTS: [&str; 6] = [
    "switch.journal_written",
    "switch.park_stored",
    "switch.park_recorded",
    "switch.installed",
    "switch.recorded",
    "switch.config_updated",
];

struct Machine {
    ctx: Context,
    mem: Arc<MemoryPlatform>,
    api: Arc<ScriptedApi>,
    root: PathBuf,
    service: String,
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn oauth(refresh: &str, expires_in_days: i64) -> Value {
    json!({
        "accessToken": format!("access-{refresh}"),
        "refreshToken": refresh,
        "expiresAt": (NOW + 3600) * 1000,
        "refreshTokenExpiresAt": (NOW + expires_in_days * 86_400) * 1000,
        "scopes": ["user:profile", "user:inference"],
    })
}

fn document(refresh: &str) -> Value {
    json!({
        "claudeAiOauth": oauth(refresh, 30),
        "organizationUuid": "org-of-the-outgoing-account",
        "mcpOAuth": {"some-server": {"token": "unrelated"}},
    })
}

fn owner(uuid: &str) -> Owner {
    Owner {
        account_uuid: uuid.into(),
        email: format!("{uuid}@example.com"),
        organization_uuid: format!("org-{uuid}"),
    }
}

/// Two accounts: `here` is signed in, `there` is parked and ready. The shape every switch
/// starts from.
fn machine(name: &str) -> Machine {
    let root = std::env::temp_dir().join(format!(
        "pitboard-crash-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a scratch home");

    let mem = MemoryPlatform::new();
    let api = ScriptedApi::new();
    let ctx = Context::new(root.clone())
        .with_pitboard_home(root.join(".pitboard"))
        .with_memory_stores(Arc::clone(&mem))
        .with_scripted_api(Arc::clone(&api))
        .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);

    // Claude Code's own files: the live credential, and the config a switch rewrites.
    let service = claude::live_service(&ctx);
    mem.live()
        .plant(&service, &document("here-refresh").to_string());
    std::fs::write(
        root.join(".claude.json"),
        json!({
            "oauthAccount": {
                "accountUuid": "here",
                "emailAddress": "here@example.com",
                "organizationUuid": "org-here",
            },
            "cachedArtifactRoster": {"org": "org-here"},
            "numStartups": 7,
        })
        .to_string(),
    )
    .expect("a config file");

    api.owned_by("access-here-refresh", owner("here"));
    api.owned_by("access-there-refresh", owner("there"));

    // `there` holds a parked login, written the way a switch would have written it.
    std::fs::create_dir_all(root.join(".pitboard")).expect("a pitboard home");
    let parked_service = park::reserve(&ctx, "there").expect("a free name");
    let parked =
        park::store_at(&ctx, &parked_service, &oauth("there-refresh", 30)).expect("parked");

    let mut state = State::default();
    state.accounts.push(account("here", "here", None));
    state.accounts.push(account("there", "there", Some(parked)));
    state.active = Some("here".into());
    state::save(&ctx, &state).expect("saved");

    Machine {
        ctx,
        mem,
        api,
        root,
        service,
    }
}

fn account(label: &str, uuid: &str, parked: Option<Park>) -> Account {
    Account {
        label: label.into(),
        account_uuid: uuid.into(),
        email: format!("{uuid}@example.com"),
        organization_uuid: format!("org-{uuid}"),
        oauth_account: json!({
            "accountUuid": uuid,
            "emailAddress": format!("{uuid}@example.com"),
            "organizationUuid": format!("org-{uuid}"),
        }),
        parked,
    }
}

/// Everything that must be true after a killed change has been recovered, whatever the
/// change was and wherever it died.
fn hold(m: &Machine, after: &str) {
    let state = state::load(&m.ctx)
        .unwrap_or_else(|e| panic!("{after}: the state file must still parse, got {e}"));

    // Nothing in the vault that the state does not name. An item nobody names holds a live
    // refresh token that no command will ever renew or delete, and on macOS nothing can
    // even list it.
    let named: HashSet<&str> = state
        .accounts
        .iter()
        .filter_map(|a| a.parked.as_ref())
        .map(|p| p.service.as_str())
        .chain(state.discarded.iter().map(String::as_str))
        .collect();
    for service in m.mem.vault().services() {
        assert!(
            named.contains(service.as_str()),
            "{after}: {service} holds a login nothing on this machine names"
        );
    }

    // One refresh token, one place. A token in two places is a token one holder will rotate
    // past, which ends the login for the other.
    let mut seen: HashSet<String> = HashSet::new();
    let mut fingerprints = Vec::new();
    for service in m.mem.vault().services() {
        let raw = m.mem.vault().peek(&service).expect("just listed");
        let value: Value = serde_json::from_str(&raw).expect("a park is JSON");
        fingerprints.push((service, park::fingerprint_of(&value)));
    }
    if let Some(raw) = m.mem.live().peek(&m.service) {
        let value: Value = serde_json::from_str(&raw).expect("the live credential is JSON");
        fingerprints.push((
            "the live slot".into(),
            park::fingerprint_of(&value["claudeAiOauth"]),
        ));
    }
    for (place, fingerprint) in fingerprints {
        assert!(
            seen.insert(fingerprint.clone()),
            "{after}: the login in {place} is also somewhere else"
        );
    }

    // Every account can still be got back to. Signed in, or holding a login that can be
    // restored, or holding nothing and saying so, but never holding one that has expired.
    let live_uuid = m
        .mem
        .live()
        .peek(&m.service)
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|doc| {
            doc["claudeAiOauth"]["accessToken"]
                .as_str()
                .map(str::to_owned)
        })
        .and_then(|token| m.api.owner(&m.ctx, &token).ok())
        .map(|o| o.account_uuid);
    for a in &state.accounts {
        if Some(&a.account_uuid) == live_uuid.as_ref() {
            continue;
        }
        if let Some(park) = &a.parked {
            assert!(
                park.restorable_at(NOW),
                "{after}: {} holds a login that can no longer be restored",
                a.label
            );
            assert!(
                m.mem.vault().peek(&park.service).is_some(),
                "{after}: {} names a park that is not in the vault",
                a.label
            );
        }
    }
}

/// Recovery, run the way the next command runs it.
fn recover(m: &Machine) -> Result<()> {
    settle(&m.ctx).map(|_| ())
}

/// Kill a switch at every durable step, recover, and check. Then recover again, because a
/// recovery that only works once leaves a machine nobody can fix.
#[test]
fn a_switch_killed_at_any_step_recovers_to_something_whole() {
    for point in POINTS {
        let m = machine(&point.replace('.', "-"));

        let settled = settle(&m.ctx).expect("nothing to recover yet").0;
        let died = fault::killing(point, || switch(settled, "there"));
        assert_eq!(
            died.unwrap_err(),
            point,
            "the switch must reach {point} on this machine, or the case proves nothing"
        );

        recover(&m).unwrap_or_else(|e| panic!("{point}: recovery refused: {e}"));
        hold(&m, point);

        recover(&m).unwrap_or_else(|e| panic!("{point}: the second recovery refused: {e}"));
        hold(&m, &format!("{point}, recovered twice"));
    }
}

/// The same kills, with Anthropic unreachable afterwards. Recovery decides what an
/// interrupted switch did by asking who owns the live login, so with nobody to ask it must
/// change nothing and keep the record for later, rather than guess.
#[test]
fn a_switch_killed_with_nobody_to_ask_changes_nothing_and_keeps_the_record() {
    for point in POINTS {
        let m = machine(&format!("offline-{}", point.replace('.', "-")));

        let settled = settle(&m.ctx).expect("nothing to recover yet").0;
        let died = fault::killing(point, || switch(settled, "there"));
        assert_eq!(died.unwrap_err(), point);

        let before = m.mem.vault().services();
        let live_before = m.mem.live().peek(&m.service);

        // Anthropic goes away. A scripted api answers Unauthorized for tokens it does not
        // know, so forgetting the tokens is how it goes offline for this run.
        let offline = ScriptedApi::new();
        let ctx = m.ctx.clone().with_scripted_api(Arc::clone(&offline));
        offline.token_trouble(
            "access-here-refresh",
            crate::api::scripted::Trouble::Offline,
        );
        offline.token_trouble(
            "access-there-refresh",
            crate::api::scripted::Trouble::Offline,
        );

        let settled = settle(&ctx);
        if journal::pending(&ctx) {
            assert!(
                settled.is_err(),
                "{point}: with nobody to ask, an interrupted switch must not be guessed at"
            );
        }
        assert_eq!(
            m.mem.vault().services(),
            before,
            "{point}: nothing may be deleted while it cannot be told what happened"
        );
        assert_eq!(
            m.mem.live().peek(&m.service),
            live_before,
            "{point}: the live login may not be moved either"
        );

        // And once Anthropic answers again, the same machine recovers.
        recover(&m).unwrap_or_else(|e| panic!("{point}: recovery refused once back: {e}"));
        hold(&m, &format!("{point}, after being offline"));
    }
}

/// Enrolling by signing in writes a login into the vault before anything names it. Killed
/// in that window, the item is an orphan: never renewed, never deleted, and on macOS not
/// listable by any tool the user has.
#[test]
fn enrolling_killed_between_the_write_and_the_record_leaves_nothing_unnamed() {
    for point in ["enroll.park_stored", "enroll.park_recorded"] {
        let m = machine(&point.replace('.', "-"));
        m.api.owned_by("access-third-refresh", owner("third"));

        let settled = settle(&m.ctx).expect("nothing to recover").0;
        let login = enroll::planted(&m.ctx, document("third-refresh")).expect("a sign-in");
        let died = fault::killing(point, || enroll(settled, "third", Some(login)));
        assert_eq!(died.unwrap_err(), point);

        recover(&m).unwrap_or_else(|e| panic!("{point}: recovery refused: {e}"));
        hold(&m, point);
    }
}

/// A switch that could not find out what it did keeps everything, including its record of
/// intent, so a later run with a store that answers decides. Nothing here is a crash: this
/// is the ordinary shape of a machine whose keychain is locked.
#[test]
fn a_switch_that_cannot_read_the_store_back_keeps_every_copy_and_its_record() {
    let m = machine("unverified");
    let before = m.mem.live().peek(&m.service).expect("a live login");
    let parked_before = m.mem.vault().services();

    // The keychain locks partway through, which is what a screen lock does. The reads the
    // switch makes before it writes still answer; the write and everything after it do not.
    let settled = settle(&m.ctx).expect("nothing to recover yet").0;
    m.mem
        .live()
        .fault(&m.service, crate::store::memory::Fault::LocksOnWrite);
    let failed = switch(settled, "there").expect_err("a keychain that locked partway");
    assert!(
        matches!(failed, Error::SwitchUnverified { .. }),
        "got {failed:?}"
    );

    assert!(
        journal::pending(&m.ctx),
        "the record of intent stays, because nobody can say what happened"
    );
    assert_eq!(
        m.mem.live().peek(&m.service).as_deref(),
        Some(before.as_str()),
        "and nothing was written"
    );
    let state = state::load(&m.ctx).expect("state");
    assert!(
        state.discarded.is_empty(),
        "nothing may be listed for deletion on a guess"
    );
    assert!(
        m.mem.vault().services().len() >= parked_before.len(),
        "and no parked login was thrown away"
    );

    // Once the store answers again, the same machine recovers and holds together.
    m.mem.live().heal_all();
    recover(&m).expect("recovery once the keychain is unlocked");
    hold(&m, "unverified, then unlocked");
}

/// The other window the roadmap named. A renewal reserves a name, writes the fresh login
/// into it, and records it; killed between the write and the record, the copy is an orphan,
/// and the renewal runs inside every plain `pitboard`.
#[test]
fn renewing_killed_between_the_write_and_the_record_leaves_nothing_unnamed() {
    let m = machine("renew-park-stored");
    // The parked login is due: its access token has lapsed.
    let mut state = state::load(&m.ctx).expect("state");
    let park = state
        .get("there")
        .expect("account")
        .parked
        .clone()
        .expect("parked");
    m.mem.vault().plant(
        &park.service,
        &json!({
            "accessToken": "access-there-refresh",
            "refreshToken": "there-refresh",
            "expiresAt": (NOW - 60) * 1000,
            "refreshTokenExpiresAt": (NOW + 30 * 86_400) * 1000
        })
        .to_string(),
    );
    state.park(
        "there",
        park::describe(
            &park.service,
            NOW,
            &json!({
                "refreshToken": "there-refresh",
                "expiresAt": (NOW - 60) * 1000,
                "refreshTokenExpiresAt": (NOW + 30 * 86_400) * 1000
            }),
        ),
    );
    state::save(&m.ctx, &state).expect("saved");
    m.api.renews(
        "there-refresh",
        crate::api::Renewed {
            access_token: "access-fresh".into(),
            refresh_token: Some("fresh".into()),
            expires_in: 3600,
            refresh_token_expires_in: Some(30 * 86_400),
            scopes: None,
        },
    );

    let died = fault::killing("renew.park_stored", || renew::renew_parked(&m.ctx));
    assert_eq!(died.unwrap_err(), "renew.park_stored");

    recover(&m).expect("recovery");
    hold(&m, "renew.park_stored");
}

/// Forgetting deletes the account before deleting its park. Killed between the two, the
/// park is listed for deletion and a later run finishes it.
#[test]
fn forgetting_killed_after_the_record_still_deletes_the_park() {
    let m = machine("forget-recorded");
    let settled = settle(&m.ctx).expect("nothing to recover").0;

    let died = fault::killing("forget.recorded", || forget::forget(settled, "there"));
    assert_eq!(died.unwrap_err(), "forget.recorded");

    recover(&m).expect("recovery");
    hold(&m, "forget.recorded");
    assert!(
        m.mem.vault().services().is_empty(),
        "a forgotten account's login is deleted, not left in the vault"
    );
}
