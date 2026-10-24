use super::*;

#[test]
fn an_interrupted_move_and_a_parks_folder_on_another_volume_are_said() {
    let mut state = State::default();
    state.accounts.push(desktop_account("work", None));
    state.moving.insert(
        "desktop".into(),
        crate::state::Moving {
            half: crate::state::Half::Leave,
            started_at: NOW - 300,
            id: "desktop_work".into(),
            label: "work".into(),
            park: "pitboard-park-desktop_work-1".into(),
            login: "f0".into(),
            items: vec!["Cookies".into()],
            keys: std::collections::BTreeMap::new(),
            asides: std::collections::BTreeMap::new(),
            slot: String::new(),
            awaited: None,
        },
    );
    let mut facts = desktop();
    facts.one_volume = Some(false);
    let checks = judge_desktop(Os::MacOs, &facts, &state, &[], NOW, &time::moment);
    let moved = said(&checks, "desktop_move");
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert_eq!(moved[0].0, Level::Warn);
    assert!(
        moved[0]
            .1
            .starts_with("a move of `desktop/work`'s login was interrupted"),
        "{moved:?}"
    );
    assert_eq!(
        said(&checks, "desktop_parks_folder"),
        [(
            Level::Fail,
            "/Users/dana/.pitboard/parks is on another volume than /Users/dana/Library/\
             Application Support/Claude"
                .into()
        )]
    );

    state.moving.clear();
    facts.one_volume = Some(true);
    let checks = judge_desktop(Os::MacOs, &facts, &state, &[], NOW, &time::moment);
    assert!(said(&checks, "desktop_move").is_empty());
    assert_eq!(said(&checks, "desktop_parks_folder")[0].0, Level::Ok);
}

#[test]
fn claude_desktops_checks_say_what_the_index_and_the_settings_say() {
    use crate::provider::folder::{Sidecar, Unfit, Unread};
    let mut state = State::default();
    state.accounts.push(desktop_account("work", None));
    state.accounts.push(desktop_account(
        "home",
        Some(folder_park("home-1", Some(NOW + 90_000))),
    ));
    state.accounts.push(desktop_account(
        "old",
        Some(folder_park("old-1", Some(NOW - 5))),
    ));
    state
        .accounts
        .push(desktop_account("lost", Some(folder_park("lost-1", None))));
    state.accounts.push(desktop_account("none", None));
    let work = crate::in_use::InUse {
        owner: Some(state.accounts[0].owner()),
        login: "f0".into(),
        known_at: NOW - 60,
        named: Some("desktop_work".into()),
    };
    state.in_use.insert("desktop".into(), work.clone());
    let parks: Vec<ParkFact> = state
        .accounts
        .iter()
        .map(|a| ParkFact {
            provider: ProviderId::Desktop,
            name: format!("desktop/{}", a.label),
            active: a.label == "work",
            last_used_at: None,
            park: a.parked.clone(),
            unreadable: None,
        })
        .collect();
    let mut facts = desktop();
    facts.known = Some(crate::in_use::Known {
        last: Some(work),
        doubt: None,
        own_record: Some(state.accounts[0].owner()),
    });
    facts.parked_here = vec![("home-1".into(), Ok(true)), ("old-1".into(), Ok(true))];
    facts.build = Some("2.31226.1".into());
    let checks = judge_desktop(Os::MacOs, &facts, &state, &parks, NOW, &time::moment);
    let codes: Vec<&str> = checks.iter().map(|c| c.code).collect();
    assert_eq!(
        codes,
        [
            "desktop_data_folder",
            "desktop_settings",
            "desktop_layout",
            "desktop_login",
            "desktop_parked_login",
            "desktop_parked_login",
            "desktop_parked_login",
            "desktop_parked_login",
            "desktop_parked_login",
            "desktop_parks_folder",
            "desktop_build",
        ]
    );
    assert_eq!(
        said(&checks, "desktop_parked_login"),
        [
            (Level::Ok, "signed in; parked when you switch away".into()),
            (Level::Ok, "parked, sign-in may lapse in 1d 1h".into()),
            (Level::Warn, "parked, sign-in may have lapsed".into()),
            (
                Level::Fail,
                "its parked login is missing from /Users/dana/.pitboard/parks".into()
            ),
            (Level::Warn, "nothing parked to switch to".into()),
        ]
    );
    assert_eq!(
        said(&checks, "desktop_settings"),
        [(Level::Ok, "config.json read".into())]
    );
    let login = said(&checks, "desktop_login");
    assert!(
        login[0]
            .1
            .starts_with("`desktop/work`, as Pitboard last read it"),
        "{login:?}"
    );
    assert_eq!(
        said(&checks, "desktop_build")[0].1,
        "Claude 2.31226.1; Pitboard's facts were read from 2.19675.0, 2.7032.0, \
         2.42304.0 and 3.6556.2, and it checks the folder before each move, not the build"
    );
    assert!(checks.iter().all(|c| !c.code.contains("dormant")));

    facts.unread = vec!["CdmStorage.db-wal".into(), "Cookies-wal".into()];
    facts.settings = Some(Err(Unread::Unknown {
        what: "has a settings file, `config.json`, that is not valid JSON".into(),
        then: crate::error::Then::CopyAside,
    }));
    facts.private = Some(crate::host::Private::Shared);
    facts.signed_out = true;
    facts.build = None;
    let checks = judge_desktop(Os::MacOs, &facts, &state, &parks, NOW, &time::moment);
    assert_eq!(
        said(&checks, "desktop_layout"),
        [(
            Level::Warn,
            "holds `CdmStorage.db-wal` and `Cookies-wal`, parts of the account's data this \
             Pitboard has not read"
                .into()
        )]
    );
    let settings = checks
        .iter()
        .find(|c| c.code == "desktop_settings")
        .unwrap();
    assert_eq!(settings.level, Level::Fail);
    assert!(
        settings.advice.starts_with("Claude resets a settings file"),
        "{}",
        settings.advice
    );
    assert!(
        settings.advice.ends_with("then run this again."),
        "{}",
        settings.advice
    );
    assert_eq!(said(&checks, "desktop_parks_folder")[0].0, Level::Fail);
    assert_eq!(
        check(&checks, "desktop_parks_folder").advice,
        "Run `chmod 700 /Users/dana/.pitboard/parks`."
    );
    assert!(
        said(&checks, "desktop_login")[0]
            .1
            .contains("looks signed out")
    );
    assert_eq!(
        said(&checks, "desktop_build")[0].1,
        "not found in /Applications"
    );

    for (unfit, detail, advice) in [
        (
            Unfit::Link,
            "/Users/dana/Library/Application Support/Claude is a link",
            "Pitboard moves no Claude Desktop login through a link.",
        ),
        (
            Unfit::ThirdParty("Claude-3p"),
            "/Users/dana/Library/Application Support/Claude is not the one Claude runs \
             from: it runs as a third-party deployment, from `Claude-3p` beside it",
            "Pitboard moves no Claude Desktop login for a third-party deployment.",
        ),
        (
            Unfit::ResetWaits,
            "/Users/dana/Library/Application Support/Claude is waiting for Reset App Data, \
             which deletes what it holds the next time Claude opens",
            "Open Claude to let the reset finish, then quit it with Command-Q.",
        ),
    ] {
        facts.refused = Some(unfit);
        let checks = judge_desktop(Os::MacOs, &facts, &state, &parks, NOW, &time::moment);
        let folder = checks
            .iter()
            .find(|c| c.code == "desktop_data_folder")
            .unwrap();
        assert_eq!(
            (folder.level, folder.detail.as_str(), folder.advice.as_str()),
            (Level::Fail, detail, advice)
        );
    }
    facts.refused = None;

    facts.settings = Some(Ok((Sidecar::Valid, true)));
    facts.root = Err("`HOME` names another folder than this account's own".into());
    facts.own_home = Some(PathBuf::from("/Users/dana"));
    let checks = judge_desktop(Os::MacOs, &facts, &state, &parks, NOW, &time::moment);
    let folder = checks
        .iter()
        .find(|c| c.code == "desktop_data_folder")
        .unwrap();
    assert_eq!(
        (folder.level, folder.detail.as_str(), folder.advice.as_str()),
        (
            Level::Fail,
            "`HOME` names another folder than this account's own",
            "Run Pitboard with `HOME` unset, or set to /Users/dana."
        )
    );
    assert!(said(&checks, "desktop_settings").is_empty());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn claude_desktops_facts_come_from_one_read_of_its_settings() {
    use crate::provider::folder::Sidecar;
    let m = switch::harness::desktop_machine("doctor-facts");
    let state = crate::state::load(&m.ctx).expect("state");
    let (facts, reads) =
        crate::fault::counting("settings.read", || desktop_facts(&m.ctx, &state, None));
    let facts = facts.expect("an account is enrolled");
    assert_eq!(reads, 1);
    assert_eq!(facts.settings, Some(Ok((Sidecar::None, true))));
    let known = facts.known.expect("judged");
    assert_eq!(known.doubt, None);
    assert_eq!(
        known.own_record.map(|owner| owner.account_uuid).as_deref(),
        Some(switch::harness::WORK.uuid)
    );
    assert!(!facts.signed_out);

    std::fs::remove_file(m.ctx_home().join("claude/data/config.json")).expect("removed");
    let facts = desktop_facts(&m.ctx, &state, None).expect("an account is enrolled");
    assert_eq!(facts.settings, Some(Ok((Sidecar::None, false))));
    let checks = judge_desktop(
        Os::MacOs,
        &facts,
        &state,
        &[],
        switch::harness::NOW,
        &time::moment,
    );
    assert_eq!(
        said(&checks, "desktop_settings"),
        [(Level::Ok, "no config.json yet".into())]
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn doctor_says_the_build_of_the_claude_installed() {
    let mut m = switch::harness::desktop_machine("doctor-build");
    m.ctx.os = Os::MacOs;
    m.mem.app_at(
        std::path::Path::new("/Applications/Claude.app"),
        "2.31226.1",
    );
    let diagnosis = run(&m.ctx, &time::moment);
    let build = diagnosis
        .checks
        .iter()
        .find(|c| c.code == "desktop_build")
        .expect("the build is said");
    assert!(
        build.detail.starts_with("Claude 2.31226.1; "),
        "{}",
        build.detail
    );
    assert_eq!(diagnosis.environment["desktop"]["build"], "2.31226.1");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_park_doctor_cannot_look_at_is_never_said_missing() {
    let m = switch::harness::desktop_pair("doctor-park-unread");
    let state = crate::state::load(&m.ctx).expect("state");
    let parks_dir = store::folder::parks_dir(&m.ctx);
    crate::host::fs::testing::deny_reading(&parks_dir).expect("parks shut");
    let facts = desktop_facts(&m.ctx, &state, None);
    crate::host::fs::testing::make_private(&parks_dir).expect("parks open again");
    let facts = facts.expect("an account is enrolled");
    let parks = park_facts(&m.ctx, &state);
    let checks = judge_desktop(
        Os::MacOs,
        &facts,
        &state,
        &parks,
        switch::harness::NOW,
        &time::moment,
    );
    let home = checks
        .iter()
        .find(|c| c.code == "desktop_parked_login" && c.name == "account desktop/home")
        .map(|c| (c.level, c.detail.clone(), c.advice.clone()))
        .expect("home's park is checked");
    assert_eq!(home.0, Level::Warn, "{home:?}");
    assert!(
        home.1.starts_with(&format!(
            "not read: Pitboard could not look at it in {} (",
            parks_dir.display()
        )),
        "{home:?}"
    );
    assert!(!home.2.contains("--sign-in"), "{home:?}");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_data_folder_is_found_where_only_a_folder_set_aside_is_left() {
    use crate::state::{Aside, SetAside};
    let m = switch::harness::desktop_pair("doctor-aside-only");
    let mut state = crate::state::load(&m.ctx).expect("state");
    state
        .accounts
        .retain(|account| account.provider() != ProviderId::Desktop);
    state.aside.push(Aside {
        name: "pitboard-aside-1".into(),
        tool: ProviderId::Desktop,
        at: NOW,
        why: SetAside::Orphan,
        account: None,
        login: None,
    });
    let facts =
        desktop_facts(&m.ctx, &state, None).expect("a folder set aside shows Claude Desktop");
    let parks = park_facts(&m.ctx, &state);
    let checks = judge_desktop(
        Os::MacOs,
        &facts,
        &state,
        &parks,
        switch::harness::NOW,
        &time::moment,
    );
    let folder = checks
        .iter()
        .find(|c| c.code == "desktop_data_folder")
        .map(|c| (c.level, c.detail.clone()))
        .expect("the data folder is checked");
    assert_eq!(folder.0, Level::Ok, "{folder:?}");
}

#[test]
fn a_folder_set_aside_warns_only_where_its_account_holds_nothing_else() {
    use crate::state::{Aside, SetAside};
    let aside = |name: &str, why, account: Option<&str>, login: Option<&str>| Aside {
        name: name.into(),
        tool: ProviderId::Desktop,
        at: NOW - 3_600,
        why,
        account: account.map(str::to_owned),
        login: login.map(str::to_owned),
    };
    let mut state = State::default();
    state.accounts.push(desktop_account("work", None));
    state
        .accounts
        .push(desktop_account("home", Some(folder_park("home-2", None))));
    state.accounts.push(desktop_account("lost", None));
    state.in_use.insert(
        "desktop".into(),
        crate::in_use::InUse {
            owner: Some(state.accounts[0].owner()),
            login: "f".into(),
            known_at: NOW,
            named: None,
        },
    );
    let set_aside = |state: &State| {
        let checks = judge_desktop(Os::MacOs, &desktop(), state, &[], NOW, &time::moment);
        said(&checks, "desktop_set_aside")
    };
    assert!(set_aside(&state).is_empty(), "none set aside, nothing said");

    state
        .aside
        .push(aside("pitboard-aside-1", SetAside::InTheWay, None, None));
    let none = set_aside(&state);
    assert_eq!(none[0].0, Level::Ok);
    assert_eq!(
        none[0].1,
        "1 in /Users/dana/.pitboard/parks, none holding a login; they hold only what Claude \
         made while signed out"
    );

    // Parts of an account's login: its earlier keys, or what a give-up left.
    state.aside.push(aside(
        "home-0",
        SetAside::Superseded,
        Some("desktop_home"),
        None,
    ));
    state.aside.push(aside(
        "work-0",
        SetAside::GivenUp,
        Some("desktop_work"),
        None,
    ));
    state.aside.push(aside(
        "gone-0",
        SetAside::GivenUp,
        Some("desktop_gone"),
        None,
    ));
    let parts = set_aside(&state);
    assert_eq!(parts[0].0, Level::Ok);
    for said in [
        "4 in /Users/dana/.pitboard/parks, none holding a login",
        "parts of `desktop/home`'s login, set aside",
        "parts of `desktop/work`'s login, set aside",
        "parts of a login no account here holds, set aside",
        "what Claude made while signed out",
    ] {
        assert!(parts[0].1.contains(said), "{said}: {parts:?}");
    }
    assert!(!parts[0].1.contains("only what Claude made"), "{parts:?}");
    state.aside.truncate(1);

    state.aside.push(aside(
        "home-1",
        SetAside::Superseded,
        Some("desktop_home"),
        Some("a"),
    ));
    state.aside.push(aside(
        "work-1",
        SetAside::Superseded,
        Some("desktop_work"),
        Some("b"),
    ));
    let kept = set_aside(&state);
    assert_eq!(
        kept[0].0,
        Level::Ok,
        "its account holds a newer park, or is in use"
    );
    assert!(
        kept[0]
            .1
            .starts_with("3 in /Users/dana/.pitboard/parks, 2 holding a login: ")
    );
    assert!(
        kept[0]
            .1
            .contains("`desktop/home`'s login from before a sign-in again, set aside"),
        "{kept:?}"
    );
    assert!(
        kept[0]
            .1
            .ends_with("; the rest hold what Claude made while signed out"),
        "{kept:?}"
    );

    state.aside.push(aside(
        "lost-1",
        SetAside::AnotherMachine,
        Some("desktop_lost"),
        Some("c"),
    ));
    state
        .aside
        .push(aside("x-1", SetAside::Orphan, None, Some("d")));
    let warned = set_aside(&state);
    assert_eq!(warned[0].0, Level::Warn);
    assert!(
        warned[0]
            .1
            .contains("`desktop/lost`'s login from another Mac"),
        "{warned:?}"
    );
    assert!(
        warned[0]
            .1
            .contains("a login no account here holds, set aside"),
        "{warned:?}"
    );
}

// Claude's settings name an account since Pitboard last read Claude signed out: doctor
// says it as the read's warning does.
#[test]
fn claude_desktop_named_since_signed_out_reads_as_the_warning_does() {
    let mut state = State::default();
    state.accounts.push(desktop_account("work", None));
    let nobody = crate::in_use::InUse {
        owner: None,
        login: String::new(),
        known_at: NOW - 60,
        named: None,
    };
    state.in_use.insert("desktop".into(), nobody.clone());
    let mut facts = desktop();
    facts.known = Some(crate::in_use::Known {
        last: Some(nobody),
        doubt: Some(crate::in_use::Doubt::NamedMoved),
        own_record: Some(state.accounts[0].owner()),
    });
    let checks = judge_desktop(Os::MacOs, &facts, &state, &[], NOW, &time::moment);
    let login = said(&checks, "desktop_login");
    let warned = crate::warning::Warning::InUseUnconfirmed {
        tool: ProviderId::Desktop,
        last: Stored::Nothing,
        named: Stored::Account("desktop/work".into()),
        asked: true,
    }
    .to_string();
    assert_eq!(
        login[0].1,
        words::desktop_named(
            &Stored::Account("desktop/work".into()),
            &Stored::Nothing,
            |stored| stored.said("nobody")
        )
    );
    assert!(warned.starts_with(&login[0].1), "{warned}");
}

#[test]
fn only_macos_is_asked_for_claude_desktops_build() {
    let memory = crate::host::memory::MemoryHost::new();
    memory.app_at(std::path::Path::new("/Applications/Claude.app"), "2.7032.0");
    for (os, build) in [
        (Os::MacOs, Some("2.7032.0")),
        (Os::Linux, None),
        (Os::Windows, None),
    ] {
        let env = crate::context::Environment::on(
            os,
            [("HOME", "/nowhere"), ("USERPROFILE", "/nowhere")],
        );
        let ctx = Context::for_command_line(&env).with_memory_stores(memory.clone());
        assert_eq!(desktop_build(&ctx).as_deref(), build, "{os:?}");
    }
}

// An index brought from a Mac.
#[test]
fn a_claude_desktop_account_in_an_index_off_macos_is_a_note() {
    let mut state = State::default();
    state.accounts.push(desktop_account("work", None));
    state
        .accounts
        .push(desktop_account("home", Some(folder_park("home-1", None))));
    let mut f = facts();
    f.desktop = Some(DesktopFacts {
        root: Err("Pitboard switches Claude Desktop accounts on macOS only".into()),
        ..desktop()
    });
    f.state = Ok(state.clone());
    for os in [Os::Windows, Os::Linux] {
        f.os = os;
        let desktops: Vec<(&str, Level, String, String)> = evaluate(&f)
            .into_iter()
            .filter(|c| section(c.code) == Some(ProviderId::Desktop))
            .map(|c| (c.code, c.level, c.detail, c.advice))
            .collect();
        assert_eq!(
            desktops,
            [(
                "desktop_data_folder",
                Level::Ok,
                "Pitboard switches Claude Desktop accounts on macOS only".to_string(),
                "This index holds `desktop/work` and `desktop/home`. Nothing here changes \
                 them."
                    .to_string()
            )],
            "{os:?}"
        );
    }

    f.desktop = Some(desktop());
    assert_eq!(
        check(&evaluate(&f), "desktop_data_folder").detail,
        "/Users/dana/Library/Application Support/Claude",
        "a scratch data folder on Linux is read as on macOS"
    );

    state.accounts.clear();
    assert_eq!(
        judge_desktop_off_macos(&state).advice,
        "This index holds folders set aside for Claude Desktop. Nothing here changes them."
    );
}
