//! Confirms that recovery reads a real journal and a real park item correctly.
//!
//! Every point a switch can be killed is already a unit test over the pure decision in
//! `switch::journal`. What only the real binary can show is that the shell around it —
//! the journal file, the keychain read, the state save — feeds that decision the right
//! facts. So this suite is small on purpose.

mod common;

use common::Env;

fn journal(env: &Env, park_service: &str, incoming_refresh: &str) {
    let journal = serde_json::json!({
        "started_at": 1_789_900_000,
        "from_label": "alpha",
        "from_uuid": env.uuid('a'),
        "to_label": "beta",
        "park_service": park_service,
        "incoming_fingerprint": pitboard::store::fingerprint(incoming_refresh),
    });
    std::fs::write(env.root.join("pitboard/journal.json"), journal.to_string()).unwrap();
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

/// Killed after the outgoing credential was parked but before state recorded it. The park
/// item exists and nothing references it, so without recovery it is invisible to every
/// command and the account falls back to an older copy whose token has since rotated.
#[test]
fn a_park_the_state_never_recorded_is_recovered_on_the_next_run() {
    let env = Env::new("orphan");
    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    env.run(&["enroll", "alpha"]);
    env.sign_in(&env.uuid('b'), "b@example.com", &env.uuid('p'), "refresh-b");
    env.run(&["enroll", "beta"]);
    env.run(&["use", "alpha"]);

    let orphan = format!("pitboard-park-{}-1789900000000", env.uuid('a'));
    common::guard_not_live(&orphan);
    pitboard::store::vault_write(
        &orphan,
        &serde_json::json!({"accessToken": "a", "refreshToken": "refresh-a-rotated"}).to_string(),
    )
    .unwrap();
    journal(&env, &orphan, "refresh-b");
    assert!(!generations_of(&env, "alpha").contains(&orphan));

    let (_, err, code) = env.run(&["use", "beta"]);

    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains("was interrupted"),
        "the user should be told: {err}"
    );
    assert!(
        generations_of(&env, "alpha").contains(&orphan),
        "the orphaned park must be attached back to the account it belongs to"
    );
    assert!(
        !env.root.join("pitboard/journal.json").exists(),
        "a recovered journal must not be replayed again"
    );
    let _ = pitboard::store::vault_delete(&orphan);
}

/// A journal naming a park that was never written leaves nothing to recover, and must not
/// invent a generation pointing at an item that does not exist.
#[test]
fn a_journal_whose_park_was_never_written_changes_nothing() {
    let env = Env::new("nopark");
    env.sign_in(&env.uuid('a'), "a@example.com", &env.uuid('o'), "refresh-a");
    env.run(&["enroll", "alpha"]);
    env.sign_in(&env.uuid('b'), "b@example.com", &env.uuid('p'), "refresh-b");
    env.run(&["enroll", "beta"]);
    env.run(&["use", "alpha"]);
    let before = generations_of(&env, "alpha");

    let never_written = format!("pitboard-park-{}-1789900000001", env.uuid('a'));
    journal(&env, &never_written, "refresh-b");
    env.run(&["use", "beta"]);

    assert!(
        !generations_of(&env, "alpha").contains(&never_written),
        "recovery must not reference a park item that does not exist"
    );
    assert!(generations_of(&env, "alpha").len() >= before.len());
}
