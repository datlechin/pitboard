//! `pitboard watch --once` against a synthetic Claude Code installation: the numbers
//! Pitboard measured are written where every front end records them, and the switch it makes
//! by itself is the real one, against a stand-in for Anthropic.

mod common;

use common::{Env, two_accounts};
use serde_json::{Value, json};

fn envelope(out: &str) -> Value {
    serde_json::from_str(out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}"))
}

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
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

#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn below_the_share_nothing_moves_and_the_share_is_the_one_asked_for() {
    let env = two_accounts("watch-below");
    env.an_hour_on();
    env.measured(&[("alpha", 90.0, 20.0), ("beta", 10.0, 30.0)]);
    let (out, err, code) = env.run(&["watch", "--once"]);
    assert_eq!(
        (code, out.as_str()),
        (0, "Nothing to switch now.\n"),
        "{err}"
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
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn with_no_account_to_go_to_it_says_so_and_nothing_moves() {
    let env = two_accounts("watch-no-room");
    env.an_hour_on();
    env.measured(&[("alpha", 97.0, 20.0), ("beta", 10.0, 99.0)]);
    let (out, err, code) = env.run(&["watch", "--once"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out,
        "alpha has used 97% of its 5-hour limit, and no other Claude Code account has room \
         below 95% in every limit.\n"
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
