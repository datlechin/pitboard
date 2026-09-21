//! Drives the real binary through a full switch against a synthetic Claude Code
//! installation: its own config directory, its own keychain slot, its own state.
//!
//! Nothing here can reach the user's real login. The scratch slot's service name is
//! derived from a temporary path and asserted to differ from the default one before a
//! single byte is written.

use std::path::PathBuf;
use std::process::Command;

const SECURITY: &str = "/usr/bin/security";

struct Env {
    root: PathBuf,
    service: String,
    name: String,
}

/// Distinct per test, because park item names contain the account uuid and the tests
/// share one keychain.
fn state_accounts(env: &Env) -> Vec<serde_json::Value> {
    env.state()["accounts"].as_array().unwrap().clone()
}

fn uuid_for(test: &str, who: char) -> String {
    let tag: String = test
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    let padded = format!("{tag:x<8}");
    format!("{padded}-{who}111-4111-8111-111111111111")
}

fn account() -> String {
    std::env::var("USER").unwrap_or_else(|_| "claude-code-user".into())
}

impl Env {
    fn new(name: &str) -> Env {
        let root = std::env::temp_dir().join(format!("pitboard-e2e-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let service = pitboard::slot::service_for_dir(&root.to_string_lossy());
        assert_ne!(
            service,
            pitboard::slot::LIVE_SERVICE,
            "the test must never address the real credential slot"
        );
        Env {
            root,
            service,
            name: name.to_string(),
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_pitboard"));
        c.args(args)
            .env("CLAUDE_CONFIG_DIR", &self.root)
            .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR")
            .env("PITBOARD_HOME", self.root.join("pitboard"));
        c
    }

    fn run(&self, args: &[&str]) -> (String, String, i32) {
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

    fn sign_in(&self, uuid: &str, email: &str, org: &str, refresh: &str) {
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

    fn uuid(&self, who: char) -> String {
        uuid_for(&self.name, who)
    }

    fn live(&self) -> serde_json::Value {
        let raw = pitboard::store::vault_read(&self.service).unwrap().unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    fn config(&self) -> serde_json::Value {
        let raw = std::fs::read_to_string(self.root.join(".claude.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    fn state(&self) -> serde_json::Value {
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

#[test]
fn a_full_switch_moves_the_identity_and_nothing_else() {
    let env = Env::new("afull");

    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    let (out, err, code) = env.run(&["enroll", "alpha"]);
    assert_eq!(code, 0, "enroll alpha failed: {err}");
    assert!(out.contains("a@example.com"), "{out}");

    // The user signs in as the second account the normal way, which overwrites the
    // live credential. Alpha survives because enrolling parked it.
    env.sign_in(&env.uuid('b'), "b@example.com", &env.uuid('p'), "refresh-b");
    let (_, err, code) = env.run(&["enroll", "beta"]);
    assert_eq!(code, 0, "enroll beta failed: {err}");

    let (out, err, code) = env.run(&["use", "alpha"]);
    assert_eq!(code, 0, "switch failed: {err}");
    assert!(out.contains("parked `beta`"), "{out}");

    let live = env.live();
    assert_eq!(live["claudeAiOauth"]["refreshToken"], "refresh-a");
    assert_eq!(
        live["slackTag"]["machineBound"], true,
        "keys outside claudeAiOauth are bound to this machine and must not move"
    );

    let config = env.config();
    assert_eq!(config["oauthAccount"]["accountUuid"], env.uuid('a'));
    assert!(
        config["oauthAccount"].get("profileFetchedAt").is_none(),
        "the identity must be refetched rather than trusted from our copy"
    );
    assert!(
        config.get("cachedArtifactRoster").is_none(),
        "a cache derived from the outgoing organisation must be dropped"
    );
    assert_eq!(config["numStartups"], 7, "machine state must survive");

    let state = env.state();
    assert_eq!(state["active"], "alpha");
    let beta = state["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["label"] == "beta")
        .unwrap();
    assert!(
        !beta["generations"].as_array().unwrap().is_empty(),
        "the outgoing account must have been parked"
    );
    assert!(
        !env.root.join("pitboard/journal.json").exists(),
        "a completed switch must leave no journal behind"
    );
}

/// Asking for the account that is already signed in is not a failure: the state the
/// caller asked for already holds, so a menu bar clicking "switch to X" while X is active
/// must not report an error.
#[test]
fn switching_to_the_account_already_signed_in_succeeds_and_changes_nothing() {
    let env = Env::new("switchingto");
    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    env.run(&["enroll", "alpha"]);
    let before = env.state();

    let (out, _, code) = env.run(&["use", "alpha"]);
    assert_eq!(code, 0);
    assert!(out.contains("already signed in"), "{out}");
    assert_eq!(
        env.live()["claudeAiOauth"]["refreshToken"],
        "refresh-a",
        "nothing should have moved"
    );
    assert_eq!(
        env.state(),
        before,
        "no generation should have been created"
    );
}

#[test]
fn switching_away_from_an_unenrolled_account_is_refused() {
    let env = Env::new("switchingaway");
    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    env.run(&["enroll", "alpha"]);
    // A third account signs in without being enrolled; parking it is impossible.
    env.sign_in(&env.uuid('c'), "c@example.com", &env.uuid('q'), "refresh-c");

    let (_, err, code) = env.run(&["use", "alpha"]);
    assert_eq!(code, 1);
    assert!(err.contains("not enrolled"), "{err}");

    let live = env.live();
    assert_eq!(
        live["claudeAiOauth"]["refreshToken"], "refresh-c",
        "a refused switch must leave the credential untouched"
    );
}

#[test]
fn forgetting_the_signed_in_account_is_refused() {
    let env = Env::new("forgettingthe");
    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    env.run(&["enroll", "alpha"]);

    let (_, err, code) = env.run(&["forget", "alpha"]);
    assert_eq!(code, 1);
    assert!(err.contains("signed in"), "{err}");
}

#[test]
fn a_used_label_cannot_be_taken_by_another_account() {
    let env = Env::new("labelreuse");
    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    env.run(&["enroll", "shared"]);

    env.sign_in(&env.uuid('b'), "b@example.com", &env.uuid('p'), "refresh-b");
    let (_, err, code) = env.run(&["enroll", "shared"]);

    assert_eq!(
        code, 1,
        "reusing a label for a different account must be refused"
    );
    assert!(err.contains("already refers to"), "{err}");

    let state = env.state();
    assert_eq!(
        state["accounts"].as_array().unwrap().len(),
        1,
        "the first account must still be there"
    );
    let kept = &state["accounts"][0];
    assert_eq!(kept["email"], "a@example.com");
    assert!(
        !kept["generations"].as_array().unwrap().is_empty(),
        "and its parked credential must still be reachable"
    );
}

#[test]
fn a_credential_that_was_installed_is_never_restored_a_second_time() {
    let env = Env::new("consumed");
    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    env.run(&["enroll", "alpha"]);
    env.sign_in(&env.uuid('b'), "b@example.com", &env.uuid('p'), "refresh-b");
    env.run(&["enroll", "beta"]);

    env.run(&["use", "alpha"]);
    env.run(&["use", "beta"]);
    let (_, err, code) = env.run(&["use", "alpha"]);
    assert_eq!(
        code, 0,
        "the fresh park made on the way out must be restorable: {err}"
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a");

    let state = env.state();
    let alpha = state["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["label"] == "alpha")
        .unwrap();
    let consumed = alpha["generations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| !g["installed_at"].is_null())
        .count();
    assert!(
        consumed >= 2,
        "every generation that was installed must be marked, so a rotated token is never          offered again; got {alpha:#}"
    );
}

#[test]
fn an_account_whose_only_copy_was_already_used_is_refused_not_destroyed() {
    let env = Env::new("exhausted");
    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    env.run(&["enroll", "alpha"]);
    env.sign_in(&env.uuid('b'), "b@example.com", &env.uuid('p'), "refresh-b");
    env.run(&["enroll", "beta"]);
    env.run(&["use", "alpha"]);

    // Simulate the loss of the fresh park that `use alpha` made for beta: the exact state
    // a run killed between installing the credential and saving state would leave.
    let path = env.root.join("pitboard/state.json");
    let mut state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for account in state["accounts"].as_array_mut().unwrap() {
        if account["label"] == "beta" {
            for g in account["generations"].as_array_mut().unwrap() {
                g["installed_at"] = serde_json::json!(1789935600);
            }
        }
    }
    std::fs::write(&path, state.to_string()).unwrap();

    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 1);
    assert!(err.contains("no restorable parked login"), "{err}");
    assert_eq!(
        env.live()["claudeAiOauth"]["refreshToken"],
        "refresh-a",
        "a refusal must leave the working credential in place"
    );
}

#[test]
fn two_switches_at_once_do_not_interleave() {
    let env = Env::new("concurrent");
    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    env.run(&["enroll", "alpha"]);
    env.sign_in(&env.uuid('b'), "b@example.com", &env.uuid('p'), "refresh-b");
    env.run(&["enroll", "beta"]);
    env.run(&["use", "alpha"]);

    let mut first = env.command(&["use", "beta"]).spawn().unwrap();
    let mut second = env.command(&["use", "beta"]).spawn().unwrap();
    let a = first.wait().unwrap().code().unwrap_or(-1);
    let b = second.wait().unwrap().code().unwrap_or(-1);

    // Both exit 0: one performs the switch, the other finds beta already signed in and
    // says so. What must never happen is two switches interleaving.
    assert_eq!(a, 0, "first run");
    assert_eq!(b, 0, "second run");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");

    let accounts = state_accounts(&env);
    let parked = accounts
        .iter()
        .filter(|a| a["label"] == "alpha")
        .flat_map(|a| a["generations"].as_array().unwrap())
        .count();
    assert_eq!(
        parked, 2,
        "alpha should have its enrol copy plus exactly one park from the single switch"
    );

    let state = env.state();
    let alpha = state["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["label"] == "alpha")
        .unwrap();
    for g in alpha["generations"].as_array().unwrap() {
        let service = g["service"].as_str().unwrap();
        assert!(
            service.contains(&env.uuid('a')),
            "alpha must never be filed under another account's uuid: {service}"
        );
    }
}
