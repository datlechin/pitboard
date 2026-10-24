use super::{
    Refusal, code, confirm_, enrol, forget_, gave_up, in_use, killed, mixed, park_of, refused, set,
    state_of, tokens_beside_no_session, two_accounts, use_, used, watched, without_items,
};
use super::{data, rested, snapshot, unchanged};
use crate::desktop_testing::EMPTIED;
use crate::error::{Error, Then};
use crate::provider::ProviderId;
use crate::provider::folder::Unfit;
use crate::service::Permit;
use crate::state;
use crate::switch::harness::{Rig, STRANGER, desktop_machine, desktop_pair, hold, recover};
use crate::switch::{Outcome, settle};
use serde_json::Value;

#[test]
fn a_missing_jar_beside_an_item_or_a_key_is_never_signed_out() {
    let m = desktop_machine("no-jar");
    std::fs::remove_file(data(&m).join("Cookies")).expect("the jar taken away by hand");
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unknown");
    assert!(
        refused
            .to_string()
            .contains("has no cookie store and still holds `Cookies-journal`"),
        "{refused}"
    );
    assert_eq!(code(use_(&m, "work")), "login_folder_unknown");
    assert_eq!(
        code(forget_(&m, "work")),
        "login_folder_unknown",
        "doubt rule"
    );
    assert_eq!(snapshot(&m), before);

    // No item at all, and the keys still in the settings.
    let m = desktop_machine("no-items");
    without_items(&m);
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unknown");
    assert!(
        refused
            .to_string()
            .contains("has no cookie store and still holds `oauth:tokenCache`"),
        "{refused}"
    );
    assert_eq!(code(use_(&m, "work")), "login_folder_unknown");
    assert_eq!(snapshot(&m), before);

    // An item 2.42304.0 clears for another account, and no jar.
    let m = desktop_machine("no-jar-preview");
    without_items(&m);
    for key in [
        "oauth:tokenCache",
        "oauth:tokenCacheV2",
        "lastKnownAccountUuid",
    ] {
        set(&m, key, None);
    }
    std::fs::create_dir(data(&m).join("Partitions")).expect("a preview browser");
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert!(
        refused
            .to_string()
            .contains("has no cookie store and still holds `Partitions`"),
        "{refused}"
    );
    assert_eq!(snapshot(&m), before);
}

#[test]
fn a_jar_that_does_not_read_this_time_refuses_every_change() {
    let m = two_accounts("unsettled");
    m.mem.jar_unsettled(&data(&m).join("Cookies"));
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unreadable");
    assert_eq!(refused.exit_code(), 1);
    assert!(
        refused.to_string().contains("(a test held the jar)"),
        "{refused}"
    );
    assert_eq!(code(use_(&m, "home")), "login_folder_unreadable");
    assert_eq!(code(confirm_(&m, "work")), "login_folder_unreadable");
    assert_eq!(
        code(forget_(&m, "work")),
        "login_folder_unreadable",
        "doubt rule"
    );
    assert_eq!(snapshot(&m), before);
}

#[test]
fn nothing_is_read_or_changed_while_claude_runs_or_cannot_be_told() {
    let m = two_accounts("open");
    let claude = m.claude();
    claude.opens();
    let before = snapshot(&m);
    let asked = m.mem.jars_asked().len();
    assert_eq!(code(enrol(&m, "work")), "app_open");
    assert_eq!(code(use_(&m, "home")), "app_open");
    assert_eq!(code(confirm_(&m, "work")), "app_open");
    assert_eq!(
        code(forget_(&m, "work")),
        "app_open",
        "the account in use, by the doubt rule"
    );
    assert_eq!(m.mem.jars_asked().len(), asked, "no jar was opened");
    assert_eq!(snapshot(&m), before);

    claude.crashes();
    m.mem.without_a_process_list();
    assert_eq!(code(enrol(&m, "work")), "app_state_unknown");
    assert_eq!(code(use_(&m, "home")), "app_state_unknown");
    assert_eq!(m.mem.jars_asked().len(), asked);
    assert_eq!(snapshot(&m), before);
}

#[test]
fn a_refusal_names_what_of_claude_still_runs() {
    let m = desktop_machine("still");
    let claude = m.claude();
    claude.opens();
    let refused = enrol(&m, "work").expect_err("refused");
    assert!(
        matches!(&refused, Error::AppOpen { still, .. } if still.is_empty()),
        "{refused:?}"
    );
    assert!(
        refused.to_string().starts_with("Claude is open"),
        "{refused}"
    );

    claude.crashes();
    let ship_it = m
        .ctx_home()
        .join("claude/Claude.app/Contents/Frameworks/Squirrel.framework/Resources/ShipIt");
    m.mem
        .runs_at("ShipIt", &[ship_it.to_string_lossy().as_ref()]);
    let refused = enrol(&m, "work").expect_err("refused");
    let Error::AppOpen { still, .. } = &refused else {
        panic!("{refused:?}");
    };
    assert_eq!(still.len(), 1);
    assert!(still[0].starts_with("ShipIt (pid "), "{still:?}");
    assert_eq!(refused.exit_code(), 1);

    // A Claude that is not the test's own never counts.
    m.mem.runs_at("ShipIt", &[]);
    m.mem.runs_at(
        "Claude",
        &["/Applications/Claude.app/Contents/MacOS/Claude"],
    );
    enrol(&m, "work").expect("the developer's own Claude is not the test's");
}

#[test]
fn a_process_list_that_does_not_read_is_refused_with_why() {
    let m = desktop_machine("listless");
    m.mem.without_a_process_list();
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "app_state_unknown");
    assert!(
        refused
            .to_string()
            .contains("(a test withheld the process list)"),
        "{refused}"
    );
    assert!(!refused.to_string().contains("doctor"), "{refused}");
}

#[test]
fn a_second_home_is_refused_while_another_holds_the_folder() {
    let m = two_accounts("busy");
    let held = crate::host::fs::lock_dir(&data(&m)).expect("another Pitboard holds it");
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_busy");
    assert_eq!(code(use_(&m, "home")), "login_folder_busy");
    assert_eq!(snapshot(&m), before);
    held.unlock().expect("let go");
    enrol(&m, "work").expect("once the other has finished");
}

#[test]
fn a_sidecar_pitboard_cannot_read_refuses_every_change() {
    let m = desktop_machine("sidecar");
    std::fs::write(data(&m).join("config.json.journal"), "not a header\n{}").expect("sidecar");
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unknown");
    assert_eq!(refused.exit_code(), 3);
    assert!(
        refused.to_string().contains("config.json.journal"),
        "{refused}"
    );
    assert_eq!(snapshot(&m), before);
}

// Row 4.
#[test]
fn a_data_folder_pitboard_moves_no_login_in_refuses_every_change() {
    let refuses = |name: &str, spoil: &dyn Fn(&Rig), words: &str| {
        let m = two_accounts(&format!("row-4-{name}"));
        spoil(&m);
        let before = snapshot(&m);
        let refused = enrol(&m, "work").expect_err("refused");
        assert_eq!(
            (refused.code(), refused.exit_code()),
            ("login_folder_unknown", 3),
            "{name}: {refused}"
        );
        assert!(refused.to_string().contains(words), "{refused}");
        assert_eq!(code(use_(&m, "home")), "login_folder_unknown", "{name}");
        assert_eq!(code(confirm_(&m, "work")), "login_folder_unknown", "{name}");
        assert_eq!(
            code(forget_(&m, "work")),
            "login_folder_unknown",
            "{name}: doubt rule"
        );
        assert_eq!(snapshot(&m), before, "{name}");
    };
    refuses(
        "third-party",
        &|m| {
            let beside = m.ctx_home().join("claude/data-3p");
            std::fs::create_dir_all(&beside).expect("a folder beside");
            std::fs::write(beside.join("config.json"), b"{}").expect("Claude's settings there");
        },
        "is not the one Claude runs from: it runs as a third-party deployment, from `Claude-3p` \
         beside it",
    );
    refuses(
        "reset",
        &|m| {
            let asked = br#"{"choice":"restart","tries":0}"#;
            std::fs::write(data(m).join("reset-app-data"), asked).expect("a reset asked for");
        },
        "is waiting for Reset App Data, which deletes what it holds the next time Claude opens, \
         so Pitboard moves no Claude Desktop login. Open Claude, quit it with Command-Q, then run \
         this again.",
    );
    refuses(
        "link",
        &|m| {
            let real = m.ctx_home().join("claude/real");
            std::fs::rename(data(m), &real).expect("moved");
            crate::host::fs::testing::link_dir(&real, &data(m)).expect("linked");
        },
        "data folder is a link",
    );
}

#[test]
fn a_reset_waiting_stops_the_recovery_of_a_move_as_it_stops_a_move() {
    let m = desktop_pair("reset-mid-move");
    killed(&m, "leave.jar_out", "home");
    let asked = br#"{"choice":"restart","tries":0}"#;
    std::fs::write(data(&m).join("reset-app-data"), asked).expect("a reset asked for");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    let Error::LoginFolderUnknown { what, then, .. } = &stopped else {
        panic!("{stopped:?}");
    };
    assert_eq!(what, &Unfit::ResetWaits.to_string());
    assert_eq!(*then, Then::OpenAndQuit);
    unchanged(&m, &before, "a reset waiting");
    assert!(
        !state_of(&m).moving.is_empty(),
        "the move waits for its recovery"
    );

    std::fs::write(data(&m).join("reset-app-data.given-up"), b"").expect("given up");
    used(&m, "home");
    assert!(
        state_of(&m).moving.is_empty(),
        "settled once Claude gave the reset up"
    );
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    hold(&m, "a reset given up");
    rested(&m, "a reset given up");
}

#[test]
fn a_part_of_the_accounts_data_pitboard_has_not_read_stops_the_start_of_a_move_and_not_its_recovery()
 {
    let m = desktop_pair("row-21");
    std::fs::create_dir(data(&m).join("Network")).expect("the jar's other place");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    assert_eq!(stopped.code(), "login_folder_unknown");
    assert_eq!(stopped.exit_code(), 3);
    assert!(stopped.to_string().contains("holds `Network`"), "{stopped}");
    unchanged(&m, &before, "nothing changed");
    assert!(matches!(used(&m, "work").0, Outcome::AlreadyActive { .. }));

    std::fs::remove_dir(data(&m).join("Network")).expect("gone");
    let died = crate::fault::killing("leave.jar_out", || used(&m, "home"));
    assert_eq!(died.expect_err("killed"), "leave.jar_out");
    std::fs::create_dir(data(&m).join("Network")).expect("there again");
    let stopped = refused(&m, "home");
    assert_eq!(
        stopped.code(),
        "login_folder_unknown",
        "the leave was finished first"
    );
    let state = state_of(&m);
    assert!(state.moving.is_empty());
    assert_eq!(in_use(&m), None);
    assert!(park_of(&m, "work").is_some());
    hold(&m, "recovered beside an entry not read");
    rested(&m, "recovered beside an entry not read");
}

#[test]
fn a_leave_is_refused_where_this_mac_cannot_be_identified() {
    let mut m = desktop_pair("no-machine");
    let known = m.ctx.clone();
    m.ctx = m.ctx.clone().with_unknown_machine();
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    assert_eq!(stopped.code(), "machine_unknown");
    assert_eq!(stopped.exit_code(), 3);
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let stopped = pitboard
        .sign_in_watched("desktop/travel")
        .expect_err("refused")
        .error;
    assert_eq!(stopped.code(), "machine_unknown");
    unchanged(&m, &before, "nothing reserved, nothing moved");

    // An arrive alone: Claude signed out, `work` parked where the Mac was known.
    m.ctx = known.clone();
    let died = crate::fault::killing("leave.done", || used(&m, "home"));
    assert_eq!(died.expect_err("killed"), "leave.done");
    m.ctx = known.with_unknown_machine();
    recover(&m).expect("swept");
    let before = snapshot(&m);
    assert_eq!(refused(&m, "home").code(), "machine_unknown");
    unchanged(&m, &before, "nothing changed");
}

#[test]
fn nothing_is_set_aside_where_this_mac_cannot_be_identified() {
    let blind = |m: &mut Rig| {
        m.ctx = m.ctx.clone().with_unknown_machine();
        recover(m).expect("swept");
        snapshot(m)
    };

    // The start of a sign-in, which sets aside the keys a Log out left.
    let mut m = desktop_machine("no-machine-start");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    let before = blind(&mut m);
    let stopped = watched(&m, "travel").expect_err("refused");
    assert_eq!(stopped.code(), "machine_unknown");
    unchanged(&m, &before, "the start of a sign-in");

    // Recovery of an arrive, which sets aside the keys Claude wrote over it.
    let mut m = desktop_pair("no-machine-settle");
    killed(&m, "arrive.jar_in", "home");
    let emptied: Value = serde_json::from_str(EMPTIED).expect("JSON");
    set(&m, "oauth:tokenCache", Some(emptied));
    let before = blind(&mut m);
    assert_eq!(code(confirm_(&m, "work")), "machine_unknown");
    unchanged(&m, &before, "recovery");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());

    // A give-up, which sets aside what the move left beside another session.
    let mut m = mixed("no-machine-give-up");
    let before = blind(&mut m);
    assert_eq!(code(enrol(&m, "third")), "machine_unknown");
    unchanged(&m, &before, "a give-up");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
}

#[test]
fn a_park_from_another_mac_is_refused_before_anything_moves() {
    let m = desktop_pair("another-mac");
    let park = park_of(&m, "home").expect("parked");
    let mut manifest = crate::store::folder::manifest(&m.ctx, &park.service)
        .expect("reads")
        .expect("there");
    manifest.machine = "f".repeat(64);
    crate::store::folder::write_manifest(&m.ctx, Permit::for_a_test(), &park.service, &manifest)
        .expect("written");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    assert_eq!(stopped.code(), "parked_credential_corrupt");
    assert!(
        stopped.to_string().contains("parked on another Mac"),
        "{stopped}"
    );
    unchanged(&m, &before, "`work` did not leave for it");
    assert_eq!(in_use(&m).as_deref(), Some("work"));
}

// A park Pitboard cannot look at, or a file of it it cannot read, is neither missing nor
// another login, so nobody is sent to sign in again for it.
#[test]
fn a_park_that_cannot_be_looked_at_is_never_said_missing() {
    use crate::host::fs::testing;
    let m = desktop_pair("park-unread");
    let park = park_of(&m, "home").expect("parked").service;
    let folder = crate::store::folder::at(&m.ctx, &park);
    let items = folder.join("items");
    recover(&m).expect("swept");
    for (path, what) in [
        (crate::store::folder::parks_dir(&m.ctx), "the park"),
        (items.join("IndexedDB"), "a listed item"),
        (items.clone(), "its items"),
        (items.join("Cookies"), "its cookie store"),
        (folder.join("manifest.json"), "its manifest"),
        (crate::store::folder::keys_file(&m.ctx, &park), "its keys"),
    ] {
        let before = snapshot(&m);
        testing::deny_reading(&path).expect("not looked into");
        let stopped = refused(&m, "home");
        testing::make_private(&path).expect("looked into again");
        assert_eq!(
            stopped.code(),
            "credential_store_unreadable",
            "{what}: {stopped}"
        );
        assert!(
            !stopped.to_string().contains("--sign-in"),
            "{what}: {stopped}"
        );
        unchanged(&m, &before, what);
    }
    used(&m, "home");
}

#[test]
fn a_name_refused_never_settles_a_folder_move() {
    let m = desktop_pair("refused-name");
    killed(&m, "leave.jar_out", "home");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let refused = pitboard.switch_to("no-such-account").expect_err("refused");
    assert!(refused.warnings.is_empty(), "{:?}", refused.warnings);
    assert!(pitboard.renew().is_ok());
    unchanged(&m, &before, "the move was not settled by any of it");
    assert_eq!(
        (m.mem.listings(), m.mem.jars_asked().len()),
        (listings, jars)
    );
}

#[test]
fn an_item_or_its_folder_on_another_volume_refuses_before_anything_moves() {
    let m = desktop_pair("another-volume");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    for at in ["Local Storage", "IndexedDB"] {
        let what = format!("holds `{at}` on another volume");
        let path = data(&m).join(at).to_string_lossy().into_owned();
        let stopped = crate::fault::failing_at("another_volume", &path, || refused(&m, "home"));
        assert_eq!(stopped.code(), "login_folder_unknown", "{at}: {stopped}");
        assert!(stopped.to_string().contains(&what), "{stopped}");
        unchanged(&m, &before, at);
    }

    // A link is followed by nothing: an item, and the folder between one and the root.
    let elsewhere = m.ctx_home().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("a folder outside");
    for linked in ["Session Storage", "IndexedDB"] {
        std::fs::rename(data(&m).join(linked), elsewhere.join(linked)).expect("moved out");
        crate::host::fs::testing::link_dir(&elsewhere.join(linked), &data(&m).join(linked))
            .expect("linked");
        let before = snapshot(&m);
        let stopped = refused(&m, "home");
        assert_eq!(
            stopped.code(),
            "login_folder_unknown",
            "{linked}: {stopped}"
        );
        assert!(
            stopped
                .to_string()
                .contains(&format!("holds `{linked}` as a link")),
            "{stopped}"
        );
        unchanged(&m, &before, linked);
        std::fs::remove_file(data(&m).join(linked)).expect("unlinked");
        std::fs::rename(elsewhere.join(linked), data(&m).join(linked)).expect("back");
    }
    assert!(matches!(used(&m, "home").0, Outcome::Switched { .. }));
    rested(&m, "switched once every item is in place");
}

#[test]
fn every_refusal_leaves_the_folder_and_the_index_as_they_were() {
    let refusals: [Refusal; 15] = [
        Refusal {
            code: "different_volume",
            set_up: |_| {},
            asked: |m| {
                let parks = crate::store::folder::parks_dir(&m.ctx);
                let parks = parks.to_string_lossy();
                crate::fault::failing_at("another_volume", &parks, || refused(m, "home"))
            },
        },
        Refusal {
            code: "parks_not_private",
            set_up: |m| {
                let parks = crate::store::folder::parks_dir(&m.ctx);
                crate::host::fs::testing::allow_changes(&parks).expect("opened");
            },
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "login_folder_busy",
            set_up: |_| {},
            asked: |m| {
                let _held = crate::host::fs::lock_dir(&data(m)).expect("another home");
                refused(m, "home")
            },
        },
        Refusal {
            code: "app_open",
            set_up: |m| m.claude().opens(),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "app_state_unknown",
            set_up: |m| m.mem.without_a_process_list(),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "login_folder_unknown",
            set_up: |m| std::fs::create_dir(data(m).join("Network")).expect("an entry not read"),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "login_folder_unreadable",
            set_up: |m| m.mem.jar_unsettled(&data(m).join("Cookies")),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "sign_in_unfinished",
            set_up: |m| set(m, "lastKnownAccountUuid", None),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "parked_credential_corrupt",
            set_up: |m| {
                let park = park_of(m, "home").expect("parked");
                let mut manifest = crate::store::folder::manifest(&m.ctx, &park.service)
                    .expect("reads")
                    .expect("there");
                manifest.machine = "f".repeat(64);
                crate::store::folder::write_manifest(
                    &m.ctx,
                    Permit::for_a_test(),
                    &park.service,
                    &manifest,
                )
                .expect("written");
            },
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "live_account_not_enrolled",
            set_up: |m| {
                let claude = m.claude();
                claude.opens();
                claude.signs_in(&STRANGER);
                claude.reports();
                claude.quits();
            },
            asked: |m| watched(m, "travel").expect_err("refused"),
        },
        Refusal {
            code: "recovery_undetermined",
            set_up: tokens_beside_no_session,
            asked: |m| confirm_(m, "work").expect_err("refused"),
        },
        Refusal {
            code: "recovery_undetermined",
            set_up: tokens_beside_no_session,
            asked: |m| forget_(m, "home").expect_err("refused"),
        },
        Refusal {
            code: "recovery_elsewhere",
            set_up: |m| {
                killed(m, "leave.jar_out", "home");
                let mut state = state_of(m);
                state.moving.get_mut("desktop").expect("recorded").slot = "/elsewhere".into();
                state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
            },
            asked: |m| confirm_(m, "work").expect_err("refused"),
        },
        Refusal {
            code: "move_mixed",
            set_up: |m| {
                killed(m, "leave.item_out", "home");
                let claude = m.claude();
                claude.opens();
                claude.signs_in(&STRANGER);
                claude.reports();
                claude.quits();
            },
            asked: |m| gave_up(m).expect_err("refused"),
        },
        Refusal {
            code: "uninstall_would_strand",
            set_up: |m| killed(m, "leave.done", "home"),
            asked: |m| {
                let settled = settle(&m.ctx, Permit::for_a_test(), None).expect("settled");
                crate::switch::uninstall(settled.0).expect_err("refused")
            },
        },
    ];
    let parks = |m: &Rig| crate::store::folder::parks_dir(&m.ctx);
    for (n, refusal) in refusals.iter().enumerate() {
        let m = desktop_pair(&format!("refusal-{n}"));
        (refusal.set_up)(&m);
        recover(&m).expect("swept");
        let before = snapshot(&m);
        let jars = m.mem.jars_asked().len();
        let error = (refusal.asked)(&m);
        assert_eq!(error.code(), refusal.code, "{n}: {error}");
        unchanged(&m, &before, refusal.code);
        for jar in &m.mem.jars_asked()[jars..] {
            assert!(
                *jar == data(&m).join("Cookies") || jar.starts_with(parks(&m)),
                "{}: read {}",
                refusal.code,
                jar.display()
            );
        }
    }
}
