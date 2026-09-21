//! Drives the real binary through complete switches against a synthetic Claude Code
//! installation — its own config directory, keychain slot and state — with a stand-in for
//! Anthropic that answers who each token belongs to.

mod common;

use common::Env;

fn accounts(env: &Env) -> Vec<serde_json::Value> {
    env.state()["accounts"].as_array().unwrap().clone()
}

fn account(env: &Env, label: &str) -> serde_json::Value {
    accounts(env)
        .into_iter()
        .find(|a| a["label"] == label)
        .unwrap_or_else(|| panic!("{label} is not enrolled"))
}

/// alpha signed in and enrolled, beta enrolled by signing in privately.
fn two_accounts(name: &str) -> Env {
    let mut env = Env::new(name);
    let (a, o, b, p) = (env.uuid('a'), env.uuid('o'), env.uuid('b'), env.uuid('p'));
    env.sign_in(&a, "a@example.com", &o, "refresh-a");
    let (_, err, code) = env.run(&["enroll", "alpha"]);
    assert_eq!(code, 0, "enroll alpha: {err}");
    let (_, err, code) = env.enroll_by_signing_in("beta", &b, "b@example.com", &p, "refresh-b");
    assert_eq!(code, 0, "enroll beta: {err}");
    env
}

#[test]
fn enrolling_the_current_account_parks_nothing_while_it_stays_signed_in() {
    let env = two_accounts("current");
    assert!(
        account(&env, "alpha")["generations"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a copy taken while Claude Code keeps rotating the token would go stale"
    );
    assert_eq!(
        env.live()["claudeAiOauth"]["refreshToken"],
        "refresh-a",
        "signing in another account must not touch the live slot"
    );
    assert_eq!(
        account(&env, "beta")["generations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_full_switch_moves_the_identity_and_nothing_else() {
    let env = two_accounts("full");
    let (out, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "switch failed: {err}");
    assert!(out.contains("parked `alpha`"), "{out}");

    let live = env.live();
    assert_eq!(live["claudeAiOauth"]["refreshToken"], "refresh-b");
    assert_eq!(
        live["slackTag"]["machineBound"], true,
        "keys outside claudeAiOauth belong to this machine and must not move"
    );

    let config = env.config();
    assert_eq!(config["oauthAccount"]["accountUuid"], env.uuid('b'));
    assert!(config["oauthAccount"].get("profileFetchedAt").is_none());
    assert!(
        config.get("cachedArtifactRoster").is_none(),
        "a cache derived from the outgoing organisation must be dropped"
    );
    assert_eq!(config["numStartups"], 7, "machine state must survive");

    assert_eq!(env.state()["active"], "beta");
    assert!(
        !account(&env, "alpha")["generations"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the outgoing account is parked at the moment it is replaced"
    );
    assert!(!env.root.join("pitboard/journal.json").exists());
}

#[test]
fn switching_back_and_forth_restores_each_account() {
    let env = two_accounts("backforth");
    for (target, token) in [
        ("beta", "refresh-b"),
        ("alpha", "refresh-a"),
        ("beta", "refresh-b"),
    ] {
        let (_, err, code) = env.run(&["use", target]);
        assert_eq!(code, 0, "use {target}: {err}");
        assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], token);
    }
    let consumed = account(&env, "beta")["generations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| !g["installed_at"].is_null())
        .count();
    assert!(
        consumed >= 2,
        "every copy that was installed must be marked, so it is never offered again"
    );
}

/// The state the caller asked for already holds, so a menu bar clicking "switch to X" while
/// X is active must not report an error.
#[test]
fn switching_to_the_account_already_signed_in_succeeds_and_changes_nothing() {
    let env = two_accounts("already");
    let before = env.state();
    let (out, _, code) = env.run(&["use", "alpha"]);
    assert_eq!(code, 0);
    assert!(out.contains("already signed in"), "{out}");
    assert_eq!(
        env.state(),
        before,
        "no generation should have been created"
    );
}

/// Identity comes from Anthropic, not from Claude Code's config, which can be a day stale.
#[test]
fn a_stale_config_cannot_make_a_switch_file_the_credential_under_the_wrong_account() {
    let env = two_accounts("staleconfig");
    let mut config = env.config();
    config["oauthAccount"]["accountUuid"] = serde_json::json!(env.uuid('b'));
    std::fs::write(env.root.join(".claude.json"), config.to_string()).unwrap();

    let (out, err, code) = env.run(&["use", "beta"]);

    assert_eq!(code, 0, "{err}");
    assert!(
        !out.contains("already signed in"),
        "the config claimed beta, but the live login is alpha's"
    );
    assert!(
        !account(&env, "alpha")["generations"]
            .as_array()
            .unwrap()
            .is_empty(),
        "alpha's login must be parked under alpha"
    );
}

#[test]
fn switching_away_from_an_account_that_is_not_enrolled_is_refused() {
    let mut env = two_accounts("stranger");
    let (c, q) = (env.uuid('c'), env.uuid('q'));
    env.sign_in(&c, "c@example.com", &q, "refresh-c");

    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 1);
    assert!(err.contains("not enrolled"), "{err}");
    assert_eq!(
        env.live()["claudeAiOauth"]["refreshToken"],
        "refresh-c",
        "a refused switch must leave the credential untouched"
    );
}

#[test]
fn a_used_label_cannot_be_taken_by_another_account() {
    let mut env = two_accounts("labelreuse");
    let (c, q) = (env.uuid('c'), env.uuid('q'));
    let (_, err, code) = env.enroll_by_signing_in("beta", &c, "c@example.com", &q, "refresh-c");
    assert_eq!(code, 1);
    assert!(err.contains("already refers to"), "{err}");
    assert_eq!(account(&env, "beta")["email"], "b@example.com");
}

#[test]
fn enrolling_an_account_that_is_already_enrolled_points_at_sign_in() {
    let env = two_accounts("dupe");
    let (_, err, code) = env.run(&["enroll", "another"]);
    assert_eq!(code, 1);
    assert!(err.contains("--sign-in"), "{err}");
}

#[test]
fn forgetting_the_signed_in_account_is_refused() {
    let env = two_accounts("forget");
    let (_, err, code) = env.run(&["forget", "alpha"]);
    assert_eq!(code, 1);
    assert!(err.contains("signed in"), "{err}");
}

#[test]
fn an_account_whose_only_copy_was_already_used_is_refused_not_destroyed() {
    let env = two_accounts("exhausted");
    let path = env.root.join("pitboard/state.json");
    let mut state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for a in state["accounts"].as_array_mut().unwrap() {
        if a["label"] == "beta" {
            for g in a["generations"].as_array_mut().unwrap() {
                g["installed_at"] = serde_json::json!(1_789_935_600);
            }
        }
    }
    std::fs::write(&path, state.to_string()).unwrap();

    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 1);
    assert!(err.contains("no restorable parked login"), "{err}");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a");
}

/// Two simultaneous switches must never interleave. Which outcome the second one sees
/// depends on timing: it either finds beta already signed in, or gives up waiting for the
/// lock and says another run is busy. Both are correct; interleaving is not.
#[test]
fn two_switches_at_once_do_not_interleave() {
    let env = two_accounts("concurrent");
    let spawn = || {
        env.command(&["use", "beta"])
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let (first, second) = (spawn(), spawn());
    let outcomes = [
        first.wait_with_output().unwrap(),
        second.wait_with_output().unwrap(),
    ];

    for out in &outcomes {
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let acceptable = out.status.success() || said.contains("another process is writing");
        assert!(acceptable, "unexpected outcome: {said}");
    }
    assert!(
        outcomes.iter().any(|o| o.status.success()),
        "at least one of them must have switched"
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");
    assert_eq!(
        account(&env, "alpha")["generations"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "exactly one park, from exactly one switch"
    );
}
