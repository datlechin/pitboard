use super::{
    SAYS_WITHIN, enrol, forget_, in_use, interrupted, killed, listed, named_in_settings, park_of,
    reads_only, said_to_the_end, said_until, set, state_of, three_tools, used, watched,
    without_items,
};
use super::{data, rested, settled, snapshot, unchanged, whereabouts};
use crate::desktop_testing::EMPTIED;
use crate::error::Error;
use crate::permit::Permit;
use crate::provider::ProviderId;
use crate::state::{self, SetAside};
use crate::switch::harness::{
    HOME, STRANGER, WORK, audit_lines, desktop_machine, desktop_pair, hold, hold_where_deleted,
    recover, sign_in_inside,
};
use crate::switch::{Outcome, enroll};
use crate::warning::Warning;
use std::sync::mpsc::RecvTimeoutError;

#[test]
fn a_sign_in_parks_the_account_in_use_and_enrols_whoever_signs_in() {
    let m = desktop_machine("sign-in-inside");
    let (enrolled, warnings) = sign_in_inside(&m, "home", &HOME).expect("added");
    assert!(
        matches!(
            enrolled,
            crate::switch::folder::Enrolled::InUse { again: false, .. }
        ),
        "{enrolled:?}"
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    let parked = park_of(&m, "work").expect("`work` was parked for it");
    assert_eq!(whereabouts(&m, &WORK), [parked.service]);
    assert_eq!(whereabouts(&m, &HOME), ["Claude"]);
    let state = state::load(&m.ctx).expect("state");
    assert!(state.awaiting.is_empty() && state.moving.is_empty());
    hold(&m, "a sign-in inside Claude");
    rested(&m, "a sign-in inside Claude");
}

#[test]
fn a_sign_in_left_halfway_is_said_by_every_read_and_put_back_by_use() {
    let m = desktop_pair("halfway");
    let waiting = watched(&m, "travel").expect("`work` parked, waiting");
    assert_eq!(waiting.put_back(), Some(m.key("work")));
    assert_eq!(
        waiting.steps().now(),
        Some(crate::switch::WaitStep::Waiting)
    );
    let lines = said_until(&waiting.said(), "Waiting for Claude to quit");
    assert_eq!(
        lines,
        [
            "Parked `desktop/work`. Claude is signed out, and nothing was signed out at \
             claude.ai.\n",
            "Open Claude and sign in to the account to add as `desktop/travel`. When Claude \
             shows your chats, quit it with Command-Q.\n",
            "Do not choose Log out in Claude: it ends the login Claude holds at claude.ai, and \
             Pitboard cannot bring it back.\n",
            "Waiting for Claude to quit. Control-C stops waiting and keeps `desktop/work` \
             parked; `pitboard use desktop/work` puts it back.\n",
        ]
    );
    waiting.cancel();

    let state = state_of(&m);
    let awaited = state
        .awaiting(ProviderId::Desktop)
        .expect("the record stands");
    assert_eq!(
        (awaited.label.as_str(), awaited.account.as_deref()),
        ("travel", None)
    );
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    assert!(
        pitboard.whole(ProviderId::Desktop),
        "Claude may be opened to sign in"
    );
    reads_only(&m, &pitboard, "a sign-in awaited");
    for read in [
        pitboard.status_offline().expect("read"),
        pitboard.status(true).expect("read"),
    ] {
        assert!(
            read.warnings.iter().any(|warning| matches!(
                warning,
                Warning::SignInAwaited { label, put_back, again: false, .. }
                    if label == "desktop/travel" && put_back == "desktop/work"
            )),
            "{:?}",
            read.warnings
        );
        assert_eq!(read.value.awaiting.len(), 1);
        assert_eq!(read.value.awaiting[0].put_back, m.key("work"));
    }
    let checks = pitboard.doctor().checks;
    assert!(checks.iter().any(|check| check.code == "desktop_awaiting"));

    assert!(matches!(used(&m, "work").0, Outcome::Installed { .. }));
    assert!(state_of(&m).awaiting.is_empty(), "the arrive ends the wait");
    rested(&m, "put back");
}

#[test]
fn the_wait_reads_the_process_list_and_no_jar() {
    let m = desktop_pair("wait-reads");
    let index = crate::home::dir(&m.ctx).join("state.json");
    let waiting = watched(&m, "travel").expect("waiting");
    said_until(&waiting.said(), "Waiting for Claude to quit");
    let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
    let (settings, recorded) = (
        std::fs::read(data(&m).join("config.json")).expect("settings"),
        std::fs::read(&index).expect("state"),
    );
    listed(&m, listings + 5);
    assert_eq!(m.mem.jars_asked().len(), jars, "no jar");
    assert_eq!(std::fs::read(&index).expect("state"), recorded);
    assert_eq!(
        std::fs::read(data(&m).join("config.json")).expect("settings"),
        settings
    );
    waiting.cancel();
}

#[test]
fn a_resumed_add_says_how_to_put_its_account_back() {
    let m = desktop_pair("resumed");
    watched(&m, "travel").expect("`work` parked").cancel();
    let resumed = watched(&m, "elsewhere").expect("taken up");
    assert_eq!(resumed.put_back(), Some(m.key("work")));
    let lines = said_until(&resumed.said(), "Waiting for Claude to quit");
    assert_eq!(
        lines.last().map(String::as_str),
        Some(
            "Waiting for Claude to quit. Control-C stops waiting and keeps `desktop/work` \
             parked; `pitboard use desktop/work` puts it back.\n"
        )
    );
    assert!(!lines.iter().any(|line| line.starts_with("Parked")));
    resumed.cancel();
    assert_eq!(
        state_of(&m)
            .awaiting(ProviderId::Desktop)
            .map(|a| (a.label.clone(), a.put_back.clone())),
        Some(("elsewhere".into(), format!("desktop_{}", WORK.uuid)))
    );
}

#[test]
fn cancelling_a_sign_in_that_parked_nothing_leaves_no_record() {
    let m = desktop_pair("parked-nothing");
    killed(&m, "leave.done", "home");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let waiting = watched(&m, "travel").expect("waiting, nothing to park");
    assert_eq!(waiting.put_back(), None);
    let lines = said_until(&waiting.said(), "Waiting for Claude to quit");
    assert!(
        !lines.iter().any(|line| line.starts_with("Parked")),
        "{lines:?}"
    );
    waiting.cancel();
    unchanged(&m, &before, "nothing to undo");
    assert!(state_of(&m).awaiting.is_empty());
}

// A move another run left while the wait went on: the wait's look settles it, and says so.
#[test]
fn the_wait_says_a_move_its_look_settled() {
    let m = desktop_pair("wait-settles");
    let waiting = watched(&m, "travel").expect("waiting");
    let (said, claude) = (waiting.said(), m.claude());
    said_until(&said, "Waiting for Claude to quit");
    killed(&m, "arrive.recorded", "home");
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.crashes();
    let lines = said_until(&said, "Claude quit with nobody signed in.");
    assert!(
        lines.iter().any(|line| line.starts_with(
            "an earlier move of `desktop/home`'s login was interrupted; Pitboard took it back out"
        )),
        "{lines:?}"
    );
    waiting.cancel();
    assert!(state_of(&m).moving.is_empty());
}

#[test]
fn still_signed_out_is_still_waiting() {
    let m = desktop_pair("still-waiting");
    let waiting = watched(&m, "travel").expect("waiting");
    let (said, steps, claude) = (waiting.said(), waiting.steps(), m.claude());
    claude.opens();
    said_until(&said, "Claude is open.");
    assert_eq!(steps.now(), Some(crate::switch::WaitStep::Open));
    claude.quits();
    said_until(&said, "Claude quit with nobody signed in. Still waiting.");
    assert_eq!(steps.now(), Some(crate::switch::WaitStep::QuitSignedOut));
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.quits_before_reporting();
    said_until(&said, "Claude quit before it finished signing in.");
    assert_eq!(steps.now(), Some(crate::switch::WaitStep::QuitUnfinished));
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("signed in at last");
    enroll(settled(&m), &m.key("travel"), Some(login)).expect("enrolled");
    assert_eq!(in_use(&m).as_deref(), Some("travel"));
}

#[test]
fn the_wait_ends_where_its_sign_in_was_settled_elsewhere() {
    // Another command puts `work` back meanwhile: this wait enrolls nothing.
    let m = desktop_pair("settled-elsewhere");
    let waiting = watched(&m, "travel").expect("waiting");
    let (said, claude) = (waiting.said(), m.claude());
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.quits();
    said_until(&said, "Still waiting.");
    used(&m, "work");
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.crashes();
    let ended = waiting.finish().expect_err("settled elsewhere");
    assert_eq!(ended.code(), "sign_in_settled_elsewhere");
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    // Nothing waits any more, so a second sign-in may start.
    watched(&m, "travel").expect("free to start").cancel();

    // The home removed meanwhile.
    let m = desktop_pair("home-removed");
    let waiting = watched(&m, "travel").expect("waiting");
    let said = waiting.said();
    m.claude().opens();
    said_until(&said, "Claude is open.");
    let home = crate::home::dir(&m.ctx);
    std::fs::remove_dir_all(&home).expect("removed meanwhile");
    m.claude().crashes();
    let ended = waiting.finish().expect_err("ended");
    assert_eq!(ended.code(), "sign_in_settled_elsewhere");
    assert!(!home.exists());
}

#[test]
fn the_wait_never_makes_a_home_uninstall_removed() {
    let m = desktop_machine("home-gone");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    let waiting = watched(&m, "work").expect("waiting, nothing parked");
    assert_eq!(waiting.put_back(), None);
    let lines = said_until(&waiting.said(), "Waiting for Claude to quit");
    assert!(
        lines
            .last()
            .expect("said")
            .ends_with("Control-C stops waiting.\n")
    );
    assert!(
        lines[0].contains("moved what it found in the way"),
        "the keys Claude kept were set aside first: {lines:?}"
    );
    assert!(
        named_in_settings(&m).is_none(),
        "the wait starts with no name"
    );
    assert!(
        state_of(&m).awaiting.is_empty(),
        "nothing parked, nothing recorded"
    );
    let home = crate::home::dir(&m.ctx);
    claude.opens();
    said_until(&waiting.said(), "Claude is open.");
    std::fs::remove_dir_all(&home).expect("uninstalled meanwhile");
    claude.crashes();
    let ended = waiting.finish().expect_err("ended");
    assert_eq!(ended.code(), "sign_in_settled_elsewhere");
    assert!(!home.exists(), "no directory was made");
}

#[test]
fn an_add_from_a_logged_out_claude_never_files_a_session_under_the_old_name() {
    let m = desktop_machine("logged-out");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    assert_eq!(
        named_in_settings(&m).as_deref(),
        Some(WORK.uuid),
        "E1b keeps the name"
    );
    let waiting = watched(&m, "travel").expect("waiting");
    let said = waiting.said();
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.quits_before_reporting();
    said_until(&said, "Claude quit before it finished signing in.");
    assert!(
        named_in_settings(&m).is_none(),
        "no stale name to take it under"
    );
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("signed in");
    let (enrolled, _) = enroll(settled(&m), &m.key("travel"), Some(login)).expect("enrolled");
    assert!(matches!(
        enrolled,
        crate::switch::folder::Enrolled::InUse { again: false, .. }
    ));
    let state = state_of(&m);
    assert_eq!(
        state.get(&m.key("travel")).expect("enrolled").account_uuid,
        STRANGER.uuid
    );
    assert_eq!(state.aside.len(), 1, "the keys Claude kept, set aside");
    assert_eq!(state.aside[0].why, SetAside::InTheWay);
    hold(&m, "an add after a Log out");
}

// Rows 3, 5 and 10: `work`'s session is gone, so the start of a sign-in records nobody in use.
#[test]
fn a_sign_in_from_a_logged_out_claude_records_nobody_in_use() {
    // Rows 5 and 3: nothing to set aside, so the record is the save.
    let emptied = desktop_machine("emptied-nobody");
    without_items(&emptied);
    for key in [
        "oauth:tokenCache",
        "oauth:tokenCacheV2",
        "lastKnownAccountUuid",
    ] {
        set(&emptied, key, None);
    }
    let no_folder = desktop_machine("no-folder-nobody");
    std::fs::remove_dir_all(data(&no_folder)).expect("removed");
    for (m, row) in [(emptied, "row 5"), (no_folder, "row 3")] {
        watched(&m, "travel").expect("waiting").cancel();
        let state = state_of(&m);
        let record = state.in_use(ProviderId::Desktop).expect("recorded");
        assert_eq!((&record.owner, &record.named), (&None, &None), "{row}");
        assert!(state.aside.is_empty(), "{row}: {:?}", state.aside);
        assert!(
            audit_lines(&m, "in-use").contains(&(String::new(), "signed_out_outside".into())),
            "{row}: {:?}",
            audit_lines(&m, "in-use")
        );
        hold_where_deleted(&m, row);
    }

    // Row 10: the keys Log out left are set aside in the same save.
    let m = desktop_machine("logged-out-nobody");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    let waiting = watched(&m, "travel").expect("waiting");
    said_until(&waiting.said(), "Waiting for Claude to quit");
    waiting.cancel();
    let state = state_of(&m);
    let record = state.in_use(ProviderId::Desktop).expect("recorded");
    assert_eq!((&record.owner, &record.named), (&None, &None));
    assert!(
        state
            .get(&m.key("work"))
            .expect("enrolled")
            .replaced_at
            .is_some(),
        "`work`'s only login was the one Log out ended"
    );
    assert!(
        audit_lines(&m, "in-use").contains(&(String::new(), "signed_out_outside".into())),
        "{:?}",
        audit_lines(&m, "in-use")
    );
    let read = crate::service::Pitboard::new(m.ctx.clone())
        .status_offline()
        .expect("read");
    assert!(
        !read
            .warnings
            .iter()
            .any(|warning| warning.code() == "in_use_unconfirmed"),
        "{:?}",
        read.warnings
    );
    hold(&m, "a sign-in started after a Log out");

    // Claude writes an emptied cache again: the next start saves its aside though the record
    // already names nobody.
    set(
        &m,
        "oauth:tokenCache",
        Some(serde_json::from_str(EMPTIED).expect("JSON")),
    );
    watched(&m, "travel").expect("waiting").cancel();
    let state = state_of(&m);
    let asides = state.aside.iter().filter(|a| a.why == SetAside::InTheWay);
    assert_eq!(asides.count(), 2, "{:?}", state.aside);
    hold(&m, "a sign-in started again after a Log out");
}

#[test]
fn signing_in_again_as_another_account_is_refused_and_named() {
    let m = desktop_pair("again");
    // Sign In Again of `home`, and Claude is signed in to somebody else.
    let waiting = watched(&m, "home").expect("waiting");
    let said = waiting.said();
    let lines = said_until(&said, "Waiting for Claude to quit");
    assert!(
        lines[1].contains("sign in to `desktop/home` again"),
        "{lines:?}"
    );
    let claude = m.claude();
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("somebody signed in");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let refused = enroll(settled(&m), &m.key("home"), Some(login)).expect_err("refused");
    match &refused {
        Error::LabelTaken { label, who } => assert_eq!(
            (label.as_str(), who.as_str()),
            ("desktop/home", "another Claude Desktop account"),
            "who is signed in is not `home`'s account"
        ),
        other => panic!("{}: {other}", other.code()),
    }
    unchanged(&m, &before, "the record of the sign-in stands");
    assert!(state_of(&m).awaiting(ProviderId::Desktop).is_some());

    // Waited for under a new label, and Claude is signed in to an enrolled account.
    enrol(&m, "third").expect("the stranger enrolled under its own label");
    let waiting = watched(&m, "fourth").expect("`third` parked, waiting");
    let said = waiting.said();
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("signed in");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let refused = enroll(settled(&m), &m.key("fourth"), Some(login)).expect_err("refused");
    assert_eq!(refused.code(), "already_enrolled");
    assert!(
        refused
            .to_string()
            .contains("Run `pitboard enroll desktop/third` to record that login"),
        "{refused}"
    );
    unchanged(&m, &before, "the record of the sign-in stands");
    assert!(state_of(&m).awaiting(ProviderId::Desktop).is_some());
}

#[test]
fn forgetting_the_account_an_add_would_put_back_leaves_no_dangling_put_back() {
    let m = desktop_pair("forget-put-back");
    watched(&m, "travel").expect("`work` parked").cancel();
    assert!(state_of(&m).awaiting(ProviderId::Desktop).is_some());
    forget_(&m, "work").expect("forgotten: it is parked, and nobody is in use");
    let state = state_of(&m);
    assert!(state.awaiting.is_empty(), "nothing is left to put back");
    assert!(state.get(&m.key("work")).is_none());
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let warnings = pitboard.status_offline().expect("read").warnings;
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(w, Warning::SignInAwaited { .. })),
        "{warnings:?}"
    );
}

#[test]
fn a_sign_in_inside_claude_says_what_settling_found_first() {
    let m = three_tools("sign-in-settles");
    interrupted(&m, ProviderId::Claude);
    let waiting = watched(&m, "travel").expect("waiting");
    let lines = said_until(&waiting.said(), "Waiting for Claude to quit");
    waiting.cancel();
    assert!(!crate::switch::journal::pending(&m.ctx), "settled");
    let parked = lines
        .iter()
        .position(|line| line.starts_with("Parked `desktop/work`"))
        .expect("parked");
    assert!(
        lines[..parked]
            .iter()
            .any(|line| line.contains("switch") && line.contains("interrupted")),
        "{lines:?}"
    );
}

#[test]
fn the_start_of_a_sign_in_writes_a_manifest_before_it_sets_keys_aside() {
    for at in ["start.aside_kept", "start.keys_cleared"] {
        let m = desktop_machine(&format!("start-{at}"));
        let claude = m.claude();
        claude.opens();
        claude.logs_out();
        claude.quits();
        let died = crate::fault::killing(at, || watched(&m, "travel").map(drop));
        assert_eq!(died.expect_err("killed"), at);
        let asides = crate::store::folder::list(&m.ctx)
            .expect("parks")
            .expect("there");
        assert_eq!(asides.len(), 1, "{at}: {asides:?}");
        let manifest = crate::store::folder::manifest(&m.ctx, &asides[0])
            .expect("reads")
            .expect("written before the settings");
        assert_eq!((manifest.account, manifest.login), (None, None));
        let kept = std::fs::read_to_string(crate::store::folder::keys_file(&m.ctx, &asides[0]))
            .expect("the keys, kept");
        assert!(kept.contains(WORK.uuid), "{at}: {kept}");
        assert_eq!(
            named_in_settings(&m).is_some(),
            at == "start.aside_kept",
            "{at}: the keys are in the settings or in the aside, never in neither"
        );
        assert!(
            state_of(&m).aside.is_empty(),
            "{at}: only `pending` names it"
        );

        recover(&m).expect("the next command's sweep");
        let state = state_of(&m);
        assert_eq!(state.aside.len(), 1, "{at}");
        assert_eq!(state.aside[0].why, SetAside::Orphan, "{at}");
        assert!(
            crate::store::folder::at(&m.ctx, &asides[0]).is_dir(),
            "{at}: kept"
        );
        watched(&m, "travel")
            .expect("the sign-in starts again")
            .cancel();
    }
}

#[test]
fn the_wait_goes_on_where_claude_opens_again_or_another_pitboard_holds_the_folder() {
    let m = desktop_pair("wait-goes-on");
    let waiting = watched(&m, "travel").expect("waiting");
    let (said, steps, claude) = (waiting.said(), waiting.steps(), m.claude());
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    // Claude quits and is open again before the wait has looked: it waits for the next quit.
    let change = crate::switch::exclusive(&m.ctx, Permit::for_a_test()).expect("state.lock");
    claude.quits();
    // A listing counted after this read Claude closed: the wait is in its look, behind the lock.
    let quit = m.mem.listings();
    listed(&m, quit + 1);
    claude.opens();
    drop(change);
    let listings = m.mem.listings();
    listed(&m, listings + 5);
    assert_eq!(steps.now(), Some(crate::switch::WaitStep::Open));
    // Another Pitboard holds the folder as Claude quits: the wait looks again at the next tick.
    let held = crate::host::fs::lock_dir(&data(&m)).expect("another home holds the folder");
    claude.quits();
    let listings = m.mem.listings();
    listed(&m, listings + 5);
    held.unlock().expect("let go");
    let login = waiting
        .finish()
        .expect("it looked again, and somebody is signed in");
    let lines = said_to_the_end(&said);
    assert!(lines.is_empty(), "no quit was said: {lines:?}");
    let (enrolled, _) = enroll(settled(&m), &m.key("travel"), Some(login)).expect("enrolled");
    assert!(matches!(
        enrolled,
        crate::switch::folder::Enrolled::InUse { again: false, .. }
    ));
    assert_eq!(in_use(&m).as_deref(), Some("travel"));
    rested(&m, "enrolled after the wait");
}

#[test]
fn the_wait_runs_on_its_own_thread_and_finish_joins_it() {
    // Made with no wait of its own, so a wait that never ends fails this test and not its set-up.
    let m = desktop_machine("own-thread");
    // It returns while Claude has never been opened, and asks the process list on its own.
    let waiting = watched(&m, "travel").expect("returns at once");
    let (said, claude) = (waiting.said(), m.claude());
    said_until(&said, "Waiting for Claude to quit");
    let listings = m.mem.listings();
    listed(&m, listings + 3);
    // It holds no lock while it waits but the sign-in's own.
    let change = crate::switch::try_exclusive(&m.ctx, Permit::for_a_test());
    assert!(change.is_some(), "state.lock is free");
    drop(change);
    assert!(
        crate::host::fs::lock_dir(&data(&m)).is_ok(),
        "the folder is free"
    );
    let second = watched(&m, "another").expect_err("refused");
    assert_eq!(second.code(), "sign_in_in_progress");

    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("signed in");
    assert_eq!(
        said.within(SAYS_WITHIN),
        Err(RecvTimeoutError::Disconnected),
        "the thread has ended, and its lines with it"
    );
    enroll(settled(&m), &m.key("travel"), Some(login)).expect("enrolled");

    // `cancel` joins it too, and lets the sign-in's lock go.
    let waiting = watched(&m, "fourth").expect("`travel` parked, waiting");
    let said = waiting.said();
    waiting.cancel();
    let lines = said_to_the_end(&said);
    assert!(
        lines
            .last()
            .is_some_and(|line| line.starts_with("Waiting for Claude to quit"))
    );

    // `finish` answers what the thread ended with.
    let waiting = watched(&m, "fourth").expect("free to start again");
    let said = waiting.said();
    claude.opens();
    said_until(&said, "Claude is open.");
    m.mem.jar_unsettled(&data(&m).join("Cookies"));
    claude.quits();
    let ended = waiting.finish().expect_err("the look refused");
    assert_eq!(ended.code(), "login_folder_unreadable", "{ended}");
}
