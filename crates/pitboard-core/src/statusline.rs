//! `pitboard statusline`: one line for Claude Code's status bar, naming the account in use
//! and what every enrolled account has left.
//!
//! Claude Code runs it after every message with its session as JSON on stdin, including the
//! limits of the account that session is using, and on a timer too when its settings ask.
//! So this reads only files, with no keychain and no network, and writes two. What the
//! session passed is kept for its next run to compare with, and what moved since its last
//! run goes into pitboard's readings as the account in use's, where it can be that
//! account's; they keep it only where it is newer than what they have. The account in use
//! shows its reading with that folded in, so a session left open shows what the busy ones
//! have recorded since. The other accounts show pitboard's last reading of them, with its
//! age once that is worth knowing.
//!
//! It is Claude Code's status bar, so it is about Claude Code's accounts and nothing else.
//! The account in use is the one Claude Code's own record names, looked up among Claude
//! Code's accounts only, and the others listed are the ones this session could be switched
//! to. A Codex account is neither, whatever its label or its identity happens to be.

use crate::context::Context;
use crate::provider::ProviderId;
use crate::sessions::{Limit, Run};
use crate::state::State;
use crate::usage::{Snapshot, Source, Window};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

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
    /// The session's own account: pitboard's reading of it, with whatever of the session's
    /// numbers can be this account's folded in where they are newer.
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
/// Code's namespace and is only ever looked up there. `offered` is what the session's
/// numbers came to as that account's, if anything.
fn line(
    state: &State,
    signed_in: Option<&str>,
    remembered: &HashMap<String, Snapshot>,
    offered: Option<&Snapshot>,
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
    let in_use = crate::usage::merge(known, offered, now);
    StatusLine {
        current: current.map(|a| a.label.clone()),
        session: in_use.map_or_else(Shares::default, |r| shares_of(&r, now)),
        others,
    }
}

/// What this run of a session's status line was given: the account Claude Code's config
/// names, and each limit the session passed.
fn run_of(input: &Value, signed_in: Option<&str>) -> Run {
    let limits = input.get("rate_limits");
    Run {
        account: signed_in.map(str::to_owned),
        limits: ["five_hour", "seven_day"]
            .into_iter()
            .filter_map(|name| {
                let w = limits?.get(name)?;
                let limit = Limit {
                    used_percentage: w.get("used_percentage")?.as_f64()?,
                    resets_at: w.get("resets_at").and_then(Value::as_i64),
                };
                Some((name.to_string(), limit))
            })
            .collect::<BTreeMap<_, _>>(),
    }
}

/// What the session passed that can be the account in use's, as a reading to offer it.
/// Free: these are numbers the session already had, not a question asked of anyone.
///
/// It carries no time. The numbers are what the session's last response said, however long
/// ago that was, so the readings stamp them only when they move something forward.
///
/// Nor does it say whose they are. A session passes the numbers of its last response every
/// time its status line runs, and one left idle passes the same ones for as long as it
/// stays open, whatever the config has named since, by a switch or a `/login`, and whether
/// or not pitboard still knows the account they were. A change is what says something. A
/// limit that appeared or moved since `before`, this session's previous run, came with a
/// response the session got since, and when the config named the same account then as now,
/// that response was on this account. So only such a limit is offered, and nothing at all
/// from a session not seen before or one whose account the config has changed since. An
/// idle session repeating old numbers is never taken for anyone.
///
/// For as long as sessions take to follow a switch, counted from when pitboard last put the
/// account to use, nothing is offered either: a session still on the account before gets
/// that account's responses however the config reads. A `/login` in Claude Code is followed
/// as slowly and leaves pitboard no time to count from, so a window another of Claude
/// Code's accounts has recorded, and this one has not, is left out too: its reset shows it
/// to be that account's. And a window whose reset has passed says nothing about now.
///
/// A session says how much of each limit is used and when it resets, and nothing else. So a
/// window is the one pitboard already has for that limit with those two numbers put in: its
/// name, and whether the account is working against it, stay as Anthropic said. A share the
/// session has moved is one Anthropic has not graded.
fn session_snapshot(
    run: &Run,
    before: Option<&Run>,
    state: &State,
    remembered: &HashMap<String, Snapshot>,
    now: i64,
) -> Option<Snapshot> {
    let signed_in = run.account.as_deref()?;
    let before = before.filter(|b| b.account == run.account)?;
    let adopting = state
        .by_uuid(ProviderId::Claude, signed_in)
        .and_then(|account| account.last_used_at)
        .is_some_and(|at| {
            (0..i64::from(crate::switch::ADOPTION_CEILING_SECONDS)).contains(&(now - at))
        });
    if adopting {
        return None;
    }
    let known = remembered.get(signed_in);
    // Claude Code's accounts only: a Codex reading is of other limits, whatever they are
    // called.
    let others: Vec<&Snapshot> = state
        .accounts
        .iter()
        .filter(|a| a.provider() == ProviderId::Claude)
        .filter(|a| a.account_uuid != signed_in)
        .filter_map(|a| remembered.get(&a.account_uuid))
        .collect();
    let has = |reading: &Snapshot, window: &Window| {
        reading.windows.iter().any(|had| had.same_window(window))
    };
    // Passed under the older names, recorded under the ones Anthropic's answer uses, so an
    // account's reading reads the same whoever took it.
    let window = |passed: &str, named: &str| -> Option<Window> {
        let limit = run.limits.get(passed)?;
        if before.limits.get(passed) == Some(limit) {
            return None;
        }
        let given = Window {
            kind: named.to_string(),
            scope: None,
            severity: None,
            percent: limit.used_percentage,
            resets_at: limit.resets_at,
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
/// It also offers the readings what moved since the session's last run, as the account in
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
    let run = run_of(&input, signed_in.as_deref());
    let before = input
        .get("session_id")
        .and_then(Value::as_str)
        .and_then(|id| crate::sessions::exchange(ctx, id, &run));
    let offered = session_snapshot(&run, before.as_ref(), &state, &remembered, now);
    if let (Some(uuid), Some(offered)) = (signed_in.as_deref(), offered.as_ref()) {
        crate::readings::remember(ctx, &[(uuid.to_string(), offered.clone())]);
    }
    line(
        &state,
        signed_in.as_deref(),
        &remembered,
        offered.as_ref(),
        now,
    )
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

    /// What `input` offers `work` from a session on it that has just had a response: one
    /// whose run before named `work` and passed nothing, so everything it passes has moved.
    fn answered(input: &Value, remembered: &HashMap<String, Snapshot>) -> Option<Snapshot> {
        let before = Run {
            account: Some("work-uuid".into()),
            limits: BTreeMap::new(),
        };
        session_snapshot(
            &run_of(input, Some("work-uuid")),
            Some(&before),
            &state(),
            remembered,
            NOW,
        )
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

    /// The same machine at another time.
    fn at(ctx: &Context, now: i64) -> Context {
        ctx.clone()
            .with_clock(Arc::new(FixedClock::at(now)) as Arc<dyn Clock>)
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

    /// Session `id`'s status line run with `input`, as Claude Code runs it.
    fn run(ctx: &Context, id: &str, input: &Value) -> StatusLine {
        let mut input = input.clone();
        input["session_id"] = json!(id);
        read(ctx, &input.to_string())
    }

    /// Session `id` starting: its status line runs before it has had a response, with no
    /// limits to pass.
    fn open(ctx: &Context, id: &str) {
        run(ctx, id, &json!({}));
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
        let offered = answered(&input, &remembered);
        let line = line(
            &state(),
            Some("work-uuid"),
            &remembered,
            offered.as_ref(),
            NOW,
        );
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
        let line = line(&state(), None, &HashMap::new(), None, NOW);
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

        let found = line(&s, Some("work-uuid"), &HashMap::new(), None, NOW);
        assert_eq!(found.current.as_deref(), Some("work"));
        let others: Vec<&str> = found.others.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(others, ["personal", "side"]);

        let found = line(&s, Some("codex-work"), &HashMap::new(), None, NOW);
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
            let offered = answered(input, remembered);
            line(
                &state(),
                Some("work-uuid"),
                remembered,
                offered.as_ref(),
                NOW,
            )
            .session
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

    /// Offered whenever a response moves it, however recently something was recorded, and
    /// kept only when it is newer. It used to be recorded only when what was remembered was
    /// a quarter of an hour old, and then over whatever was there.
    #[test]
    fn the_sessions_numbers_are_recorded_when_they_are_newer() {
        let (ctx, _scratch) = machine("records");
        crate::readings::remember(
            &ctx,
            &[("work-uuid".into(), reading(22.0, 6.0, NOW - 60, NOW + 600))],
        );
        run(&ctx, "busy", &session(22.0, 6.0, NOW + 600));
        run(&ctx, "slow", &session(18.0, 4.0, NOW + 600));

        let busy = run(&ctx, "busy", &session(25.0, 7.0, NOW + 600));
        assert_eq!(busy.session, shares(25.0, 7.0));
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(25.0, 7.0));

        let slow = run(&ctx, "slow", &session(20.0, 5.0, NOW + 600));
        assert_eq!(slow.session, shares(25.0, 7.0));
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

        open(&ctx, "pane");
        run(&ctx, "pane", &session(25.0, 70.0, NOW + 600));
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
        let personals = passed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY));
        sign_in(&ctx, "personal");
        open(&ctx, "pane");
        run(&ctx, "pane", &personals);

        sign_in(&ctx, "work");
        for _ in 0..2 {
            let idle = run(&ctx, "pane", &personals);
            assert_eq!(idle.current.as_deref(), Some("work"));
            assert_eq!(
                idle.session,
                shares(10.0, 30.0),
                "the pane shows work's own"
            );
            assert_eq!(shares_of(&recorded(&ctx), NOW), shares(10.0, 30.0));
        }

        let mut answered = work(12.0, 31.0, NOW);
        answered.source = Source::Live;
        crate::readings::remember(&ctx, &[("work-uuid".into(), answered)]);
        assert_eq!(
            shares_of(&recorded(&ctx), NOW),
            shares(12.0, 31.0),
            "and what Anthropic says about work is taken"
        );
    }

    /// Forgetting an account takes its reading with it, and that reading was all that showed
    /// its windows to be its own. An idle session still holding its numbers had them recorded
    /// as the account in use, over what Anthropic went on to say, until those windows reset.
    #[test]
    fn a_session_holding_a_forgotten_accounts_numbers_is_not_recorded_as_the_one_in_use() {
        let (ctx, _scratch) = machine("forgotten");
        let mut accounts = state();
        crate::state::save(&ctx, &accounts).expect("an account index");
        let personals = passed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY));
        sign_in(&ctx, "personal");
        open(&ctx, "pane");
        run(&ctx, "pane", &personals);
        let works = timed(
            (10.0, NOW + 2 * HOUR),
            (30.0, NOW + 3 * DAY),
            NOW - 2 * HOUR,
        );
        crate::readings::remember(&ctx, &[("work-uuid".into(), works)]);

        accounts.accounts[0].last_used_at = Some(NOW - HOUR);
        crate::state::save(&ctx, &accounts).expect("an account index");
        sign_in(&ctx, "work");
        run(&ctx, "pane", &personals);
        accounts.accounts.retain(|a| a.label != "personal");
        crate::state::save(&ctx, &accounts).expect("an account index");
        crate::readings::forget(&ctx, "personal-uuid");

        let idle = run(&ctx, "pane", &personals);
        assert_eq!(
            idle.session,
            shares(10.0, 30.0),
            "the pane shows work's own"
        );
        let mut answered = timed((12.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY), NOW);
        answered.source = Source::Live;
        crate::readings::remember(&ctx, &[("work-uuid".into(), answered)]);
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(12.0, 31.0));
    }

    /// Claude Code's own `/login` can sign in an account nobody enrolled and back again, and
    /// pitboard has no reading of an account it does not know to tell its windows by. An idle
    /// session holding that account's numbers had them recorded as the one signed in after.
    #[test]
    fn a_session_holding_an_unenrolled_accounts_numbers_is_not_recorded_after_a_login() {
        let (ctx, _scratch) = machine("unenrolled");
        crate::state::save(&ctx, &state()).expect("an account index");
        let works = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        crate::readings::remember(&ctx, &[("work-uuid".into(), works)]);
        let guests = passed((97.0, NOW + 4 * HOUR), (80.0, NOW + 6 * DAY));
        sign_in(&ctx, "guest");
        open(&ctx, "pane");
        run(&ctx, "pane", &guests);

        sign_in(&ctx, "work");
        for _ in 0..2 {
            let idle = run(&ctx, "pane", &guests);
            assert_eq!(
                idle.session,
                shares(10.0, 30.0),
                "the pane shows work's own"
            );
        }
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(10.0, 30.0));
    }

    /// A session pitboard has not seen before passes the numbers of whatever response it had
    /// last, which can be any account's: it may have been open since before a switch. They
    /// are left out, and what its next response moves is this account's.
    #[test]
    fn a_session_seen_for_the_first_time_offers_nothing_until_its_next_response() {
        let (ctx, _scratch) = machine("first");
        crate::state::save(&ctx, &state()).expect("an account index");
        let works = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        crate::readings::remember(&ctx, &[("work-uuid".into(), works)]);

        let first = run(
            &ctx,
            "pane",
            &passed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY)),
        );
        assert_eq!(
            first.session,
            shares(10.0, 30.0),
            "the pane shows work's own"
        );
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(10.0, 30.0));

        let next = run(
            &ctx,
            "pane",
            &passed((12.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY)),
        );
        assert_eq!(next.session, shares(12.0, 31.0));
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(12.0, 31.0));
    }

    /// Claude Code's config can name another account between two runs of one session, with
    /// `/login` as well as a switch, and the response the session had in between can have
    /// been on either. One on the account before whose five-hour window had just reset has a
    /// window no reading has, so nothing else shows whose it is.
    #[test]
    fn what_a_session_passes_as_the_account_named_changes_is_not_recorded() {
        let (ctx, _scratch) = machine("changed");
        crate::state::save(&ctx, &state()).expect("an account index");
        let works = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        crate::readings::remember(&ctx, &[("work-uuid".into(), works)]);
        sign_in(&ctx, "personal");
        open(&ctx, "pane");
        run(
            &ctx,
            "pane",
            &passed((100.0, NOW - HOUR), (50.0, NOW + 5 * DAY)),
        );

        sign_in(&ctx, "work");
        let changed = run(
            &ctx,
            "pane",
            &passed((2.0, NOW + 4 * HOUR), (51.0, NOW + 5 * DAY)),
        );
        assert_eq!(
            changed.session,
            shares(10.0, 30.0),
            "the pane shows work's own"
        );
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(10.0, 30.0));
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
        open(&ctx, "pane");

        let unread = passed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY));
        let shown = run(&ctx, "pane", &unread);
        assert_eq!(
            shown.session,
            shares(10.0, 30.0),
            "the pane shows work's own"
        );
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(10.0, 30.0));

        switched.accounts[0].last_used_at =
            Some(NOW - i64::from(crate::switch::ADOPTION_CEILING_SECONDS));
        crate::state::save(&ctx, &switched).expect("an account index");
        run(
            &ctx,
            "pane",
            &passed((12.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY)),
        );
        assert_eq!(
            shares_of(&recorded(&ctx), NOW),
            shares(12.0, 31.0),
            "and once they have, what they say is"
        );
    }

    /// The half minute starts in the second pitboard puts the account to use, and a session
    /// can run twice within it: once idle, as the config changes under it, and again with a
    /// response the account before served it.
    #[test]
    fn nothing_is_recorded_in_the_second_the_account_is_put_to_use() {
        let (ctx, _scratch) = machine("same-second");
        let mut switched = state();
        crate::state::save(&ctx, &switched).expect("an account index");
        let works = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        crate::readings::remember(&ctx, &[("work-uuid".into(), works)]);
        let personals = passed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY));
        sign_in(&ctx, "personal");
        run(&at(&ctx, NOW - 60), "pane", &personals);

        switched.accounts[0].last_used_at = Some(NOW);
        crate::state::save(&ctx, &switched).expect("an account index");
        sign_in(&ctx, "work");
        run(&ctx, "pane", &personals);
        run(
            &ctx,
            "pane",
            &passed((2.0, NOW + 5 * HOUR), (61.0, NOW + 5 * DAY)),
        );
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(10.0, 30.0));
    }

    /// A `/login` in Claude Code leaves pitboard no time to count from, and a session takes
    /// as long to follow it as a switch, so its next response can still be the account
    /// before's with the account after named both times. Where pitboard has read the
    /// account before, its windows show whose the response is.
    #[test]
    fn a_response_from_the_account_before_a_login_is_known_by_its_windows() {
        let (ctx, _scratch) = machine("login");
        crate::state::save(&ctx, &state()).expect("an account index");
        let works = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        crate::readings::remember(&ctx, &[("work-uuid".into(), works)]);
        sign_in(&ctx, "personal");
        open(&ctx, "pane");
        let personals = passed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY));
        run(&ctx, "pane", &personals);

        sign_in(&ctx, "work");
        run(&ctx, "pane", &personals);
        run(
            &ctx,
            "pane",
            &passed((96.0, NOW + 4 * HOUR), (61.0, NOW + 5 * DAY)),
        );
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(10.0, 30.0));
    }

    /// A window whose reset has passed says nothing about now, and a pane left open past it
    /// can be holding any account's. Recorded, it put the share of a window that was over
    /// into a reading, where the menu bar and the command line showed it.
    #[test]
    fn a_window_past_its_reset_is_not_recorded() {
        let (ctx, _scratch) = machine("passed");
        crate::state::save(&ctx, &state()).expect("an account index");
        open(&ctx, "pane");
        run(
            &ctx,
            "pane",
            &passed((90.0, NOW - 60), (40.0, NOW + 3 * DAY)),
        );
        let kinds: Vec<String> = recorded(&ctx).windows.into_iter().map(|w| w.kind).collect();
        assert_eq!(kinds, ["weekly_all"]);
    }

    /// A session can hold a response from just before its five-hour window reset, one that
    /// moved the weekly limit too, and run after the reset. Its numbers then leave the
    /// five-hour window out, and the reading lost it: the pane showed no five-hour share
    /// rather than none used, `pitboard status` and the menu bar lost the row, and with only
    /// the weekly window left to time it by, the app waited a hundred minutes to ask again
    /// rather than three.
    #[test]
    fn a_window_that_resets_stays_in_the_reading_with_nothing_used() {
        let (ctx, _scratch) = machine("reset");
        crate::state::save(&ctx, &state()).expect("an account index");
        let works = timed((40.0, NOW - 10), (30.0, NOW + 3 * DAY), NOW - HOUR);
        crate::readings::remember(&ctx, &[("work-uuid".into(), works)]);
        let floor = crate::budget::floor_for(Some(&recorded(&ctx)));
        run(
            &at(&ctx, NOW - 60),
            "pane",
            &passed((40.0, NOW - 10), (30.0, NOW + 3 * DAY)),
        );

        let shown = run(
            &ctx,
            "pane",
            &passed((41.0, NOW - 10), (31.0, NOW + 3 * DAY)),
        );
        assert_eq!(shown.session, shares(0.0, 31.0));
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(0.0, 31.0));
        let offline = crate::status::gather_offline(&ctx, &state());
        let row = offline
            .rows
            .iter()
            .find(|r| r.account_uuid == "work-uuid")
            .and_then(|r| r.usage.as_ref())
            .expect("work's numbers");
        let kinds: Vec<&str> = row.windows.iter().map(|w| w.kind.as_str()).collect();
        assert_eq!(kinds, ["session", "weekly_all"], "the row is kept");
        assert_eq!(crate::budget::floor_for(Some(&recorded(&ctx))), floor);
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
        run(
            &ctx,
            "pane",
            &passed((100.0, NOW - 60), (40.0, NOW + 3 * DAY)),
        );
        let shown = run(
            &ctx,
            "pane",
            &passed((2.0, NOW + 5 * HOUR), (41.0, NOW + 3 * DAY)),
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
        run(
            &ctx,
            "pane",
            &passed((20.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY)),
        );
        let shown = run(
            &ctx,
            "pane",
            &passed((25.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY)),
        );
        assert_eq!(shown.session, shares(25.0, 31.0));
        assert_eq!(shares_of(&recorded(&ctx), NOW), shares(25.0, 31.0));
    }
}
