// Each test's own Claude: a SQLite jar, and the stand-in under its own `Claude.app`.

mod common;

use common::{Env, HOME, TRAVEL, WORK, os};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn json_of(env: &Env, args: &[&str]) -> (Value, i32) {
    let mut with_json = args.to_vec();
    with_json.push("--json");
    let (out, err, code) = env.run(&with_json);
    let value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}{err}"));
    (value, code)
}

fn data(env: &Env) -> PathBuf {
    env.claude_desktop().join("data")
}

fn settings(env: &Env) -> Value {
    let raw = std::fs::read_to_string(data(env).join("config.json")).expect("config.json");
    serde_json::from_str(&raw).expect("JSON")
}

fn parks(env: &Env) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(env.root.join("pitboard/parks"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

type Stat = (PathBuf, Option<u64>, u64, Option<SystemTime>);

// Every entry under `root`, links not followed: inode, size and when it last changed.
fn stats(root: &Path) -> Vec<Stat> {
    fn walk(root: &Path, dir: &Path, found: &mut Vec<Stat>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let within = path.strip_prefix(root).expect("under it").to_path_buf();
            found.push((
                within,
                pitboard_core::testing::fs::inode(&path).ok(),
                meta.len(),
                meta.modified().ok(),
            ));
            if meta.is_dir() {
                walk(root, &path, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort();
    found
}

#[derive(Debug, PartialEq)]
struct Kept {
    folder: Vec<Stat>,
    files: Vec<Option<Vec<u8>>>,
    parks: Vec<String>,
}

// What a refusal leaves as it was: Claude's folder, Pitboard's index and `parks`.
fn kept(env: &Env) -> Kept {
    let read = |path: PathBuf| std::fs::read(path).ok();
    Kept {
        folder: stats(&data(env)),
        files: vec![
            read(env.root.join("pitboard/state.json")),
            read(env.root.join("pitboard/pending")),
            read(data(env).join("config.json")),
            read(data(env).join("config.json.journal")),
        ],
        parks: parks(env),
    }
}

#[test]
fn enrol_switch_both_ways_and_forget() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = Env::new("desktop-round-trip");
    env.install_fake_codex("0.154.0");
    env.plant_desktop(&WORK);
    let (out, err, code) = env.run(&["enroll", "desktop/work"]);
    assert_eq!(code, 0, "enroll desktop/work: {err}");
    assert_eq!(
        out,
        "Enrolled desktop/work, the account Claude is signed in to now. Check that Claude shows \
         that account: Pitboard cannot ask claude.ai whose it is.\n\
         Add another: pitboard enroll desktop/<label> --sign-in\n"
    );
    let (out, err, code) =
        env.sign_in_inside_claude(&["enroll", "desktop/home", "--sign-in"], &HOME);
    assert_eq!(code, 0, "enroll desktop/home --sign-in: {err}");
    assert_eq!(
        out,
        "Enrolled desktop/home, the account Claude is signed in to now.\n"
    );
    assert_eq!(env.in_use("desktop").as_deref(), Some("home"));

    let (out, err, code) = env.run(&["use", "desktop/work"]);
    assert_eq!(code, 0, "use desktop/work: {err}");
    assert_eq!(
        out,
        "Switched to desktop/work; desktop/home is parked.\nClaude opens signed in to it.\n"
    );
    let storage = data(&env).join("Local Storage/leveldb/000003.log");
    assert_eq!(
        std::fs::read_to_string(&storage).expect("work's storage"),
        WORK.session,
        "the account's own storage came back with it"
    );
    let config = settings(&env);
    assert_eq!(config["lastKnownAccountUuid"], WORK.uuid);
    assert_eq!(
        (&config["locale"], &config["userThemeMode"]),
        (&json!("en-US"), &json!("dark")),
        "a setting of Claude's own stays as it was"
    );

    let (_, err, code) = env.run(&["use", "desktop/home"]);
    assert_eq!(code, 0, "use desktop/home: {err}");
    assert_eq!(settings(&env)["lastKnownAccountUuid"], HOME.uuid);
    assert_eq!(
        std::fs::read_to_string(&storage).expect("home's storage"),
        "made by Claude"
    );
    let (_, err, code) = env.run(&["use", "desktop/work"]);
    assert_eq!(code, 0, "use desktop/work: {err}");
    assert_eq!(env.in_use("desktop").as_deref(), Some("work"));

    let (text, err, code) = env.run(&["status"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        text.matches("Pitboard reads no usage for Claude Desktop")
            .count(),
        1,
        "{text}"
    );
    let row = |label: &str| {
        text.lines()
            .find(|l| l.split_whitespace().nth(1) == Some(label))
            .unwrap_or_else(|| panic!("{label}: {text}"))
            .to_string()
    };
    assert!(row("work").contains("signed in · read <1m ago"), "{text}");
    // The planted session cookie expired in November 2025, so a run on the real clock reads it
    // as lapsed.
    assert!(
        row("home")
            .ends_with("ready · sign-in may have lapsed · pitboard enroll desktop/home --sign-in"),
        "{text}"
    );
    let (value, code) = json_of(&env, &["status"]);
    assert_eq!(code, 0, "{value}");
    let account = |label: &str| {
        value["data"]["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["qualified"] == format!("desktop/{label}"))
            .unwrap_or_else(|| panic!("{label}: {value}"))
            .clone()
    };
    assert!(account("work")["read_at"].is_i64(), "{value}");
    assert!(
        account("home")["parked"]["session_expires_at"].is_i64(),
        "{value}"
    );
    assert_eq!(value["data"]["awaiting"], json!([]));

    let (text, _, _) = env.run(&["doctor"]);
    let lines: Vec<&str> = text.lines().collect();
    let heading = lines
        .iter()
        .position(|l| *l == "Claude Desktop")
        .expect(&text);
    let at = lines
        .iter()
        .position(|l| l.contains("account desktop/home"))
        .expect(&text);
    assert!(at > heading, "listed under Claude Desktop: {text}");
    assert!(
        lines[at].contains("parked, sign-in may have lapsed"),
        "{text}"
    );

    let home_parks = |env: &Env| {
        parks(env)
            .into_iter()
            .filter(|name| name.contains(HOME.uuid))
            .count()
    };
    assert_eq!(home_parks(&env), 1, "{:?}", parks(&env));
    let (out, err, code) = env.run(&["forget", "desktop/home", "--yes"]);
    assert_eq!(code, 0, "forget desktop/home: {err}");
    assert_eq!(out, "Forgot desktop/home.\n");
    assert_eq!(
        home_parks(&env),
        0,
        "the park went with the account: {:?}",
        parks(&env)
    );
    assert_eq!(
        settings(&env)["lastKnownAccountUuid"],
        WORK.uuid,
        "the account in use is untouched"
    );
    assert!(data(&env).join("Cookies").is_file());
    let (value, code) = json_of(&env, &["forget", "desktop/work", "--yes"]);
    assert_eq!(code, 1, "the account in use is not forgotten: {value}");
    assert_eq!(value["error"]["code"], "cannot_forget_active_account");
}

#[test]
fn every_change_is_refused_while_the_stand_in_claude_runs() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = Env::new("desktop-app-open");
    env.desktop_accounts();
    // Every change first sweeps the `pending` line the last switch left, refused or not.
    let (_, err, code) = env.run(&["repair"]);
    assert_eq!(code, 0, "{err}");
    let claude = env.open_claude();
    let before = kept(&env);
    for args in [
        &["use", "desktop/home"][..],
        &["enroll", "desktop/work"],
        &["enroll", "desktop/travel"],
        &["enroll", "desktop/travel", "--sign-in"],
        &["uninstall", "--yes"],
    ] {
        let (value, code) = json_of(&env, args);
        assert_eq!(code, 1, "{args:?}: {value}");
        assert_eq!(value["error"]["code"], "app_open", "{args:?}: {value}");
        assert_eq!(kept(&env), before, "{args:?}: nothing changed");
    }
    let (_, err, code) = env.run(&["use", "desktop/home"]);
    assert_eq!(code, 1, "{err}");
    assert_eq!(
        err,
        "error: Claude is open, and Pitboard changes its login only while it is closed. Quit \
         Claude with Command-Q, then run this again. Nothing has changed.\n"
    );
    assert_eq!(kept(&env), before);
    drop(claude);
    let (_, err, code) = env.run(&["use", "desktop/home"]);
    assert_eq!(code, 0, "once Claude has quit: {err}");
}

#[test]
fn a_sign_in_parks_the_account_in_use_and_enrols_whoever_claude_quits_with() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = Env::new("desktop-sign-in");
    env.plant_desktop(&WORK);
    let (_, err, code) = env.run(&["enroll", "desktop/work"]);
    assert_eq!(code, 0, "enroll desktop/work: {err}");

    let mut adding = env.running(&["enroll", "desktop/travel", "--sign-in"]);
    let said = adding.until("Waiting for Claude to quit");
    assert_eq!(
        said,
        [
            "Parked `desktop/work`. Claude is signed out, and nothing was signed out at \
             claude.ai.",
            "Open Claude and sign in to the account to add as `desktop/travel`. When Claude \
             shows your chats, quit it with Command-Q.",
            "Do not choose Log out in Claude: it ends the login Claude holds at claude.ai, and \
             Pitboard cannot bring it back.",
            "Waiting for Claude to quit. Control-C stops waiting and keeps `desktop/work` \
             parked; `pitboard use desktop/work` puts it back.",
        ]
    );
    assert!(
        !data(&env).join("Cookies").exists(),
        "work's session is parked, and Claude is signed out"
    );
    let claude = env.open_claude();
    adding.until("Claude is open.");
    env.claude_signs_in(&TRAVEL);
    drop(claude);
    let (out, err, code) = adding.finish();
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(
        out,
        "Enrolled desktop/travel, the account Claude is signed in to now.\n"
    );
    assert_eq!(env.in_use("desktop").as_deref(), Some("travel"));
    let (value, _) = json_of(&env, &["status"]);
    let work = value["data"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["qualified"] == "desktop/work")
        .unwrap_or_else(|| panic!("{value}"))
        .clone();
    assert_eq!(work["signed_in"], false, "{work}");
    assert!(work["parked"]["session_expires_at"].is_i64(), "{work}");
    assert_eq!(value["data"]["awaiting"], json!([]), "{value}");
}

#[test]
fn control_c_leaves_the_add_halfway_and_use_puts_it_back() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = Env::new("desktop-control-c");
    env.plant_desktop(&WORK);
    let (_, err, code) = env.run(&["enroll", "desktop/work"]);
    assert_eq!(code, 0, "enroll desktop/work: {err}");
    let mut adding = env.running(&["enroll", "desktop/travel", "--sign-in"]);
    adding.until("Waiting for Claude to quit");
    // Pitboard catches no signal, so Control-C ends it as a kill does.
    drop(adding);

    let (text, err, code) = env.run(&["status"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains(
            "warning: Claude is signed out so you can sign in to another account, and \
             `desktop/work` is parked. Sign in inside Claude, quit it, then run `pitboard enroll \
             desktop/travel`. To go back, run `pitboard use desktop/work`."
        ),
        "{err}"
    );
    assert!(!text.contains('●'), "nobody is in use: {text}");
    let (value, _) = json_of(&env, &["status"]);
    let awaiting = &value["data"]["awaiting"];
    assert_eq!(awaiting.as_array().map(Vec::len), Some(1), "{value}");
    assert_eq!(
        (
            &awaiting[0]["provider"],
            &awaiting[0]["label"],
            &awaiting[0]["put_back"],
            &awaiting[0]["again"]
        ),
        (
            &json!("desktop"),
            &json!("desktop/travel"),
            &json!("desktop/work"),
            &json!(false)
        )
    );
    assert!(awaiting[0]["since"].is_i64(), "{value}");

    let (out, err, code) = env.run(&["use", "desktop/work"]);
    assert_eq!(code, 0, "use desktop/work: {err}");
    assert_eq!(
        out,
        "Switched Claude Desktop to desktop/work.\nClaude opens signed in to it.\n"
    );
    assert_eq!(settings(&env)["lastKnownAccountUuid"], WORK.uuid);
    assert_eq!(env.in_use("desktop").as_deref(), Some("work"));
    let (value, _) = json_of(&env, &["status"]);
    assert_eq!(value["data"]["awaiting"], json!([]), "{value}");
}

#[test]
fn signing_in_again_as_another_account_is_refused_and_named() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = Env::new("desktop-again");
    env.desktop_accounts();
    let (out, err, code) =
        env.sign_in_inside_claude(&["enroll", "desktop/home", "--sign-in"], &TRAVEL);
    assert_eq!(code, 1, "{out}{err}");
    assert!(
        err.contains("Open Claude and sign in to `desktop/home` again."),
        "{err}"
    );
    assert!(
        err.ends_with(
            "error: `desktop/home` already refers to another Claude Desktop account. Choose a \
             different label.\n"
        ),
        "{err}"
    );
    let (value, _) = json_of(&env, &["status"]);
    let awaiting = &value["data"]["awaiting"][0];
    assert_eq!(
        (
            &awaiting["label"],
            &awaiting["put_back"],
            &awaiting["again"]
        ),
        (&json!("desktop/home"), &json!("desktop/work"), &json!(true)),
        "the sign-in is still waited for: {value}"
    );

    let (_, err, code) = env.run(&["enroll", "desktop/travel"]);
    assert_eq!(code, 0, "enrolled under its own label: {err}");
    assert_eq!(env.in_use("desktop").as_deref(), Some("travel"));
}

#[test]
fn messages_print_no_empty_brackets_for_a_claude_desktop_account() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = Env::new("desktop-no-email");
    env.desktop_accounts();
    let mut said = String::new();
    for args in [
        &["enroll", "desktop/work"][..],
        &["rename", "desktop/home", "job"],
        &["forget", "desktop/job", "--yes"],
    ] {
        let (out, err, code) = env.run(args);
        assert_eq!(code, 0, "{args:?}: {out}{err}");
        said.push_str(&out);
        said.push_str(&err);
    }
    for line in [
        "Enrolled desktop/work, the account Claude is signed in to now.",
        "Renamed desktop/home to job.\n",
        "Forgot desktop/job.\n",
    ] {
        assert!(said.contains(line), "{line}: {said}");
    }
    assert!(!said.contains("()"), "{said}");
    assert!(!said.contains("( )"), "{said}");
}

#[test]
fn nothing_here_reads_the_real_data_folder() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let places = pitboard_core::testing::real_login_places(&common::ctx()).expect("places");
    for tail in [
        "Library/Application Support/Claude",
        "Library/Application Support/Claude-3p",
        "Applications/Claude.app",
    ] {
        let place = places
            .iter()
            .find(|place| place.ends_with(tail))
            .unwrap_or_else(|| panic!("{tail} is not among the real login places"));
        let inside = place.join("pitboard-guard");
        let caught = std::panic::catch_unwind(|| common::refuse_a_real_login_place(&inside));
        assert!(caught.is_err(), "{} must be refused", inside.display());
    }

    let env = Env::new("desktop-isolated");
    env.install_fake_codex("0.154.0");
    let (value, _) = json_of(&env, &["doctor"]);
    let desktop = &value["data"]["environment"]["desktop"];
    assert_eq!(
        desktop["data_folder"].as_str().map(PathBuf::from),
        Some(data(&env)),
        "{value}"
    );
    assert_eq!(desktop["present"], false, "{value}");
    // None of these waits: each finds nothing of Claude's in the test's own folder.
    for args in [
        &["status", "--offline"][..],
        &["doctor"],
        &["enroll", "desktop/someone"],
        &["use", "desktop/someone"],
        &["forget", "desktop/someone", "--yes"],
    ] {
        let (out, err, _) = env.run(args);
        assert!(
            !format!("{out}{err}").contains("Library/Application Support/Claude"),
            "{args:?} named the real folder: {out}{err}"
        );
    }
    assert!(!data(&env).exists(), "nothing made a data folder either");
}

// `desktop/home` signed in to again, which parks `desktop/work` and sets home's older login aside.
fn signed_in_again(name: &str) -> Env {
    let env = Env::new(name);
    env.desktop_accounts();
    let (out, err, code) =
        env.sign_in_inside_claude(&["enroll", "desktop/home", "--sign-in"], &HOME);
    assert_eq!(code, 0, "{out}{err}");
    env
}

#[test]
fn uninstall_says_how_many_folders_set_aside_it_deleted() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = signed_in_again("desktop-uninstall-aside");
    let (out, err, code) = env.run(&["uninstall", "--yes"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\nDeleted 1 folder(s) set aside.\n"), "{out}");

    let env = signed_in_again("desktop-uninstall-aside-json");
    let (value, code) = json_of(&env, &["uninstall", "--yes"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["set_aside_removed"], 1, "{value}");
}

// As a home that came from another Mac: its machine stamp, and the one in home's park.
fn from_another_mac(name: &str) -> Env {
    let env = Env::new(name);
    env.desktop_accounts();
    let state = env.state();
    let service = state["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["account_uuid"] == HOME.uuid)
        .and_then(|a| a["parked"]["service"].as_str())
        .unwrap_or_else(|| panic!("home is parked: {state}"))
        .to_string();
    let manifest = env
        .root
        .join("pitboard/parks")
        .join(service)
        .join("manifest.json");
    let mut written: Value =
        serde_json::from_slice(&std::fs::read(&manifest).expect("its manifest")).unwrap();
    written["machine"] = json!("another Mac");
    std::fs::write(&manifest, written.to_string()).unwrap();
    env.edit_state(|s| s["machine"] = json!("another Mac"));
    env
}

#[test]
fn adopt_sets_aside_a_login_parked_on_another_mac_and_says_how_to_sign_in_again() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = from_another_mac("desktop-adopt");
    let (out, err, code) = env.run(&["adopt"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains(
            "Pitboard set aside the parked Claude Desktop logins of `desktop/home`. They were \
             parked on another Mac, and Claude cannot read them here where its key differs."
        ),
        "{out}"
    );
    assert!(
        out.ends_with("Sign in to each again:\n  pitboard enroll desktop/home --sign-in\n"),
        "{out}"
    );

    let env = from_another_mac("desktop-adopt-json");
    let (value, code) = json_of(&env, &["adopt"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(
        value["data"]["set_aside"],
        json!(["desktop/home"]),
        "{value}"
    );
}

// A `state.json` that is lost: no account takes the folder home was parked in.
fn index_lost(name: &str) -> Env {
    let env = Env::new(name);
    env.desktop_accounts();
    // Every change first sweeps the `pending` line the last switch left.
    let (_, err, code) = env.run(&["repair"]);
    assert_eq!(code, 0, "{err}");
    std::fs::remove_file(env.root.join("pitboard/state.json")).expect("state.json");
    env
}

#[test]
fn repair_sets_aside_a_folder_no_account_takes_and_says_where() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = index_lost("desktop-repair-aside");
    let (out, err, code) = env.run(&["repair"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains(&format!(
            "Set aside 1 folder(s) in {} that hold what no account here can take. Nothing was \
             deleted; `pitboard doctor` lists them.\n",
            env.root.join("pitboard/parks").display()
        )),
        "{out}"
    );

    let env = index_lost("desktop-repair-aside-json");
    let (value, code) = json_of(&env, &["repair"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(
        value["data"]["set_aside"].as_array().map(Vec::len),
        Some(1),
        "{value}"
    );
}
