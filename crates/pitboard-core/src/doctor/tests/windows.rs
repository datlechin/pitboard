use super::*;

// Claude Code 2.1.301 runs it in Git Bash where it finds one, else in PowerShell.
#[test]
fn windows_is_told_which_shell_runs_the_status_line_and_whether_it_finds_pitboard() {
    use std::cell::Cell;
    let mut facts = facts();
    facts.os = Os::Windows;
    assert!(evaluate(&facts).iter().all(|c| c.code != "status_line"));
    let bash = PathBuf::from(r"C:\Program Files\Git\bin\bash.exe");
    let pitboard = PathBuf::from(r"C:\Users\x\.cargo\bin\pitboard.exe");
    let named = PathBuf::from(r"D:\tools\git.exe");
    let runs_pitboard = Cell::new(true);
    let five = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let powershell = std::cell::RefCell::new(Some(claude_paths::PowerShell::Windows(five.clone())));
    let mut said = |found: Option<&PathBuf>, passed_over: Option<&PathBuf>, on_path: bool| {
        facts.status_line = Some(StatusLineFact {
            git_bash: claude_paths::GitBash {
                found: found.cloned(),
                passed_over: passed_over.cloned(),
            },
            powershell: powershell.borrow().clone(),
            pitboard: on_path.then(|| pitboard.clone()),
            runs_pitboard: runs_pitboard.get(),
        });
        let checks = evaluate(&facts);
        let line = check(&checks, "status_line");
        (line.level, line.detail.clone(), line.advice.clone())
    };
    assert_eq!(
        said(Some(&bash), None, true),
        (
            Level::Ok,
            words::pitboard_there(&words::in_git_bash(bash.display()), pitboard.display(), ""),
            String::new()
        )
    );
    assert_eq!(
        said(None, Some(&named), true),
        (
            Level::Ok,
            words::pitboard_there(
                &words::in_windows_powershell(five.display()),
                pitboard.display(),
                &words::passes_over(named.display())
            ),
            String::new()
        )
    );
    let seven = PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe");
    *powershell.borrow_mut() = Some(claude_paths::PowerShell::Seven(seven.clone()));
    assert_eq!(
        said(None, None, true).1,
        words::pitboard_there(
            &words::in_powershell_7(seven.display()),
            pitboard.display(),
            ""
        )
    );
    *powershell.borrow_mut() = None;
    assert_eq!(
        said(None, None, true).1,
        words::pitboard_there(words::NO_SHELL, pitboard.display(), "")
    );
    *powershell.borrow_mut() = Some(claude_paths::PowerShell::Windows(five.clone()));
    assert_eq!(
        said(Some(&bash), None, false),
        (
            Level::Warn,
            words::no_pitboard_there(&words::in_git_bash(bash.display()), ""),
            words::ADD_PITBOARD_TO_PATH.to_string()
        )
    );
    runs_pitboard.set(false);
    assert_eq!(
        said(Some(&bash), None, false).0,
        Level::Ok,
        "nothing it runs needs one"
    );
    facts.claude_present = false;
    facts.codex.present = true;
    assert!(
        evaluate(&facts).iter().all(|c| c.code != "status_line"),
        "a machine with no Claude Code is not told"
    );
}

#[test]
fn only_a_bare_pitboard_needs_one_on_the_path() {
    for command in [
        "pitboard statusline",
        "  PITBOARD.EXE statusline --offline",
        "\"pitboard\" statusline",
        "pitboard",
    ] {
        assert!(runs_bare_pitboard(command), "{command}");
    }
    for command in [
        r"C:\Tools\pitboard.exe statusline",
        "~/bin/status.sh",
        "npx ccstatusline",
        "",
    ] {
        assert!(!runs_bare_pitboard(command), "{command}");
    }
}

// L5 and H9 from Codex 0.163.0's and Claude Code 2.1.301's builds, read as bytes.
#[test]
fn windows_is_told_what_its_settings_change_for_pitboard_and_the_tools() {
    let said = |facts: &Facts| -> Vec<(&'static str, Level, String, String)> {
        evaluate(facts)
            .into_iter()
            .filter(|c| ["wsl", "pitboard_copies", "system_proxy"].contains(&c.code))
            .map(|c| (c.code, c.level, c.detail, c.advice))
            .collect()
    };
    let copy = |folder: &str| PathBuf::from(format!(r"C:\{folder}\pitboard.exe"));
    let mut facts = facts();
    facts.os = Os::Windows;
    facts.windows_settings = Some(crate::host::WindowsSettings::default());
    facts.pitboards = vec![copy("a")];
    assert!(said(&facts).is_empty());

    facts.windows_settings = Some(crate::host::WindowsSettings {
        wsl: vec!["Ubuntu".into(), "Debian".into()],
        proxy: Some("proxy.example.com:8080".into()),
        admin_approval: Some(true),
    });
    facts.pitboards = vec![copy("a"), copy("b"), copy("c")];
    facts.codex.home = PathBuf::from(r"C:\Users\x\.codex");
    assert_eq!(
        said(&facts),
        [
            (
                "wsl",
                Level::Ok,
                words::wsl_holds(&["Ubuntu".into(), "Debian".into()]),
                words::installs_of_their_own(facts.codex.home.display())
            ),
            (
                "pitboard_copies",
                Level::Ok,
                words::copies_on_path(
                    copy("a").display(),
                    &[r"C:\b\pitboard.exe".into(), r"C:\c\pitboard.exe".into()]
                ),
                words::UNINSTALL_THE_OTHERS.to_string()
            ),
            (
                "system_proxy",
                Level::Ok,
                words::system_proxy("proxy.example.com:8080"),
                words::set_https_proxy(words::CODEX_GOES_THROUGH_IT, words::THE_OTHERS_GO_DIRECTLY)
            ),
        ]
    );

    let advice = |facts: &Facts| check(&evaluate(facts), "system_proxy").advice.clone();
    let named_by = |variable: &'static str| {
        by(ProxyFact {
            variable,
            ..through(None, &[])
        })
    };
    facts.network = named_by("HTTP_PROXY");
    assert_eq!(
        advice(&facts),
        words::set_https_proxy(
            words::CODEX_GOES_THROUGH_IT,
            &words::the_others_go(
                &words::the_way(Some("HTTP_PROXY")),
                &words::the_way(Some("HTTP_PROXY"))
            )
        )
    );
    facts.network = named_by("ALL_PROXY");
    assert_eq!(
        advice(&facts),
        words::set_https_proxy(
            words::CODEX_GOES_THROUGH_IT,
            &words::the_others_go(&words::the_way(None), &words::the_way(Some("ALL_PROXY")))
        )
    );
    facts.network = direct();
    let with_server = |facts: &mut Facts, server: &str| {
        if let Some(settings) = facts.windows_settings.as_mut() {
            settings.proxy = Some(server.into());
        }
    };
    with_server(
        &mut facts,
        "http=web.example.com:80;https=secure.example.com:443",
    );
    assert_eq!(
        advice(&facts),
        words::set_https_proxy(
            &words::codex_goes(&words::the_way(None)),
            words::THE_OTHERS_GO_DIRECTLY
        )
    );
    with_server(&mut facts, "https=secure.example.com:443");
    assert_eq!(
        advice(&facts),
        words::set_https_proxy(
            &words::codex_fails_through("https=secure.example.com:443"),
            words::THE_OTHERS_GO_DIRECTLY
        )
    );
    with_server(&mut facts, "proxy.example.com:8080");

    let codes = |facts: &Facts| {
        said(facts)
            .into_iter()
            .map(|(code, ..)| code)
            .collect::<Vec<_>>()
    };
    facts.network = by(through(None, &[]));
    assert_eq!(
        codes(&facts),
        ["wsl", "pitboard_copies"],
        "a variable names one"
    );
    facts.network = NetworkFact {
        proxy: None,
        passed_over: vec!["HTTPS_PROXY"],
        set: vec!["HTTPS_PROXY"],
    };
    assert_eq!(
        codes(&facts),
        ["wsl", "pitboard_copies"],
        "a variable is set"
    );
    facts.windows_settings = None;
    assert_eq!(codes(&facts), ["pitboard_copies"]);
}

#[test]
fn windows_settings_are_read_from_the_host_on_windows_alone() {
    let host = crate::host::memory::MemoryHost::new();
    let settings = crate::host::WindowsSettings {
        wsl: vec!["Ubuntu".into()],
        proxy: Some("proxy.example.com:8080".into()),
        admin_approval: Some(false),
    };
    host.sets_windows(settings.clone());
    let ctx = Context::for_unit_test().with_memory_stores(host);
    assert_eq!(
        gather(&ctx).windows_settings,
        (crate::host::OS == Os::Windows).then_some(settings)
    );
}

// RF-F3: a bare name runs the first a shell finds.
#[test]
fn every_pitboard_a_windows_shell_finds_is_one_copy_first_first() {
    let root = tempfile::tempdir().expect("a scratch folder");
    let (a, b) = (root.path().join("a"), root.path().join("b"));
    for folder in [&a, &b] {
        std::fs::create_dir_all(folder).expect("a folder");
        let program = folder.join("pitboard.exe");
        std::fs::write(&program, b"MZ").expect("a program");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
                .expect("runnable");
        }
    }
    let empty = root.path().join("empty");
    std::fs::create_dir_all(&empty).expect("a folder");
    // As winget's Links entry leads to its package's copy.
    let link = root.path().join("link");
    std::fs::create_dir_all(&link).expect("a folder");
    #[cfg(unix)]
    std::os::unix::fs::symlink(a.join("pitboard.exe"), link.join("pitboard.exe")).expect("a link");
    let path = std::env::join_paths([&a, &empty, &link, &b, &a]).expect("a PATH");
    let ctx = Context::for_unit_test().with_search_path(path.to_string_lossy().into_owned());
    assert_eq!(
        pitboards(&ctx),
        [a.join("pitboard.exe"), b.join("pitboard.exe")]
    );
}

// Codex reads a user name and a password out of `ProxyServer` as out of a variable.
#[test]
fn a_password_in_windows_proxy_setting_is_never_shown() {
    let ctx = Context::new(PathBuf::from("/home/x"));
    let mut f = facts();
    f.os = Os::Windows;
    for (server, shown) in [
        (
            "http=alice:s3cret@proxy.example.com:8080",
            "http=***:***@proxy.example.com:8080",
        ),
        (
            "alice:p=ss@proxy.example.com:8080",
            "***:***@proxy.example.com:8080",
        ),
        (
            "http://alice@proxy.example.com:8080;https=bob:s3cret@secure.example.com:443",
            "http://***@proxy.example.com:8080;https=***:***@secure.example.com:443",
        ),
    ] {
        f.windows_settings = Some(crate::host::WindowsSettings {
            proxy: Some(server.into()),
            ..crate::host::WindowsSettings::default()
        });
        let checks = evaluate(&f);
        let line = check(&checks, "system_proxy");
        assert_eq!(line.detail, words::system_proxy(shown));
        for said in [&line.detail, &line.advice] {
            let redacted = redaction_for(&ctx, &f).over(said);
            for secret in ["alice", "bob", "s3cret", "p=ss"] {
                assert!(!said.contains(secret), "{said}");
                assert!(!redacted.contains(secret), "{redacted}");
            }
        }
    }
}

#[test]
fn linux_inside_wsl_is_told_what_pitboard_switches_there() {
    let mut facts = facts();
    facts.os = Os::Linux;
    assert!(evaluate(&facts).iter().all(|c| c.code != "wsl"));
    let said = |facts: &Facts| {
        let checks = evaluate(facts);
        let wsl = check(&checks, "wsl");
        (wsl.level, wsl.detail.clone(), wsl.advice.clone())
    };
    facts.wsl = Some(WslFact {
        distribution: Some("Ubuntu".into()),
        codex_home_on_windows: None,
    });
    assert_eq!(
        said(&facts),
        (
            Level::Ok,
            words::inside_wsl(Some("Ubuntu")),
            words::WINDOWS_KEEPS_ITS_LOGINS.into()
        )
    );
    facts.wsl = Some(WslFact {
        distribution: None,
        codex_home_on_windows: Some(PathBuf::from("/mnt/c/Users/x/.codex")),
    });
    assert_eq!(
        said(&facts),
        (
            Level::Ok,
            words::inside_wsl(None),
            words::codex_home_on_windows("/mnt/c/Users/x/.codex", Some(r"C:\Users\x\.codex"))
        )
    );
    facts.wsl = Some(WslFact {
        distribution: None,
        codex_home_on_windows: Some(PathBuf::from("/media/c/codex")),
    });
    assert_eq!(
        said(&facts).2,
        words::codex_home_on_windows("/media/c/codex", None)
    );
}

#[test]
fn a_windows_drive_wsl_mounts_is_named_as_windows_names_it() {
    for (path, windows) in [
        ("/mnt/c", Some(r"C:\")),
        ("/mnt/c/Users/x/.codex", Some(r"C:\Users\x\.codex")),
        ("/mnt/d/", Some(r"D:\")),
        ("/mnt/cd/x", None),
        ("/mnt/1/x", None),
        ("/home/x/.codex", None),
    ] {
        assert_eq!(
            on_windows(std::path::Path::new(path)).as_deref(),
            windows,
            "{path}"
        );
    }
}

// `codex_wsl`: `WSL_DISTRO_NAME` set, even empty, or `/proc/version` naming Microsoft.
#[test]
fn wsl_is_read_as_codex_reads_it_and_on_linux_alone() {
    use crate::context::Environment;
    let home = tempfile::tempdir().expect("a scratch home");
    let codex = home.path().join("codex");
    let read = |os: Os, distribution: Option<&str>, host: &Arc<crate::host::memory::MemoryHost>| {
        let mut pairs = vec![
            ("HOME", home.path().to_string_lossy().into_owned()),
            ("USERPROFILE", home.path().to_string_lossy().into_owned()),
            ("CODEX_HOME", codex.to_string_lossy().into_owned()),
        ];
        pairs.extend(distribution.map(|name| ("WSL_DISTRO_NAME", name.to_owned())));
        let ctx =
            Context::read(&Environment::on(os, pairs), "cli").with_memory_stores(host.clone());
        wsl_fact(&ctx).map(|wsl| (wsl.distribution, wsl.codex_home_on_windows))
    };
    let host = crate::host::memory::MemoryHost::new();
    assert_eq!(read(Os::Linux, None, &host), None);
    assert_eq!(
        read(Os::Linux, Some("Ubuntu"), &host),
        Some((Some("Ubuntu".into()), None))
    );
    assert_eq!(read(Os::Linux, Some(""), &host), Some((None, None)));
    for os in [Os::MacOs, Os::Windows] {
        assert_eq!(read(os, Some("Ubuntu"), &host), None, "{os:?}");
    }
    host.runs_in_wsl();
    host.mounts_a_windows_drive(home.path());
    assert_eq!(
        read(Os::Linux, None, &host),
        Some((None, Some(codex.clone())))
    );
    assert_eq!(read(Os::Windows, None, &host), None);
}

// RF-B1 measured the profile, local app data and the home agreeing on a local profile.
#[test]
fn doctor_says_where_windows_keeps_an_accounts_folders_apart_from_the_home() {
    let home = PathBuf::from("C:").join("Users").join("x");
    let measured = || WindowsHomes {
        home: home.clone(),
        default_pitboard_home: Os::Windows.default_pitboard_home(&home),
        local_app_data: Some(home.join("AppData").join("Local")),
        profile: Some(home.clone()),
        codex_home_set: false,
        roams: false,
    };
    let codes = |windows: Option<WindowsHomes>| {
        let mut facts = facts();
        facts.windows = windows;
        judge_windows_homes(&facts)
            .into_iter()
            .chain(judge_codex_installer_home(&facts))
            .map(|check| (check.code, check.level))
            .collect::<Vec<_>>()
    };
    assert!(codes(None).is_empty());
    assert!(codes(Some(measured())).is_empty());
    let in_any_case = PathBuf::from("c:").join("users").join("X");
    let other_case = WindowsHomes {
        local_app_data: Some(in_any_case.join("appdata").join("local")),
        profile: Some(in_any_case),
        ..measured()
    };
    assert!(codes(Some(other_case)).is_empty());
    let moved = WindowsHomes {
        local_app_data: Some(PathBuf::from("D:").join("Local")),
        ..measured()
    };
    assert_eq!(codes(Some(moved)), [("local_app_data", Level::Warn)]);
    let elsewhere = PathBuf::from("C:").join("Users").join("y");
    let parted = || WindowsHomes {
        profile: Some(elsewhere.clone()),
        ..measured()
    };
    assert_eq!(
        codes(Some(parted())),
        [("codex_installer_home", Level::Warn)]
    );
    let named = WindowsHomes {
        codex_home_set: true,
        ..parted()
    };
    assert!(codes(Some(named)).is_empty(), "CODEX_HOME names one folder");
    let roams = WindowsHomes {
        roams: true,
        ..measured()
    };
    assert_eq!(codes(Some(roams)), [("roaming_profile", Level::Warn)]);
}
