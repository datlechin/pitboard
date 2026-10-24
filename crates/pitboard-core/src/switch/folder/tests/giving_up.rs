use super::{
    beside_desktop, claudes, code, confirm_, enrol, forget_, gave_up, in_the_way, in_use,
    interrupted, killed, left_by_a_give_up, list_left, listed_left, mixed, names, park_of,
    session_of, set, state_of, switch_given_up_beside, three_tools, tokens_beside_no_session, use_,
    used, watched,
};
use super::{data, rested, settled, snapshot, tree, unchanged, whereabouts};
use crate::desktop_testing::{Person, write_stand_in_jar};
use crate::error::Error;
use crate::provider::ProviderId;
use crate::service::{Permit, Warning};
use crate::state::{self, SetAside};
use crate::switch::harness::{HOME, Rig, STRANGER, WORK, desktop_pair, hold};
use crate::switch::repair;
use serde_json::{Map, Value};
use std::path::PathBuf;

#[test]
fn a_custom_claude_code_endpoint_never_stops_a_claude_desktop_give_up() {
    let mut m = desktop_pair("custom-endpoint");
    tokens_beside_no_session(&m);
    m.ctx.custom_oauth = true;
    let gave = gave_up(&m).expect("no journal, so the endpoint is not asked");
    m.ctx.custom_oauth = false;
    assert_eq!(gave.moves.len(), 1, "{gave:?}");
    assert!(state_of(&m).moving.is_empty());
}

#[test]
fn abandon_settles_a_move_it_can_before_giving_up_any() {
    let m = desktop_pair("abandon-settles");
    killed(&m, "leave.keys_out", "home");
    let gave = gave_up(&m).expect("settled");
    assert!(gave.switch.is_none() && gave.moves.is_empty());
    assert!(
        matches!(
            gave.warnings.as_slice(),
            [Warning::MoveSettled(moved)]
                if moved.ended == crate::switch::folder::Ended::Undone && moved.half == state::Half::Leave
        ),
        "{:?}",
        gave.warnings
    );
    assert_eq!(gave.warnings[0].code(), "interrupted_move_undone");
    assert_eq!(
        gave.warnings[0].to_string(),
        "an earlier move of `desktop/work`'s login was interrupted; Pitboard undid it, and \
         `desktop/work` is in use"
    );
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    rested(&m, "settled by abandon");
    let nothing = gave_up(&m).expect("nothing waits");
    assert!(nothing.switch.is_none() && nothing.moves.is_empty() && nothing.warnings.is_empty());
}

// A give-up reads no settings, which it never writes, and a jar or an item it cannot read is
// refused with its own way out, never with a sentence that sends the person back to `abandon`.
#[test]
fn abandon_gives_up_whatever_the_settings_and_says_why_a_jar_or_an_item_stops_it() {
    use crate::host::fs::testing;
    let m = desktop_pair("abandon-settings");
    killed(&m, "leave.jar_out", "home");
    std::fs::write(data(&m).join("config.json"), "[]").expect("settings not read");
    let gave = gave_up(&m).expect("given up");
    assert_eq!(gave.moves.len(), 1, "{gave:?}");
    assert!(state_of(&m).moving.is_empty());

    let m = desktop_pair("abandon-jar");
    killed(&m, "leave.keys_out", "home");
    write_stand_in_jar(&data(&m).join("Cookies"), 25, &session_of(Some(&WORK)));
    let stopped = gave_up(&m).expect_err("refused");
    assert_eq!(stopped.code(), "login_folder_unknown", "{stopped}");
    let said = stopped.to_string();
    assert!(said.contains("Update Pitboard"), "{said}");
    assert!(!said.contains("pitboard abandon"), "{said}");
    // The first give-up swept what the kill left; one refused moves and indexes nothing.
    let before = snapshot(&m);
    let again = gave_up(&m).expect_err("refused again");
    assert_eq!(again.code(), "login_folder_unknown", "{again}");
    unchanged(&m, &before, "a jar not read");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());

    let m = desktop_pair("abandon-item");
    killed(&m, "leave.jar_out", "home");
    let stores = data(&m).join("IndexedDB");
    testing::deny_reading(&stores).expect("not looked into");
    let stopped = gave_up(&m).expect_err("refused");
    let before = snapshot(&m);
    let again = gave_up(&m).expect_err("refused again");
    unchanged(&m, &before, "an item not looked at");
    testing::make_private(&stores).expect("looked into again");
    for refused in [&stopped, &again] {
        assert_eq!(refused.code(), "login_folder_unknown", "{refused}");
        let said = refused.to_string();
        assert!(said.contains("could not look at"), "{said}");
        assert!(!said.contains("pitboard abandon"), "{said}");
    }
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());

    // What of the park Pitboard cannot look at or read is said as the park's, never Claude's.
    let m = desktop_pair("abandon-park");
    killed(&m, "leave.verified", "home");
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("waits")
        .park
        .clone();
    let items = crate::store::folder::at(&m.ctx, &park).join("items");
    let jar = items.join("Cookies");
    let refused_twice = |m: &Rig| {
        let stopped = gave_up(m).expect_err("refused");
        let before = snapshot(m);
        let again = gave_up(m).expect_err("refused again");
        unchanged(m, &before, "the park not looked at");
        [stopped, again]
    };
    let said_of = |refused: &[Error; 2], named: &std::path::Path| {
        for refused in refused {
            assert_eq!(refused.code(), "credential_store_unreadable", "{refused}");
            let said = refused.to_string();
            assert!(said.contains(&named.display().to_string()), "{said}");
            for never in ["Open Claude", "data folder", "pitboard abandon"] {
                assert!(!said.contains(never), "{said}");
            }
        }
    };
    let stores = items.join("IndexedDB");
    testing::deny_reading(&stores).expect("not looked into");
    let refused = refused_twice(&m);
    testing::make_private(&stores).expect("looked into again");
    said_of(&refused, &stores);
    testing::deny_reading(&items).expect("not looked into");
    let refused = refused_twice(&m);
    testing::make_private(&items).expect("looked into again");
    said_of(&refused, &jar);
    m.mem.jar_unsettled(&jar);
    let refused = refused_twice(&m);
    said_of(&refused, &jar);
    assert!(
        refused[0].to_string().contains("(a test held the jar)"),
        "{}",
        refused[0]
    );
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
}

#[test]
fn giving_up_indexes_every_folder_moves_nothing_and_deletes_nothing() {
    let m = desktop_pair("give-up");
    killed(&m, "leave.jar_out", "home");
    // The jar is parked; its journal and the rest are in Claude's folder. Its keys come
    // back by hand, as tokens with no session: no row of recovery names that.
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let kept =
        std::fs::read_to_string(crate::store::folder::keys_file(&m.ctx, &park)).expect("keys kept");
    let tokens: Map<String, Value> = serde_json::from_str(&kept).expect("JSON");
    set(
        &m,
        "oauth:tokenCache",
        tokens.get("oauth:tokenCache").cloned(),
    );
    let before = (
        tree(&data(&m)),
        tree(&crate::store::folder::parks_dir(&m.ctx)),
    );
    let stopped = code(confirm_(&m, "work"));
    assert_eq!(stopped, "recovery_undetermined");
    assert_eq!(
        code(forget_(&m, "home")),
        "recovery_undetermined",
        "no account is forgotten while a move waits"
    );
    assert!(state_of(&m).get(&m.key("home")).is_some());

    let gave = gave_up(&m).expect("given up");
    assert_eq!(gave.moves.len(), 1, "{gave:?}");
    let given = &gave.moves[0];
    assert_eq!((given.parked, given.set_aside), (1, 0), "{given}");
    assert!(given.left.contains(&"Local Storage".to_string()), "{given}");
    assert!(!given.left.iter().any(|item| item.starts_with("Cookies")));
    assert!(
        given
            .to_string()
            .contains("stay in Claude's folder. Open Claude once"),
        "{given}"
    );
    let manifest = crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there");
    assert_eq!(manifest.left_in_folder, given.left);
    assert_eq!(manifest.login.as_deref(), Some(WORK.login().as_str()));
    let after = (
        tree(&data(&m)),
        tree(&crate::store::folder::parks_dir(&m.ctx)),
    );
    assert_eq!(after.0, before.0, "nothing of Claude's folder moved");
    let paths = |found: &[(PathBuf, Option<Vec<u8>>)]| -> Vec<PathBuf> {
        found.iter().map(|(path, _)| path.clone()).collect()
    };
    assert_eq!(
        paths(&after.1),
        paths(&before.1),
        "nothing under parks was moved or deleted"
    );
    let state = state_of(&m);
    assert!(state.moving.is_empty() && state.in_use(ProviderId::Desktop).is_none());
    assert_eq!(park_of(&m, "work").map(|p| p.service), Some(park));
    hold(&m, "given up");
}

#[test]
fn giving_up_refuses_where_parts_of_two_logins_would_be_left_unsaid() {
    let m = mixed("mixed");
    let before = snapshot(&m);
    let mixed = gave_up(&m).expect_err("refused");
    assert_eq!(mixed.code(), "move_mixed");
    assert_eq!(mixed.exit_code(), 3);
    assert!(
        mixed
            .to_string()
            .contains("`Local Storage` in Claude's folder"),
        "{mixed}"
    );
    unchanged(&m, &before, "nothing changed");

    // The record's own session in Claude's folder, under keys it does not name: nothing of
    // the folder is touched, and `enroll` under the account Claude names lands.
    let m = desktop_pair("mine-and-other");
    killed(&m, "leave.keys_kept", "home");
    names(&m, &HOME);
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    let folder = tree(&data(&m));
    let gave = gave_up(&m).expect("given up, nothing refused");
    assert_eq!(gave.moves.len(), 1, "{gave:?}");
    assert!(gave.moves[0].left.is_empty());
    assert_eq!(tree(&data(&m)), folder, "nothing of Claude's folder moved");
    // Row 18 now that nothing is recorded in use: `home`'s older park is set aside.
    let (_, warnings) = enrol(&m, "home").expect("filed as Claude names it");
    assert!(
        matches!(
            warnings.as_slice(),
            [Warning::SetAside { why: SetAside::Superseded, label: Some(label), .. }]
                if label == "desktop/home"
        ),
        "{warnings:?}"
    );
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    assert_eq!(whereabouts(&m, &WORK), ["Claude"]);
    hold(&m, "given up over the record's own session");
}

#[test]
fn enroll_refused_under_a_taken_label_gives_nothing_up() {
    let m = mixed("taken-label");
    let before = snapshot(&m);
    assert_eq!(code(enrol(&m, "home")), "label_taken");
    unchanged(&m, &before, "the record, the folder and parks as they were");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
}

#[test]
fn enroll_sets_aside_parts_left_beside_the_login_it_files() {
    let m = mixed("parts-aside");
    let jar = std::fs::read(data(&m).join("Cookies")).expect("the stranger's jar");
    let (_, warnings) = enrol(&m, "third").expect("the person's word is the way out");
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            Warning::SetAside { why: SetAside::GivenUp, label: Some(label), .. }
                if label == "desktop/work"
        )),
        "{warnings:?}"
    );
    assert_eq!(
        std::fs::read(data(&m).join("Cookies")).expect("the jar"),
        jar
    );
    assert_eq!(in_use(&m).as_deref(), Some("third"));
    let state = state_of(&m);
    let aside = state
        .aside
        .iter()
        .find(|a| a.why == SetAside::GivenUp)
        .expect("set aside");
    assert_eq!(
        aside.account.as_deref(),
        Some(format!("desktop_{}", WORK.uuid).as_str())
    );
    assert!(!crate::store::folder::item(&m.ctx, &aside.name, "Cookies").exists());
    assert!(!crate::store::folder::item(&m.ctx, &aside.name, "Cookies-journal").exists());
    assert!(crate::store::folder::item(&m.ctx, &aside.name, "Local Storage").exists());
    assert!(!data(&m).join("Local Storage").exists());
    assert_eq!(
        whereabouts(&m, &WORK).len(),
        1,
        "`work`'s session is in its park alone"
    );
    assert!(park_of(&m, "work").is_some());
    hold(&m, "enrolled out of a mixed move");
    rested(&m, "enrolled out of a mixed move");
}

#[test]
fn enroll_is_the_way_out_of_a_stopped_move_with_claude_signed_in() {
    // Somebody signed in after an arrive was recorded: `home` keeps its park.
    let m = desktop_pair("way-out-arrive");
    killed(&m, "arrive.recorded", "home");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    let parked = park_of(&m, "home").expect("parked").service;
    enrol(&m, "third").expect("enrolled");
    assert_eq!(in_use(&m).as_deref(), Some("third"));
    assert_eq!(park_of(&m, "home").map(|p| p.service), Some(parked));
    assert!(state_of(&m).moving.is_empty());
    hold(&m, "out of a stopped arrive");
    rested(&m, "out of a stopped arrive");

    // Claude replaced the session of the account arriving: it is filed under that account.
    let m = desktop_pair("way-out-renewed");
    killed(&m, "arrive.keys_in", "home");
    let claude = m.claude();
    claude.opens();
    claude.renews(&HOME, "home-session-renewed");
    claude.quits();
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    enrol(&m, "home").expect("enrolled");
    let renewed = Person {
        uuid: HOME.uuid,
        session: "home-session-renewed",
    };
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    assert_eq!(whereabouts(&m, &renewed), ["Claude"]);
    let state = state_of(&m);
    assert_eq!(
        state.in_use(ProviderId::Desktop).map(|r| r.login.clone()),
        Some(renewed.login())
    );
    hold(&m, "out of a stopped arrive Claude renewed");
    rested(&m, "out of a stopped arrive Claude renewed");
}

#[test]
fn a_give_up_never_moves_the_jar_in_claudes_folder() {
    for (at, who, label) in [
        ("leave.keys_out", WORK, "work"),
        ("arrive.jar_in", HOME, "home"),
        ("arrive.keys_in", HOME, "home"),
    ] {
        let m = desktop_pair(&format!("jar-stays-{at}"));
        killed(&m, at, "home");
        let claude = m.claude();
        claude.opens();
        claude.renews(&who, "a-session-claude-renewed");
        claude.quits();
        let inode = |item: &str| crate::host::fs::testing::inode(&data(&m).join(item)).ok();
        let jar = (inode("Cookies"), inode("Cookies-journal"));
        let park = state_of(&m)
            .moving(ProviderId::Desktop)
            .expect("recorded")
            .park
            .clone();
        assert!(jar.0.is_some() && jar.1.is_some(), "{at}");
        enrol(&m, label).unwrap_or_else(|e| panic!("{at}: {e}"));
        assert_eq!((inode("Cookies"), inode("Cookies-journal")), jar, "{at}");
        let renewed = Person {
            uuid: who.uuid,
            session: "a-session-claude-renewed",
        };
        let state = state_of(&m);
        assert_eq!(
            state.in_use(ProviderId::Desktop).map(|r| r.login.clone()),
            Some(renewed.login()),
            "{at}"
        );
        assert!(
            state
                .aside
                .iter()
                .all(|a| a.why != SetAside::GivenUp || a.name == park),
            "{at}: no aside made by the give-up: {:?}",
            state.aside
        );
        hold(&m, at);
        rested(&m, at);
    }
}

#[test]
fn a_give_up_never_files_a_jar_it_did_not_identify() {
    let m = desktop_pair("unidentified-jar");
    killed(&m, "arrive.recorded", "home");
    let park = park_of(&m, "home").expect("parked").service;
    let parked = crate::store::folder::item(&m.ctx, &park, "Cookies");
    write_stand_in_jar(&parked, 24, &session_of(Some(&STRANGER)));
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    let gave = gave_up(&m).expect("given up");
    assert_eq!((gave.moves[0].parked, gave.moves[0].set_aside), (0, 1));
    assert!(park_of(&m, "home").is_none(), "`home` holds nothing");
    let state = state_of(&m);
    let aside = state.aside.iter().find(|a| a.name == park).expect("kept");
    assert_eq!(aside.why, SetAside::GivenUp);
    assert_eq!(aside.login.as_deref(), Some(STRANGER.login().as_str()));
    assert_eq!(
        aside.account.as_deref(),
        Some(format!("desktop_{}", HOME.uuid).as_str())
    );
    repair(settled(&m)).expect("repaired");
    assert!(park_of(&m, "home").is_none(), "never given back");
    assert!(crate::store::folder::at(&m.ctx, &park).is_dir());
    hold(&m, "a jar given up unidentified");
}

#[test]
fn a_give_up_never_points_a_manifest_at_a_session_it_did_not_park() {
    let m = desktop_pair("manifest-kept");
    killed(&m, "leave.jar_out", "home");
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let parked = crate::store::folder::item(&m.ctx, &park, "Cookies");
    write_stand_in_jar(&parked, 24, &session_of(Some(&STRANGER)));
    gave_up(&m).expect("given up");
    let manifest = crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there");
    assert_eq!(
        manifest.login.as_deref(),
        Some(WORK.login().as_str()),
        "the login the leave wrote"
    );
    let state = state_of(&m);
    let aside = state.aside.iter().find(|a| a.name == park).expect("kept");
    assert_eq!(aside.login.as_deref(), Some(STRANGER.login().as_str()));
    repair(settled(&m)).expect("repaired");
    let state = state_of(&m);
    assert!(park_of(&m, "work").is_none(), "given to nobody");
    assert!(state.aside.iter().any(|a| a.name == park));
}

#[test]
fn what_a_give_up_left_is_filed_under_its_account_only_until_another_takes_it() {
    let work = Some(format!("desktop_{}", WORK.uuid));
    let m = left_by_a_give_up("left-filed");
    assert!(!listed_left(&m, "work").is_empty());
    let claude = m.claude();
    claude.opens();
    claude.quits();
    used(&m, "home");
    assert_eq!(
        in_the_way(&m),
        std::slice::from_ref(&work),
        "nothing took them since"
    );
    assert!(listed_left(&m, "work").is_empty(), "set aside");
    hold(&m, "left by a give-up, set aside");

    // The arrive killed once it set the first of them aside: recovery sets the rest aside with
    // it, and what Claude makes after is nobody's.
    let m = left_by_a_give_up("left-killed");
    let claude = m.claude();
    claude.opens();
    claude.quits();
    killed(&m, "arrive.item_aside", "home");
    // The next change settles the arrive first.
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert_eq!(
        in_the_way(&m),
        std::slice::from_ref(&work),
        "set aside by recovery"
    );
    assert!(listed_left(&m, "work").is_empty(), "set aside by recovery");
    claude.opens();
    claude.quits();
    used(&m, "home");
    assert_eq!(in_the_way(&m), [work.clone(), None], "Claude's own since");
    hold(&m, "left by a give-up, set aside by recovery");
    rested(&m, "left by a give-up, set aside by recovery");

    // The arrive killed before its jar went aside: the listing stays with the items.
    let m = left_by_a_give_up("left-keys-killed");
    let claude = m.claude();
    claude.opens();
    claude.quits();
    killed(&m, "arrive.keys_cleared", "home");
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert!(
        !listed_left(&m, "work").is_empty(),
        "still in Claude's folder"
    );
    used(&m, "home");
    assert_eq!(
        in_the_way(&m),
        [work.clone(), work.clone()],
        "the keys, then the items"
    );
    assert!(listed_left(&m, "work").is_empty(), "set aside");
    claude.opens();
    claude.logs_out();
    claude.quits();
    used(&m, "work");
    assert_eq!(
        in_the_way(&m),
        [work.clone(), work, None],
        "Claude's own since"
    );
    hold(&m, "left by a give-up, set aside after a kill");
    rested(&m, "left by a give-up, set aside after a kill");

    // Somebody signs in over them and is enrolled, leaves with them, then a Log out.
    let m = left_by_a_give_up("left-taken");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    enrol(&m, "j").expect("filed as the person says");
    assert!(listed_left(&m, "work").is_empty(), "filed under `j`");
    used(&m, "home");
    claude.opens();
    claude.logs_out();
    claude.quits();
    used(&m, "j");
    assert_eq!(in_the_way(&m), [None], "never `work`'s");
    hold(&m, "left by a give-up, then taken by another");
    rested(&m, "left by a give-up, then taken by another");

    // A listing nothing took goes with the next leave, and with the next arrive.
    let m = desktop_pair("left-moved");
    list_left(&m, "home");
    watched(&m, "travel").expect("`work` parked").cancel();
    assert!(
        listed_left(&m, "home").is_empty(),
        "taken by `work`'s leave"
    );
    list_left(&m, "home");
    used(&m, "work");
    assert!(
        listed_left(&m, "home").is_empty(),
        "gone by `work`'s arrive"
    );
    hold(&m, "a listing nothing took, then a leave and an arrive");
}

// A leave whose commit cannot clear a listing waits, and the next change finishes it.
#[test]
fn a_listing_that_cannot_be_cleared_leaves_the_leave_to_the_next_change() {
    let m = desktop_pair("left-unwritable");
    list_left(&m, "home");
    let park = crate::store::folder::at(&m.ctx, &park_of(&m, "home").expect("parked").service);
    let mode = std::fs::metadata(&park).expect("there").permissions();
    crate::host::fs::testing::deny_changes(&park).expect("read-only");
    let refused = use_(&m, "home");
    std::fs::set_permissions(&park, mode).expect("changeable again");
    assert_eq!(code(refused), "home_unwritable");
    let moving = state_of(&m)
        .moving(ProviderId::Desktop)
        .cloned()
        .expect("the leave waits");
    assert_eq!(moving.half, state::Half::Leave);
    assert_eq!(in_use(&m).as_deref(), Some("work"), "nothing committed");
    assert!(!listed_left(&m, "home").is_empty(), "kept");
    assert!(!data(&m).join("Cookies").exists(), "`work`'s jar left");
    assert!(
        crate::store::folder::item(&m.ctx, &moving.park, "Cookies").exists(),
        "`work`'s jar parked"
    );
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert!(
        state_of(&m).moving(ProviderId::Desktop).is_none(),
        "finished"
    );
    assert!(listed_left(&m, "home").is_empty(), "taken by the leave");
    used(&m, "home");
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    hold(&m, "a leave finished by the next change");
    rested(&m, "a leave finished by the next change");
}

#[test]
fn giving_up_an_arrive_keeps_the_accounts_park() {
    let m = desktop_pair("arrive-kept");
    killed(&m, "arrive.recorded", "home");
    let parked = park_of(&m, "home").expect("parked");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let gave = gave_up(&m).expect("given up");
    assert_eq!((gave.moves[0].parked, gave.moves[0].set_aside), (1, 0));
    assert_eq!(park_of(&m, "home"), Some(parked.clone()));
    assert_eq!(whereabouts(&m, &HOME), [parked.service]);
    let state = state_of(&m);
    assert!(state.moving.is_empty() && state.in_use(ProviderId::Desktop).is_none());
    hold(&m, "an arrive given up");
}

#[test]
fn giving_up_a_move_elsewhere_reads_no_claude_folder() {
    let m = desktop_pair("elsewhere");
    killed(&m, "leave.jar_out", "home");
    let mut state = state_of(&m);
    state.moving.get_mut("desktop").expect("recorded").slot = "/somewhere/else".into();
    let there = state
        .in_use(ProviderId::Desktop)
        .cloned()
        .expect("work in use");
    state
        .other_slots
        .entry("desktop".into())
        .or_default()
        .insert("/somewhere/else".into(), there);
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    assert_eq!(code(confirm_(&m, "work")), "recovery_elsewhere");
    let here = state_of(&m).in_use(ProviderId::Desktop).cloned();
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let (jars, listings) = (m.mem.jars_asked().len(), m.mem.listings());
    m.claude().opens();
    let gave = gave_up(&m).expect("given up, Claude open or not");
    let asked = m.mem.jars_asked();
    assert!(
        asked[jars..].iter().all(|jar| !jar.starts_with(data(&m))),
        "no jar of Claude's folder was read: {asked:?}"
    );
    assert_eq!(m.mem.listings(), listings, "no process was listed");
    assert_eq!(gave.moves[0].slot.as_deref(), Some("/somewhere/else"));
    assert_eq!((gave.moves[0].parked, gave.moves[0].set_aside), (1, 0));
    assert!(
        gave.moves[0]
            .to_string()
            .contains("which Pitboard did not read")
    );
    let state = state_of(&m);
    assert!(state.moving.is_empty());
    assert!(state.aside.is_empty(), "{:?}", state.aside);
    assert_eq!(
        state.in_use(ProviderId::Desktop),
        here.as_ref(),
        "this data folder's record is kept"
    );
    assert!(
        state.other_slots.is_empty(),
        "the move's data folder records nobody: {:?}",
        state.other_slots
    );
    assert_eq!(
        park_of(&m, "work").map(|p| p.service),
        Some(park.clone()),
        "work's only session is its park again"
    );
    let manifest = crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there");
    assert_eq!(manifest.items, ["Cookies"], "what the park holds");
    m.claude().crashes();
    hold(&m, "given up elsewhere");
}

// The parts left in Claude's folder go aside one at a time.
#[test]
fn a_give_up_that_stops_partway_never_says_nothing_has_changed() {
    let m = mixed("give-up-partway");
    let stopped =
        crate::fault::failing_nth("move_in", 2, || enrol(&m, "third")).expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    let said = stopped.to_string();
    assert!(!said.contains("Nothing has changed"), "{said}");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
    enrol(&m, "third").expect("the person's word is still the way out");
    assert_eq!(in_use(&m).as_deref(), Some("third"));
    hold(&m, "enrolled after a give-up that stopped");
    rested(&m, "enrolled after a give-up that stopped");
}

#[test]
fn claude_open_never_stops_abandon_giving_up_a_claude_code_switch() {
    let m = three_tools("abandon-open");
    killed(&m, "leave.jar_out", "home");
    interrupted(&m, ProviderId::Claude);
    m.claude().opens();
    let folder = claudes(&m);
    let gave = gave_up(&m).expect("the switch is given up");
    switch_given_up_beside(&m, &gave, "Claude is open");
    assert_eq!(
        gave.warnings[0].to_string(),
        "Pitboard did not give up the interrupted move of `desktop/work`'s login, because Claude \
         is open. Quit Claude with Command-Q, then run `pitboard abandon` again."
    );
    assert_eq!(claudes(&m), folder, "nothing of Claude's moved");
    let before = snapshot(&m);
    let refused = gave_up(&m).expect_err("the move alone, refused as before");
    assert_eq!(refused.code(), "app_open");
    unchanged(&m, &before, "nothing changed");
    m.claude().crashes();
    let gave = gave_up(&m).expect("settled");
    assert!(gave.switch.is_none() && gave.moves.is_empty());
    assert_eq!(
        gave.warnings.iter().map(Warning::code).collect::<Vec<_>>(),
        ["interrupted_move_finished"]
    );
}

#[test]
fn a_codex_switch_is_given_up_beside_a_move_that_cannot_be() {
    let m = beside_desktop(crate::switch::harness::codex_machine("abandon-mixed"));
    killed(&m, "leave.item_out", "home");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    interrupted(&m, ProviderId::Codex);
    let folder = claudes(&m);
    let gave = gave_up(&m).expect("the switch is given up");
    switch_given_up_beside(&m, &gave, "beside another login in Claude's folder");
    assert!(
        gave.warnings[0]
            .to_string()
            .contains("`Local Storage` in Claude's folder"),
        "{}",
        gave.warnings[0]
    );
    assert_eq!(claudes(&m), folder, "nothing of Claude's moved");

    let m = beside_desktop(crate::switch::harness::codex_machine("abandon-unreadable"));
    killed(&m, "leave.jar_out", "home");
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    m.mem
        .jar_unsettled(&crate::store::folder::item(&m.ctx, &park, "Cookies"));
    interrupted(&m, ProviderId::Codex);
    let folder = claudes(&m);
    let gave = gave_up(&m).expect("the switch is given up");
    switch_given_up_beside(&m, &gave, "(a test held the jar)");
    let said = gave.warnings[0].to_string();
    assert!(
        said.contains(
            &crate::store::folder::at(&m.ctx, &park)
                .display()
                .to_string()
        ),
        "{said}"
    );
    assert!(!said.contains("Open Claude"), "{said}");
    assert_eq!(claudes(&m), folder, "nothing of Claude's moved");
}

// The enrolment gives the move up and sets `home`'s older park aside, then cannot save: it
// says only what giving up saved.
#[test]
fn an_enrolment_that_cannot_save_says_nothing_it_did_not_save() {
    let m = desktop_pair("enrol-unsaved");
    killed(&m, "leave.keys_kept", "home");
    names(&m, &HOME);
    let file = crate::home::dir(&m.ctx).join("state.json");
    let kept = crate::home::dir(&m.ctx).join("state.kept");
    let failed = crate::fault::meanwhile(
        "give_up.done",
        {
            let (file, kept) = (file.clone(), kept.clone());
            move || {
                std::fs::rename(&file, &kept).expect("moved");
                std::fs::create_dir_all(file.join("in the way")).expect("a folder in its place");
            }
        },
        || enrol(&m, "home"),
    );
    std::fs::remove_dir_all(&file).expect("taken out of the way");
    std::fs::rename(&kept, &file).expect("put back");
    let mut failed = failed.expect_err("refused");
    assert_eq!(failed.code(), "state_write_failed", "{failed}");
    let said = failed.take_warnings();
    assert!(
        !said.iter().any(|warning| matches!(
            warning,
            Warning::SetAside {
                why: SetAside::Superseded,
                ..
            } | Warning::TakenAsNamed { .. }
        )),
        "{said:?}"
    );
    let state = state_of(&m);
    assert!(state.moving(ProviderId::Desktop).is_none(), "given up");
    assert!(
        state
            .aside
            .iter()
            .all(|aside| aside.why != SetAside::Superseded),
        "{:?}",
        state.aside
    );
}
