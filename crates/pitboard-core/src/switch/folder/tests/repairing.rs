use super::{
    WORK_BEFORE, code, confirm_, enrol, gave_up, in_use, jar, killed, park_by_hand, park_name,
    park_of, refused, renewed, session_of, set, settled_then_refused, state_of, superseded,
    two_accounts, use_, used,
};
use super::{as_a_dead_runs, data, rested, settled, snapshot, tree, unchanged, whereabouts};
use crate::desktop_testing::{EMPTIED, write_stand_in_jar};
use crate::permit::Permit;
use crate::provider::ProviderId;
use crate::state::{self, SetAside};
use crate::switch::harness::{HOME, NOW, Rig, STRANGER, WORK, desktop_pair, hold, recover};
use crate::switch::{Outcome, forget, repair, settle};
use crate::warning::Warning;
use serde_json::{Value, json};

#[test]
fn repair_gives_a_folder_back_where_its_manifest_its_keys_and_its_jar_agree() {
    let given = |name: &str, spoil: &dyn Fn(&Rig, &str)| {
        let m = two_accounts(name);
        let homes = park_name(&HOME);
        park_by_hand(&m, None, &homes, &HOME);
        spoil(&m, &homes);
        let (reclaimed, _) = repair(settled(&m)).expect("repaired");
        let state = state::load(&m.ctx).expect("state");
        let parked = state
            .get(&m.key("home"))
            .and_then(|a| a.parked.clone())
            .map(|p| p.service);
        assert!(
            crate::store::folder::at(&m.ctx, &homes).exists(),
            "{name}: never deleted"
        );
        hold(&m, name);
        (parked == Some(homes.clone()), reclaimed)
    };
    let (back, reclaimed) = given("repair-agrees", &|_, _| {});
    assert!(back, "{reclaimed:?}");
    // A listed item that is not under `items/`, the jar still there.
    let (back, reclaimed) = given("repair-item-gone", &|m, name| {
        let path = crate::store::folder::at(&m.ctx, name).join("manifest.json");
        let mut manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("manifest")).expect("JSON");
        manifest["items"] = json!(["Cookies", "Local Storage"]);
        std::fs::write(&path, manifest.to_string()).expect("rewritten");
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    // Another token cache under the same name: the manifest's digests are what differs.
    let (back, reclaimed) = given("repair-keys-differ", &|m, name| {
        std::fs::write(
            crate::store::folder::keys_file(&m.ctx, name),
            format!(
                "{{\"oauth:tokenCache\": \"other\", \"lastKnownAccountUuid\": \"{}\"}}",
                HOME.uuid
            ),
        )
        .expect("keys");
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    // Parked on another Mac, whose key Claude's jar was written under.
    let (back, reclaimed) = given("repair-another-mac", &|m, name| {
        let path = crate::store::folder::at(&m.ctx, name).join("manifest.json");
        let mut manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("manifest")).expect("JSON");
        manifest["machine"] = json!("another Mac");
        std::fs::write(&path, manifest.to_string()).expect("rewritten");
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    // The jar holds another session than the manifest says: never the wrong account's.
    let (back, reclaimed) = given("repair-jar-differs", &|m, name| {
        let jar = crate::store::folder::item(&m.ctx, name, "Cookies");
        write_stand_in_jar(&jar, 24, &session_of(Some(&STRANGER)));
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    // No keys file is a folder nothing names, not one that could not be read.
    let (back, reclaimed) = given("repair-no-keys", &|m, name| {
        std::fs::remove_file(crate::store::folder::keys_file(&m.ctx, name)).expect("keys");
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    assert!(reclaimed.unreadable.is_empty(), "{reclaimed:?}");
}

#[test]
fn repair_gives_nothing_back_to_the_account_in_use() {
    // Row 17, then `enroll`: the older login is set aside, and `repair` leaves it there.
    let m = two_accounts("repair-row-17");
    let older = park_name(&WORK);
    park_by_hand(&m, Some(&m.key("work")), &older, &WORK_BEFORE);
    jar(&m, Some(&STRANGER));
    enrol(&m, "work").expect("filed");
    let (reclaimed, _) = repair(settled(&m)).expect("repaired");
    assert!(reclaimed.is_empty(), "{reclaimed:?}");
    let state = state::load(&m.ctx).expect("state");
    assert!(state.get(&m.key("work")).expect("kept").parked.is_none());
    let aside = state.aside.iter().find(|a| a.name == older);
    assert_eq!(aside.map(|a| a.why), Some(SetAside::Superseded));
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    hold(&m, "row 17, enrolled, repaired");

    // A folder nothing names, of the account in use: set aside, whatever session it holds.
    let m = two_accounts("repair-in-use");
    park_by_hand(&m, None, &older, &WORK_BEFORE);
    let (reclaimed, _) = repair(settled(&m)).expect("repaired");
    assert!(reclaimed.given_back.is_empty(), "{reclaimed:?}");
    assert_eq!(reclaimed.set_aside, [older.as_str()]);
    let state = state::load(&m.ctx).expect("state");
    assert!(state.get(&m.key("work")).expect("kept").parked.is_none());
    let aside = state.aside.iter().find(|a| a.name == older).expect("aside");
    assert_eq!(aside.why, SetAside::Orphan);
    assert_eq!(aside.login, Some(WORK_BEFORE.login()));
    hold(&m, "an older login of the account in use");
}

#[test]
fn an_aside_set_aside_while_the_machine_could_not_be_read_is_given_back() {
    let m = two_accounts("unknown-machine");
    let homes = park_name(&HOME);
    let park = park_by_hand(&m, None, &homes, &HOME);
    let mut state = state::load(&m.ctx).expect("state");
    state.aside.push(state::Aside {
        name: homes.clone(),
        tool: ProviderId::Desktop,
        at: NOW,
        why: SetAside::AnotherMachine,
        account: Some(format!("desktop_{}", HOME.uuid)),
        login: Some(park.login.clone()),
    });
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");

    let blind = m.ctx.clone().with_unknown_machine();
    let (unknown, _) = settle(&blind, Permit::for_a_test(), None).expect("settled");
    repair(unknown).expect("repaired");
    let state = state::load(&m.ctx).expect("state");
    assert!(state.get(&m.key("home")).expect("kept").parked.is_none());
    assert!(state.aside.iter().any(|a| a.name == homes), "kept aside");

    repair(settled(&m)).expect("repaired");
    let state = state::load(&m.ctx).expect("state");
    assert_eq!(
        state
            .get(&m.key("home"))
            .and_then(|a| a.parked.as_ref())
            .map(|p| p.service.as_str()),
        Some(homes.as_str())
    );
    assert!(!state.aside.iter().any(|a| a.name == homes));
}

#[test]
fn repair_gives_an_account_its_park_back_where_the_one_recorded_is_gone() {
    // A backup of `state.json` from while `home` was parked, restored once it was parked again.
    let restored = |name: &str| {
        let m = desktop_pair(name);
        let file = crate::home::dir(&m.ctx).join("state.json");
        let backup = std::fs::read(&file).expect("a backup");
        let recorded = park_of(&m, "home").expect("parked").service;
        use_(&m, "home").expect("`home` in use");
        m.ctx.clock.pause(1);
        use_(&m, "work").expect("`work` in use again");
        recover(&m).expect("swept");
        let again = park_of(&m, "home").expect("parked again").service;
        assert_ne!(again, recorded);
        assert!(!crate::store::folder::at(&m.ctx, &recorded).exists());
        std::fs::write(&file, backup).expect("restored");
        (m, recorded, again)
    };
    let given_back = |m: &Rig, recorded: &str, again: &str, why: &str| {
        let state = state_of(m);
        assert_eq!(
            park_of(m, "home").map(|p| p.service).as_deref(),
            Some(again),
            "{why}"
        );
        assert!(!state.names(recorded), "{why}: the gone park is let go");
        assert!(state.aside.is_empty(), "{why}: {:?}", state.aside);
        use_(m, "home").unwrap_or_else(|e| panic!("{why}: use home: {e}"));
        assert_eq!(whereabouts(m, &HOME), ["Claude"], "{why}");
        rested(m, why);
    };

    let (m, recorded, again) = restored("repair-restored");
    let (reclaimed, _) = repair(settled(&m)).expect("repaired");
    assert!(reclaimed.set_aside.is_empty(), "{reclaimed:?}");
    assert_eq!(
        reclaimed.given_back,
        [("desktop/home".to_string(), again.clone())]
    );
    given_back(&m, &recorded, &again, "found under `parks`");

    // What an earlier `repair` left: the folder set aside under `home`.
    let (m, recorded, again) = restored("repair-restored-aside");
    let mut state = state_of(&m);
    state.aside.push(state::Aside {
        name: again.clone(),
        tool: ProviderId::Desktop,
        at: NOW,
        why: SetAside::Orphan,
        account: Some(format!("desktop_{}", HOME.uuid)),
        login: Some(HOME.login()),
    });
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    repair(settled(&m)).expect("repaired");
    given_back(&m, &recorded, &again, "set aside");
}

#[test]
fn repair_never_lets_go_of_a_park_a_move_holds() {
    // An arrive of `home` waits with Claude open, its park deleted by hand, and another login
    // of `home` is found under `parks`.
    let m = desktop_pair("repair-move-holds");
    killed(&m, "arrive.recorded", "home");
    let held = park_of(&m, "home").expect("parked").service;
    m.claude().opens();
    std::fs::remove_dir_all(crate::store::folder::at(&m.ctx, &held)).expect("deleted by hand");
    let found = crate::park::service_name(&format!("desktop_{}", HOME.uuid), (NOW + 60) * 1000);
    park_by_hand(&m, None, &found, &HOME);

    let (reclaimed, warnings) = repair(settled(&m)).expect("repaired");

    assert!(
        matches!(warnings.as_slice(), [Warning::MoveNotSettled { .. }]),
        "{warnings:?}"
    );
    assert!(reclaimed.given_back.is_empty(), "{reclaimed:?}");
    assert_eq!(reclaimed.set_aside, [found.as_str()]);
    let state = state_of(&m);
    assert_eq!(park_of(&m, "home").map(|p| p.service), Some(held.clone()));
    assert!(state.moves(&held));
    assert!(!state.discarded.iter().any(|d| d.service == held));
    assert!(state.aside.iter().any(|aside| aside.name == found));
}

// A leave stopped after the keys went, then Claude run signed out beside grants it kept.
#[test]
fn a_leave_finished_beside_connector_grants_verifies_and_commits() {
    let m = desktop_pair("grants-settled");
    killed(&m, "leave.item_out", "home");
    let claude = m.claude();
    claude.opens();
    claude.quits();
    set(&m, "custom3pMcpOAuth", Some(json!({"grant": "kept"})));

    let gave = gave_up(&m).expect("settled");
    assert!(gave.moves.is_empty(), "nothing given up: {gave:?}");
    assert!(
        matches!(
            gave.warnings.as_slice(),
            [Warning::MoveSettled(moved)]
                if moved.half == state::Half::Leave && moved.ended == crate::switch::folder::Ended::Finished
        ),
        "{:?}",
        gave.warnings
    );
    assert_eq!(in_use(&m), None);
}

#[test]
fn repair_says_a_move_it_cannot_settle_as_a_warning() {
    let m = desktop_pair("repair-waiting");
    let listings = m.mem.listings();
    let (_, warnings) = repair(settled(&m)).expect("repaired");
    assert!(warnings.is_empty());
    assert_eq!(
        m.mem.listings(),
        listings,
        "nothing of Claude is read where no move waits"
    );

    killed(&m, "leave.jar_out", "home");
    m.claude().opens();
    let (_, warnings) = repair(settled(&m)).expect("the rest is repaired");
    let not_settled = |warnings: &[Warning]| {
        let codes: Vec<&str> = warnings.iter().map(Warning::code).collect();
        assert_eq!(codes, ["move_not_settled"], "{warnings:?}");
        let said = warnings[0].to_string();
        assert!(said.contains("`desktop/work`'s login"), "{said}");
        said
    };
    let said = not_settled(&warnings);
    assert!(
        said.contains("because Claude is open. Quit Claude with Command-Q"),
        "{said}"
    );
    assert!(!said.contains("Keep Claude closed"), "{said}");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
    m.claude().crashes();
    let (_, warnings) = repair(settled(&m)).expect("repaired");
    assert!(
        matches!(warnings.as_slice(), [Warning::MoveSettled(moved)] if moved.ended == crate::switch::folder::Ended::Finished),
        "{warnings:?}"
    );
    rested(&m, "settled by repair");

    // What no next change settles either is said with its own way out.
    let m = desktop_pair("repair-undetermined");
    killed(&m, "leave.jar_out", "home");
    std::fs::write(data(&m).join("config.json"), "[]").expect("settings not read");
    let (_, warnings) = repair(settled(&m)).expect("the rest is repaired");
    let said = not_settled(&warnings);
    assert!(said.contains("cannot tell where it stopped"), "{said}");
    assert!(said.contains("`pitboard abandon`"), "{said}");
    assert!(!said.contains("settles it first"), "{said}");
    assert!(
        !said.contains("Nothing has changed"),
        "repair went on: {said}"
    );
    assert!(
        state_of(&m).moving(ProviderId::Desktop).is_some(),
        "the move still waits"
    );
}

#[test]
fn a_failure_inside_recovery_keeps_the_record_and_never_settles_again() {
    let m = desktop_pair("recovery-fails");
    killed(&m, "arrive.jar_in", "home");
    let before = state_of(&m)
        .moving(ProviderId::Desktop)
        .cloned()
        .expect("recorded");
    let (stopped, tried) = crate::fault::failing_counted("move_in", || confirm_(&m, "work"));
    let stopped = stopped.expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    assert!(
        stopped.to_string().contains("could not be moved"),
        "{stopped}"
    );
    assert_eq!(tried, 1, "one move tried: recovery did not run again");
    assert_eq!(state_of(&m).moving(ProviderId::Desktop), Some(&before));
    assert_eq!(
        code(confirm_(&m, "work")),
        "account_not_in_use",
        "its next run lands"
    );
    rested(&m, "taken back");
    assert!(park_of(&m, "home").is_some());
}

// The jar goes back first, so a later item that will not move leaves something changed.
#[test]
fn a_recovery_that_stops_partway_never_says_nothing_has_changed() {
    let m = desktop_pair("recovery-partway");
    killed(&m, "arrive.jar_in", "home");
    assert_eq!(whereabouts(&m, &HOME), ["Claude"]);
    let before = state_of(&m)
        .moving(ProviderId::Desktop)
        .cloned()
        .expect("recorded");
    let stopped =
        crate::fault::failing_nth("move_in", 2, || confirm_(&m, "work")).expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    let said = stopped.to_string();
    assert!(!said.contains("Nothing has changed"), "{said}");
    assert!(
        said.contains("Keep Claude closed and run this again"),
        "{said}"
    );
    assert_eq!(
        whereabouts(&m, &HOME),
        std::slice::from_ref(&before.park),
        "its jar went back"
    );
    assert_eq!(state_of(&m).moving(ProviderId::Desktop), Some(&before));
    assert_eq!(
        code(confirm_(&m, "work")),
        "account_not_in_use",
        "its next run lands"
    );
    rested(&m, "taken back");
}

// A leave gone on with, and an arrive taken back past Claude's own emptied keys.
#[test]
fn every_row_that_stops_partway_never_says_nothing_has_changed() {
    let emptied: Value = serde_json::from_str(EMPTIED).expect("JSON");
    for (at, name, signed_out) in [
        ("leave.jar_out", "stops-jar-out", false),
        ("leave.item_out", "stops-item-out", false),
        ("arrive.jar_in", "stops-keys-aside", true),
    ] {
        let m = desktop_pair(name);
        killed(&m, at, "home");
        if signed_out {
            set(&m, "oauth:tokenCache", Some(emptied.clone()));
        }
        let before = state_of(&m)
            .moving(ProviderId::Desktop)
            .cloned()
            .expect("recorded");
        let folder = tree(&data(&m));
        let stopped =
            crate::fault::failing_nth("move_in", 2, || confirm_(&m, "work")).expect_err("stopped");
        assert_eq!(stopped.code(), "recovery_undetermined", "{at}");
        let said = stopped.to_string();
        assert!(!said.contains("Nothing has changed"), "{at}: {said}");
        assert_ne!(tree(&data(&m)), folder, "{at}: something moved first");
        let mut kept = state_of(&m)
            .moving(ProviderId::Desktop)
            .cloned()
            .expect("the record is kept");
        kept.asides
            .retain(|aside, _| before.asides.contains_key(aside));
        assert_eq!(kept, before, "{at}");
        assert_eq!(
            code(confirm_(&m, "work")),
            "account_not_in_use",
            "{at}: its next run lands"
        );
        rested(&m, at);
    }
}

#[test]
fn claude_seen_after_the_last_write_keeps_the_record() {
    let m = desktop_pair("third-look");
    let app = m.claude();
    let midway = crate::fault::meanwhile(
        "arrive.keys_in",
        move || app.opens(),
        || refused(&m, "home"),
    );
    assert_eq!(midway.code(), "app_opened_midway");
    assert!(
        midway.to_string().contains("moved `desktop/home`'s login"),
        "{midway}"
    );
    let state = state_of(&m);
    assert_eq!(
        state.moving(ProviderId::Desktop).map(|r| r.half),
        Some(state::Half::Arrive)
    );
    assert_eq!(in_use(&m), None, "nothing is recorded as done");
    m.claude().crashes();
    assert!(matches!(used(&m, "home").0, Outcome::AlreadyActive { .. }));
    rested(&m, "recorded by the next change");
}

// Each change settles a move a killed leave left, then refuses, or forgets by the doubt rule.
#[test]
fn a_refusal_after_a_settled_move_says_it_settled() {
    let finished = "interrupted_move_finished";
    let undone = "interrupted_move_undone";
    let m = desktop_pair("settled-then-not-in-use");
    killed(&m, "leave.jar_out", "home");
    settled_then_refused(&m, "account_not_in_use", finished, |pitboard| {
        pitboard.update_config("desktop/work").expect_err("refused")
    });

    let m = desktop_pair("settled-then-unknown");
    killed(&m, "leave.jar_out", "home");
    std::fs::create_dir(data(&m).join("Network")).expect("an entry not read");
    settled_then_refused(&m, "login_folder_unknown", finished, |pitboard| {
        pitboard.switch_to("desktop/home").expect_err("refused")
    });

    let m = desktop_pair("settled-then-stranded");
    killed(&m, "leave.jar_out", "home");
    settled_then_refused(&m, "uninstall_would_strand", finished, |pitboard| {
        pitboard.uninstall().expect_err("refused")
    });
    assert!(crate::home::dir(&m.ctx).is_dir());
    assert!(state_of(&m).get(&m.key("work")).is_some());
    assert!(state_of(&m).get(&m.key("home")).is_some());

    let m = desktop_pair("settled-then-no-sign-in");
    killed(&m, "leave.jar_out", "home");
    std::fs::create_dir(data(&m).join("File System")).expect("an item the move never listed");
    settled_then_refused(&m, "login_folder_unknown", finished, |pitboard| {
        pitboard
            .sign_in_watched("desktop/travel")
            .expect_err("refused")
    });

    let m = desktop_pair("settled-then-signed-out");
    killed(&m, "leave.jar_out", "home");
    settled_then_refused(&m, "live_credential_absent", finished, |pitboard| {
        pitboard
            .enroll_current("desktop/travel")
            .expect_err("refused")
    });

    // Undone, so `work` is in use again.
    let m = desktop_pair("settled-then-in-use");
    killed(&m, "leave.keys_out", "home");
    settled_then_refused(&m, "cannot_forget_active_account", undone, |pitboard| {
        pitboard.forget("desktop/work").expect_err("refused")
    });
    assert!(state_of(&m).get(&m.key("work")).is_some());

    // With nobody recorded in use, the undone login is a session not seen: the doubt rule
    // refuses, since `work` holds no park.
    let m = desktop_pair("settled-then-doubt-unparked");
    killed(&m, "leave.keys_out", "home");
    let mut state = state_of(&m);
    state.in_use.remove(ProviderId::Desktop.code());
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    settled_then_refused(&m, "login_in_doubt", undone, |pitboard| {
        pitboard.forget("desktop/work").expect_err("refused")
    });
    assert!(state_of(&m).get(&m.key("work")).is_some());

    // Forgotten by the doubt rule: `File System` was not there when the leave started.
    let m = desktop_pair("settled-then-doubt");
    killed(&m, "leave.jar_out", "home");
    std::fs::create_dir(data(&m).join("File System")).expect("an item the move never listed");
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let done = pitboard.forget("desktop/home").expect("forgotten");
    assert!(
        done.warnings
            .iter()
            .any(|warning| warning.code() == "interrupted_move_finished"),
        "{:?}",
        done.warnings
    );
    assert!(state_of(&m).get(&m.key("home")).is_none());
}

// A step that fails after the change settled a move still says the move settled.
#[test]
fn a_failure_after_a_settled_move_still_says_it_settled() {
    use crate::host::fs::testing;
    let undone = "interrupted_move_undone";
    let m = desktop_pair("settled-then-scheduled");
    killed(&m, "leave.keys_out", "home");
    crate::schedule::install(&m.ctx, Permit::for_a_test()).expect("scheduled");
    let dir = crate::schedule::path(&m.ctx)
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .expect("where the scheduler keeps it");
    testing::deny_changes(&dir).expect("read-only");
    settled_then_refused(&m, "home_unwritable", undone, |pitboard| {
        pitboard.uninstall().expect_err("refused")
    });
    testing::allow_changes(&dir).expect("changeable again");
    assert!(crate::home::dir(&m.ctx).is_dir());

    let m = desktop_pair("settled-then-unsaved");
    let park = park_of(&m, "home").expect("parked").service;
    killed(&m, "leave.keys_out", "home");
    let file = crate::home::dir(&m.ctx).join("state.json");
    let kept = crate::home::dir(&m.ctx).join("state.kept");
    let failed = crate::fault::meanwhile(
        "settle.saved",
        {
            let (file, kept) = (file.clone(), kept.clone());
            move || {
                std::fs::rename(&file, &kept).expect("moved");
                std::fs::create_dir_all(file.join("in the way")).expect("a folder in its place");
            }
        },
        || forget(settled(&m), &m.key("home")),
    );
    std::fs::remove_dir_all(&file).expect("taken out of the way");
    std::fs::rename(&kept, &file).expect("put back");
    let mut failed = failed.expect_err("refused");
    assert_eq!(failed.code(), "state_write_failed", "{failed}");
    let said = failed.take_warnings();
    assert!(
        said.iter().any(|warning| warning.code() == undone),
        "{said:?}"
    );
    // What forget deletes goes only after its record is saved.
    assert!(crate::store::folder::at(&m.ctx, &park).is_dir());
    assert_eq!(whereabouts(&m, &HOME), [park]);
}

// What settling set aside is said before the refusal, with the move it settled. The keys
// Claude rewrote since the arrive are all the folder kept holds.
#[test]
fn a_refusal_after_a_settled_move_says_all_it_did_first() {
    let m = desktop_pair("superseded-then-refused");
    killed(&m, "arrive.keys_in", "home");
    renewed(&m);
    let failed = crate::service::Pitboard::new(m.ctx.clone())
        .update_config("desktop/work")
        .expect_err("refused");
    assert_eq!(
        failed.error.code(),
        "account_not_in_use",
        "{}",
        failed.error
    );
    let codes =
        |said: &[Warning]| -> Vec<&'static str> { said.iter().map(Warning::code).collect() };
    assert_eq!(
        codes(&failed.warnings),
        ["interrupted_move_finished", "set_aside"]
    );
    assert_eq!(failed.said_first, 2);
    assert_eq!(
        failed.warnings[1].to_string(),
        format!(
            "Claude rewrote `desktop/home`'s keys. Pitboard kept the earlier ones in {} and \
             deleted nothing. `pitboard doctor` lists them.",
            superseded(&m).display()
        )
    );
}

// Keys Claude rewrote while a leave waited are kept aside, and the change says so.
#[test]
fn keys_claude_rewrote_during_a_leave_are_said_set_aside() {
    let m = desktop_pair("keys-rewritten");
    killed(&m, "leave.keys_kept", "home");
    renewed(&m);
    let done = crate::service::Pitboard::new(m.ctx.clone())
        .update_config("desktop/work")
        .expect("`work` in use");
    // The leave never moved the jar: the folder kept holds only the keys.
    let said: Vec<String> = done
        .warnings
        .iter()
        .filter(|warning| warning.code() == "set_aside")
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        said,
        [format!(
            "Claude rewrote `desktop/work`'s keys. Pitboard kept the earlier ones in {} and \
             deleted nothing. `pitboard doctor` lists them.",
            superseded(&m).display()
        )]
    );
}

#[test]
fn recovery_stops_where_the_jar_is_in_neither_place_or_both() {
    let m = desktop_pair("neither-or-both");
    killed(&m, "leave.jar_out", "home");
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let parked = crate::store::folder::item(&m.ctx, &park, "Cookies");
    // In both: the parked jar copied back, as a restore from a backup would.
    std::fs::copy(&parked, data(&m).join("Cookies")).expect("copied");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = confirm_(&m, "work").expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    assert!(stopped.to_string().contains("in both places"), "{stopped}");
    unchanged(&m, &before, "in both places");
    std::fs::remove_file(data(&m).join("Cookies")).expect("the copy gone");

    // In neither: the parked jar taken away.
    let away = m.ctx_home().join("Cookies-away");
    std::fs::rename(&parked, &away).expect("taken away");
    let before = snapshot(&m);
    let stopped = confirm_(&m, "work").expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    assert!(
        stopped.to_string().contains("in neither place"),
        "{stopped}"
    );
    unchanged(&m, &before, "in neither place");

    std::fs::rename(&away, &parked).expect("back");
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert!(park_of(&m, "work").is_some(), "the leave was finished");
    rested(&m, "finished once the jar was back");
}

#[test]
fn a_leave_finished_after_claude_ran_signed_out_verifies_and_commits() {
    let m = desktop_pair("ran-signed-out");
    killed(&m, "leave.item_out", "home");
    let claude = m.claude();
    claude.opens();
    claude.quits();
    let inode = |item: &str| crate::host::fs::testing::inode(&data(&m).join(item)).ok();
    let own = (inode("Cookies"), inode("Cookies-journal"));
    assert!(
        own.0.is_some() && own.1.is_some(),
        "Claude made its own jar"
    );
    let settings = std::fs::read(data(&m).join("config.json")).expect("settings");
    assert!(String::from_utf8_lossy(&settings).contains(EMPTIED));

    let gave = gave_up(&m).expect("settled");
    assert!(gave.moves.is_empty(), "nothing given up: {gave:?}");
    assert!(
        matches!(
            gave.warnings.as_slice(),
            [Warning::MoveSettled(moved)]
                if moved.half == state::Half::Leave && moved.ended == crate::switch::folder::Ended::Finished
        ),
        "{:?}",
        gave.warnings
    );
    assert_eq!((inode("Cookies"), inode("Cookies-journal")), own);
    assert_eq!(
        std::fs::read(data(&m).join("config.json")).expect("settings"),
        settings,
        "the settings are not written"
    );
    let parked = park_of(&m, "work").expect("parked");
    let manifest = crate::store::folder::manifest(&m.ctx, &parked.service)
        .expect("reads")
        .expect("there");
    assert!(manifest.items.contains(&"Local Storage".to_string()));
    assert!(!data(&m).join("Local Storage").exists());
    assert_eq!(whereabouts(&m, &WORK), [parked.service]);
    assert_eq!(in_use(&m), None);
    hold(&m, "a leave finished");
    rested(&m, "a leave finished");
    assert!(matches!(used(&m, "home").0, Outcome::Installed { .. }));
}

#[test]
fn a_temporary_left_inside_a_settings_write_is_swept() {
    let m = desktop_pair("temporary");
    let settings = std::fs::read(data(&m).join("config.json")).expect("settings");
    killed(&m, "settings.temp_written", "home");
    let temporaries = || {
        std::fs::read_dir(data(&m))
            .expect("listed")
            .flatten()
            .filter(|entry| {
                crate::atomic::temp_owner(&entry.file_name().to_string_lossy()).is_some()
            })
            .count()
    };
    assert_eq!(temporaries(), 1, "the kill left one");
    assert!(matches!(
        confirm_(&m, "work").expect("undone"),
        Outcome::AlreadyActive { .. }
    ));
    assert_eq!(temporaries(), 0, "swept");
    assert_eq!(
        std::fs::read(data(&m).join("config.json")).expect("settings"),
        settings
    );
    rested(&m, "swept");
}

#[test]
fn a_power_cut_that_undid_a_committed_rename_is_refused_and_moves_nothing() {
    let m = desktop_pair("power-cut");
    killed(&m, "leave.done", "home");
    let park = park_of(&m, "work").expect("parked").service;
    std::fs::rename(
        crate::store::folder::item(&m.ctx, &park, "Cookies"),
        data(&m).join("Cookies"),
    )
    .expect("the rename undone");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    assert!(
        ["sign_in_unfinished", "login_in_doubt"].contains(&stopped.code()),
        "row 12 or 20: {stopped}"
    );
    unchanged(&m, &before, "nothing deleted, nothing moved");
}

#[test]
fn a_second_aside_never_writes_over_the_first() {
    let m = desktop_pair("two-asides");
    killed(&m, "arrive.jar_in", "home");
    // `home`'s jar is in Claude's folder with no keys. Claude's own, emptied, come by hand.
    let emptied: Value = serde_json::from_str(EMPTIED).expect("JSON");
    set(&m, "oauth:tokenCache", Some(emptied.clone()));
    let recovering = |at: &'static str| {
        let died = crate::fault::killing(at, || confirm_(&m, "work"));
        assert_eq!(died.expect_err("killed"), at);
        as_a_dead_runs(&m);
        state_of(&m)
            .moving(ProviderId::Desktop)
            .expect("the record is kept")
            .asides
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    let first = recovering("settle.aside_kept");
    assert_eq!(first.len(), 1, "{first:?}");
    let kept = |name: &str| {
        std::fs::read(crate::store::folder::keys_file(&m.ctx, name)).expect("its keys")
    };
    let first_keys = kept(&first[0]);
    set(&m, "oauth:tokenCacheV2", Some(emptied));
    let both = recovering("settle.keys_cleared");
    assert_eq!(
        both.len(),
        2,
        "a new aside, never the one the record names: {both:?}"
    );
    assert_eq!(
        kept(&first[0]),
        first_keys,
        "the first is as it was written"
    );
    let second = both
        .iter()
        .find(|name| **name != first[0])
        .expect("new")
        .clone();
    let second_keys = kept(&second);
    assert_ne!(second_keys, first_keys);

    // Recovery takes `home`'s jar back and is killed once it has; then somebody signs in.
    let died = crate::fault::killing("settle.item_back", || confirm_(&m, "work"));
    assert_eq!(died.expect_err("killed"), "settle.item_back");
    as_a_dead_runs(&m);
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let (ctx, named) = (m.ctx.clone(), std::rc::Rc::clone(&seen));
    let read = move || {
        let state = state::load(&ctx).expect("state");
        let moving = state.moving(ProviderId::Desktop).expect("recorded");
        *named.borrow_mut() = moving.asides.keys().cloned().collect::<Vec<_>>();
    };
    crate::fault::meanwhile("give_up.aside_named", read, || enrol(&m, "third"))
        .expect("enrolled, what `home` left set aside");
    let all = seen.borrow().clone();
    assert_eq!(all.len(), 3, "a third, never one the record names: {all:?}");
    let given_up = all.iter().find(|name| !both.contains(name)).expect("new");
    assert_eq!(kept(&first[0]), first_keys, "each as it was written");
    assert_eq!(kept(&second), second_keys, "each as it was written");

    let state = state_of(&m);
    let why = |name: &str| {
        let aside = state.aside.iter().find(|a| a.name == name);
        aside.map(|a| (a.why, a.login.clone()))
    };
    for name in &both {
        assert_eq!(why(name), Some((SetAside::InTheWay, None)), "{name}");
    }
    assert_eq!(why(given_up), Some((SetAside::GivenUp, None)));
    assert!(park_of(&m, "home").is_some(), "`home` keeps its park");
    assert_eq!(in_use(&m).as_deref(), Some("third"));
    hold(&m, "three asides");
    rested(&m, "three asides");
}
