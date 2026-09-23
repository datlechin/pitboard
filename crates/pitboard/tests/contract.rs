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
        // Read off whichever `codex` is on this machine's PATH, so it is this machine's.
        ".envelope.data.environment.codex.version" => "[version]",
        ".envelope.data.checks" => "[checks]",
    });
}
