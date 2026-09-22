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

use super::harness::{NOW, POINTS, document, hold, machine, owner, recover};
use super::*;
use crate::api::scripted::{ScriptedApi, Trouble};
use crate::fault;
use crate::store::memory::Fault;
use serde_json::json;
use std::sync::Arc;

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
/// interrupted switch did from which side the live credential's refresh token came from,
/// and both sides were fingerprinted when the record was written, so the common case is a
/// comparison and not a round trip. That is what makes a switch recoverable on a plane.
#[test]
fn a_switch_killed_with_nobody_to_ask_is_recovered_from_the_record() {
    for point in POINTS {
        let m = machine(&format!("offline-{}", point.replace('.', "-")));

        let settled = settle(&m.ctx).expect("nothing to recover yet").0;
        let died = fault::killing(point, || switch(settled, "there"));
        assert_eq!(died.unwrap_err(), point);

        // Anthropic goes away. A scripted api answers Unauthorized for tokens it does not
        // know, so telling it to be unreachable for both is how it goes offline.
        let offline = ScriptedApi::new();
        let ctx = m.ctx.clone().with_scripted_api(Arc::clone(&offline));
        offline.token_trouble("access-here-refresh", Trouble::Offline);
        offline.token_trouble("access-there-refresh", Trouble::Offline);

        // Which side the live credential came from is written in the record as two
        // fingerprints, so this settles without asking anyone. Dropped at once: a settled
        // machine holds pitboard's exclusivity lock until it is.
        let decided = settle(&ctx).is_ok();
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
fn a_switch_whose_token_rotated_while_it_was_interrupted_still_needs_anthropic() {
    let m = machine("rotated-offline");
    let settled = settle(&m.ctx).expect("nothing to recover yet").0;
    let died = fault::killing("switch.park_recorded", || switch(settled, "there"));
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
        settle(&ctx).is_err(),
        "with nothing to read off and nobody to ask, this must not be guessed at"
    );
    assert_eq!(m.mem.vault().services(), before, "nothing may be deleted");
    assert_eq!(m.mem.live().peek(&m.service), live_before);
    assert!(
        journal::pending(&ctx),
        "and the record is kept for a later run"
    );
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

/// The one Claude Code write this lock cannot exclude. Measured in 2.1.278: a `/logout`
/// that has given up waiting deletes the credential with nothing held. If it lands just
/// after the install, the incoming login is gone, and pitboard used to print "Switched to
/// work" and exit 0 over an account that was signed out.
#[test]
fn a_switch_whose_login_was_removed_again_does_not_report_a_switch() {
    let m = machine("did-not-hold");
    let parked_before = m.mem.vault().services();

    let settled = settle(&m.ctx).expect("nothing to recover yet").0;
    m.mem.live().fault(&m.service, Fault::DeletedAfterWrite);
    let failed = switch(settled, "there").expect_err("the login did not stay");
    assert!(
        matches!(failed, Error::SwitchDidNotHold { .. }),
        "got {failed:?}"
    );

    let state = state::load(&m.ctx).expect("state");
    // Both logins are still here: the one that was parked on the way out, and the one that
    // was being installed. Neither may be thrown away over a write that did not hold.
    assert!(
        state.get("here").expect("account").parked.is_some(),
        "the outgoing login was parked and stays parked"
    );
    assert!(
        state.get("there").expect("account").parked.is_some(),
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
fn a_switch_that_cannot_read_the_store_back_keeps_every_copy_and_its_record() {
    let m = machine("unverified");
    let before = m.mem.live().peek(&m.service).expect("a live login");
    let parked_before = m.mem.vault().services();

    // The keychain locks partway through, which is what a screen lock does. The reads the
    // switch makes before it writes still answer; the write and everything after it do not.
    let settled = settle(&m.ctx).expect("nothing to recover yet").0;
    m.mem.live().fault(&m.service, Fault::LocksOnWrite);
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
            at: None,
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
