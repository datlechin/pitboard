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

use super::harness::{
    NOW, POINTS, codex_machine, document, hold, in_organisation, login_of, machine, owner, recover,
    signed_in,
};
use super::*;
use crate::api::scripted::{ScriptedApi, Trouble};
use crate::fault;
use crate::service::Permit;
use crate::store::memory::Fault;
use serde_json::json;
use std::sync::Arc;

/// Kill a switch at every durable step, recover, and check. Then recover again, because a
/// recovery that only works once leaves a machine nobody can fix.
///
/// For every tool, through the same invariants: what must be true after a crash is a fact
/// about parking a login, and a tool whose park may never be a copy is exactly the one
/// where getting it wrong costs most.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_killed_at_any_step_recovers_to_something_whole() {
    type Make = fn(&str) -> super::harness::Machine;
    let machines: [(&str, Make); 2] = [("claude", machine), ("codex", codex_machine)];
    for (tool, make) in machines {
        for point in POINTS {
            let m = make(&point.replace('.', "-"));

            let settled = settle(&m.ctx, Permit::for_a_test(), None)
                .expect("nothing to recover yet")
                .0;
            let died = fault::killing(point, || switch(settled, &m.key("there")));
            assert_eq!(
                died.unwrap_err(),
                point,
                "{tool}: the switch must reach {point} on this machine, or the case proves \
                 nothing"
            );

            let at = format!("{tool}, {point}");
            recover(&m).unwrap_or_else(|e| panic!("{at}: recovery refused: {e}"));
            hold(&m, &at);

            recover(&m).unwrap_or_else(|e| panic!("{at}: the second recovery refused: {e}"));
            hold(&m, &format!("{at}, recovered twice"));
        }
    }
}

/// The same kills, with Anthropic unreachable afterwards. Recovery decides what an
/// interrupted switch did from which side the live credential's refresh token came from,
/// and both sides were fingerprinted when the record was written, so the common case is a
/// comparison and not a round trip. That is what makes a switch recoverable on a plane.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_killed_with_nobody_to_ask_is_recovered_from_the_record() {
    for point in POINTS {
        let m = machine(&format!("offline-{}", point.replace('.', "-")));

        let settled = settle(&m.ctx, Permit::for_a_test(), None)
            .expect("nothing to recover yet")
            .0;
        let died = fault::killing(point, || {
            switch(
                settled,
                &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
            )
        });
        assert_eq!(died.unwrap_err(), point);

        // Anthropic goes away. A scripted api answers Unauthorized for tokens it does not
        // know, so telling it to be unreachable for both is how it goes offline.
        let offline = ScriptedApi::new();
        let ctx = m.ctx.clone().with_scripted_api(Arc::clone(&offline));
        offline.token_trouble("access-here-refresh", Trouble::Offline);
        offline.token_trouble("access-there-refresh", Trouble::Offline);

        // Which side the live credential came from is written in the record as two
        // fingerprints, so this settles without asking anyone. Dropped at once: a settled
        // machine holds Pitboard's exclusivity lock until it is.
        let decided = settle(&ctx, Permit::for_a_test(), None).is_ok();
        assert!(
            decided,
            "{point}: recovery should read the live login's fingerprint rather than \
             needing Anthropic"
        );
        assert_eq!(
            offline.calls(),
            0,
            "{point}: and it should not have asked at all"
        );
        hold(&m, &format!("{point}, recovered with no network"));

        // Still true when Anthropic comes back, and still true run twice.
        recover(&m).unwrap_or_else(|e| panic!("{point}: recovery refused once back: {e}"));
        hold(&m, &format!("{point}, then online"));
    }
}

/// The fingerprints narrow the network dependency; they do not remove it. Claude Code
/// rotating the token inside the seconds of an interrupted switch leaves a live credential
/// matching neither side, which is exactly when there is nothing to read off and Anthropic
/// has to be asked. With nobody to ask, the only right answer is to change nothing.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_whose_token_rotated_while_it_was_interrupted_still_needs_anthropic() {
    let m = machine("rotated-offline");
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover yet")
        .0;
    let died = fault::killing("switch.park_recorded", || {
        switch(
            settled,
            &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
        )
    });
    assert_eq!(died.unwrap_err(), "switch.park_recorded");

    // Claude Code refreshes the login it still believes is signed in, so the slot now holds
    // a token neither side of the record fingerprints to.
    m.mem.live().plant(
        &m.service,
        &json!({"claudeAiOauth": {
            "refreshToken": "rotated-since",
            "accessToken": "access-here-refresh",
            "expiresAt": (NOW + 3600) * 1000,
            "refreshTokenExpiresAt": (NOW + 30 * 86_400) * 1000,
        }})
        .to_string(),
    );

    let before = m.mem.vault().services();
    let live_before = m.mem.live().peek(&m.service);
    let offline = ScriptedApi::new();
    let ctx = m.ctx.clone().with_scripted_api(Arc::clone(&offline));
    offline.token_trouble("access-here-refresh", Trouble::Offline);

    assert!(
        settle(&ctx, Permit::for_a_test(), None).is_err(),
        "with nothing to read off and nobody to ask, this must not be guessed at"
    );
    assert_eq!(m.mem.vault().services(), before, "nothing may be deleted");
    assert_eq!(m.mem.live().peek(&m.service), live_before);
    assert!(
        journal::pending(&ctx),
        "and the record is kept for a later run"
    );
}

/// One person's two organisations are two accounts with one account uuid between them. A
/// switch from one to the other, interrupted once the new login is in and then rotated
/// past both fingerprints, is settled by asking Anthropic, whose answer names the
/// organisation as well as the person.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_between_two_organisations_is_settled_by_the_organisation_anthropic_names() {
    let m = machine("two-organisations-rotated");
    let team = crate::api::Owner {
        account_uuid: "here".into(),
        email: "here@example.com".into(),
        organization_uuid: "org-team".into(),
    };
    m.api.owned_by("access-team-refresh", team.clone());
    let mut state = crate::state::load(&m.ctx).expect("state");
    let team_id = crate::state::new_id(crate::provider::ProviderId::Claude, &team);
    let service =
        crate::park::reserve(&m.ctx, Permit::for_a_test(), &team_id).expect("a free name");
    let parked = crate::park::store_at(
        &m.ctx,
        Permit::for_a_test(),
        crate::provider::ProviderId::Claude,
        &service,
        &super::harness::oauth("team-refresh", 30),
    )
    .expect("parked");
    state.upsert(in_organisation("team", "here", "org-team", Some(parked)));
    crate::state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");

    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover yet")
        .0;
    let to_team = crate::state::Key::new(crate::provider::ProviderId::Claude, "team");
    let died = fault::killing("switch.installed", || switch(settled, &to_team));
    assert_eq!(died.unwrap_err(), "switch.installed");
    m.mem.live().plant(
        &m.service,
        &json!({"claudeAiOauth": {
            "refreshToken": "rotated-since",
            "accessToken": "access-team-refresh",
            "expiresAt": (NOW + 3600) * 1000,
            "refreshTokenExpiresAt": (NOW + 30 * 86_400) * 1000,
        }})
        .to_string(),
    );

    let (settled, recovered) = settle(&m.ctx, Permit::for_a_test(), None).expect("settled");
    assert!(
        recovered.is_some_and(|r| r.finished),
        "the switch had landed"
    );
    assert_eq!(
        settled
            .state
            .active_for(crate::provider::ProviderId::Claude),
        Some("team")
    );
}

/// How a machine is left after a switch was killed, before anything reads it: what reading
/// it asks, and the service it asks.
type Leave = fn(&super::harness::Machine) -> (Context, Arc<ScriptedApi>);

/// Every way a machine is found after a switch was killed: as it was; signed out of the
/// tool; with a login the tool renewed since, which matches neither side the record kept;
/// with the copy the switch parked unreadable, alone and with a renewed login; with the
/// service out of reach, alone and with a renewed login; with the record damaged; and with
/// the record written for another slot.
const AFTERWARDS: [(&str, Leave); 9] = [
    ("as it was", |m| (m.ctx.clone(), Arc::clone(&m.api))),
    ("signed out", |m| {
        m.fault_live(Fault::Vanish);
        (m.ctx.clone(), Arc::clone(&m.api))
    }),
    ("renewed since", |m| {
        m.sign_in(&login_of(m, "here", "renewed-since"));
        (m.ctx.clone(), Arc::clone(&m.api))
    }),
    ("park unreadable", |m| {
        unreadable(m);
        (m.ctx.clone(), Arc::clone(&m.api))
    }),
    ("renewed since and park unreadable", |m| {
        m.sign_in(&login_of(m, "here", "renewed-since"));
        unreadable(m);
        (m.ctx.clone(), Arc::clone(&m.api))
    }),
    ("offline", unreachable),
    ("renewed since and offline", |m| {
        m.sign_in(&login_of(m, "here", "renewed-since"));
        unreachable(m)
    }),
    ("record damaged", |m| {
        if recorded(m, "park_service").is_some() {
            std::fs::write(
                home::dir(&m.ctx).join("journal.json"),
                "{\"started_at\": 17",
            )
            .expect("written");
        }
        (m.ctx.clone(), Arc::clone(&m.api))
    }),
    ("recorded for another slot", |m| {
        let path = home::dir(&m.ctx).join("journal.json");
        if let Ok(raw) = std::fs::read_to_string(&path) {
            let mut record: Value = serde_json::from_str(&raw).expect("a record");
            record["slot"] = json!("somewhere else");
            std::fs::write(&path, record.to_string()).expect("written");
        }
        (m.ctx.clone(), Arc::clone(&m.api))
    }),
];

/// The copy the switch parked, where it reserved one, cannot be read.
fn unreadable(m: &super::harness::Machine) {
    if let Some(park) = recorded(m, "park_service") {
        m.mem
            .vault()
            .fault(&park, Fault::Unreadable("locked".into()));
    }
}

/// The machine with its service out of reach for every login it has held.
fn unreachable(m: &super::harness::Machine) -> (Context, Arc<ScriptedApi>) {
    let offline = ScriptedApi::new();
    for refresh in ["here-refresh", "there-refresh", "renewed-since"] {
        offline.token_trouble(&format!("access-{refresh}"), Trouble::Offline);
    }
    let ctx = m.ctx.clone().with_scripted_api(Arc::clone(&offline));
    (ctx, offline)
}

/// A field of the record of the interrupted switch, where there is one.
fn recorded(m: &super::harness::Machine, field: &str) -> Option<String> {
    let raw = std::fs::read_to_string(home::dir(&m.ctx).join("journal.json")).ok()?;
    let record: Value = serde_json::from_str(&raw).ok()?;
    record[field].as_str().map(str::to_owned)
}

/// Everything on the machine: Pitboard's files, every parked login and the live one.
type Everything = (
    Vec<(String, Vec<u8>)>,
    Vec<(String, Option<String>)>,
    Option<String>,
);

fn everything(m: &super::harness::Machine) -> Everything {
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(home::dir(&m.ctx))
        .expect("a Pitboard home")
        .map(|entry| {
            let path = entry.expect("an entry").path();
            let body = std::fs::read(&path).unwrap_or_default();
            (path.display().to_string(), body)
        })
        .collect();
    files.sort();
    let vault = m.mem.vault();
    let parked = vault
        .services()
        .into_iter()
        .map(|service| {
            let held = vault.peek(&service);
            (service, held)
        })
        .collect();
    let (store, service) = m.live_store();
    (files, parked, store.peek(&service))
}

/// What a read says of an interrupted switch is what the next change does with it, worked
/// out without doing it. For every tool, every step a switch can be killed at, and every
/// way the machine can be found afterwards, a read says the switch is waiting exactly where
/// settling it is refused as `recovery_undetermined`, in that refusal's words, and changes
/// nothing on the way. A switch under way in another run leaves the same record at each
/// step as one killed there, so this is also what a read made meanwhile says.
///
/// A read that asks nobody says the same wherever the record and the logins tell it, and
/// nothing where only the service could: never something else, and never with a request.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_read_says_a_switch_is_waiting_exactly_where_settling_it_is_refused() {
    type Make = fn(&str) -> super::harness::Machine;
    let machines: [(&str, Make); 2] = [("claude", machine), ("codex", codex_machine)];
    for (tool, make) in machines {
        for point in POINTS {
            for (left, leave) in AFTERWARDS {
                let at = format!("{tool}, {point}, {left}");
                let m = make(&format!(
                    "read-{}-{}",
                    point.replace('.', "-"),
                    left.replace(' ', "-")
                ));
                let settled = settle(&m.ctx, Permit::for_a_test(), None)
                    .expect("nothing to recover yet")
                    .0;
                let died = fault::killing(point, || switch(settled, &m.key("there")));
                assert_eq!(died.unwrap_err(), point, "{at}");
                let (ctx, api) = leave(&m);
                let state = state::load(&ctx).expect("an account list");
                let before = everything(&m);

                let asked = stuck(&ctx, &state, Asking::Service).map(|e| e.to_string());
                let calls = api.calls();
                let unasked = stuck(&ctx, &state, Asking::Nobody).map(|e| e.to_string());

                assert_eq!(api.calls(), calls, "{at}: asking nobody asks nobody");
                assert_eq!(everything(&m), before, "{at}: a read changes nothing");
                let refused = match settle(&ctx, Permit::for_a_test(), None) {
                    Err(e) if e.code() == "recovery_undetermined" => Some(e.to_string()),
                    _ => None,
                };
                assert_eq!(asked, refused, "{at}");
                assert!(
                    unasked.is_none() || unasked == asked,
                    "{at}: {unasked:?} where a read that asks says {asked:?}"
                );
                if m.which == ProviderId::Codex {
                    assert_eq!(unasked, asked, "{at}: a Codex login names its account");
                }
            }
        }
    }
}

/// Enrolling by signing in writes a login into the vault before anything names it. Killed
/// in that window, the item is an orphan: never renewed, never deleted, and on macOS not
/// listable by any tool the user has.
#[test]
#[cfg_attr(
    windows,
    ignore = "W16: Pitboard writing, replacing and removing files on Windows"
)]
fn enrolling_killed_between_the_write_and_the_record_leaves_nothing_unnamed() {
    for point in ["enroll.park_stored", "enroll.park_recorded"] {
        let m = machine(&point.replace('.', "-"));
        m.api.owned_by("access-third-refresh", owner("third"));

        let settled = settle(&m.ctx, Permit::for_a_test(), None)
            .expect("nothing to recover")
            .0;
        let login = enroll::planted(
            &m.ctx,
            Permit::for_a_test(),
            ProviderId::Claude,
            document("third-refresh"),
        )
        .expect("a sign-in");
        let died = fault::killing(point, || {
            enroll(
                settled,
                &crate::state::Key::new(crate::provider::ProviderId::Claude, "third"),
                Some(login),
            )
        });
        assert_eq!(died.unwrap_err(), point);

        recover(&m).unwrap_or_else(|e| panic!("{point}: recovery refused: {e}"));
        hold(&m, point);
    }
}

/// Signing in again to the account in use writes its new login in place of the old one,
/// then records it, and parks nothing. Killed after either, the account is signed in with
/// the login that was written and every other account still has its own.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn signing_in_again_killed_at_any_step_recovers_to_something_whole() {
    type Make = fn(&str) -> super::harness::Machine;
    let machines: [(&str, Make); 2] = [("claude", machine), ("codex", codex_machine)];
    for (tool, make) in machines {
        for point in ["enroll.installed", "enroll.recorded"] {
            let m = make(&format!("again-{}", point.replace('.', "-")));
            let login = signed_in(&m, "here", "here-refresh-2");

            let settled = settle(&m.ctx, Permit::for_a_test(), None)
                .expect("nothing to recover yet")
                .0;
            let died = fault::killing(point, || enroll(settled, &m.key("here"), Some(login)));
            assert_eq!(
                died.unwrap_err(),
                point,
                "{tool}: the sign-in must reach {point} on this machine, or the case proves \
                 nothing"
            );

            let at = format!("{tool}, {point}");
            recover(&m).unwrap_or_else(|e| panic!("{at}: recovery refused: {e}"));
            hold(&m, &at);
            let live = m.live().expect("a login in use");
            assert_eq!(
                crate::provider::of(m.which).fingerprint(&live),
                store::fingerprint("here-refresh-2"),
                "{at}: the new login is the one in use"
            );

            recover(&m).unwrap_or_else(|e| panic!("{at}: the second recovery refused: {e}"));
            hold(&m, &format!("{at}, recovered twice"));
        }
    }
}

/// The one Claude Code write this lock cannot exclude. Measured in 2.1.278: a `/logout`
/// that has given up waiting deletes the credential with nothing held. If it lands just
/// after the install, the incoming login is gone, and Pitboard used to print "Switched to
/// work" and exit 0 over an account that was signed out.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_whose_login_was_removed_again_does_not_report_a_switch() {
    let m = machine("did-not-hold");
    let parked_before = m.mem.vault().services();

    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover yet")
        .0;
    m.mem.live().fault(&m.service, Fault::DeletedAfterWrite);
    let failed = switch(
        settled,
        &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
    )
    .expect_err("the login did not stay");
    assert!(
        matches!(failed, Error::SwitchDidNotHold { .. }),
        "got {failed:?}"
    );

    let state = state::load(&m.ctx).expect("state");
    // Both logins are still here: the one that was parked on the way out, and the one that
    // was being installed. Neither may be thrown away over a write that did not hold.
    assert!(
        state
            .get(&crate::state::Key::new(
                crate::provider::ProviderId::Claude,
                "here"
            ))
            .expect("account")
            .parked
            .is_some(),
        "the outgoing login was parked and stays parked"
    );
    assert!(
        state
            .get(&crate::state::Key::new(
                crate::provider::ProviderId::Claude,
                "there"
            ))
            .expect("account")
            .parked
            .is_some(),
        "the incoming login is not discarded over a switch that did not stand"
    );
    assert!(m.mem.vault().services().len() > parked_before.len());
    assert!(
        !journal::pending(&m.ctx),
        "and there is nothing half-done to finish"
    );
    hold(&m, "did not hold");
}

/// A switch that could not find out what it did keeps everything, including its record of
/// intent, so a later run with a store that answers decides. Nothing here is a crash: this
/// is the ordinary shape of a machine whose keychain is locked.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_that_cannot_read_the_store_back_keeps_every_copy_and_its_record() {
    let m = machine("unverified");
    let before = m.mem.live().peek(&m.service).expect("a live login");
    let parked_before = m.mem.vault().services();

    // The keychain locks partway through, which is what a screen lock does. The reads the
    // switch makes before it writes still answer; the write and everything after it do not.
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover yet")
        .0;
    m.mem.live().fault(&m.service, Fault::LocksOnWrite);
    let failed = switch(
        settled,
        &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
    )
    .expect_err("a keychain that locked partway");
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
#[cfg_attr(
    windows,
    ignore = "W16: Pitboard writing, replacing and removing files on Windows"
)]
fn renewing_killed_between_the_write_and_the_record_leaves_nothing_unnamed() {
    let m = machine("renew-park-stored");
    // The parked login is due: its access token has lapsed.
    let mut state = state::load(&m.ctx).expect("state");
    let park = state
        .get(&crate::state::Key::new(
            crate::provider::ProviderId::Claude,
            "there",
        ))
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
        &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
        park::describe(
            crate::provider::ProviderId::Claude,
            &park.service,
            NOW,
            &json!({
                "refreshToken": "there-refresh",
                "expiresAt": (NOW - 60) * 1000,
                "refreshTokenExpiresAt": (NOW + 30 * 86_400) * 1000
            }),
        ),
    );
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    m.api.renews(
        "there-refresh",
        crate::api::Renewed {
            access_token: "access-fresh".into(),
            refresh_token: Some("fresh".into()),
            expires_in: 3600,
            refresh_token_expires_in: Some(30 * 86_400),
            scopes: None,
            at: None,
        },
    );

    let died = fault::killing("renew.park_stored", || {
        renew::renew_parked(&m.ctx, Permit::for_a_test())
    });
    assert_eq!(died.unwrap_err(), "renew.park_stored");

    recover(&m).expect("recovery");
    hold(&m, "renew.park_stored");

    // Which chain survived is the whole point. Anthropic spent `there-refresh` when it
    // answered; the copy written just before the kill is the only one that still works.
    let kept = state::load(&m.ctx)
        .expect("state")
        .get(&crate::state::Key::new(
            crate::provider::ProviderId::Claude,
            "there",
        ))
        .and_then(|a| a.parked.clone())
        .expect("`there` still holds a login");
    assert_eq!(
        kept.refresh_fingerprint,
        crate::provider::claude::document::fingerprint_of(&json!({"refreshToken": "fresh"})),
        "the fresh login is kept and the spent one dropped, not the other way round"
    );
    assert_eq!(m.mem.vault().services(), vec![kept.service]);
}

/// A renewal whose answer is written and whose record cannot be saved keeps what it wrote.
/// The service has already spent the chain the record still names, so deleting the fresh
/// copy, which this once did, left the account nothing that works. The next change gives
/// it back.
#[test]
#[cfg_attr(
    windows,
    ignore = "W16: Pitboard writing, replacing and removing files on Windows"
)]
fn a_renewal_that_cannot_record_its_answer_keeps_it_for_the_next_run() {
    use crate::host::fs::testing;
    let m = machine("renew-save-fails");
    let key = crate::state::Key::new(crate::provider::ProviderId::Claude, "there");
    let mut state = state::load(&m.ctx).expect("state");
    let held = state.get(&key).unwrap().parked.clone().unwrap();
    let lapsed = json!({
        "accessToken": "access-there-refresh",
        "refreshToken": "there-refresh",
        "expiresAt": (NOW - 60) * 1000,
        "refreshTokenExpiresAt": (NOW + 30 * 86_400) * 1000
    });
    m.mem.vault().plant(&held.service, &lapsed.to_string());
    state.park(
        &key,
        park::describe(
            crate::provider::ProviderId::Claude,
            &held.service,
            NOW,
            &lapsed,
        ),
    );
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    m.api.renews(
        "there-refresh",
        crate::api::Renewed {
            access_token: "access-fresh".into(),
            refresh_token: Some("fresh".into()),
            expires_in: 3600,
            refresh_token_expires_in: Some(30 * 86_400),
            scopes: None,
            at: None,
        },
    );

    // Pitboard's home goes read-only once the fresh login is in the vault, so the record
    // of it cannot be written.
    let home = crate::home::dir(&m.ctx);
    let locked = home.clone();
    let outcomes = fault::meanwhile(
        "renew.park_stored",
        move || {
            testing::deny_changes(&locked).expect("read-only");
        },
        || renew::renew_parked(&m.ctx, Permit::for_a_test()),
    );
    testing::allow_changes(&home).expect("changeable again");
    assert!(
        outcomes
            .iter()
            .any(|(_, r)| matches!(r, renew::Renewal::Failed(_))),
        "the save failed, and says so"
    );

    recover(&m).expect("the next change");
    let kept = state::load(&m.ctx)
        .expect("state")
        .get(&key)
        .and_then(|a| a.parked.clone())
        .expect("`there` still holds a login");
    assert_eq!(
        kept.refresh_fingerprint,
        crate::provider::claude::document::fingerprint_of(&json!({"refreshToken": "fresh"})),
    );
    hold(&m, "after a renewal that could not be recorded");
}

/// Forgetting deletes the account before deleting its park. Killed between the two, the
/// park is listed for deletion and a later run finishes it.
#[test]
#[cfg_attr(
    windows,
    ignore = "W16: Pitboard writing, replacing and removing files on Windows"
)]
fn forgetting_killed_after_the_record_still_deletes_the_park() {
    let m = machine("forget-recorded");
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;

    let died = fault::killing("forget.recorded", || {
        forget::forget(
            settled,
            &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
        )
    });
    assert_eq!(died.unwrap_err(), "forget.recorded");

    recover(&m).expect("recovery");
    hold(&m, "forget.recorded");
    assert!(
        m.mem.vault().services().is_empty(),
        "a forgotten account's login is deleted, not left in the vault"
    );
}
