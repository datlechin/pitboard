use super::*;
use crate::provider::codex::paths::Backend;

#[test]
fn codex_has_a_section_only_where_there_is_a_codex() {
    let without = evaluate(&facts());
    assert!(without.iter().all(|c| !c.code.starts_with("codex_")));

    let mut f = facts();
    f.codex = with_codex();
    let with = evaluate(&f);
    let claude = |checks: &[Check]| -> Vec<(String, String, String)> {
        checks
            .iter()
            .filter(|c| !c.code.starts_with("codex_"))
            .map(|c| (c.code.to_string(), c.detail.clone(), c.advice.clone()))
            .collect()
    };
    assert_eq!(claude(&without), claude(&with), "Claude Code's checks move");
    for code in [
        "codex_backend",
        "codex_auth_file",
        "codex_login",
        "codex_version",
        "codex_running",
    ] {
        assert_eq!(check(&with, code).level, Level::Ok, "{code}");
    }
    assert!(healthy(&with));
    assert_eq!(
        check(&with, "codex_login").detail,
        words::codex_login("w@example.com", "work-account", "0123456789abcdef")
    );

    // Accounts enrolled where Codex's home has gone still get the section.
    f.codex = CodexFacts {
        enrolled: 1,
        ..no_codex()
    };
    let checks = evaluate(&f);
    let file = check(&checks, "codex_auth_file");
    assert_eq!(file.level, Level::Warn);
    assert_eq!(file.advice, words::use_or_sign_in("codex"));

    // Windows PowerShell 5.1 refuses the `codex.ps1` beside npm's `codex.cmd`.
    f.os = Os::Windows;
    f.codex.program = Some(PathBuf::from("C:/Users/x/AppData/Roaming/npm/codex.cmd"));
    let checks = evaluate(&f);
    let file = check(&checks, "codex_auth_file");
    assert_eq!(file.advice, words::use_or_sign_in("codex.cmd"));
}

// The session's This PC listed none for a Codex installed with npm and never run.
#[test]
fn codex_installed_and_never_run_has_its_section() {
    let mut f = facts();
    f.codex = CodexFacts {
        program: Some(PathBuf::from(r"C:\Users\x\AppData\Roaming\npm\codex.cmd")),
        ..no_codex()
    };
    let checks = evaluate(&f);
    for code in ["codex_backend", "codex_auth_file", "codex_version"] {
        assert_eq!(check(&checks, code).level, Level::Ok, "{code}");
    }
    assert!(healthy(&checks));
}

// A choice, not a fault: it fails only for somebody with Codex accounts enrolled.
#[test]
fn each_codex_store_is_judged_for_what_pitboard_can_do_with_it() {
    let setting = |backend: Backend| {
        format!(
            "`cli_auth_credentials_store = \"{}\"` in /home/x/.codex/config.toml",
            backend.word()
        )
    };
    let remedy = "To use the file store, set `cli_auth_credentials_store = \"file\"` in \
                  /home/x/.codex/config.toml, then sign in again with `codex login`";
    let judged = |backend: Backend, enrolled: usize| {
        let codex = CodexFacts {
            backend,
            enrolled,
            backend_setting: setting(backend),
            backend_remedy: remedy.into(),
            ..with_codex()
        };
        codex_checks(&codex)
    };

    for backend in [
        Backend::Keyring,
        Backend::Either,
        Backend::Secrets,
        Backend::Ephemeral,
    ] {
        let store = backend.word();
        let described = match backend {
            Backend::Ephemeral => words::in_memory_only(&setting(backend)),
            _ => words::store_set_by(store, &setting(backend)),
        };
        let checks = judged(backend, 1);
        let found = check(&checks, "codex_backend");
        assert_eq!(found.level, Level::Fail, "{store}");
        assert_eq!(found.detail, described, "says which file set it");
        assert_eq!(
            found.advice,
            match backend {
                Backend::Ephemeral => words::nothing_at_rest(remedy),
                _ => words::no_account_to_switch_to(
                    crate::provider::codex::layers::file_store_only(crate::host::OS),
                    remedy
                ),
            },
            "says the line and where it goes"
        );
        assert!(!healthy(&checks), "{store}");
        assert!(
            checks.iter().all(|c| c.code != "codex_login"),
            "a store other than the file is not read, so there is nothing to say about \
             its login"
        );

        let checks = judged(backend, 0);
        let found = check(&checks, "codex_backend");
        assert_eq!(
            found.level,
            Level::Ok,
            "nothing Pitboard does is broken for somebody with no Codex accounts: {store}"
        );
        assert_eq!(found.detail, words::switched_only_in_the_file(&described));
    }
}

#[test]
fn the_file_store_says_what_chose_it_and_what_is_not_read() {
    let checks = codex_checks(&with_codex());
    let found = check(&checks, "codex_backend");
    assert_eq!(found.level, Level::Ok);
    assert_eq!(
        found.detail,
        words::file_store("Codex's default", "/home/x/.codex/auth.json")
    );
    assert_eq!(found.advice, words::PROJECTS_NOT_READ);
}

#[test]
fn a_codex_store_nobody_can_tell_is_broken() {
    let unknown = |enrolled| CodexFacts {
        backend: Backend::Unknown,
        backend_setting: "/etc/codex/config.toml is not TOML Codex can read, at line 3".into(),
        backend_remedy: "Codex 0.160.0 does not start until that is put right".into(),
        enrolled,
        ..with_codex()
    };
    for (enrolled, level) in [(1, Level::Fail), (0, Level::Warn)] {
        let checks = codex_checks(&unknown(enrolled));
        let found = check(&checks, "codex_backend");
        assert_eq!(found.level, level);
        let facts = unknown(enrolled);
        assert_eq!(
            found.detail,
            words::cannot_tell_the_store(&facts.backend_setting)
        );
        assert_eq!(found.advice, words::no_store_known(&facts.backend_remedy));
        assert!(checks.iter().all(|c| c.code != "codex_login"));
    }
}

// Codex 0.164.0 signs out of those logins (`codex_login_restrictions`).
#[test]
fn codexs_login_restrictions_are_named_with_the_accounts_they_rule_out() {
    use crate::switch::harness::codex_account;
    let host = crate::host::memory::MemoryHost::new();
    let ctx = Context::for_unit_test().with_memory_stores(host.clone());
    let mut state = State::default();
    state.accounts.push(codex_account("here", "here", None));
    state.accounts.push(codex_account("there", "there", None));
    let judged = || codex_checks(&codex_facts(&ctx, Some(&state)));
    assert!(
        judged()
            .iter()
            .all(|c| c.code != "codex_login_restrictions")
    );

    let config = crate::provider::codex::layers::system_file(&ctx, "config.toml");
    host.administers(config.clone(), "forced_chatgpt_workspace_id = \"here\"\n");
    let checks = judged();
    let found = check(&checks, "codex_login_restrictions");
    assert_eq!(found.level, Level::Warn);
    assert_eq!(
        found.detail,
        crate::provider::codex::paths::restrictions(&ctx)
            .said()
            .join("; ")
    );
    assert_eq!(found.advice, words::codex_rules_out("`codex/there`"));

    host.administers(
        config.clone(),
        "forced_chatgpt_workspace_id = [\"here\", \"there\"]\n",
    );
    assert_eq!(
        check(&judged(), "codex_login_restrictions").level,
        Level::Ok
    );

    host.administers(config, "forced_login_method = \"sso\"\n");
    let checks = judged();
    let found = check(&checks, "codex_login_restrictions");
    assert_eq!(found.level, Level::Fail, "Codex accounts are enrolled");
    assert_eq!(found.advice, words::RESTRICTIONS_UNREADABLE);
}

// On Windows a keyring store is the secret store by default.
#[test]
fn a_store_an_administrator_pinned_is_gathered_and_said() {
    let host = crate::host::memory::MemoryHost::new();
    let ctx = Context::for_unit_test().with_memory_stores(host.clone());
    let required = crate::provider::codex::layers::system_file(&ctx, "requirements.toml");
    host.administers(
        required.clone(),
        "cli_auth_credentials_store = \"keyring\"\n",
    );
    let codex = CodexFacts {
        enrolled: 1,
        ..codex_facts(&ctx, None)
    };
    let backend = match crate::host::OS {
        Os::MacOs | Os::Linux => Backend::Keyring,
        Os::Windows => Backend::Secrets,
    };
    let store = crate::provider::codex::paths::store(&ctx);
    assert_eq!(codex.backend, backend);
    assert_eq!(codex.backend_setting, store.setting());
    let checks = codex_checks(&codex);
    let found = check(&checks, "codex_backend");
    assert_eq!(found.level, Level::Fail);
    assert_eq!(
        found.detail,
        words::store_set_by(backend.word(), &store.setting())
    );
    assert_eq!(
        found.advice,
        words::no_account_to_switch_to(
            crate::provider::codex::layers::file_store_only(crate::host::OS),
            &codex.backend_remedy
        )
    );

    host.administers(required, "x = \n");
    let unknown = codex_facts(&ctx, None);
    let store = crate::provider::codex::paths::store(&ctx);
    assert_eq!(unknown.backend, Backend::Unknown);
    assert_eq!(Some(unknown.backend_setting), store.why_unknown());
    assert_eq!(Some(unknown.backend_remedy), store.while_unknown());
    assert!(matches!(unknown.login, Ok(None)), "nothing is read");
}

#[test]
fn a_store_codex_uses_only_when_told_to_is_said_in_each_systems_words() {
    let keyring = CodexFacts {
        backend: Backend::Keyring,
        backend_setting: "`cli_auth_credentials_store = \"keyring\"` in config.toml".into(),
        backend_remedy: "To use the file store, set it".into(),
        enrolled: 1,
        ..with_codex()
    };
    let advice = |os| {
        check(
            &judge_codex(os, None, &keyring, &[], NOW, &time::moment),
            "codex_backend",
        )
        .advice
        .clone()
    };
    for os in [Os::MacOs, Os::Linux, Os::Windows] {
        assert_eq!(
            advice(os),
            words::no_account_to_switch_to(
                crate::provider::codex::layers::file_store_only(os),
                "To use the file store, set it"
            ),
            "{os:?}"
        );
    }
}

// By name alone, so no item's hex or secret is said, and only while Codex keeps the file.
#[test]
fn codexs_items_in_credential_manager_are_a_note_counted_by_name() {
    let host = crate::host::memory::MemoryHost::new();
    let home = tempfile::tempdir().expect("a Codex home, which Windows' Codex needs");
    let ctx = Context::for_unit_test()
        .with_codex_home(home.path().to_string_lossy().into_owned())
        .with_memory_stores(host.clone());
    host.keeps_credentials(&[
        "CLI|0123456789abcdef.Codex Auth",
        "secrets|0123456789abcdef.codex",
        "secrets|0123456789abcdef|gateway-oauth.codex",
        "cli|0123456789abcdef.Codex MCP Credentials",
        "Claude Code-credentials",
        "GitHub - https://github.com",
    ]);
    let facts = codex_facts(&ctx, None);
    assert_eq!(facts.stored_elsewhere, (1, 2));
    let checks = codex_checks(&facts);
    let note = check(&checks, "codex_credential_manager");
    assert_eq!(note.level, Level::Ok);
    assert_eq!(
        note.detail,
        said::listed([words::items(1, "`cli|`"), words::items(2, "`secrets|`")])
    );
    assert_eq!(note.advice, words::CODEX_KEEPS_IT_THERE_TOO);
    assert!(!format!("{}{}", note.detail, note.advice).contains("0123456789abcdef"));

    std::fs::write(
        home.path().join("config.toml"),
        "cli_auth_credentials_store = \"ephemeral\"\n",
    )
    .expect("a config");
    assert_eq!(codex_facts(&ctx, None).stored_elsewhere, (0, 0));
    assert!(
        codex_checks(&with_codex())
            .iter()
            .all(|c| c.code != "codex_credential_manager")
    );
}

// Windows' alone: Codex does not start there where `CODEX_HOME` names no folder.
#[test]
fn a_codex_home_that_names_no_folder_is_said_as_it_is() {
    for (enrolled, level) in [(1, Level::Fail), (0, Level::Warn)] {
        let facts = CodexFacts {
            present: false,
            auth_access: None,
            login: Err(CodexLoginTrouble::NoHome(PathBuf::from(r"C:\nowhere"))),
            enrolled,
            ..with_codex()
        };
        let checks = codex_checks(&facts);
        let found = check(&checks, "codex_login");
        assert_eq!(
            (found.level, found.detail.clone()),
            (
                level,
                crate::error::codex_home_no_folder(std::path::Path::new(r"C:\nowhere"))
            )
        );
        assert_eq!(
            found.advice,
            format!("{}.", crate::error::CODEX_HOME_REMEDY)
        );
    }
}

// It used to fail the report, telling a person to sign in with `codex` again.
#[test]
fn a_codex_login_with_an_api_key_is_a_choice_rather_than_a_fault() {
    let why = "Codex is signed in with an API key rather than a ChatGPT account, so there is \
               no account login to park or switch";
    let api_key = || CodexFacts {
        login: Err(CodexLoginTrouble::NotAnAccount(why.into())),
        ..with_codex()
    };

    let checks = codex_checks(&api_key());
    let login = check(&checks, "codex_login");
    assert_eq!(
        login.level,
        Level::Ok,
        "stated, for somebody with no Codex accounts"
    );
    assert_eq!(login.detail, why);
    assert!(checks.iter().all(|c| c.level == Level::Ok));

    let checks = codex_checks(&CodexFacts {
        enrolled: 2,
        ..api_key()
    });
    let login = check(&checks, "codex_login");
    assert_eq!(login.level, Level::Warn);
    assert!(healthy(&checks), "nothing is broken");
    assert_eq!(login.detail, why);
    assert_eq!(login.advice, words::chatgpt_sign_ins_only("codex"));
}

#[test]
fn a_codex_account_is_judged_in_codex_section_under_a_codex_code() {
    let claude_only = {
        let mut f = facts();
        f.parks = vec![parked("work", Some(NOW + 20 * 86_400))];
        f.codex = with_codex();
        evaluate(&f)
    };

    let mut f = facts();
    let mut codex = codex_parked("work", Some(NOW - 1));
    codex.last_used_at = Some(NOW - 400 * 86_400);
    f.parks = vec![parked("work", Some(NOW + 20 * 86_400)), codex];
    f.codex = CodexFacts {
        enrolled: 1,
        ..with_codex()
    };
    let checks = evaluate(&f);

    let about_codex: Vec<&Check> = checks
        .iter()
        .filter(|c| c.name.contains("codex/"))
        .collect();
    let codes: Vec<&str> = about_codex.iter().map(|c| c.code).collect();
    assert_eq!(codes, ["codex_parked_login", "codex_dormant_account"]);
    let section = checks
        .iter()
        .position(|c| c.code == "codex_backend")
        .expect("a Codex section");
    let first = checks
        .iter()
        .position(|c| c.name.contains("codex/"))
        .unwrap();
    assert!(first > section, "after the Codex heading");

    let claude = |checks: &[Check]| -> Vec<(&'static str, String)> {
        checks
            .iter()
            .filter(|c| !c.code.starts_with("codex_"))
            .map(|c| (c.code, c.name.clone()))
            .collect()
    };
    assert_eq!(
        claude(&checks),
        claude(&claude_only),
        "Claude Code's section is Claude Code's accounts alone"
    );
}

// As behind a version manager's shim.
#[test]
fn a_codex_whose_version_cannot_be_read_is_not_called_missing() {
    let found = CodexFacts {
        program: Some(PathBuf::from("/home/x/.volta/bin/codex")),
        version: None,
        ..with_codex()
    };
    let verified = read_from(ProviderId::Codex, crate::host::OS);
    assert_eq!(
        check(&codex_checks(&found), "codex_version").detail,
        words::build_path_says_nothing("/home/x/.volta/bin/codex", verified)
    );

    let missing = CodexFacts {
        program: None,
        version: None,
        ..with_codex()
    };
    assert_eq!(
        check(&codex_checks(&missing), "codex_version").detail,
        words::build_not_found(verified)
    );
}

#[test]
fn a_codex_login_anybody_can_read_or_nobody_can_parse_is_said() {
    let mut codex = with_codex();
    codex.auth_access = Some(mode(0o644));
    let checks = judge_codex(Os::Linux, None, &codex, &[], NOW, &time::moment);
    let file = check(&checks, "codex_auth_file");
    assert_eq!(file.level, Level::Warn);
    assert_eq!(
        file.detail,
        words::open_to(codex.auth_file.display(), &mode(0o644))
    );
    assert_eq!(
        file.advice,
        words::codex_file_open(Os::Linux, &["chmod 600 /home/x/.codex/auth.json".into()])
    );

    codex.auth_file = PathBuf::from(r"C:\Users\dana\.codex\auth.json");
    codex.auth_access = Some(open_to_users());
    let checks = judge_codex(Os::Windows, Some(SID), &codex, &[], NOW, &time::moment);
    let file = check(&checks, "codex_auth_file");
    assert_eq!(file.level, Level::Warn);
    let [reset, grant] = crate::host::acl::icacls(false, r"C:\Users\dana\.codex\auth.json", SID);
    assert_eq!(
        file.advice,
        words::codex_file_open(Os::Windows, &[reset, grant])
    );

    codex.auth_access = Some(mode(0o600));
    codex.login = Err(CodexLoginTrouble::Unusable(
        "its id token is not readable".into(),
    ));
    assert_eq!(
        check(&codex_checks(&codex), "codex_login").level,
        Level::Warn
    );
    codex.enrolled = 2;
    let checks = codex_checks(&codex);
    let login = check(&checks, "codex_login");
    assert_eq!(login.level, Level::Fail);
    assert_eq!(
        login.detail,
        words::cannot_be_used("its id token is not readable")
    );
}

// Said, never warned about: running it is the point of having it.
#[test]
fn running_codex_processes_are_reported_as_a_fact() {
    use crate::holder::classify;
    use crate::host::Process;
    let at = |pid: u32, path: &str| Process {
        pid,
        path: PathBuf::from(path),
        package: None,
    };
    let holders = crate::provider::codex::holders::HOLDERS;
    let mut codex = with_codex();
    codex.running = Some(classify(
        &[at(4321, "codex"), at(99, "codex")],
        holders,
        "codex",
        &[],
    ));
    let said_of = |codex: &CodexFacts| {
        let holding = codex.running.as_deref().expect("listed");
        (
            words::each_keeps_its_account(&crate::holder::described_with_pids(holding)),
            crate::holder::remedies(holding, "to take a switch"),
        )
    };
    let checks = codex_checks(&codex);
    let running = check(&checks, "codex_running");
    assert_eq!(running.level, Level::Ok);
    assert_eq!(
        (running.detail.clone(), running.advice.clone()),
        said_of(&codex)
    );

    let many: Vec<Process> = (1..=23).map(|pid| at(pid, "codex")).collect();
    codex.running = Some(classify(&many, holders, "codex", &[]));
    assert_eq!(
        check(&codex_checks(&codex), "codex_running").detail,
        said_of(&codex).0
    );

    codex.running = Some(classify(
        &[at(
            7,
            "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/\
             Contents/MacOS/codex",
        )],
        holders,
        "codex",
        &[],
    ));
    let checks = codex_checks(&codex);
    let app = check(&checks, "codex_running");
    assert_eq!((app.detail.clone(), app.advice.clone()), said_of(&codex));

    codex.running = None;
    assert_eq!(
        check(&codex_checks(&codex), "codex_running").detail,
        words::COULD_NOT_TELL
    );
}

// They used to count with two scans, and only the switch's could be stood in for.
#[test]
fn doctor_and_a_switch_see_the_same_codex_running() {
    use crate::host::memory::MemoryHost;
    let host = MemoryHost::new();
    host.runs_at(
        "codex",
        &[
            "codex",
            "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/\
             MacOS/codex",
        ],
    );
    let home = tempfile::tempdir().expect("a scratch folder");
    let ctx = Context::new(home.path().to_path_buf()).with_memory_stores(Arc::clone(&host));
    let seen = running_codex(&ctx).expect("readable");
    assert_eq!(
        seen.iter().map(|h| h.holder.kind).collect::<Vec<_>>(),
        ["chatgpt_app", "session"]
    );
    assert_eq!(
        switch::StillHolding::These(seen),
        switch::still_holding(&ctx, ProviderId::Codex)
    );

    host.runs_at("codex", &[]);
    assert_eq!(running_codex(&ctx), Some(Vec::new()));
    assert_eq!(
        switch::still_holding(&ctx, ProviderId::Codex),
        switch::StillHolding::Nothing
    );

    host.without_a_process_list();
    assert_eq!(running_codex(&ctx), None);
    assert_eq!(
        switch::still_holding(&ctx, ProviderId::Codex),
        switch::StillHolding::Unknown
    );
}

#[test]
fn every_codex_failure_and_warning_tells_the_user_something() {
    for backend in [
        Backend::File,
        Backend::Keyring,
        Backend::Either,
        Backend::Ephemeral,
        Backend::Secrets,
        Backend::Unknown,
    ] {
        let codex = CodexFacts {
            backend,
            enrolled: 1,
            auth_access: Some(mode(0o666)),
            login: Err(CodexLoginTrouble::Unusable("unreadable".into())),
            ..with_codex()
        };
        for c in codex_checks(&codex) {
            assert!(c.code.starts_with("codex_"), "{}", c.code);
            if c.level != Level::Ok {
                assert!(!c.advice.is_empty(), "{} has no advice", c.code);
            }
        }
    }
}

#[test]
fn codex_facts_are_read_off_the_disk() {
    use crate::host::fs::testing;

    let scratch = tempfile::tempdir().expect("a scratch home");
    let root = scratch.path().to_path_buf();
    let home = root.join("codex");
    // Looked up on `PATH`, it would be this machine's, kept inside the real `~/.codex`.
    let codex = format!("codex{}", std::env::consts::EXE_SUFFIX);
    let program = root.join("bin").join(&codex);
    let ctx = Context::new(root.clone())
        .with_pitboard_home(root.join(".pitboard"))
        .with_codex_home(home.to_string_lossy().into())
        .with_codex_program(program.clone());

    let absent = codex_facts(&ctx, None);
    assert!(!absent.present);
    assert_eq!(absent.backend, Backend::File);
    assert_eq!(absent.auth_access, None);
    match crate::host::OS {
        Os::MacOs | Os::Linux => assert!(matches!(absent.login, Ok(None))),
        Os::Windows => assert!(
            matches!(&absent.login, Err(CodexLoginTrouble::NoHome(path)) if *path == home),
            "Codex does not start there"
        ),
    }
    assert_eq!(absent.program, None, "the one named, and it is not there");
    assert_eq!(absent.version, None);

    let installed = root
        .join("releases/0.154.0-aarch64-apple-darwin/bin")
        .join(&codex);
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    std::fs::write(&installed, "").unwrap();
    // A program is what can be run, as the installer leaves it.
    testing::make_runnable(&installed).expect("runnable");
    std::fs::create_dir_all(program.parent().unwrap()).unwrap();
    testing::link(&installed, &program).expect("a link");
    let found = codex_facts(&ctx, None);
    assert_eq!(found.program.as_deref(), Some(program.as_path()));
    assert_eq!(found.version.as_deref(), Some("0.154.0"));

    std::fs::create_dir_all(&home).expect("a Codex home");
    let auth = home.join("auth.json");
    std::fs::write(
        &auth,
        json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "id_token": crate::provider::jwt::unsigned(&json!({
                    "email": "w@example.com",
                    "https://api.openai.com/auth": {"chatgpt_account_id": "work-acc"},
                })),
                "access_token": "a",
                "refresh_token": "r",
                "account_id": "work-acc",
            },
        })
        .to_string(),
    )
    .expect("a login");
    testing::open_to_others(&auth).expect("opened to others");
    let found = codex_facts(&ctx, None);
    assert!(found.present);
    assert!(found.auth_access.expect("there").shared);
    let login = found.login.expect("readable").expect("there");
    assert_eq!(login.email, "w@example.com");
    assert_eq!(login.fingerprint.len(), 16);

    // Signed in with an API key: a choice, said in Codex's terms.
    std::fs::write(
        &auth,
        json!({"auth_mode": "apikey", "OPENAI_API_KEY": "sk-not-a-real-key"}).to_string(),
    )
    .unwrap();
    match codex_facts(&ctx, None).login {
        Err(CodexLoginTrouble::NotAnAccount(why)) => {
            assert!(why.contains("API key"), "{why}");
            assert!(!why.contains("sk-"), "never the key itself: {why}");
        }
        Err(other) => panic!("an API key is not an account: {other:?}"),
        Ok(_) => panic!("an API key is not an account"),
    }

    // As a running codex leaves it, refreshing in the middle of a switch.
    let mixed = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": crate::provider::jwt::unsigned(&json!({
                "email": "w@example.com",
                "https://api.openai.com/auth": {"chatgpt_account_id": "work-acc"},
            })),
            "access_token": "a",
            "refresh_token": "r",
            "account_id": "home-acc",
        },
    });
    std::fs::write(&auth, mixed.to_string()).unwrap();
    assert!(
        matches!(
            codex_facts(&ctx, None).login,
            Err(CodexLoginTrouble::Unusable(_))
        ),
        "a login mixing two accounts is one Pitboard cannot use"
    );

    std::fs::write(
        home.join("config.toml"),
        "cli_auth_credentials_store = \"ephemeral\"\n",
    )
    .expect("a config");
    let ephemeral = codex_facts(&ctx, None);
    assert_eq!(ephemeral.backend, Backend::Ephemeral);
    assert!(
        matches!(ephemeral.login, Ok(None)),
        "a store Pitboard does not handle is not read"
    );

    // Named as what it is, never as plain `keyring`, which the configuration does not say.
    std::fs::write(
        home.join("config.toml"),
        "cli_auth_credentials_store = \"auto\"\n[features]\nsecret_auth_storage = true\n",
    )
    .expect("a config");
    let secrets = codex_facts(&ctx, None);
    let store = crate::provider::codex::paths::store(&ctx);
    assert_eq!(secrets.backend, Backend::Secrets);
    assert_eq!(secrets.backend_setting, store.setting());
    assert_eq!(secrets.backend_remedy, store.to_use_the_file("codex"));
}

// The command line heads a tool's checks where they start, so each tool's come together.
#[test]
fn where_codex_keeps_its_home_on_windows_is_one_of_codexs_checks() {
    let home = PathBuf::from("C:").join("Users").join("x");
    let mut facts = facts();
    facts.os = Os::Windows;
    facts.codex = with_codex();
    facts.windows = Some(WindowsHomes {
        home: home.clone(),
        default_pitboard_home: Os::Windows.default_pitboard_home(&home),
        local_app_data: Some(home.join("AppData").join("Local")),
        profile: Some(PathBuf::from("C:").join("Users").join("y")),
        codex_home_set: false,
        roams: true,
    });
    let checks = evaluate(&facts);
    let codes: Vec<&str> = checks.iter().map(|c| c.code).collect();
    let installer = codes
        .iter()
        .position(|code| *code == "codex_installer_home")
        .expect("said");
    assert!(
        codes[..installer]
            .iter()
            .all(|code| section(code).is_none()),
        "{codes:?}"
    );
    assert!(
        codes[installer..]
            .iter()
            .all(|code| section(code) == Some(ProviderId::Codex)),
        "{codes:?}"
    );
    facts.codex = no_codex();
    let codes: Vec<&str> = evaluate(&facts).iter().map(|c| c.code).collect();
    assert!(codes.contains(&"roaming_profile"), "{codes:?}");
    assert!(
        codes.iter().all(|code| section(code).is_none()),
        "a machine with no Codex has no Codex heading: {codes:?}"
    );
}
