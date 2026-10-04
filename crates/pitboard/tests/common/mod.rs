//! The harness shared by the integration tests: a synthetic Claude Code installation, a
//! stand-in for Anthropic, and the guard that keeps every test away from a real login.
//!
//! Every service name a test writes is derived from that test's own name, which Rust
//! already forbids two tests in a module from sharing.

// Compiled separately into every test binary that declares `mod common`, so a binary that
// does not use one of these helpers would otherwise warn about it.
#![allow(dead_code)]

/// A Claude Desktop data folder and Pitboard's record of its accounts, made by hand.
#[cfg(target_os = "macos")]
pub mod desktop;

/// Write a program a test then runs, and make it runnable.
///
/// A separate process writes it, so this process never holds the file open for writing.
/// Tests run on threads, and on Linux a thread that starts a process while another holds
/// such a descriptor gives the child a copy of it; running the file then fails with
/// ETXTBSY, "Text file busy", until that child has started its own program. CI met it on
/// Linux (run 36457687750): the test that ran a fake `codex` it had just written.
pub fn write_program(path: &Path, contents: &str) {
    let status = std::process::Command::new("/bin/sh")
        .args(["-c", "printf %s \"$2\" > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(path)
        .arg(contents)
        .status()
        .expect("sh runs");
    assert!(status.success(), "could not write {}", path.display());
}

/// What every command a test runs is given of the environment the tests run in: who is
/// running them, which names their keychain account, and their home, which each tool's own
/// variable moves away from in [`Env::command`]. Every other variable Pitboard reads is
/// withheld. One exported in the shell that ran `cargo test`, such as `PITBOARD_CLAUDE`,
/// would otherwise have a sign-in run the real `claude` with the real home.
const PASSED_ON: [&str; 2] = ["HOME", "USER"];

/// The variables Pitboard reads that no test takes from whoever runs it, from the core's own
/// list of what it reads, so a variable added there is withheld here too.
pub fn withheld() -> impl Iterator<Item = &'static str> {
    pitboard_core::testing::variables().filter(|name| !PASSED_ON.contains(name))
}

/// This test process's own environment, less what is withheld: the machine's real keychain
/// account, and the slot Claude Code reads by default.
#[allow(
    clippy::disallowed_methods,
    reason = "the tests' own environment, from which this withholds what Pitboard reads"
)]
pub fn ctx() -> pitboard_core::context::Context {
    let passed: pitboard_core::context::Environment = std::env::vars_os()
        .filter(|(name, _)| !withheld().any(|kept| name == kept))
        .collect();
    pitboard_core::context::Context::for_command_line(&passed)
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
        service, "Claude Safe Storage",
        "a test must never address the key Claude Desktop encrypts its cookies with"
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
    let caught = std::panic::catch_unwind(|| guard_not_live("Claude Safe Storage"));
    assert!(
        caught.is_err(),
        "Claude Desktop's cookie key must be refused"
    );
}

/// The home directory the passwd database gives this user, read without looking at `HOME`.
///
/// A test may point `HOME` anywhere, and a guard that trusted it would wave a write into the
/// real `~/Library/Application Support/Claude` through as soon as one did. The shell's
/// `~name` asks the passwd database, as `getpwnam` does, and `id -un` names this user id.
pub fn passwd_home() -> PathBuf {
    let out = Command::new("/bin/sh")
        .args(["-c", "eval \"printf %s ~$(/usr/bin/id -un)\""])
        .env_remove("HOME")
        .output()
        .expect("sh runs");
    assert!(
        out.status.success(),
        "could not read this user's passwd entry"
    );
    let home = PathBuf::from(String::from_utf8(out.stdout).expect("a home that is UTF-8"));
    assert!(
        home.is_absolute(),
        "the passwd home `{}` is not absolute",
        home.display()
    );
    home
}

/// `path` as somewhere from the root, with `.` and `..` taken out by reading it rather than
/// by asking the filesystem, which a path that does not exist yet cannot answer.
fn lexically_absolute(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).expect("a path that can be made absolute");
    let mut out = PathBuf::new();
    for part in absolute.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// `path` with every symlink in the part of it that exists followed, and the rest kept as
/// written.
fn resolved(path: &Path) -> PathBuf {
    let path = lexically_absolute(path);
    let mut rest = Vec::new();
    let mut here = path.as_path();
    loop {
        if let Ok(real) = here.canonicalize() {
            return rest.iter().rev().fold(real, |at, part| at.join(part));
        }
        match (here.parent(), here.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                here = parent;
            }
            _ => return path,
        }
    }
}

/// Refuse a directory or file that holds a real login, or is inside one: Claude Desktop's
/// data folder, Pitboard's own home, Claude Code's and Codex's.
///
/// Every test that writes into a Claude Desktop data folder calls this first. The folders
/// are named from the passwd home, never from `HOME`, and compared both as written and with
/// symlinks followed, so neither `..` nor a link reaches one.
pub fn guard_not_live_dir(path: &Path) {
    let home = passwd_home();
    let live = [
        home.join("Library/Application Support/Claude"),
        home.join(".pitboard"),
        home.join(".claude"),
        home.join(".claude.json"),
        home.join(".codex"),
    ];
    for candidate in [lexically_absolute(path), resolved(path)] {
        for root in &live {
            for root in [root.clone(), resolved(root)] {
                assert!(
                    !candidate.starts_with(&root),
                    "a test must never write to {}, which is inside {}",
                    path.display(),
                    root.display()
                );
            }
        }
    }
}

#[test]
fn the_directory_guard_refuses_every_real_login_and_nothing_else() {
    let home = passwd_home();
    let refused = [
        home.join("Library/Application Support/Claude"),
        home.join("Library/Application Support/Claude/Cookies"),
        home.join("Library/Application Support/Claude/Local Storage/leveldb"),
        home.join("Library/Application Support/Claude/./config.json"),
        home.join("Library/Application Support/Other/../Claude"),
        home.join(".pitboard"),
        home.join(".pitboard/desktop/parks"),
        home.join(".claude"),
        home.join(".claude.json"),
        home.join(".codex/auth.json"),
    ];
    for path in refused {
        let caught = std::panic::catch_unwind(|| guard_not_live_dir(&path));
        assert!(caught.is_err(), "{} must be refused", path.display());
    }

    let env = Env::new("dir-guard");
    guard_not_live_dir(&env.root.join("claude-desktop"));
    guard_not_live_dir(&env.root.join("claude-desktop/Cookies"));
    guard_not_live_dir(&home.join("Library/Application Support/Claude-Other"));
    guard_not_live_dir(&home.join(".pitboard-scratch"));

    // A link from a scratch directory into the real data folder is the real data folder.
    let link = env.root.join("linked");
    std::os::unix::fs::symlink(home.join("Library/Application Support"), &link)
        .expect("a symlink in this test's own directory");
    let caught = std::panic::catch_unwind(|| guard_not_live_dir(&link.join("Claude/Cookies")));
    let reaches = home.join("Library/Application Support").exists();
    assert_eq!(
        caught.is_err(),
        reaches,
        "a link into the real data folder must be refused wherever that folder exists"
    );
}

/// The passwd home is this user's, whatever `HOME` says.
#[test]
fn the_passwd_home_does_not_follow_home() {
    let out = Command::new("/bin/sh")
        .args(["-c", "eval \"printf %s ~$(/usr/bin/id -un)\""])
        .env("HOME", "/nowhere/at/all")
        .output()
        .expect("sh runs");
    assert_eq!(
        PathBuf::from(String::from_utf8(out.stdout).unwrap()),
        passwd_home()
    );
    assert_ne!(passwd_home(), Path::new("/nowhere/at/all"));
}

/// The harness stands in for the credential store on two platforms, and each thing it does
/// there is written twice. `delete_park`'s second half was missing: on Linux it deleted
/// nothing and said nothing, so a test that took a login away still had it, and the test
/// went on to watch the opposite of what it meant to. Nothing asserted the harness itself
/// did what it said, so this does, on whichever platform it is running.
#[test]
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
/// it, and `guard_not_live` refuses the two names that could be real. Codex needed the same
/// and did not have it: `status` reads every tool's live login, so the suite quietly started
/// reading the developer's own signed-in Codex account and putting it in a snapshot.
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
    for home in [
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "PITBOARD_HOME",
        "PITBOARD_CLAUDE_DESKTOP_DIR",
        "PITBOARD_CLAUDE_DESKTOP_APP",
    ] {
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
            given.iter().any(|k| k == name),
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

    #[allow(
        clippy::disallowed_methods,
        reason = "the tests' own PATH, behind the scratch programs a command is given first"
    )]
    pub fn command(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut c = Command::new(env!("CARGO_BIN_EXE_pitboard"));
        for name in withheld() {
            c.env_remove(name);
        }
        c.args(args)
            .env("CLAUDE_CONFIG_DIR", &self.root)
            // Every tool Pitboard reads gets a scratch home of its own, empty unless a
            // test puts something in it. Without this the suite reads whatever the person
            // running it happens to be signed in to, which is both a flaky test and a real
            // login no test may touch.
            .env("CODEX_HOME", self.codex_home())
            // Claude Desktop's data folder and app, neither of which exists until a test
            // makes one, so no command can find or quit the person's own Claude.
            .env(
                "PITBOARD_CLAUDE_DESKTOP_DIR",
                self.root.join("claude-desktop"),
            )
            .env("PITBOARD_CLAUDE_DESKTOP_APP", self.root.join("Claude.app"))
            .env("PITBOARD_HOME", self.root.join("pitboard"))
            .env("PITBOARD_API_BASE", self.server.url())
            .env("PATH", path);
        c
    }

    /// This test's own `CODEX_HOME`, created because Codex requires the directory to exist.
    pub fn codex_home(&self) -> PathBuf {
        let dir = self.root.join("codex");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    pub fn run(&self, args: &[&str]) -> (String, String, i32) {
        let out = self.command(args).output().expect("run Pitboard");
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
            // 0600, and by the same route Claude Code takes: write, then chmod. A shim
            // that leaves the umask to decide writes a login anybody can read, which
            // `doctor` is right to fail on and which Claude Code does not do.
            format!(
                r#"printf %s '{credential}' > "$CLAUDE_CONFIG_DIR/.credentials.json"
chmod 600 "$CLAUDE_CONFIG_DIR/.credentials.json""#
            )
        };
        write_program(
            &bin.join("claude"),
            // Talks on stdout like the real one, and can be made to wait like a person does.
            &format!(
                "#!/bin/sh\n[ \"$1 $2\" = \"auth login\" ] || exit 64\n\
                 echo 'Opening browser to sign in'\nsleep \"${{FAKE_SIGN_IN_SECONDS:-0}}\"\n{store}\n"
            ),
        );
    }

    /// Park a login where the binary under test will look for it. On macOS that is the
    /// keychain; elsewhere it is this test's own Pitboard home, never the machine's.
    pub fn write_park(&self, service: &str, contents: &str) {
        guard_not_live(service);
        if cfg!(target_os = "macos") {
            pitboard_core::testing::vault_write(&ctx(), service, contents).unwrap();
        } else {
            // The modes Pitboard's own file vault writes, for the same reason: a parked
            // login is a plaintext token and `doctor` fails on one anybody can read.
            use std::os::unix::fs::PermissionsExt;
            let vault = self.root.join("pitboard/vault");
            std::fs::create_dir_all(&vault).unwrap();
            std::fs::set_permissions(&vault, std::fs::Permissions::from_mode(0o700)).unwrap();
            let path = vault.join(format!("{service}.json"));
            std::fs::write(&path, contents).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
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

    /// Take a parked login out of the store behind Pitboard's back, which is how a test
    /// says "this one is gone" without Pitboard's own records agreeing.
    ///
    /// The Linux half was missing, so on Linux this deleted nothing and every test that
    /// used it went on with the park still there. `use` then gave it back, which is
    /// `repair`'s own behaviour and correct, so the test that meant to see a refusal saw a
    /// switch. Never caught, because CI's Linux leg was being cancelled by a lint failure
    /// before it got this far.
    pub fn delete_park(&self, service: &str) {
        if cfg!(target_os = "macos") {
            let _ = pitboard_core::testing::vault_delete(&ctx(), service);
        } else {
            let _ = std::fs::remove_file(self.root.join(format!("pitboard/vault/{service}.json")));
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
        use std::os::unix::fs::PermissionsExt;
        let path = self.codex_home().join("auth.json");
        std::fs::write(&path, login.to_string()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    /// Stand in for `codex login`: signs `login` into whichever `CODEX_HOME` it is run with,
    /// the private directory Pitboard makes for a sign-in, and does nothing else. Every
    /// test that runs a Codex sign-in installs this first, because the harness keeps the
    /// real `PATH` behind its own `bin`, and the real `codex login` must never run here.
    pub fn install_fake_codex_login(&self, login: &serde_json::Value) {
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("codex");
        let _ = std::fs::remove_file(&script);
        write_program(&script, &format!("#!/bin/sh\n{}", fake_codex_login(login)));
    }

    /// `install_fake_codex_login` laid out the way npm installs Codex, in a prefix of this
    /// test's own that no `PATH` has. Returns `<prefix>/bin/codex`, the program as found.
    ///
    /// npm links `<prefix>/bin/codex` to a script inside `lib/node_modules` whose first line
    /// is `#!/usr/bin/env node`, and puts the `node` that installed it in `<prefix>/bin`.
    /// Here that is `fakenode`, which runs the script with `sh`, so a real `node` on this
    /// machine can never be the one found: the script starts only where `PATH` has its
    /// prefix's `bin`.
    pub fn install_fake_npm_codex_login(&self, login: &serde_json::Value) -> PathBuf {
        let prefix = self.root.join("npm");
        let bin = prefix.join("bin");
        let package = prefix.join("lib/node_modules/@openai/codex/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&package).unwrap();
        write_program(&bin.join("fakenode"), "#!/bin/sh\nexec /bin/sh \"$@\"\n");
        write_program(
            &package.join("codex.js"),
            &format!("#!/usr/bin/env fakenode\n{}", fake_codex_login(login)),
        );
        let program = bin.join("codex");
        let _ = std::fs::remove_file(&program);
        std::os::unix::fs::symlink("../lib/node_modules/@openai/codex/bin/codex.js", &program)
            .unwrap();
        program
    }

    /// The live Codex login, as this test's `CODEX_HOME` holds it.
    pub fn codex_live(&self) -> serde_json::Value {
        let raw = std::fs::read_to_string(self.codex_home().join("auth.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    /// Sign Codex in with an API key rather than an account, the way `codex login
    /// --with-api-key` leaves it. The key is made up.
    pub fn sign_in_codex_with_an_api_key(&self) {
        use std::os::unix::fs::PermissionsExt;
        let login = serde_json::json!({
            "auth_mode": "apikey",
            "OPENAI_API_KEY": "sk-not-a-real-key",
        });
        let path = self.codex_home().join("auth.json");
        std::fs::write(&path, login.to_string()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    /// Put a `codex` of this test's own first on `PATH`, laid out the way Codex's standalone
    /// installer lays one out, so whatever reads which Codex is installed reads this one.
    ///
    /// The `PATH` a test runs with ends with the real one, and the real standalone install
    /// lives inside the developer's own `~/.codex`, which no test may go near. Running it
    /// fails loudly: nothing that only asks which version it is ever runs it.
    pub fn install_fake_codex(&self, version: &str) {
        let installed = self
            .root
            .join("codex-install/releases")
            .join(format!("{version}-test-target"))
            .join("bin/codex");
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        write_program(
            &installed,
            "#!/bin/sh\necho 'a test stand-in for codex, not meant to run' >&2\nexit 64\n",
        );
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let link = bin.join("codex");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&installed, &link).unwrap();
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

    /// Where this platform's Claude Code keeps the live credential: the keychain slot on
    /// macOS, and `.credentials.json` in the config directory everywhere else.
    fn live_path(&self) -> PathBuf {
        self.root.join(".credentials.json")
    }

    /// Replaces the live credential document, for a test that needs it to be a particular
    /// shape rather than whatever a sign-in produced.
    ///
    /// Written the way Claude Code writes a large one, through `security`'s argument line,
    /// because Pitboard itself refuses to and that refusal is what some of these tests are
    /// about. The item is this test's own, guarded like every other write here.
    pub fn replace_live(&self, credential: &serde_json::Value) {
        let body = credential.to_string();
        if !cfg!(target_os = "macos") {
            self.write_live(&body);
            return;
        }
        guard_not_live(&self.service);
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
        if cfg!(target_os = "macos") {
            pitboard_core::testing::vault_write(&ctx(), &self.service, credential).unwrap();
        } else {
            // 0600, because that is what Claude Code writes: it chmods the plaintext
            // credential after writing it, and a stand-in that leaves the umask to decide
            // is a stand-in for something else. `doctor` reads these modes and fails on a
            // login anybody can read, which is how this was found.
            use std::os::unix::fs::PermissionsExt;
            let path = self.live_path();
            std::fs::write(&path, credential).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
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

/// What a stand-in for `codex login` does once started, whatever line starts it: store
/// `login` in whichever `CODEX_HOME` it is run with, and nothing else.
fn fake_codex_login(login: &serde_json::Value) -> String {
    format!(
        "[ \"$1\" = login ] || exit 64\n\
         [ -n \"$CODEX_HOME\" ] || exit 65\n\
         cat > \"$CODEX_HOME/auth.json\" <<'LOGIN'\n{login}\nLOGIN\n\
         chmod 600 \"$CODEX_HOME/auth.json\"\n\
         echo 'Successfully logged in' >&2\n"
    )
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
