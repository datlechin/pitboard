//! Nothing Pitboard reads reaches a command a test runs from the environment `cargo test`
//! was started in. `PITBOARD_CLAUDE` exported in the developer's shell would otherwise
//! have every sign-in test run that `claude`, the real one, with the real home, and an
//! `HTTPS_PROXY` exported there would send every request a test makes to that proxy.
//!
//! The variables have to be in the test process's environment from the start, as a shell
//! exporting them would put them, and setting them from a test would change the environment
//! while the harness's other threads may read it. So each test runs this binary again, as a
//! child given them, and the child runs the same test, which then does what it checks.

mod common;

use pitboard_core::testing::stand_in::{Script, Step};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

/// Set in the child's environment, which is how the child knows it is the one to sign in.
const CHILD: &str = "PITBOARD_TEST_INHERITED_CHILD";

/// This test's name, which the child is asked to run and nothing else.
const NAME: &str = "a_sign_in_never_runs_the_program_the_tests_environment_names";

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
#[allow(
    clippy::disallowed_methods,
    reason = "the child is told it is the child through its environment"
)]
fn a_sign_in_never_runs_the_program_the_tests_environment_names() {
    match std::env::var_os(CHILD) {
        Some(_) => signing_in_runs_the_stand_in(),
        None => in_a_child_given_both_variables(),
    }
}

/// A scratch directory, removed however the test ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs this test again in a child whose environment names, for both tools, a program no
/// test may run, which only writes down that it was run.
fn in_a_child_given_both_variables() {
    let scratch =
        Scratch(std::env::temp_dir().join(format!("pitboard-inherited-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&scratch.0);
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let ran = scratch.0.join("ran");
    let trap = scratch.0.join("must-not-run");
    common::put_stand_in(
        &trap,
        &Script::Plays {
            args: None,
            steps: vec![
                Step::Records {
                    at: ran.clone(),
                    variables: Vec::new(),
                },
                Step::Exits(1),
            ],
        },
    );

    let out = Command::new(std::env::current_exe().expect("this test binary"))
        .args([NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .env("PITBOARD_CLAUDE", &trap)
        .env("PITBOARD_CODEX", &trap)
        .output()
        .expect("this test binary runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let trapped = std::fs::read_to_string(&ran).unwrap_or_default();
    assert!(
        trapped.is_empty(),
        "a sign-in ran the program the environment names: {trapped}\n{said}"
    );
    assert!(out.status.success(), "the child failed:\n{said}");
    assert!(said.contains("1 passed"), "the child ran no test:\n{said}");
}

/// A sign-in for each tool runs the test's stand-in, which the harness puts first on `PATH`,
/// and never the program the test process's environment names, which the parent checks.
fn signing_in_runs_the_stand_in() {
    let mut env = common::Env::new("inherited");
    let (b, p) = (env.uuid('b'), env.uuid('p'));
    let (_, err, code) = env.enroll_by_signing_in("work", &b, "b@example.com", &p, "refresh-b");
    assert_eq!(code, 0, "enroll work --sign-in: {err}");
    env.install_fake_codex_login(&common::codex_login(
        &env.uuid('c'),
        "c@example.com",
        "codex-refresh-c",
    ));
    let (_, err, code) = env.run(&["enroll", "codex/work", "--sign-in"]);
    assert_eq!(code, 0, "enroll codex/work --sign-in: {err}");
}

/// The proxy test's name, which its child is asked to run and nothing else.
const PROXIED: &str = "a_command_never_goes_through_the_proxy_the_tests_environment_names";

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
#[allow(
    clippy::disallowed_methods,
    reason = "the child is told it is the child through its environment"
)]
fn a_command_never_goes_through_the_proxy_the_tests_environment_names() {
    match std::env::var_os(CHILD) {
        Some(_) => the_stand_in_is_asked_directly(),
        None => in_a_child_given_every_proxy_variable(),
    }
}

/// Runs this test again in a child whose environment names, in every variable a proxy is
/// read from, a listener here, with a user name and password, and exempts no host from it.
/// The listener only counts: anything that reaches it is a request that went through the
/// proxy the tests were started with.
fn in_a_child_given_every_proxy_variable() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port on loopback");
    listener.set_nonblocking(true).expect("never waits");
    let proxy = format!(
        "http://alice:s3cret@{}",
        listener.local_addr().expect("its address")
    );
    let mut child = Command::new(std::env::current_exe().expect("this test binary"));
    child
        .args([PROXIED, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1");
    for name in [
        "ALL_PROXY",
        "all_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        child.env(name, &proxy);
    }
    // Empty, the first of these is the one read, and it exempts no host, so a developer's
    // own `no_proxy=127.0.0.1` cannot send the requests around the listener.
    let out = child
        .env("NO_PROXY", "")
        .env("no_proxy", "")
        .output()
        .expect("this test binary runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        listener.accept().is_err(),
        "a command went through the proxy the tests' environment names:\n{said}"
    );
    assert!(out.status.success(), "the child failed:\n{said}");
    assert!(said.contains("1 passed"), "the child ran no test:\n{said}");
}

/// Enrolling asks the stand-in for Anthropic who the login is, and `doctor` says requests go
/// out directly: the proxy the parent named is withheld from every command, like everything
/// else Pitboard reads.
fn the_stand_in_is_asked_directly() {
    let mut env = common::Env::new("inherited-proxy");
    let (a, o) = (env.uuid('a'), env.uuid('o'));
    env.sign_in(&a, "a@example.com", &o, "refresh-a");
    let (_, err, code) = env.run(&["enroll", "work"]);
    assert_eq!(code, 0, "enroll work: {err}");

    let (out, err, _) = env.run(&["doctor", "--json"]);
    let value: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}{err}"));
    let network = value["data"]["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["code"] == "network")
        .unwrap_or_else(|| panic!("no network check: {value}"));
    assert_eq!(
        network["detail"], "direct: no variable names a proxy",
        "{network}"
    );
}
