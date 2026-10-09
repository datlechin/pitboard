//! What a switch does when it should not happen at all.
//!
//! A switch used to ask Anthropic twice about the login it was throwing away and never
//! once about the login it was installing. So an account whose refresh chain had been
//! revoked, signed out elsewhere, or refused installed cleanly, read back cleanly, and
//! reported a switch; the person found out the next time they ran `claude`, by which point
//! the login they had left was parked and the one they had arrived at did not work.

use super::harness::{audit_lines, machine, owner, recover, state_file};
use super::*;
use crate::api::scripted::Trouble;
use crate::service::Permit;

/// The account is kept, the label is kept, and the copy that cannot work is dropped, so the
/// way back is one sign-in rather than an enrolment.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_parked_login_anthropic_refuses_is_not_installed() {
    let m = machine("refused-park");
    m.api
        .token_trouble("access-there-refresh", Trouble::Unauthorized);
    m.api.renew_trouble("there-refresh", Trouble::InvalidGrant);
    let live_before = m.mem.live().peek(&m.service);

    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed = switch(
        settled,
        &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
    )
    .expect_err("a refused login is not a switch");
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
        state
            .get(&crate::state::Key::new(
                crate::provider::ProviderId::Claude,
                "here"
            ))
            .expect("account")
            .parked
            .is_none(),
        "and it was never parked, because nothing was taken away"
    );
    let there = state
        .get(&crate::state::Key::new(
            crate::provider::ProviderId::Claude,
            "there",
        ))
        .expect("account");
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
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_parked_login_that_belongs_to_another_account_is_refused() {
    let m = machine("misfiled-park");
    m.api
        .owned_by("access-there-refresh", owner("somebody-else"));
    let live_before = m.mem.live().peek(&m.service);
    let parked_before = m.mem.vault().services();

    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed = switch(
        settled,
        &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
    )
    .expect_err("it is not that account's login");
    assert!(
        matches!(failed, Error::ParkedLoginBelongsElsewhere { .. }),
        "got {failed:?}"
    );

    assert_eq!(m.mem.live().peek(&m.service), live_before);
    assert_eq!(
        m.mem.vault().services(),
        parked_before,
        "nothing is deleted over a park Pitboard will not use"
    );
    recover(&m).expect("and the machine is untouched");
}

/// With nobody to ask about the login going in, the answer is to do nothing. The login that
/// is working is worth more than the switch.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_will_not_install_a_login_it_could_not_ask_about() {
    let m = machine("offline-in");
    m.api
        .token_trouble("access-there-refresh", Trouble::Offline);
    let live_before = m.mem.live().peek(&m.service);
    let parked_before = m.mem.vault().services();

    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed = switch(
        settled,
        &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
    )
    .expect_err("nobody could be asked");
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
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_park_that_answers_for_its_own_account_is_installed() {
    let m = machine("proved");
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let (outcome, _) = switch(
        settled,
        &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
    )
    .expect("a switch");
    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");

    let asked = m.api.asked();
    assert!(
        asked.iter().any(
            |q| matches!(q, crate::api::scripted::Asked::Owner(t) if t == "access-there-refresh")
        ),
        "the login going in is asked about, not only the one coming out: {asked:?}"
    );
}

/// The login going out is the one Anthropic last named for the store, by its refresh
/// token's fingerprint, so only the login going in is asked about. The login going out was
/// asked about on every switch, which also refused to move a login whose session had lapsed
/// though its account was known.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_from_a_login_already_identified_asks_only_about_the_login_going_in() {
    let m = machine("known-going-out");
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let (outcome, _) = switch(settled, &m.key("there")).expect("a switch");
    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
    assert_eq!(
        m.api.asked(),
        [crate::api::scripted::Asked::Owner(
            "access-there-refresh".into()
        )]
    );
}

/// Two situations with one message until now. Nobody signed in is an ordinary state with an
/// ordinary answer. Claude Code's config naming somebody as signed in while Pitboard finds
/// no login anywhere it looks means Pitboard is looking in the wrong place, and writing a
/// login there would put it where nobody reads.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn a_login_pitboard_cannot_find_is_not_the_same_as_nobody_being_signed_in() {
    let m = machine("elsewhere");

    // Claude Code's config still says who is signed in; the login is not in any store.
    m.mem.live().delete_everything();
    let recorded = state_file(&m);
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed = switch(
        settled,
        &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
    )
    .expect_err("there is nothing to move");
    match &failed {
        Error::LiveCredentialElsewhere { email } => assert_eq!(email, "here@example.com"),
        other => panic!("got {other:?}"),
    }
    assert_eq!(failed.code(), "live_credential_elsewhere");
    // Nothing is known of whose login the store holds, so nothing is recorded of it.
    assert_eq!(state_file(&m), recorded);
    assert!(crate::audit::read(&m.ctx, 100).is_empty());

    // With nothing in the config either, nobody is signed in and that is all it says.
    std::fs::write(m.ctx_home().join(".claude.json"), "{}").expect("a config");
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed = switch(
        settled,
        &crate::state::Key::new(crate::provider::ProviderId::Claude, "there"),
    )
    .expect_err("still nothing to move");
    assert!(
        matches!(failed, Error::LiveCredentialAbsent { .. }),
        "got {failed:?}"
    );
    // A sign-out outside Pitboard, which took the only login `here` had.
    assert_eq!(
        audit_lines(&m, "in-use"),
        [
            (String::new(), "signed_out_outside".to_string()),
            ("here".to_string(), "login_replaced".to_string()),
        ]
    );
}

/// While `.credentials.json` sits behind the keychain, a session already running keeps the
/// login it holds until that login is next renewed, whatever the file holds, as the
/// register's `fallback_file_pins_session_login` reads 2.1.294: it looks at the file and
/// never reads it to decide. A switch says so, naming the file, in place of the 33 seconds
/// it takes without one, also where Pitboard cannot read what the file holds.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
#[cfg_attr(
    target_os = "linux",
    ignore = "on Linux the file is Claude Code's only store, never behind a keychain"
)]
fn a_switch_with_a_file_behind_the_keychain_says_sessions_take_it_at_renewal() {
    let switched = |m: &super::harness::Machine| {
        let settled = settle(&m.ctx, Permit::for_a_test(), None)
            .expect("nothing to recover")
            .0;
        switch(settled, &m.key("there")).expect("a switch").0
    };

    let m = machine("adoption-nothing-behind");
    assert!(
        matches!(
            switched(&m),
            Outcome::Switched {
                adoption: provider::Adoption::PollingWithin(33),
                ..
            }
        ),
        "nothing behind the keychain"
    );

    for (name, unreadable) in [
        ("adoption-file-behind", false),
        ("adoption-unreadable-behind", true),
    ] {
        let m = machine(name);
        let file = provider::claude::live::credential_file(&m.ctx);
        let behind = m.mem.file_at(file.clone());
        behind.plant(&m.service, "{}");
        if unreadable {
            behind.fault(
                &m.service,
                crate::store::memory::Fault::UnreadableContents("permission denied".into()),
            );
        }
        let outcome = switched(&m);
        let Outcome::Switched { adoption, .. } = outcome else {
            panic!("{name}: a switch, not {outcome:?}");
        };
        assert_eq!(adoption, provider::Adoption::AtRenewal { file }, "{name}");
    }
}

/// What Claude Code's config names, by account.
fn config_names_now(m: &super::harness::Machine) -> String {
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(m.ctx_home().join(".claude.json")).expect("a config"),
    )
    .expect("JSON");
    config["oauthAccount"]["accountUuid"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// Claude Code's config names another account than the one whose login it has stored, as a
/// Claude Code process started on another login leaves it, and `/status` in Claude Code shows
/// that one. Using the account in use writes it there again, as a switch to it does, keeping
/// a copy of the config first, and moves no login. It changed nothing.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn using_the_account_in_use_writes_claude_codes_config_again() {
    let m = machine("use-names-again");
    super::harness::config_names(&m, "there");
    let live = m.live();
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());

    let done = pitboard.switch_to("here").expect("already in use");

    assert!(
        matches!(
            done.value,
            Outcome::AlreadyActive {
                config_updated: true,
                ..
            }
        ),
        "{:?}",
        done.value
    );
    assert!(done.warnings.is_empty(), "{:?}", done.warnings);
    assert_eq!(config_names_now(&m), "here");
    assert_eq!(m.live(), live, "no login moved");
    let backups = std::fs::read_dir(crate::home::dir(&m.ctx).join("backups"))
        .map(|entries| entries.count())
        .unwrap_or_default();
    assert_eq!(backups, 1, "a copy of the config is kept first");
    assert_eq!(
        audit_lines(&m, "use"),
        [("here".to_string(), "config_updated".to_string())]
    );
    let read = pitboard.status_offline().expect("a read of what is known");
    let codes: Vec<&str> = read.warnings.iter().map(Warning::code).collect();
    assert!(codes.is_empty(), "{codes:?}");

    let again = pitboard.switch_to("here").expect("already in use");
    assert!(
        matches!(
            again.value,
            Outcome::AlreadyActive {
                config_updated: false,
                ..
            }
        ),
        "{:?}",
        again.value
    );
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    assert_eq!(
        audit_lines(&m, "use"),
        [
            ("here".to_string(), "config_updated".to_string()),
            ("here".to_string(), "already_active".to_string()),
        ]
    );
}

/// Where the config cannot be written, the account is still the one in use and the command
/// says why the config was not, as a switch does; the config and what Pitboard records of
/// it are as they were, so every read still says it names another account.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_config_that_could_not_be_written_again_is_said() {
    let m = machine("use-names-again-refused");
    super::harness::config_names(&m, "there");
    std::fs::write(crate::home::dir(&m.ctx).join("backups"), "").expect("in the way");
    let config = std::fs::read(m.ctx_home().join(".claude.json")).expect("a config");
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());

    let done = pitboard.switch_to("here").expect("already in use");

    assert!(
        matches!(
            done.value,
            Outcome::AlreadyActive {
                config_updated: false,
                ..
            }
        ),
        "{:?}",
        done.value
    );
    let codes: Vec<&str> = done.warnings.iter().map(Warning::code).collect();
    assert_eq!(codes, ["config_backup_failed"]);
    assert_eq!(
        std::fs::read(m.ctx_home().join(".claude.json")).expect("a config"),
        config
    );
    assert_eq!(
        audit_lines(&m, "use"),
        [("here".to_string(), "already_active".to_string())]
    );
    let read = pitboard.status_offline().expect("a read of what is known");
    let codes: Vec<&str> = read.warnings.iter().map(Warning::code).collect();
    assert_eq!(codes, ["config_names_another"]);
}

/// A switch whose login landed and whose config could not be written records the config as
/// naming what it named before, as `use` does, so a read from files alone says the config
/// names another account, not that something may have signed in.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_whose_config_could_not_be_written_is_said_to_name_another() {
    let m = machine("switch-config-refused");
    std::fs::write(crate::home::dir(&m.ctx).join("backups"), "").expect("in the way");
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());

    let done = pitboard.switch_to("there").expect("a switch");

    assert!(
        matches!(done.value, Outcome::Switched { .. }),
        "{:?}",
        done.value
    );
    let codes: Vec<&str> = done.warnings.iter().map(Warning::code).collect();
    assert_eq!(codes, ["config_backup_failed"]);
    assert_eq!(config_names_now(&m), "here");
    let read = pitboard.status_offline().expect("a read of what is known");
    let codes: Vec<&str> = read.warnings.iter().map(Warning::code).collect();
    assert_eq!(codes, ["config_names_another"]);
}

/// The app's button writes the account in use into Claude Code's config as `use` does, and is
/// recorded as `use`. Pressed once another account is in use, as after a switch that landed
/// before the press, it writes nothing and moves no login, where `use` would switch back.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_config_is_written_only_while_the_account_is_in_use() {
    let m = machine("name-again");
    super::harness::config_names(&m, "there");
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());

    let done = pitboard.update_config("here").expect("in use");
    assert!(
        matches!(
            done.value,
            Outcome::AlreadyActive {
                config_updated: true,
                ..
            }
        ),
        "{:?}",
        done.value
    );
    assert_eq!(config_names_now(&m), "here");

    pitboard.switch_to("there").expect("a switch");
    let live = m.live();
    let config = std::fs::read(m.ctx_home().join(".claude.json")).expect("a config");
    let refused = pitboard.update_config("here").expect_err("not in use");
    assert!(
        matches!(refused.error, Error::AccountNotInUse { .. }),
        "{:?}",
        refused.error
    );
    assert_eq!(m.live(), live, "no login moved");
    assert_eq!(
        std::fs::read(m.ctx_home().join(".claude.json")).expect("a config"),
        config
    );
    assert_eq!(
        audit_lines(&m, "use"),
        [
            ("here".to_string(), "config_updated".to_string()),
            ("there".to_string(), "ok".to_string()),
            ("here".to_string(), "account_not_in_use".to_string()),
        ]
    );
}
