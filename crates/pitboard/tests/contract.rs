//! Pins the `--json` contract v1: every command's envelope, byte for byte once the values
//! that differ between runs are redacted. A change to one of these snapshots is a change to
//! the contract, and has to be one on purpose.

mod common;

use common::{Env, two_accounts};
use serde_json::Value;

fn json(env: &Env, args: &[&str]) -> (Value, i32) {
    let mut with_json = args.to_vec();
    with_json.push("--json");
    let (out, err, code) = env.run(&with_json);
    let value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}{err}"));
    (value, code)
}

/// Every envelope, with the exit code it came with.
macro_rules! contract {
    ($name:literal, $value:expr, $code:expr) => {
        insta::assert_json_snapshot!($name, serde_json::json!({ "exit": $code, "envelope": $value }), {
            ".envelope.data.accounts[].usage.observed_at" => "[time]",
            ".envelope.data.accounts[].parked.parked_at" => "[time]",
            ".envelope.data.accounts[].parked.access_expires_at" => "[time]",
            ".envelope.data.accounts[].parked.refresh_expires_at" => "[time]",
            ".envelope.data.parked_at" => "[time]",
            // Hashed from the config directory, so it is this machine's; `slot` has its
            // own tests.
            ".envelope.data.slot.service" => "[slot]",
        });
    };
}

#[test]
fn status() {
    let env = two_accounts("contract-status");
    let (value, code) = json(&env, &["status"]);
    contract!("status", value, code);
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
    env.profile_trouble(503);
    let (value, code) = json(&env, &["use", "beta"]);
    contract!("use_anthropic_unwell", value, code);
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
    for field in ["given_back", "deleted", "strangers", "unreadable"] {
        assert!(
            value["data"][field].is_array(),
            "the envelope always carries {field}"
        );
    }
    assert!(
        value["data"]["deleted"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "this pitboard wrote nothing down that nothing recorded, so it deletes nothing"
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
}

/// The last thing a person runs, and the one whose shape matters to whatever wrapper runs
/// it: how many parked logins went, how many did not, and whether the home is gone.
/// The reading that asks nobody anything: what was last measured, and who Claude Code's
/// config says is signed in. Its envelope is a contract like any other.
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
    let (value, code) = json(&env, &["forget", "work"]);
    contract!("forget", value, code);
    let (value, code) = json(&env, &["forget", "alpha"]);
    contract!("forget_signed_in", value, code);
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
    insta::assert_json_snapshot!("doctor", serde_json::json!({ "exit": code, "envelope": value }), {
        ".envelope.data.environment.config_file" => "[path]",
        ".envelope.data.environment.storage_dir" => "[path]",
        ".envelope.data.environment.home" => "[path]",
        ".envelope.data.environment.credential_service" => "[slot]",
        ".envelope.data.environment.credential_store" => "[backend]",
        ".envelope.data.environment.codex.home" => "[path]",
        ".envelope.data.checks" => "[checks]",
    });
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
