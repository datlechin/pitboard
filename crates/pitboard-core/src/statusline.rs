//! `pitboard statusline`: one line for Claude Code's status bar, naming the account in use
//! and what every enrolled account has left.
//!
//! Claude Code runs it after every message with its session as JSON on stdin, including the
//! limits of the account that session is using, and on a timer too when its settings ask.
//! So this reads only files, with no keychain and no network, and writes one: what the
//! session passed goes into pitboard's readings as the account in use's, where it can be
//! that account's, and they keep it only where it is newer than what they have. The account
//! in use shows the newer of the two, so a session left open shows what the busy ones have
//! recorded since. The other accounts show pitboard's last reading of them, with its age
//! once that is worth knowing.
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
    /// The session's own account: the newer, limit by limit, of what Claude Code passed,
    /// where it can be this account's, and what pitboard's readings have.
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
    let offered = session_snapshot(input, signed_in, state, remembered, now);
    let in_use = crate::usage::merge(known, offered.as_ref(), now);
    StatusLine {
        current: current.map(|a| a.label.clone()),
        session: in_use.map_or_else(Shares::default, |r| shares_of(&r, now)),
        others,
    }
}

/// What Claude Code passed about the session's own account, as a reading to offer the
/// account `signed_in` names. Free: these are numbers the session already had, not a
/// question asked of anyone.
///
/// It carries no time. The numbers are what the session's last response said, however long
/// ago that was, so the readings stamp them only when they move something forward.
///
/// Nor does it say whose they are, and they can be another account's. A switch rewrites
/// Claude Code's config at once, while an open session goes on using the account before
/// until it takes up the switch, and one left idle passes that account's numbers until its
/// next response. A window's reset is what tells them apart: each account's windows start
/// when that account is first used in them, and its sessions recorded them under it while
/// it was in use. So a window another of Claude Code's accounts has recorded, and this one
/// has not, is left out, and so is one whose reset has passed, which says nothing about
/// now. A window no account has is offered, which is how a session records its own
/// account's next one. For as long as sessions take to follow a switch, counted from when
/// pitboard last put the account to use, nothing is offered at all: a response they get
/// then is the account before's, and an account nothing has read yet has no windows to be
/// known by.
///
/// A session says how much of each limit is used and when it resets, and nothing else. So a
/// window is the one pitboard already has for that limit with those two numbers put in: its
/// name, and whether the account is working against it, stay as Anthropic said. A share the
/// session has moved is one Anthropic has not graded.
fn session_snapshot(
    input: &Value,
    signed_in: Option<&str>,
    state: &State,
    remembered: &HashMap<String, Snapshot>,
    now: i64,
) -> Option<Snapshot> {
    let adopting = signed_in
        .and_then(|uuid| state.by_uuid(ProviderId::Claude, uuid))
        .and_then(|account| account.last_used_at)
        .is_some_and(|at| {
            (0..i64::from(crate::switch::ADOPTION_CEILING_SECONDS)).contains(&(now - at))
        });
    if adopting {
        return None;
    }
    let limits = input.get("rate_limits")?;
    let known = signed_in.and_then(|uuid| remembered.get(uuid));
    // Claude Code's accounts only: a Codex reading is of other limits, whatever they are
    // called.
    let others: Vec<&Snapshot> = state
        .accounts
        .iter()
        .filter(|a| a.provider() == ProviderId::Claude)
        .filter(|a| Some(a.account_uuid.as_str()) != signed_in)
        .filter_map(|a| remembered.get(&a.account_uuid))
        .collect();
    let has = |reading: &Snapshot, window: &Window| {
        reading.windows.iter().any(|had| had.same_window(window))
    };
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
        let over = given.resets_at.is_some_and(|at| at <= now);
        let anothers =
            !known.is_some_and(|k| has(k, &given)) && others.iter().any(|r| has(r, &given));
        if over || anothers {
            return None;
        }
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
/// It also offers the readings what the session told it, every time, as the account in
/// use's wherever it can be that account's, and they keep it where it is newer. So the
/// accounts a person actually works in stop reading as unknown without anyone running
/// `pitboard` by hand, and every session and the menu bar show the newest numbers any of
/// them has seen. It still asks nobody anything: no network, no credential.
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
        && let Some(offered) = session_snapshot(&input, Some(uuid), &state, &remembered, now)
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
    const HOUR: i64 = 3_600;
    const DAY: i64 = 86_400;

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
        timed((five, resets_at), (week, resets_at), observed_at)
    }

    /// A reading of the five-hour and weekly limits, each as its share and when it resets.
    fn timed(five_hour: (f64, i64), weekly: (f64, i64), observed_at: i64) -> Snapshot {
        let window = |kind: &str, (percent, resets_at): (f64, i64)| Window {
            kind: kind.into(),
            scope: None,
            percent,
            resets_at: Some(resets_at),
            is_active: false,
            severity: None,
            length_seconds: None,
        };
        Snapshot {
            windows: vec![window("session", five_hour), window("weekly_all", weekly)],
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
        passed((five_hour, resets_at), (weekly, resets_at))
    }

    /// What Claude Code passes a session's status line: each limit's share and when it
    /// resets.
    fn passed(five_hour: (f64, i64), weekly: (f64, i64)) -> Value {
        json!({"rate_limits": {
            "five_hour": {"used_percentage": five_hour.0, "resets_at": five_hour.1},
            "seven_day": {"used_percentage": weekly.0, "resets_at": weekly.1}
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
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        sign_in(&ctx, "work");
        (ctx, Scratch(root))
    }

    /// Claude Code's config naming `label`'s account as signed in, as a switch leaves it.
    fn sign_in(ctx: &Context, label: &str) {
        std::fs::write(
            ctx.home().join(".claude.json"),
            json!({"oauthAccount": {
                "accountUuid": format!("{label}-uuid"),
                "emailAddress": format!("{label}@example.com"),
                "organizationUuid": "o",
            }})
            .to_string(),
        )
        .expect("a Claude Code config");
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
                reading(12.0, 40.0, NOW - 60, NOW + 2 * HOUR),
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

    /// A switch rewrites Claude Code's config at once, and a session left idle goes on
    /// passing the numbers of its last response, which were the account before's. Recorded
    /// as the account switched to, their later resets stood over every answer Anthropic gave
    /// about it until its own windows reset: days, for the weekly limit.
    #[test]
    fn a_session_holding_the_account_before_a_switch_is_not_recorded_as_the_one_after() {
        let (ctx, _scratch) = machine("switched");
        crate::state::save(&ctx, &state()).expect("an account index");
        let work = |five: f64, weekly: f64, observed_at: i64| {
            timed((five, NOW + 2 * HOUR), (weekly, NOW + 3 * DAY), observed_at)
        };
        crate::readings::remember(&ctx, &[("work-uuid".into(), work(10.0, 30.0, NOW - 60))]);
        let personals = passed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY)).to_string();
        sign_in(&ctx, "personal");
        read(&ctx, &personals);

        sign_in(&ctx, "work");
        let idle = read(&ctx, &personals);
        assert_eq!(idle.current.as_deref(), Some("work"));
        assert_eq!(
            idle.session,
            shares(10.0, 30.0),
            "the pane shows work's own"
        );
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(10.0, 30.0));

        let mut answered = work(12.0, 31.0, NOW);
        answered.source = Source::Live;
        crate::readings::remember(&ctx, &[("work-uuid".into(), answered)]);
        assert_eq!(
            shares_of(&recorded(&ctx), NOW),
            shares(12.0, 31.0),
            "and what Anthropic says about work is taken"
        );
    }

    /// For half a minute after a switch a session goes on using the account before, and its
    /// responses are that account's. One nothing has read has no windows to know them by,
    /// so only the time says so.
    #[test]
    fn nothing_is_recorded_while_sessions_are_still_taking_up_a_switch() {
        let (ctx, _scratch) = machine("adopting");
        let mut switched = state();
        switched.accounts[0].last_used_at = Some(NOW - 10);
        crate::state::save(&ctx, &switched).expect("an account index");
        let works = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        crate::readings::remember(&ctx, &[("work-uuid".into(), works)]);

        let unread = passed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY)).to_string();
        let shown = read(&ctx, &unread);
        assert_eq!(
            shown.session,
            shares(10.0, 30.0),
            "the pane shows work's own"
        );
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(10.0, 30.0));

        switched.accounts[0].last_used_at =
            Some(NOW - i64::from(crate::switch::ADOPTION_CEILING_SECONDS));
        crate::state::save(&ctx, &switched).expect("an account index");
        read(
            &ctx,
            &passed((12.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY)).to_string(),
        );
        assert_eq!(
            shares_of(&recorded(&ctx), NOW),
            shares(12.0, 31.0),
            "and once they have, what they say is"
        );
    }

    /// A window whose reset has passed says nothing about now, and a pane left open past it
    /// can be holding any account's. Recorded, it put the share of a window that was over
    /// into a reading, where the menu bar and the command line showed it.
    #[test]
    fn a_window_past_its_reset_is_not_recorded() {
        let (ctx, _scratch) = machine("passed");
        crate::state::save(&ctx, &state()).expect("an account index");
        read(
            &ctx,
            &passed((90.0, NOW - 60), (40.0, NOW + 3 * DAY)).to_string(),
        );
        let kinds: Vec<String> = recorded(&ctx).windows.into_iter().map(|w| w.kind).collect();
        assert_eq!(kinds, ["weekly_all"]);
    }

    /// An account's windows start when it is first used in them, so a reset no other
    /// account has is this account's own. It is how a session records the window after one
    /// that ran out, before anybody has asked Anthropic.
    #[test]
    fn a_session_still_records_its_own_accounts_next_window() {
        let (ctx, _scratch) = machine("next");
        crate::state::save(&ctx, &state()).expect("an account index");
        crate::readings::remember(
            &ctx,
            &[
                (
                    "work-uuid".into(),
                    timed((100.0, NOW - 60), (40.0, NOW + 3 * DAY), NOW - HOUR),
                ),
                (
                    "personal-uuid".into(),
                    timed((20.0, NOW + 4 * HOUR), (50.0, NOW + 5 * DAY), NOW - 60),
                ),
            ],
        );
        let shown = read(
            &ctx,
            &passed((2.0, NOW + 5 * HOUR), (41.0, NOW + 3 * DAY)).to_string(),
        );
        assert_eq!(shown.session, shares(2.0, 41.0));
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(2.0, 41.0));
    }

    /// Two accounts' windows can reset within a minute of each other. When the account in
    /// use has that window too, the session's numbers can be its own, and they are offered.
    #[test]
    fn a_window_both_accounts_have_is_still_offered() {
        let (ctx, _scratch) = machine("coincident");
        crate::state::save(&ctx, &state()).expect("an account index");
        crate::readings::remember(
            &ctx,
            &[
                (
                    "work-uuid".into(),
                    timed((20.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60),
                ),
                (
                    "personal-uuid".into(),
                    timed((50.0, NOW + 2 * HOUR + 30), (60.0, NOW + 5 * DAY), NOW - 60),
                ),
            ],
        );
        let shown = read(
            &ctx,
            &passed((25.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY)).to_string(),
        );
        assert_eq!(shown.session, shares(25.0, 31.0));
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(25.0, 31.0));
    }
}
