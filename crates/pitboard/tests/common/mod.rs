//! The harness shared by the integration tests: a synthetic Claude Code installation, a
//! stand-in for Anthropic, and the guard that keeps every test away from a real login.
//!
//! Every service name a test writes is derived from that test's own name, which Rust
//! already forbids two tests in a module from sharing.

// Compiled separately into every test binary that declares `mod common`, so a binary that
// does not use one of these helpers would otherwise warn about it.
#![allow(dead_code)]

/// This test process's own environment: the machine's real keychain account and slot.
pub fn ctx() -> pitboard_core::context::Context {
    pitboard_core::context::Context::from_env()
}

/// Refuse a service name that this machine's Claude Code would actually read.
///
/// A slot hashed from a scratch directory is safe by construction and is exactly what the
/// round-trip tests need, so the family as a whole is not off limits, only the two names
/// that resolve to a real login here.
pub fn guard_not_live(service: &str) {
    assert_ne!(
        service,
        pitboard_core::testing::LIVE_SERVICE,
        "a test must never address the default credential slot"
    );
    assert_ne!(
        service,
        pitboard_core::testing::live_service(&ctx()),
        "a test must never address the slot this machine's Claude Code reads"
    );
}

#[test]
fn the_guard_refuses_the_slots_that_hold_a_real_login() {
    guard_not_live("pitboard-citest-1");
    guard_not_live("Claude Code-credentials-deadbeef");

    let caught = std::panic::catch_unwind(|| guard_not_live(pitboard_core::testing::LIVE_SERVICE));
    assert!(caught.is_err(), "the default slot must be refused");
    let caught =
        std::panic::catch_unwind(|| guard_not_live(&pitboard_core::testing::live_service(&ctx())));
    assert!(caught.is_err(), "this machine's live slot must be refused");
}

use std::path::PathBuf;
use std::process::Command;

const SECURITY: &str = "/usr/bin/security";

pub struct Env {
    pub root: PathBuf,
    pub service: String,
    name: String,
    /// Stands in for Anthropic. It answers who a token belongs to exactly the way the real
    /// profile endpoint does, so the binary identifies accounts through its real code path.
    server: mockito::ServerGuard,
    mocks: Vec<mockito::Mock>,
}

/// alpha signed in and enrolled, beta enrolled by signing in privately.
pub fn two_accounts(name: &str) -> Env {
    let mut env = Env::new(name);
    let (a, o, b, p) = (env.uuid('a'), env.uuid('o'), env.uuid('b'), env.uuid('p'));
    env.sign_in(&a, "a@example.com", &o, "refresh-a");
    let (_, err, code) = env.run(&["enroll", "alpha"]);
    assert_eq!(code, 0, "enroll alpha: {err}");
    let (_, err, code) = env.enroll_by_signing_in("beta", &b, "b@example.com", &p, "refresh-b");
    assert_eq!(code, 0, "enroll beta: {err}");
    env
}

/// Distinct per test and per role. Park item names contain the account id and every test
/// shares one keychain, so two tests must never derive the same id.
pub fn uuid_for(test: &str, who: char) -> String {
    format!(
        "{}-{who}111-4111-8111-111111111111",
        pitboard_core::testing::dir_hash(test)
    )
}

pub fn account() -> String {
    std::env::var("USER").unwrap_or_else(|_| "claude-code-user".into())
}

impl Env {
    pub fn new(name: &str) -> Env {
        let root = std::env::temp_dir().join(format!("pitboard-e2e-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let service = pitboard_core::testing::service_for_dir(&root.to_string_lossy());
        guard_not_live(&service);

        let mut server = mockito::Server::new();
        let usage = server
            .mock("GET", "/api/oauth/usage")
            .with_status(200)
            .with_body(
                serde_json::json!({
                    "five_hour": {"utilization": 12.0, "resets_at": "2026-09-21T10:30:00+00:00"},
                    "seven_day": {"utilization": 40.0, "resets_at": "2026-09-27T02:00:00+00:00"},
                })
                .to_string(),
            )
            .create();
        Env {
            root,
            service,
            name: name.to_string(),
            server,
            mocks: vec![usage],
        }
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut c = Command::new(env!("CARGO_BIN_EXE_pitboard"));
        c.args(args)
            .env("CLAUDE_CONFIG_DIR", &self.root)
            .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR")
            .env("PITBOARD_HOME", self.root.join("pitboard"))
            .env("PITBOARD_API_BASE", self.server.url())
            .env("PATH", path);
        c
    }

    pub fn run(&self, args: &[&str]) -> (String, String, i32) {
        let out = self.command(args).output().expect("run pitboard");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.code().unwrap_or(-1),
        )
    }

    /// Enroll another account through `--sign-in`, the way a user would, with a stand-in
    /// for `claude auth login` that stores a login for the private directory it is given.
    pub fn enroll_by_signing_in(
        &mut self,
        label: &str,
        uuid: &str,
        email: &str,
        org: &str,
        refresh: &str,
    ) -> (String, String, i32) {
        let credential = credential(refresh).to_string();
        self.install_fake_claude(&credential);
        self.owns(&format!("access-{refresh}"), uuid, email, org);
        self.run(&["enroll", label, "--sign-in"])
    }

    /// `enroll_by_signing_in`, asking for the JSON envelope.
    pub fn enroll_by_signing_in_json(
        &mut self,
        label: &str,
        uuid: &str,
        email: &str,
        org: &str,
        refresh: &str,
    ) -> (String, String, i32) {
        let credential = credential(refresh).to_string();
        self.install_fake_claude(&credential);
        self.owns(&format!("access-{refresh}"), uuid, email, org);
        self.run(&["enroll", label, "--sign-in", "--json"])
    }

    pub fn install_fake_claude(&self, credential: &str) {
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let store = if cfg!(target_os = "macos") {
            format!(
                r#"hash=$(printf %s "$CLAUDE_CONFIG_DIR" | shasum -a 256 | cut -c1-8)
/usr/bin/security add-generic-password -U -a "{account}" -s "Claude Code-credentials-$hash" -w '{credential}'"#,
                account = account(),
            )
        } else {
            format!(r#"printf %s '{credential}' > "$CLAUDE_CONFIG_DIR/.credentials.json""#)
        };
        let script = bin.join("claude");
        std::fs::write(
            &script,
            // Talks on stdout like the real one, and can be made to wait like a person does.
            format!(
                "#!/bin/sh\n[ \"$1 $2\" = \"auth login\" ] || exit 64\n\
                 echo 'Opening browser to sign in'\nsleep \"${{FAKE_SIGN_IN_SECONDS:-0}}\"\n{store}\n"
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Park a login where the binary under test will look for it. On macOS that is the
    /// keychain; elsewhere it is this test's own pitboard home, never the machine's.
    pub fn write_park(&self, service: &str, contents: &str) {
        guard_not_live(service);
        if cfg!(target_os = "macos") {
            pitboard_core::testing::vault_write(&ctx(), service, contents).unwrap();
        } else {
            let vault = self.root.join("pitboard/vault");
            std::fs::create_dir_all(&vault).unwrap();
            std::fs::write(vault.join(format!("{service}.json")), contents).unwrap();
        }
    }

    pub fn is_parked(&self, service: &str) -> bool {
        if cfg!(target_os = "macos") {
            pitboard_core::testing::vault_read(&ctx(), service)
                .unwrap()
                .is_some()
        } else {
            self.root
                .join(format!("pitboard/vault/{service}.json"))
                .exists()
        }
    }

    pub fn delete_park(&self, service: &str) {
        if cfg!(target_os = "macos") {
            let _ = pitboard_core::testing::vault_delete(&ctx(), service);
        }
    }

    /// Make the fake token endpoint answer a renewal of `refresh`. The caller keeps the mock
    /// alive and asserts it was asked exactly once.
    pub fn answers_renewal(
        &mut self,
        refresh: &str,
        status: usize,
        body: serde_json::Value,
    ) -> mockito::Mock {
        self.server
            .mock("POST", "/v1/oauth/token")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh,
                "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
            })))
            .with_status(status)
            .with_body(body.to_string())
            .expect(1)
            .create()
    }

    /// Make the fake Anthropic answer as it does for an expired session.
    pub fn expire(&mut self, access_token: &str) {
        let mock = self
            .server
            .mock("GET", "/api/oauth/profile")
            .match_header("authorization", format!("Bearer {access_token}").as_str())
            .with_status(401)
            .with_body(r#"{"type":"error","error":{"type":"authentication_error"}}"#)
            .create();
        self.mocks.push(mock);
    }

    /// Teach the fake Anthropic who a token belongs to.
    pub fn owns(&mut self, access_token: &str, uuid: &str, email: &str, org: &str) {
        let mock = self
            .server
            .mock("GET", "/api/oauth/profile")
            .match_header("authorization", format!("Bearer {access_token}").as_str())
            .with_status(200)
            .with_body(
                serde_json::json!({
                    "account": {"uuid": uuid, "email": email},
                    "organization": {"uuid": org},
                })
                .to_string(),
            )
            .create();
        self.mocks.push(mock);
    }

    pub fn sign_in(&mut self, uuid: &str, email: &str, org: &str, refresh: &str) {
        let credential = credential(refresh);
        self.write_live(&credential.to_string());
        self.owns(&format!("access-{refresh}"), uuid, email, org);

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

    /// Where this platform's Claude Code keeps the live credential: the keychain slot on
    /// macOS, and `.credentials.json` in the config directory everywhere else.
    fn live_path(&self) -> PathBuf {
        self.root.join(".credentials.json")
    }

    /// Replaces the live credential document, for a test that needs it to be a particular
    /// shape rather than whatever a sign-in produced.
    pub fn replace_live(&self, credential: &serde_json::Value) {
        self.write_live(&credential.to_string());
    }

    fn write_live(&self, credential: &str) {
        if cfg!(target_os = "macos") {
            pitboard_core::testing::vault_write(&ctx(), &self.service, credential).unwrap();
        } else {
            std::fs::write(self.live_path(), credential).unwrap();
        }
    }

    pub fn live(&self) -> serde_json::Value {
        let raw = if cfg!(target_os = "macos") {
            pitboard_core::testing::vault_read(&ctx(), &self.service)
                .unwrap()
                .unwrap()
        } else {
            std::fs::read_to_string(self.live_path()).unwrap()
        };
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

    /// Change the account index the way only time or another tool would.
    pub fn edit_state(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let mut state = self.state();
        edit(&mut state);
        std::fs::write(self.root.join("pitboard/state.json"), state.to_string()).unwrap();
    }

    /// The item holding an account's parked login, if it has one.
    pub fn parked_service(&self, label: &str) -> Option<String> {
        self.state()["accounts"]
            .as_array()?
            .iter()
            .find(|a| a["label"] == label)?["parked"]["service"]
            .as_str()
            .map(str::to_owned)
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        // Off macOS every credential this harness created lives under `root`, which the
        // final line removes. On macOS they are keychain items and must be deleted by name.
        if cfg!(target_os = "macos")
            && let Ok(state) = std::fs::read_to_string(self.root.join("pitboard/state.json"))
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&state)
        {
            let parked = v["accounts"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| &a["parked"]["service"])
                .chain(v["discarded"].as_array().into_iter().flatten());
            for s in parked.filter_map(serde_json::Value::as_str) {
                let _ = Command::new(SECURITY)
                    .args(["delete-generic-password", "-a", &account(), "-s", s])
                    .output();
            }
        }
        if cfg!(target_os = "macos") {
            let _ = Command::new(SECURITY)
                .args([
                    "delete-generic-password",
                    "-a",
                    &account(),
                    "-s",
                    &self.service,
                ])
                .output();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A credential shaped like Claude Code's, keyed so the fake Anthropic can tell who owns it.
pub fn credential(refresh: &str) -> serde_json::Value {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    serde_json::json!({
        "claudeAiOauth": {
            "accessToken": format!("access-{refresh}"),
            "refreshToken": refresh,
            "expiresAt": now_ms + 8 * 3_600_000,
            "refreshTokenExpiresAt": now_ms + 30 * 86_400_000,
            "scopes": ["user:inference", "user:profile"],
            "subscriptionType": "max"
        },
        "slackTag": {"machineBound": true}
    })
}
