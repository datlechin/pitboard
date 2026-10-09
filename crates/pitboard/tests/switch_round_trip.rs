//! Drives the real binary through complete switches against a synthetic Claude Code
//! installation, with its own config directory, keychain slot and state, and a stand-in for
//! Anthropic that answers who each token belongs to.

mod common;

use common::{Env, two_accounts};

fn accounts(env: &Env) -> Vec<serde_json::Value> {
    env.state()["accounts"].as_array().unwrap().clone()
}

fn account(env: &Env, label: &str) -> serde_json::Value {
    accounts(env)
        .into_iter()
        .find(|a| a["label"] == label)
        .unwrap_or_else(|| panic!("{label} is not enrolled"))
}

fn envelope(out: &str) -> serde_json::Value {
    serde_json::from_str(out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}"))
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn enrolling_the_current_account_parks_nothing_while_it_stays_signed_in() {
    let env = two_accounts("current");
    assert!(
        env.parked_service("alpha").is_none(),
        "a copy taken while Claude Code keeps rotating the token would go stale"
    );
    assert_eq!(
        env.live()["claudeAiOauth"]["refreshToken"],
        "refresh-a",
        "signing in another account must not touch the live slot"
    );
    let beta = env.parked_service("beta").expect("beta is parked");
    assert!(env.is_parked(&beta));
    assert!(
        account(&env, "beta")["parked"]["refresh_expires_at"].is_i64(),
        "when a parked login stops working is recorded"
    );
}

/// A switch writes its park's name down before it writes the login into it, and records the
/// park in `state.json` once it has. The name stays on the list until the next change
/// resolves it, and doctor counted it as one it could not read.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn doctor_finds_no_park_outstanding_after_a_switch() {
    let env = two_accounts("pending-after-switch");
    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "switch failed: {err}");

    let (out, err, _) = env.run(&["doctor", "--json"]);
    let value = serde_json::from_str::<serde_json::Value>(&out)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {out}{err}"));
    let pending = value["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["code"] == "pending_parks")
        .expect("doctor checks the parks being reclaimed")
        .clone();
    assert_eq!(pending["level"], "ok", "{pending}");
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn a_full_switch_moves_the_identity_and_nothing_else() {
    let env = two_accounts("full");
    let beta_park = env.parked_service("beta").unwrap();
    let (out, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "switch failed: {err}");
    assert!(out.contains("Switched to beta; alpha is parked"), "{out}");

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

    assert_eq!(env.in_use("claude").as_deref(), Some("beta"));
    assert!(
        env.parked_service("alpha").is_some(),
        "the outgoing account is parked at the moment it is replaced"
    );
    assert!(
        env.parked_service("beta").is_none() && !env.is_parked(&beta_park),
        "the installed copy is Claude Code's now; keeping it would only offer a stale token"
    );
    assert_eq!(env.state()["discarded"], serde_json::json!([]));
    assert!(!env.root.join("pitboard/journal.json").exists());
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn switching_back_and_forth_restores_each_account_and_leaves_no_copies_behind() {
    let env = two_accounts("backforth");
    let mut seen = Vec::new();
    for (target, token) in [
        ("beta", "refresh-b"),
        ("alpha", "refresh-a"),
        ("beta", "refresh-b"),
    ] {
        seen.extend(env.parked_service(target));
        let (_, err, code) = env.run(&["use", target]);
        assert_eq!(code, 0, "use {target}: {err}");
        assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], token);
        assert!(env.parked_service(target).is_none());
    }
    assert!(
        seen.iter().all(|s| !env.is_parked(s)),
        "every copy that was installed is deleted, never offered again"
    );
}

/// The state the caller asked for already holds, so a menu bar clicking "switch to X" while
/// X is active must not report an error.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn switching_to_the_account_already_signed_in_succeeds_and_changes_nothing() {
    let env = two_accounts("already");
    let before = env.state();
    let (out, _, code) = env.run(&["use", "alpha"]);
    assert_eq!(code, 0);
    assert!(out.contains("already signed in"), "{out}");
    assert_eq!(env.state(), before, "nothing should have been parked");
}

/// Identity comes from Anthropic, not from Claude Code's config, which can be a day stale.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
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
        env.parked_service("alpha").is_some(),
        "alpha's login must be parked under alpha"
    );
}

/// Claude Code's config can name another account than the one whose login it has stored, as
/// a Claude Code process started on another login leaves it. Using the account in use writes
/// that account there again and moves no login.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn using_the_account_in_use_writes_it_into_claude_codes_config_again() {
    let env = two_accounts("namesagain");
    let names_beta = || {
        let mut config = env.config();
        config["oauthAccount"]["accountUuid"] = serde_json::json!(env.uuid('b'));
        config["oauthAccount"]["organizationUuid"] = serde_json::json!(env.uuid('p'));
        std::fs::write(env.root.join(".claude.json"), config.to_string()).unwrap();
    };

    names_beta();
    let (out, err, code) = env.run(&["use", "alpha"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("alpha is already signed in. Claude Code's config named another account"),
        "{out}"
    );
    assert_eq!(env.config()["oauthAccount"]["accountUuid"], env.uuid('a'));
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a");

    names_beta();
    let (out, err, code) = env.run(&["use", "alpha", "--json"]);
    assert_eq!(code, 0, "{err}");
    let data = &envelope(&out)["data"];
    assert_eq!(
        (&data["changed"], &data["config_updated"]),
        (&false.into(), &true.into())
    );
    let (out, _, _) = env.run(&["use", "alpha", "--json"]);
    assert_eq!(envelope(&out)["data"]["config_updated"], false);
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
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
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn a_used_label_cannot_be_taken_by_another_account() {
    let mut env = two_accounts("labelreuse");
    let (c, q) = (env.uuid('c'), env.uuid('q'));
    let (_, err, code) = env.enroll_by_signing_in("beta", &c, "c@example.com", &q, "refresh-c");
    assert_eq!(code, 1);
    assert!(err.contains("already refers to"), "{err}");
    assert_eq!(account(&env, "beta")["email"], "b@example.com");
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn enrolling_an_account_under_a_second_label_points_at_sign_in() {
    let env = two_accounts("dupe");
    let (_, err, code) = env.run(&["enroll", "another"]);
    assert_eq!(code, 1);
    assert!(err.contains("--sign-in"), "{err}");
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn enrolling_the_signed_in_account_again_is_not_an_error() {
    let env = two_accounts("again");
    let before = env.state();
    let (_, err, code) = env.run(&["enroll", "alpha"]);
    assert_eq!(code, 0, "{err}");

    // Nothing moves but when the account was last used, which naming the account you are
    // signed in as is: it is how Pitboard can tell an account nobody has come back to.
    let forget_when = |accounts: &serde_json::Value| {
        let mut accounts = accounts.clone();
        for a in accounts.as_array_mut().expect("accounts") {
            a["last_used_at"] = serde_json::Value::Null;
        }
        accounts
    };
    assert_eq!(
        forget_when(&env.state()["accounts"]),
        forget_when(&before["accounts"])
    );
    assert!(
        env.state()["accounts"][0]["last_used_at"].as_i64()
            >= before["accounts"][0]["last_used_at"].as_i64(),
        "and that only ever moves forward"
    );
}

/// The way back for an account whose parked login was used or expired, and what every
/// message about one says to run.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn signing_in_to_an_enrolled_account_again_renews_its_parked_login() {
    let mut env = two_accounts("renew");
    let old = env.parked_service("beta").unwrap();
    let (b, p) = (env.uuid('b'), env.uuid('p'));

    let (out, err, code) = env.enroll_by_signing_in("beta", &b, "b@example.com", &p, "refresh-b2");

    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Renewed beta"), "{out}");
    let new = env.parked_service("beta").unwrap();
    assert_ne!(new, old);
    assert!(!env.is_parked(&old), "the login it replaces is deleted");
    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b2");
}

/// Signing in again to the account in use, whose login is broken or about to lapse, puts
/// the new login in use and parks nothing, so the next switch away parks the new login and
/// not the one it replaced.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn signing_in_again_to_the_account_in_use_puts_the_new_login_in_use() {
    let mut env = two_accounts("again-in-use");
    let (a, o) = (env.uuid('a'), env.uuid('o'));

    let (out, err, code) = env.enroll_by_signing_in("alpha", &a, "a@example.com", &o, "refresh-a2");

    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("Signed in to alpha") && out.contains("in use now"),
        "{out}"
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a2");
    assert!(env.parked_service("alpha").is_none(), "nothing is parked");
    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "{err}");
    let (_, err, code) = env.run(&["use", "alpha"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a2");
}

/// A label typed wrong at enroll time is fixed without signing in again, whichever account
/// it names.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn renaming_keeps_the_login_and_the_new_label_switches() {
    let env = two_accounts("rename");
    let beta_park = env.parked_service("beta").unwrap();

    for (from, to) in [("alpha", "personal"), ("beta", "work")] {
        let (out, err, code) = env.run(&["rename", from, to]);
        assert_eq!(code, 0, "{err}");
        assert!(out.contains(&format!("Renamed {from} to {to}")), "{out}");
    }
    assert_eq!(env.in_use("claude").as_deref(), Some("personal"));
    assert_eq!(env.parked_service("work"), Some(beta_park.clone()));
    assert!(env.is_parked(&beta_park), "a rename deletes nothing");

    let (_, err, code) = env.run(&["use", "work"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");

    let (_, err, code) = env.run(&["rename", "personal", "work"]);
    assert_eq!(code, 1);
    assert!(err.contains("already refers to"), "{err}");
}

/// A program reading `--json` gets exactly one JSON line, whatever Claude Code's sign-in
/// prints along the way.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn signing_in_keeps_the_json_output_pure() {
    let mut env = two_accounts("pure");
    let (c, q) = (env.uuid('c'), env.uuid('q'));
    let credential = common::credential("refresh-c").to_string();
    env.install_fake_claude(&credential);
    env.owns("access-refresh-c", &c, "c@example.com", &q);

    let (out, err, code) = env.run(&["enroll", "side", "--sign-in", "--json"]);

    assert_eq!(code, 0, "{err}");
    assert_eq!(envelope(&out)["data"]["enrolled"], "signed_in");
    assert!(
        err.contains("Opening browser"),
        "Claude Code's words go to stderr: {err}"
    );
}

/// A sign-in waits on a person in a browser. Nothing else may wait on it.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn a_sign_in_in_progress_does_not_hold_up_a_switch() {
    let mut env = two_accounts("waiting");
    let (c, q) = (env.uuid('c'), env.uuid('q'));
    let credential = common::credential("refresh-c").to_string();
    env.install_fake_claude_waiting(&credential, std::time::Duration::from_secs(4));
    env.owns("access-refresh-c", &c, "c@example.com", &q);
    let signing_in = env
        .command(&["enroll", "side", "--sign-in"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    let started = std::time::Instant::now();
    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "the switch waited {:?} on a browser",
        started.elapsed()
    );

    let (_, err, code) = env.run(&["enroll", "other", "--sign-in"]);
    assert_eq!(code, 1);
    assert!(err.contains("already waiting"), "{err}");

    let finished = signing_in.wait_with_output().unwrap();
    assert!(
        finished.status.success(),
        "{}",
        String::from_utf8_lossy(&finished.stderr)
    );
    assert!(env.parked_service("side").is_some());
}

fn access_lapsed(env: &Env, label: &str) {
    env.edit_state(|s| {
        for a in s["accounts"].as_array_mut().unwrap() {
            if a["label"] == label {
                a["parked"]["access_expires_at"] = serde_json::json!(1_000);
            }
        }
    });
}

/// A parked login is Pitboard's alone, so status renews it once its access lapses, stores
/// it the way Claude Code would, and a later switch installs the renewed login.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn status_renews_a_parked_login_whose_access_has_lapsed() {
    let mut env = two_accounts("renewal");
    let old = env.parked_service("beta").unwrap();
    access_lapsed(&env, "beta");
    let renewal = env.answers_renewal(
        "refresh-b",
        200,
        serde_json::json!({
            "access_token": "access-refresh-b2", "refresh_token": "refresh-b2",
            "expires_in": 28_800, "refresh_token_expires_in": 2_592_000,
            "scope": "user:inference user:profile", "token_type": "Bearer"
        }),
    );
    // The renewed token has to be answerable too: a switch asks who the login going in
    // belongs to before it takes the live one away.
    let beta_uuid = env.uuid('b');
    env.owns(
        "access-refresh-b2",
        &beta_uuid,
        "beta@example.com",
        &env.uuid('p'),
    );

    let (out, err, code) = env.run(&["status", "--json"]);

    assert_eq!(code, 0, "{err}");
    let status = envelope(&out);
    assert_eq!(status["warnings"], serde_json::json!([]));
    let beta = status["data"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["label"] == "beta")
        .unwrap()
        .clone();
    assert_eq!(
        beta["usage"]["source"], "live",
        "asked with the renewed token"
    );
    assert!(beta["parked"]["access_expires_at"].as_i64().unwrap() > 1_000);
    let new = env.parked_service("beta").unwrap();
    assert_ne!(new, old);
    assert!(
        !env.is_parked(&old),
        "the copy holding the spent token is deleted"
    );
    renewal.assert();

    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "{err}");
    let live = env.live();
    assert_eq!(live["claudeAiOauth"]["refreshToken"], "refresh-b2");
    assert_eq!(
        live["claudeAiOauth"]["scopes"],
        serde_json::json!(["user:inference", "user:profile"])
    );
    assert_eq!(live["claudeAiOauth"]["subscriptionType"], "max");
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn a_parked_login_anthropic_refuses_is_dropped_with_the_way_back() {
    let mut env = two_accounts("refused");
    let old = env.parked_service("beta").unwrap();
    access_lapsed(&env, "beta");
    let renewal = env.answers_renewal(
        "refresh-b",
        400,
        serde_json::json!({"error": "invalid_grant", "error_description": "refresh token revoked"}),
    );

    let (out, err, code) = env.run(&["status", "--json"]);

    assert_eq!(code, 0, "{err}");
    let status = envelope(&out);
    assert_eq!(status["warnings"][0]["code"], "parked_login_refused");
    assert!(
        status["warnings"][0]["message"]
            .as_str()
            .unwrap()
            .contains("pitboard enroll beta --sign-in")
    );
    assert!(env.parked_service("beta").is_none());
    assert!(!env.is_parked(&old));
    renewal.assert();
}

/// `pitboard renew` says what it did in the sentence the app's Renew Now shows: nothing due
/// reads as the good answer it is, and one renewal as one.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn renew_says_what_it_did_as_the_app_says_it() {
    let mut env = two_accounts("renew-words");
    let (out, err, code) = env.run(&["renew"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "No parked login was due.\n");

    access_lapsed(&env, "beta");
    let renewal = env.answers_renewal(
        "refresh-b",
        200,
        serde_json::json!({
            "access_token": "access-refresh-b2", "refresh_token": "refresh-b2",
            "expires_in": 28_800, "refresh_token_expires_in": 2_592_000,
            "scope": "user:inference user:profile", "token_type": "Bearer"
        }),
    );
    let (out, err, code) = env.run(&["renew"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "Renewed one.\n");
    renewal.assert();
}

/// Every file under `dir`, by path, with what it holds.
fn tree(dir: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut found = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(dir).expect("a directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            found.extend(tree(&path));
        } else {
            let body = std::fs::read(&path).expect("a file");
            found.insert(path, body);
        }
    }
    found
}

/// The schedule's job runs `pitboard renew --scheduled` on Linux, where Pitboard does not
/// rely on what systemd passes a job, and such a run renews the default home,
/// `~/.pitboard`, whatever `PITBOARD_HOME` says: a folder elsewhere, empty, or relative,
/// which every other command refuses. The job is given a `HOME` of the test's own, so its
/// default home is a folder of the test's, and is run from the test's own folder, so a
/// relative one followed would show. The same run without the marker renews the home
/// `PITBOARD_HOME` names, as it did.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn a_scheduled_renewal_renews_the_default_home_whatever_pitboard_home_says() {
    let mut env = two_accounts("renew-scheduled");
    access_lapsed(&env, "beta");
    let renewal = env.answers_renewal(
        "refresh-b",
        200,
        serde_json::json!({
            "access_token": "access-refresh-b2", "refresh_token": "refresh-b2",
            "expires_in": 28_800, "refresh_token_expires_in": 2_592_000,
            "scope": "user:inference user:profile", "token_type": "Bearer"
        }),
    );
    let home = env.root.join("home");
    std::fs::create_dir_all(&home).expect("a home of the test's own");
    let state = env.state();

    let elsewhere = env.root.join("pitboard");
    for pitboard_home in [elsewhere.as_os_str(), "".as_ref(), "relative".as_ref()] {
        let at = format!("PITBOARD_HOME={pitboard_home:?}");
        let _ = std::fs::remove_dir_all(home.join(".pitboard"));
        let out = env
            .command(&["renew", "--scheduled", "--json"])
            .env("HOME", &home)
            .env("PITBOARD_HOME", pitboard_home)
            .current_dir(&env.root)
            .output()
            .expect("run Pitboard");
        let ran = envelope(&String::from_utf8_lossy(&out.stdout));
        assert_eq!(out.status.code(), Some(0), "{at}: {ran}");
        assert_eq!(ran["command"], "renew", "{at}");
        assert_eq!(ran["data"]["renewed"], 0, "{at}: {ran}");
        assert!(
            home.join(".pitboard").is_dir(),
            "{at}: the run was the default home's"
        );
    }
    assert!(
        !renewal.matched(),
        "the parked login PITBOARD_HOME holds is not asked about"
    );
    assert_eq!(env.state(), state, "nor changed");
    assert!(
        !env.root.join("relative").exists(),
        "nor a relative one made"
    );

    let (out, err, code) = env.run(&["renew"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "Renewed one.\n");
    renewal.assert();
}

/// Under sudo, `renew` renewed a parked login as root and wrote what came back as root,
/// where the person's own runs might not replace it again. It is refused now, with the code
/// a program branches on, having asked nobody and written nothing, the audit log included.
/// Without sudo, the same run renews.
// `SUDO_UID` is Unix's sign of sudo; Windows' `sudo` elevates the token (`windows_refuses`).
#[cfg(unix)]
#[test]
fn renew_under_sudo_is_refused_and_changes_nothing() {
    let mut env = two_accounts("renew-sudo");
    access_lapsed(&env, "beta");
    let renewal = env.answers_renewal(
        "refresh-b",
        200,
        serde_json::json!({
            "access_token": "access-refresh-b2", "refresh_token": "refresh-b2",
            "expires_in": 28_800, "refresh_token_expires_in": 2_592_000,
            "scope": "user:inference user:profile", "token_type": "Bearer"
        }),
    );
    let before = tree(&env.root);

    let out = env
        .command(&["renew", "--json"])
        .env("SUDO_UID", "501")
        .output()
        .expect("run Pitboard");

    assert_eq!(out.status.code(), Some(1));
    let refused = envelope(&String::from_utf8_lossy(&out.stdout));
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["command"], "renew");
    assert_eq!(refused["error"]["code"], "elevated");
    assert_eq!(
        refused["error"]["message"],
        "Pitboard changes nothing when it runs as root or with sudo. Run it as yourself."
    );
    assert!(tree(&env.root) == before, "nothing is written");
    assert!(!renewal.matched(), "nobody is asked");

    let (out, err, code) = env.run(&["renew"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "Renewed one.\n");
    renewal.assert();
}

/// Under sudo, `enroll --sign-in` is refused before it says it is opening the tool's
/// sign-in, as `forget` and `uninstall` ask no question first: no sign-in opens, so none is
/// announced, and nothing is written.
// `SUDO_UID` is Unix's sign of sudo; Windows' `sudo` elevates the token (`windows_refuses`).
#[cfg(unix)]
#[test]
fn enrolling_by_sign_in_under_sudo_announces_no_sign_in() {
    let env = two_accounts("enroll-sudo");
    // First on the PATH, so a sign-in the gate let through would start this stand-in and
    // never the person's own `claude`.
    env.install_fake_claude(&common::credential("refresh-c").to_string());
    let before = tree(&env.root);

    let out = env
        .command(&["enroll", "side", "--sign-in"])
        .env("SUDO_UID", "501")
        .output()
        .expect("run Pitboard");

    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("sign-in. Sign in as"), "{err}");
    assert!(
        err.contains("Pitboard changes nothing when it runs as root or with sudo."),
        "{err}"
    );
    assert!(tree(&env.root) == before, "nothing is written");
}

/// With `HOME` empty or relative, every command but the two that print a generated file is
/// refused in its own envelope with `home_not_absolute`, before it reads or writes anything,
/// and `doctor` fails its `homes` check and checks nothing else. Pitboard took the home to
/// be under the folder it ran in, and `status` read each tool's default folder there. Run
/// from the test's own folder, so anything written that way would show.
///
/// `schedule install` is not run here. It is the one command that would ask the system's
/// service manager, the person's own launchd or systemd, were it let through: launchd finds
/// a job by the label inside its file, so a scratch copy would replace their real schedule.
/// The gate's refusal of it is the core's `service` tests', on a machine in memory, and no
/// build for tests asks the service manager at all
/// (`no_build_for_tests_asks_the_systems_service_manager`).
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn every_command_refuses_a_home_that_is_not_a_full_path() {
    let mut env = two_accounts("home-not-absolute");
    access_lapsed(&env, "beta");
    env.expect_usage_requests(0);
    let before = tree(&env.root);
    let commands: [(&[&str], &str); 15] = [
        (&["status"], "status"),
        (&["status", "--offline"], "status"),
        (&["enroll", "gamma"], "enroll"),
        (&["use", "beta"], "use"),
        (&["renew"], "renew"),
        (&["forget", "beta", "--yes"], "forget"),
        (&["stow", "--yes"], "stow"),
        (&["rename", "beta", "gamma"], "rename"),
        (&["abandon"], "abandon"),
        (&["repair"], "repair"),
        (&["adopt"], "adopt"),
        (&["schedule", "status"], "schedule"),
        (&["log"], "log"),
        (&["statusline"], "statusline"),
        (&["uninstall", "--yes"], "uninstall"),
    ];
    let run = |args: &[&str], home: &str| {
        env.command(&[args, &["--json"]].concat())
            .env("HOME", home)
            .current_dir(&env.root)
            .output()
            .expect("run Pitboard")
    };
    for home in ["", "relative"] {
        let out = run(&["doctor"], home);
        assert_eq!(out.status.code(), Some(3), "HOME={home:?} pitboard doctor");
        let diagnosed = envelope(&String::from_utf8_lossy(&out.stdout));
        assert_eq!(diagnosed["error"]["code"], "checks_failed");
        let checks = diagnosed["data"]["checks"].as_array().expect("its checks");
        assert_eq!(checks.len(), 1, "nothing else is checked: {diagnosed}");
        assert_eq!(checks[0]["code"], "homes");
        assert_eq!(checks[0]["level"], "fail");
    }
    for home in ["", "relative"] {
        for (args, command) in commands {
            let at = format!("HOME={home:?} pitboard {}", args.join(" "));
            let out = run(args, home);
            assert_eq!(out.status.code(), Some(1), "{at}");
            let refused = envelope(&String::from_utf8_lossy(&out.stdout));
            assert_eq!(refused["command"], command, "{at}");
            assert_eq!(refused["error"]["code"], "home_not_absolute", "{at}");
            assert!(
                refused["error"]["message"]
                    .as_str()
                    .is_some_and(|said| said.starts_with("HOME is ")),
                "{at}: {refused}"
            );
        }
    }
    assert!(tree(&env.root) == before, "nothing is written");
    env.assert_usage_requests();

    let out = env
        .command(&["completions", "bash"])
        .env("HOME", "")
        .current_dir(&env.root)
        .output()
        .expect("run Pitboard");
    assert_eq!(out.status.code(), Some(0), "a generated file reads no home");
    assert!(!out.stdout.is_empty());
}

/// A build for tests never asks the system's service manager, whose jobs are the person's
/// own whatever home a test gives: `pitboard schedule install`, with a scratch home that is
/// a full path, `PITBOARD_HOME` the default directory under it, since the schedule is
/// installed from there alone, and nothing else in its way, is refused with
/// `schedule_refused` and leaves no unit behind. The `systemctl` it would start is a
/// stand-in on `PATH` that writes down that it was asked, which it was. Linux only: launchd
/// is asked by its full path, so no stand-in can take its place there, and running this
/// against a build that asked it would replace the person's real schedule.
#[cfg(target_os = "linux")]
#[test]
fn no_build_for_tests_asks_the_systems_service_manager() {
    use pitboard_core::testing::stand_in::{Script, Step};
    let env = two_accounts("no-service-manager");
    let asked = env.root.join("systemctl-was-asked");
    let systemctl = env.root.join("bin/systemctl");
    std::fs::create_dir_all(systemctl.parent().expect("its folder")).expect("made");
    common::put_stand_in(
        &systemctl,
        &Script::Plays {
            args: None,
            steps: vec![Step::Records {
                at: asked.clone(),
                variables: Vec::new(),
            }],
        },
    );
    let home = env.root.join("home");
    std::fs::create_dir_all(&home).expect("a home of the test's own");

    let out = env
        .command(&["schedule", "install", "--json"])
        .env("HOME", &home)
        .env("PITBOARD_HOME", home.join(".pitboard"))
        .output()
        .expect("run Pitboard");

    let refused = envelope(&String::from_utf8_lossy(&out.stdout));
    assert_eq!(out.status.code(), Some(1), "{refused}");
    assert_eq!(refused["error"]["code"], "schedule_refused", "{refused}");
    assert!(
        !asked.exists(),
        "systemctl was asked: {}",
        std::fs::read_to_string(&asked).unwrap_or_default()
    );
    assert!(
        !home
            .join(".config/systemd/user/pitboard-renew.timer")
            .exists(),
        "the timer was taken away again"
    );
}

/// Under sudo, `pitboard status` answers what `--offline` answers and says why, asking
/// Anthropic nothing and writing nothing.
// `SUDO_UID` is Unix's sign of sudo; Windows' `sudo` elevates the token (`windows_refuses`).
#[cfg(unix)]
#[test]
fn status_under_sudo_answers_from_what_was_measured() {
    let mut env = two_accounts("status-sudo");
    env.expect_usage_requests(0);
    let before = tree(&env.root);

    let out = env
        .command(&["status", "--json"])
        .env("SUDO_UID", "501")
        .output()
        .expect("run Pitboard");

    assert_eq!(out.status.code(), Some(0));
    let read = envelope(&String::from_utf8_lossy(&out.stdout));
    assert_eq!(read["ok"], true);
    let codes: Vec<&str> = read["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .filter_map(|w| w["code"].as_str())
        .collect();
    assert_eq!(codes, ["read_only"]);
    assert!(
        read["warnings"][0]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("Pitboard runs with sudo, so it changes nothing")),
        "{read}"
    );
    assert!(tree(&env.root) == before, "nothing is written");
    env.assert_usage_requests();
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn forgetting_the_signed_in_account_is_refused() {
    let env = two_accounts("forget");
    let (_, err, code) = env.run(&["forget", "alpha"]);
    assert_eq!(code, 1);
    assert!(err.contains("signed in"), "{err}");
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn forgetting_an_account_deletes_its_parked_login() {
    let env = two_accounts("forget-parked");
    let parked = env.parked_service("beta").unwrap();
    assert!(env.is_parked(&parked));

    let (_, err, code) = env.run(&["forget", "beta"]);

    assert_eq!(code, 0, "{err}");
    assert!(accounts(&env).iter().all(|a| a["label"] != "beta"));
    assert!(!env.is_parked(&parked));
    assert_eq!(
        env.state()["discarded"],
        serde_json::json!([]),
        "a deleted item must not stay listed for another attempt"
    );
}

/// A home that arrived from another computer stops every command, which is right: a parked
/// login is a refresh token, and two machines taking turns presenting one ends the login
/// for both. What was missing was a way out that is not "delete everything and start over".
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn a_home_from_another_computer_is_taken_over_rather_than_being_a_dead_end() {
    let env = two_accounts("adopted");
    let parked = env.parked_service("beta").expect("beta is parked");
    env.edit_state(|s| s["machine"] = serde_json::json!("a hash from another computer"));

    // Every ordinary command refuses it, and says what to run.
    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 1);
    assert!(err.contains("pitboard adopt"), "{err}");

    let (out, err, code) = env.run(&["adopt", "--json"]);
    assert_eq!(code, 0, "{err}");
    let envelope = envelope(&out);
    assert_eq!(envelope["data"]["adopted"], true);
    assert_eq!(
        envelope["data"]["logins_dropped"],
        serde_json::json!(["beta"])
    );

    // The accounts are still here, the logins are not, and the tool works again.
    let labels: Vec<String> = accounts(&env)
        .iter()
        .map(|a| a["label"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(labels.contains(&"alpha".to_string()));
    assert!(labels.contains(&"beta".to_string()));
    assert!(
        !env.is_parked(&parked),
        "the login that came with it is deleted"
    );

    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("nothing parked") || err.contains("--sign-in"),
        "the refusal is now about the login, not about the computer: {err}"
    );
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn an_account_with_nothing_parked_is_refused_with_the_way_back() {
    let env = two_accounts("exhausted");
    // Nothing parked means nothing parked: the state must not name one, and the vault must
    // not hold one either, or Pitboard gives it back rather than refusing.
    for label in ["alpha", "beta"] {
        if let Some(service) = env.parked_service(label) {
            env.delete_park(&service);
        }
    }
    env.edit_state(|s| {
        for a in s["accounts"].as_array_mut().unwrap() {
            a["parked"] = serde_json::Value::Null;
        }
    });

    let (out, _, code) = env.run(&["use", "beta", "--json"]);

    assert_eq!(code, 1);
    let envelope = envelope(&out);
    assert_eq!(envelope["error"]["code"], "nothing_parked");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("pitboard enroll beta --sign-in")
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a");
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn an_expired_parked_login_is_refused_rather_than_installed() {
    let env = two_accounts("expired");
    env.edit_state(|s| {
        for a in s["accounts"].as_array_mut().unwrap() {
            if a["label"] == "beta" {
                a["parked"]["refresh_expires_at"] = serde_json::json!(1_000);
            }
        }
    });

    let (out, _, code) = env.run(&["use", "beta", "--json"]);

    assert_eq!(code, 1);
    assert_eq!(envelope(&out)["error"]["code"], "parked_login_expired");
    assert_eq!(
        env.live()["claudeAiOauth"]["refreshToken"],
        "refresh-a",
        "installing a dead login would leave nothing signed in"
    );
}

/// Two simultaneous switches must never interleave. Pitboard's runs exclude each other with
/// a kernel lock, so the second waits for the first to finish and then finds beta already
/// signed in.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
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
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let already = outcomes
        .iter()
        .filter(|o| String::from_utf8_lossy(&o.stdout).contains("already signed in"))
        .count();
    assert_eq!(already, 1, "the second run must see the first one's result");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");
    assert!(env.parked_service("alpha").is_some());
}

#[test]
fn a_mistyped_command_line_still_answers_in_json_when_asked() {
    let env = Env::new("usage");
    let (out, _, code) = env.run(&["use", "--json"]);
    assert_eq!(code, 2);
    let envelope = envelope(&out);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "usage");
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn the_status_line_names_the_account_in_use_and_the_others() {
    let env = two_accounts("statusline");
    let statusline = |session: serde_json::Value| {
        let mut child = env
            .command(&["statusline"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(session.to_string().as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let opened = statusline(serde_json::json!({"session_id": "pane"}));
    assert!(opened.status.success());
    assert_eq!(
        anstream::adapter::strip_str(&String::from_utf8_lossy(&opened.stdout)).to_string(),
        "alpha ?·?  beta ?·?\n"
    );
    // Nobody has asked Anthropic about alpha's usage here, so no reading holds the windows
    // of the numbers its first response brought: they are the session's, and the label is
    // marked.
    let out = statusline(serde_json::json!({"session_id": "pane", "rate_limits": {
        "five_hour": {"used_percentage": 46.0, "resets_at": 4_000_000_000i64},
        "seven_day": {"used_percentage": 70.0, "resets_at": 4_000_000_000i64}
    }}));

    assert!(out.status.success());
    let line = String::from_utf8_lossy(&out.stdout);
    assert!(
        line.contains("\u{1b}["),
        "Claude Code draws colours from a pipe: {line:?}"
    );
    assert_eq!(
        anstream::adapter::strip_str(&line).to_string(),
        "alpha? 46%·70%  beta ?·?\n"
    );
}

/// Uninstalling has one job beyond deleting files: the parked logins are live refresh
/// tokens, and ~/.pitboard is the only index of them. Removing the directory without them
/// would leave credentials on the machine that nothing can name.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn uninstalling_takes_the_parked_logins_with_it() {
    let env = two_accounts("uninstall-clean");
    let parked = env
        .parked_service("beta")
        .expect("beta was enrolled by signing in, so it has a parked login");
    assert!(env.is_parked(&parked), "the park is there to begin with");

    let (out, err, code) = env.run(&["uninstall", "--yes"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Removed 1 parked login"), "{out}");

    assert!(!env.is_parked(&parked), "the parked login is gone");
    assert!(
        !env.root.join("pitboard").exists(),
        "Pitboard's own directory is gone"
    );
    // The account that was signed in is still signed in: uninstalling is not a logout.
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a");
}

/// macOS reads at most 4097 bytes of command from `security`'s stdin, and MCP server tokens
/// make a login larger than that. There is no third way to write one: the only other route
/// `security` offers is the argument line, which is what Claude Code uses for the same
/// login. So the switch goes through, and says that is what it did.
#[cfg(target_os = "macos")]
#[test]
fn a_login_too_large_for_stdin_is_written_on_the_argument_line() {
    let env = two_accounts("too-large");
    let mut live = env.live();
    live["mcpOAuth"] = serde_json::json!({ "server": "x".repeat(4096) });
    env.replace_live(&live);

    let (out, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(err.contains("argument line"), "it says what it did: {err}");
    assert_eq!(
        env.live()["claudeAiOauth"]["refreshToken"],
        "refresh-b",
        "and the switch actually happened"
    );
    assert_eq!(
        env.live()["mcpOAuth"]["server"].as_str().map(str::len),
        Some(4096),
        "what belongs to the machine came across with it"
    );
}

/// For anyone who would rather have the refusal: it says the size, the limit, and that the
/// refusal is theirs.
#[cfg(target_os = "macos")]
#[test]
fn the_argument_line_can_be_refused() {
    let env = two_accounts("too-large-refused");
    let mut live = env.live();
    live["mcpOAuth"] = serde_json::json!({ "server": "x".repeat(4096) });
    env.replace_live(&live);
    let before = env.live();

    let (_, err, code) = env
        .command(&["use", "beta"])
        .env("PITBOARD_NO_ARGV", "1")
        .output()
        .map(|o| {
            (
                String::from_utf8_lossy(&o.stdout).into_owned(),
                String::from_utf8_lossy(&o.stderr).into_owned(),
                o.status.code().unwrap_or(-1),
            )
        })
        .expect("ran");
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("PITBOARD_NO_ARGV"), "{err}");
    assert_eq!(env.live(), before, "nothing moved");
}

/// How often Pitboard asks Anthropic is a design question, not an accident. A number is
/// only worth asking for again once the tightest limit it describes could have moved by a
/// percentage point, which for a five-hour window is three minutes. Two runs inside that
/// make one request between them, however many front ends are involved.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn asking_twice_in_a_row_asks_anthropic_once() {
    let mut env = two_accounts("budget");
    // Two accounts, so one pass is two requests. The second pass is none.
    env.expect_usage_requests(2);

    let (_, err, code) = env.run(&["status", "--json"]);
    assert_eq!(code, 0, "{err}");
    let (out, err, code) = env.run(&["status", "--json"]);
    assert_eq!(code, 0, "{err}");

    env.assert_usage_requests();

    // And it says so rather than passing a remembered number off as a live one.
    let rows = envelope(&out)["data"]["accounts"].clone();
    let stale: Vec<&str> = rows
        .as_array()
        .expect("accounts")
        .iter()
        .filter_map(|a| a["stale"].as_str())
        .collect();
    assert!(
        stale.contains(&"asked_recently"),
        "a number served from the last reading must say so: {stale:?}"
    );
}

/// Asking for it is always allowed: the floor is a default, not a rule about what a person
/// may do.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn asking_for_a_fresh_reading_asks_anthropic_again() {
    let mut env = two_accounts("budget-fresh");
    env.expect_usage_requests(4);

    let (_, err, code) = env.run(&["status", "--json"]);
    assert_eq!(code, 0, "{err}");
    let (_, err, code) = env.run(&["status", "--fresh", "--json"]);
    assert_eq!(code, 0, "{err}");

    env.assert_usage_requests();
}

/// The fortnight of readings per account an older Pitboard kept, to work out how long an
/// account lasts, goes the first time this one reads usage: pace needs one reading. Only
/// what Pitboard wrote goes, and a file somebody else put there keeps the folder.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn the_readings_an_older_pitboard_kept_go_and_nothing_else_does() {
    let env = two_accounts("retired-readings");
    let readings = env.root.join("pitboard/readings");
    std::fs::create_dir_all(&readings).expect("an older Pitboard's readings");
    let series = readings.join(format!("{}.ndjson", env.uuid('a')));
    std::fs::write(&series, "{\"at\":1,\"windows\":[[\"five_hour\",12.0]]}\n").expect("a series");

    let (_, err, code) = env.run(&["status", "--json"]);
    assert_eq!(code, 0, "{err}");
    assert!(!readings.exists(), "nothing but Pitboard's own was there");

    std::fs::create_dir_all(&readings).expect("readings again");
    std::fs::write(&series, "{}\n").expect("a series");
    std::fs::write(readings.join("notes.txt"), "mine").expect("somebody's own file");
    let (_, err, code) = env.run(&["status", "--json"]);
    assert_eq!(code, 0, "{err}");
    assert!(!series.exists());
    assert!(readings.join("notes.txt").exists());
}

/// A label written by 0.1.x could contain a slash. Every message about its lapsed login
/// says to sign in to it again with `pitboard enroll <label> --sign-in`, and that has to
/// work for such a label as it did before labels could name a tool.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn an_old_label_with_a_slash_can_be_signed_in_to_again() {
    let mut env = two_accounts("old-slash-label");
    env.edit_state(|state| {
        for account in state["accounts"].as_array_mut().unwrap() {
            if account["label"] == "beta" {
                account["label"] = "team/beta".into();
            }
        }
    });
    let (b, p) = (env.uuid('b'), env.uuid('p'));
    let (_, err, code) =
        env.enroll_by_signing_in("team/beta", &b, "b@example.com", &p, "refresh-b2");
    assert_eq!(code, 0, "{err}");
    let renewed = accounts(&env)
        .into_iter()
        .find(|a| a["label"] == "team/beta")
        .expect("still enrolled under its old label");
    assert_eq!(renewed["provider"], "claude");
}
