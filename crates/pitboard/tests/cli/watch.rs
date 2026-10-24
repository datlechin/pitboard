//! `pitboard watch`, with `--once` and left running, against a synthetic Claude Code
//! installation: the numbers Pitboard measured are written where every front end records
//! them, and the switch it makes by itself is the real one, against a stand-in for Anthropic.

use crate::common::{Env, envelope, two_accounts};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// `pitboard watch --json` left running, killed and reaped on drop, panic or not: CI waits on
/// a process a test leaves behind.
struct Watching {
    running: Child,
    lines: Receiver<String>,
}

impl Watching {
    fn start(env: &Env) -> Watching {
        let mut running = env
            .command(&["watch", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("pitboard starts");
        let stdout = running.stdout.take().expect("its standard output");
        let (send, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Watching { running, lines }
    }

    /// The next envelope it prints: it decides within 2 seconds of a change to the account
    /// index or the readings, and at least every 30 seconds.
    fn printed(&self) -> Value {
        let line = self
            .lines
            .recv_timeout(Duration::from_secs(60))
            .expect("watch printed an event within a minute");
        envelope(&line)
    }

    /// The data of the next event it prints.
    fn next(&self) -> Value {
        self.printed()["data"].clone()
    }

    /// The account the next event, an `idle` one, names. A look made while a `pitboard use`
    /// holds Pitboard's lock waits for it, and says nothing of that switch's record.
    fn watched(&self) -> Value {
        let data = self.next();
        assert_eq!(data["event"], "idle", "{data}");
        data["account"].clone()
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        let _ = self.running.kill();
        let _ = self.running.wait();
    }
}

#[test]
fn an_account_at_the_share_is_switched_from_once() {
    let env = two_accounts("watch-switches");
    env.an_hour_on();
    env.measured(&[("alpha", 96.0, 20.0), ("beta", 10.0, 30.0)]);

    let (out, err, code) = env.run(&["watch", "--once", "--json"]);
    assert_eq!(code, 0, "{err}");
    let data = &envelope(&out)["data"];
    assert_eq!(data["event"], "switched", "{out}");
    assert_eq!(
        (&data["from"], &data["to"]),
        (&json!("alpha"), &json!("beta"))
    );
    assert_eq!(data["limit"]["kind"], "session");
    assert_eq!(data["adoption"]["follows"], "polling");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");

    let (out, _, _) = env.run(&["log", "--json"]);
    let changes = envelope(&out)["data"]["entries"].clone();
    let automatic: Vec<&Value> = changes
        .as_array()
        .expect("entries")
        .iter()
        .filter(|change| change["verb"] == "auto-switch")
        .collect();
    assert_eq!(automatic.len(), 1, "{changes}");
    assert_eq!(
        (
            &automatic[0]["subject"],
            &automatic[0]["outcome"],
            &automatic[0]["caller"]
        ),
        (&json!("beta"), &json!("ok"), &json!("cli"))
    );

    let (out, err, code) = env.run(&["watch", "--once", "--json"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        envelope(&out)["data"]["event"],
        "idle",
        "beta has room: {out}"
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");
}

/// The app with switching on and the watch service can decide at once from one reading, each
/// in a process of its own. Pitboard's lock lets one switch through; each other decides again
/// under it, finds the account switched to in use with room, and switches nothing.
#[test]
fn front_ends_that_decide_at_once_from_one_reading_make_one_switch() {
    let env = two_accounts("watch-at-once");
    env.an_hour_on();
    env.measured(&[("alpha", 96.0, 20.0), ("beta", 10.0, 30.0)]);

    let deciding: Vec<Child> = (0..4)
        .map(|_| {
            env.command(&["watch", "--once", "--json"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("pitboard starts")
        })
        .collect();
    let events: Vec<Value> = deciding
        .into_iter()
        .map(|child| {
            let out = child.wait_with_output().expect("it ends");
            let printed = String::from_utf8_lossy(&out.stdout);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{printed}{}",
                String::from_utf8_lossy(&out.stderr)
            );
            envelope(&printed)["data"]["event"].clone()
        })
        .collect();

    assert_eq!(
        events.iter().filter(|event| **event == "switched").count(),
        1,
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .all(|event| *event == "switched" || *event == "idle"),
        "{events:?}"
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");
    let (out, _, _) = env.run(&["log", "--json"]);
    let switches: Vec<Value> = envelope(&out)["data"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|entry| entry["verb"] == "auto-switch")
        .map(|entry| entry["outcome"].clone())
        .collect();
    assert_eq!(switches, [json!("ok")]);
}

/// No build for tests asks the system's own service manager, which is the person's whatever
/// home a test gives: `watch install` from the default home is refused with
/// `schedule_refused`, and nothing is left saying the service is on. From another home it is
/// refused first, with `schedule_not_default_home`.
#[test]
fn no_build_for_tests_installs_the_watch_service() {
    let env = two_accounts("watch-service");
    let home = env.root.join("home");
    std::fs::create_dir_all(&home).expect("a home of the test's own");
    let (home, default) = if cfg!(windows) {
        let profile = env.root.join("profile");
        (
            profile.clone(),
            profile.join("AppData").join("Local").join("Pitboard"),
        )
    } else {
        (home.clone(), home.join(".pitboard"))
    };
    let (home, default) = (home.display().to_string(), default.display().to_string());
    let given = [("HOME", home.as_str()), ("PITBOARD_HOME", default.as_str())];

    let (out, _, code) = env.run(&["watch", "install", "--json"]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(
        envelope(&out)["error"]["code"],
        "schedule_not_default_home",
        "{out}"
    );

    let (out, _, code) = env.run_with(&["watch", "install", "--json"], &given);
    assert_eq!(code, 1, "{out}");
    assert_eq!(envelope(&out)["error"]["code"], "schedule_refused", "{out}");
    let (out, err, code) = env.run_with(&["watch", "status", "--json"], &given);
    assert_eq!(code, 0, "{err}");
    assert_eq!(envelope(&out)["data"], json!({"installed": false}), "{out}");
}

#[test]
fn below_the_share_nothing_moves_and_the_share_is_the_one_asked_for() {
    let env = two_accounts("watch-below");
    env.an_hour_on();
    env.measured(&[("alpha", 90.0, 20.0), ("beta", 10.0, 30.0)]);
    let (out, err, code) = env.run(&["watch", "--once"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.starts_with("Watching alpha: 90% of its 5-hour limit, as of "),
        "{out}"
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a");

    let (out, err, code) = env.run(&["watch", "--once", "--at", "90"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.starts_with(
            "Switched Claude Code from alpha to beta: alpha has used 90% of its 5-hour limit."
        ),
        "{out}"
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");
}

#[test]
fn with_no_account_to_go_to_it_says_so_and_nothing_moves() {
    let env = two_accounts("watch-no-room");
    env.an_hour_on();
    env.measured(&[("alpha", 97.0, 20.0), ("beta", 10.0, 99.0)]);
    let (out, err, code) = env.run(&["watch", "--once"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out,
        "alpha has used 97% of its 5-hour limit. Pitboard is not switching: no other Claude \
         Code account has room below 95% in every limit.\n"
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a");
}

/// A reason is said with the reset it was recorded under, so a later answer giving the same
/// window's reset a second off is told as the same reason, and recorded once.
#[test]
fn a_reason_is_said_with_the_reset_it_was_recorded_under() {
    let env = two_accounts("watch-said-window");
    env.an_hour_on();
    env.measured(&[("alpha", 97.0, 20.0), ("beta", 10.0, 99.0)]);
    let said = || {
        let (out, err, code) = env.run(&["watch", "--once", "--json"]);
        assert_eq!(code, 0, "{err}");
        let data = envelope(&out)["data"].clone();
        assert_eq!(data["event"], "no_room", "{out}");
        data["limit"]["resets_at"].clone()
    };
    let recorded = said();
    let alpha = env.account_id("alpha");
    env.edit_readings(|readings| {
        let session = &mut readings[&alpha]["windows"][0];
        session["resets_at"] = json!(session["resets_at"].as_i64().expect("a reset") - 1);
    });
    assert_eq!(said(), recorded);

    let (out, _, _) = env.run(&["log", "--json"]);
    let stays: Vec<Value> = envelope(&out)["data"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|entry| entry["verb"] == "auto-stay")
        .map(|entry| entry["outcome"].clone())
        .collect();
    assert_eq!(stays, [json!("no_room")]);
}

/// While it runs, `watch` names the account it watches once it has read every account, and
/// again whenever that changes, as after a switch somebody makes by hand and one back.
#[test]
fn watching_says_which_account_it_watches_when_that_changes() {
    let env = two_accounts("watch-watching");
    env.an_hour_on();
    let watch = Watching::start(&env);
    assert_eq!(watch.next()["event"], "watching");
    assert_eq!(watch.watched(), "alpha");
    for label in ["beta", "alpha"] {
        let (_, err, code) = env.run(&["use", label]);
        assert_eq!(code, 0, "use {label}: {err}");
        assert_eq!(watch.watched(), label);
    }
}

// A warning a decision finds is printed though the line it comes with was printed before. A
// sign-in over SSH leaves its login in a file behind the keychain or Credential Manager and
// Claude Code's config naming that account, so whose login is stored is in doubt, and the
// decision that settles it under the lock comes to the account already watched, and finds the
// file. Linux keeps no file behind a store.
#[cfg(any(target_os = "macos", windows))]
#[test]
fn a_warning_is_printed_though_the_line_it_comes_with_was_printed_before() {
    let env = two_accounts("watch-warned");
    #[cfg(windows)]
    env.turn_credential_manager_on();
    env.an_hour_on();
    let watch = Watching::start(&env);
    assert_eq!(watch.next()["event"], "watching");
    assert_eq!(watch.watched(), "alpha");

    std::fs::write(
        env.root.join(".credentials.json"),
        crate::common::credential("refresh-ssh").to_string(),
    )
    .unwrap();
    let mut config = env.config();
    config["oauthAccount"]["accountUuid"] = json!(env.uuid('b'));
    std::fs::write(env.root.join(".claude.json"), config.to_string()).unwrap();
    // As another front end recording readings does, which `watch` decides on at its next look.
    env.edit_readings(|_| {});

    let printed = watch.printed();
    assert_eq!(
        (&printed["data"]["event"], &printed["data"]["account"]),
        (&json!("idle"), &json!("alpha")),
        "{printed}"
    );
    assert_eq!(
        printed["warnings"][0]["code"], "fallback_login",
        "{printed}"
    );
}

/// What a decision found on the way, such as a sign-in that overrides the login Pitboard
/// moves, is printed with what it came to, as every command prints its warnings.
#[test]
fn warnings_found_while_deciding_are_printed() {
    let env = two_accounts("watch-warnings");
    env.an_hour_on();
    env.measured(&[("alpha", 96.0, 20.0), ("beta", 10.0, 30.0)]);
    let overridden = [("ANTHROPIC_API_KEY", "sk-ant-not-a-key")];

    let (out, err, code) = env.run_with(&["watch", "--once", "--json"], &overridden);
    assert_eq!(code, 0, "{err}");
    let printed = envelope(&out);
    assert_eq!(
        (&printed["data"]["event"], &printed["data"]["reason"]),
        (&json!("skipped"), &json!("auth_overridden")),
        "{out}"
    );
    assert_eq!(printed["warnings"][0]["code"], "auth_overridden", "{out}");

    // Another limit, whose reason is not recorded yet, so it is decided under the lock again.
    env.measured(&[("alpha", 20.0, 97.0), ("beta", 10.0, 30.0)]);
    let (out, err, code) = env.run_with(&["watch", "--once"], &overridden);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.starts_with("alpha has used 97% of its weekly limit. Pitboard is not switching"),
        "{out}"
    );
    assert_eq!(
        err,
        "warning: ANTHROPIC_API_KEY in the environment is set, so Claude Code signs in with \
         it and not with the login Pitboard moved. Unset it for the switch to take effect.\n"
    );
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a");
}

#[test]
fn a_share_outside_fifty_to_ninety_nine_is_refused() {
    let env = Env::new("watch-share");
    for share in ["49", "100", "ninety"] {
        let (_, err, code) = env.run(&["watch", "--once", "--at", share]);
        assert_eq!(code, 2, "--at {share}: {err}");
    }
}

/// With an account turned on, `watch` says so as it starts, and warms the account up once its
/// five-hour period is not running: the run's answer and the reading after it say it started.
#[test]
fn watch_warms_up_an_account_turned_on_once_its_period_is_not_running() {
    let mut env = two_accounts("watch-warm");
    env.an_hour_on();
    env.install_fake_claude_print();
    env.claude_usage(None);
    let (_, err, code) = env.run(&["status"]);
    assert_eq!(code, 0, "status: {err}");
    let (_, err, code) = env.run(&["warm", "on", "beta"]);
    assert_eq!(code, 0, "warm on: {err}");
    // All day, so the hour this runs at does not matter.
    let (_, err, code) = env.run(&["warm", "range", "00:00", "24:00"]);
    assert_eq!(code, 0, "warm range: {err}");
    // The read `watch` makes as it starts is held off by the one just made; the reading after
    // the run is not.
    env.claude_usage(Some(5 * 3600));

    let watching = Watching::start(&env);
    let started = watching.next();
    assert_eq!(started["event"], "watching", "{started}");
    assert_eq!(started["warm"]["accounts"], json!(["beta"]), "{started}");
    let warmed = (0..4)
        .map(|_| watching.next())
        .find(|data| data["event"] == "warmed_up")
        .expect("beta warmed up");
    assert_eq!(warmed["account"], "beta");
    assert!(warmed["until"].as_i64().is_some(), "{warmed}");
}
