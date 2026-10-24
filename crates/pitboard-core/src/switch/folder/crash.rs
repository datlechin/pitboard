// A move killed at every point it reaches, with Claude doing each thing it can do before the
// next change, then recovered by that change's first step.

use super::super::harness::{HOME, Rig, STRANGER, WORK, desktop_pair, hold, recover};
use super::super::{abandon, enroll, switch, update_config};
use super::tests::{
    FOLDER_POINTS, as_a_dead_runs, data, rested, settled, snapshot, stats, tree, unchanged,
};
use crate::desktop_testing::{EMPTIED, Person};
use crate::error::Error;
use crate::fault;
use crate::provider::ProviderId;
use crate::service::{Permit, Pitboard, Warning};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    // a switch from `work` to `home`
    Switch,
    // the leave of `enroll --sign-in`
    Park,
    // the start of `enroll --sign-in` from a Claude that logged out
    Start,
    // an arrive into a folder Claude left signed out, so with an aside
    ArriveOver,
    // an arrive into an empty folder with no `IndexedDB`
    ArriveEmpty,
}

const CHANGES: [Change; 5] = [
    Change::Switch,
    Change::Park,
    Change::Start,
    Change::ArriveOver,
    Change::ArriveEmpty,
];

// What Claude does between the kill and the recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claude {
    NeverRan,
    // opened and quit as whoever its folder held: nobody, or the account whose jar was there
    RanAndQuit,
    Renewed,
    Stranger,
    StillOpen,
    Crashed,
}

// Whose session the jar in Claude's folder holds, of the two accounts.
fn in_the_folder(m: &Rig) -> Option<Person> {
    let raw = std::fs::read_to_string(data(m).join("Cookies")).ok()?;
    [WORK, HOME]
        .into_iter()
        .find(|who| raw.contains(&hex::encode(format!("v10{}", who.session))))
}

fn machine(change: Change, name: &str) -> Rig {
    let m = desktop_pair(&format!("crash-{change:?}-{name}"));
    let signed_out = |m: &Rig| {
        let died = fault::killing("leave.done", || switch(settled(m), &m.key("home")));
        assert_eq!(died.expect_err("killed"), "leave.done");
    };
    match change {
        Change::Switch | Change::Park => {}
        Change::Start => {
            let claude = m.claude();
            claude.opens();
            claude.logs_out();
            claude.quits();
        }
        Change::ArriveOver => {
            signed_out(&m);
            let claude = m.claude();
            claude.opens();
            claude.quits();
        }
        Change::ArriveEmpty => {
            signed_out(&m);
            std::fs::remove_dir(data(&m).join("IndexedDB")).expect("no parent");
        }
    }
    recover(&m).expect("swept");
    m
}

fn run(m: &Rig, change: Change) -> Result<(), Error> {
    match change {
        Change::Switch | Change::ArriveOver | Change::ArriveEmpty => {
            switch(settled(m), &m.key("home")).map(drop)
        }
        Change::Park | Change::Start => Pitboard::new(m.ctx.clone())
            .sign_in_watched("desktop/travel")
            .map(super::super::WatchedSignIn::cancel)
            .map_err(|failed| failed.error),
    }
}

// The next change's first step: `begin`, which settles, and for the start of a sign-in the
// sweep, since it leaves no record but its `pending` line.
fn recovery(m: &Rig) -> Result<(), Error> {
    match update_config(settled(m), &m.key("work")) {
        Ok(_) => Ok(()),
        // By its code: where settling did anything, the refusal comes with what it said.
        Err(refused) if refused.code() == "account_not_in_use" => Ok(()),
        Err(refused) => Err(refused),
    }
}

// What any next command sweeps first, `pending`'s lines and Pitboard's own temporaries, taken
// before a snapshot so the snapshot compares what recovery's row did.
fn swept(m: &Rig) {
    recover(m).expect("swept");
    crate::atomic::sweep(Permit::for_a_test(), &data(m));
}

fn walk(dir: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        // Pitboard's own temporary, which a recovery sweeps before it reads anything.
        let name = entry.file_name().to_string_lossy().into_owned();
        if crate::atomic::temp_owner(&name).is_some() {
            continue;
        }
        found.push(path.display().to_string());
        if std::fs::symlink_metadata(&path).is_ok_and(|kind| kind.is_dir()) {
            found.extend(walk(&path));
        }
    }
    found
}

fn inode(path: PathBuf) -> Option<u64> {
    crate::host::fs::testing::inode(&path).ok()
}

// By inode, which a rename keeps; not the settings, which each write of them makes anew.
fn entries(m: &Rig) -> BTreeSet<(String, Option<u64>)> {
    walk(&data(m))
        .into_iter()
        .filter(|path| !path.ends_with("/config.json"))
        .map(|path| {
            let inode = inode(PathBuf::from(&path));
            (path, inode)
        })
        .collect()
}

fn app() -> &'static dyn crate::provider::folder::Folder {
    match crate::provider::of(ProviderId::Desktop) {
        crate::provider::LoginKind::Folder(app) => app,
        crate::provider::LoginKind::Document(_) => unreachable!("a folder tool"),
    }
}

fn keys(m: &Rig) -> BTreeMap<String, String> {
    app()
        .settings(&data(m))
        .map(|settings| settings.account_keys(app()).digests())
        .unwrap_or_default()
}

fn kept_keys(m: &Rig) -> Vec<BTreeMap<String, String>> {
    let names = crate::store::folder::list(&m.ctx)
        .ok()
        .flatten()
        .unwrap_or_default();
    names
        .iter()
        .filter_map(|name| crate::store::folder::keys(&m.ctx, name).ok()?.ok())
        .map(|kept| kept.digests())
        .collect()
}

// Claude's folder by inode, its keys, and every folder's kept keys, before a recovery.
struct Inside {
    entries: BTreeSet<(String, Option<u64>)>,
    keys: BTreeMap<String, String>,
    kept: Vec<BTreeMap<String, String>>,
}

fn inside(m: &Rig) -> Inside {
    Inside {
        entries: entries(m),
        keys: keys(m),
        kept: kept_keys(m),
    }
}

// Keys are cleared only once a folder under `parks` kept them, and put only where none were.
fn took_nothing_in(m: &Rig, at: &str, before: &Inside) {
    let now = entries(m);
    let came = now.difference(&before.entries).collect::<Vec<_>>();
    assert!(
        came.is_empty(),
        "{at}: recovery put {came:?} into Claude's folder"
    );
    let after = keys(m);
    if after == before.keys {
        return;
    }
    let kept: Vec<_> = before.kept.iter().chain(&kept_keys(m)).cloned().collect();
    if before.keys.is_empty() {
        assert!(
            kept.contains(&after),
            "{at}: keys put back that no park kept"
        );
    } else {
        assert!(after.is_empty(), "{at}: keys written over keys");
        assert!(
            kept.contains(&before.keys),
            "{at}: keys cleared that no folder kept"
        );
    }
}

// Every point a change reaches, a point reached once per item at its first and at its last:
// found by running it once.
fn reached(change: Change) -> Vec<(&'static str, usize)> {
    let m = machine(change, "trail");
    let (landed, trail) = fault::trail(|| run(&m, change));
    landed.unwrap_or_else(|e| panic!("{change:?} lands where nothing kills it: {e}"));
    let mut points: Vec<(&'static str, usize)> = Vec::new();
    for point in FOLDER_POINTS {
        match trail.iter().filter(|reached| **reached == point).count() {
            0 => {}
            1 => points.push((point, 1)),
            many => points.extend([(point, 1), (point, many)]),
        }
    }
    points
}

fn played(m: &Rig, claude: Claude) {
    let app = m.claude();
    let there = in_the_folder(m);
    match (claude, there) {
        (Claude::NeverRan, _) => {}
        (Claude::RanAndQuit, None) => {
            app.opens();
            app.quits();
        }
        (Claude::RanAndQuit, Some(who)) => {
            app.opens();
            app.signs_in(&who);
            app.reports();
            app.quits();
        }
        (Claude::Renewed, Some(who)) => {
            app.opens();
            app.renews(&who, "a-session-claude-renewed");
            app.quits();
        }
        (Claude::Renewed, None) => {}
        (Claude::Stranger, _) => {
            app.opens();
            app.signs_in(&STRANGER);
            app.reports();
            app.quits();
        }
        (Claude::StillOpen, _) => app.opens(),
        // Unlisted, nothing written.
        (Claude::Crashed, _) => app.crashes(),
    }
}

// One cell: killed at `point`, Claude doing `claude`, then recovered. The detail of a
// recovery that stopped, if it did.
fn cell(change: Change, point: &'static str, nth: usize, claude: Claude) -> Option<String> {
    let at = format!("{change:?} killed at {point} #{nth}, Claude {claude:?}");
    let m = machine(change, &format!("{point}-{nth}-{claude:?}"));
    let died = fault::killing_nth(point, nth, || run(&m, change));
    assert_eq!(died.expect_err(&at), point, "{at}");
    as_a_dead_runs(&m);
    hold(&m, &at);
    let renewed = claude == Claude::Renewed && in_the_folder(&m).is_some();
    let named = in_the_folder(&m).map(|who| if who == WORK { "work" } else { "home" });
    played(&m, claude);
    swept(&m);

    if claude == Claude::StillOpen {
        let before = snapshot(&m);
        let refused = recovery(&m).expect_err(&at);
        assert_eq!(refused.code(), "app_open", "{at}: {refused}");
        unchanged(&m, &before, &format!("{at}, while Claude was open"));
        played(&m, Claude::RanAndQuit);
    }
    swept(&m);
    let before = snapshot(&m);
    let jar = inode(data(&m).join("Cookies"));
    let was = inside(&m);
    let recovered = recovery(&m);
    took_nothing_in(&m, &at, &was);
    if claude == Claude::Stranger || renewed {
        assert!(
            matches!(recovered, Err(Error::RecoveryUndetermined { .. })),
            "{at}: recovery went on over a session it did not record: {recovered:?}"
        );
    }
    match recovered {
        Ok(()) => {
            hold(&m, &at);
            if change != Change::Start {
                rested(&m, &at);
            }
            let after = snapshot(&m);
            recovery(&m).unwrap_or_else(|e| panic!("{at}: recovered twice: {e}"));
            unchanged(&m, &after, &format!("{at}, by a second recovery"));
            if !matches!(claude, Claude::Renewed | Claude::Stranger) {
                run(&m, change).unwrap_or_else(|e| panic!("{at}: the change again: {e}"));
                hold(&m, &at);
                rested(&m, &at);
            }
            None
        }
        Err(Error::RecoveryUndetermined { detail, .. }) => {
            unchanged(&m, &before, &format!("{at}, by a recovery that stopped"));
            // Claude signed in: `enroll` under the person's word is the way out.
            let signed_in = match claude {
                Claude::Renewed if renewed => named,
                Claude::Stranger => {
                    if mixed(&m) {
                        let refused = abandon(&m.ctx, Permit::for_a_test()).expect_err(&at);
                        assert_eq!(refused.code(), "move_mixed", "{at}: {refused}");
                        unchanged(&m, &before, &format!("{at}, by a give-up refused"));
                    }
                    Some("third")
                }
                _ => None,
            };
            let way_out = match signed_in {
                Some(label) => enroll(settled(&m), &m.key(label), None).map(drop),
                None => abandon(&m.ctx, Permit::for_a_test()).map(drop),
            };
            way_out.unwrap_or_else(|e| panic!("{at}: the way out of `{detail}`: {e}"));
            assert_eq!(
                inode(data(&m).join("Cookies")),
                jar,
                "{at}: the jar in Claude's folder is never moved by a give-up"
            );
            if let Some(label) = signed_in {
                let state = crate::state::load(&m.ctx).expect("state");
                let session = app()
                    .session(&m.ctx, &data(&m).join("Cookies"))
                    .expect("the jar reads")
                    .expect("a session");
                assert_eq!(
                    state
                        .account_in_use(ProviderId::Desktop)
                        .map(|a| a.label.as_str()),
                    Some(label),
                    "{at}"
                );
                assert_eq!(
                    state.in_use(ProviderId::Desktop).map(|r| r.login.as_str()),
                    Some(session.login.as_str()),
                    "{at}: recorded with the session Claude's folder holds"
                );
            }
            hold(&m, &at);
            rested(&m, &at);
            Some(detail)
        }
        Err(other) => panic!("{at}: recovery answered {}: {other}", other.code()),
    }
}

// Where a give-up without the person's word refuses: the park holds the record's jar, and an
// item of the account's other than the jar's is in Claude's folder and not in the park.
fn mixed(m: &Rig) -> bool {
    let state = crate::state::load(&m.ctx).expect("state");
    let Some(moving) = state.moving(ProviderId::Desktop) else {
        return false;
    };
    let parked = |item: &str| crate::store::folder::item(&m.ctx, &moving.park, item);
    let mine = app()
        .session(&m.ctx, &parked("Cookies"))
        .ok()
        .flatten()
        .is_some_and(|session| session.login == moving.login);
    mine && moving
        .items
        .iter()
        .filter(|item| !item.starts_with("Cookies"))
        .any(|item| data(m).join(item).exists() && !parked(item).exists())
}

// Where the jar in Claude's folder holds an account's session, so Claude can replace it.
fn jar_in_the_folder(point: &str, nth: usize) -> bool {
    matches!(
        (point, nth),
        ("leave.recorded" | "leave.keys_kept" | "leave.keys_out", _)
            | ("arrive.jar_in" | "arrive.keys_in" | "arrive.verified", _)
            | ("settings.temp_written", 1)
    )
}

fn column(change: Change, claude: Claude) -> Vec<(String, String)> {
    reached(change)
        .into_iter()
        .filter(|(point, _)| !point.ends_with(".done") && *point != "start.saved")
        .filter(|(point, nth)| claude != Claude::Renewed || jar_in_the_folder(point, *nth))
        .filter_map(|(point, nth)| {
            cell(change, point, nth, claude)
                .map(|detail| (format!("{change:?} {point} #{nth}"), detail))
        })
        .collect()
}

#[test]
fn every_point_recovers_to_a_resting_state() {
    for change in CHANGES {
        let stopped = column(change, Claude::NeverRan);
        assert!(stopped.is_empty(), "{change:?}: {stopped:?}");
    }
    let stopped = column(Change::Switch, Claude::Crashed);
    assert!(stopped.is_empty(), "crashed: {stopped:?}");
}

#[test]
fn a_point_killed_after_its_save_left_nothing_to_recover() {
    for (change, point) in [
        (Change::Switch, "leave.done"),
        (Change::Switch, "arrive.done"),
        (Change::Park, "leave.done"),
        (Change::Start, "start.saved"),
        (Change::ArriveOver, "arrive.done"),
    ] {
        let m = machine(change, point);
        let died = fault::killing(point, || run(&m, change));
        assert_eq!(died.expect_err("killed"), point);
        assert!(crate::state::load(&m.ctx).expect("state").moving.is_empty());
        hold(&m, point);
        rested(&m, point);
    }
}

// Claude opened and quit as whoever its folder held: every cell is a row's, and none stops.
#[test]
fn every_cell_takes_a_named_row() {
    for change in CHANGES {
        let stopped = column(change, Claude::RanAndQuit);
        assert!(stopped.is_empty(), "{change:?}: {stopped:#?}");
    }
}

#[test]
fn claude_still_open_refuses_and_keeps_the_record() {
    let stopped = column(Change::Switch, Claude::StillOpen);
    assert!(stopped.is_empty(), "{stopped:#?}");
}

// Claude replaced the session, or somebody signed in to a third account: recovery changes
// nothing, says which row stopped it, and `enroll` is the way out.
#[test]
fn recovery_stops_at_a_session_it_did_not_record() {
    for change in [Change::Switch, Change::ArriveOver] {
        for claude in [Claude::Renewed, Claude::Stranger] {
            let stopped = column(change, claude);
            assert!(!stopped.is_empty(), "{change:?}, {claude:?}");
            assert!(
                stopped
                    .iter()
                    .all(|(_, detail)| detail.contains("a session that is not the one moving")),
                "{change:?}, {claude:?}: {stopped:#?}"
            );
        }
    }
}

// What Claude wrote in its settings between a kill and the recovery, without running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Since {
    Nothing,
    // its own tokens, under the name the settings held
    Renewed,
    // `oauth:tokenCache` as a Log out leaves it
    Emptied,
}

fn set(m: &Rig, key: &str, value: &str) {
    let path = data(m).join("config.json");
    let mut settings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("settings")).expect("JSON");
    settings.insert(key.into(), serde_json::from_str(value).expect("JSON"));
    let written = serde_json::to_string_pretty(&settings).expect("JSON");
    std::fs::write(&path, written).expect("set");
}

// A change killed at `point` and not yet recovered, Claude never run since.
fn killed(change: Change, point: &'static str, since: Since, name: &str) -> Rig {
    let m = machine(change, &format!("{point}-{since:?}-{name}"));
    let died = fault::killing(point, || run(&m, change));
    assert_eq!(died.expect_err("killed"), point);
    as_a_dead_runs(&m);
    match since {
        Since::Nothing => {}
        Since::Renewed => {
            let path = data(&m).join("config.json");
            let written = std::fs::read_to_string(&path).expect("settings");
            let renewed = [WORK, HOME].iter().fold(written.clone(), |held, who| {
                let session = format!("|{}\"", who.session);
                held.replacen(&session, &format!("|{}-renewed\"", who.session), 1)
            });
            assert_ne!(renewed, written, "tokens to renew");
            std::fs::write(&path, renewed).expect("Claude's own write");
        }
        Since::Emptied => set(&m, "oauth:tokenCache", EMPTIED),
    }
    m
}

// One cell for each row of recovery a killed change leaves, by what it settles from.
const ROWS: [(Change, &str, Since); 10] = [
    (Change::Switch, "leave.keys_kept", Since::Nothing),
    (Change::Switch, "leave.keys_out", Since::Nothing),
    (Change::Switch, "leave.keys_kept", Since::Renewed),
    (Change::Switch, "leave.jar_out", Since::Nothing),
    (Change::Switch, "arrive.item_in", Since::Nothing),
    (Change::Switch, "arrive.jar_in", Since::Nothing),
    (Change::Switch, "arrive.jar_in", Since::Emptied),
    (Change::Switch, "arrive.keys_in", Since::Nothing),
    (Change::Switch, "arrive.keys_in", Since::Renewed),
    (Change::ArriveOver, "arrive.item_aside", Since::Nothing),
];

fn reached_by_recovery(change: Change, point: &'static str, since: Since) -> Vec<&'static str> {
    let m = killed(change, point, since, "trail");
    let (recovered, mut trail) = fault::trail(|| recovery(&m));
    recovered.unwrap_or_else(|e| panic!("{change:?} at {point}: {e}"));
    trail.dedup();
    trail
}

#[test]
fn recovery_puts_nothing_into_claudes_folder() {
    for (change, point, since) in ROWS {
        for claude in [Claude::NeverRan, Claude::RanAndQuit, Claude::Stranger] {
            let at = format!("{change:?} killed at {point}, {since:?}, Claude {claude:?}");
            let m = killed(change, point, since, &format!("{claude:?}-nothing-in"));
            played(&m, claude);
            let was = inside(&m);
            let _ = recovery(&m);
            took_nothing_in(&m, &at, &was);
        }
    }
}

#[test]
fn recovery_never_writes_keys_over_keys() {
    let cache = "oauth:tokenCache";
    // Claude renewed its tokens since a leave was recorded: they stay as Claude wrote them.
    let m = killed(
        Change::Switch,
        "leave.recorded",
        Since::Renewed,
        "renewed-tokens",
    );
    let held = keys(&m);
    recovery(&m).expect("undone");
    assert_eq!(keys(&m), held, "the keys Claude wrote are untouched");
    rested(&m, "renewed tokens");

    // Keys taken out by a leave whose jar never left come back from the park, into none.
    let m = killed(
        Change::Switch,
        "leave.keys_out",
        Since::Nothing,
        "keys-back",
    );
    assert!(keys(&m).is_empty());
    recovery(&m).expect("undone");
    assert!(keys(&m).contains_key(cache));
    rested(&m, "keys back");

    // Over keys Claude left, recovery writes none: they are kept aside, then cleared.
    for (change, point, since) in ROWS {
        let at = format!("{change:?} killed at {point}, {since:?}");
        let m = killed(change, point, since, "keys-over-keys");
        played(&m, Claude::RanAndQuit);
        let was = inside(&m);
        let _ = recovery(&m);
        took_nothing_in(&m, &at, &was);
    }
}

#[test]
fn recovery_killed_partway_recovers() {
    for (change, point, since) in ROWS {
        for at in reached_by_recovery(change, point, since) {
            let cell = format!("{change:?} killed at {point}, {since:?}, its recovery at {at}");
            let m = killed(change, point, since, at);
            let died = fault::killing(at, || recovery(&m));
            assert_eq!(died.expect_err(&cell), at, "{cell}");
            as_a_dead_runs(&m);
            hold(&m, &cell);
            recovery(&m).unwrap_or_else(|e| panic!("{cell}: recovered again: {e}"));
            hold(&m, &cell);
            rested(&m, &cell);
            let after = snapshot(&m);
            recovery(&m).unwrap_or_else(|e| panic!("{cell}: and once more: {e}"));
            unchanged(&m, &after, &format!("{cell}, by a third recovery"));
        }
    }
}

#[test]
fn claude_opened_during_recovery_keeps_the_record() {
    for (change, point, since) in ROWS {
        let mut seen = BTreeSet::new();
        // Every point of the row before its save: the save's own point comes after the look.
        let moved: Vec<&str> = reached_by_recovery(change, point, since)
            .into_iter()
            .filter(|at| FOLDER_POINTS.contains(at))
            .filter(|at| !at.ends_with(".done") && *at != "settle.saved")
            .filter(|at| seen.insert(*at))
            .collect();
        // A row that only records reaches no point before its save.
        for at in moved {
            let cell = format!("{change:?} killed at {point}, {since:?}, Claude opened at {at}");
            let m = killed(change, point, since, &format!("meanwhile-{at}"));
            let app = m.claude();
            let midway = fault::meanwhile(at, move || app.opens(), || recovery(&m));
            let midway = midway.expect_err(&cell);
            assert_eq!(midway.code(), "app_opened_midway", "{cell}: {midway}");
            assert!(
                crate::state::load(&m.ctx)
                    .expect("state")
                    .moving(ProviderId::Desktop)
                    .is_some(),
                "{cell}: the record is kept"
            );
            hold(&m, &cell);
            played(&m, Claude::RanAndQuit);
            recovery(&m).unwrap_or_else(|e| panic!("{cell}: once Claude has quit: {e}"));
            hold(&m, &cell);
            rested(&m, &cell);
        }
    }

    // A give-up setting aside what a move left beside another login, which only `enroll` runs.
    let m = killed(
        Change::Switch,
        "leave.item_out",
        Since::Nothing,
        "give-up-meanwhile",
    );
    played(&m, Claude::Stranger);
    let third = |m: &Rig| enroll(settled(m), &m.key("third"), None);
    let app = m.claude();
    let midway = fault::meanwhile("give_up.item_aside", move || app.opens(), || third(&m));
    let midway = midway.expect_err("Claude opened while a give-up set things aside");
    assert_eq!(midway.code(), "app_opened_midway", "{midway}");
    let state = crate::state::load(&m.ctx).expect("state");
    let moving = state
        .moving(ProviderId::Desktop)
        .expect("the record is kept");
    assert_eq!(moving.asides.len(), 1, "with the aside it named");
    hold(&m, "a give-up seen midway");
    m.claude().quits();
    third(&m).unwrap_or_else(|e| panic!("enrolled once Claude has quit: {e}"));
    hold(&m, "a give-up seen midway, then enrolled");
    rested(&m, "a give-up seen midway, then enrolled");

    // The start of a sign-in from a Claude that logged out: no record but `pending`'s line.
    let m = machine(Change::Start, "start-meanwhile");
    let app = m.claude();
    let midway = fault::meanwhile(
        "start.keys_cleared",
        move || app.opens(),
        || run(&m, Change::Start),
    );
    let midway = midway.expect_err("Claude opened while the start set keys aside");
    assert_eq!(midway.code(), "app_opened_midway", "{midway}");
    assert!(
        midway.to_string().contains("set aside the account name"),
        "{midway}"
    );
    let state = crate::state::load(&m.ctx).expect("state");
    assert!(state.aside.is_empty(), "the index names no aside");
    let pending =
        std::fs::read_to_string(crate::home::dir(&m.ctx).join("pending")).expect("pending");
    assert!(
        pending.lines().any(|line| line.starts_with("folder ")),
        "{pending}"
    );
    m.claude().quits();
    recover(&m).expect("the half-made aside set aside");
    run(&m, Change::Start).unwrap_or_else(|e| panic!("the start again: {e}"));
    hold(&m, "a start seen midway, then started again");
}

// Claude opened at `point`: refused by a look, or let be after the commit, then recovered.
fn opened_at(change: Change, point: &'static str) {
    let at = format!("{change:?}, Claude opened at {point}");
    let m = machine(change, &format!("opened-{point}"));
    let app = m.claude();
    // Reached after the save that commits the change, or its leave.
    let committed = point.ends_with(".done") || point == "start.saved";
    let (ran, trail) =
        fault::trail(|| fault::meanwhile(point, move || app.opens(), || run(&m, change)));
    let opened = trail.iter().position(|reached| *reached == point);
    let settled: Vec<_> = trail[opened.expect("reached") + 1..]
        .iter()
        .filter(|reached| reached.starts_with("settle.") || reached.ends_with(".done"))
        .collect();
    assert!(
        settled.is_empty(),
        "{at}: a move settled with Claude open: {settled:?}"
    );
    match ran {
        Ok(()) => assert!(committed, "{at}: went on with Claude open"),
        // A rename refused over a store Claude made meanwhile is a failure seen with Claude open.
        Err(midway) if !committed && midway.code() == "app_opened_midway" => {
            let state = crate::state::load(&m.ctx).expect("state");
            assert!(
                change == Change::Start || state.moving(ProviderId::Desktop).is_some(),
                "{at}: the record is kept"
            );
        }
        // The first look of the arrive after a switch's leave was committed: work stays parked.
        Err(left)
            if change == Change::Switch
                && point == "leave.done"
                && left.code() == "switch_left_signed_out" =>
        {
            assert!(
                left.to_string().contains("(Claude was opened)"),
                "{at}: {left}"
            );
            let state = crate::state::load(&m.ctx).expect("state");
            assert!(state.moving.is_empty(), "{at}: no arrive was recorded");
            assert!(
                state
                    .get(&m.key("work"))
                    .and_then(|a| a.parked.as_ref())
                    .is_some(),
                "{at}: work holds its park"
            );
        }
        Err(other) => panic!("{at}: {}: {other}", other.code()),
    }
    hold(&m, &at);
    played(&m, Claude::RanAndQuit);
    let was = inside(&m);
    recovery(&m).unwrap_or_else(|e| panic!("{at}: once Claude has quit: {e}"));
    took_nothing_in(&m, &at, &was);
    hold(&m, &at);
    if change != Change::Start {
        rested(&m, &at);
    }
    run(&m, change).unwrap_or_else(|e| panic!("{at}: the change again: {e}"));
    hold(&m, &at);
}

#[test]
fn claude_opened_at_any_point_stops_at_the_third_look() {
    for change in CHANGES {
        for (point, nth) in reached(change) {
            if nth == 1 {
                opened_at(change, point);
            }
        }
    }
}

// A stranger signed in where the park holds the record's jar and items of it are in Claude's
// folder: a give-up killed partway, then `enroll` run again, which names a second aside.
#[test]
fn a_give_up_killed_partway_is_taken_up_by_enroll_again() {
    let last = reached(Change::Switch)
        .into_iter()
        .filter(|(point, _)| *point == "arrive.item_in")
        .map(|(_, nth)| nth)
        .max()
        .expect("items arrive");
    let third = |m: &Rig| enroll(settled(m), &m.key("third"), None);
    for (point, nth) in [("leave.item_out", 1), ("arrive.item_in", last)] {
        for (given, given_nth) in [
            ("give_up.aside_named", 1),
            ("give_up.item_aside", 1),
            ("give_up.done", 1),
        ] {
            let at = format!("killed at {point} #{nth}, its give-up at {given}");
            let m = machine(Change::Switch, &format!("given-{point}-{given}"));
            let died = fault::killing_nth(point, nth, || run(&m, Change::Switch));
            assert_eq!(died.expect_err(&at), point);
            as_a_dead_runs(&m);
            played(&m, Claude::Stranger);
            let jar = inode(data(&m).join("Cookies"));
            let died = fault::killing_nth(given, given_nth, || third(&m));
            assert_eq!(died.expect_err(&at), given, "{at}");
            as_a_dead_runs(&m);
            hold(&m, &at);

            let state = crate::state::load(&m.ctx).expect("state");
            let first = state.moving(ProviderId::Desktop).map(|moving| {
                let name = moving.asides.keys().next().cloned().expect("named");
                let folder = crate::store::folder::at(&m.ctx, &name);
                (name, tree(&folder))
            });
            assert_eq!(
                first.is_none(),
                given == "give_up.done",
                "{at}: the record goes only with the give-up's save"
            );
            let asides = std::rc::Rc::new(std::cell::RefCell::new(None));
            let seen = {
                let (ctx, asides) = (m.ctx.clone(), std::rc::Rc::clone(&asides));
                move || {
                    let state = crate::state::load(&ctx).expect("state");
                    *asides.borrow_mut() = state
                        .moving(ProviderId::Desktop)
                        .map(|moving| moving.asides.clone());
                }
            };
            fault::meanwhile("give_up.aside_named", seen, || third(&m))
                .unwrap_or_else(|e| panic!("{at}: enrolled again: {e}"));
            if let Some((name, files)) = &first {
                let named = asides.borrow().clone().expect("a second give-up");
                assert_eq!(named.len(), 2, "{at}: {named:?}");
                assert!(named.contains_key(name), "{at}: the first aside is kept");
                let folder = crate::store::folder::at(&m.ctx, name);
                assert_eq!(&tree(&folder), files, "{at}: the first aside as it was");
            }
            let state = crate::state::load(&m.ctx).expect("state");
            assert_eq!(
                state
                    .account_in_use(ProviderId::Desktop)
                    .map(|a| a.label.clone()),
                Some("third".to_string()),
                "{at}"
            );
            assert_eq!(
                inode(data(&m).join("Cookies")),
                jar,
                "{at}: Claude's jar never moves"
            );
            hold(&m, &at);
            rested(&m, &at);
        }
    }
}

// A Log out, then a third account signed in before Claude reported it: the arriving account's
// name stays beside another session, which nothing files under that name (row 12).
#[test]
fn a_session_signed_in_after_a_log_out_is_never_filed_under_the_arriving_name() {
    let m = killed(
        Change::Switch,
        "arrive.keys_in",
        Since::Nothing,
        "logged-out-third",
    );
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.signs_in(&STRANGER);
    claude.quits_before_reporting();
    // `quits` writes the signed-in session's tokens; a Log out leaves both caches emptied (E1b).
    for cache in ["oauth:tokenCache", "oauth:tokenCacheV2"] {
        set(&m, cache, EMPTIED);
    }
    swept(&m);
    let before = snapshot(&m);
    let stopped = recovery(&m).expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined", "{stopped}");
    let refused = enroll(settled(&m), &m.key("home"), None).expect_err("refused");
    assert_eq!(refused.code(), "sign_in_unfinished", "{refused}");
    unchanged(&m, &before, "the stranger's session under `home`'s name");
    assert!(
        crate::state::load(&m.ctx)
            .expect("state")
            .moving(ProviderId::Desktop)
            .is_some(),
        "the record is kept"
    );
}

// The second look of a leave, after its folder is reserved and before its record.
#[test]
fn claude_opened_before_a_leave_is_recorded_stops_it() {
    let m = desktop_pair("second-look");
    recover(&m).expect("swept");
    let (_, trail) = fault::trail(|| {
        let m = machine(Change::Switch, "second-look-trail");
        run(&m, Change::Switch)
    });
    let first = trail.iter().position(|at| *at == "atomic.temp_written");
    let recorded = trail.iter().position(|at| *at == "leave.recorded");
    assert!(
        first.is_some() && first < recorded,
        "the folder is reserved before the record: {trail:?}"
    );
    let folder = data(&m);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(None));
    let opened = {
        let (app, folder, seen) = (m.claude(), folder.clone(), std::rc::Rc::clone(&seen));
        move || {
            app.opens();
            *seen.borrow_mut() = Some((tree(&folder), stats(&folder)));
        }
    };
    let refused = fault::meanwhile("atomic.temp_written", opened, || run(&m, Change::Switch))
        .expect_err("refused");
    assert_eq!(refused.code(), "app_open", "{refused}");
    let state = crate::state::load(&m.ctx).expect("state");
    assert!(state.moving.is_empty(), "no record");
    let seen = seen.borrow_mut().take().expect("opened");
    assert_eq!(
        (tree(&folder), stats(&folder)),
        seen,
        "nothing of Claude's folder moved"
    );
    m.claude().crashes();
    recover(&m).expect("swept");
    rested(&m, "a leave stopped at its second look");
}

// A give-up killed while it set aside what a leave left beside a stranger's session, then the
// stranger logged out: the leave is finished, and the aside the give-up named is indexed.
#[test]
fn a_leave_finished_after_a_give_up_killed_partway_indexes_its_aside() {
    let m = machine(Change::Switch, "leave-aside");
    let died = fault::killing_nth("leave.item_out", 1, || run(&m, Change::Switch));
    assert_eq!(died.expect_err("killed"), "leave.item_out");
    as_a_dead_runs(&m);
    played(&m, Claude::Stranger);
    let died = fault::killing_nth("give_up.item_aside", 1, || {
        enroll(settled(&m), &m.key("third"), None)
    });
    assert_eq!(died.expect_err("killed"), "give_up.item_aside");
    as_a_dead_runs(&m);
    let state = crate::state::load(&m.ctx).expect("state");
    let moving = state.moving(ProviderId::Desktop).expect("recorded");
    assert_eq!(moving.half, crate::state::Half::Leave);
    let aside = moving
        .asides
        .keys()
        .next()
        .cloned()
        .expect("an aside named");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();

    let mut settled = settled(&m);
    let begun =
        super::begin(&settled.ctx, settled.permit, &mut settled.state, app()).expect("settled");
    let said: Vec<&str> = begun.warnings.iter().map(Warning::code).collect();
    assert_eq!(said.first(), Some(&"interrupted_move_finished"), "{said:?}");
    assert!(said.contains(&"set_aside"), "{said:?}");
    drop((begun, settled));
    let state = crate::state::load(&m.ctx).expect("state");
    assert!(state.moving.is_empty());
    assert!(
        state
            .aside
            .iter()
            .any(|kept| kept.name == aside && kept.why == crate::state::SetAside::GivenUp),
        "{:?}",
        state.aside
    );
    hold(&m, "a leave finished after its give-up was killed");
    rested(&m, "a leave finished after its give-up was killed");
}
