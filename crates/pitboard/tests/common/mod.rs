//! The harness shared by the integration tests: a synthetic Claude Code installation, a
//! stand-in for Anthropic, and the guard that keeps every test away from a real login.
//!
//! Every service name a test writes is derived from that test's own name, which Rust
//! already forbids two tests in a module from sharing.

// Compiled separately into every test binary that declares `mod common`, so a binary that
// does not use one of these helpers would otherwise warn about it.
#![allow(dead_code)]

pub mod os;

use os::Kept;
use pitboard_core::context::Environment;
use pitboard_core::testing::fs as files;
use pitboard_core::testing::stand_in::{self, Script, Step};

/// Put a program at `path` that plays `script`: the compiled stand-in, the same on every
/// system, which `pitboard_core::testing::stand_in` says how to write a script for. A test
/// run alone, which builds no example, is told how to build it.
pub fn put_stand_in(path: &Path, script: &Script) {
    stand_in::install(path, script)
        .unwrap_or_else(|e| panic!("no stand-in at {}: {e}", path.display()));
}

/// The variables Pitboard reads that no test takes from whoever runs it, from the core's own
/// list of what it reads, so a variable added there is withheld here too.
// An inherited `PITBOARD_CLAUDE` would have a sign-in run the real `claude` on the real home.
pub fn withheld() -> impl Iterator<Item = &'static str> {
    pitboard_core::testing::variables().filter(|name| !os::passed_on().contains(name))
}

/// This test process's own environment, less what is withheld: the machine's real keychain
/// account, and the slot Claude Code reads by default.
#[allow(
    clippy::disallowed_methods,
    reason = "the tests' own environment, from which this withholds what Pitboard reads"
)]
pub fn ctx() -> pitboard_core::context::Context {
    let passed: pitboard_core::context::Environment = std::env::vars_os()
        .filter(|(name, _)| !withheld().any(|kept| is(name, kept)))
        .collect();
    pitboard_core::context::Context::for_command_line(&passed)
}

fn is(name: &std::ffi::OsStr, variable: &str) -> bool {
    name.to_str()
        .is_some_and(|name| os::same_variable(name, variable))
}

fn started_in() -> pitboard_core::context::Context {
    pitboard_core::context::Context::for_command_line(&Environment::of_this_process())
}

pub fn real_slots() -> [String; 2] {
    [
        pitboard_core::testing::LIVE_SERVICE.to_owned(),
        pitboard_core::testing::live_service(&started_in()),
    ]
}

pub fn refuse_a_real_login_place(scratch: &Path) {
    let places = pitboard_core::testing::real_login_places(&started_in())
        .unwrap_or_else(|why| panic!("{why}, so no test can tell its folder from a real login's"));
    if let Some(place) = places.iter().find(|place| clashes(scratch, place)) {
        panic!(
            "a test may not use {} as its own folder: it is, holds or lies in {}",
            scratch.display(),
            place.display()
        );
    }
}

fn clashes(scratch: &Path, place: &Path) -> bool {
    let same = pitboard_core::host::same_path_in_any_case;
    let (scratch, place) = (resolved(scratch), resolved(place));
    scratch.ancestors().any(|folder| same(folder, &place))
        || place.ancestors().any(|folder| same(folder, &scratch))
}

fn resolved(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(found) = std::fs::canonicalize(existing) {
            return rest
                .iter()
                .rev()
                .fold(found, |found, name| found.join(name));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name);
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

#[test]
fn a_folder_that_is_holds_or_lies_in_a_login_place_clashes() {
    let root = std::env::temp_dir().join(format!("pitboard-clash-{}", std::process::id()));
    let place = root.join("home").join(".claude");
    std::fs::create_dir_all(&place).unwrap();
    for scratch in [
        place.clone(),
        place.join("inside"),
        root.join("home"),
        root.join("home").join(".CLAUDE").join("inside"),
    ] {
        assert!(clashes(&scratch, &place), "{}", scratch.display());
    }
    for scratch in [root.join("home").join(".claudex"), root.join("elsewhere")] {
        assert!(!clashes(&scratch, &place), "{}", scratch.display());
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
#[cfg_attr(windows, ignore = "W15: links a test makes on Windows")]
fn a_link_into_a_login_place_clashes() {
    let root = std::env::temp_dir().join(format!("pitboard-clash-link-{}", std::process::id()));
    let place = root.join(".codex");
    std::fs::create_dir_all(&place).unwrap();
    files::link_dir(&place, &root.join("through")).unwrap();
    assert!(clashes(&root.join("through").join("inside"), &place));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn every_real_login_place_is_refused() {
    let places = pitboard_core::testing::real_login_places(&started_in()).unwrap();
    assert!(!places.is_empty());
    for place in places {
        let inside = place.join("pitboard-guard");
        let caught = std::panic::catch_unwind(|| refuse_a_real_login_place(&inside));
        assert!(caught.is_err(), "{} must be refused", inside.display());
    }
}

// By pattern, never by hashing real homes: how Codex spells its home on Windows is unmeasured.
pub fn guard_not_live(name: &str) {
    if let Some(why) = refusal(name, None) {
        panic!("{why}");
    }
}

fn refusal(name: &str, own: Option<&str>) -> Option<String> {
    let family = pitboard_core::provider::names::family(name)?;
    (!own.is_some_and(|own| spells(name, own))).then(|| {
        format!(
            "a test must never address {name:?}, a {} name a real login may be kept under",
            family.as_str()
        )
    })
}

fn spells(name: &str, own: &str) -> bool {
    let whole = match name.rsplit_once('#') {
        Some((whole, piece))
            if piece == "m"
                || piece == "p"
                || (!piece.is_empty() && piece.bytes().all(|b| b.is_ascii_digit())) =>
        {
            whole
        }
        _ => name,
    };
    let slot =
        whole.split_once('/').map_or(
            whole,
            |(slot, account)| if account.contains('/') { whole } else { slot },
        );
    slot.eq_ignore_ascii_case(own)
}

#[test]
fn the_guard_refuses_every_name_a_real_login_may_be_kept_under() {
    let mut refused: Vec<String> = [
        "Claude Code-credentials",
        "Claude Code-credentials/claude-code-user",
        "Claude Code-credentials/claude-code-user#0",
        "Claude Code-credentials/claude-code-user#12",
        "Claude Code-credentials/claude-code-user#m",
        "Claude Code-credentials/claude-code-user#p",
        "Claude Code-credentials-e80beed8",
        "Claude Code-credentials-e80beed8/claude-code-user#3",
        "Claude Code-credentials-deadbeef",
        "claude code-credentials/CLAUDE-CODE-USER",
        "LegacyGeneric:target=Claude Code-credentials/claude-code-user",
        "Claude Code-staging-credentials",
        "cli|1a2b3c4d5e6f7a8b",
        "CLI|1A2B3C4D5E6F7A8B",
        "secrets|1a2b3c4d5e6f7a8b",
        "Codex MCP Credentials",
        "Codex MCP Credentials/linear",
        "linear|3f2a.Codex MCP Credentials",
    ]
    .map(str::to_owned)
    .into();
    for slot in real_slots() {
        for account in ["claude-code-user", &account()] {
            refused.push(format!("{slot}/{account}"));
            for piece in ["#0", "#1", "#m", "#p"] {
                refused.push(format!("{slot}/{account}{piece}"));
            }
        }
        refused.push(slot);
    }
    for name in &refused {
        let caught = std::panic::catch_unwind(|| guard_not_live(name));
        assert!(caught.is_err(), "{name} must be refused");
    }
    for name in [
        "pitboard-citest-1",
        "pitboard-park-0a1b2c3d-1",
        "GitHub - https://github.com",
    ] {
        guard_not_live(name);
    }
}

#[test]
fn a_test_may_address_its_own_slot_alone() {
    let env = Env::new("own-slot");
    let own = env.service.clone();
    for spelling in [
        own.clone(),
        own.to_lowercase(),
        format!("{own}/claude-code-user"),
        format!("{own}/{}", account()),
        format!("{own}/claude-code-user#0"),
        format!("{own}/claude-code-user#7"),
        format!("{own}/claude-code-user#m"),
        format!("{own}/claude-code-user#p"),
        format!("{own}#0"),
    ] {
        env.guard(&spelling);
    }
    let other = pitboard_core::testing::service_for_dir(&env.root.join("other").to_string_lossy());
    for name in [
        other.clone(),
        format!("{other}/claude-code-user#0"),
        format!("{own}/claude-code-user/more"),
        format!("{own}#x"),
        format!("{own}0"),
    ] {
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| env.guard(&name)));
        assert!(caught.is_err(), "{name} must be refused");
    }
}

/// The harness stands in for the credential store on two platforms, and each thing it does
/// there is written twice. `delete_park`'s second half was missing: on Linux it deleted
/// nothing and said nothing, so a test that took a login away still had it, and the test
/// went on to watch the opposite of what it meant to. Nothing asserted the harness itself
/// did what it said, so this does, on whichever platform it is running.
#[test]
#[cfg_attr(windows, ignore = "W20: parking logins on Windows")]
fn the_harness_can_park_a_login_find_it_and_take_it_away() {
    let env = Env::new("harness-round-trip");
    let service = format!(
        "pitboard-park-{}-1",
        pitboard_core::testing::dir_hash("harness")
    );

    assert!(!env.is_parked(&service), "nothing is parked to begin with");
    env.write_park(&service, r#"{"claudeAiOauth":{"refreshToken":"r"}}"#);
    assert!(env.is_parked(&service), "what was written is found");
    env.delete_park(&service);
    assert!(!env.is_parked(&service), "what was deleted is gone");
    // Twice: a test may delete a park that is already gone, and that is not an error.
    env.delete_park(&service);
}

/// No test may read a login the person running it is actually using.
///
/// The harness gives Claude Code a scratch config directory and a keychain slot hashed from
/// it, and `guard_not_live` refuses every other slot. Codex needed the same and did not have
/// it: `status` reads every tool's live login, so the suite quietly started reading the
/// developer's own signed-in Codex account and putting it in a snapshot.
#[test]
fn every_tool_is_pointed_at_a_scratch_home() {
    let env = Env::new("isolation");
    let command = env.command(&["status"]);
    let named: Vec<(String, String)> = command
        .get_envs()
        .filter_map(|(k, v)| {
            Some((
                k.to_string_lossy().into_owned(),
                v?.to_string_lossy().into_owned(),
            ))
        })
        .collect();
    let folders = os::account_folders(&env.root);
    let homes = ["CLAUDE_CONFIG_DIR", "CODEX_HOME", "PITBOARD_HOME"]
        .into_iter()
        .chain(folders.iter().map(|(name, _)| *name));
    for (_, folder) in &folders {
        assert!(folder.is_dir(), "{} is not made", folder.display());
    }
    for home in homes {
        let set = named.iter().find(|(k, _)| k == home).unwrap_or_else(|| {
            panic!("{home} is not pointed anywhere, so a test reads a real one")
        });
        assert!(
            std::path::Path::new(&set.1).starts_with(&env.root),
            "{home} is {} , which is outside this test's own directory",
            set.1
        );
    }
    // Set at all, even empty, this pins Claude Code's default slot whatever
    // CLAUDE_CONFIG_DIR says, which is the real login. It has to be taken away, not left to
    // whatever the person running the tests exported.
    let removed: Vec<String> = command
        .get_envs()
        .filter(|(_, v)| v.is_none())
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    assert!(
        removed
            .iter()
            .any(|k| k == "CLAUDE_SECURESTORAGE_CONFIG_DIR"),
        "CLAUDE_SECURESTORAGE_CONFIG_DIR is inherited, so a test can read the real slot"
    );
    // Nor is anything else Pitboard reads: each is taken away or set here.
    let given: Vec<String> = command
        .get_envs()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    for name in withheld() {
        assert!(
            given.iter().any(|k| os::same_variable(k, name)),
            "{name} is inherited from whoever runs the tests"
        );
    }
}

use std::path::{Path, PathBuf};
use std::process::Command;

const SECURITY: &str = "/usr/bin/security";

pub struct Env {
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

#[allow(
    clippy::disallowed_methods,
    reason = "who runs the tests, whose keychain account a command is given as USER"
)]
pub fn account() -> String {
    std::env::var("USER").unwrap_or_else(|_| "claude-code-user".into())
}

impl Env {
    pub fn new(name: &str) -> Env {
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
    pub fn expect_usage_requests(&mut self, expected: usize) {
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
    pub fn assert_usage_requests(&self) {
        self.usage.assert();
    }

    // std's `Command` folds case on Windows: given `PATH` then `Path`, a child had one, `Path`.
    pub fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_pitboard"));
        for name in withheld() {
            c.env_remove(name);
        }
        c.args(args).envs(self.given());
        c
    }

    // Without these the suite reads whatever login the person running it is signed in to.
    fn given(&self) -> Vec<(&'static str, std::ffi::OsString)> {
        let mut given = vec![
            ("CLAUDE_CONFIG_DIR", self.root.clone().into_os_string()),
            ("CODEX_HOME", self.codex_home().into_os_string()),
            ("PITBOARD_HOME", self.root.join("pitboard").into_os_string()),
            ("PITBOARD_API_BASE", self.server.url().into()),
            ("PATH", os::search_path(&self.root.join("bin"))),
        ];
        given.extend(
            os::account_folders(&self.root)
                .into_iter()
                .map(|(name, folder)| (name, folder.into_os_string())),
        );
        given
    }

    // The runners' `Path` (run 37673429917) stays beside the given `PATH` until W14.
    #[allow(
        clippy::disallowed_methods,
        reason = "the tests' own environment, from which this withholds what Pitboard reads"
    )]
    pub fn context(&self) -> pitboard_core::context::Context {
        let kept = std::env::vars_os().filter(|(name, _)| !withheld().any(|kept| is(name, kept)));
        let given = self
            .given()
            .into_iter()
            .map(|(name, value)| (std::ffi::OsString::from(name), value));
        let env: Environment = kept.chain(given).collect();
        pitboard_core::context::Context::for_command_line(&env)
    }

    pub fn guard(&self, name: &str) {
        if let Some(why) = refusal(name, Some(&self.service)) {
            panic!("{why}");
        }
    }

    /// This test's own `CODEX_HOME`, created because Codex requires the directory to exist.
    pub fn codex_home(&self) -> PathBuf {
        let dir = self.root.join("codex");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    pub fn run(&self, args: &[&str]) -> (String, String, i32) {
        self.run_with(args, &[])
    }

    /// `run`, with `vars` set besides, over what the harness sets or withholds.
    pub fn run_with(&self, args: &[&str], vars: &[(&str, &str)]) -> (String, String, i32) {
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
    pub fn api_address(&self) -> String {
        self.server.host_with_port()
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

    /// Stand in for `claude auth login`: stores `credential` where Claude Code keeps the
    /// login of whichever `CLAUDE_CONFIG_DIR` it is run with, the private directory Pitboard
    /// makes for a sign-in, and does nothing else. Started any other way, it refuses.
    pub fn install_fake_claude(&self, credential: &str) {
        self.install_fake_claude_waiting(credential, std::time::Duration::ZERO);
    }

    /// `install_fake_claude`, whose sign-in waits `waiting` before it stores the login, as
    /// a person takes a while in the browser.
    pub fn install_fake_claude_waiting(&self, credential: &str, waiting: std::time::Duration) {
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
        let waits = u64::try_from(waiting.as_millis()).expect("a wait a test can make");
        put_stand_in(
            &bin.join(os::program("claude")),
            // Talks on stdout like the real one, and can be made to wait like a person does.
            &Script::Plays {
                args: Some(vec!["auth".into(), "login".into()]),
                steps: vec![
                    Step::Says("Opening browser to sign in\n".into()),
                    Step::Waits(waits),
                    store,
                ],
            },
        );
    }

    /// Park a login where the binary under test will look for it: the keychain where
    /// Pitboard parks there, and otherwise this test's own Pitboard home, never the
    /// machine's.
    pub fn write_park(&self, service: &str, contents: &str) {
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

    pub fn is_parked(&self, service: &str) -> bool {
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
    pub fn delete_park(&self, service: &str) {
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
    pub fn sign_out(&self) {
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

    /// Anthropic answers, badly. The switch that cannot identify the signed-in account
    /// must say so in a way a program can act on, rather than as one code and a sentence.
    pub fn profile_trouble(&mut self, status: usize) {
        let mock = self
            .server
            .mock("GET", "/api/oauth/profile")
            .with_status(status)
            .with_body(r#"{"type":"error","error":{"type":"api_error"}}"#)
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

    /// Sign Codex in to `account` the way `codex login` leaves it: an `auth.json` in this
    /// test's own `CODEX_HOME`, mode 0600, whose ID token names the account. The token is
    /// signed by nothing, and nothing in Pitboard checks a signature: it reads the claims.
    pub fn sign_in_codex(&self, account: &str, email: &str, refresh: &str) {
        let login = codex_login(account, email, refresh);
        os::write_private(&self.codex_home().join("auth.json"), &login.to_string());
    }

    /// Stand in for `codex login`: signs `login` into whichever `CODEX_HOME` it is run with,
    /// the private directory Pitboard makes for a sign-in, and does nothing else. Every
    /// test that runs a Codex sign-in installs this first, because the harness keeps the
    /// real `PATH` behind its own `bin`, and the real `codex login` must never run here.
    pub fn install_fake_codex_login(&self, login: &serde_json::Value) {
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let program = bin.join(os::program("codex"));
        let _ = std::fs::remove_file(&program);
        put_stand_in(&program, &fake_codex_login(login));
    }

    /// `install_fake_codex_login` laid out the way npm installs Codex, in a prefix of this
    /// test's own that no `PATH` has. Returns the program as found.
    pub fn install_fake_npm_codex_login(&self, login: &serde_json::Value) -> PathBuf {
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
                let program = prefix.join("codex.cmd");
                write(&program, &npm_shim("fakenode", &target.join("\\")));
                program
            }
        }
    }

    /// The live Codex login, as this test's `CODEX_HOME` holds it.
    pub fn codex_live(&self) -> serde_json::Value {
        let raw = std::fs::read_to_string(self.codex_home().join("auth.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    /// Sign Codex in with an API key rather than an account, the way `codex login
    /// --with-api-key` leaves it. The key is made up.
    pub fn sign_in_codex_with_an_api_key(&self) {
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
    pub fn install_fake_codex(&self, version: &str) {
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
    pub fn codex_usage(&mut self, five_hour: f64, weekly: f64) {
        let window = |percent: f64, seconds: i64, reset_at: i64| {
            serde_json::json!({
                "used_percent": percent,
                "limit_window_seconds": seconds,
                "reset_after_seconds": 3600,
                "reset_at": reset_at,
            })
        };
        let mock = self
            .server
            .mock("GET", "/wham/usage")
            .with_status(200)
            .with_body(
                serde_json::json!({
                    "plan_type": "pro",
                    "rate_limit": {
                        "allowed": true,
                        "limit_reached": false,
                        "primary_window": window(five_hour, 18_000, 1_789_990_000),
                        "secondary_window": window(weekly, 604_800, 1_790_500_000),
                    },
                })
                .to_string(),
            )
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
    pub fn replace_live(&self, credential: &serde_json::Value) {
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

    pub fn live(&self) -> serde_json::Value {
        let raw = match os::claude_code_login() {
            Kept::InKeychain => pitboard_core::testing::vault_read(&self.context(), &self.service)
                .unwrap()
                .unwrap(),
            Kept::InFile => std::fs::read_to_string(self.live_path()).unwrap(),
            Kept::Sealed => unreachable!("{SEALS_NOTHING}"),
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

    /// The label of `tool`'s account whose login Pitboard last recorded `tool`'s store
    /// holding.
    pub fn in_use(&self, tool: &str) -> Option<String> {
        let state: pitboard_core::state::State = serde_json::from_value(self.state()).unwrap();
        let which = pitboard_core::provider::ProviderId::parse(tool).unwrap();
        state
            .account_in_use(which)
            .map(|account| account.label.clone())
    }

    /// What Claude Code's `label` is filed under: its parked logins, its readings and a
    /// switch's record name it by this.
    pub fn account_id(&self, label: &str) -> String {
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
    pub fn edit_state(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let mut state = self.state();
        edit(&mut state);
        std::fs::write(self.root.join("pitboard/state.json"), state.to_string()).unwrap();
    }

    /// What Pitboard has measured of each Claude Code account's five-hour and weekly limits,
    /// by label, written where every front end records what it measured. Each limit resets
    /// an hour and a day from now.
    pub fn measured(&self, shares: &[(&str, f64, f64)]) {
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
                    "account_uuid": id,
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

    /// Every account was last put in use an hour ago. Enrolling the account signed in puts
    /// it in use, and Pitboard switches nothing away from an account for five minutes after.
    pub fn an_hour_on(&self) {
        self.edit_state(|state| {
            for account in state["accounts"].as_array_mut().expect("accounts") {
                if let Some(at) = account["last_used_at"].as_i64() {
                    account["last_used_at"] = serde_json::json!(at - 3600);
                }
            }
        });
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
            let parked = v["accounts"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| &a["parked"]["service"])
                .chain(v["discarded"].as_array().into_iter().flatten());
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
            let why = refusal(service, Some(&self.service));
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
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A Codex login the way `codex login` writes one, for `account`: an ID token naming it,
/// signed by nothing, and nothing in Pitboard checks a signature: it reads the claims.
pub fn codex_login(account: &str, email: &str, refresh: &str) -> serde_json::Value {
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
            "id_token": format!(
                "{}.{}.{}",
                base64url(br#"{"alg":"RS256"}"#),
                base64url(claims.to_string().as_bytes()),
                base64url(b"not a real signature"),
            ),
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

/// Unpadded base64url, which is how every part of a JWT is written.
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut held = 0u32;
        for (at, byte) in chunk.iter().enumerate() {
            held |= u32::from(*byte) << (16 - 8 * at);
        }
        for at in 0..(chunk.len() * 8).div_ceil(6) {
            out.push(char::from(
                ALPHABET[((held >> (18 - 6 * at)) & 0x3f) as usize],
            ));
        }
    }
    out
}

/// A credential shaped like Claude Code's, keyed so the fake Anthropic can tell who owns it.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

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
