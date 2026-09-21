//! Confirms that recovery reads a real journal and a real park item correctly.
//!
//! Every way a switch can be killed is a unit test over the pure decision in
//! `switch::journal`. What only the real binary shows is that the shell around it — the
//! journal file, the parked login, the question put to Anthropic — feeds that decision the
//! right facts.

mod common;

use common::Env;

fn two_accounts(name: &str) -> Env {
    let mut env = Env::new(name);
    let (a, o, b, p) = (env.uuid('a'), env.uuid('o'), env.uuid('b'), env.uuid('p'));
    env.sign_in(&a, "a@example.com", &o, "refresh-a");
    env.run(&["enroll", "alpha"]);
    env.enroll_by_signing_in("beta", &b, "b@example.com", &p, "refresh-b");
    env
}

fn beta_generation(env: &Env) -> String {
    env.state()["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["label"] == "beta")
        .unwrap()["generations"][0]["service"]
        .as_str()
        .unwrap()
        .to_string()
}

fn generations_of(env: &Env, label: &str) -> Vec<String> {
    env.state()["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["label"] == label)
        .unwrap()["generations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["service"].as_str().unwrap().to_string())
        .collect()
}

/// A switch from alpha to beta that died after parking alpha and before installing beta.
fn interrupted_switch(env: &Env) -> String {
    let orphan = format!("pitboard-park-{}-1789900000000", env.uuid('a'));
    env.write_park(
        &orphan,
        &serde_json::json!({"accessToken": "access-refresh-a", "refreshToken": "refresh-a"})
            .to_string(),
    );
    let journal = serde_json::json!({
        "started_at": 1_789_900_000,
        "from_label": "alpha",
        "from_uuid": env.uuid('a'),
        "to_label": "beta",
        "to_uuid": env.uuid('b'),
        "park_service": orphan,
        "incoming_service": beta_generation(env),
    });
    std::fs::write(env.root.join("pitboard/journal.json"), journal.to_string()).unwrap();
    orphan
}

#[test]
fn a_park_the_state_never_recorded_is_recovered_on_the_next_run() {
    let env = two_accounts("orphan");
    let orphan = interrupted_switch(&env);
    assert!(!generations_of(&env, "alpha").contains(&orphan));

    let (_, err, code) = env.run(&["use", "beta"]);

    assert_eq!(code, 0, "{err}");
    assert!(err.contains("had not finished"), "{err}");
    assert!(
        generations_of(&env, "alpha").contains(&orphan),
        "the orphaned park must be attached back to the account it came from"
    );
    assert!(!env.root.join("pitboard/journal.json").exists());
    env.delete_park(&orphan);
}

/// Recovery that cannot tell what happened must change nothing and keep its record, so a
/// later run with a working session can finish the job.
#[test]
fn an_undeterminable_outcome_keeps_the_record_and_changes_nothing() {
    let mut env = two_accounts("undetermined");
    let orphan = interrupted_switch(&env);
    env.expire("access-refresh-a");
    let before = env.state();

    let (_, err, code) = env.run(&["use", "beta"]);

    assert_eq!(code, 1, "{err}");
    assert!(err.contains("cannot yet"), "{err}");
    assert!(
        env.root.join("pitboard/journal.json").exists(),
        "the only record of the interrupted switch must survive"
    );
    assert_eq!(env.state(), before, "nothing may change on a guess");
    env.delete_park(&orphan);
}
