use super::*;

#[test]
fn a_credential_whose_login_is_not_an_object_is_a_failure() {
    let mut f = facts();
    f.credential = Ok(Some(json!({"claudeAiOauth": "something else"})));
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "credential").level, Level::Fail);
    assert!(!healthy(&checks));
}

#[test]
fn an_unreadable_store_fails_rather_than_reporting_nothing_stored() {
    let mut f = facts();
    f.credential = Err(store::Error::Unreadable("security exited 1".into()));
    f.backend = Err(store::Error::Unreadable("security exited 1".into()));
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "credential").level, Level::Fail);
    assert_eq!(check(&checks, "credential_store").level, Level::Fail);
}

// Every parked login behind it used to fail too, each advised to sign in again for nothing.
#[test]
fn a_locked_keychain_is_one_failure_and_sends_nobody_to_sign_in_again() {
    let mut f = facts();
    f.os = Os::MacOs;
    f.credential = Err(store::Error::Locked);
    let mut behind = parked("work", Some(NOW + 20 * 86_400));
    behind.unreadable = Some(Unreadable::Locked);
    f.parks = vec![behind];

    let checks = evaluate(&f);
    let credential = check(&checks, "credential");
    assert_eq!(credential.level, Level::Fail);
    assert_eq!(credential.detail, words::KEYCHAIN_LOCKED);
    assert_eq!(
        credential.advice,
        words::unlock_the_keychain(words::THEN_SIGNED_OUT)
    );
    let park = named(&checks, "account work");
    assert_eq!(park.level, Level::Warn, "{}", park.detail);
    assert_eq!(
        (park.detail.as_str(), park.advice.as_str()),
        (
            words::NOT_READ_THE_KEYCHAIN_IS_LOCKED,
            words::UNLOCK_IT_THEN_DOCTOR
        )
    );
    assert_eq!(
        checks.iter().filter(|c| c.level == Level::Fail).count(),
        1,
        "one cause, one failure"
    );

    f.fallback_login = Some(FallbackLogin {
        path: f.credential_file.clone(),
        fingerprint: Ok(Some("0123456789abcdef".into())),
    });
    assert_eq!(
        check(&evaluate(&f), "credential").advice,
        words::unlock_the_keychain(&words::then_signs_in_with(f.credential_file.display()))
    );
}

#[test]
fn a_login_left_behind_the_keychain_is_said_with_what_reads_it() {
    let mut f = facts();
    f.os = Os::MacOs;
    assert!(evaluate(&f).iter().all(|c| c.code != "fallback_login"));

    f.fallback_login = Some(FallbackLogin {
        path: f.credential_file.clone(),
        fingerprint: Ok(Some("0123456789abcdef".into())),
    });
    let checks = evaluate(&f);
    let said = check(&checks, "fallback_login");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(said.name, "fallback login");
    assert_eq!(
        said.detail,
        words::file_refresh(f.credential_file.display(), "0123456789abcdef")
    );
    assert_eq!(said.advice, words::the_file_signs_sessions_in(Os::MacOs));
    assert!(healthy(&checks), "nothing Pitboard does is stopped by it");
}

// A running session watches the file by its being there, whatever it holds.
#[test]
fn a_file_behind_the_keychain_with_no_login_in_it_is_said_with_what_it_holds_back() {
    let mut f = facts();
    f.fallback_login = Some(FallbackLogin {
        path: f.credential_file.clone(),
        fingerprint: Ok(None),
    });
    let checks = evaluate(&f);
    let said = check(&checks, "fallback_login");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(
        said.detail,
        words::file_without_a_login(f.credential_file.display())
    );
    assert_eq!(said.advice, words::stow_deletes_it());
    assert_eq!(
        check(&checks, "auth_source").detail,
        words::THE_STORED_LOGIN_ONCE_READ
    );

    f.os = Os::MacOs;
    f.credential = Err(store::Error::Locked);
    assert_eq!(
        check(&evaluate(&f), "credential").advice,
        words::unlock_the_keychain(words::THEN_SIGNED_OUT)
    );
}

// A running session looks at the file and never reads it to decide.
#[test]
fn a_file_behind_the_keychain_that_cannot_be_read_is_said_with_why() {
    let mut f = facts();
    f.os = Os::MacOs;
    f.fallback_login = Some(FallbackLogin {
        path: f.credential_file.clone(),
        fingerprint: Err(store::Error::Unreadable(
            "cannot read /home/x/.claude/.credentials.json: Permission denied".into(),
        )),
    });
    let checks = evaluate(&f);
    let said = check(&checks, "fallback_login");
    assert_eq!(said.level, Level::Warn);
    let why = store::Error::Unreadable(
        "cannot read /home/x/.claude/.credentials.json: Permission denied".into(),
    );
    assert_eq!(
        said.detail,
        words::file_not_read(f.credential_file.display(), why)
    );
    assert_eq!(
        said.advice,
        words::delete_it_yourself(Os::MacOs, "rm '/home/x/.claude/.credentials.json'")
    );
    assert!(healthy(&checks), "nothing Pitboard does is stopped by it");

    f.credential = Err(store::Error::Locked);
    assert_eq!(
        check(&evaluate(&f), "credential").advice,
        words::unlock_the_keychain(&words::then_cannot_tell(f.credential_file.display()))
    );
}

// PowerShell deletes it, with each single quote in the path doubled.
#[test]
fn a_file_behind_credential_manager_is_said_in_the_words_of_windows() {
    let mut f = facts();
    f.os = Os::Windows;
    let path = PathBuf::from(r"C:\Users\o'neil\.claude\.credentials.json");
    f.fallback_login = Some(FallbackLogin {
        path: path.clone(),
        fingerprint: Ok(Some("0123456789abcdef".into())),
    });
    assert_eq!(
        check(&evaluate(&f), "fallback_login").advice,
        words::the_file_signs_sessions_in(Os::Windows)
    );

    f.fallback_login = Some(FallbackLogin {
        path,
        fingerprint: Err(store::Error::Unreadable("access is denied".into())),
    });
    assert_eq!(
        check(&evaluate(&f), "fallback_login").advice,
        words::delete_it_yourself(
            Os::Windows,
            r"Remove-Item -LiteralPath 'C:\Users\o''neil\.claude\.credentials.json'"
        )
    );
    assert_eq!(
        Os::Windows.remove_file_command(std::path::Path::new("C:\\it\u{2019}s")),
        "Remove-Item -LiteralPath 'C:\\it\u{2019}\u{2019}s'"
    );
    assert_eq!(
        Os::MacOs.remove_file_command(std::path::Path::new("/Users/x/.claude/.credentials.json")),
        "rm '/Users/x/.claude/.credentials.json'"
    );
    assert_eq!(
        Os::Linux.remove_file_command(std::path::Path::new(
            "/home/x/Application Support/it's/.credentials.json"
        )),
        r"rm '/home/x/Application Support/it'\''s/.credentials.json'"
    );
}

#[test]
fn a_login_left_behind_the_keychain_is_found_in_its_file() {
    use crate::host::memory::MemoryHost;
    use crate::store::memory::Fault;

    let host = MemoryHost::new();
    let ctx = Context::for_unit_test().with_memory_stores(Arc::clone(&host));
    let service = claude_paths::live_service(&ctx);
    let file = host.file_at(claude_live::credential_file(&ctx));
    let login = |token: &str| json!({"claudeAiOauth": {"refreshToken": token}}).to_string();
    let fallback_login = || found(&ctx, &service).fallback;

    host.live().plant(&service, &login("in use"));
    assert!(fallback_login().is_none());

    file.plant(&service, &login("left"));
    let found = fallback_login().expect("a login behind the keychain's");
    assert_eq!(found.path, claude_live::credential_file(&ctx));
    assert_eq!(
        found.fingerprint.expect("readable"),
        Some(store::fingerprint("left"))
    );

    file.plant(&service, &json!({"mcpOAuth": {}}).to_string());
    let found = fallback_login().expect("what `/logout` leaves is still there");
    assert_eq!(
        found.fingerprint.expect("readable"),
        None,
        "and holds no login"
    );

    file.plant(&service, &login("left"));
    file.fault(
        &service,
        Fault::UnreadableContents("permission denied".into()),
    );
    let found = fallback_login().expect("what cannot be read is still there");
    assert!(
        matches!(found.fingerprint, Err(store::Error::Unreadable(_))),
        "and what it holds is not guessed"
    );
    file.heal(&service);

    host.live().delete_everything();
    assert!(
        fallback_login().is_none(),
        "the file is the login in use when the keychain holds none"
    );
}

// One that gives no date is no warning.
#[test]
fn the_login_in_use_is_judged_by_when_its_refresh_token_expires_as_a_park_is() {
    let credential = |f: &mut Facts, expires: Option<i64>| {
        let mut oauth = json!({"refreshToken": "r"});
        if let Some(at) = expires {
            oauth["refreshTokenExpiresAt"] = json!(at * 1000);
        }
        f.credential = Ok(Some(json!({ "claudeAiOauth": oauth })));
        let checks = evaluate(f);
        let said = check(&checks, "credential");
        (said.level, said.detail.clone(), said.advice.clone())
    };
    let mut f = facts();
    let now = f.now;
    let keys = ["claudeAiOauth"];
    let fingerprint = store::fingerprint("r");
    let with_life = |at: i64| {
        words::refresh_life_and_keys(&fingerprint, &said::parked_life_column(at, now), &keys)
    };
    let at = now + RENEW_WITHIN;
    assert_eq!(
        credential(&mut f, Some(at)),
        (Level::Ok, with_life(at), String::new())
    );
    let at = now + RENEW_WITHIN - 1;
    assert_eq!(
        credential(&mut f, Some(at)),
        (Level::Warn, with_life(at), words::EXPIRES_SOON.into())
    );
    let at = now - 60;
    assert_eq!(
        credential(&mut f, Some(at)),
        (Level::Warn, with_life(at), words::EXPIRED.into())
    );
    assert_eq!(
        credential(&mut f, None),
        (
            Level::Ok,
            words::refresh_and_keys(&fingerprint, &keys),
            String::new()
        )
    );
}

#[test]
fn the_successor_backend_alone_does_not_move_the_login() {
    let server_on = || {
        let mut f = facts();
        f.config = Ok(json!({"cachedGrowthBookFeatures": {"tengu_hover_rest": true}}));
        f
    };
    let env_on = || {
        let mut f = facts();
        f.hover_rest_env = true;
        f
    };
    for mut f in [server_on(), env_on()] {
        // On the keychain, the flag changes nothing Pitboard reads.
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
        // In the fallback, it is the one case worth saying something about.
        f.backend = Ok(store::Backend::File);
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "storage_v5").level, Level::Warn);
    }
}

#[test]
fn the_successor_backend_is_quiet_when_it_is_off() {
    let mut f = facts();
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
    // The fallback store on its own is not the successor backend.
    f.backend = Ok(store::Backend::File);
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
}

// On macOS the file is a fallback after a keychain write failed.
#[test]
fn a_file_store_is_the_whole_answer_on_linux_and_windows() {
    let mut f = facts();
    f.backend = Ok(store::Backend::File);
    for (os, level, advice) in [
        (Os::Linux, Level::Ok, ""),
        (Os::MacOs, Level::Warn, words::FELL_BACK_TO_A_FILE),
        (Os::Windows, Level::Ok, ""),
    ] {
        f.os = os;
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(
            (store.level, store.advice.as_str()),
            (level, advice),
            "{os:?}"
        );
        assert_eq!(
            store.detail,
            words::plaintext_file(f.credential_file.display()),
            "{os:?}"
        );
    }
}

// A mismatch, not an absence: what would follow Claude Code moving where it keeps a login.
#[test]
fn a_config_that_names_somebody_signed_in_with_no_login_anywhere_is_a_failure() {
    let mut f = facts();
    f.backend = Ok(store::Backend::Absent);
    let checks = evaluate(&f);
    let store = check(&checks, "credential_store");
    assert_eq!(store.level, Level::Fail);
    assert!(store.detail.contains("a@b.c"));

    // Nobody signed in at all is an ordinary state with an ordinary answer.
    f.config_account = None;
    let checks = evaluate(&f);
    let store = check(&checks, "credential_store");
    assert_eq!(store.level, Level::Warn);
    assert_eq!(store.advice, words::NOTHING_SIGNED_IN);
}

// Enterprise persistence (`credman_attributes`): the login roams with a roaming profile.
#[test]
fn credential_manager_is_named_by_its_item_and_a_roaming_profile_is_warned_of() {
    let mut f = facts();
    f.os = Os::Windows;
    f.backend = Ok(store::Backend::CredentialManager);
    f.account = "claude-code-user".into();
    f.windows = Some(WindowsHomes {
        home: PathBuf::from(r"C:\Users\dana"),
        default_pitboard_home: PathBuf::from(r"C:\Users\dana\AppData\Local\Pitboard"),
        local_app_data: None,
        profile: None,
        codex_home_set: false,
        roams: false,
    });
    let checks = evaluate(&f);
    let store = check(&checks, "credential_store");
    assert_eq!(store.level, Level::Ok);
    assert_eq!(
        store.detail,
        words::in_credential_manager(&f.service, "claude-code-user")
    );
    if let Some(homes) = f.windows.as_mut() {
        homes.roams = true;
    }
    let checks = evaluate(&f);
    let store = check(&checks, "credential_store");
    assert_eq!(store.level, Level::Warn);
    assert_eq!(store.advice, words::ROAMS_WITH_THE_PROFILE);
}

#[test]
fn a_login_in_credential_manager_is_counted_in_pieces_and_257_fail() {
    let mut f = facts();
    f.os = Os::Windows;
    f.backend = Ok(store::Backend::CredentialManager);
    let size = |f: &Facts, needs: usize| {
        let mut f_cost = f.credential_cost.expect("a cost");
        f_cost.needs = needs;
        judge_pieces(f_cost, &f.credential_parts)
    };
    f.credential_cost = Some(store::Cost {
        needs: 900,
        limit: 460_800,
        second_route: false,
    });
    assert_eq!(
        check(&evaluate(&f), "credential_size").detail,
        words::in_one_item(900)
    );
    let pieces = size(&f, 2401);
    assert_eq!(pieces.level, Level::Ok);
    assert_eq!(
        pieces.detail,
        words::in_pieces(2401, 2, crate::provider::claude::pieces::PIECE)
    );
    let past = size(&f, 460_801);
    assert_eq!(past.level, Level::Fail);
    assert_eq!(past.detail, said::past_credential_manager(460_801, 460_800));
}

#[test]
fn pitboard_no_argv_is_said_to_change_nothing_on_windows() {
    let mut f = facts();
    f.no_argv = true;
    f.os = Os::MacOs;
    assert!(evaluate(&f).iter().all(|c| c.code != "no_argv"));
    f.os = Os::Windows;
    let checks = evaluate(&f);
    let said = check(&checks, "no_argv");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(said.detail, words::NO_ARGV_CHANGES_NOTHING);
}

// A network logon, as over sshd with a key, has no credential set: 1312 (MC-net).
#[test]
fn credential_manager_out_of_this_logons_reach_is_said_in_its_own_words() {
    let mut f = facts();
    f.os = Os::Windows;
    f.credential_file = PathBuf::from(r"C:\Users\dana\.claude\.credentials.json");
    f.credential = Err(store::Error::Locked);
    let checks = evaluate(&f);
    let said = check(&checks, "credential");
    assert_eq!(said.level, Level::Fail);
    assert_eq!(said.detail, words::CREDENTIAL_MANAGER_OUT_OF_REACH);
    assert_eq!(
        said.advice,
        words::from_a_desktop_session(f.credential_file.display())
    );
}

#[test]
fn a_login_credential_manager_holds_that_nothing_turns_on_is_said_to_sign_sessions_out() {
    let mut f = facts();
    f.os = Os::Windows;
    f.backend = Err(store::Error::Unreadable(
        "Credential Manager holds a login".into(),
    ));
    let checks = evaluate(&f);
    let said = check(&checks, "credential_store");
    assert_eq!(said.level, Level::Fail);
    assert_eq!(said.advice, words::UNKNOWN_AND_SIGNED_OUT);
}

#[test]
fn a_login_past_the_ceiling_reads_differently_when_the_way_past_it_is_refused() {
    let mut f = facts();
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "credential_size").level, Level::Ok);

    f.credential_cost = Some(store::Cost {
        needs: 8503,
        limit: 4032,
        second_route: true,
    });
    let checks = evaluate(&f);
    let written = check(&checks, "credential_size");
    assert_eq!(written.level, Level::Warn);
    assert_eq!(
        written.detail,
        words::size_on_the_argument_line(8503, 4032, "")
    );

    f.credential_cost = Some(store::Cost {
        needs: 8503,
        limit: 4032,
        second_route: false,
    });
    let checks = evaluate(&f);
    let refused = check(&checks, "credential_size");
    assert_eq!(
        refused.level,
        Level::Fail,
        "there is no third way to write it"
    );
    assert_eq!(refused.detail, words::size_refused(8503, 4032, ""));
}

// It said "8503 of 4032 bytes" and left the person to guess what to sign out of.
#[test]
fn a_login_too_big_says_what_is_taking_up_the_room() {
    let mut f = facts();
    f.credential_cost = Some(store::Cost {
        needs: 8503,
        limit: 4032,
        second_route: true,
    });
    f.credential_parts = parts_of(&json!({
        "claudeAiOauth": {"refreshToken": "r"},
        "mcpOAuth": {
            "one": {"token": "x".repeat(300)},
            "two": {"token": "y".repeat(200)},
            "three": {"token": "z".repeat(100)},
        },
    }));

    let checks = evaluate(&f);
    let size = check(&checks, "credential_size");
    assert!(size.detail.contains("mcpOAuth one"), "{}", size.detail);
    assert!(size.detail.contains("mcpOAuth two"), "{}", size.detail);
    assert!(
        !size.detail.contains("claudeAiOauth"),
        "three is enough to act on, and the login itself is never the problem: {}",
        size.detail
    );
}

#[test]
fn what_takes_up_the_room_is_listed_largest_first_and_named_per_server() {
    let parts = parts_of(&json!({
        "claudeAiOauth": {"refreshToken": "r"},
        "mcpOAuth": {"small": {"t": "x"}, "large": {"t": "y".repeat(500)}},
    }));
    let names: Vec<&str> = parts.iter().map(|(what, _)| what.as_str()).collect();
    assert_eq!(names[0], "mcpOAuth large", "largest first: {names:?}");
    assert!(names.contains(&"claudeAiOauth"));
    assert!(
        names.contains(&"mcpOAuth small"),
        "each server is named, because that is what a person signs out of"
    );

    // One server is not worth breaking apart; the key says it already.
    let single = parts_of(&json!({"mcpOAuth": {"only": {"t": "x"}}}));
    assert_eq!(single.len(), 1);
    assert_eq!(single[0].0, "mcpOAuth");
}

#[test]
fn the_daemon_is_reported_without_being_a_problem() {
    let mut f = facts();
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "claude_daemon").detail, words::NO_DAEMON);

    f.daemon = Some(daemon::Daemon {
        pid: 4321,
        version: Some("2.1.278".into()),
        running: true,
    });
    let checks = evaluate(&f);
    let running = check(&checks, "claude_daemon");
    assert_eq!(running.level, Level::Ok);
    let version = words::daemon_version(Some("2.1.278"));
    assert_eq!(running.detail, words::daemon_running(&version, 4321));

    f.daemon.as_mut().expect("set above").running = false;
    let checks = evaluate(&f);
    let stopped = check(&checks, "claude_daemon");
    assert_eq!(stopped.level, Level::Ok);
    assert_eq!(stopped.detail, words::daemon_stopped(&version));
}

#[test]
fn the_account_in_use_is_the_one_anthropic_named() {
    use crate::switch::harness::{NOW, config_names, machine};
    let m = machine("doctor-in-use");
    let at = time::moment(NOW, NOW);

    let facts = gather(&m.ctx);
    let checks = evaluate(&facts);
    let said = check(&checks, "in_use");
    assert_eq!(said.level, Level::Ok);
    assert_eq!(said.detail, words::as_anthropic_said("`here`", &at));
    assert_eq!(in_use(&facts), ["here"]);

    config_names(&m, "there");
    crate::service::Pitboard::new(m.ctx.clone())
        .status(false)
        .expect("a read");
    let facts = gather(&m.ctx);
    let checks = evaluate(&facts);
    let said = check(&checks, "in_use");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(
        said.detail,
        words::config_names_another("`here`", &at, "`there`")
    );
    assert_eq!(
        said.advice,
        words::status_shows("`there`", &Stored::Account("here".into()).how_to_name())
    );
    assert_eq!(in_use(&facts), ["here"]);
}

// Claude Code renews its login every few hours, and doctor asks nobody.
#[test]
fn a_renewed_login_is_not_a_warning() {
    use crate::switch::harness::{NOW, document, machine};
    let m = machine("doctor-renewed");
    m.sign_in(&document("here-renewed"));

    let checks = evaluate(&gather(&m.ctx));
    let said = check(&checks, "in_use");

    assert_eq!(said.level, Level::Ok);
    assert_eq!(
        said.detail,
        words::renewed_since("`here`", &time::moment(NOW, NOW))
    );
    assert_eq!(m.api.calls(), 0);
}

// As a sign-in leaves it, and another session starting on another login can.
#[test]
fn a_config_that_moved_since_is_a_warning() {
    use crate::switch::harness::{NOW, config_names, machine};
    let m = machine("doctor-config-moved");
    config_names(&m, "there");

    let facts = gather(&m.ctx);
    let checks = evaluate(&facts);
    let said = check(&checks, "in_use");

    assert_eq!(said.level, Level::Warn);
    assert_eq!(
        said.detail,
        words::config_named_since(
            "`there`",
            &words::anthropic_named("`here`", &time::moment(NOW, NOW))
        )
    );
    assert_eq!(said.advice, words::ASK_WITH_STATUS);
    assert_eq!(in_use(&facts), ["here"]);
    assert_eq!(m.api.calls(), 0);
}

// Claude Code 2.1.309 refuses those logins as a session starts (`organisation_pin`).
#[test]
fn a_managed_organisation_pin_is_named_with_the_accounts_it_rules_out() {
    use crate::switch::harness::account;
    let host = crate::host::memory::MemoryHost::new();
    let ctx = Context::for_unit_test().with_memory_stores(host.clone());
    let mut state = State::default();
    state.accounts.push(account("here", "here", None));
    state.accounts.push(account("there", "there", None));
    let judged = |f: &Facts| evaluate(f);
    let mut f = facts();
    (f.login_pins, f.pinned_out) = login_pins(&ctx, Some(&state));
    assert!(judged(&f).iter().all(|c| c.code != "login_restrictions"));

    let file = crate::settings::managed_dir(ctx.os).join("managed-settings.json");
    host.administers(&file, r#"{"forceLoginOrgUUID": ["org-here"]}"#);
    (f.login_pins, f.pinned_out) = login_pins(&ctx, Some(&state));
    let checks = judged(&f);
    let found = check(&checks, "login_restrictions");
    assert_eq!(found.level, Level::Warn);
    assert_eq!(found.name, "login restrictions");
    assert_eq!(
        found.detail,
        words::org_pinned("`org-here`", file.display())
    );
    assert_eq!(found.advice, words::refused_as_a_session_starts("`there`"));

    host.administers(&file, r#"{"forceLoginOrgUUID": ["org-here", "org-there"]}"#);
    (f.login_pins, f.pinned_out) = login_pins(&ctx, Some(&state));
    assert_eq!(check(&judged(&f), "login_restrictions").level, Level::Ok);
}

// Under it Claude Code 2.1.310 signs in to Console (`login_method_pin`): a sign-in is refused,
// and a switch goes ahead.
#[test]
fn a_managed_console_pin_is_named_beside_an_organisation_pin() {
    let host = crate::host::memory::MemoryHost::new();
    let ctx = Context::for_unit_test().with_memory_stores(host.clone());
    let file = crate::settings::managed_dir(ctx.os).join("managed-settings.json");
    let mut f = facts();
    host.administers(&file, r#"{"forceLoginMethod": "claudeai"}"#);
    (f.login_pins, f.pinned_out) = login_pins(&ctx, None);
    assert!(evaluate(&f).iter().all(|c| c.code != "login_restrictions"));

    host.administers(&file, r#"{"forceLoginMethod": "console"}"#);
    (f.login_pins, f.pinned_out) = login_pins(&ctx, None);
    let checks = evaluate(&f);
    let found = check(&checks, "login_restrictions");
    assert_eq!(found.level, Level::Ok);
    assert_eq!(found.detail, words::console_pinned(file.display()));

    host.administers(
        &file,
        r#"{"forceLoginMethod": "console", "forceLoginOrgUUID": "org-here"}"#,
    );
    (f.login_pins, f.pinned_out) = login_pins(&ctx, None);
    assert_eq!(
        check(&evaluate(&f), "login_restrictions").detail,
        format!(
            "{}; {}",
            words::org_pinned("`org-here`", file.display()),
            words::console_pinned(file.display())
        )
    );
}

// Add account said Claude Code was not installed, and This PC said to run it.
#[test]
fn a_machine_without_claude_code_is_told_to_install_it() {
    let mut facts = facts();
    facts.claude_program = None;
    facts.config = Err(Error::ClaudeConfigMissing {
        path: PathBuf::from("/home/x/.claude.json"),
        spelling: said::Spelling::Bare,
    });
    facts.config_account = None;
    let checks = evaluate(&facts);
    let said = |code: &str| {
        let check = checks.iter().find(|c| c.code == code).expect(code);
        (check.detail.clone(), check.advice.clone())
    };
    assert_eq!(
        said("config_file"),
        (
            words::not_installed_and_no_config("/home/x/.claude.json"),
            words::install_then_run_once("claude")
        )
    );
    assert_eq!(
        said("identity").1,
        words::written_after_the_first_call(&words::install_then_run_once("claude"))
    );
}

// Windows PowerShell 5.1 refuses the `claude.ps1` beside npm's `claude.cmd`.
#[test]
fn a_machine_where_claude_code_never_ran_is_told_to_run_it() {
    for (os, found, spelling) in [
        (Os::MacOs, "/opt/homebrew/bin/claude", said::Spelling::Bare),
        (
            Os::Windows,
            "C:/Users/x/.local/bin/claude.exe",
            said::Spelling::Bare,
        ),
        (
            Os::Windows,
            "C:/Users/x/AppData/Roaming/npm/claude.cmd",
            said::Spelling::Cmd,
        ),
    ] {
        let program = spelling.of(ProviderId::Claude);
        let mut facts = facts();
        facts.os = os;
        facts.claude_program = Some(PathBuf::from(found));
        let missing = Error::ClaudeConfigMissing {
            path: PathBuf::from("/home/x/.claude.json"),
            spelling,
        };
        let detail = missing.to_string();
        facts.config = Err(missing);
        facts.config_account = None;
        let checks = evaluate(&facts);
        let said = |code: &str| {
            let check = checks.iter().find(|c| c.code == code).expect(code);
            (check.detail.clone(), check.advice.clone())
        };
        assert_eq!(
            said("config_file"),
            (detail, words::run_once(program)),
            "{found}"
        );
        assert_eq!(
            said("identity").1,
            words::written_after_the_first_call(&words::run_once(program))
        );
    }
}

// The session's fresh PC read "1 broken" for this alone, with nothing enrolled.
#[test]
fn claude_code_that_never_ran_breaks_nothing() {
    let mut facts = facts();
    facts.config = Err(Error::ClaudeConfigMissing {
        path: PathBuf::from("/home/x/.claude.json"),
        spelling: said::Spelling::Bare,
    });
    facts.config_account = None;
    facts.in_use = None;
    facts.credential = Ok(None);
    for installed in [true, false] {
        facts.claude_program = installed.then(|| PathBuf::from("/opt/homebrew/bin/claude"));
        let checks = evaluate(&facts);
        assert_eq!(
            check(&checks, "config_file").level,
            Level::Warn,
            "{installed}"
        );
        assert!(
            checks.iter().all(|c| c.level != Level::Fail),
            "{installed}: {:?}",
            checks
                .iter()
                .filter(|c| c.level == Level::Fail)
                .map(|c| c.code)
                .collect::<Vec<_>>()
        );
    }
}

// What Claude Code 2.1.302 left on the session's PC (2026-10-18) moving into Credential Manager.
#[test]
fn pieces_nobody_can_join_are_said_to_be_unreadable_to_claude_code_too() {
    for (found, spelling) in [
        ("C:/Users/x/.local/bin/claude.exe", said::Spelling::Bare),
        (
            "C:/Users/x/AppData/Roaming/npm/claude.cmd",
            said::Spelling::Cmd,
        ),
    ] {
        let program = spelling.of(ProviderId::Claude);
        let mut facts = facts();
        facts.os = Os::Windows;
        facts.claude_program = Some(PathBuf::from(found));
        facts.backend = Ok(store::Backend::CredentialManager);
        let why = "`Claude Code-credentials/claude-code-user` lacks piece #2 of the 3 its \
                   manifest names";
        facts.credential = Err(store::Error::Broken {
            why: why.into(),
            spelling,
        });
        let check = judge_credential(&facts);
        assert_eq!(check.level, Level::Fail);
        assert_eq!(check.detail, words::unreadable_to_claude_code_too(why));
        assert_eq!(check.advice, words::log_out_to_delete_it(program));
    }
}

// What `/logout` leaves is nobody signed in, not a login whose shape moved.
#[test]
fn a_signed_out_credential_is_said_to_be_one() {
    let mut facts = facts();
    facts.credential = Ok(Some(serde_json::json!({"mcpOAuth": {"server": {}}})));
    let check = judge_credential(&facts);
    assert!(matches!(check.level, Level::Warn), "{}", check.detail);
    assert_eq!(check.detail, words::signed_out_keeps(&["mcpOAuth"]));
}

// A switch refuses it as a shape it does not know, so it is no signed-out document.
#[test]
fn a_credential_that_is_not_an_object_is_a_failure() {
    use crate::provider::ProviderError;
    let document = serde_json::json!(["claudeAiOauth"]);
    assert!(matches!(
        crate::provider::claude_code().slice(&document),
        Err(ProviderError::ShapeUnexpected { .. })
    ));
    let mut facts = facts();
    facts.credential = Ok(Some(document));
    let judged = judge_credential(&facts);
    assert_eq!(judged.level, Level::Fail);
    assert_eq!(judged.detail, words::NOT_A_JSON_OBJECT);
}

// Which login it is cannot be told, which is not that none is stored.
#[test]
fn a_stored_login_that_is_not_an_object_leaves_the_account_in_use_as_named() {
    use crate::switch::harness::{NOW, machine};
    let m = machine("doctor-not-an-object");
    m.sign_in(&serde_json::json!(["claudeAiOauth"]));
    let checks = evaluate(&gather(&m.ctx));
    assert_eq!(
        check(&checks, "in_use").detail,
        words::as_anthropic_said("`here`", &time::moment(NOW, NOW))
    );
}

// On macOS each look and each read is a `security` call, and a renewal between two makes the
// facts disagree.
#[test]
fn claude_codes_login_is_looked_for_and_read_once() {
    let m = switch::harness::machine("doctor-one-read");
    m.mem.live().takes_on_stdin(4032);
    let before = m.mem.trail().len();
    let facts = gather(&m.ctx);
    let calls = |call: &str| {
        m.mem.trail()[before..]
            .iter()
            .filter(|touch| touch.store == "live" && touch.service == m.service)
            .filter(|touch| touch.call == call)
            .count()
    };
    // A Credential Manager look is a read of the item.
    let (backend, looks_and_reads) = match crate::host::OS {
        Os::Windows => (store::Backend::CredentialManager, (0, 2)),
        Os::MacOs | Os::Linux => (store::Backend::Keychain, (1, 1)),
    };
    assert_eq!((calls("contains"), calls("read")), looks_and_reads);
    assert!(matches!(facts.backend, Ok(held) if held == backend));
    assert!(matches!(facts.credential, Ok(Some(_))));
    assert!(facts.credential_cost.is_some());
    assert!(!facts.credential_parts.is_empty());
    assert!(facts.claude_present);
}
