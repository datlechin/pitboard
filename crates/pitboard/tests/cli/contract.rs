//! Pins the `--json` contract v1: every command's envelope, byte for byte once the values
//! that differ between runs are redacted. A change to one of these snapshots is a change to
//! the contract, and has to be one on purpose.

use crate::common;
use crate::common::{Env, HOME, WORK, os, two_accounts};
use serde_json::Value;

fn json(env: &Env, args: &[&str]) -> (Value, i32) {
    let (_, value, code) = json_as_printed(env, args);
    (value, code)
}

/// The envelope as printed, and as parsed. Every byte of it is ASCII, whatever it carries.
fn json_as_printed(env: &Env, args: &[&str]) -> (String, Value, i32) {
    let mut with_json = args.to_vec();
    with_json.push("--json");
    let (out, err, code) = env.run(&with_json);
    assert!(
        out.is_ascii(),
        "`pitboard {}` printed a byte outside ASCII: {out}",
        with_json.join(" ")
    );
    let value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}{err}"));
    (out, value, code)
}

// A moment where there is one: a null is part of the contract too.
fn time() -> insta::internals::Redaction {
    insta::dynamic_redaction(|value, _| {
        if value.is_nil() {
            value
        } else {
            insta::internals::Content::from("[time]")
        }
    })
}

/// Every envelope, with the exit code it came with.
macro_rules! contract {
    ($name:literal, $value:expr, $code:expr) => {
        insta::assert_json_snapshot!($name, serde_json::json!({ "exit": $code, "envelope": $value }), {
            ".envelope.data.accounts[].usage.observed_at" => time(),
            ".envelope.data.accounts[].parked.parked_at" => time(),
            ".envelope.data.accounts[].parked.access_expires_at" => time(),
            ".envelope.data.accounts[].parked.refresh_expires_at" => time(),
            ".envelope.data.accounts[].parked.session_expires_at" => time(),
            ".envelope.data.accounts[].read_at" => time(),
            ".envelope.data.awaiting[].since" => time(),
            ".envelope.data.parked_at" => time(),
            ".envelope.data.limit.resets_at" => time(),
            ".envelope.data.as_of" => time(),
            // Hashed from the config directory, so it is this machine's; `slot` has its
            // own tests.
            ".envelope.data.slot.service" => "[slot]",
            // In the test's own scratch home.
            ".envelope.data.path" => "[path]",
        });
    };
}

// `doctor`'s envelope, with this machine's paths and the checks' wording redacted.
macro_rules! doctor_contract {
    ($name:literal, $value:expr, $code:expr) => {
        insta::assert_json_snapshot!($name, serde_json::json!({ "exit": $code, "envelope": $value }), {
            ".envelope.data.environment.config_file" => "[path]",
            ".envelope.data.environment.storage_dir" => "[path]",
            ".envelope.data.environment.home" => "[path]",
            ".envelope.data.environment.credential_service" => "[slot]",
            ".envelope.data.environment.credential_store" => "[backend]",
            ".envelope.data.environment.codex.home" => "[path]",
            ".envelope.data.environment.desktop.data_folder" => "[path]",
            ".envelope.data.checks" => "[checks]",
        });
    };
}

#[test]
fn status() {
    let env = two_accounts("contract-status");
    let (value, code) = json(&env, &["status"]);
    contract!("status", value, code);
}

/// A label is the person's own word, in whatever script they write it, and the envelope
/// still prints as ASCII alone: each character outside it as a `\u` escape, and one above
/// U+FFFF as its UTF-16 surrogate pair. A program that decodes the bytes as something other
/// than UTF-8 then parses the same values. Pitboard printed them as raw UTF-8.
#[test]
fn a_label_outside_ascii_is_printed_as_escapes() {
    let mut env = Env::new("contract-ascii");
    let (a, o, b, p) = (env.uuid('a'), env.uuid('o'), env.uuid('b'), env.uuid('p'));
    env.sign_in(&a, "a@example.com", &o, "refresh-a");
    let (_, err, code) = env.run(&["enroll", "Đạt"]);
    assert_eq!(code, 0, "enroll Đạt: {err}");
    let (_, err, code) = env.enroll_by_signing_in("🏁", &b, "b@example.com", &p, "refresh-b");
    assert_eq!(code, 0, "enroll 🏁: {err}");

    let (printed, value, code) = json_as_printed(&env, &["status"]);
    for escaped in [
        r#""label":"\u0110\u1ea1t""#,
        r#""qualified":"claude/\u0110\u1ea1t""#,
        r#""label":"\ud83c\udfc1""#,
        r#""qualified":"claude/\ud83c\udfc1""#,
    ] {
        assert!(printed.contains(escaped), "{escaped} in {printed}");
    }
    let labels: Vec<&str> = value["data"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["label"].as_str())
        .collect();
    assert_eq!(labels, ["Đạt", "🏁"], "parsed back as enrolled");
    contract!("status_outside_ascii", value, code);

    let (printed, value, code) = json_as_printed(&env, &["doctor"]);
    assert_eq!(code, 0, "{value}");
    for escaped in [
        r#""name":"account \u0110\u1ea1t""#,
        r#""name":"account \ud83c\udfc1""#,
    ] {
        assert!(printed.contains(escaped), "{escaped} in {printed}");
    }
    let accounts: Vec<&Value> = value["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["name"]
                .as_str()
                .is_some_and(|n| n.starts_with("account "))
        })
        .collect();
    let names: Vec<&str> = accounts.iter().filter_map(|c| c["name"].as_str()).collect();
    assert_eq!(
        names,
        ["account Đạt", "account 🏁"],
        "parsed back as enrolled"
    );
    // Which other checks run depends on the platform, as `doctor` below says, so only the
    // accounts' are pinned. How long a parked login is good for counts down as the test runs.
    insta::assert_json_snapshot!("doctor_outside_ascii", serde_json::json!({ "exit": code, "accounts": accounts }), {
        ".accounts[1].detail" => "[parked, good for]",
    });

    // An error's message is in the envelope too, and names what was typed.
    let (printed, value, code) = json_as_printed(&env, &["use", "Đạt2"]);
    assert!(printed.contains(r"`\u0110\u1ea1t2`"), "{printed}");
    contract!("use_unknown_outside_ascii", value, code);
}

#[test]
fn use_switches() {
    let env = two_accounts("contract-use");
    let (value, code) = json(&env, &["use", "beta"]);
    contract!("use_switched", value, code);
    let (value, code) = json(&env, &["use", "beta"]);
    contract!("use_already_active", value, code);
    let (value, code) = json(&env, &["use", "nobody"]);
    contract!("use_unknown", value, code);
}

/// The cause is the field a program reads to decide whether to try again. Without it every
/// failure that was not a 401 arrived as `identity_unverifiable` and a sentence of prose.
#[test]
fn a_failure_says_what_went_wrong_underneath() {
    let mut env = two_accounts("contract-cause");
    env.identity_trouble(503);
    let (value, code) = json(&env, &["use", "beta"]);
    contract!("use_anthropic_unwell", value, code);
}

// The report of 16 October 2026: Anthropic answered 400 to the profile of a login it still
// names. A switch between two enrolled accounts needs no profile.
#[test]
fn a_switch_between_enrolled_accounts_needs_no_profile() {
    let mut env = two_accounts("contract-no-profile");
    env.profile_answers(
        400,
        r#"{"type":"error","error":{"type":"invalid_request_error","message":"This organization cannot be read."}}"#,
    );
    let (value, code) = json(&env, &["use", "beta"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["to"], "beta", "{value}");
    assert_eq!(value["data"]["changed"], true, "{value}");
}

// A login Anthropic no longer accepts cannot be parked: the switch says so and asks, and
// `--yes` puts the account's login in its place.
#[test]
fn a_switch_from_a_login_anthropic_no_longer_accepts_asks_and_replaces_it_with_yes() {
    let mut env = two_accounts("contract-dead-login");
    let mut dead = common::credential("refresh-dead");
    dead["claudeAiOauth"]["expiresAt"] = serde_json::json!(1_000);
    env.replace_live(&dead);
    let _refused = env.answers_renewal(
        "refresh-dead",
        400,
        serde_json::json!({"error": "invalid_grant", "error_description": "Refresh token not found or invalid"}),
    );

    let (value, code) = json(&env, &["use", "beta"]);
    assert_eq!(code, 1, "{value}");
    assert_eq!(value["error"]["code"], "live_login_refused", "{value}");
    assert!(
        value["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("pitboard use beta --yes")),
        "{value}"
    );

    let (value, code) = json(&env, &["use", "beta", "--yes"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["to"], "beta", "{value}");
    assert!(value["data"]["from"].is_null(), "{value}");
}

// A request Anthropic no longer takes is a change on its side, which no sign-in mends.
#[test]
fn a_request_anthropic_no_longer_takes_says_to_update_pitboard() {
    let mut env = two_accounts("contract-not-found");
    env.identity_answers(
        404,
        r#"{"type":"error","error":{"type":"not_found_error","message":"Not Found"}}"#,
    );
    let (value, code) = json(&env, &["use", "beta"]);
    assert_eq!(code, 1, "{value}");
    assert_eq!(
        value["error"]["cause"]["code"], "answer_not_understood",
        "{value}"
    );
    assert!(
        value["error"]["message"]
            .as_str()
            .is_some_and(|m| m.ends_with("Check for a Pitboard update.")),
        "{value}"
    );
}

// The reporter's dialog said only "Anthropic answered 400".
#[test]
fn a_refusal_says_why_and_is_not_worth_retrying() {
    let mut env = two_accounts("contract-refused");
    env.identity_answers(
        400,
        r#"{"type":"error","error":{"type":"invalid_request_error","message":"This organization cannot be read."}}"#,
    );
    let (value, code) = json(&env, &["use", "beta"]);
    contract!("use_anthropic_refuses", value, code);
}

/// The keychain belongs to the whole machine, so what `repair` finds depends on what else
/// is on it. The envelope's shape is the contract; the lists are not snapshotted.
#[test]
fn repair() {
    let env = two_accounts("contract-repair");
    let (value, code) = json(&env, &["repair"]);
    assert_eq!(code, 0);
    assert_eq!(value["command"], "repair");
    assert_eq!(value["ok"], true);
    for field in [
        "given_back",
        "deleted",
        "strangers",
        "unreadable",
        "set_aside",
    ] {
        assert!(
            value["data"][field].is_array(),
            "the envelope always carries {field}"
        );
    }
    assert!(
        value["data"]["deleted"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "this Pitboard wrote nothing down that nothing recorded, so it deletes nothing"
    );
}

#[test]
fn enroll() {
    let mut env = Env::new("contract-enroll");
    let (a, o, b, p) = (env.uuid('a'), env.uuid('o'), env.uuid('b'), env.uuid('p'));
    env.sign_in(&a, "a@example.com", &o, "refresh-a");
    let (value, code) = json(&env, &["enroll", "alpha"]);
    contract!("enroll_current", value, code);

    let (out, _, code) =
        env.enroll_by_signing_in_json("beta", &b, "b@example.com", &p, "refresh-b");
    contract!(
        "enroll_signed_in",
        serde_json::from_str::<Value>(&out).unwrap(),
        code
    );
    let (out, _, code) =
        env.enroll_by_signing_in_json("beta", &b, "b@example.com", &p, "refresh-b2");
    contract!(
        "enroll_renewed",
        serde_json::from_str::<Value>(&out).unwrap(),
        code
    );
    let (out, _, code) =
        env.enroll_by_signing_in_json("alpha", &a, "a@example.com", &o, "refresh-a2");
    contract!(
        "enroll_in_use",
        serde_json::from_str::<Value>(&out).unwrap(),
        code
    );
}

/// The last thing a person runs, and the one whose shape matters to whatever wrapper runs
/// it: how many parked logins went, how many did not, and whether the home is gone.
/// The reading that asks nobody anything: what was last measured, and whose login each tool
/// has stored, as its service last said. Its envelope is a contract like any other.
#[test]
fn status_offline() {
    let env = two_accounts("contract-offline");
    env.run(&["status"]); // one live read, so there is something remembered to show
    let (value, code) = json(&env, &["status", "--offline"]);
    contract!("status_offline", value, code);
}

#[test]
fn uninstall() {
    let env = two_accounts("contract-uninstall");
    let (value, code) = json(&env, &["uninstall", "--yes"]);
    contract!("uninstall", value, code);
}

#[test]
fn forget_and_rename() {
    let env = two_accounts("contract-forget");
    let (value, code) = json(&env, &["rename", "beta", "work"]);
    contract!("rename", value, code);
    let (value, code) = json(&env, &["forget", "work", "--yes"]);
    contract!("forget", value, code);
    let (value, code) = json(&env, &["forget", "alpha", "--yes"]);
    contract!("forget_signed_in", value, code);
}

/// Turning an account on, setting the span, and the overview that shows both. An account
/// with no reading from Anthropic yet waits for one; a span that starts and ends at once is
/// a usage error.
#[test]
fn warm() {
    let env = two_accounts("contract-warm");
    let (value, code) = json(&env, &["warm", "on", "beta"]);
    contract!("warm_on", value, code);
    let (value, code) = json(&env, &["warm", "range", "07:30", "23:00"]);
    contract!("warm_range", value, code);
    let (value, code) = json(&env, &["warm"]);
    contract!("warm", value, code);
    let (value, code) = json(&env, &["warm", "range", "06:00", "06:00"]);
    contract!("warm_range_empty", value, code);
}

/// With nobody to ask and no `--yes`, a command that deletes something refuses before it
/// changes anything.
#[test]
fn unconfirmed() {
    let env = two_accounts("contract-unconfirmed");
    let (value, code) = json(&env, &["forget", "beta"]);
    contract!("forget_unconfirmed", value, code);
    let (value, code) = json(&env, &["uninstall"]);
    contract!("uninstall_unconfirmed", value, code);
}

#[test]
fn statusline() {
    let env = two_accounts("contract-statusline");
    let (value, code) = json(&env, &["statusline"]);
    contract!("statusline", value, code);
}

#[test]
fn usage_error() {
    let env = Env::new("contract-usage");
    let (value, code) = json(&env, &["use"]);
    contract!("usage_error", value, code);
}

/// Which checks run depends on the platform, so the snapshot pins the envelope and every
/// check is held to the same five fields.
///
/// The promise is pinned here rather than in the snapshot, because a snapshot of the checks
/// would be a snapshot of this machine. The bug template asks people to paste this, and
/// tells them it carries labels, codes, paths and times and nothing else. It used to carry
/// their email address, their organisation uuid, their login name and a value derived from
/// their refresh token, and the snapshot could not catch it because it redacted the whole
/// array.
#[test]
fn doctor() {
    let env = two_accounts("contract-doctor");
    env.install_fake_codex("0.154.0");
    let (value, code) = json(&env, &["doctor"]);

    let printed = value.to_string();
    let alpha = env.uuid('a');
    let beta = env.uuid('b');
    for (secret, what) in [
        ("a@example.com", "an email address"),
        ("b@example.com", "another email address"),
        (alpha.as_str(), "an account uuid"),
        (beta.as_str(), "another account uuid"),
    ] {
        assert!(
            !printed.contains(secret),
            "a report meant to be pasted somewhere carries {what}: {secret}"
        );
    }
    // Paths are what the template promises and what a report is for; it is the name inside
    // a home path that goes, and `redact` proves that on its own.
    assert!(
        printed.contains("alpha"),
        "the labels are the person's own words"
    );
    assert!(printed.contains("beta"), "{printed}");

    for check in value["data"]["checks"].as_array().unwrap() {
        let mut keys: Vec<&str> = check
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["advice", "code", "detail", "level", "name"],
            "{check}"
        );
        assert!(["ok", "warn", "fail"].contains(&check["level"].as_str().unwrap()));
    }
    doctor_contract!("doctor", value, code);
}

// The test's own Claude: a SQLite jar, and the stand-in under its own `Claude.app` while it runs.
#[test]
fn desktop() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = two_accounts("contract-desktop");
    env.install_fake_codex("0.154.0");
    env.plant_desktop(&WORK);
    let (value, code) = json(&env, &["enroll", "desktop/work"]);
    contract!("enroll_desktop_current", value, code);
    let (_, err, code) = env.sign_in_inside_claude(&["enroll", "desktop/home", "--sign-in"], &HOME);
    assert_eq!(code, 0, "enroll desktop/home --sign-in: {err}");
    let (_, err, code) = env.run(&["use", "desktop/work"]);
    assert_eq!(code, 0, "use desktop/work: {err}");

    let (value, code) = json(&env, &["status"]);
    contract!("status_with_desktop", value, code);

    let (value, code) = json(&env, &["doctor"]);
    let printed = value.to_string();
    for secret in [WORK.uuid, HOME.uuid, "a@example.com"] {
        assert!(!printed.contains(secret), "the report carries {secret}");
    }
    for check in value["data"]["checks"].as_array().unwrap() {
        let mut keys: Vec<&str> = check
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["advice", "code", "detail", "level", "name"],
            "{check}"
        );
    }
    doctor_contract!("doctor_with_desktop", value, code);

    let claude = env.open_claude();
    let (value, code) = json(&env, &["use", "desktop/home"]);
    contract!("use_desktop_app_open", value, code);
    drop(claude);
    let (value, code) = json(&env, &["use", "desktop/home"]);
    contract!("use_desktop_switched", value, code);
    let (_, err, code) = env.run(&["use", "desktop/work"]);
    assert_eq!(code, 0, "use desktop/work: {err}");
    let (value, code) = json(&env, &["forget", "desktop/home", "--yes"]);
    contract!("forget_desktop", value, code);
}

#[test]
fn desktop_awaiting() {
    // windows: Claude Desktop is macOS only
    if !os::claude_desktop_runs_here() {
        return;
    }
    let env = two_accounts("contract-desktop-awaiting");
    env.desktop_accounts();
    let mut adding = env.running(&["enroll", "desktop/travel", "--sign-in"]);
    adding.until("Waiting for Claude to quit");
    // Pitboard catches no signal, so Control-C ends it as a kill does.
    drop(adding);
    let (value, code) = json(&env, &["status"]);
    contract!("status_desktop_awaiting", value, code);
    let (value, code) = json(&env, &["use", "desktop/work"]);
    contract!("use_desktop_installed", value, code);
}

// A build for tests given no Claude of its own has none, after the change's lock is taken.
#[test]
fn desktop_unsupported() {
    let env = Env::new("contract-desktop-unsupported");
    let without = |args: &[&str]| {
        let mut command = env.command(&[args, &["--json"]].concat());
        command.env_remove("PITBOARD_TEST_CLAUDE_DESKTOP");
        let out = command.output().expect("run Pitboard");
        let printed = String::from_utf8_lossy(&out.stdout).into_owned();
        let value: Value =
            serde_json::from_str(&printed).unwrap_or_else(|e| panic!("{e}: {printed}"));
        (value, out.status.code().unwrap_or(-1))
    };
    let (value, code) = without(&["enroll", "desktop/work", "--sign-in"]);
    assert_eq!(
        (value["error"]["code"].as_str(), code),
        (Some("live_store_unsupported"), 3),
        "{value}"
    );
    let (value, code) = without(&["enroll", "desktop/work"]);
    contract!("desktop_unsupported", value, code);
}

/// A machine with Claude Code and Codex both signed in. Every account says which tool it is
/// for, a Codex account's name is given the way it is typed, and Claude Code's rows are
/// what they were with two fields added.
#[test]
fn status_with_codex() {
    let mut env = two_accounts("contract-codex");
    let work = env.uuid('w');
    env.sign_in_codex(&work, "w@example.com", "codex-refresh-w");
    env.codex_usage(25.0, 60.0);
    let (_, err, code) = env.run(&["enroll", "codex/work"]);
    assert_eq!(code, 0, "enroll codex/work: {err}");

    let (value, code) = json(&env, &["status"]);
    contract!("status_with_codex", value, code);

    // And for a person: each tool under its own heading, windows named alike.
    let (text, err, code) = env.run(&["status"]);
    assert_eq!(code, 0, "{err}");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Claude Code", "{text}");
    let codex = lines.iter().position(|l| *l == "Codex").expect(&text);
    assert!(lines[codex + 1].contains("w@example.com"), "{text}");
    assert!(lines[codex + 2].trim_start().starts_with("5h"), "{text}");
    assert!(lines[codex + 3].trim_start().starts_with("week"), "{text}");
}

/// A Codex login is in the report too, and is kept out of what is pasted as carefully as
/// Claude Code's.
#[test]
fn doctor_with_codex() {
    let mut env = two_accounts("contract-doctor-codex");
    env.install_fake_codex("0.154.0");
    let work = env.uuid('w');
    env.sign_in_codex(&work, "w@example.com", "codex-refresh-w");
    env.codex_usage(25.0, 60.0);
    let (_, err, code) = env.run(&["enroll", "codex/work"]);
    assert_eq!(code, 0, "enroll codex/work: {err}");

    let (value, code) = json(&env, &["doctor"]);
    assert_eq!(code, 0, "{value}");
    let printed = value.to_string();
    for secret in ["w@example.com", work.as_str()] {
        assert!(!printed.contains(secret), "the report carries {secret}");
    }
    let codex = &value["data"]["environment"]["codex"];
    assert_eq!(codex["present"], true);
    assert_eq!(codex["backend"], "file");
    assert_eq!(codex["login_present"], true);
    let codes: Vec<&str> = value["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["code"].as_str())
        .filter(|c| c.starts_with("codex_"))
        .collect();
    for expected in [
        "codex_backend",
        "codex_auth_file",
        "codex_login",
        "codex_version",
        "codex_running",
    ] {
        assert!(codes.contains(&expected), "{expected} in {codes:?}");
    }
    let account = value["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "account codex/work")
        .expect("the Codex account, named the way it is typed");
    assert_eq!(account["level"], "ok", "it is the one signed in: {account}");
    assert_eq!(
        account["code"], "codex_parked_login",
        "everything about Codex is found by its prefix"
    );
    assert_eq!(
        codex["version"], "0.154.0",
        "the test's own codex, never this machine's"
    );

    let (text, _, _) = env.run(&["doctor"]);
    let lines: Vec<&str> = text.lines().collect();
    let heading = lines.iter().position(|l| *l == "Codex").expect(&text);
    let at = lines
        .iter()
        .position(|l| l.contains("account codex/work"))
        .expect(&text);
    assert!(at > heading, "listed under Codex: {text}");
}

/// Codex signed in with an API key, on a machine with a Codex account enrolled. Something
/// is signed in and it is no account: `status` says so rather than that nobody is, and
/// `doctor` says it is a choice rather than a broken login, so a script reading its exit
/// code is not told to stop switching accounts.
#[test]
fn codex_signed_in_with_an_api_key() {
    let mut env = two_accounts("contract-codex-api-key");
    env.install_fake_codex("0.154.0");
    let work = env.uuid('w');
    env.sign_in_codex(&work, "w@example.com", "codex-refresh-w");
    env.codex_usage(25.0, 60.0);
    let (_, err, code) = env.run(&["enroll", "codex/work"]);
    assert_eq!(code, 0, "enroll codex/work: {err}");
    env.sign_in_codex_with_an_api_key();

    let (value, code) = json(&env, &["status"]);
    assert_eq!(code, 0, "{value}");
    let accounts = value["data"]["accounts"].as_array().unwrap();
    let codex: Vec<&Value> = accounts
        .iter()
        .filter(|a| a["provider"] == "codex")
        .collect();
    assert!(
        codex.iter().all(|a| a["signed_in"] == false),
        "no Codex account is signed in: {value}"
    );
    let said = codex
        .iter()
        .find(|a| a["label"].is_null())
        .expect("a row for the login on no account");
    assert_eq!(said["stale"], "login_unusable", "{said}");
    assert!(said["qualified"].is_null(), "{said}");
    assert!(
        !value.to_string().contains("sk-not-a-real-key"),
        "the key itself is never shown"
    );

    let (value, code) = json(&env, &["doctor"]);
    assert_eq!(code, 0, "a choice is not a failure: {value}");
    let login = value["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == "codex_login")
        .expect("a codex_login check");
    assert_eq!(login["level"], "warn", "{login}");
}

/// What `watch` decides, once, from what Pitboard measured: a switch it made by itself, and
/// nothing to do.
#[test]
fn watch() {
    let env = two_accounts("contract-watch");
    env.an_hour_on();
    env.measured(&[("alpha", 96.0, 20.0), ("beta", 10.0, 30.0)]);
    let (value, code) = json(&env, &["watch", "--once"]);
    contract!("watch_switched", value, code);
    let (value, code) = json(&env, &["watch", "--once"]);
    contract!("watch_idle", value, code);
}

/// Nothing behind the store Claude Code keeps its login in is nothing to put away, and is
/// not asked about: so on Linux, where the file is that store, always.
#[test]
fn stow_with_nothing_left() {
    let env = two_accounts("contract-stow-nothing");
    let (value, code) = json(&env, &["stow"]);
    contract!("stow_nothing_left", value, code);
}

// The login Claude Code has stored, left in `.credentials.json` behind the keychain or Credential
// Manager too, goes with the file. Linux keeps no file behind a store.
#[test]
#[cfg(any(target_os = "macos", windows))]
fn stow() {
    let env = two_accounts("contract-stow");
    #[cfg(windows)]
    env.turn_credential_manager_on();
    let left = env.root.join(".credentials.json");
    os::write_private(&left, &common::credential("refresh-a").to_string());
    let (value, code) = json(&env, &["stow"]);
    assert!(left.exists(), "the file is kept");
    contract!("stow_unconfirmed", value, code);
    let (value, code) = json(&env, &["stow", "--yes"]);
    assert!(!left.exists(), "the file is gone");
    contract!("stow", value, code);
}
