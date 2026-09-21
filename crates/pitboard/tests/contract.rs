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
#[test]
fn doctor() {
    let env = two_accounts("contract-doctor");
    let (value, code) = json(&env, &["doctor"]);
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
        ".envelope.data.checks" => "[checks]",
    });
}
