//! Drives the real binary through Codex enrolments and switches, against a synthetic Codex
//! home of the test's own and a stand-in for OpenAI.
//!
//! No real `codex` runs: a sign-in runs a stand-in that writes the login it was given into
//! the private home it is handed, which is all `codex login` leaves behind.

mod common;

use common::{Env, codex_login};

/// The identity a Codex login's own claims give the account `account`: the ChatGPT account
/// with the person inside it, as the harness's logins carry both.
fn codex_id(account: &str) -> String {
    format!("{account}_user-{account}")
}

fn envelope(out: &str) -> serde_json::Value {
    serde_json::from_str(out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}"))
}

fn account(env: &Env, provider: &str, label: &str) -> serde_json::Value {
    env.state()["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["provider"] == provider && a["label"] == label)
        .cloned()
        .unwrap_or_else(|| panic!("{provider}/{label} is not enrolled"))
}

/// `work` signed in to Codex and enrolled, `personal` enrolled by signing in privately.
fn two_codex_accounts(name: &str) -> Env {
    let mut env = Env::new(name);
    env.codex_usage(10.0, 20.0);
    env.sign_in_codex(&env.uuid('w'), "w@example.com", "codex-refresh-w");
    let (_, err, code) = env.run(&["enroll", "codex/work"]);
    assert_eq!(code, 0, "enroll codex/work: {err}");
    env.install_fake_codex_login(&codex_login(
        &env.uuid('p'),
        "p@example.com",
        "codex-refresh-p",
    ));
    let (_, err, code) = env.run(&["enroll", "codex/personal", "--sign-in"]);
    assert_eq!(code, 0, "enroll codex/personal --sign-in: {err}");
    env
}

/// The Codex account signed in now is enrolled from its own login: its ID token names the
/// account, so nobody is asked, and nothing is parked while it stays signed in.
#[test]
fn the_codex_account_in_use_is_enrolled_from_its_own_login() {
    let env = Env::new("codex-current");
    env.sign_in_codex(&env.uuid('w'), "w@example.com", "codex-refresh-w");

    let (out, err, code) = env.run(&["--json", "enroll", "codex/work"]);
    assert_eq!(code, 0, "{err}");
    let data = &envelope(&out)["data"];
    assert_eq!(data["provider"], "codex");
    assert_eq!(data["enrolled"], "current");

    let work = account(&env, "codex", "work");
    assert_eq!(work["account_uuid"], codex_id(&env.uuid('w')));
    assert!(
        work["parked"].is_null(),
        "a copy of a login still in use would be a twin"
    );
    assert_eq!(env.state()["active"]["codex"], "work");
}

/// A second account signs in through Codex's own login, pointed at a private home, so the
/// account in use is never signed out and never revoked. What it signed in to is parked as
/// a Codex account, and the private home is gone afterwards.
#[test]
fn a_second_codex_account_signs_in_privately_and_the_one_in_use_stays() {
    let env = two_codex_accounts("codex-sign-in");

    assert_eq!(
        env.codex_live()["tokens"]["refresh_token"],
        "codex-refresh-w",
        "the login in use is untouched"
    );
    let personal = account(&env, "codex", "personal");
    assert_eq!(personal["account_uuid"], codex_id(&env.uuid('p')));
    let parked = personal["parked"]["service"]
        .as_str()
        .expect("the new login is parked");
    assert!(env.is_parked(parked));
    assert!(
        !env.root.join("pitboard/signin").exists(),
        "the private home a sign-in used is taken away"
    );
}

/// A Codex switch moves one login in and the other out, and says to restart `codex`
/// rather than counting down seconds nothing will ever follow.
#[test]
fn a_codex_switch_moves_the_login_and_says_to_restart_codex() {
    let env = two_codex_accounts("codex-switch");
    let personal_park = account(&env, "codex", "personal")["parked"]["service"]
        .as_str()
        .unwrap()
        .to_string();

    let (out, err, code) = env.run(&["use", "codex/personal"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Restart any running `codex`"), "{out}");

    assert_eq!(
        env.codex_live()["tokens"]["refresh_token"],
        "codex-refresh-p"
    );
    assert_eq!(env.state()["active"]["codex"], "personal");
    assert!(account(&env, "codex", "personal")["parked"].is_null());
    let work_park = account(&env, "codex", "work")["parked"]["service"]
        .as_str()
        .expect("the outgoing login is parked")
        .to_string();
    assert!(env.is_parked(&work_park));
    assert!(
        !env.is_parked(&personal_park),
        "the incoming login's park is gone once it is live: a copy of it would be a twin"
    );

    let (out, err, code) = env.run(&["--json", "use", "codex/work"]);
    assert_eq!(code, 0, "{err}");
    let data = &envelope(&out)["data"];
    assert_eq!(data["provider"], "codex");
    assert_eq!(data["adoption"]["follows"], "restart");
    assert_eq!(data["adoption"]["program"], "codex");
    assert!(
        data["adoption_ceiling_seconds"].is_null(),
        "no number of seconds for a tool that never follows: {data}"
    );
    assert_eq!(
        env.codex_live()["tokens"]["refresh_token"],
        "codex-refresh-w"
    );
}

/// Signing in again to the Codex account in use writes the new login into `auth.json`, the
/// way `codex login` would, and parks nothing. Parked beside the old login, the new one was
/// thrown away by the next switch away, which parked the old one over it.
#[test]
fn signing_in_again_to_the_codex_account_in_use_puts_the_new_login_in_use() {
    let env = two_codex_accounts("codex-again");
    env.install_fake_codex_login(&codex_login(
        &env.uuid('w'),
        "w@example.com",
        "codex-refresh-w2",
    ));

    let (out, err, code) = env.run(&["--json", "enroll", "codex/work", "--sign-in"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(envelope(&out)["data"]["enrolled"], "in_use", "{out}");
    assert_eq!(
        env.codex_live()["tokens"]["refresh_token"],
        "codex-refresh-w2"
    );
    assert!(
        account(&env, "codex", "work")["parked"].is_null(),
        "nothing is parked beside the login in use"
    );

    let (_, err, code) = env.run(&["use", "codex/personal"]);
    assert_eq!(code, 0, "{err}");
    let (_, err, code) = env.run(&["use", "codex/work"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        env.codex_live()["tokens"]["refresh_token"],
        "codex-refresh-w2",
        "the switch away parked the new login, and the switch back put it in use"
    );
}

/// Two tools can each have a `work`. A bare `work` then names two accounts and is refused
/// with both listed; a prefix names exactly one.
#[test]
fn the_same_label_on_both_tools_is_two_accounts() {
    let mut env = Env::new("codex-same-label");
    env.codex_usage(10.0, 20.0);
    let (a, o) = (env.uuid('a'), env.uuid('o'));
    env.sign_in(&a, "a@example.com", &o, "refresh-a");
    let (_, err, code) = env.run(&["enroll", "work"]);
    assert_eq!(code, 0, "enroll work: {err}");
    env.sign_in_codex(&env.uuid('w'), "w@example.com", "codex-refresh-w");
    let (_, err, code) = env.run(&["enroll", "codex/work"]);
    assert_eq!(code, 0, "enroll codex/work: {err}");

    assert_eq!(account(&env, "claude", "work")["account_uuid"], a);
    assert_eq!(
        account(&env, "codex", "work")["account_uuid"],
        codex_id(&env.uuid('w'))
    );

    let (out, _, code) = env.run(&["--json", "use", "work"]);
    assert_ne!(code, 0);
    let error = &envelope(&out)["error"];
    assert_eq!(error["code"], "label_ambiguous", "{error}");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("claude/work") && message.contains("codex/work"),
        "{message}"
    );

    let (out, err, code) = env.run(&["use", "codex/work"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("already signed in"), "{out}");
}

/// A sign-in for a tool that is not installed is refused before any browser opens, naming
/// the program it looked for.
///
/// `PATH` holds nothing but an empty directory, so no `codex` can be found, whatever this
/// machine has installed: the real one must never run under a test.
#[test]
fn a_codex_sign_in_without_codex_says_so_first() {
    let env = Env::new("codex-missing");
    let empty = env.root.join("nothing-on-path");
    std::fs::create_dir_all(&empty).unwrap();
    let out = env
        .command(&["--json", "enroll", "codex/personal", "--sign-in"])
        .env("PATH", &empty)
        .output()
        .expect("pitboard runs");
    assert!(!out.status.success());
    let error = &envelope(&String::from_utf8_lossy(&out.stdout))["error"];
    assert_eq!(error["code"], "codex_program_missing", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Codex"),
        "{error}"
    );
    assert!(
        !env.root.join("pitboard/signin").exists(),
        "nothing was started, so there is nothing to clean up"
    );
}

/// A Codex installed with npm is a script that starts through `env`, which finds its
/// interpreter on `PATH`, and npm puts that interpreter beside the program under a prefix an
/// app opened from Finder does not have on its `PATH`. Named by the path it was found at,
/// with a search path lacking its directory, the sign-in still starts and hands back the
/// login it stored.
///
/// Driven through the library, the way the app drives it: the command line looks for a
/// program on its own `PATH`, where its directory always is.
#[test]
fn an_npm_installed_codex_signs_in_from_a_path_without_its_directory() {
    let env = Env::new("codex-npm");
    let program = env.install_fake_npm_codex_login(&codex_login(
        &env.uuid('p'),
        "p@example.com",
        "codex-refresh-p",
    ));
    let search = "/usr/bin:/bin";
    let alone = std::process::Command::new(&program)
        .arg("login")
        .env_clear()
        .env("PATH", search)
        .output()
        .expect("env runs");
    assert_eq!(
        alone.status.code(),
        Some(127),
        "the stand-in cannot start from that PATH on its own, so the test would prove nothing"
    );

    let ctx = pitboard_core::context::Context::new(env.root.clone())
        .with_pitboard_home(env.root.join("pitboard"))
        .with_claude_config_dir(env.root.to_string_lossy().into_owned())
        .with_codex_home(env.codex_home().to_string_lossy().into_owned())
        .with_codex_program(program)
        .with_search_path(search.into());
    let pitboard = pitboard_core::service::Pitboard::new(ctx);
    let signing_in = pitboard
        .sign_in_watched("codex/personal")
        .unwrap_or_else(|e| panic!("the sign-in did not start: {e}"));
    let said = signing_in.said();
    let login = signing_in
        .finish()
        .unwrap_or_else(|e| panic!("the sign-in did not finish: {e}"));
    assert_eq!(login.provider(), pitboard_core::provider::ProviderId::Codex);
    let heard: String = std::iter::from_fn(|| said.next()).collect();
    assert!(heard.contains("Successfully logged in"), "{heard}");
    assert!(
        !env.codex_home().join("auth.json").exists(),
        "it signed in to the private home and nowhere else"
    );
}
