//! What a switch does when it should not happen at all.
//!
//! A switch used to ask Anthropic twice about the login it was throwing away and never
//! once about the login it was installing. So an account whose refresh chain had been
//! revoked, signed out elsewhere, or refused installed cleanly, read back cleanly, and
//! reported a switch; the person found out the next time they ran `claude`, by which point
//! the login they had left was parked and the one they had arrived at did not work.

use super::harness::{machine, owner, recover};
use super::*;
use crate::api::scripted::Trouble;

/// The account is kept, the label is kept, and the copy that cannot work is dropped, so the
/// way back is one sign-in rather than an enrolment.
#[test]
fn a_parked_login_anthropic_refuses_is_not_installed() {
    let m = machine("refused-park");
    m.api
        .token_trouble("access-there-refresh", Trouble::Unauthorized);
    m.api.renew_trouble("there-refresh", Trouble::InvalidGrant);
    let live_before = m.mem.live().peek(&m.service);

    let settled = settle(&m.ctx).expect("nothing to recover").0;
    let failed = switch(settled, "there").expect_err("a refused login is not a switch");
    assert!(
        matches!(failed, Error::ParkedLoginRefused { .. }),
        "got {failed:?}"
    );

    assert_eq!(
        m.mem.live().peek(&m.service),
        live_before,
        "the login that was working is exactly where it was"
    );
    let state = state::load(&m.ctx).expect("state");
    assert!(
        state.get("here").expect("account").parked.is_none(),
        "and it was never parked, because nothing was taken away"
    );
    let there = state.get("there").expect("account");
    assert_eq!(there.email, "there@example.com", "the account is kept");
    assert!(there.parked.is_none(), "the copy that cannot work is not");
    assert!(
        state
            .discarded
            .iter()
            .any(|s| s.starts_with("pitboard-park-there-")),
        "the dead copy is listed for deletion, so a delete that fails is retried"
    );

    recover(&m).expect("the next command settles");
    assert!(
        m.mem.vault().services().is_empty(),
        "and that is when it goes"
    );
}

/// A park filed under the wrong label is the failure the identity check exists to prevent,
/// caught on the way in rather than after both accounts have moved.
#[test]
fn a_parked_login_that_belongs_to_another_account_is_refused() {
    let m = machine("misfiled-park");
    m.api
        .owned_by("access-there-refresh", owner("somebody-else"));
    let live_before = m.mem.live().peek(&m.service);
    let parked_before = m.mem.vault().services();

    let settled = settle(&m.ctx).expect("nothing to recover").0;
    let failed = switch(settled, "there").expect_err("it is not that account's login");
    assert!(
        matches!(failed, Error::ParkedLoginBelongsElsewhere { .. }),
        "got {failed:?}"
    );

    assert_eq!(m.mem.live().peek(&m.service), live_before);
    assert_eq!(
        m.mem.vault().services(),
        parked_before,
        "nothing is deleted over a park pitboard will not use"
    );
    recover(&m).expect("and the machine is untouched");
}

/// With nobody to ask about the login going in, the answer is to do nothing. The login that
/// is working is worth more than the switch.
#[test]
fn a_switch_will_not_install_a_login_it_could_not_ask_about() {
    let m = machine("offline-in");
    m.api
        .token_trouble("access-there-refresh", Trouble::Offline);
    let live_before = m.mem.live().peek(&m.service);
    let parked_before = m.mem.vault().services();

    let settled = settle(&m.ctx).expect("nothing to recover").0;
    let failed = switch(settled, "there").expect_err("nobody could be asked");
    assert!(
        matches!(
            failed,
            Error::IdentityUnverifiable { .. } | Error::RenewalFailed { .. }
        ),
        "got {failed:?}"
    );

    assert_eq!(m.mem.live().peek(&m.service), live_before);
    assert_eq!(m.mem.vault().services(), parked_before);
    assert!(!journal::pending(&m.ctx), "and nothing was begun");
}

/// The ordinary path, kept honest: a park that answers for the account it is filed under is
/// installed, and asking about it costs one round trip and no writes.
#[test]
fn a_park_that_answers_for_its_own_account_is_installed() {
    let m = machine("proved");
    let settled = settle(&m.ctx).expect("nothing to recover").0;
    let (outcome, _) = switch(settled, "there").expect("a switch");
    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");

    let asked = m.api.asked();
    assert!(
        asked.iter().any(
            |q| matches!(q, crate::api::scripted::Asked::Owner(t) if t == "access-there-refresh")
        ),
        "the login going in is asked about, not only the one coming out: {asked:?}"
    );
}

/// Two situations with one message until now. Nobody signed in is an ordinary state with an
/// ordinary answer. Claude Code's config naming somebody as signed in while pitboard finds
/// no login anywhere it looks means pitboard is looking in the wrong place, and writing a
/// login there would put it where nobody reads.
#[test]
fn a_login_pitboard_cannot_find_is_not_the_same_as_nobody_being_signed_in() {
    let m = machine("elsewhere");

    // Claude Code's config still says who is signed in; the login is not in any store.
    m.mem.live().delete_everything();
    let settled = settle(&m.ctx).expect("nothing to recover").0;
    let failed = switch(settled, "there").expect_err("there is nothing to move");
    match &failed {
        Error::LiveCredentialElsewhere { email } => assert_eq!(email, "here@example.com"),
        other => panic!("got {other:?}"),
    }
    assert_eq!(failed.code(), "live_credential_elsewhere");

    // With nothing in the config either, nobody is signed in and that is all it says.
    std::fs::write(m.ctx_home().join(".claude.json"), "{}").expect("a config");
    let settled = settle(&m.ctx).expect("nothing to recover").0;
    let failed = switch(settled, "there").expect_err("still nothing to move");
    assert!(
        matches!(failed, Error::LiveCredentialAbsent),
        "got {failed:?}"
    );
}
