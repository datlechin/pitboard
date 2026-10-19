//! `pitboard warm`, run as a person runs it, with a stand-in for Claude Code's headless mode
//! started for real: what it is started as, what it is handed, and what Pitboard records.

mod common;

use common::os::{self, Handed};
use common::two_accounts;

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// Asked to warm up a parked account now, Pitboard starts Claude Code's print mode with that
/// account's access token alone, on descriptor 3 or on Windows in `CLAUDE_CODE_OAUTH_TOKEN`, in
/// a home of its own, and records what the next reading says: its five-hour period running.
#[test]
fn warming_up_now_runs_claude_code_with_the_access_token_alone() {
    let mut env = two_accounts("warm-now");
    let (started, handed) = env.install_fake_claude_print();
    env.claude_usage(None);
    let (_, err, code) = env.run(&["status"]);
    assert_eq!(code, 0, "status: {err}");
    env.claude_usage(Some(5 * 3600));
    let bash = env.root.join("Git").join("bin").join("bash.exe");
    std::fs::create_dir_all(bash.parent().unwrap()).unwrap();
    std::fs::write(&bash, "never run").unwrap();

    let (out, err, code) = env.run_with(
        &["warm", "now", "beta", "--json"],
        &[
            ("CLAUDE_CODE_GIT_BASH_PATH", bash.to_str().unwrap()),
            ("ANTHROPIC_API_KEY", "sk-not-this"),
            ("ANTHROPIC_AUTH_TOKEN", "token-not-this"),
            ("CLAUDE_BG_AUTH_SNAPSHOT_PATH", "snapshot-not-this"),
            ("CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR", "9"),
        ],
    );
    assert_eq!(code, 0, "warm now: {out}{err}");
    let envelope = json(&out);
    assert_eq!(envelope["data"]["outcome"], "started", "{out}");
    assert_eq!(
        envelope["warnings"],
        serde_json::json!([]),
        "a key in this shell reaches no run, so nothing warns of it"
    );
    assert!(envelope["data"]["until"].as_i64().is_some(), "{out}");

    let started = json(std::fs::read_to_string(&started).unwrap().trim());
    assert_eq!(
        started["args"],
        serde_json::json!([
            "-p",
            "Reply with the single word OK.",
            "--model",
            "haiku",
            "--tools",
            "",
            "--no-session-persistence",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--output-format",
            "json"
        ])
    );
    let variables = &started["variables"];
    assert_eq!(variables["ANTHROPIC_API_KEY"], serde_json::Value::Null);
    assert_eq!(variables["PITBOARD_HOME"], serde_json::Value::Null);
    let config = variables["CLAUDE_CONFIG_DIR"]
        .as_str()
        .expect("a config dir");
    assert!(
        std::path::Path::new(config).ends_with("pitboard/warm/claude/config"),
        "{config}"
    );
    let names: Vec<&str> = started["names"]
        .as_array()
        .expect("the names it was given")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    for absent in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_BG_AUTH_SNAPSHOT_PATH",
        "PITBOARD_HOME",
    ] {
        assert!(!names.contains(&absent), "{absent}: {names:?}");
    }

    let handed = json(&std::fs::read_to_string(&handed).unwrap());
    assert_eq!(handed["input"], "", "nothing on standard input");
    match os::warm_up_token() {
        Handed::OnDescriptor => {
            assert_eq!(variables["CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR"], "3");
            assert_eq!(handed["descriptor"], "access-refresh-b");
            assert_eq!(
                variables["CLAUDE_CODE_OAUTH_TOKEN"],
                serde_json::Value::Null
            );
            assert_eq!(
                variables["CLAUDE_CODE_GIT_BASH_PATH"],
                serde_json::Value::Null
            );
        }
        Handed::InVariable => {
            assert_eq!(
                variables["CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR"],
                serde_json::Value::Null
            );
            assert_eq!(variables["CLAUDE_CODE_SUBPROCESS_ENV_SCRUB"], "1");
            assert_eq!(handed["descriptor"], serde_json::Value::Null);
            assert_eq!(variables["CLAUDE_CODE_OAUTH_TOKEN"], "access-refresh-b");
            assert_eq!(
                variables["CLAUDE_CODE_GIT_BASH_PATH"],
                bash.to_str().unwrap()
            );
            for given in ["USERPROFILE", "SystemRoot"] {
                assert!(names.contains(&given), "{given}: {names:?}");
            }
        }
    }

    let (log, _, _) = env.run(&["log", "--json"]);
    let entries = json(&log);
    assert!(
        entries["data"]["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .any(|e| e["verb"] == "warm-up" && e["subject"] == "beta" && e["outcome"] == "ok"),
        "{log}"
    );
}

/// An account whose period runs already is not warmed up again; it says until when.
#[test]
fn warming_up_now_an_account_whose_period_runs_starts_nothing() {
    let mut env = two_accounts("warm-running");
    let (started, _) = env.install_fake_claude_print();
    env.claude_usage(Some(3600));
    let (_, err, code) = env.run(&["status"]);
    assert_eq!(code, 0, "status: {err}");
    let (out, err, code) = env.run(&["warm", "now", "beta"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("runs already"), "{out}");
    assert!(!started.exists(), "Claude Code was not started");
}
