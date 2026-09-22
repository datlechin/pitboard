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

#[test]
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

    assert_eq!(env.state()["active"], "beta");
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
fn enrolling_an_account_under_a_second_label_points_at_sign_in() {
    let env = two_accounts("dupe");
    let (_, err, code) = env.run(&["enroll", "another"]);
    assert_eq!(code, 1);
    assert!(err.contains("--sign-in"), "{err}");
}

#[test]
fn enrolling_the_signed_in_account_again_is_not_an_error() {
    let env = two_accounts("again");
    let before = env.state();
    let (_, err, code) = env.run(&["enroll", "alpha"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(env.state()["accounts"], before["accounts"]);
}

/// The way back for an account whose parked login was used or expired, and what every
/// message about one says to run.
#[test]
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

/// A label typed wrong at enroll time is fixed without signing in again, whichever account
/// it names.
#[test]
fn renaming_keeps_the_login_and_the_new_label_switches() {
    let env = two_accounts("rename");
    let beta_park = env.parked_service("beta").unwrap();

    for (from, to) in [("alpha", "personal"), ("beta", "work")] {
        let (out, err, code) = env.run(&["rename", from, to]);
        assert_eq!(code, 0, "{err}");
        assert!(out.contains(&format!("Renamed {from} to {to}")), "{out}");
    }
    assert_eq!(env.state()["active"], "personal");
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
fn a_sign_in_in_progress_does_not_hold_up_a_switch() {
    let mut env = two_accounts("waiting");
    let (c, q) = (env.uuid('c'), env.uuid('q'));
    let credential = common::credential("refresh-c").to_string();
    env.install_fake_claude(&credential);
    env.owns("access-refresh-c", &c, "c@example.com", &q);
    let signing_in = env
        .command(&["enroll", "side", "--sign-in"])
        .env("FAKE_SIGN_IN_SECONDS", "4")
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

/// A parked login is pitboard's alone, so status renews it once its access lapses, stores
/// it the way Claude Code would, and a later switch installs the renewed login.
#[test]
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

#[test]
fn forgetting_the_signed_in_account_is_refused() {
    let env = two_accounts("forget");
    let (_, err, code) = env.run(&["forget", "alpha"]);
    assert_eq!(code, 1);
    assert!(err.contains("signed in"), "{err}");
}

#[test]
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
fn an_account_with_nothing_parked_is_refused_with_the_way_back() {
    let env = two_accounts("exhausted");
    // Nothing parked means nothing parked: the state must not name one, and the vault must
    // not hold one either, or pitboard gives it back rather than refusing.
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

/// Two simultaneous switches must never interleave. pitboard's runs exclude each other with
/// a kernel lock, so the second waits for the first to finish and then finds beta already
/// signed in.
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
fn the_status_line_names_the_account_in_use_and_the_others() {
    let env = two_accounts("statusline");
    let session = serde_json::json!({"rate_limits": {
        "five_hour": {"used_percentage": 46.0, "resets_at": 4_000_000_000i64},
        "seven_day": {"used_percentage": 70.0, "resets_at": 4_000_000_000i64}
    }});
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
    let out = child.wait_with_output().unwrap();

    assert!(out.status.success());
    let line = String::from_utf8_lossy(&out.stdout);
    assert!(
        line.contains("\u{1b}["),
        "Claude Code draws colours from a pipe: {line:?}"
    );
    assert_eq!(
        anstream::adapter::strip_str(&line).to_string(),
        "alpha 46%·70%  beta ?·?\n"
    );
}

/// Uninstalling has one job beyond deleting files: the parked logins are live refresh
/// tokens, and ~/.pitboard is the only index of them. Removing the directory without them
/// would leave credentials on the machine that nothing can name.
#[test]
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
        "pitboard's own directory is gone"
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
