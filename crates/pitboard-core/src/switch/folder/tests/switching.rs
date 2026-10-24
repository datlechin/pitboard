use super::{
    code, confirm_, forget_, in_use, killed, named_in_settings, park_of, refused, set, setting,
    state_of, three_tools, two_accounts, use_, used,
};
use super::{data, rested, snapshot, tree, whereabouts};
use crate::desktop_testing::Person;
use crate::permit::Permit;
use crate::provider::ProviderId;
use crate::state::{self, Key, ParkKind, SetAside};
use crate::switch::harness::{HOME, NOW, Rig, WORK, audit_lines, desktop_pair, hold};
use crate::switch::{Outcome, settle, switch};
use crate::warning::Warning;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

#[test]
fn use_of_an_account_with_nothing_parked_is_refused_and_the_one_in_use_is_not_forgotten() {
    let m = two_accounts("use");
    assert!(matches!(
        use_(&m, "work").expect("in use"),
        Outcome::AlreadyActive {
            config_updated: false,
            ..
        }
    ));
    assert!(matches!(
        confirm_(&m, "work").expect("in use"),
        Outcome::AlreadyActive { .. }
    ));
    let before = snapshot(&m);
    assert_eq!(code(confirm_(&m, "home")), "account_not_in_use");
    assert_eq!(code(use_(&m, "home")), "nothing_parked");
    assert_eq!(code(forget_(&m, "work")), "cannot_forget_active_account");
    assert_eq!(snapshot(&m), before);
    forget_(&m, "home").expect("forgotten");
    assert!(
        state::load(&m.ctx)
            .expect("state")
            .get(&m.key("home"))
            .is_none()
    );
    hold(&m, "forgotten");
}

#[test]
fn a_switch_parks_the_account_in_use_and_puts_the_other_in() {
    let m = desktop_pair("switch");
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    assert_eq!(whereabouts(&m, &WORK), ["Claude"]);
    let home_park = park_of(&m, "home").expect("`home` is parked");
    assert_eq!(whereabouts(&m, &HOME), vec![home_park.service.clone()]);
    assert_eq!(home_park.kind, ParkKind::Folder);
    assert!(home_park.session_expires_at.is_some());
    let inode = |path: PathBuf| crate::host::fs::testing::inode(&path).expect("there");
    let jar_before = inode(data(&m).join("Cookies"));

    let (outcome, warnings) = used(&m, "home");
    let Outcome::Switched {
        from,
        to,
        parked,
        adoption,
        ..
    } = outcome
    else {
        panic!("switched: {outcome:?}");
    };
    assert_eq!(
        (from.as_str(), to.as_str()),
        ("desktop/work", "desktop/home")
    );
    assert_eq!(
        adoption,
        crate::provider::Adoption::NextOpen { app: "Claude" }
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    assert_eq!(whereabouts(&m, &HOME), ["Claude"]);
    assert_eq!(whereabouts(&m, &WORK), vec![parked.service.clone()]);
    assert_eq!(named_in_settings(&m).as_deref(), Some(HOME.uuid));
    assert_eq!(
        inode(crate::store::folder::item(
            &m.ctx,
            &parked.service,
            "Cookies"
        )),
        jar_before,
        "moved by rename, never copied"
    );
    assert!(
        !crate::store::folder::at(&m.ctx, &home_park.service).exists(),
        "the emptied park is removed"
    );
    assert!(park_of(&m, "home").is_none());
    hold(&m, "a switch");
    rested(&m, "a switch");

    assert!(matches!(
        used(&m, "home").0,
        Outcome::AlreadyActive {
            config_updated: false,
            ..
        }
    ));
    used(&m, "work");
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    assert_eq!(
        tree(&data(&m).join("Local Storage")),
        tree(&m.ctx_home().join("claude/data/Local Storage"))
    );
    hold(&m, "and back");
    rested(&m, "and back");
}

// What 2.42304.0 clears when another account signs in: a switch hides that change from it,
// so it carries them instead.
#[test]
fn a_switch_carries_what_claude_clears_for_another_account() {
    let m = desktop_pair("carried");
    let preview = data(&m).join("Partitions/launch-preview-static/Cookies");
    std::fs::create_dir_all(preview.parent().expect("a partition")).expect("a partition");
    std::fs::write(&preview, "work's preview").expect("a preview jar");
    std::fs::create_dir(data(&m).join("builtin-mcp-auth")).expect("a connector cache");
    for file in [
        "custom3p-builtin-mcp.json",
        "custom3p-builtin-mcp.json.journal",
        "chrome-import-sites.json",
        "session-popouts.json",
    ] {
        std::fs::write(data(&m).join(file), "{}").expect("a file of work's");
    }
    set(&m, "custom3pMcpOAuth", Some(json!({"grant": "work's"})));
    let carried = [
        "Partitions",
        "builtin-mcp-auth",
        "custom3p-builtin-mcp.json",
        "custom3p-builtin-mcp.json.journal",
        "chrome-import-sites.json",
        "session-popouts.json",
    ];

    // `home`'s park holds none of them, as one an earlier Pitboard made.
    let (_, warnings) = used(&m, "home");
    assert!(warnings.is_empty(), "{warnings:?}");
    for item in carried {
        assert!(!data(&m).join(item).exists(), "`{item}` left with `work`");
    }
    assert_eq!(setting(&m, "custom3pMcpOAuth"), None);
    hold(&m, "a switch");
    rested(&m, "a switch");

    used(&m, "work");
    for item in carried {
        assert!(data(&m).join(item).exists(), "`{item}` back with `work`");
    }
    assert_eq!(
        std::fs::read_to_string(&preview).expect("back"),
        "work's preview"
    );
    assert_eq!(
        setting(&m, "custom3pMcpOAuth"),
        Some(json!({"grant": "work's"}))
    );
    hold(&m, "and back");
    rested(&m, "and back");
}

// A switch Pitboard makes writes no `in-use` line. One into a Claude signed out since writes
// who left and whose only login went, as a read of any other tool would.
#[test]
fn a_use_into_a_signed_out_claude_says_who_left_in_the_activity_log() {
    let m = desktop_pair("use-signed-out");
    let before = audit_lines(&m, "in-use");
    used(&m, "home");
    assert_eq!(audit_lines(&m, "in-use"), before, "Pitboard's own switch");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    used(&m, "work");
    let lines = audit_lines(&m, "in-use");
    assert_eq!(
        lines[before.len()..],
        [
            (String::new(), "signed_out_outside".to_string()),
            ("desktop/home".to_string(), "login_replaced".to_string()),
        ]
    );
}

#[test]
fn the_keys_leave_before_the_jar_and_arrive_after_it() {
    let m = desktop_pair("order");
    set(&m, "custom3pMcpOAuth", Some(json!({"grant": "work's"})));
    used(&m, "home");
    set(&m, "custom3pMcpOAuth", Some(json!({"grant": "home's"})));
    used(&m, "work");
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    // How many of the account's keys the settings hold.
    let look = |at: &'static str| {
        let (seen, root) = (std::rc::Rc::clone(&seen), data(&m));
        move || {
            let settings: Map<String, Value> = serde_json::from_str(
                &std::fs::read_to_string(root.join("config.json")).expect("settings"),
            )
            .expect("JSON");
            let keys = [
                "oauth:tokenCache",
                "oauth:tokenCacheV2",
                "lastKnownAccountUuid",
                "custom3pMcpOAuth",
            ]
            .into_iter()
            .filter(|key| settings.contains_key(*key))
            .count();
            seen.borrow_mut()
                .push((at, keys, root.join("Cookies").exists()));
        }
    };
    // `meanwhile` holds one hook at a time, so each point is looked at in a run of its own.
    for at in [
        "leave.keys_out",
        "leave.jar_out",
        "arrive.jar_in",
        "arrive.keys_in",
    ] {
        crate::fault::meanwhile(at, look(at), || used(&m, "home"));
        used(&m, "work");
    }
    assert_eq!(
        *seen.borrow(),
        [
            ("leave.keys_out", 0, true),
            ("leave.jar_out", 0, false),
            ("arrive.jar_in", 0, true),
            ("arrive.keys_in", 4, true),
        ]
    );
}

#[test]
fn a_session_under_the_recorded_name_is_parked_as_claude_names_it() {
    let m = desktop_pair("as-named");
    let claude = m.claude();
    claude.opens();
    claude.renews(&WORK, "work-session-renewed");
    claude.quits();
    let renewed = Person {
        uuid: WORK.uuid,
        session: "work-session-renewed",
    };

    let (outcome, warnings) = used(&m, "home");
    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
    assert!(
        matches!(
            warnings.as_slice(),
            [Warning::TakenAsNamed { label, .. }] if label == "desktop/work"
        ),
        "{warnings:?}"
    );
    assert!(
        warnings[0]
            .to_string()
            .starts_with("Pitboard had not seen the login Claude held"),
        "{}",
        warnings[0]
    );
    let parked = park_of(&m, "work").expect("parked");
    assert_eq!(parked.login, renewed.login(), "under the fingerprint found");
    assert_eq!(whereabouts(&m, &renewed), [parked.service]);
    assert!(state::load(&m.ctx).expect("state").aside.is_empty());
    hold(&m, "row 16");
    rested(&m, "row 16");
}

#[test]
fn a_login_is_never_moved_over_anything() {
    // `work` holds no `File System`; `home`'s park is made to, so the arrive needs the place.
    let with_a_folder = |name: &str| {
        let m = desktop_pair(name);
        let home_park = park_of(&m, "home").expect("parked");
        let listed = crate::store::folder::item(&m.ctx, &home_park.service, "File System");
        std::fs::create_dir_all(&listed).expect("an item");
        let mut manifest = crate::store::folder::manifest(&m.ctx, &home_park.service)
            .expect("reads")
            .expect("there");
        manifest.items.push("File System".into());
        crate::store::folder::write_manifest(
            &m.ctx,
            Permit::for_a_test(),
            &home_park.service,
            &manifest,
        )
        .expect("written");
        (m, listed)
    };

    // In the way before the arrive starts: its look refuses it, and nothing of `home` moves.
    let (m, listed) = with_a_folder("never-over");
    let in_the_way = data(&m).join("File System");
    let failed = crate::fault::meanwhile(
        "leave.done",
        {
            let in_the_way = in_the_way.clone();
            move || std::fs::write(&in_the_way, b"planted meanwhile").expect("planted")
        },
        || refused(&m, "home"),
    );
    assert_eq!(failed.code(), "switch_left_signed_out", "{failed}");
    assert_eq!(failed.exit_code(), 1);
    assert_eq!(
        std::fs::read(&in_the_way).expect("still there"),
        b"planted meanwhile"
    );
    assert!(listed.is_dir(), "`home`'s item stayed in its park");
    assert_eq!(in_use(&m), None, "Claude is signed out");
    assert!(park_of(&m, "work").is_some() && park_of(&m, "home").is_some());
    assert!(state_of(&m).moving.is_empty());
    hold(&m, "an arrive that did not start");
    std::fs::remove_file(&in_the_way).expect("out of the way");
    assert!(matches!(used(&m, "home").0, Outcome::Installed { .. }));
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    rested(&m, "arrived");

    // An empty folder made after every look, which a plain rename would replace unseen.
    let (m, listed) = with_a_folder("never-over-a-folder");
    let in_the_way = data(&m).join("File System");
    let planted = std::rc::Rc::new(std::cell::Cell::new(None));
    let failed = crate::fault::meanwhile(
        "arrive.recorded",
        {
            let (in_the_way, planted) = (in_the_way.clone(), std::rc::Rc::clone(&planted));
            move || {
                std::fs::create_dir(&in_the_way).expect("planted");
                planted.set(crate::host::fs::testing::inode(&in_the_way).ok());
            }
        },
        || refused(&m, "home"),
    );
    let planted = planted.get().expect("planted");
    assert_eq!(failed.code(), "switch_left_signed_out", "{failed}");
    assert!(
        failed
            .to_string()
            .contains("`File System` could not be moved"),
        "{failed}"
    );
    assert_eq!(
        crate::host::fs::testing::inode(&in_the_way).ok(),
        Some(planted),
        "the folder in the way is the one planted"
    );
    assert!(listed.is_dir(), "`home`'s item stayed in its park");
    assert_eq!(in_use(&m), None, "Claude is signed out");
    assert!(state_of(&m).moving.is_empty());
    hold(&m, "an arrive taken back");
    std::fs::remove_dir(&in_the_way).expect("out of the way");
    assert!(matches!(used(&m, "home").0, Outcome::Installed { .. }));
    rested(&m, "arrived over nothing");
}

#[test]
fn a_claude_3p_folder_claude_does_not_run_from_stops_no_switch() {
    let m = desktop_pair("third-party-not-run");
    let beside = m.ctx_home().join("claude/data-3p");
    std::fs::create_dir_all(&beside).expect("a folder beside");
    let left = br#"{"enterpriseConfig": {}, "_cfprefsMigrated": true}"#;
    let config = beside.join("claude_desktop_config.json");
    std::fs::write(&config, left).expect("an earlier build's");
    assert!(matches!(used(&m, "home").0, Outcome::Switched { .. }));
    std::fs::write(beside.join("config.json"), b"{}").expect("Claude ran from it once");
    std::fs::write(&config, br#"{"deploymentMode": "1p"}"#).expect("claude.ai picked since");
    assert!(matches!(used(&m, "work").0, Outcome::Switched { .. }));
    hold(&m, "beside a `Claude-3p` Claude does not run from");
    rested(&m, "beside a `Claude-3p` Claude does not run from");
}

#[test]
fn a_name_pitboard_does_not_move_stays_where_claude_keeps_it() {
    let m = desktop_pair("names-stay");
    let geometry = br#"{"width":1200,"height":800,"x":0,"y":25,"isMaximized":false}"#;
    std::fs::write(data(&m).join("design"), geometry).expect("Claude Design's window");
    // Issue 271's: an earlier Chromium's store, which Chromium 152 no longer opens.
    for name in ["SharedStorage", "SharedStorage-wal", "SharedStorage-shm"] {
        std::fs::write(data(&m).join(name), name).expect("a store nothing reads");
    }
    std::fs::create_dir(data(&m).join("one-shot-cwd")).expect("a folder of Claude 3's");
    std::fs::write(data(&m).join("session-popouts.json.tmp"), b"").expect("a write cut short");
    let elsewhere = data(&m).join("IndexedDB/https_www.example.com_0.indexeddb.leveldb");
    std::fs::create_dir_all(&elsewhere).expect("another site's store");
    for label in ["home", "work"] {
        assert!(
            matches!(used(&m, label).0, Outcome::Switched { .. }),
            "{label}"
        );
        assert_eq!(in_use(&m).as_deref(), Some(label));
        assert_eq!(
            std::fs::read(data(&m).join("design")).expect("still there"),
            geometry
        );
        for name in ["SharedStorage", "SharedStorage-wal", "SharedStorage-shm"] {
            assert_eq!(
                std::fs::read(data(&m).join(name)).expect(name),
                name.as_bytes()
            );
        }
        assert!(data(&m).join("one-shot-cwd").is_dir(), "{label}");
        assert!(
            data(&m).join("session-popouts.json.tmp").is_file(),
            "{label}"
        );
        assert!(elsewhere.is_dir(), "{label}");
    }
    hold(&m, "names that stay");
    rested(&m, "names that stay");
}

#[test]
fn a_pending_settings_write_of_claudes_survives_a_switch() {
    let m = desktop_pair("sidecar");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&WORK);
    claude.reports();
    claude.quits();
    let sidecar = data(&m).join("config.json.journal");
    let left = std::fs::read(&sidecar).expect("Claude left a settings write");
    assert!(
        std::fs::read_to_string(data(&m).join("config.json"))
            .expect("settings")
            .contains("updaterRunningInstance"),
        "the file still holds what the sidecar takes out"
    );

    used(&m, "home");
    let written = std::fs::read_to_string(data(&m).join("config.json")).expect("settings");
    assert!(
        !written.contains("updaterRunningInstance"),
        "Pitboard wrote the settings as the sidecar had them: {written}"
    );
    assert!(written.contains(HOME.uuid));
    assert_eq!(
        std::fs::read(&sidecar).expect("the sidecar stays"),
        left,
        "a settings write deletes nothing of Claude's"
    );
    hold(&m, "a switch over a sidecar");
    rested(&m, "a switch over a sidecar");
}

#[test]
fn an_arrive_into_a_folder_with_no_indexeddb_makes_its_parent() {
    let m = desktop_pair("no-indexeddb");
    let died = crate::fault::killing("leave.done", || used(&m, "home"));
    assert_eq!(died.expect_err("killed"), "leave.done");
    assert!(data(&m).join("IndexedDB").is_dir(), "the parent stays");
    std::fs::remove_dir(data(&m).join("IndexedDB")).expect("as a new folder has none");
    assert!(matches!(used(&m, "home").0, Outcome::Installed { .. }));
    assert!(
        data(&m)
            .join("IndexedDB/https_claude.ai_0.indexeddb.leveldb/000003.log")
            .is_file()
    );
    assert!(crate::host::fs::testing::is_private(&data(&m).join("IndexedDB")).expect("its mode"));
    rested(&m, "arrived");
}

#[test]
fn an_item_in_neither_place_is_gone_on_without_and_said() {
    let m = desktop_pair("item-gone");
    let gone = data(&m).join("Session Storage");
    let (_, warnings) = crate::fault::meanwhile(
        "leave.recorded",
        move || std::fs::remove_dir_all(&gone).expect("gone meanwhile"),
        || used(&m, "home"),
    );
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            Warning::ItemGone { item, label, .. }
                if item == "Session Storage" && label == "desktop/work"
        )),
        "{warnings:?}"
    );
    let park = park_of(&m, "work").expect("parked");
    let manifest = crate::store::folder::manifest(&m.ctx, &park.service)
        .expect("reads")
        .expect("there");
    assert!(!manifest.items.contains(&"Session Storage".to_string()));
    used(&m, "work");
    rested(&m, "back without it");
}

// A leave whose jar would not move is settled at once and undone.
#[test]
fn a_leave_that_fails_partway_is_undone_and_its_account_stays_in_use() {
    let m = desktop_pair("leave-undone");
    let keys = |m: &Rig| {
        let path = data(m).join("config.json");
        let settings: Map<String, Value> =
            serde_json::from_str(&std::fs::read_to_string(path).expect("settings")).expect("JSON");
        [
            "oauth:tokenCache",
            "oauth:tokenCacheV2",
            "lastKnownAccountUuid",
        ]
        .map(|key| settings.get(key).cloned())
    };
    let held = keys(&m);
    let undone = crate::fault::failing_nth("move_in", 1, || refused(&m, "home"));
    assert_eq!(undone.code(), "move_undone", "{undone}");
    assert!(
        undone.to_string().contains("`desktop/work` is in use"),
        "{undone}"
    );
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    assert_eq!(keys(&m), held, "its keys are back");
    assert_eq!(whereabouts(&m, &WORK), ["Claude"]);
    assert!(state_of(&m).moving.is_empty());
    hold(&m, "a leave undone");
    rested(&m, "a leave undone");
}

// What Claude left after a Log out, set aside before the arrive failed and was undone, is
// said beside the refusal, whether only its keys or all of it had gone.
#[test]
fn an_arrive_undone_at_once_says_what_it_set_aside() {
    for nth in [1, 7] {
        let m = desktop_pair(&format!("arrive-undone-{nth}"));
        let claude = m.claude();
        claude.opens();
        claude.logs_out();
        claude.quits();
        let lines = audit_lines(&m, "in-use");
        let mut stopped = crate::fault::failing_nth("move_in", nth, || refused(&m, "home"));
        assert_eq!(stopped.code(), "switch_left_signed_out", "{nth}: {stopped}");
        // Saved with the arrive's record, whatever the arrive came to.
        assert_eq!(
            audit_lines(&m, "in-use")[lines.len()..],
            [
                (String::new(), "signed_out_outside".to_string()),
                ("desktop/work".to_string(), "login_replaced".to_string()),
            ],
            "{nth}"
        );
        let said = stopped.take_warnings();
        let aside = state_of(&m)
            .aside
            .iter()
            .find(|aside| aside.why == SetAside::InTheWay)
            .map(|aside| crate::store::folder::at(&m.ctx, &aside.name))
            .expect("set aside");
        assert!(
            said.iter().any(|warning| matches!(
                warning,
                Warning::SetAside { why: SetAside::InTheWay, path, .. } if *path == aside
            )),
            "{nth}: {said:?}"
        );
        assert!(
            !said
                .iter()
                .any(|warning| matches!(warning, Warning::MoveSettled(_))),
            "{nth}: the refusal says it was undone: {said:?}"
        );
        rested(&m, "an arrive undone at once");
    }
}

#[test]
fn a_flush_that_fails_after_a_rename_is_a_move_made() {
    let m = desktop_pair("not-synced");
    let (outcome, _) = crate::fault::failing("move_in.flushed", || used(&m, "home"));
    assert!(matches!(outcome, Outcome::Switched { .. }));
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    rested(&m, "moved, not synced");
}

#[test]
fn a_folder_move_waiting_does_not_stop_a_claude_code_switch() {
    let m = three_tools("three-tools");
    killed(&m, "leave.jar_out", "home");
    let waiting = state_of(&m).moving(ProviderId::Desktop).cloned();
    let folder = tree(&data(&m));
    let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
    let without = |state: &mut state::State| {
        state.moving.clear();
    };
    let as_if_none = || {
        let mut state = state_of(&m);
        without(&mut state);
        state
    };

    let ledger = crate::duty::autoswitch::Ledger::load(&m.ctx);
    let judged = |state: &state::State| {
        let here = state
            .get(&Key::new(ProviderId::Claude, "here"))
            .expect("enrolled");
        format!(
            "{:?}",
            crate::duty::autoswitch::judge(
                &m.ctx,
                state,
                here,
                &ledger,
                crate::duty::autoswitch::Threshold::DEFAULT,
                NOW
            )
        )
    };
    assert_eq!(judged(&state_of(&m)), judged(&as_if_none()));
    let was = state_of(&m).in_use(ProviderId::Claude).cloned();

    let (settled, _) =
        settle(&m.ctx, Permit::for_a_test(), Some(ProviderId::Claude)).expect("nothing waits");
    let (outcome, _) = switch(settled, &Key::new(ProviderId::Claude, "there")).expect("switched");
    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
    let state = state_of(&m);
    assert_eq!(
        state
            .account_in_use(ProviderId::Claude)
            .map(|a| a.label.clone()),
        Some("there".into())
    );

    // `here`, parked by that switch, due: it is renewed as with no move waiting.
    let mut due = state_of(&m);
    let here = Key::new(ProviderId::Claude, "here");
    let park = due
        .accounts
        .iter_mut()
        .find(|a| a.is(&here))
        .and_then(|a| a.parked.as_mut())
        .expect("parked");
    park.access_expires_at = Some(NOW - 1);
    state::save(&m.ctx, Permit::for_a_test(), &due).expect("saved");
    m.api.renews(
        "here-refresh",
        crate::provider::claude::api::Renewed {
            access_token: "renewed-access".into(),
            refresh_token: Some("here-renewed".into()),
            expires_in: 3600,
            refresh_token_expires_in: Some(30 * 86_400),
            scopes: None,
            at: None,
            owner: None,
        },
    );
    let renewed = crate::switch::renew::renew_due(
        &m.ctx,
        Permit::for_a_test(),
        crate::switch::renew::Due::ToBeAsked,
    );
    assert!(
        matches!(renewed.as_slice(), [(key, renewal)] if *key == here && renewal.code() == "renewed"),
        "{renewed:?}"
    );

    // A record that says `here` while `there` is stored: the read is recorded.
    let found = state.in_use(ProviderId::Claude).cloned().expect("recorded");
    let mut stale = state_of(&m);
    stale
        .in_use
        .insert("claude".into(), was.expect("`here` was in use"));
    state::save(&m.ctx, Permit::for_a_test(), &stale).expect("saved");
    crate::switch::identify::record_read(
        &m.ctx,
        Permit::for_a_test(),
        &[(ProviderId::Claude, found.clone())],
        &std::collections::BTreeMap::new(),
    );
    let recorded = state_of(&m).in_use(ProviderId::Claude).cloned();
    assert!(
        recorded.is_some_and(|recorded| recorded.agrees(ProviderId::Claude, &found)),
        "the read was recorded"
    );
    assert_eq!(state_of(&m).moving(ProviderId::Desktop).cloned(), waiting);
    assert_eq!(
        tree(&data(&m)),
        folder,
        "Claude's folder as the kill left it"
    );
    assert_eq!(
        (m.mem.listings(), m.mem.jars_asked().len()),
        (listings, jars),
        "nothing of Claude was read"
    );
}
