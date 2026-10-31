use super::*;

fn at(percent: u8) -> crate::watch_service::Switching {
    let at = crate::duty::autoswitch::Threshold::new(percent).expect("a share");
    crate::watch_service::Switching::At(at)
}

#[test]
fn a_schedule_is_judged_by_whether_the_pitboard_it_runs_is_still_there() {
    let mut f = facts();
    assert!(
        evaluate(&f).iter().all(|c| c.code != "schedule"),
        "nothing is said where there is no schedule"
    );

    f.schedule = Some(ScheduleFact {
        location: Location::File(PathBuf::from(
            "/home/x/Library/LaunchAgents/com.datlechin.pitboard.renew.plist",
        )),
        task: None,
        program: Some(PathBuf::from("/opt/homebrew/bin/pitboard")),
        program_found: true,
        network: direct(),
        network_differs: false,
        outdated: false,
        refused: None,
    });
    let checks = evaluate(&f);
    let found = check(&checks, "schedule");
    assert_eq!(found.level, Level::Ok);
    assert_eq!(
        found.detail,
        words::runs_daily("/opt/homebrew/bin/pitboard")
    );

    f.schedule.as_mut().expect("set above").program_found = false;
    let checks = evaluate(&f);
    let gone = check(&checks, "schedule");
    assert_eq!(gone.level, Level::Fail, "every renewal from now on fails");
    assert_eq!(
        gone.detail,
        words::runs_what_is_gone("/opt/homebrew/bin/pitboard")
    );
    assert_eq!(gone.advice, again(crate::host::OS));

    f.schedule = Some(ScheduleFact {
        location: Location::File(PathBuf::from(
            "/home/x/Library/LaunchAgents/com.datlechin.pitboard.renew.plist",
        )),
        task: None,
        program: Some(PathBuf::from(
            "/Applications/Pitboard.app/Contents/MacOS/Pitboard",
        )),
        program_found: true,
        network: direct(),
        network_differs: false,
        outdated: false,
        refused: None,
    });
    f.os = Os::MacOs;
    let checks = evaluate(&f);
    let the_app = check(&checks, "schedule");
    assert_eq!(
        the_app.level,
        Level::Fail,
        "0.3.0's app renews nothing when started with `renew`"
    );
    assert_eq!(
        the_app.detail,
        words::runs_the_app("/Applications/Pitboard.app/Contents/MacOS/Pitboard")
    );
    f.schedule.as_mut().expect("set above").program = Some(PathBuf::from(
        "/Applications/Pitboard.app/Contents/Helpers/pitboard",
    ));
    assert_eq!(
        check(&evaluate(&f), "schedule").level,
        Level::Ok,
        "the command line an app comes with is a command line"
    );
    f.os = Os::Linux;
    for (program, level) in [
        ("/usr/bin/pitboard-app", Level::Fail),
        ("/usr/bin/pitboard", Level::Ok),
    ] {
        f.schedule.as_mut().expect("set above").program = Some(PathBuf::from(program));
        assert_eq!(check(&evaluate(&f), "schedule").level, level, "{program}");
    }
    f.os = crate::host::OS;

    f.schedule.as_mut().expect("set above").program = None;
    let checks = evaluate(&f);
    let unread = check(&checks, "schedule");
    assert_eq!(unread.level, Level::Warn);
    assert_eq!(
        (unread.detail.clone(), unread.advice.as_str()),
        (
            words::does_not_say_which_pitboard(&f.schedule.as_ref().expect("set above").location),
            again(crate::host::OS)
        )
    );
}

#[test]
fn the_watch_service_is_judged_by_its_pitboard_and_its_last_run() {
    let mut f = facts();
    assert!(
        evaluate(&f).iter().all(|c| c.code != "watch_service"),
        "nothing is said where there is no service"
    );
    let service = |program: Option<&str>, task: Option<Task>| WatchServiceFact {
        location: Location::File(PathBuf::from(
            "/home/x/Library/LaunchAgents/com.datlechin.pitboard.watch.plist",
        )),
        program: program.map(PathBuf::from),
        program_found: true,
        switching: Some(at(90)),
        task,
        outdated: false,
        refused: None,
    };

    f.watch_service = Some(service(Some("/opt/homebrew/bin/pitboard"), None));
    let checks = evaluate(&f);
    let found = check(&checks, "watch_service");
    assert_eq!(found.level, Level::Ok);
    assert_eq!(
        found.detail,
        words::runs_from_sign_in(Some(at(90)), "/opt/homebrew/bin/pitboard")
    );
    assert_eq!(found.name, "watch service");
    assert_eq!(section("watch_service"), None, "Pitboard's own");

    f.watch_service.as_mut().expect("set above").program_found = false;
    let checks = evaluate(&f);
    let gone = check(&checks, "watch_service");
    assert_eq!(gone.level, Level::Fail, "it switches nothing from now on");
    assert_eq!(gone.advice, words::INSTALL_THE_SERVICE_AGAIN);

    f.watch_service = Some(service(None, None));
    assert_eq!(check(&evaluate(&f), "watch_service").level, Level::Warn);

    let ran = Task {
        last_run: Some(NOW - 60),
        last_result: Some(1),
        drifted: Vec::new(),
    };
    f.watch_service = Some(service(Some(r"C:\bin\pitboard.exe"), Some(ran)));
    let checks = evaluate(&f);
    let failed = check(&checks, "watch_service");
    assert_eq!(failed.level, Level::Warn);
    assert_eq!(
        (failed.detail.clone(), failed.advice.as_str()),
        (
            words::last_run_ended(
                &words::runs_from_sign_in(Some(at(90)), r"C:\bin\pitboard.exe"),
                None,
                1
            ),
            words::WATCH_TO_SEE_WHY
        )
    );
}

#[test]
fn a_task_is_judged_by_its_settings_and_its_last_run() {
    let mut f = facts();
    f.os = Os::Windows;
    let program = r"C:\Users\x\scoop\apps\pitboard\current\pitboard.exe";
    let task = |last_run: Option<i64>, last_result: i32, drifted: Vec<Drift>| ScheduleFact {
        location: Location::Task(r"\Pitboard\Renew S-1-5-21-1".into()),
        task: Some(Task {
            last_run,
            last_result: Some(last_result),
            drifted,
        }),
        program: Some(PathBuf::from(program)),
        program_found: true,
        network: direct(),
        network_differs: false,
        outdated: false,
        refused: None,
    };
    let judged = |f: &Facts| {
        evaluate(f)
            .into_iter()
            .find(|c| c.code == "schedule")
            .expect("the schedule's line")
    };

    f.schedule = Some(task(None, 0x0004_1303, Vec::new()));
    let fresh = judged(&f);
    assert_eq!(fresh.level, Level::Ok);
    let runs = words::runs_daily(program);
    assert_eq!(fresh.detail, words::not_run_yet(&runs));

    let an_hour_ago = Some(f.now - 3_660);
    for fine in [0, 0x0004_1301] {
        f.schedule = Some(task(an_hour_ago, fine, Vec::new()));
        let ran = judged(&f);
        assert_eq!(ran.level, Level::Ok, "{fine:#x}");
        assert_eq!(ran.detail, words::last_ran(&runs, 3_660));
    }

    for result in [1, 0x8007_10E0_u32 as i32] {
        f.schedule = Some(task(an_hour_ago, result, Vec::new()));
        let failed = judged(&f);
        assert_eq!(failed.level, Level::Warn);
        assert_eq!(
            (failed.detail, failed.advice.as_str()),
            (
                words::last_run_ended(&runs, Some(3_660), result),
                words::RENEW_TO_SEE_WHY
            )
        );
    }

    f.schedule = Some(task(
        an_hour_ago,
        0,
        vec![Drift::OnAcPowerOnly, Drift::Disabled],
    ));
    let drifted = judged(&f);
    assert_eq!(drifted.level, Level::Warn);
    assert_eq!(
        drifted.detail,
        words::drifted_in_task_scheduler(
            &runs,
            &[
                words::drift(&Drift::OnAcPowerOnly),
                words::drift(&Drift::Disabled)
            ]
        )
    );
    assert_eq!(drifted.advice, again(Os::Windows));
}

/// A timer an earlier Pitboard wrote counts only time awake, and a service it wrote fails
/// once its Pitboard is gone, which installing again mends. Read from the disk on Linux, the
/// one system whose jobs say so.
#[test]
fn a_unit_an_earlier_pitboard_wrote_is_said_by_doctor() {
    let mut f = facts();
    let program = "/usr/local/bin/pitboard";
    f.schedule = Some(ScheduleFact {
        outdated: true,
        refused: None,
        ..scheduled(direct(), false)
    });
    f.watch_service = Some(WatchServiceFact {
        location: Location::File(PathBuf::from(
            "/home/x/.config/systemd/user/pitboard-watch.service",
        )),
        program: Some(PathBuf::from(program)),
        program_found: true,
        switching: None,
        task: None,
        outdated: true,
        refused: None,
    });
    let checks = evaluate(&f);
    let renewal = check(&checks, "schedule");
    assert_eq!(
        (
            renewal.level,
            renewal.detail.clone(),
            renewal.advice.as_str()
        ),
        (
            Level::Warn,
            words::renews_by_time_awake(&words::runs_daily(program)),
            again(f.os)
        )
    );
    let watch = check(&checks, "watch_service");
    assert_eq!(
        (watch.level, watch.detail.clone(), watch.advice.as_str()),
        (
            Level::Warn,
            words::fails_once_its_pitboard_is_gone(&words::runs_from_sign_in(None, program)),
            words::INSTALL_THE_SERVICE_AGAIN
        )
    );
    f.schedule.as_mut().expect("set above").program_found = false;
    assert_eq!(
        check(&evaluate(&f), "schedule").level,
        Level::Fail,
        "a Pitboard gone is said first"
    );

    #[cfg(target_os = "linux")]
    {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let home = scratch.path().to_path_buf();
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its folder")).expect("a bin");
        std::fs::write(&program, "").expect("a Pitboard");
        let ctx = Context::new(home.clone())
            .with_memory_stores(crate::host::memory::MemoryHost::new())
            .with_schedule_program(program.clone());
        let units = home.join(".config/systemd/user");
        std::fs::create_dir_all(&units).expect("made");
        for (file, body) in [
            (
                "pitboard-renew.timer",
                "[Timer]\nOnUnitActiveSec=86400\nOnStartupSec=900\nPersistent=true\n".to_string(),
            ),
            (
                "pitboard-renew.service",
                format!(
                    "[Service]\nType=oneshot\nExecStart={} renew --scheduled\n",
                    program.display()
                ),
            ),
            (
                "pitboard-watch.service",
                format!(
                    "[Service]\nExecStart={} watch --scheduled --at 90\n",
                    program.display()
                ),
            ),
        ] {
            std::fs::write(units.join(file), body).expect("as an earlier Pitboard wrote it");
        }
        assert!(schedule_fact(&ctx).expect("installed").outdated);
        assert!(watch_service_fact(&ctx).expect("installed").outdated);

        crate::schedule::install(&ctx, Permit::for_a_test()).expect("installed again");
        crate::watch_service::install(&ctx, Permit::for_a_test(), at(90)).expect("installed again");
        assert!(!schedule_fact(&ctx).expect("installed").outdated);
        assert!(!watch_service_fact(&ctx).expect("installed").outdated);
    }
}

#[test]
fn a_broken_schedule_is_written_again_by_whatever_this_machine_has() {
    for os in [Os::MacOs, Os::Windows] {
        assert_eq!(again(os), words::AGAIN_IN_THE_APP_OR_A_TERMINAL, "{os:?}");
    }
    assert_eq!(
        again(Os::Linux),
        words::AGAIN_IN_A_TERMINAL,
        "there is no app here"
    );
}

#[test]
fn a_schedule_whose_pitboard_is_gone_is_found_on_the_disk() {
    let scratch = tempfile::tempdir().expect("a scratch home");
    let root = scratch.path().to_path_buf();

    let program = root.join("bin/pitboard");
    let ctx = Context::new(root.clone())
        .with_memory_stores(crate::host::memory::MemoryHost::new())
        .with_schedule_program(program.clone());
    assert!(schedule_fact(&ctx).is_none(), "nothing installed yet");

    std::fs::create_dir_all(root.join("bin")).expect("a bin");
    std::fs::write(&program, "").expect("a Pitboard");
    crate::schedule::install(&ctx, Permit::for_a_test()).expect("installed");
    let fact = schedule_fact(&ctx).expect("installed");
    assert_eq!(fact.program.as_deref(), Some(program.as_path()));
    assert!(fact.program_found);

    std::fs::remove_file(&program).expect("taken away");
    assert!(!schedule_fact(&ctx).expect("still installed").program_found);

    assert!(
        schedule_fact(&ctx.with_pitboard_home(root.join("elsewhere"))).is_none(),
        "a Pitboard pointed at another home has no schedule"
    );
}

// Through the pretend Task Scheduler, on any system; a reinstall is the fix doctor names.
#[test]
fn a_task_changed_in_task_scheduler_is_read_where_it_is_kept() {
    let scratch = tempfile::tempdir().expect("a scratch home");
    let home = scratch.path().to_string_lossy().into_owned();
    let program = scratch.path().join("bin").join("pitboard.exe");
    std::fs::create_dir_all(program.parent().expect("its folder")).expect("a bin");
    std::fs::write(&program, "").expect("a Pitboard");
    let env: crate::context::Environment =
        [("HOME", home.as_str()), ("USERPROFILE", home.as_str())]
            .into_iter()
            .collect();
    let ctx = Context::for_command_line(&env)
        .with_memory_stores(crate::host::memory::MemoryHost::scheduling_tasks(
            "S-1-5-21-1",
        ))
        .with_schedule_program(program);
    crate::schedule::install(&ctx, Permit::for_a_test()).expect("installed");
    let fresh = Task {
        last_run: None,
        last_result: Some(0x0004_1303),
        drifted: Vec::new(),
    };
    let read = |ctx: &Context| schedule_fact(ctx).expect("installed").task;
    assert_eq!(read(&ctx), Some(fresh.clone()));

    let kept = crate::schedule::path(&ctx).expect("the task's file");
    let xml = std::fs::read_to_string(&kept).expect("written");
    std::fs::write(
        &kept,
        xml.replace(
            "<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>",
            "<StopIfGoingOnBatteries>true</StopIfGoingOnBatteries>",
        ),
    )
    .expect("changed by hand");
    assert_eq!(
        read(&ctx).map(|task| task.drifted),
        Some(vec![Drift::StopsOnBattery])
    );

    crate::schedule::install(&ctx, Permit::for_a_test()).expect("installed again");
    assert_eq!(read(&ctx), Some(fresh));
}

#[test]
fn a_schedule_whose_proxy_differs_from_this_runs_is_said() {
    let mut f = facts();
    f.schedule = Some(scheduled(direct(), false));
    assert!(
        evaluate(&f).iter().all(|c| c.code != "schedule_proxy"),
        "nothing is said where they go the same way"
    );

    f.schedule = Some(scheduled(by(through(None, &[])), true));
    let checks = evaluate(&f);
    let line = check(&checks, "schedule_proxy");
    let installed = || words::installed_with(&words::proxy_named(&through(None, &[])));
    assert_eq!(
        (line.level, line.name.as_str(), line.detail.clone()),
        (
            Level::Warn,
            "renewal schedule's proxy",
            words::installed_and_here(&installed(), &words::goes_directly("this run"))
        )
    );
    assert_eq!(line.advice, words::INSTALL_AGAIN_FROM_HERE);

    let on_loopback = || ProxyFact {
        variable: "ALL_PROXY",
        address: "http://127.0.0.1:3128".into(),
        server: "127.0.0.1:3128".into(),
        on_loopback: true,
        ..through(Some("NO_PROXY"), &["chatgpt.com"])
    };
    f.network = by(on_loopback());
    f.schedule = Some(scheduled(direct(), true));
    assert_eq!(
        check(&evaluate(&f), "schedule_proxy").detail,
        words::installed_and_here(
            words::INSTALLED_WITH_NO_PROXY,
            &words::goes_through("this run", &words::proxy_named(&on_loopback()))
        )
    );

    // Installed again from here, the schedule would be given a SOCKS proxy that fails too.
    let socks = || ProxyFact {
        address: "socks5h://127.0.0.1:1080".into(),
        server: "127.0.0.1:1080".into(),
        socks: true,
        fails: true,
        ..on_loopback()
    };
    f.network = by(socks());
    let checks = evaluate(&f);
    let line = check(&checks, "schedule_proxy");
    assert_eq!(
        (line.detail.clone(), line.advice.clone()),
        (
            words::installed_and_here(
                words::INSTALLED_WITH_NO_PROXY,
                &words::is_given("this run", &words::proxy_named(&socks()))
            ),
            words::socks_would_fail("ALL_PROXY")
        )
    );
    f.network = by(ProxyFact {
        exempt: [
            "api.anthropic.com",
            "platform.claude.com",
            "auth.openai.com",
            "chatgpt.com",
        ]
        .map(String::from)
        .to_vec(),
        fails: false,
        ..socks()
    });
    let checks = evaluate(&f);
    assert_eq!(
        check(&checks, "schedule_proxy").advice,
        words::INSTALL_AGAIN_FROM_HERE
    );

    f.network = by(through(None, &[]));
    f.schedule = Some(scheduled(by(through(None, &[])), true));
    assert_eq!(
        check(&evaluate(&f), "schedule_proxy").detail,
        words::another_user_or_password("this run", &words::proxy_named(&through(None, &[])))
    );
}

// The app's environment names no proxy a shell set, so the line is no warning there.
#[test]
fn in_the_app_a_schedules_other_proxy_is_said_without_a_warning() {
    let mut f = facts();
    f.in_the_app = true;
    f.schedule = Some(scheduled(direct(), false));
    assert!(evaluate(&f).iter().all(|c| c.code != "schedule_proxy"));

    f.schedule = Some(scheduled(by(through(None, &[])), true));
    let checks = evaluate(&f);
    let line = check(&checks, "schedule_proxy");
    let proxy = || words::proxy_named(&through(None, &[]));
    assert_eq!(
        (line.level, line.name.as_str(), line.detail.clone()),
        (
            Level::Ok,
            "renewal schedule's proxy",
            words::installed_and_here(
                &words::installed_with(&proxy()),
                &words::goes_directly("the app")
            )
        )
    );
    assert_eq!(line.advice, words::THE_APPS_OWN_ENVIRONMENT);

    f.network = by(through(None, &[]));
    assert_eq!(
        check(&evaluate(&f), "schedule_proxy").detail,
        words::another_user_or_password("the app", &proxy())
    );
}

#[test]
fn the_schedules_proxy_is_read_from_what_install_wrote() {
    let scratch = tempfile::tempdir().expect("a scratch home");
    let root = scratch.path().to_path_buf();
    let program = root.join("bin/pitboard");
    std::fs::create_dir_all(root.join("bin")).expect("a bin");
    std::fs::write(&program, "").expect("a Pitboard");
    let machine = crate::host::memory::MemoryHost::new();
    let home = root.to_string_lossy().into_owned();
    let run = |proxy: Option<&str>| {
        let env: crate::context::Environment =
            [("HOME", home.as_str()), ("USERPROFILE", home.as_str())]
                .into_iter()
                .chain(proxy.map(|address| ("HTTPS_PROXY", address)))
                .collect();
        Context::for_command_line(&env)
            .with_memory_stores(Arc::clone(&machine))
            .with_schedule_program(program.clone())
    };
    let gathered = |ctx: &Context| {
        let mut f = facts();
        f.schedule = schedule_fact(ctx);
        f.network = network_fact(&ctx.proxy);
        f
    };
    let installer = run(Some("http://alice:s3cret@proxy.example.com:3128"));
    crate::schedule::install(&installer, Permit::for_a_test()).expect("installed");

    let same = gathered(&installer);
    assert!(!same.schedule.as_ref().expect("installed").network_differs);
    assert!(evaluate(&same).iter().all(|c| c.code != "schedule_proxy"));

    let changed = gathered(&run(Some("http://alice:changed@proxy.example.com:3128")));
    let detail = check(&evaluate(&changed), "schedule_proxy").detail.clone();
    assert_eq!(
        detail,
        words::another_user_or_password(
            "this run",
            &words::proxy_named(changed.network.proxy.as_ref().expect("a proxy"))
        )
    );
    for secret in ["alice", "s3cret", "changed"] {
        assert!(!detail.contains(secret), "{secret}: {detail}");
    }

    let direct = run(None);
    let f = gathered(&direct);
    let detail = check(&evaluate(&f), "schedule_proxy").detail.clone();
    assert_eq!(
        detail,
        words::installed_and_here(
            &words::installed_with(&words::proxy_named(
                f.schedule
                    .as_ref()
                    .and_then(|schedule| schedule.network.proxy.as_ref())
                    .expect("installed with a proxy")
            )),
            &words::goes_directly("this run")
        )
    );
    let hidden = redaction_for(&direct, &f).over(&detail);
    assert!(
        hidden.starts_with("the schedule was installed with http://***:***@<proxy ")
            && !hidden.contains("proxy.example.com"),
        "{hidden}"
    );
}

// A run at boot can find the store locked, and doctor runs where it is open: what the run kept
// is said, with when, below what a gone Pitboard or an earlier one's unit says.
#[test]
fn a_run_that_could_not_open_the_store_is_said_with_when() {
    let mut f = facts();
    let mut schedule = scheduled(direct(), false);
    schedule.refused = Some(crate::schedule::Refused {
        at: NOW - 33_120,
        vault: Some(crate::host::Vault::SecretService),
        codes: ["credential_store_locked".to_string()].into(),
    });
    f.schedule = Some(schedule);
    for os in [Os::Linux, Os::MacOs, Os::Windows] {
        f.os = os;
        let checks = evaluate(&f);
        let refused = check(&checks, "schedule");
        assert_eq!(refused.level, Level::Warn, "{os:?}");
        assert_eq!(
            (refused.detail.as_str(), refused.advice.as_str()),
            (
                words::could_not_open(
                    &words::runs_daily("/usr/local/bin/pitboard"),
                    33_120,
                    Some(crate::host::Vault::SecretService),
                    true,
                    LOGIN
                )
                .as_str(),
                words::runs_again(os)
            ),
            "{os:?}"
        );
        if os == Os::Linux {
            for said in [&refused.detail, &refused.advice] {
                assert!(!crate::words::names_what_linux_has_not(said), "{said}");
            }
        }
    }

    let schedule = f.schedule.as_mut().expect("set above");
    schedule.outdated = true;
    assert_eq!(
        check(&evaluate(&f), "schedule").detail,
        words::renews_by_time_awake(&words::runs_daily("/usr/local/bin/pitboard"))
    );
    let schedule = f.schedule.as_mut().expect("set above");
    schedule.program_found = false;
    assert_eq!(check(&evaluate(&f), "schedule").level, Level::Fail);
}

// The watch service never asks to unlock the store either, and runs for days: what one of its
// switches or warm-ups could not open is said, with when, until one opens it.
#[test]
fn a_switch_of_the_watch_service_that_could_not_open_the_store_is_said_with_when() {
    let mut f = facts();
    let program = "/usr/local/bin/pitboard";
    f.watch_service = Some(WatchServiceFact {
        location: Location::File(PathBuf::from(
            "/home/x/.config/systemd/user/pitboard-watch.service",
        )),
        program: Some(PathBuf::from(program)),
        program_found: true,
        switching: Some(at(90)),
        task: None,
        outdated: false,
        refused: Some(crate::schedule::Refused {
            at: NOW - 600,
            vault: Some(crate::host::Vault::SecretService),
            codes: ["credential_store_locked".to_string()].into(),
        }),
    });
    for os in [Os::Linux, Os::MacOs, Os::Windows] {
        f.os = os;
        let checks = evaluate(&f);
        let refused = check(&checks, "watch_service");
        assert_eq!(refused.level, Level::Warn, "{os:?}");
        assert_eq!(
            (refused.detail.as_str(), refused.advice.as_str()),
            (
                words::switch_could_not_open(
                    &words::runs_from_sign_in(Some(at(90)), program),
                    600,
                    Some(crate::host::Vault::SecretService),
                    true,
                    LOGIN
                )
                .as_str(),
                words::TRIES_AGAIN_AT_ITS_NEXT_SWITCH
            ),
            "{os:?}"
        );
    }

    let service = f.watch_service.as_mut().expect("set above");
    service.outdated = true;
    assert_eq!(
        check(&evaluate(&f), "watch_service").detail,
        words::fails_once_its_pitboard_is_gone(&words::runs_from_sign_in(Some(at(90)), program))
    );
}
