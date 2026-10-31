use super::*;

#[test]
fn section_names_the_tool_whose_checks_these_are() {
    for code in [
        "claude_version",
        "claude_daemon",
        "credential",
        "state",
        "schedule_proxy",
        "system_too_old",
        "desktop",
        "codexy",
    ] {
        assert_eq!(section(code), None, "{code}");
    }
    for code in ["codex_backend", "codex_login", "codex_parked_login"] {
        assert_eq!(section(code), Some(ProviderId::Codex), "{code}");
    }
    let mut state = State::default();
    state
        .accounts
        .push(desktop_account("work", Some(folder_park("p", None))));
    let checks = judge_desktop(Os::MacOs, &desktop(), &state, &[], NOW, &time::moment);
    assert!(!checks.is_empty());
    for check in checks {
        assert_eq!(
            section(check.code),
            Some(ProviderId::Desktop),
            "{}",
            check.code
        );
    }
}

#[test]
fn every_check_is_pitboards_own_or_one_tools() {
    for code in [
        "state",
        "schedule",
        "network",
        "home",
        "notification_not_posted",
    ] {
        assert_eq!(tool_of(code), None, "{code}");
    }
    for code in CLAUDE_CODES_OWN
        .iter()
        .chain(&["parked_login", "dormant_account"])
    {
        assert_eq!(tool_of(code), Some(ProviderId::Claude), "{code}");
    }
    assert_eq!(tool_of("codex_parked_login"), Some(ProviderId::Codex));
    assert_eq!(tool_of("desktop_move"), Some(ProviderId::Desktop));
}

#[test]
fn a_healthy_machine_reports_no_failures() {
    let checks = evaluate(&facts());
    assert!(checks.iter().all(|c| c.level != Level::Fail));
    assert!(healthy(&checks));
}

// The command line keeps what doctor prints for it.
#[test]
fn the_example_mac_has_nothing_amiss() {
    let checks = example::healthy_mac();
    assert!(checks.iter().all(|c| c.level == Level::Ok), "{checks:#?}");
}

#[test]
fn every_failure_and_warning_tells_the_user_something() {
    let mut f = facts();
    f.credential = Ok(Some(json!({"slackTag": {}})));
    f.home_access = Some(mode(0o755));
    for c in evaluate(&f) {
        if c.level != Level::Ok {
            assert!(!c.advice.is_empty(), "{} has no advice", c.code);
        }
    }
}

// A string broken across lines without its continuation keeps the indentation as spaces.
#[test]
fn advice_reads_as_it_was_written() {
    let mut f = facts();
    f.credential = Ok(Some(json!({"slackTag": {}})));
    f.home_access = Some(mode(0o755));
    f.readable_by_others = vec![(
        "/home/a/.claude/.credentials.json".into(),
        Kind::File,
        mode(0o644),
    )];
    for c in evaluate(&f) {
        assert!(!c.advice.contains("  "), "{}: {:?}", c.code, c.advice);
    }
}

#[test]
fn each_check_is_told_apart_by_code_and_name() {
    let mut f = facts();
    f.parks = vec![parked("work", None), parked("personal", None)];
    let checks = evaluate(&f);
    let mut keys: Vec<(&str, &str)> = checks.iter().map(|c| (c.code, c.name.as_str())).collect();
    keys.sort_unstable();
    let before = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), before);
}

// The app says a moment on the person's own clock.
#[test]
fn each_moment_a_check_names_is_written_as_the_front_end_writes_it() {
    use crate::switch::harness::{NOW, machine};
    let m = machine("doctor-in-use-moment");
    let diagnosis = crate::service::Pitboard::new(m.ctx.clone())
        .doctor_with(&|at, now| format!("at {at} of {now}"));

    assert_eq!(
        check(&diagnosis.checks, "in_use").detail,
        words::as_anthropic_said("`here`", &format!("at {NOW} of {NOW}"))
    );
}

// Windows' facts are read from builds of their own, which its version lines name.
#[test]
fn a_build_is_set_beside_the_one_read_for_this_system() {
    use crate::assumptions::{Platform, verified_against};
    for os in [Os::MacOs, Os::Linux, Os::Windows] {
        let claude = verified_against(ProviderId::Claude, Platform::of(os));
        let mut f = facts();
        f.os = os;
        f.claude_version = None;
        assert_eq!(
            check(&evaluate(&f), "claude_version").detail,
            words::build_not_found(claude),
            "{os:?}"
        );
        f.claude_version = Some(claude.into());
        assert_eq!(
            check(&evaluate(&f), "claude_version").detail,
            words::build_as_read(claude),
            "{os:?}"
        );

        let codex = verified_against(ProviderId::Codex, Platform::of(os));
        let installed = CodexFacts {
            version: Some(codex.into()),
            ..with_codex()
        };
        let checks = judge_codex(os, None, &installed, &[], NOW, LOGIN, &time::moment);
        assert_eq!(
            check(&checks, "codex_version").detail,
            words::build_as_read(codex),
            "{os:?}"
        );
    }
}

#[test]
fn no_system_is_told_a_warm_up_is_not_available() {
    let mut facts = facts();
    for os in [Os::MacOs, Os::Linux, Os::Windows] {
        facts.os = os;
        assert!(
            evaluate(&facts).iter().all(|c| c.code != "warm_up"),
            "{os:?}"
        );
    }
}

#[test]
fn a_machine_with_only_codex_is_not_judged_on_claude_code() {
    let mut windows = facts();
    windows.os = Os::Windows;
    windows.fallback_login = Some(FallbackLogin {
        path: windows.credential_file.clone(),
        store: SecretStore::CredentialManager,
        fingerprint: Ok(None),
    });
    windows.status_line = Some(StatusLineFact {
        git_bash: claude_paths::GitBash {
            found: None,
            passed_over: None,
        },
        powershell: None,
        pitboard: None,
        runs_pitboard: false,
    });
    windows.login_pins = vec!["`forceLoginOrgUUID` set to `org` there".into()];
    let made: Vec<&str> = evaluate(&windows).iter().map(|c| c.code).collect();
    // A name no check makes hides nothing, and the loop under it would pass over it.
    for own in CLAUDE_CODES_OWN {
        assert!(made.contains(own), "{own} is no check's code");
    }

    for mut facts in [facts(), windows] {
        facts.claude_present = false;
        facts.config = Err(Error::ClaudeConfigMissing {
            path: PathBuf::from("/nowhere/.claude.json"),
            spelling: said::Spelling::Bare,
        });
        facts.codex.present = true;
        let checks = evaluate(&facts);
        for own in CLAUDE_CODES_OWN {
            assert!(
                checks.iter().all(|c| c.code != *own),
                "{own} is about Claude Code, which is not here"
            );
        }
        assert!(
            checks.iter().any(|c| c.code == "state"),
            "Pitboard's own still is"
        );
        assert!(checks.iter().any(|c| c.code.starts_with("codex_")));
    }
}

#[test]
fn a_home_that_is_not_a_full_path_still_hears_why_nothing_changes() {
    use crate::context::Environment;
    let env: Environment = [("HOME", "")].into_iter().collect();
    let host = crate::host::memory::MemoryHost::new();
    host.runs_on(crate::host::Floor::Below { build: 22631 });
    host.runs_with(crate::host::Elevation::Unknown);
    let ctx = Context::for_command_line(&env).with_memory_stores(host);
    let codes: Vec<&str> = run(&ctx, &time::moment)
        .checks
        .iter()
        .map(|c| c.code)
        .collect();
    assert_eq!(codes, ["system_too_old", "elevated", "homes"]);
}

// With neither tool present, a new machine is told what to do first, as it always was.
#[test]
fn a_machine_with_neither_tool_still_hears_about_claude_code() {
    let mut facts = facts();
    facts.claude_present = false;
    let checks = evaluate(&facts);
    assert!(checks.iter().any(|c| c.code == "config_file"));
}

// It read Claude Code's and Pitboard's files under whatever folder it was run from.
#[test]
fn doctor_checks_nothing_under_a_home_that_is_not_a_full_path() {
    use crate::context::Environment;
    let variable = crate::host::OS.home_variable();
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let full = scratch.path().to_string_lossy().into_owned();
    let empty = words::not_a_full_path(variable, std::path::Path::new(""));
    let relative = words::not_a_full_path("CODEX_HOME", std::path::Path::new("codex"));
    for (pairs, said) in [
        (&[(variable, "")][..], empty.as_str()),
        (
            &[(variable, full.as_str()), ("CODEX_HOME", "codex")],
            relative.as_str(),
        ),
    ] {
        let env: Environment = pairs.iter().copied().collect();
        let ctx = Context::for_command_line(&env)
            .with_memory_stores(crate::host::memory::MemoryHost::new());
        let diagnosis = run(&ctx, &time::moment);
        let checks: Vec<(&str, Level, &str)> = diagnosis
            .checks
            .iter()
            .map(|c| (c.code, c.level, c.detail.as_str()))
            .collect();
        assert_eq!(checks, [("homes", Level::Fail, said)], "{pairs:?}");
        assert_eq!(diagnosis.checks[0].advice, words::SET_A_FULL_PATH);
        assert_eq!(diagnosis.environment, json!({}));
    }
}

#[test]
fn no_check_said_on_linux_names_a_keychain_a_menu_bar_or_activity_monitor() {
    let named: Vec<String> = evaluate(&everything_said(Os::Linux))
        .into_iter()
        .flat_map(|check| {
            [check.name, check.detail, check.advice].map(|said| format!("{}: {said}", check.code))
        })
        .filter(|said| crate::words::names_what_linux_has_not(said))
        .collect();
    assert!(named.is_empty(), "{named:#?}");
}

// The command line heads each tool's section where its codes start.
#[test]
fn the_checks_come_in_the_order_each_system_prints_them() {
    let codes = |os| -> Vec<&'static str> {
        evaluate(&everything_said(os))
            .iter()
            .map(|c| c.code)
            .collect()
    };
    assert_eq!(
        codes(Os::MacOs),
        [
            "system_too_old",
            "elevated",
            "security_tool",
            "config_file",
            "identity",
            "in_use",
            "slot",
            "credential_size",
            "credential_store",
            "credential",
            "fallback_login",
            "home",
            "private_on_disk",
            "state",
            "parked_login",
            "interrupted_switch",
            "discarded",
            "machine_id",
            "storage_v5",
            "claude_daemon",
            "pending_parks",
            "schedule",
            "schedule_proxy",
            "watch_service",
            "claude_version",
            "auth_source",
            "login_restrictions",
            "asking",
            "network",
            "dormant_account",
            "codex_backend",
            "codex_login_restrictions",
            "codex_auth_file",
            "codex_login",
            "codex_parked_login",
            "codex_dormant_account",
            "codex_version",
            "codex_running",
            "desktop_data_folder",
            "desktop_settings",
            "desktop_layout",
            "desktop_login",
            "desktop_parked_login",
            "desktop_parked_login",
            "desktop_move",
            "desktop_awaiting",
            "desktop_parks_folder",
            "desktop_set_aside",
            "desktop_build",
        ]
    );
    assert_eq!(
        codes(Os::Windows),
        [
            "system_too_old",
            "elevated",
            "config_file",
            "identity",
            "in_use",
            "slot",
            "credential_size",
            "no_argv",
            "credential_store",
            "credential",
            "fallback_login",
            "home",
            "private_on_disk",
            "vault",
            "state",
            "parked_login",
            "interrupted_switch",
            "discarded",
            "machine_id",
            "local_app_data",
            "roaming_profile",
            "wsl",
            "pitboard_copies",
            "system_proxy",
            "storage_v5",
            "claude_daemon",
            "pending_parks",
            "schedule",
            "schedule_proxy",
            "watch_service",
            "claude_version",
            "status_line",
            "auth_source",
            "login_restrictions",
            "asking",
            "network",
            "dormant_account",
            "codex_installer_home",
            "codex_backend",
            "codex_login_restrictions",
            "codex_auth_file",
            "codex_login",
            "codex_credential_manager",
            "codex_parked_login",
            "codex_dormant_account",
            "codex_version",
            "codex_running",
            "desktop_data_folder",
        ]
    );
    assert_eq!(
        codes(Os::Linux),
        [
            "system_too_old",
            "elevated",
            "config_file",
            "identity",
            "in_use",
            "slot",
            "credential_store",
            "credential",
            "home",
            "private_on_disk",
            "vault",
            "state",
            "parked_login",
            "interrupted_switch",
            "discarded",
            "machine_id",
            "wsl",
            "storage_v5",
            "claude_daemon",
            "pending_parks",
            "schedule",
            "schedule_proxy",
            "watch_service",
            "claude_version",
            "auth_source",
            "login_restrictions",
            "asking",
            "network",
            "dormant_account",
            "codex_backend",
            "codex_login_restrictions",
            "codex_auth_file",
            "codex_login",
            "codex_parked_login",
            "codex_dormant_account",
            "codex_version",
            "codex_running",
        ]
    );
}
