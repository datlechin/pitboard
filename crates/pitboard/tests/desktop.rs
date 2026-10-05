//! Claude Desktop through the binary, against a data folder made in each test's own
//! directory: a cookie jar `sqlite3` writes the way Chromium lays it out, a `config.json`
//! naming the account, and Pitboard's own parks beside it.
//!
//! Nothing here can reach the person's own Claude. `PITBOARD_CLAUDE_DESKTOP_DIR` and
//! `PITBOARD_CLAUDE_DESKTOP_APP` point into the test's directory, every write is checked
//! by `guard_not_live_dir` first, and no test opens, quits or signs out of the app.
//!
//! A switch asks first whether anything is running from the app's bundle. With both the
//! bundle and the data folder moved into the test's directory, that is the test's own
//! `Claude.app`, which nothing runs from, so the person's own Claude being open neither
//! stops a switch here nor is touched by one: every test runs, on any Mac, and none is
//! skipped. A test that needs a switch refused holds the test's own folder with a lock
//! naming the test's process, the way the app holds its own.

#![cfg(target_os = "macos")]

mod common;

use common::Env;
use common::desktop::{
    FAR_OFF, claude_is_running, hold_with_a_stand_in, home_signed_in_work_parked, inodes_under,
    sign_in_desktop, signed_in_uuid, support, write_history,
};
use serde_json::Value;

fn json_of(env: &Env, args: &[&str]) -> (Value, i32) {
    let mut with_json = args.to_vec();
    with_json.push("--json");
    let (out, err, code) = env.run(&with_json);
    let value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}{err}"));
    (value, code)
}

/// Enrol the account signed in, switch to another, and back: each session goes into its
/// park and comes out of it whole, and the folder holds whoever was asked for.
#[test]
fn enroll_use_and_switch_back_round_trip() {
    let env = Env::new("desktop-round-trip");
    let (home, work) = (env.uuid('h'), env.uuid('w'));
    sign_in_desktop(&env, 'h', &home);
    let (out, err, code) = env.run(&["enroll", "desktop/home"]);
    assert_eq!(code, 0, "enroll desktop/home: {out}{err}");
    assert!(out.contains("Enrolled home for Claude Desktop"), "{out}");

    let (value, code) = json_of(&env, &["use", "desktop", "--signed-out"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["signed_out"], true, "{value}");
    assert_eq!(signed_in_uuid(&env), None, "nobody is signed in now");

    sign_in_desktop(&env, 'w', &work);
    let (out, err, code) = env.run(&["enroll", "desktop/work"]);
    assert_eq!(code, 0, "enroll desktop/work: {out}{err}");

    let (out, err, code) = env.run(&["use", "desktop/home"]);
    assert_eq!(code, 0, "use desktop/home: {out}{err}");
    assert_eq!(signed_in_uuid(&env), Some(home.clone()));
    let (out, err, code) = env.run(&["use", "desktop/work"]);
    assert_eq!(code, 0, "use desktop/work: {out}{err}");
    assert_eq!(signed_in_uuid(&env), Some(work));
    assert!(
        std::fs::read_to_string(support(&env).join("Local Storage/leveldb/000003.log"))
            .unwrap()
            .contains("storage of w"),
        "the account's own storage came back with it"
    );
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(support(&env).join("config.json")).unwrap())
            .unwrap();
    assert_eq!(
        config["darkMode"], "system",
        "a setting of the machine's stays where it was"
    );
}

/// Signing out parks the account and says what to do next, and the account signed in after
/// it is enrolled as a second one.
#[test]
fn sign_out_then_enroll_adds_a_second_account() {
    let env = Env::new("desktop-sign-out");
    let (home, work) = (env.uuid('h'), env.uuid('w'));
    sign_in_desktop(&env, 'h', &home);
    let (_, err, code) = env.run(&["enroll", "desktop/home"]);
    assert_eq!(code, 0, "{err}");

    let (out, err, code) = env.run(&["use", "desktop", "--signed-out"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("Parked home. Claude Desktop is signed out."),
        "{out}"
    );
    assert!(out.contains("pitboard enroll desktop/<label>"), "{out}");

    let (status, _) = json_of(&env, &["status"]);
    assert_eq!(
        status["data"]["desktop"]["awaiting_sign_in"]["from"], "home",
        "{status}"
    );

    sign_in_desktop(&env, 'w', &work);
    let (_, err, code) = env.run(&["enroll", "desktop/work"]);
    assert_eq!(code, 0, "{err}");
    let (status, _) = json_of(&env, &["status"]);
    let labels: Vec<&str> = status["data"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["provider"] == "desktop")
        .filter_map(|a| a["label"].as_str())
        .collect();
    assert_eq!(labels.len(), 2, "{status}");
    assert!(
        status["data"]["desktop"]["awaiting_sign_in"].is_null(),
        "nothing is waiting once the next account is in: {status}"
    );
}

/// While Claude holds its folder Pitboard switches nothing, says which app is in the way,
/// and leaves the folder and the parks exactly as they were. The app is stood in for by its
/// lock, naming a stand-in process, so this runs whether or not Claude is open here.
#[test]
fn use_desktop_while_claude_runs_is_app_still_open() {
    let env = Env::new("desktop-app-open");
    let (home, _) = home_signed_in_work_parked(&env);
    let _held = hold_with_a_stand_in(&env);
    let parks = env.root.join("pitboard/desktop/parks");
    let before = (inodes_under(&support(&env)), inodes_under(&parks));

    for args in [
        &["use", "desktop/work"][..],
        &["use", "desktop", "--signed-out"][..],
    ] {
        let (value, code) = json_of(&env, args);
        assert_eq!(code, 3, "{args:?}: {value}");
        assert_eq!(
            value["error"]["code"], "app_still_open",
            "{args:?}: {value}"
        );
        assert_eq!(
            (inodes_under(&support(&env)), inodes_under(&parks)),
            before,
            "{args:?}: nothing moved"
        );
    }
    assert_eq!(signed_in_uuid(&env), Some(home));
    assert!(
        parks.join("pitboard-tree-work-1/Cookies").is_file(),
        "the park is where it was"
    );

    let (out, err, code) = env.run(&["use", "desktop/work"]);
    assert_eq!(code, 3, "{out}{err}");
    assert!(
        err.contains("Claude is still open, so nothing was moved."),
        "{err}"
    );
}

/// Forgetting a parked account deletes its park and nothing else: not the account signed
/// in, not its folder.
#[test]
fn forget_deletes_only_its_park() {
    let env = Env::new("desktop-forget");
    let (home, _) = home_signed_in_work_parked(&env);
    let park = env.root.join("pitboard/desktop/parks/pitboard-tree-work-1");
    assert!(park.is_dir());

    let (value, code) = json_of(&env, &["forget", "desktop/work", "--yes"]);
    assert_eq!(code, 0, "{value}");
    assert!(!park.exists(), "the park went with the account");
    assert_eq!(
        signed_in_uuid(&env),
        Some(home),
        "the live login is untouched"
    );
    assert!(support(&env).join("Cookies").is_file());

    let (value, code) = json_of(&env, &["forget", "desktop/home", "--yes"]);
    assert_ne!(code, 0, "the account signed in is not forgotten: {value}");
    assert_eq!(value["error"]["code"], "cannot_forget_active_account");
}

/// A Claude Desktop account has no email, so no message prints one: no "()" after its
/// name when it is enrolled again, renamed or forgotten. The lab run of 4 October 2026 saw
/// `Signed in to desktop/b () again.`
#[test]
fn messages_print_no_empty_brackets_for_a_desktop_account() {
    let env = Env::new("desktop-no-email");
    home_signed_in_work_parked(&env);
    let mut said = String::new();
    for args in [
        &["enroll", "desktop/home"][..],
        &["rename", "desktop/work", "job"][..],
        &["forget", "desktop/job", "--yes"][..],
    ] {
        let (out, err, code) = env.run(args);
        assert_eq!(code, 0, "{args:?}: {out}{err}");
        said.push_str(&out);
        said.push_str(&err);
    }
    assert!(said.contains("again"), "home was enrolled again: {said}");
    assert!(said.contains("job"), "{said}");
    assert!(!said.contains("()"), "{said}");
    assert!(!said.contains("( )"), "{said}");
}

/// Nothing renews a Claude Desktop sign-in, so `renew` says so for each account, with when
/// it lapses, and counts none of them as renewed or failed.
#[test]
fn renew_reports_desktop_as_not_renewable() {
    let env = Env::new("desktop-renew");
    home_signed_in_work_parked(&env);
    let (value, code) = json_of(&env, &["renew"]);
    assert_eq!(code, 0, "{value}");
    let entries = value["data"]["accounts"]
        .as_array()
        .unwrap_or_else(|| panic!("{value}"));
    let work = entries
        .iter()
        .find(|e| e["label"] == "desktop/work")
        .unwrap_or_else(|| panic!("{value}"));
    assert_eq!(work["outcome"], "not_renewable", "{work}");
    assert_eq!(work["expires_at"], FAR_OFF, "{work}");

    let (out, err, code) = env.run(&["renew"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("desktop/work: not renewable; its sign-in lapses 2031-01-01"),
        "{out}"
    );
    assert!(!out.contains("renewed"), "nothing was: {out}");
}

/// Offline, a Claude Desktop account's usage is the app's own history, which experiment
/// E10 confirmed against claude.ai, so neither the JSON nor the table calls it unconfirmed.
#[test]
fn status_offline_shows_history_marked_confirmed() {
    let env = Env::new("desktop-history");
    let (home, _) = home_signed_in_work_parked(&env);
    write_history(&env, &home);

    let (value, code) = json_of(&env, &["status", "--offline"]);
    assert_eq!(code, 0, "{value}");
    let row = value["data"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["provider"] == "desktop" && a["label"] == "home")
        .unwrap_or_else(|| panic!("{value}"))
        .clone();
    assert_eq!(row["usage"]["source"], "desktop_history", "{row}");
    assert!(
        row["usage"].get("verified").is_none(),
        "a measured reading leaves `verified` out: {row}"
    );
    assert_eq!(
        value["data"]["desktop"]["live_usage"]["enabled"], false,
        "live usage is off until somebody turns it on: {value}"
    );

    let (text, err, code) = env.run(&["status", "--offline"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        text.contains("from Claude's own history, measured"),
        "{text}"
    );
    assert!(!text.contains("unconfirmed"), "{text}");
}

/// `doctor` describes the test's Claude Desktop and its parks, under a heading of its own,
/// and carries no account's uuid or email.
#[test]
fn doctor_reports_the_desktop_environment() {
    let env = Env::new("desktop-doctor");
    let (home, work) = home_signed_in_work_parked(&env);
    let (value, _) = json_of(&env, &["doctor"]);
    let desktop = &value["data"]["environment"]["desktop"];
    assert!(desktop.is_object(), "{value}");
    let printed = value.to_string();
    for secret in [
        home.as_str(),
        work.as_str(),
        "home@example.com",
        "work@example.com",
    ] {
        assert!(!printed.contains(secret), "the report carries {secret}");
    }
    let codes: Vec<&str> = value["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["code"].as_str())
        .filter(|c| c.starts_with("desktop_"))
        .collect();
    assert!(!codes.is_empty(), "{value}");
    let parked = value["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "account desktop/work")
        .expect(&printed);
    assert_eq!(parked["level"], "ok", "its park is whole: {parked}");

    let (text, _, _) = env.run(&["doctor"]);
    let lines: Vec<&str> = text.lines().collect();
    let heading = lines
        .iter()
        .position(|l| *l == "Claude Desktop")
        .expect(&text);
    let at = lines
        .iter()
        .position(|l| l.contains("account desktop/work"))
        .expect(&text);
    assert!(at > heading, "listed under Claude Desktop: {text}");
}

/// There is no sign-in Pitboard can run for Claude Desktop, so asking for one is refused
/// before anything is touched.
#[test]
fn sign_in_flag_is_unsupported_for_desktop() {
    let env = Env::new("desktop-sign-in");
    let (home, _) = home_signed_in_work_parked(&env);
    let before = std::fs::read(env.root.join("pitboard/state.json")).unwrap();
    let (value, code) = json_of(&env, &["enroll", "desktop/other", "--sign-in"]);
    assert_ne!(code, 0, "{value}");
    assert_eq!(
        before,
        std::fs::read(env.root.join("pitboard/state.json")).unwrap(),
        "nothing was written"
    );
    assert_eq!(signed_in_uuid(&env), Some(home));

    let (value, code) = json_of(&env, &["use", "codex", "--signed-out"]);
    assert_ne!(code, 0, "only Claude Desktop is signed out: {value}");
    assert_eq!(value["error"]["code"], "usage", "{value}");
}

/// What a move or a write in the person's own Claude Desktop folder would change, read as
/// metadata only: no file in it is opened. The folder itself and every item an account's
/// login is made of, by inode, which a move changes; and, where Claude is not open to be
/// writing there itself, the names in the folder and the times each was last changed.
#[derive(Debug, PartialEq)]
struct Untouched {
    inodes: Vec<(String, Option<u64>)>,
    quiet: Option<Vec<(String, Option<std::time::SystemTime>)>>,
}

fn untouched(dir: &std::path::Path) -> Untouched {
    use std::os::unix::fs::MetadataExt;
    // The items of an account's login, as provider::desktop's ITEMS names them, less the
    // journal SQLite makes and deletes on every write while the app runs.
    const ITEMS: &[&str] = &[
        "Cookies",
        "Local Storage",
        "Session Storage",
        "IndexedDB",
        "IndexedDB/https_claude.ai_0.indexeddb.leveldb",
        "WebStorage",
        "File System",
    ];
    let meta = |relative: &str| std::fs::symlink_metadata(dir.join(relative)).ok();
    let inodes = std::iter::once("")
        .chain(ITEMS.iter().copied())
        .map(|item| (item.to_string(), meta(item).map(|m| m.ino())))
        .collect();
    let quiet = (!claude_is_running()).then(|| {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.push(String::new());
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let changed = meta(&name).and_then(|m| m.modified().ok());
                (name, changed)
            })
            .collect()
    });
    Untouched { inodes, quiet }
}

/// Every test here points Pitboard at its own Claude Desktop, so the person's own folder is
/// never read, let alone written. The harness says where; this checks the binary listens,
/// through every command that reads or changes a Claude Desktop login.
#[test]
fn nothing_here_reads_the_real_support_dir() {
    let real = common::passwd_home().join("Library/Application Support/Claude");
    let before = untouched(&real);

    let env = Env::new("desktop-isolated");
    // No Claude Desktop folder at all in the test's directory: Pitboard must see none,
    // whatever is on this Mac.
    let (value, code) = json_of(&env, &["status", "--offline"]);
    assert_eq!(code, 0, "{value}");
    let desktop_rows = value["data"]["accounts"]
        .as_array()
        .map(|rows| rows.iter().filter(|a| a["provider"] == "desktop").count())
        .unwrap_or(0);
    assert_eq!(desktop_rows, 0, "{value}");
    let (value, _) = json_of(&env, &["doctor"]);
    let printed = value.to_string();
    assert!(
        !printed.contains("Library/Application Support/Claude"),
        "doctor looked at the real folder: {printed}"
    );
    // Each of these finds nothing to act on in the test's directory. Pointed at the real
    // folder, every one of them would have something to read, and most something to move.
    // `desktop live-usage enable` is left out: it reads Claude's key, which no test may.
    for args in [
        &["status"][..],
        &["enroll", "desktop/someone"][..],
        &["use", "desktop/someone"][..],
        &["use", "desktop", "--signed-out"][..],
        &["forget", "desktop/someone", "--yes"][..],
        &["renew"][..],
        &["desktop", "live-usage", "status"][..],
        &["doctor"][..],
    ] {
        let (out, err, _) = env.run(args);
        assert!(
            !format!("{out}{err}").contains("Library/Application Support/Claude"),
            "{args:?} named the real folder: {out}{err}"
        );
    }
    assert!(!support(&env).exists(), "nothing made a folder either");
    assert_eq!(
        untouched(&real),
        before,
        "the person's own Claude Desktop folder changed"
    );
}
