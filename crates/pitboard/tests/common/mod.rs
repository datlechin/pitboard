//! The harness shared by the integration tests: a synthetic Claude Code installation, a
//! stand-in for Anthropic, and the guard that keeps every test away from a real login.
//!
//! Each `Env` has a root named after its process and the name its test gives `Env::new`,
//! which refuses a name already used in that process. Claude Code's slot is hashed from the
//! root. The account ids, and the parks Pitboard names after them, are hashed from the name
//! alone, so a snapshot is the same on every run and two runs of one test at once share them.

// Compiled separately into every test binary that declares `mod common`, so a binary that
// does not use one of these helpers would otherwise warn about it.
#![allow(dead_code)]

pub(crate) mod os;

use os::Kept;
use pitboard_core::context::Environment;
use pitboard_core::testing::fs as files;
use pitboard_core::testing::lies_in;
use pitboard_core::testing::stand_in::{self, Script, Step};

/// Put a program at `path` that plays `script`: the compiled stand-in, the same on every
/// system, which `pitboard_core::testing::stand_in` says how to write a script for. A test
/// run alone, which builds no example, is told how to build it.
pub(crate) fn put_stand_in(path: &Path, script: &Script) {
    stand_in::install(path, script)
        .unwrap_or_else(|e| panic!("no stand-in at {}: {e}", path.display()));
}

/// The variables Pitboard reads that no test takes from whoever runs it, from the core's own
/// list of what it reads, so a variable added there is withheld here too.
// An inherited `PITBOARD_CLAUDE` would have a sign-in run the real `claude` on the real home.
pub(crate) fn withheld() -> impl Iterator<Item = &'static str> {
    pitboard_core::testing::variables().filter(|name| !os::passed_on().contains(name))
}

/// This test process's own environment, less what is withheld: the machine's real keychain
/// account, and the slot Claude Code reads by default.
#[allow(
    clippy::disallowed_methods,
    reason = "the tests' own environment, from which this withholds what Pitboard reads"
)]
pub(crate) fn ctx() -> pitboard_core::context::Context {
    // A folder nothing makes, so no Anthropic profile in the passed-on `HOME` is read.
    let no_profiles = std::env::temp_dir().join(format!(
        "pitboard-tests-{}-no-anthropic-profiles",
        std::process::id()
    ));
    let passed: Environment = std::env::vars_os()
        .filter(|(name, _)| !withheld().any(|kept| is(name, kept)))
        .chain([("ANTHROPIC_CONFIG_DIR".into(), no_profiles.into_os_string())])
        .collect();
    pitboard_core::context::Context::for_command_line(&passed)
}

fn is(name: &std::ffi::OsStr, variable: &str) -> bool {
    name.to_str()
        .is_some_and(|name| os::same_variable(name, variable))
}

pub(crate) fn started_in() -> pitboard_core::context::Context {
    pitboard_core::context::Context::from_env()
}

pub(crate) fn real_slots() -> [String; 2] {
    [
        pitboard_core::testing::LIVE_SERVICE.to_owned(),
        pitboard_core::testing::live_service(&started_in()),
    ]
}

pub(crate) fn refuse_a_real_login_place(scratch: &Path) {
    let places = pitboard_core::testing::real_login_places(&started_in())
        .unwrap_or_else(|why| panic!("{why}, so no test can tell its folder from a real login's"));
    if let Some(place) = places.iter().find(|place| lies_in(scratch, place)) {
        panic!(
            "a test may not use {} as its own folder: it is, holds or lies in {}",
            scratch.display(),
            place.display()
        );
    }
}

// By pattern, never by hashing real homes: how Codex spells its home on Windows is unmeasured.
pub(crate) fn guard_not_live(name: &str) {
    if let Some(why) = pitboard_core::provider::names::refusal(name, &[]) {
        panic!("{why}");
    }
}

use pitboard_core::testing::desktop::{self, JarAs, Person};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

const SECURITY: &str = "/usr/bin/security";

pub(crate) struct Env {
    pub root: PathBuf,
    pub service: String,
    name: String,
    /// Stands in for Anthropic. It answers who a token belongs to exactly the way the real
    /// profile endpoint does, so the binary identifies accounts through its real code path.
    server: mockito::ServerGuard,
    /// The usage endpoint, kept apart so a test can say how often Pitboard asked it. How
    /// often is a design question here, not an accident, so it is worth counting.
    usage: mockito::Mock,
    mocks: Vec<mockito::Mock>,
}

/// alpha signed in and enrolled, beta enrolled by signing in privately.
pub(crate) fn two_accounts(name: &str) -> Env {
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
pub(crate) fn uuid_for(test: &str, who: char) -> String {
    format!(
        "{}-{who}111-4111-8111-111111111111",
        pitboard_core::testing::dir_hash(test)
    )
}

#[allow(
    clippy::disallowed_methods,
    reason = "who runs the tests, whose keychain account a command is given as USER"
)]
pub(crate) fn account() -> String {
    std::env::var("USER").unwrap_or_else(|_| "claude-code-user".into())
}

static TAKEN: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

impl Env {
    pub(crate) fn new(name: &str) -> Env {
        let fresh = TAKEN.lock().unwrap().insert(name.to_owned());
        assert!(fresh, "another Env in this process is named {name:?}");
        let root = std::env::temp_dir().join(format!("pitboard-e2e-{}-{name}", std::process::id()));
        // Before anything there is emptied.
        refuse_a_real_login_place(&root);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        os::make_account_folders(&root);

        let service = pitboard_core::testing::service_for_dir(&root.to_string_lossy());
        for real in real_slots() {
            assert_ne!(
                service, real,
                "a test's own slot is one that holds a real login here"
            );
        }

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
            usage,
            mocks: Vec::new(),
        }
    }

    /// Count how often Pitboard asks Anthropic what an account has left, from here on.
    ///
    /// How often is a design question in this project rather than an accident, so it is
    /// worth a test. The expectation has to be set when the mock is made, so this replaces
    /// the one the environment started with.
    pub(crate) fn expect_usage_requests(&mut self, expected: usize) {
        self.usage.remove();
        self.usage = self
            .server
            .mock("GET", "/api/oauth/usage")
            .with_status(200)
            .with_body(
                serde_json::json!({
                    "five_hour": {"utilization": 12.0, "resets_at": "2026-09-21T10:30:00+00:00"},
                    "seven_day": {"utilization": 40.0, "resets_at": "2026-09-27T02:00:00+00:00"},
                })
                .to_string(),
            )
            .expect(expected)
            .create();
    }

    /// Check what `expect_usage_requests` asked for.
    pub(crate) fn assert_usage_requests(&self) {
        self.usage.assert();
    }

    /// A stand-in for `claude` that records how it was started, at the first path, and what
    /// it was handed on descriptor 3 and standard input, at the second, then answers as print
    /// mode does when a request is answered.
    pub(crate) fn install_fake_claude_print(&self) -> (PathBuf, PathBuf) {
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let started = self.root.join("claude-started.json");
        let handed = self.root.join("claude-handed.json");
        put_stand_in(
            &bin.join(os::program("claude")),
            &Script::Plays {
                args: None,
                steps: vec![
                    Step::Records {
                        at: started.clone(),
                        variables: vec![
                            "CLAUDE_CONFIG_DIR".into(),
                            "CLAUDE_CODE_GIT_BASH_PATH".into(),
                            "CLAUDE_CODE_OAUTH_TOKEN".into(),
                            "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR".into(),
                            "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB".into(),
                            "ANTHROPIC_API_KEY".into(),
                            "PITBOARD_HOME".into(),
                        ],
                    },
                    Step::ReadsDescriptor { at: handed.clone() },
                    Step::Says(
                        r#"{"type":"result","subtype":"success","is_error":false,"result":"OK","api_error_status":null}"#
                            .into(),
                    ),
                    Step::Exits(0),
                ],
            },
        );
        (started, handed)
    }

    /// Make Anthropic answer what every Claude Code account has left with its limits listed:
    /// its five-hour period running until `five_hour_resets_in` seconds from now, or not
    /// running where that is `None`, at 0%, and its weekly limit at 40% for a day more.
    pub(crate) fn claude_usage(&mut self, five_hour_resets_in: Option<i64>) {
        let at = |seconds: i64| {
            let when = std::time::SystemTime::now() + Duration::from_secs(seconds as u64);
            let epoch = when
                .duration_since(std::time::UNIX_EPOCH)
                .expect("after 1970")
                .as_secs() as i64;
            pitboard_core::time::local(epoch, "%Y-%m-%dT%H:%M:%S%:z")
        };
        self.usage.remove();
        self.usage = self
            .server
            .mock("GET", "/api/oauth/usage")
            .with_status(200)
            .with_body(
                serde_json::json!({
                    "limits": [
                        {"kind": "session", "percent": 0, "resets_at": five_hour_resets_in.map(at)},
                        {"kind": "weekly_all", "percent": 40, "resets_at": at(86_400)},
                    ],
                })
                .to_string(),
            )
            .create();
    }

    /// Make Anthropic answer as for a Team seat: a five-hour limit at 20% and one model's
    /// weekly limit, every limit listed, and no weekly limit for all models.
    pub(crate) fn team_seat_usage(&mut self) {
        self.usage.remove();
        self.usage = self
            .server
            .mock("GET", "/api/oauth/usage")
            .with_status(200)
            .with_body(
                serde_json::json!({
                    "five_hour": {"utilization": 20.0, "resets_at": "2099-01-01T10:00:00+00:00"},
                    "limits": [
                        {"kind": "session", "group": "session", "percent": 20,
                         "resets_at": "2099-01-01T10:00:00+00:00", "scope": null},
                        {"kind": "weekly_scoped", "group": "weekly", "percent": 10,
                         "resets_at": "2099-01-05T02:00:00+00:00",
                         "scope": {"model": {"id": null, "display_name": "Fable"}}},
                    ],
                })
                .to_string(),
            )
            .create();
    }

    // std's `Command` folds case on Windows: given `PATH` then `Path`, a child had one, `Path`.
    pub(crate) fn command(&self, args: &[&str]) -> Command {
        self.started(env!("CARGO_BIN_EXE_pitboard"), args)
    }

    // Another program, such as a shell that runs Pitboard, given what Pitboard is given.
    pub(crate) fn started(&self, program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Command {
        let mut c = Command::new(program);
        for name in withheld() {
            c.env_remove(name);
        }
        c.args(args).envs(self.given());
        c
    }

    // A copy of this build in the folder first on the PATH a command is given, so a shell finds
    // the bare `pitboard`, and a run holds nothing of the build the tests share.
    pub(crate) fn put_pitboard_on_path(&self) -> PathBuf {
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).expect("a bin folder");
        let copy = bin.join(os::program("pitboard"));
        std::fs::copy(env!("CARGO_BIN_EXE_pitboard"), &copy)
            .unwrap_or_else(|e| panic!("{} could not be made: {e}", copy.display()));
        copy
    }

    // Without these the suite reads whatever login the person running it is signed in to.
    fn given(&self) -> Vec<(&'static str, std::ffi::OsString)> {
        let mut given = vec![
            ("CLAUDE_CONFIG_DIR", self.root.clone().into_os_string()),
            ("CODEX_HOME", self.codex_home().into_os_string()),
            ("PITBOARD_HOME", self.root.join("pitboard").into_os_string()),
            (
                "PITBOARD_TEST_CLAUDE_DESKTOP",
                self.root.join("claude").into_os_string(),
            ),
            ("PITBOARD_API_BASE", self.server.url().into()),
            ("PATH", os::search_path(&self.root.join("bin"))),
            // Ahead of `HOME`, which is passed on, for Claude Code's Anthropic profiles.
            (
                "ANTHROPIC_CONFIG_DIR",
                self.root.join("anthropic").into_os_string(),
            ),
        ];
        given.extend(
            os::account_folders(&self.root)
                .into_iter()
                .map(|(name, folder)| (name, folder.into_os_string())),
        );
        given
    }

    // Every variable a command is started with, for a start std's Command cannot make.
    #[allow(
        clippy::disallowed_methods,
        reason = "the tests' own environment, from which this withholds what Pitboard reads"
    )]
    pub(crate) fn environment(&self) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
        let given = self.given();
        let set: Vec<&str> = given.iter().map(|(name, _)| *name).collect();
        let kept: Vec<_> = std::env::vars_os()
            .filter(|(name, _)| {
                !withheld().any(|kept| is(name, kept)) && !set.iter().any(|set| is(name, set))
            })
            .collect();
        kept.into_iter()
            .chain(
                given
                    .into_iter()
                    .map(|(name, value)| (std::ffi::OsString::from(name), value)),
            )
            .collect()
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "the tests' own environment, from which this withholds what Pitboard reads"
    )]
    pub(crate) fn context(&self) -> pitboard_core::context::Context {
        let kept = std::env::vars_os().filter(|(name, _)| !withheld().any(|kept| is(name, kept)));
        let given = self
            .given()
            .into_iter()
            .map(|(name, value)| (std::ffi::OsString::from(name), value));
        let env: Environment = kept.chain(given).collect();
        pitboard_core::context::Context::for_command_line(&env)
    }

    pub(crate) fn guard(&self, name: &str) {
        if let Some(why) =
            pitboard_core::provider::names::refusal(name, std::slice::from_ref(&self.service))
        {
            panic!("{why}");
        }
    }

    /// This test's own `CODEX_HOME`, created because Codex requires the directory to exist.
    pub(crate) fn codex_home(&self) -> PathBuf {
        let dir = self.root.join("codex");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    pub(crate) fn run(&self, args: &[&str]) -> (String, String, i32) {
        self.run_with(args, &[])
    }

    /// `run`, with `vars` set besides, over what the harness sets or withholds.
    pub(crate) fn run_with(&self, args: &[&str], vars: &[(&str, &str)]) -> (String, String, i32) {
        let mut command = self.command(args);
        for (name, value) in vars {
            command.env(name, value);
        }
        let out = command.output().expect("run Pitboard");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.code().unwrap_or(-1),
        )
    }

    /// Where the stand-in for Anthropic listens, as `host:port`, which is what a proxy is
    /// asked to connect to for a request to it.
    pub(crate) fn api_address(&self) -> String {
        self.server.host_with_port()
    }

    /// Enroll another account through `--sign-in`, the way a user would, with a stand-in
    /// for `claude auth login` that stores a login for the private directory it is given.
    pub(crate) fn enroll_by_signing_in(
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
    pub(crate) fn enroll_by_signing_in_json(
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

    /// Stand in for `claude auth login`: stores `credential` where Claude Code keeps the
    /// login of whichever `CLAUDE_CONFIG_DIR` it is run with, the private directory Pitboard
    /// makes for a sign-in, and does nothing else. Started any other way, it refuses.
    pub(crate) fn install_fake_claude(&self, credential: &str) {
        self.put_fake_claude(credential, None);
    }

    /// `install_fake_claude`, whose sign-in waits for a line on its standard input before it
    /// stores the login, as a person takes a while in the browser. Where its input ends
    /// first, it stores nothing.
    pub(crate) fn install_fake_claude_held(&self, credential: &str) {
        self.put_fake_claude(credential, Some(Step::WaitsForALine));
    }

    fn put_fake_claude(&self, credential: &str, held: Option<Step>) {
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let store = match os::claude_code_login() {
            // The item Claude Code makes for the directory, written through `security` as the
            // script this replaced wrote it.
            Kept::InKeychain => Step::StoresInKeychain {
                under: "CLAUDE_CONFIG_DIR".into(),
                account: account(),
                contents: credential.into(),
                never: real_slots().into(),
            },
            // 0600, and by the same route Claude Code takes: write, then chmod. A shim
            // that leaves the umask to decide writes a login anybody can read, which
            // `doctor` is right to fail on and which Claude Code does not do.
            Kept::InFile => Step::WritesLogin {
                under: "CLAUDE_CONFIG_DIR".into(),
                name: ".credentials.json".into(),
                contents: credential.into(),
            },
            Kept::Sealed => unreachable!("{SEALS_NOTHING}"),
        };
        let mut steps = vec![Step::Says("Opening browser to sign in\n".into())];
        steps.extend(held);
        steps.push(store);
        put_stand_in(
            &bin.join(os::program("claude")),
            // Talks on stdout like the real one, and can be made to wait like a person does.
            &Script::Plays {
                args: Some(vec!["auth".into(), "login".into()]),
                steps,
            },
        );
    }

    /// Park a login where the binary under test will look for it: the keychain where
    /// Pitboard parks there, and otherwise this test's own Pitboard home, never the
    /// machine's.
    pub(crate) fn write_park(&self, service: &str, contents: &str) {
        guard_not_live(service);
        match os::parked_login() {
            Kept::InKeychain | Kept::Sealed => {
                pitboard_core::testing::vault_write(&self.context(), service, contents)
                    .unwrap_or_else(|e| panic!("{service} could not be parked: {e}"));
            }
            // The access Pitboard's own file vault gives, for the same reason: a parked
            // login is a plaintext token and `doctor` fails on one anybody can read.
            Kept::InFile => {
                let vault = self.root.join("pitboard/vault");
                os::create_private_dir(&vault);
                os::write_private(&vault.join(format!("{service}.json")), contents);
            }
        }
    }

    pub(crate) fn is_parked(&self, service: &str) -> bool {
        match os::parked_login() {
            Kept::InKeychain | Kept::Sealed => {
                pitboard_core::testing::vault_read(&self.context(), service)
                    .unwrap_or_else(|e| panic!("{service} could not be read: {e}"))
                    .is_some()
            }
            Kept::InFile => self
                .root
                .join(format!("pitboard/vault/{service}.json"))
                .exists(),
        }
    }

    /// Take a parked login out of the store behind Pitboard's back, which is how a test
    /// says "this one is gone" without Pitboard's own records agreeing.
    ///
    /// The Linux half was missing, so on Linux this deleted nothing and every test that
    /// used it went on with the park still there. `use` then gave it back, which is
    /// `repair`'s own behaviour and correct, so the test that meant to see a refusal saw a
    /// switch. Never caught, because CI's Linux leg was being cancelled by a lint failure
    /// before it got this far.
    pub(crate) fn delete_park(&self, service: &str) {
        guard_not_live(service);
        match os::parked_login() {
            Kept::InKeychain | Kept::Sealed => {
                let _ = pitboard_core::testing::vault_delete(&self.context(), service);
            }
            Kept::InFile => {
                let _ =
                    std::fs::remove_file(self.root.join(format!("pitboard/vault/{service}.json")));
            }
        }
    }

    /// Claude Code signed out behind Pitboard's back: its login is gone from where it keeps
    /// it, and nothing else changed.
    pub(crate) fn sign_out(&self) {
        self.guard(&self.service);
        match os::claude_code_login() {
            Kept::InKeychain => {
                let _ = pitboard_core::testing::vault_delete(&self.context(), &self.service);
            }
            Kept::InFile => {
                let _ = std::fs::remove_file(self.live_path());
            }
            Kept::Sealed => unreachable!("{SEALS_NOTHING}"),
        }
    }

    /// Make the fake token endpoint answer a renewal of `refresh`. The caller keeps the mock
    /// alive and asserts it was asked exactly once.
    pub(crate) fn answers_renewal(
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
    pub(crate) fn expire(&mut self, access_token: &str) {
        for (method, path) in WHOSE {
            let mock = self
                .server
                .mock(method, path)
                .match_header("authorization", format!("Bearer {access_token}").as_str())
                .with_status(401)
                .with_body(r#"{"type":"error","error":{"type":"authentication_error"}}"#)
                .create();
            self.mocks.push(mock);
        }
    }

    /// Anthropic answers, badly. The switch that cannot identify the signed-in account
    /// must say so in a way a program can act on, rather than as one code and a sentence.
    pub(crate) fn identity_trouble(&mut self, status: usize) {
        self.identity_answers(status, r#"{"type":"error","error":{"type":"api_error"}}"#);
    }

    /// Anthropic answers every question of whose a token is with `status` and `body`, as
    /// request `req_test`.
    pub(crate) fn identity_answers(&mut self, status: usize, body: &str) {
        for (method, path) in WHOSE {
            let mock = self
                .server
                .mock(method, path)
                .with_status(status)
                .with_header("request-id", "req_test")
                .with_body(body)
                .create();
            self.mocks.push(mock);
        }
    }

    /// Anthropic answers every profile request with `status` and `body`, and nothing else.
    pub(crate) fn profile_answers(&mut self, status: usize, body: &str) {
        let mock = self
            .server
            .mock("GET", "/api/oauth/profile")
            .with_status(status)
            .with_body(body)
            .create();
        self.mocks.push(mock);
    }

    /// Teach the fake Anthropic who a token belongs to.
    pub(crate) fn owns(&mut self, access_token: &str, uuid: &str, email: &str, org: &str) {
        let validated = self
            .server
            .mock("POST", "/api/oauth/validate")
            .match_header("authorization", format!("Bearer {access_token}").as_str())
            .with_status(200)
            .with_body(
                serde_json::json!({
                    "valid": true, "account_uuid": uuid, "organization_uuid": org,
                    "scopes": ["user:inference", "user:profile"],
                })
                .to_string(),
            )
            .create();
        self.mocks.push(validated);
        let profile = self
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
        self.mocks.push(profile);
    }

    pub(crate) fn sign_in(&mut self, uuid: &str, email: &str, org: &str, refresh: &str) {
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

    pub(crate) fn uuid(&self, who: char) -> String {
        uuid_for(&self.name, who)
    }

    /// Sign Codex in to `account` the way `codex login` leaves it: an `auth.json` in this
    /// test's own `CODEX_HOME`, mode 0600, whose ID token names the account. The token is
    /// signed by nothing, and nothing in Pitboard checks a signature: it reads the claims.
    pub(crate) fn sign_in_codex(&self, account: &str, email: &str, refresh: &str) {
        let login = codex_login(account, email, refresh);
        os::write_private(&self.codex_home().join("auth.json"), &login.to_string());
    }

    /// Stand in for `codex login`: signs `login` into whichever `CODEX_HOME` it is run with,
    /// the private directory Pitboard makes for a sign-in, and does nothing else. Every
    /// test that runs a Codex sign-in installs this first, because the harness keeps the
    /// real `PATH` behind its own `bin`, and the real `codex login` must never run here.
    pub(crate) fn install_fake_codex_login(&self, login: &serde_json::Value) {
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let program = bin.join(os::program("codex"));
        let _ = std::fs::remove_file(&program);
        put_stand_in(&program, &fake_codex_login(login));
    }

    /// `install_fake_codex_login` laid out the way npm installs Codex, in a prefix of this
    /// test's own that no `PATH` has. Returns the program as found.
    pub(crate) fn install_fake_npm_codex_login(&self, login: &serde_json::Value) -> PathBuf {
        let prefix = self.root.join("npm");
        let script = serde_json::to_string(&fake_codex_login(login)).expect("a script");
        let script = format!("#!/usr/bin/env fakenode\n{script}\n");
        let write = |at: &Path, contents: &str| {
            std::fs::create_dir_all(at.parent().expect("a folder")).unwrap();
            stand_in::write_program(at, contents)
                .unwrap_or_else(|e| panic!("nothing at {}: {e}", at.display()));
        };
        match os::npm() {
            os::Npm::LinksTheScript => {
                let bin = prefix.join("bin");
                std::fs::create_dir_all(&bin).unwrap();
                put_stand_in(&bin.join("fakenode"), &Script::Interprets);
                write(
                    &prefix.join("lib/node_modules/@openai/codex/bin/codex.js"),
                    &script,
                );
                let program = bin.join("codex");
                let _ = std::fs::remove_file(&program);
                files::link(
                    Path::new("../lib/node_modules/@openai/codex/bin/codex.js"),
                    &program,
                )
                .unwrap();
                program
            }
            os::Npm::WritesAShim => {
                // Node's Windows installer keeps `node.exe` in its own folder, not npm's prefix.
                let node = self.root.join("nodejs");
                std::fs::create_dir_all(&node).unwrap();
                put_stand_in(&node.join(os::program("fakenode")), &Script::Interprets);
                let target = ["node_modules", "@openai", "codex", "bin", "codex.js"];
                write(
                    &target.iter().fold(prefix.clone(), |at, part| at.join(part)),
                    &script,
                );
                // The platform's package npm installs beside it, whose program a sign-in starts.
                let (platform, triple) = if cfg!(target_arch = "aarch64") {
                    ("codex-win32-arm64", "aarch64-pc-windows-msvc")
                } else {
                    ("codex-win32-x64", "x86_64-pc-windows-msvc")
                };
                let package = [
                    "node_modules",
                    "@openai",
                    "codex",
                    "node_modules",
                    "@openai",
                ]
                .iter()
                .fold(prefix.clone(), |at, part| at.join(part))
                .join(platform);
                let vendored = package.join("vendor").join(triple).join("bin");
                std::fs::create_dir_all(&vendored).unwrap();
                std::fs::write(package.join("package.json"), "{}").unwrap();
                put_stand_in(&vendored.join("codex.exe"), &fake_codex_login(login));
                let program = prefix.join("codex.cmd");
                write(&program, &npm_shim("fakenode", &target.join("\\")));
                program
            }
        }
    }

    /// The live Codex login, as this test's `CODEX_HOME` holds it.
    pub(crate) fn codex_live(&self) -> serde_json::Value {
        let raw = std::fs::read_to_string(self.codex_home().join("auth.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    /// Sign Codex in with an API key rather than an account, the way `codex login
    /// --with-api-key` leaves it. The key is made up.
    pub(crate) fn sign_in_codex_with_an_api_key(&self) {
        let login = serde_json::json!({
            "auth_mode": "apikey",
            "OPENAI_API_KEY": "sk-not-a-real-key",
        });
        os::write_private(&self.codex_home().join("auth.json"), &login.to_string());
    }

    /// Put a `codex` of this test's own first on `PATH`, laid out the way Codex's standalone
    /// installer lays one out, so whatever reads which Codex is installed reads this one.
    ///
    /// The `PATH` a test runs with ends with the real one on macOS and Linux, and the real
    /// standalone install lives inside the developer's own `~/.codex`, which no test may go
    /// near. Running it fails loudly: nothing that only asks which version it is ever runs it.
    pub(crate) fn install_fake_codex(&self, version: &str) {
        let installed = self
            .root
            .join("codex-install/releases")
            .join(format!("{version}-test-target"))
            .join("bin")
            .join(os::program("codex"));
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        put_stand_in(
            &installed,
            &Script::refusing("a test stand-in for codex, not meant to run\n"),
        );
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let link = bin.join(os::program("codex"));
        let _ = std::fs::remove_file(&link);
        files::link(&installed, &link).unwrap();
    }

    /// Make the fake OpenAI answer what a Codex login has left: a five-hour window and a
    /// weekly one, in the shape its usage endpoint answers with.
    pub(crate) fn codex_usage(&mut self, five_hour: f64, weekly: f64) {
        let window = |percent: f64, seconds: i64, reset_at: i64| {
            serde_json::json!({
                "used_percent": percent,
                "limit_window_seconds": seconds,
                "reset_after_seconds": 3600,
                "reset_at": reset_at,
            })
        };
        self.codex_usage_answer(&serde_json::json!({
            "plan_type": "pro",
            "rate_limit": {
                "allowed": true,
                "limit_reached": false,
                "primary_window": window(five_hour, 18_000, 1_789_990_000),
                "secondary_window": window(weekly, 604_800, 1_790_500_000),
            },
        }));
    }

    // OpenAI's usage endpoint answers `answer` to every login.
    pub(crate) fn codex_usage_answer(&mut self, answer: &serde_json::Value) {
        let mock = self
            .server
            .mock("GET", "/wham/usage")
            .with_status(200)
            .with_body(answer.to_string())
            .create();
        self.mocks.push(mock);
    }

    /// Where Claude Code keeps the live credential where it keeps it in a file:
    /// `.credentials.json` in the config directory.
    fn live_path(&self) -> PathBuf {
        self.root.join(".credentials.json")
    }

    /// Replaces the live credential document, for a test that needs it to be a particular
    /// shape rather than whatever a sign-in produced.
    ///
    /// In the keychain, written the way Claude Code writes a large one, through `security`'s
    /// argument line, because Pitboard itself refuses to and that refusal is what some of
    /// these tests are about. The item is this test's own, guarded like every other write
    /// here.
    pub(crate) fn replace_live(&self, credential: &serde_json::Value) {
        let body = credential.to_string();
        match os::claude_code_login() {
            Kept::InKeychain => {}
            Kept::InFile => {
                self.write_live(&body);
                return;
            }
            Kept::Sealed => unreachable!("{SEALS_NOTHING}"),
        }
        self.guard(&self.service);
        let done = Command::new(SECURITY)
            .args([
                "add-generic-password",
                "-U",
                "-a",
                &account(),
                "-s",
                &self.service,
                "-X",
                &hex(body.as_bytes()),
            ])
            .status()
            .expect("security ran");
        assert!(
            done.success(),
            "security refused to write the test credential"
        );
    }

    fn write_live(&self, credential: &str) {
        self.guard(&self.service);
        match os::claude_code_login() {
            Kept::InKeychain => {
                pitboard_core::testing::vault_write(&self.context(), &self.service, credential)
                    .unwrap();
            }
            // Private, because that is what Claude Code writes: it chmods the plaintext
            // credential after writing it, and a stand-in that leaves the umask to decide
            // is a stand-in for something else. `doctor` reads these modes and fails on a
            // login anybody can read, which is how this was found.
            Kept::InFile => os::write_private(&self.live_path(), credential),
            Kept::Sealed => unreachable!("{SEALS_NOTHING}"),
        }
    }

    // Claude Code with Credential Manager turned on in this test's own config, its login moved
    // into this test's own slot as its first write there moves it (`credman_migration`). The
    // harness deletes the slot's items by name.
    #[cfg(windows)]
    pub(crate) fn turn_credential_manager_on(&self) {
        self.guard(&self.service);
        let mut config = self.config();
        config["cachedGrowthBookFeatures"] = serde_json::json!({"tengu_windows_credman": true});
        std::fs::write(self.root.join(".claude.json"), config.to_string()).unwrap();
        let login = std::fs::read_to_string(self.live_path()).unwrap();
        pitboard_core::testing::credman::write(&self.context(), &self.service, &login).unwrap();
        std::fs::remove_file(self.live_path()).unwrap();
    }

    pub(crate) fn live(&self) -> serde_json::Value {
        let raw = match os::claude_code_login() {
            Kept::InKeychain => pitboard_core::testing::vault_read(&self.context(), &self.service)
                .unwrap()
                .unwrap(),
            Kept::InFile => std::fs::read_to_string(self.live_path()).unwrap(),
            Kept::Sealed => unreachable!("{SEALS_NOTHING}"),
        };
        serde_json::from_str(&raw).unwrap()
    }

    pub(crate) fn config(&self) -> serde_json::Value {
        let raw = std::fs::read_to_string(self.root.join(".claude.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    pub(crate) fn state(&self) -> serde_json::Value {
        let raw = std::fs::read_to_string(self.root.join("pitboard/state.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    /// The label of `tool`'s account whose login Pitboard last recorded `tool`'s store
    /// holding.
    pub(crate) fn in_use(&self, tool: &str) -> Option<String> {
        let state: pitboard_core::state::State = serde_json::from_value(self.state()).unwrap();
        let which = pitboard_core::provider::ProviderId::parse(tool).unwrap();
        state
            .account_in_use(which)
            .map(|account| account.label.clone())
    }

    /// What Claude Code's `label` is filed under: its parked logins, its readings and a
    /// switch's record name it by this.
    pub(crate) fn account_id(&self, label: &str) -> String {
        self.state()["accounts"]
            .as_array()
            .and_then(|accounts| {
                accounts
                    .iter()
                    .find(|a| a["provider"] == "claude" && a["label"] == label)
            })
            .and_then(|account| account["id"].as_str())
            .unwrap_or_else(|| panic!("no Claude Code account `{label}`"))
            .to_owned()
    }

    /// Change the account index the way only time or another tool would.
    pub(crate) fn edit_state(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let mut state = self.state();
        edit(&mut state);
        std::fs::write(self.root.join("pitboard/state.json"), state.to_string()).unwrap();
    }

    /// What Anthropic last answered of each Claude Code account's five-hour and weekly limits,
    /// by label, written where and as every front end records an answer. Each limit resets an
    /// hour and a day from now.
    pub(crate) fn measured(&self, shares: &[(&str, f64, f64)]) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after 1970")
            .as_secs() as i64;
        let window = |kind: &str, percent: f64, resets_in: i64| {
            serde_json::json!({
                "kind": kind,
                "scope": null,
                "percent": percent,
                "resets_at": now + resets_in,
                "is_active": true,
            })
        };
        let readings: serde_json::Map<String, serde_json::Value> = shares
            .iter()
            .map(|&(label, session, weekly)| {
                let id = self.account_id(label);
                let reading = serde_json::json!({
                    "windows": [
                        window("session", session, 3600),
                        window("weekly_all", weekly, 86_400),
                    ],
                    "observed_at": now,
                    "answered_at": now,
                    "lists_every_limit": true,
                    "source": "live",
                });
                (id, reading)
            })
            .collect();
        std::fs::write(
            self.root.join("pitboard/usage.json"),
            serde_json::Value::Object(readings).to_string(),
        )
        .unwrap();
    }

    /// Change the readings by account id, as a later answer from Anthropic would.
    pub(crate) fn edit_readings(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let path = self.root.join("pitboard/usage.json");
        let mut readings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        edit(&mut readings);
        std::fs::write(path, readings.to_string()).unwrap();
    }

    /// Every account was last put in use an hour ago. Enrolling the account signed in puts
    /// it in use, and Pitboard switches nothing away from an account for five minutes after.
    pub(crate) fn an_hour_on(&self) {
        self.edit_state(|state| {
            for account in state["accounts"].as_array_mut().expect("accounts") {
                if let Some(at) = account["last_used_at"].as_i64() {
                    account["last_used_at"] = serde_json::json!(at - 3600);
                }
            }
        });
    }

    /// The item holding an account's parked login, if it has one.
    pub(crate) fn parked_service(&self, label: &str) -> Option<String> {
        self.state()["accounts"]
            .as_array()?
            .iter()
            .find(|a| a["label"] == label)?["parked"]["service"]
            .as_str()
            .map(str::to_owned)
    }
}

// Claude Desktop accounts, each a session of its own, as a test's Claude signs in to them.
pub(crate) const WORK: Person = Person {
    uuid: "6f1c0a52-1111-4a6e-9d0b-2f6c1a9e7b10",
    session: "work-session",
};
pub(crate) const HOME: Person = Person {
    uuid: "b2aa91c4-3333-4c7a-a1d2-5e8f0c21d4e5",
    session: "home-session",
};
pub(crate) const TRAVEL: Person = Person {
    uuid: "c3bb02d5-5555-4d8b-b2e3-6f9a1d32e5f6",
    session: "travel-session",
};

impl Env {
    // This test's own Claude: its data folder `data` and its app `Claude.app`, never the real ones.
    pub(crate) fn claude_desktop(&self) -> PathBuf {
        self.root.join("claude")
    }

    // Claude closed and signed in to `who`, as it leaves its folder, with a SQLite jar.
    pub(crate) fn plant_desktop(&self, who: &Person) {
        desktop::plant(&self.claude_desktop().join("data"), who, JarAs::Sqlite);
    }

    // What Claude writes as it quits signed in to `who`, over whatever its folder holds.
    pub(crate) fn claude_signs_in(&self, who: &Person) {
        desktop::sign_in_on_disk(&self.claude_desktop().join("data"), who, JarAs::Sqlite);
    }

    // The stand-in under this test's own `Claude.app`, which `ps` lists there as Claude.
    pub(crate) fn open_claude(&self) -> RunningClaude {
        let program = self
            .claude_desktop()
            .join("Claude.app/Contents/MacOS/Claude");
        std::fs::create_dir_all(program.parent().expect("its folder")).unwrap();
        put_stand_in(
            &program,
            &Script::Plays {
                args: None,
                steps: vec![Step::WaitsForALine],
            },
        );
        // Its input stays open, and nothing is typed to it, so it runs until it is dropped.
        let running = Command::new(&program)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in Claude starts");
        RunningClaude(running)
    }

    // `pitboard <args>` left running, its standard error read a line at a time.
    pub(crate) fn running(&self, args: &[&str]) -> Running {
        let mut running = self
            .command(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("pitboard starts");
        let stderr = running.stderr.take().expect("its standard error");
        let (send, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Running {
            running,
            lines,
            said: Vec::new(),
        }
    }

    // A whole sign-in inside Claude: Claude opened once Pitboard waits, `who` signed in, quit.
    pub(crate) fn sign_in_inside_claude(
        &self,
        args: &[&str],
        who: &Person,
    ) -> (String, String, i32) {
        let mut signing_in = self.running(args);
        signing_in.until("Waiting for Claude to quit");
        let claude = self.open_claude();
        signing_in.until("Claude is open.");
        self.claude_signs_in(who);
        drop(claude);
        signing_in.finish()
    }

    // `desktop/work` in use and `desktop/home` parked, by a real enrolment, sign-in and switch.
    pub(crate) fn desktop_accounts(&self) {
        self.plant_desktop(&WORK);
        let (_, err, code) = self.run(&["enroll", "desktop/work"]);
        assert_eq!(code, 0, "enroll desktop/work: {err}");
        let (_, err, code) =
            self.sign_in_inside_claude(&["enroll", "desktop/home", "--sign-in"], &HOME);
        assert_eq!(code, 0, "enroll desktop/home --sign-in: {err}");
        let (_, err, code) = self.run(&["use", "desktop/work"]);
        assert_eq!(code, 0, "use desktop/work: {err}");
    }
}

// Killed and reaped on drop, panic or not: CI waits on a process a test leaves behind.
pub(crate) struct RunningClaude(std::process::Child);

impl Drop for RunningClaude {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// Killed and reaped on drop, as `RunningClaude` is.
pub(crate) struct Running {
    running: std::process::Child,
    lines: mpsc::Receiver<String>,
    said: Vec<String>,
}

impl Running {
    // Every line said so far, once one starting with `line` has been, within a minute.
    pub(crate) fn until(&mut self, line: &str) -> Vec<String> {
        loop {
            let next = self
                .lines
                .recv_timeout(Duration::from_secs(60))
                .unwrap_or_else(|_| panic!("no `{line}` within a minute: {:?}", self.said));
            let found = next.starts_with(line);
            self.said.push(next);
            if found {
                return self.said.clone();
            }
        }
    }

    // What it printed and its exit status, once it has ended, within a minute.
    pub(crate) fn finish(mut self) -> (String, String, i32) {
        let deadline = Instant::now() + Duration::from_secs(60);
        // Its standard error closes as it exits, and nothing it starts keeps it.
        loop {
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) => self.said.push(line),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("still running after a minute: {:?}", self.said)
                }
            }
        }
        let status = self.running.wait().expect("its status");
        let mut out = String::new();
        if let Some(mut stdout) = self.running.stdout.take() {
            let _ = stdout.read_to_string(&mut out);
        }
        let err: String = self.said.iter().map(|line| format!("{line}\n")).collect();
        (out, err, status.code().unwrap_or(-1))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.running.kill();
        let _ = self.running.wait();
    }
}

impl Env {
    /// Every park this test's Pitboard names: in its account index, parked or discarded, and
    /// in the record of a switch left interrupted, which names its parks there and nowhere
    /// else.
    fn parks_named(&self) -> Vec<String> {
        let read = |name: &str| {
            std::fs::read_to_string(self.root.join("pitboard").join(name))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        };
        let mut named = Vec::new();
        if let Some(v) = read("state.json") {
            // Schema 7 lists each as its name.
            let discarded = v["discarded"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|d| if d.is_string() { d } else { &d["service"] });
            let parked = v["accounts"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| &a["parked"]["service"])
                .chain(discarded);
            named.extend(
                parked
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned),
            );
        }
        if let Some(v) = read("journal.json") {
            let recorded = [&v["park_service"], &v["incoming_service"]];
            named.extend(
                recorded
                    .into_iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned),
            );
        }
        named
    }
}

/// Deletes the keychain item `service` under this test's account, if it is there.
fn delete_keychain_item(service: &str) {
    let _ = Command::new(SECURITY)
        .args(["delete-generic-password", "-a", &account(), "-s", service])
        .output();
}

const SEALS_NOTHING: &str = "Claude Code keeps no login sealed: only Pitboard's vault does";

impl Drop for Env {
    fn drop(&mut self) {
        // A login kept in a file lives under `root`, which the final line removes. One kept
        // in the keychain outlives it, and is deleted by name.
        // Refused names are left with a warning: a panic while unwinding aborts the test binary.
        let ours = |service: &&String| {
            let why = pitboard_core::provider::names::refusal(
                service,
                std::slice::from_ref(&self.service),
            );
            if let Some(why) = &why {
                eprintln!("{why}; left as it is");
            }
            why.is_none()
        };
        match os::parked_login() {
            Kept::InKeychain => {
                for service in self.parks_named().iter().filter(ours) {
                    delete_keychain_item(service);
                }
            }
            Kept::InFile | Kept::Sealed => {}
        }
        match os::claude_code_login() {
            Kept::InKeychain => delete_keychain_item(&self.service),
            Kept::InFile => {}
            Kept::Sealed => unreachable!("{SEALS_NOTHING}"),
        }
        // A test that turned Claude Code's Credential Manager store on left its login in this
        // test's slot there, each piece by name.
        #[cfg(windows)]
        if let Err(why) = pitboard_core::testing::credman::sweep(&self.context(), &self.service) {
            eprintln!("{} could not be deleted: {why}", self.service);
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A Codex login the way `codex login` writes one, for `account`: an ID token naming it,
/// signed by nothing, and nothing in Pitboard checks a signature: it reads the claims.
pub(crate) fn codex_login(account: &str, email: &str, refresh: &str) -> serde_json::Value {
    let claims = serde_json::json!({
        "email": email,
        "https://api.openai.com/auth": {
            "chatgpt_account_id": account,
            "chatgpt_user_id": format!("user-{account}"),
            "chatgpt_plan_type": "pro",
        },
    });
    serde_json::json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": pitboard_core::testing::unsigned_token(&claims),
            "access_token": format!("codex-access-{refresh}"),
            "refresh_token": refresh,
            "account_id": account,
        },
        "last_refresh": "2026-09-15T05:05:11Z",
    })
}

/// What a stand-in for `codex login` does once started: store `login` in whichever
/// `CODEX_HOME` it is run with, private to its owner, say so on stderr, and nothing else.
/// Without a `CODEX_HOME` it stores nothing and exits 65.
///
/// It runs only as Pitboard runs Codex's sign-in, `codex -c cli_auth_credentials_store="file"
/// login`, and exits 64 otherwise, so every test that signs in to Codex checks the `-c` that
/// keeps a setting of `/etc/codex` or of a project from sending the new login to the keychain.
fn fake_codex_login(login: &serde_json::Value) -> Script {
    Script::Plays {
        args: Some(vec![
            "-c".into(),
            "cli_auth_credentials_store=\"file\"".into(),
            "login".into(),
        ]),
        steps: vec![
            Step::WritesLogin {
                under: "CODEX_HOME".into(),
                name: "auth.json".into(),
                contents: format!("{login}\n"),
            },
            Step::Warns("Successfully logged in\n".into()),
        ],
    }
}

// As cmd-shim 9.0.2's `lib/index.js` writes it, `\r\n` included (read on 8 October 2026).
fn npm_shim(interpreter: &str, target: &str) -> String {
    [
        "@ECHO off",
        "GOTO start",
        ":find_dp0",
        "SET dp0=%~dp0",
        "EXIT /b",
        ":start",
        "SETLOCAL",
        "CALL :find_dp0",
        "",
        &format!(r#"IF EXIST "%dp0%\{interpreter}.exe" ("#),
        &format!(r#"  SET "_prog=%dp0%\{interpreter}.exe""#),
        ") ELSE (",
        &format!(r#"  SET "_prog={interpreter}""#),
        ")",
        "",
        &format!(
            r#"endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & set PATHEXT=%PATHEXT:;.JS;=;% & "%_prog%"  "%dp0%\{target}" %*"#
        ),
    ]
    .iter()
    .map(|line| format!("{line}\r\n"))
    .collect()
}

/// A credential shaped like Claude Code's, keyed so the fake Anthropic can tell who owns it.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn credential(refresh: &str) -> serde_json::Value {
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

// The two requests that ask Anthropic whose a token is.
const WHOSE: [(&str, &str); 2] = [
    ("POST", "/api/oauth/validate"),
    ("GET", "/api/oauth/profile"),
];
