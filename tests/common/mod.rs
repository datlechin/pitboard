//! Shared guards for tests that touch the real keychain.
//!
//! Getting this wrong costs the user a login, so the rule is mandatory rather than
//! remembered: every service name a test writes must be derived from that test's own
//! identity, which Rust already forbids two tests in a module from sharing.

// Compiled separately into every test binary that declares `mod common`, so a binary that
// does not use one of these helpers would otherwise warn about it.
#![allow(dead_code)]

/// Refuse a service name that this machine's Claude Code would actually read.
///
/// A slot hashed from a scratch directory is safe by construction and is exactly what the
/// round-trip tests need, so the family as a whole is not off limits — only the two names
/// that resolve to a real login here.
pub fn guard_not_live(service: &str) {
    assert_ne!(
        service,
        pitboard::slot::LIVE_SERVICE,
        "a test must never address the default credential slot"
    );
    assert_ne!(
        service,
        pitboard::claude::live_service(),
        "a test must never address the slot this machine's Claude Code reads"
    );
}

#[test]
fn the_guard_refuses_the_slots_that_hold_a_real_login() {
    guard_not_live("pitboard-citest-1");
    guard_not_live("Claude Code-credentials-deadbeef");

    let caught = std::panic::catch_unwind(|| guard_not_live(pitboard::slot::LIVE_SERVICE));
    assert!(caught.is_err(), "the default slot must be refused");
    let caught = std::panic::catch_unwind(|| guard_not_live(&pitboard::claude::live_service()));
    assert!(caught.is_err(), "this machine's live slot must be refused");
}

use std::path::PathBuf;
use std::process::Command;

const SECURITY: &str = "/usr/bin/security";

pub struct Env {
    pub root: PathBuf,
    pub service: String,
    name: String,
}

/// Distinct per test, because park item names contain the account uuid and the tests
/// share one keychain.
pub fn state_accounts(env: &Env) -> Vec<serde_json::Value> {
    env.state()["accounts"].as_array().unwrap().clone()
}

pub fn uuid_for(test: &str, who: char) -> String {
    let tag: String = test
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    let padded = format!("{tag:x<8}");
    format!("{padded}-{who}111-4111-8111-111111111111")
}

pub fn account() -> String {
    std::env::var("USER").unwrap_or_else(|_| "claude-code-user".into())
}

impl Env {
    pub fn new(name: &str) -> Env {
        let root = std::env::temp_dir().join(format!("pitboard-e2e-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let service = pitboard::slot::service_for_dir(&root.to_string_lossy());
        guard_not_live(&service);
        Env {
            root,
            service,
            name: name.to_string(),
        }
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_pitboard"));
        c.args(args)
            .env("CLAUDE_CONFIG_DIR", &self.root)
            .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR")
            .env("PITBOARD_HOME", self.root.join("pitboard"));
        c
    }

    pub fn run(&self, args: &[&str]) -> (String, String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_pitboard"))
            .args(args)
            .env("CLAUDE_CONFIG_DIR", &self.root)
            .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR")
            .env("PITBOARD_HOME", self.root.join("pitboard"))
            .output()
            .expect("run pitboard");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.code().unwrap_or(-1),
        )
    }

    pub fn sign_in(&self, uuid: &str, email: &str, org: &str, refresh: &str) {
        let credential = serde_json::json!({
            "claudeAiOauth": {
                "accessToken": format!("access-{refresh}"),
                "refreshToken": refresh,
                "expiresAt": 1789928611576i64,
                "refreshTokenExpiresAt": 1792216138576i64,
                "scopes": ["user:inference", "user:profile"],
                "subscriptionType": "max"
            },
            "slackTag": {"machineBound": true}
        });
        pitboard::store::vault_write(&self.service, &credential.to_string()).unwrap();

        let config = serde_json::json!({
            "oauthAccount": {
                "accountUuid": uuid, "emailAddress": email, "organizationUuid": org,
                "organizationType": "claude_max", "profileFetchedAt": 1789871209282i64
            },
            "cachedArtifactRoster": {"org": org},
            "numStartups": 7
        });
        std::fs::write(self.root.join(".claude.json"), config.to_string()).unwrap();
    }

    pub fn uuid(&self, who: char) -> String {
        uuid_for(&self.name, who)
    }

    pub fn live(&self) -> serde_json::Value {
        let raw = pitboard::store::vault_read(&self.service).unwrap().unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    pub fn config(&self) -> serde_json::Value {
        let raw = std::fs::read_to_string(self.root.join(".claude.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    pub fn state(&self) -> serde_json::Value {
        let raw = std::fs::read_to_string(self.root.join("pitboard/state.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Ok(state) = std::fs::read_to_string(self.root.join("pitboard/state.json"))
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&state)
        {
            for a in v["accounts"].as_array().into_iter().flatten() {
                for g in a["generations"].as_array().into_iter().flatten() {
                    if let Some(s) = g["service"].as_str() {
                        let _ = Command::new(SECURITY)
                            .args(["delete-generic-password", "-a", &account(), "-s", s])
                            .output();
                    }
                }
            }
        }
        let _ = Command::new(SECURITY)
            .args([
                "delete-generic-password",
                "-a",
                &account(),
                "-s",
                &self.service,
            ])
            .output();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
