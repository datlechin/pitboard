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
fn a_parked_login_anthropic_refuses_is_not_installed() {
    let m = machine("refused-park");
    m.api
        .token_trouble("access-there-refresh", Trouble::Unauthorized);
    m.api.renew_trouble("there-refresh", Trouble::InvalidGrant);
    let live_before = m.mem.live().peek(&m.service);

    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed = switch(settled, &Key::new(ProviderId::Claude, "there"))
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
            .get(&Key::new(ProviderId::Claude, "here"))
            .expect("account")
            .parked
            .is_none(),
        "and it was never parked, because nothing was taken away"
    );
    let there = state
        .get(&Key::new(ProviderId::Claude, "there"))
        .expect("account");
    assert_eq!(there.email, "there@example.com", "the account is kept");
    assert!(there.parked.is_none(), "the copy that cannot work is not");
    assert!(
        state
            .discarded
            .iter()
            .any(|d| d.service.starts_with("pitboard-park-there-")),
        "the dead copy is listed for deletion, so a delete that fails is retried"
    );

    recover(&m).expect("the next command settles");
    assert!(
        m.mem.vault().services().is_empty(),
        "and that is when it goes"
    );
}

// Claude Code keeps no login past 256 pieces of 2400 characters, 460800 bytes, in Credential
// Manager, and neither does a switch.
#[cfg(windows)]
#[test]
fn a_login_too_large_for_credential_manager_is_refused_before_anything_moves() {
    let m = machine("too-large-for-pieces");
    let mut login: Value =
        serde_json::from_str(&m.mem.live().peek(&m.service).expect("a live login")).expect("JSON");
    login["mcpOAuth"]["large"] =
        serde_json::json!("x".repeat(provider::claude::pieces::MOST_BYTES));
    m.mem.live().plant(&m.service, &login.to_string());
    let (live_before, vault_before) = (m.mem.live().services(), m.mem.vault().services());

    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed = switch(settled, &Key::new(ProviderId::Claude, "there")).expect_err("too large");

    assert_eq!(failed.code(), "credential_too_large");
    assert_eq!(m.mem.live().services(), live_before);
    assert_eq!(m.mem.vault().services(), vault_before);
    assert!(!journal::pending(&m.ctx));
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

    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed = switch(settled, &Key::new(ProviderId::Claude, "there"))
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
fn a_switch_will_not_install_a_login_it_could_not_ask_about() {
    let m = machine("offline-in");
    m.api
        .token_trouble("access-there-refresh", Trouble::Offline);
    let live_before = m.mem.live().peek(&m.service);
    let parked_before = m.mem.vault().services();

    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed =
        switch(settled, &Key::new(ProviderId::Claude, "there")).expect_err("nobody could be asked");
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
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let (outcome, _) = switch(settled, &Key::new(ProviderId::Claude, "there")).expect("a switch");
    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");

    let asked = m.api.asked();
    assert!(
        asked.iter().any(
            |q| matches!(q, crate::api::scripted::Asked::Identity(t) if t == "access-there-refresh")
        ),
        "the login going in is asked about, not only the one coming out: {asked:?}"
    );
}

/// The login going out is the one Anthropic last named for the store, by its refresh
/// token's fingerprint, so only the login going in is asked about. The login going out was
/// asked about on every switch, which also refused to move a login whose session had lapsed
/// though its account was known.
#[test]
fn a_switch_from_a_login_already_identified_asks_only_about_the_login_going_in() {
    let m = machine("known-going-out");
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let (outcome, _) = switch(settled, &m.key("there")).expect("a switch");
    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
    assert_eq!(
        m.api.asked(),
        [crate::api::scripted::Asked::Identity(
            "access-there-refresh".into()
        )]
    );
}

/// Two situations with one message until now. Nobody signed in is an ordinary state with an
/// ordinary answer. Claude Code's config naming somebody as signed in while Pitboard finds
/// no login anywhere it looks means Pitboard is looking in the wrong place, and writing a
/// login there would put it where nobody reads.
#[test]
fn a_login_pitboard_cannot_find_is_not_the_same_as_nobody_being_signed_in() {
    let m = machine("elsewhere");

    // Claude Code's config still says who is signed in; the login is not in any store.
    m.mem.live().delete_everything();
    let recorded = state_file(&m);
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let failed = switch(settled, &Key::new(ProviderId::Claude, "there"))
        .expect_err("there is nothing to move");
    match &failed {
        Error::LiveCredentialElsewhere { email } => assert_eq!(email, "here@example.com"),
        other => panic!("got {other:?}"),
    }
    assert_eq!(failed.code(), "live_credential_elsewhere");
    // Nothing is known of whose login the store holds, so nothing is recorded of it.
    assert_eq!(state_file(&m), recorded);
    assert!(crate::audit::read(&m.ctx, 100).is_empty());

    // With nothing in the config either, nobody is signed in, and the switch puts `there`'s
    // login where Claude Code reads it.
    std::fs::write(m.ctx_home().join(".claude.json"), "{}").expect("a config");
    let settled = settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0;
    let (outcome, _) = switch(settled, &Key::new(ProviderId::Claude, "there"))
        .expect("nobody is signed in to switch from");
    assert!(
        matches!(&outcome, Outcome::Installed { to, .. } if to == "there"),
        "got {outcome:?}"
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

// The session's fresh PC (2026-10-18) refused `pitboard use a` right after
// `pitboard enroll a --sign-in` said to run it, with nothing signed in to Claude Code yet.
#[test]
fn the_first_account_goes_in_where_nothing_is_signed_in() {
    for (tool, make) in [
        ("claude", machine as fn(&str) -> harness::Machine),
        ("codex", harness::codex_machine),
    ] {
        let m = make(&format!("first-{tool}"));
        let (live, _) = m.live_store();
        live.delete_everything();
        // Claude Code that never ran has no config at all.
        let _ = std::fs::remove_file(m.ctx_home().join(".claude.json"));
        let settled = settle(&m.ctx, Permit::for_a_test(), None)
            .expect("nothing to recover")
            .0;
        let (outcome, warnings) = switch(settled, &m.key("there")).expect(tool);
        assert!(
            matches!(&outcome, Outcome::Installed { to, .. } if *to == m.key("there").typed()),
            "{tool}: {outcome:?}"
        );
        assert!(
            warnings.iter().all(|w| w.code() != "config_not_updated"),
            "{tool}: {warnings:?}"
        );
        let installed = m.live().expect("a login where the tool reads it");
        let refresh = match harness::Tool::of(m.which) {
            harness::Tool::Claude => &installed["claudeAiOauth"]["refreshToken"],
            harness::Tool::Codex => &installed["tokens"]["refresh_token"],
        };
        assert_eq!(refresh, "there-refresh", "{tool}");
        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state
                .in_use(m.which)
                .and_then(|known| known.owner.as_ref())
                .map(|owner| owner.email.as_str()),
            Some("there@example.com"),
            "{tool}"
        );
        assert!(
            state
                .get(&m.key("there"))
                .expect("enrolled")
                .parked
                .is_none(),
            "{tool}: its park is the login in use now"
        );
    }
}

/// While `.credentials.json` sits behind the keychain, a session already running keeps the
/// login it holds until that login is next renewed, whatever the file holds, as the
/// register's `fallback_file_pins_session_login` reads 2.1.294: it looks at the file and
/// never reads it to decide. A switch says so, naming the file, in place of the 33 seconds
/// it takes without one, also where Pitboard cannot read what the file holds.
#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "on Linux the file is Claude Code's only store, never behind a keychain"
)]
fn a_switch_with_a_file_behind_the_keychain_says_sessions_take_it_at_renewal() {
    let switched = |m: &harness::Machine| {
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
                store::memory::Fault::UnreadableContents("permission denied".into()),
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
fn config_names_now(m: &harness::Machine) -> String {
    let config: Value = serde_json::from_str(
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
fn using_the_account_in_use_writes_claude_codes_config_again() {
    let m = machine("use-names-again");
    harness::config_names(&m, "there");
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
    let backups = std::fs::read_dir(home::dir(&m.ctx).join("backups"))
        .map(Iterator::count)
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
fn a_config_that_could_not_be_written_again_is_said() {
    let m = machine("use-names-again-refused");
    harness::config_names(&m, "there");
    std::fs::write(home::dir(&m.ctx).join("backups"), "").expect("in the way");
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
fn a_switch_whose_config_could_not_be_written_is_said_to_name_another() {
    let m = machine("switch-config-refused");
    std::fs::write(home::dir(&m.ctx).join("backups"), "").expect("in the way");
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
fn the_config_is_written_only_while_the_account_is_in_use() {
    let m = machine("name-again");
    harness::config_names(&m, "there");
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

fn settled(m: &harness::Machine) -> Settled {
    settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0
}

/// `here`'s login in use, lapsed, and refused for good as it is renewed, as Claude Code
/// leaves a login nobody ran for a month: the switch cannot park it, and says so first.
fn dead_in_use(name: &str) -> harness::Machine {
    let m = machine(name);
    m.sign_in(&harness::lapsed("here-dead"));
    m.api.renew_trouble("here-dead", Trouble::InvalidGrant);
    m
}

#[test]
fn a_switch_from_a_login_anthropic_refuses_for_good_asks_first_and_moves_nothing() {
    let m = dead_in_use("replace-asks");
    let before = m.mem.live().peek(&m.service);

    let asked = switch(settled(&m), &m.key("there")).expect_err("it asks first");

    assert!(
        matches!(&asked, Error::LiveLoginRefused { label, .. } if label == "there"),
        "{asked:?}"
    );
    assert_eq!(asked.code(), "live_login_refused");
    assert_eq!(m.mem.live().peek(&m.service), before, "nothing moved");
}

/// Agreed to, the account's login goes in, nothing is parked for the refused one, and the
/// account is recorded in use.
#[test]
fn a_login_anthropic_refuses_for_good_is_replaced_once_agreed_to() {
    let m = dead_in_use("replace-agreed");

    let (outcome, _) = replace(settled(&m), &m.key("there")).expect("replaced");

    assert!(
        matches!(&outcome, Outcome::Installed { to, .. } if to == "there"),
        "{outcome:?}"
    );
    assert_eq!(
        m.live()
            .and_then(|live| live.pointer("/claudeAiOauth/refreshToken").cloned()),
        Some(serde_json::json!("there-refresh"))
    );
    let state = state::load(&m.ctx).expect("state");
    assert!(state.get(&m.key("there")).expect("there").parked.is_none());
    assert!(state.get(&m.key("here")).expect("here").parked.is_none());
    assert_eq!(
        state
            .account_in_use(ProviderId::Claude)
            .map(|a| a.label.as_str()),
        Some("there")
    );
}

/// A login signed in after the switch refused is told, and parked as any switch parks it,
/// never written over.
#[test]
fn a_login_signed_in_since_the_refusal_is_parked_not_written_over() {
    let m = dead_in_use("replace-signed-in-since");
    switch(settled(&m), &m.key("there")).expect_err("it asks first");
    m.api.owned_by("access-here-again", owner("here"));
    m.sign_in(&harness::document("here-again"));

    let (outcome, _) = replace(settled(&m), &m.key("there")).expect("a switch");

    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
    let state = state::load(&m.ctx).expect("state");
    let parked = state.get(&m.key("here")).and_then(|a| a.parked.clone());
    assert_eq!(
        parked.map(|p| p.login),
        Some(store::fingerprint("here-again")),
        "the login signed in since is parked"
    );
}

/// Neither an account on hold, whose login comes back, nor an outage is a login to replace.
#[test]
fn an_account_on_hold_or_an_outage_is_never_offered_a_replacement() {
    for (trouble, code) in [
        (Trouble::OnHold, "account_on_hold"),
        (Trouble::Offline, "identity_unverifiable"),
    ] {
        let m = machine(&format!("replace-not-{code}"));
        m.sign_in(&harness::lapsed("here-held"));
        m.api.renew_trouble("here-held", trouble);
        let before = m.mem.live().peek(&m.service);

        let refused = switch(settled(&m), &m.key("there")).expect_err("no switch");
        assert_eq!(refused.code(), code, "{refused:?}");
        let refused = replace(settled(&m), &m.key("there")).expect_err("no replacement");
        assert_eq!(refused.code(), code, "{refused:?}");
        assert_eq!(
            m.mem.live().peek(&m.service),
            before,
            "{code}: nothing moved"
        );
    }
}

/// A login Anthropic will not name may still work: only a refresh token refused for good is
/// a login to replace, as Claude Code takes `invalid_grant` alone to end one.
#[test]
fn a_login_anthropic_will_not_name_is_never_offered_for_replacing() {
    let m = machine("replace-unnamed");
    m.sign_in(&harness::document("here-unnamed"));
    m.api.token_trouble(
        "access-here-unnamed",
        Trouble::Answered(
            400,
            r#"{"error":{"type":"invalid_request_error","message":"no"}}"#,
        ),
    );
    let before = m.mem.live().peek(&m.service);

    for change in [switch, replace] {
        let refused = change(settled(&m), &m.key("there")).expect_err("no switch");
        assert_eq!(refused.code(), "identity_unverifiable", "{refused:?}");
    }
    assert_eq!(m.mem.live().peek(&m.service), before, "nothing moved");
}

/// A lapsed login renewed, whose renewed access token `validate` then refuses: the login
/// Anthropic has just renewed is never written over.
#[test]
fn a_login_renewed_then_refused_a_name_is_never_written_over() {
    let m = machine("replace-renewed-then-refused");
    m.sign_in(&harness::lapsed("here-lapsed"));
    harness::renews(&m, "here-lapsed", "here-renewed");
    m.api.token_trouble(
        "access-here-renewed",
        Trouble::Answered(404, r#"{"error":{"type":"not_found_error"}}"#),
    );

    let refused = replace(settled(&m), &m.key("there")).expect_err("no replacement");

    assert_eq!(refused.code(), "identity_unverifiable", "{refused:?}");
    assert_eq!(
        m.live()
            .and_then(|live| live.pointer("/claudeAiOauth/refreshToken").cloned()),
        Some(serde_json::json!("here-renewed")),
        "the renewed login is where the renewal saved it"
    );
}

/// `validate` names the account and no organisation, and the profile is refused: the
/// organisation comes from Claude Code's own record of that account, and the switch is made.
#[test]
fn a_login_validated_without_an_organisation_is_told_by_claude_codes_record() {
    let m = machine("validate-unorganised");
    m.sign_in(&harness::document("here-again"));
    m.api.owned_by("access-here-again", owner("here"));
    m.api.validated_without_organisation("access-here-again");
    m.api.profile_trouble(
        "access-here-again",
        Trouble::Answered(400, r#"{"error":{"type":"invalid_request_error"}}"#),
    );

    let (outcome, _) = switch(settled(&m), &m.key("there")).expect("a switch");

    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
}

/// Killed once the account's login is written in place of the refused one, before anything is
/// recorded: the journal holds every renewal of the park going in back, and the next change
/// finishes it, whether or not Claude Code renewed the login in the slot meanwhile.
#[test]
fn a_replacement_killed_once_written_is_finished_by_the_next_change() {
    for rotated in [false, true] {
        let m = dead_in_use(&format!("replace-killed-{rotated}"));
        let died = fault::killing("replace.installed", || {
            replace(settled(&m), &m.key("there")).map(|_| ())
        });
        assert_eq!(died.err().as_deref(), Some("replace.installed"));
        if rotated {
            m.api.owned_by("access-there-rotated", owner("there"));
            m.sign_in(&harness::document("there-rotated"));
        }

        assert!(
            journal::pending(&m.ctx),
            "{rotated}: the replacement waits, which holds every renewal of a park back"
        );
        assert!(
            renew_due(&m.ctx, Permit::for_a_test(), Due::ToStayAlive).is_empty(),
            "{rotated}"
        );

        drop(settled(&m));
        let state = state::load(&m.ctx).expect("state");
        assert!(
            state.get(&m.key("there")).expect("there").parked.is_none(),
            "{rotated}: the park now in use is dropped"
        );
        assert_eq!(
            state
                .account_in_use(ProviderId::Claude)
                .map(|a| a.label.as_str()),
            Some("there"),
            "{rotated}"
        );
    }
}

/// Anthropic names the account of the login in use and will not give its email, and nothing
/// else names it: the login works, so it is never offered for replacing.
#[test]
fn a_login_whose_email_cannot_be_learned_is_not_offered_for_replacing() {
    let m = machine("replace-unnamed-email");
    m.sign_in(&harness::document("stranger"));
    m.api.owned_by("access-stranger", owner("stranger"));
    m.api.profile_trouble(
        "access-stranger",
        Trouble::Answered(400, r#"{"error":{"type":"invalid_request_error"}}"#),
    );

    let refused = switch(settled(&m), &m.key("there")).expect_err("no switch");

    assert!(
        matches!(
            &refused,
            Error::IdentityUnverifiable {
                login: crate::error::Unidentified::Unnamed,
                ..
            }
        ),
        "{refused:?}"
    );
}

/// A login with no refresh token cannot be renewed, so it is replaced as a refused one is.
/// Killed with the record written and nothing installed, the next change finds a slot whose
/// login has no refresh token, which the login going in has, and settles it as not finished,
/// keeping the park, rather than asking Anthropic about a login it cannot name.
#[test]
fn a_replacement_of_a_login_with_no_refresh_token_killed_before_its_write_is_undone() {
    let m = machine("replace-no-refresh");
    let mut lapsed = harness::lapsed("here-none");
    lapsed["claudeAiOauth"]
        .as_object_mut()
        .expect("a login")
        .remove("refreshToken");
    m.sign_in(&lapsed);

    let asked = switch(settled(&m), &m.key("there")).expect_err("it asks first");
    assert_eq!(asked.code(), "live_login_refused", "{asked:?}");
    let died = fault::killing("replace.journal_written", || {
        replace(settled(&m), &m.key("there")).map(|_| ())
    });
    assert_eq!(died.err().as_deref(), Some("replace.journal_written"));

    let (settled, warnings) = settle(&m.ctx, Permit::for_a_test(), None).expect("settled");
    drop(settled);
    let said: Vec<String> = warnings.iter().map(ToString::to_string).collect();
    assert!(
        said.iter()
            .any(|w| w.starts_with("an earlier switch to `there` was interrupted; it had not")),
        "{said:?}"
    );
    let state = state::load(&m.ctx).expect("state");
    assert!(
        state.get(&m.key("there")).expect("there").parked.is_some(),
        "the park going in is kept"
    );
    assert!(!journal::pending(&m.ctx));
}

// Codex 0.164.0 signs out of a login from a workspace `forced_chatgpt_workspace_id` leaves out
// as it starts, deleting it (`codex_login_restrictions`), and a Codex park is the only copy.
#[test]
fn a_codex_login_its_settings_rule_out_is_refused_before_anything_moves() {
    let m = harness::codex_machine("restricted");
    let config = m.ctx_home().join(".codex").join("config.toml");
    let there = || {
        state::load(&m.ctx)
            .expect("state")
            .get(&m.key("there"))
            .and_then(|account| account.parked.clone())
    };
    let (live, vault, parked) = (m.live(), m.mem.vault().services(), there());
    for (holds, set) in [
        (
            "forced_chatgpt_workspace_id = \"here\"\n",
            "forced_chatgpt_workspace_id",
        ),
        ("forced_login_method = \"api\"\n", "forced_login_method"),
    ] {
        std::fs::write(&config, holds).expect("a config");
        let refused = switch(settled(&m), &m.key("there")).expect_err(set);
        assert_eq!(refused.code(), "login_restricted", "{refused:?}");
        assert_eq!(refused.exit_code(), 3);
        let said = refused.to_string();
        assert!(said.contains(set) && said.contains("config.toml"), "{said}");
        assert_eq!(m.live(), live, "{set}: the login in use stays");
        assert_eq!(m.mem.vault().services(), vault, "{set}");
        assert_eq!(there(), parked, "{set}: and so does the park");
    }

    std::fs::write(
        &config,
        "forced_chatgpt_workspace_id = [\"here\", \"there\"]\n",
    )
    .expect("a config");
    switch(settled(&m), &m.key("there")).expect("a workspace it allows");
}

// Claude Code 2.1.309 refuses, as a session starts, a login of an organisation that a managed
// `forceLoginOrgUUID` leaves out (organisation_pin), so it would not use the account switched to.
#[test]
fn a_claude_code_login_of_an_organisation_the_managed_settings_leave_out_is_refused() {
    let m = machine("organisation-pin");
    let file = crate::settings::managed_dir(m.ctx.os).join("managed-settings.json");
    let there = || {
        state::load(&m.ctx)
            .expect("state")
            .get(&m.key("there"))
            .and_then(|account| account.parked.clone())
    };
    let (live, vault, parked) = (m.live(), m.mem.vault().services(), there());
    for (pin, named) in [
        (serde_json::json!("org-here"), "`org-here`"),
        (
            serde_json::json!(["org-here", "org-elsewhere"]),
            "`org-here` and `org-elsewhere`",
        ),
        (serde_json::json!([]), "an empty list"),
    ] {
        m.mem.administers(
            &file,
            &serde_json::json!({ "forceLoginOrgUUID": pin }).to_string(),
        );
        let refused = switch(settled(&m), &m.key("there")).expect_err("refused");
        assert_eq!(refused.code(), "login_restricted", "{refused:?}");
        assert_eq!(refused.exit_code(), 3);
        let said = refused.to_string();
        assert!(
            said.starts_with("Claude Code would not keep `there`'s login:")
                && said.contains(&format!(
                    "{} sets `forceLoginOrgUUID` to {named}, and this login is of `org-there`",
                    file.display()
                )),
            "{said}"
        );
        assert_eq!(m.live(), live, "{pin}: the login in use stays");
        assert_eq!(m.mem.vault().services(), vault, "{pin}");
        assert_eq!(there(), parked, "{pin}: and so does the park");
    }

    m.mem.administers(
        &file,
        &serde_json::json!({"forceLoginOrgUUID": ["org-here", "org-there"]}).to_string(),
    );
    switch(settled(&m), &m.key("there")).expect("an organisation it allows");
}
