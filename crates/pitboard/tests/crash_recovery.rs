//! Confirms that recovery reads a real journal and a real park item correctly.
//!
//! Every way a switch can be killed is a unit test over the pure decision in
//! `switch::journal`. What only the real binary shows is that the shell around it (the
//! journal file, the parked login, the question put to Anthropic) feeds that decision the
//! right facts.

mod common;

use common::{Env, two_accounts};

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
        "incoming_service": env.parked_service("beta").unwrap(),
    });
    std::fs::write(env.root.join("pitboard/journal.json"), journal.to_string()).unwrap();
    orphan
}

/// alpha is still signed in, so the orphan is a second copy of a login Claude Code keeps
/// rotating: it must go, not be kept as alpha's way back.
#[test]
fn a_park_of_a_login_still_signed_in_is_dropped_on_the_next_run() {
    let env = two_accounts("orphan");
    let orphan = interrupted_switch(&env);

    let (_, err, code) = env.run(&["use", "beta"]);

    assert_eq!(code, 0, "{err}");
    assert!(err.contains("had not finished"), "{err}");
    assert!(!env.is_parked(&orphan), "the stale copy must be deleted");
    assert_ne!(
        env.parked_service("alpha").as_deref(),
        Some(orphan.as_str())
    );
    assert!(!env.root.join("pitboard/journal.json").exists());
}

/// Killed after the install: beta is live, and the orphan is now alpha's only copy.
#[test]
fn a_park_the_state_never_recorded_is_kept_once_its_switch_landed() {
    let mut env = two_accounts("landed");
    let orphan = interrupted_switch(&env);
    let beta_park = env.parked_service("beta").unwrap();
    let (b, p) = (env.uuid('b'), env.uuid('p'));
    env.sign_in(&b, "b@example.com", &p, "refresh-b");

    let (out, err, code) = env.run(&["status", "--json"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("\"signed_in\":true"),
        "status settles nothing: {out}"
    );

    let (_, err, code) = env.run(&["use", "alpha"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("had in fact finished"), "{err}");
    assert!(
        !env.is_parked(&beta_park),
        "beta's installed copy is consumed"
    );
    assert_eq!(
        env.live()["claudeAiOauth"]["refreshToken"],
        "refresh-a",
        "the recovered orphan is what alpha switches back to"
    );
    assert!(!env.is_parked(&orphan));
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

/// `use beta` killed after installing beta's login and before recording it: the state still
/// says alpha. Forgetting beta must first learn that beta is the one signed in.
#[test]
fn forgetting_waits_for_an_interrupted_switch_to_be_settled() {
    let mut env = two_accounts("forget-pending");
    let orphan = interrupted_switch(&env);
    let (b, p) = (env.uuid('b'), env.uuid('p'));
    env.sign_in(&b, "b@example.com", &p, "refresh-b");

    let (out, err, code) = env.run(&["forget", "beta", "--json"]);
    let envelope: serde_json::Value = serde_json::from_str(&out).expect(&err);

    assert_eq!(code, 1, "{out}");
    assert_eq!(envelope["error"]["code"], "cannot_forget_active_account");
    assert_eq!(
        envelope["warnings"][0]["code"], "interrupted_switch_finished",
        "what recovery found belongs in the envelope, even when the command then fails"
    );
    assert_eq!(env.state()["active"]["claude"], "beta");
    assert!(!env.root.join("pitboard/journal.json").exists());
    env.delete_park(&orphan);
}

#[test]
fn a_damaged_record_is_refused_rather_than_guessed_at() {
    let env = two_accounts("damaged");
    std::fs::write(
        env.root.join("pitboard/journal.json"),
        "{\"started_at\": 17",
    )
    .unwrap();
    let before = env.state();

    let (out, _, code) = env.run(&["use", "beta", "--json"]);
    let envelope: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(code, 3);
    assert_eq!(envelope["error"]["code"], "recovery_record_corrupt");
    assert!(env.root.join("pitboard/journal.json").exists());
    assert_eq!(env.state(), before, "nothing may change on a guess");
}

/// Recovery has to ask Anthropic who owns the live login, so offline it cannot finish, and
/// every command that changes anything stops at that. Giving up is the way out, and it must
/// delete nothing: which copy is live is exactly what is unknown.
#[test]
fn giving_up_on_an_unfinishable_switch_keeps_every_login() {
    let mut env = two_accounts("abandon");
    let orphan = interrupted_switch(&env);
    assert!(env.is_parked(&orphan), "the interrupted run parked a copy");

    // Anthropic no longer recognises the session, so the live owner cannot be learned.
    env.expire("access-refresh-a");
    let (_, err, code) = env.run(&["use", "beta"]);
    assert_ne!(
        code, 0,
        "a switch cannot proceed over an unfinished one: {err}"
    );

    let (out, err, code) = env.run(&["abandon"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Gave up on the interrupted switch"), "{out}");
    assert!(env.is_parked(&orphan), "nothing was deleted");
    assert!(
        !env.root.join("pitboard").join("journal.json").exists(),
        "the record is gone, so the next command is not blocked by it"
    );

    let (_, err, _) = env.run(&["doctor"]);
    assert!(
        !err.contains("interrupted"),
        "doctor is no longer blocked: {err}"
    );
}
