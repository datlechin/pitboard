//! `pitboard statusline`: one line for Claude Code's status bar, naming the account in use
//! and what every enrolled account has left.
//!
//! Claude Code runs it after every message with its session as JSON on stdin, including the
//! limits of the account that session is using, and on a timer too when its settings ask.
//! So this reads only files, with no keychain and no network, and writes one: what the
//! session passed goes into pitboard's readings, which keep it only where it is newer than
//! what they have. The account in use shows the newer of the two, so a session left open
//! shows what the busy ones have recorded since. The other accounts show pitboard's last
//! reading of them, with its age once that is worth knowing.
//!
//! It is Claude Code's status bar, so it is about Claude Code's accounts and nothing else.
//! The account in use is the one Claude Code's own record names, looked up among Claude
//! Code's accounts only, and the others listed are the ones this session could be switched
//! to. A Codex account is neither, whatever its label or its identity happens to be.

use crate::context::Context;
use crate::provider::ProviderId;
use crate::state::State;
use crate::usage::{Snapshot, Source, Window};
use serde_json::Value;
use std::collections::HashMap;

/// Older than this, a remembered reading shows its age.
const FRESH_FOR: i64 = 15 * 60;

/// The share of the five-hour and weekly limits already used, when known.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Shares {
    pub five_hour: Option<f64>,
    pub weekly: Option<f64>,
}

/// An enrolled account other than the one in use.
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub label: String,
    pub shares: Shares,
    /// Seconds since this was measured, once that is worth knowing.
    pub age: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub struct StatusLine {
    /// The account Claude Code's config names, or `None` when it is not enrolled.
    pub current: Option<String>,
    /// The session's own account: the newer, limit by limit, of what Claude Code passed
    /// and what pitboard's readings have.
    pub session: Shares,
    pub others: Vec<Entry>,
}

/// The five-hour and weekly shares of a reading.
fn shares_of(reading: &Snapshot, now: i64) -> Shares {
    let share = |kinds: &[&str]| {
        reading
            .windows
            .iter()
            .find(|w| w.scope.is_none() && kinds.contains(&w.kind.as_str()))
            .map(|w| w.used(now))
    };
    Shares {
        five_hour: share(&["session", "five_hour"]),
        weekly: share(&["weekly_all", "seven_day"]),
    }
}

/// `signed_in` is the account Claude Code's config names, which is an identity in Claude
/// Code's namespace and is only ever looked up there.
fn line(
    input: &Value,
    state: &State,
    signed_in: Option<&str>,
    remembered: &HashMap<String, Snapshot>,
    now: i64,
) -> StatusLine {
    // Claude Code runs this, so the line is about Claude Code's accounts. Another tool's
    // account is not something this session could switch to.
    let current = signed_in.and_then(|uuid| state.by_uuid(ProviderId::Claude, uuid));
    let others = state
        .accounts
        .iter()
        .filter(|a| a.provider() == ProviderId::Claude)
        .filter(|a| current.is_none_or(|c| c.account_uuid != a.account_uuid))
        .map(|account| {
            let reading = remembered.get(&account.account_uuid);
            Entry {
                label: account.label.clone(),
                shares: reading.map_or_else(Shares::default, |r| shares_of(r, now)),
                age: reading
                    .and_then(|r| r.observed_at)
                    .map(|at| now - at)
                    .filter(|age| *age > FRESH_FOR),
            }
        })
        .collect();
    let known = signed_in.and_then(|uuid| remembered.get(uuid));
    let in_use = crate::usage::merge(known, session_snapshot(input, known).as_ref(), now);
    StatusLine {
        current: current.map(|a| a.label.clone()),
        session: in_use.map_or_else(Shares::default, |r| shares_of(&r, now)),
        others,
    }
}

/// What Claude Code passed about the session's own account, as a reading to offer. Free:
/// these are numbers the session already had, not a question asked of anyone.
///
/// It carries no time. The numbers are what the session's last response said, however long
/// ago that was, so the readings stamp them only when they move something forward.
///
/// A session says how much of each limit is used and when it resets, and nothing else. So a
/// window is the one pitboard already has for that limit with those two numbers put in: its
/// name, and whether the account is working against it, stay as Anthropic said. A share the
/// session has moved is one Anthropic has not graded.
fn session_snapshot(input: &Value, known: Option<&Snapshot>) -> Option<Snapshot> {
    let limits = input.get("rate_limits")?;
    // Passed under the older names, recorded under the ones Anthropic's answer uses, so an
    // account's reading reads the same whoever took it.
    let window = |passed: &str, named: &str| -> Option<Window> {
        let w = limits.get(passed)?;
        let given = Window {
            kind: named.to_string(),
            scope: None,
            severity: None,
            percent: w.get("used_percentage")?.as_f64()?,
            resets_at: w.get("resets_at").and_then(Value::as_i64),
            is_active: true,
            length_seconds: crate::usage::anthropic_window_length(named),
        };
        Some(
            match known.and_then(|k| k.windows.iter().find(|h| h.same_limit(&given))) {
                Some(had) => Window {
                    percent: given.percent,
                    resets_at: given.resets_at,
                    severity: None,
                    ..had.clone()
                },
                None => given,
            },
        )
    };
    let windows: Vec<Window> = [("five_hour", "session"), ("seven_day", "weekly_all")]
        .into_iter()
        .filter_map(|(passed, named)| window(passed, named))
        .collect();
    (!windows.is_empty()).then_some(Snapshot {
        windows,
        observed_at: None,
        account_uuid: None,
        source: Source::Live,
    })
}

/// Reads Claude Code's session JSON. Never fails: a status bar has nowhere to show an
/// error, so whatever cannot be read is left out.
///
/// It also offers the readings what the session told it about the account in use, every
/// time, and they keep it where it is newer. So the accounts a person actually works in
/// stop reading as unknown without anyone running `pitboard` by hand, and every session and
/// the menu bar show the newest numbers any of them has seen. It still asks nobody
/// anything: no network, no credential.
pub fn read(ctx: &Context, input: &str) -> StatusLine {
    let input: Value = serde_json::from_str(input).unwrap_or(Value::Null);
    let state = crate::state::load(ctx).unwrap_or_default();
    // Claude Code's own record of who is signed in, which is its config: a file, and so
    // something a status bar can afford to read after every message.
    let signed_in = crate::provider::of(ProviderId::Claude)
        .recorded_identity(ctx)
        .map(|id| id.account_id);
    let now = ctx.now();
    let remembered = crate::readings::load(ctx);
    if let Some(uuid) = signed_in.as_deref()
        && let Some(offered) = session_snapshot(&input, remembered.get(uuid))
    {
        crate::readings::remember(ctx, &[(uuid.to_string(), offered)]);
    }
    line(&input, &state, signed_in.as_deref(), &remembered, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Account;
    use crate::time::{Clock, FixedClock};
    use crate::usage::{Source, Window};
    use serde_json::json;
    use std::sync::Arc;

    const NOW: i64 = 1_789_935_000;

    fn state() -> State {
        let account = |label: &str| Account {
            last_used_at: None,
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            detail: crate::state::Detail::Claude {
                organization_uuid: "o".into(),
                oauth_account: json!({}),
            },
            parked: None,
        };
        State {
            accounts: vec![account("work"), account("personal"), account("side")],
            ..State::default()
        }
    }

    fn reading(five: f64, week: f64, observed_at: i64, resets_at: i64) -> Snapshot {
        let window = |kind: &str, percent| Window {
            kind: kind.into(),
            scope: None,
            percent,
            resets_at: Some(resets_at),
            is_active: false,
            severity: None,
            length_seconds: None,
        };
        Snapshot {
            windows: vec![window("session", five), window("weekly_all", week)],
            observed_at: Some(observed_at),
            account_uuid: None,
            source: Source::Remembered,
        }
    }

    fn shares(five_hour: f64, weekly: f64) -> Shares {
        Shares {
            five_hour: Some(five_hour),
            weekly: Some(weekly),
        }
    }

    /// What Claude Code passes a session's status line, with both windows resetting at once.
    fn session(five_hour: f64, weekly: f64, resets_at: i64) -> Value {
        json!({"rate_limits": {
            "five_hour": {"used_percentage": five_hour, "resets_at": resets_at},
            "seven_day": {"used_percentage": weekly, "resets_at": resets_at}
        }})
    }

    /// A home of this test's own, removed when the test is done with it.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A machine whose Claude Code config names `work-uuid` as signed in.
    fn machine(name: &str) -> (Context, Scratch) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-statusline-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch home");
        std::fs::write(
            root.join(".claude.json"),
            json!({"oauthAccount": {
                "accountUuid": "work-uuid",
                "emailAddress": "work@example.com",
                "organizationUuid": "o",
            }})
            .to_string(),
        )
        .expect("a Claude Code config");
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        (ctx, Scratch(root))
    }

    fn recorded(ctx: &Context) -> Snapshot {
        crate::readings::load(ctx)
            .remove("work-uuid")
            .expect("a reading")
    }

    #[test]
    fn names_the_account_in_use_and_what_every_account_has_left() {
        let input = json!({"rate_limits": {
            "five_hour": {"used_percentage": 46.4, "resets_at": NOW + 600},
            "seven_day": {"used_percentage": 70.0, "resets_at": NOW + 86_400}
        }});
        let remembered = HashMap::from([
            (
                "personal-uuid".to_string(),
                reading(12.0, 40.0, NOW - 60, NOW + 600),
            ),
            (
                "side-uuid".to_string(),
                reading(90.0, 88.0, NOW - 3 * 3_600, NOW - 1),
            ),
        ]);
        let line = line(&input, &state(), Some("work-uuid"), &remembered, NOW);
        assert_eq!(line.current.as_deref(), Some("work"));
        assert_eq!(line.session, shares(46.4, 70.0));
        assert_eq!(
            line.others,
            [
                Entry {
                    label: "personal".into(),
                    shares: shares(12.0, 40.0),
                    age: None,
                },
                Entry {
                    label: "side".into(),
                    shares: shares(0.0, 0.0),
                    age: Some(3 * 3_600),
                },
            ],
            "a window past its reset counts as reset, and an old reading says how old"
        );
    }

    #[test]
    fn what_is_unknown_is_left_unknown_not_zero() {
        let line = line(&json!({}), &state(), None, &HashMap::new(), NOW);
        assert_eq!(line.current, None);
        assert_eq!(line.session, Shares::default());
        assert!(line.others.iter().all(|e| e.shares == Shares::default()));
        assert_eq!(line.others.len(), 3);
    }

    /// Claude Code runs this, so the line is about Claude Code's accounts. A Codex account
    /// whose identity matches the session's is not the account in use, and one that shares
    /// a label is not an account this session could be switched to.
    #[test]
    fn another_tools_accounts_are_not_on_claude_codes_line() {
        let codex = |label: &str, uuid: &str| Account {
            last_used_at: None,
            label: label.into(),
            account_uuid: uuid.into(),
            email: format!("{label}@example.com"),
            detail: crate::state::Detail::Codex {
                workspace_id: None,
                plan: None,
            },
            parked: None,
        };
        let mut s = state();
        s.accounts.insert(0, codex("shadow", "work-uuid"));
        s.accounts.push(codex("work", "codex-work"));

        let found = line(&json!({}), &s, Some("work-uuid"), &HashMap::new(), NOW);
        assert_eq!(found.current.as_deref(), Some("work"));
        let others: Vec<&str> = found.others.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(others, ["personal", "side"]);

        let found = line(&json!({}), &s, Some("codex-work"), &HashMap::new(), NOW);
        assert_eq!(
            found.current, None,
            "a Codex identity names no Claude Code account"
        );
    }

    /// The owner's panes on one account: busy ones had recorded 22%·6%, and an idle one
    /// still held the 20%·5% of its last response. Every pane shows the newer of what its
    /// own session passed and what any session recorded.
    #[test]
    fn the_account_in_use_shows_the_newer_of_the_session_and_what_is_remembered() {
        let remembered = HashMap::from([(
            "work-uuid".to_string(),
            reading(22.0, 6.0, NOW - 60, NOW + 600),
        )]);
        let shown = |input: &Value, remembered: &HashMap<String, Snapshot>| {
            line(input, &state(), Some("work-uuid"), remembered, NOW).session
        };
        assert_eq!(
            shown(&session(20.0, 5.0, NOW + 600), &remembered),
            shares(22.0, 6.0)
        );
        assert_eq!(
            shown(&session(25.0, 7.0, NOW + 600), &remembered),
            shares(25.0, 7.0)
        );

        // Rerun at its reset, an idle session holds a window that is over; another session
        // has already recorded the one after it.
        let next = HashMap::from([(
            "work-uuid".to_string(),
            reading(1.0, 6.0, NOW - 60, NOW + 5 * 3_600),
        )]);
        assert_eq!(shown(&session(90.0, 5.0, NOW - 1), &next), shares(1.0, 6.0));
    }

    /// Offered every time, however recently something was recorded, and kept only when it
    /// is newer. It used to be recorded only when what was remembered was a quarter of an
    /// hour old, and then over whatever was there.
    #[test]
    fn the_sessions_numbers_are_recorded_when_they_are_newer() {
        let (ctx, _scratch) = machine("records");
        crate::readings::remember(
            &ctx,
            &[("work-uuid".into(), reading(22.0, 6.0, NOW - 60, NOW + 600))],
        );

        let busy = read(&ctx, &session(25.0, 7.0, NOW + 600).to_string());
        assert_eq!(busy.session, shares(25.0, 7.0));
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(25.0, 7.0));

        let idle = read(&ctx, &session(20.0, 5.0, NOW + 600).to_string());
        assert_eq!(idle.session, shares(25.0, 7.0));
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(25.0, 7.0));
    }

    /// A session knows how much is used and when it resets. Which limit the account is
    /// working against is Anthropic's to say, and the menu bar shows the limit it names:
    /// recorded as a session's guess, a weekly limit at 70% took the menu bar from the
    /// five-hour limit that binds.
    #[test]
    fn a_session_moves_the_numbers_and_leaves_what_only_anthropic_says() {
        let (ctx, _scratch) = machine("leaves");
        let mut answered = reading(22.0, 6.0, NOW - 60, NOW + 600);
        answered.windows[0].severity = Some("normal".into());
        answered.windows[0].is_active = true;
        crate::readings::remember(&ctx, &[("work-uuid".into(), answered)]);

        read(&ctx, &session(25.0, 70.0, NOW + 600).to_string());
        let windows = recorded(&ctx).windows;
        let said: Vec<(&str, f64, bool)> = windows
            .iter()
            .map(|w| (w.kind.as_str(), w.percent, w.is_active))
            .collect();
        assert_eq!(
            said,
            [("session", 25.0, true), ("weekly_all", 70.0, false)],
            "under the names it had, and working against the limit it was"
        );
        assert_eq!(
            windows[0].severity, None,
            "a share Anthropic has not graded"
        );
    }
}
