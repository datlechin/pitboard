use super::*;

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
    });
    let checks = evaluate(&f);
    let found = check(&checks, "schedule");
    assert_eq!(found.level, Level::Ok);
    assert!(found.detail.contains("/opt/homebrew/bin/pitboard"));

    f.schedule.as_mut().expect("set above").program_found = false;
    let checks = evaluate(&f);
    let gone = check(&checks, "schedule");
    assert_eq!(gone.level, Level::Fail, "every renewal from now on fails");
    assert!(gone.detail.contains("/opt/homebrew/bin/pitboard"));
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
    });
    let checks = evaluate(&f);
    let the_app = check(&checks, "schedule");
    assert_eq!(
        the_app.level,
        Level::Fail,
        "0.3.0's app renews nothing when started with `renew`"
    );
    assert!(
        the_app.detail.contains("the app itself"),
        "{}",
        the_app.detail
    );
    f.schedule.as_mut().expect("set above").program = Some(PathBuf::from(
        "/Applications/Pitboard.app/Contents/Helpers/pitboard",
    ));
    assert_eq!(
        check(&evaluate(&f), "schedule").level,
        Level::Ok,
        "the command line an app comes with is a command line"
    );

    f.schedule.as_mut().expect("set above").program = None;
    let checks = evaluate(&f);
    let unread = check(&checks, "schedule");
    assert_eq!(unread.level, Level::Warn);
    assert!(!unread.advice.is_empty());
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
        at: crate::duty::autoswitch::Threshold::new(90),
        task,
    };

    f.watch_service = Some(service(Some("/opt/homebrew/bin/pitboard"), None));
    let checks = evaluate(&f);
    let found = check(&checks, "watch_service");
    assert_eq!(found.level, Level::Ok);
    assert_eq!(
        found.detail,
        "from sign-in  ·  switches at 90%  ·  runs /opt/homebrew/bin/pitboard"
    );
    assert_eq!(found.name, "watch service");
    assert_eq!(section("watch_service"), None, "Pitboard's own");

    f.watch_service.as_mut().expect("set above").program_found = false;
    let checks = evaluate(&f);
    let gone = check(&checks, "watch_service");
    assert_eq!(gone.level, Level::Fail, "it switches nothing from now on");
    assert_eq!(
        gone.advice,
        "Install it again with `pitboard watch install`."
    );

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
    assert!(
        failed
            .detail
            .ends_with("and its last run ended with exit code 1"),
        "{}",
        failed.detail
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
    assert_eq!(
        fresh.detail,
        format!("daily  ·  runs {program}  ·  not run yet")
    );

    let an_hour_ago = Some(f.now - 3_660);
    for fine in [0, 0x0004_1301] {
        f.schedule = Some(task(an_hour_ago, fine, Vec::new()));
        let ran = judged(&f);
        assert_eq!(ran.level, Level::Ok, "{fine:#x}");
        assert_eq!(
            ran.detail,
            format!("daily  ·  runs {program}  ·  last ran 1h 01m ago")
        );
    }

    for (result, ended) in [
        (1, "with exit code 1"),
        (0x8007_10E0_u32 as i32, "with 0x800710E0"),
    ] {
        f.schedule = Some(task(an_hour_ago, result, Vec::new()));
        let failed = judged(&f);
        assert_eq!(failed.level, Level::Warn);
        assert_eq!(
            failed.detail,
            format!("daily  ·  runs {program}, and its last run, 1h 01m ago, ended {ended}")
        );
        assert!(
            failed.advice.contains("`pitboard renew`"),
            "{}",
            failed.advice
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
        format!(
            "daily  ·  runs {program}, but in Task Scheduler it starts only if the computer \
             is on AC power and is disabled"
        )
    );
    assert_eq!(drifted.advice, again(Os::Windows));
}

/// The way back works from the command line everywhere, and names the app only where
/// there is one.
#[test]
fn a_broken_schedule_is_written_again_by_whatever_this_machine_has() {
    for os in [Os::MacOs, Os::Windows] {
        let with_app = again(os);
        assert!(
            with_app.contains("the app's Settings")
                && with_app.contains("pitboard schedule install"),
            "{os:?}: {with_app}"
        );
    }
    let linux = again(Os::Linux);
    assert!(linux.contains("pitboard schedule install"), "{linux}");
    assert!(!linux.contains("app"), "there is no app here: {linux}");
}

/// What the check above is given, read off a real disk: a schedule written the way
/// `pitboard schedule install` writes it, whose Pitboard is then taken away.
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

/// The schedule's runs are given the proxy variables of the Pitboard that installed it.
/// Where those send requests another way than this run's, doctor says so, with what each
/// goes through and the user name and password as `***`, and says how to give the
/// schedule this run's. Where they go the same way, nothing is said.
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
    assert_eq!(
        (line.level, line.name.as_str(), line.detail.as_str()),
        (
            Level::Warn,
            "renewal schedule's proxy",
            "the schedule was installed with http://***:***@proxy.example.com:3128, from \
             HTTPS_PROXY; this run goes directly"
        )
    );
    assert!(
        line.advice.contains("`pitboard schedule install`"),
        "{}",
        line.advice
    );

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
        "the schedule was installed with no proxy; this run goes through \
         http://127.0.0.1:3128, from ALL_PROXY, NO_PROXY exempting chatgpt.com"
    );

    // A SOCKS proxy, which Pitboard does not use yet, is given, not gone through. Installing
    // the schedule again from here would give it that proxy, and its requests would fail
    // too, so the advice is to change the variable first.
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
        (line.detail.as_str(), line.advice.as_str()),
        (
            "the schedule was installed with no proxy; this run is given \
             socks5h://127.0.0.1:1080, from ALL_PROXY, NO_PROXY exempting chatgpt.com",
            "The schedule's runs are given the proxy variables it was installed with. \
             Installed again from here, they would be given this run's SOCKS proxy, which \
             Pitboard does not use yet, and their requests would fail. Set ALL_PROXY to an \
             HTTP proxy's address, or unset it, then install it again with `pitboard \
             schedule install`, which takes the proxy variables set where it runs."
        )
    );
    // With every host Pitboard calls exempt from it, nothing fails, and installing the
    // schedule again from here is the advice as for any other proxy.
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
    let advice = &check(&checks, "schedule_proxy").advice;
    assert!(
        advice.starts_with(
            "The schedule's runs are given the proxy variables it was installed with. To \
             give them this run's, install it again from here"
        ),
        "{advice}"
    );

    f.network = by(through(None, &[]));
    f.schedule = Some(scheduled(by(through(None, &[])), true));
    assert_eq!(
        check(&evaluate(&f), "schedule_proxy").detail,
        "the schedule was installed with this run's proxy, \
         http://***:***@proxy.example.com:3128, from HTTPS_PROXY, but with another user \
         name or password"
    );
}

/// In the app, whose environment is the system's and names no proxy a shell set, a
/// schedule installed from a terminal with a proxy differs from it for good, and the
/// app's switch would take that proxy away. So the line is no warning there: it says what
/// each goes through, and how each way of installing the schedule chooses its proxy.
#[test]
fn in_the_app_a_schedules_other_proxy_is_said_without_a_warning() {
    let mut f = facts();
    f.in_the_app = true;
    f.schedule = Some(scheduled(direct(), false));
    assert!(evaluate(&f).iter().all(|c| c.code != "schedule_proxy"));

    f.schedule = Some(scheduled(by(through(None, &[])), true));
    let checks = evaluate(&f);
    let line = check(&checks, "schedule_proxy");
    assert_eq!(
        (line.level, line.name.as_str(), line.detail.as_str()),
        (
            Level::Ok,
            "renewal schedule's proxy",
            "the schedule was installed with http://***:***@proxy.example.com:3128, from \
             HTTPS_PROXY; the app goes directly"
        )
    );
    for said in ["Settings", "`pitboard schedule install` in a terminal"] {
        assert!(line.advice.contains(said), "{said}: {}", line.advice);
    }

    f.network = by(through(None, &[]));
    assert_eq!(
        check(&evaluate(&f), "schedule_proxy").detail,
        "the schedule was installed with the app's proxy, \
         http://***:***@proxy.example.com:3128, from HTTPS_PROXY, but with another user \
         name or password"
    );
}

/// What the line above is given, read off a real disk: a schedule `pitboard schedule
/// install` wrote with a proxy, against runs with the same proxy, the same one with
/// another password, and none. The report a person pastes hides the proxy's host, as it
/// does this run's.
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
    assert!(
        detail.ends_with("but with another user name or password"),
        "{detail}"
    );
    for secret in ["alice", "s3cret", "changed"] {
        assert!(!detail.contains(secret), "{secret}: {detail}");
    }

    let direct = run(None);
    let f = gathered(&direct);
    let detail = check(&evaluate(&f), "schedule_proxy").detail.clone();
    assert_eq!(
        detail,
        "the schedule was installed with http://***:***@proxy.example.com:3128, from \
         HTTPS_PROXY; this run goes directly"
    );
    let hidden = redaction_for(&direct, &f).over(&detail);
    assert!(
        hidden.starts_with("the schedule was installed with http://***:***@<proxy ")
            && !hidden.contains("proxy.example.com"),
        "{hidden}"
    );
}
