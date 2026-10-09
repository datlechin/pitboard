//! `pitboard statusline`: one line for Claude Code's status bar, naming the account a session
//! is on and what every enrolled account has left.
//!
//! Claude Code runs it after every message with its session as JSON on stdin, and on a timer
//! too when its settings ask. So this reads only files, with no keychain and no network, and
//! writes two. The JSON holds how much of the session's five-hour and weekly limits is used
//! and when each resets, as its last response gave them, and names no account (the
//! register's `status_line_input_names_no_account`).
//!
//! Their windows say whose they are: an account's windows start when it is first used in
//! them. Where one account's reading, as Anthropic last answered for it, holds a window the
//! session passed and does not rule the session out, the session is on that account,
//! whatever Claude Code's config names and however long the session takes to follow a
//! switch. A reading rules it out with another running window of a limit the session passed,
//! or by listing every limit its account has and not that one. What moved since the
//! session's last run goes into that account's reading, where it moves a limit Anthropic
//! gave it, and the line shows that reading, so a session left open shows what the busy ones
//! have recorded since.
//!
//! Numbers that prove no account are shown as the session's own and filed nowhere, under the
//! account whose login Anthropic last said Claude Code has stored, marked as unsure, or under
//! none where that account's reading rules the session out. A session that passed nothing
//! shows that account's reading. Where that login is no enrolled account's, the line says so
//! in either case. Those last two are marked while nobody has asked Anthropic whose the
//! login is, or Claude Code's config has moved since. The other accounts show Pitboard's last
//! reading of them, with its age once that is worth knowing.
//!
//! It is Claude Code's status bar, so it is about Claude Code's accounts and nothing else.
//! The account a session is on is looked up among Claude Code's accounts only, and the others
//! listed are the ones this session could be switched to. A Codex account is neither,
//! whatever its label, its identity or its windows happen to be.

use crate::context::Context;
use crate::in_use::Known;
use crate::pace::Standing;
use crate::provider::ProviderId;
use crate::sessions::{Limit, Run};
use crate::state::{Account, State};
use crate::usage::{Snapshot, Window};
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

/// How fast the session's account is going on its five-hour and weekly limits, where that
/// means something. Only that account: the others are parked, and not being used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Paces {
    pub five_hour: Option<Standing>,
    pub weekly: Option<Standing>,
}

/// An enrolled account other than the session's.
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub label: String,
    pub shares: Shares,
    /// Seconds since this was measured, once that is worth knowing.
    pub age: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub struct StatusLine {
    /// The account the session is on: the one its numbers prove, or else the one whose login
    /// Anthropic last said Claude Code has stored. `None` where that is no enrolled account,
    /// or where its reading rules the session out: it holds another running window of a
    /// limit the session passed, or lists every limit of its account and not that one.
    pub current: Option<String>,
    /// Whether `current` is known to be right. It is where the session's numbers prove it.
    /// Otherwise it is where nothing has put in doubt whose login Anthropic last said Claude
    /// Code has stored, and either the session passed no numbers, or that login is no
    /// enrolled account's and the numbers prove no enrolled account either.
    pub sure: bool,
    /// The session's own account: Pitboard's reading of it, with what the session moved
    /// folded in, or what the session passed where its numbers prove no account or Pitboard
    /// has no reading of it.
    pub session: Shares,
    /// The session's own account's pace on each of those limits.
    pub pace: Paces,
    pub others: Vec<Entry>,
}

/// What the line calls its two limits, by the names a reading can give each.
const FIVE_HOUR: &[&str] = &["session", "five_hour"];
const WEEKLY: &[&str] = &["weekly_all", "seven_day"];

/// The limit among `windows` called one of `kinds`, for every model.
fn limit<'a>(windows: &'a [Window], kinds: &[&str]) -> Option<&'a Window> {
    windows
        .iter()
        .find(|w| w.scope.is_none() && kinds.contains(&w.kind.as_str()))
}

/// The five-hour and weekly shares among `windows`.
fn shares_of(windows: &[Window], now: i64) -> Shares {
    let share = |kinds| limit(windows, kinds).map(|w| w.used(now));
    Shares {
        five_hour: share(FIVE_HOUR),
        weekly: share(WEEKLY),
    }
}

/// The five-hour and weekly paces among `windows`, as of `at`, when they were read.
fn paces_of(windows: &[Window], at: i64, now: i64) -> Paces {
    let pace = |kinds| Some(limit(windows, kinds)?.pace(at, now)?.standing);
    Paces {
        five_hour: pace(FIVE_HOUR),
        weekly: pace(WEEKLY),
    }
}

/// What a session's numbers say of the account it is on.
enum Whose<'a> {
    /// That account: its reading, as Anthropic answered it, holds a window the session passed
    /// and does not rule the session out ([`contradicts`]), and no other account's does.
    Proven(&'a Account),
    /// No account, or more than one.
    Nobody,
    /// Nothing: the session passed no window still running.
    Unsaid,
}

/// The windows among `passed` still running at `now`. One past its reset says nothing about
/// now: a session left open past it can be holding any account's.
fn running(passed: &[Window], now: i64) -> impl Iterator<Item = &Window> {
    passed
        .iter()
        .filter(move |window| window.resets_at.is_some_and(|at| at > now))
}

/// `account`'s reading where Anthropic has answered for it. One an older Pitboard wrote says
/// nothing of that, and can hold numbers it filed under the wrong account, so its windows
/// prove nothing.
fn answered<'a>(
    remembered: &'a HashMap<String, Snapshot>,
    account: &Account,
) -> Option<&'a Snapshot> {
    remembered
        .get(&account.id)
        .filter(|reading| reading.answered_at.is_some())
}

/// Whether `reading` holds a window of `passed` that is still running at `now`.
fn holds(reading: &Snapshot, passed: &[Window], now: i64) -> bool {
    running(passed, now).any(|given| reading.windows.iter().any(|had| had.same_window(given)))
}

/// Whether `reading` rules out that a session which passed `passed` is on its account, by a
/// limit whose window in `passed` is still running at `now`: it holds another running
/// window of that limit, which a session on its account would have passed, or it lists every
/// limit its account has and not that one, which the account then does not have, as
/// [`crate::usage::room`] counts it.
fn contradicts(reading: &Snapshot, passed: &[Window], now: i64) -> bool {
    running(passed, now).any(|given| {
        let theirs = || reading.windows.iter().filter(|had| had.same_limit(given));
        let elsewhere =
            theirs().any(|had| !had.same_window(given) && had.resets_at.is_some_and(|at| at > now));
        elsewhere || (reading.lists_every_limit && theirs().next().is_none())
    })
}

/// The account a session that passed `passed` is on, by the windows Anthropic last gave each
/// of Claude Code's accounts. A run's windows come from one response, so where one is an
/// account's own, the others are that account's too, though one may be a window no reading
/// holds yet: the next one of a limit whose last has reset.
fn whose<'a>(
    state: &'a State,
    remembered: &HashMap<String, Snapshot>,
    passed: &[Window],
    now: i64,
) -> Whose<'a> {
    if running(passed, now).next().is_none() {
        return Whose::Unsaid;
    }
    let mut theirs = state
        .accounts
        .iter()
        .filter(|a| a.provider() == ProviderId::Claude)
        .filter(|a| {
            answered(remembered, a).is_some_and(|reading| {
                holds(reading, passed, now) && !contradicts(reading, passed, now)
            })
        });
    match (theirs.next(), theirs.next()) {
        (Some(account), None) => Whose::Proven(account),
        _ => Whose::Nobody,
    }
}

/// The line for a session whose numbers say `whose` of it. `in_use` is whose login Claude
/// Code has stored, as files say. `passed` is every limit the session passed, and `moved`
/// those of them that moved since its last run.
fn line(
    state: &State,
    in_use: &Known,
    remembered: &HashMap<String, Snapshot>,
    whose: &Whose,
    passed: &[Window],
    moved: &[Window],
    now: i64,
) -> StatusLine {
    // Claude Code runs this, so the line is about Claude Code's accounts. Another tool's
    // account is not one this session could be on.
    let stored = in_use
        .owner()
        .and_then(|owner| state.account_of(ProviderId::Claude, owner));
    let (current, sure) = match (whose, stored) {
        (Whose::Proven(account), _) => (Some(*account), true),
        // A session on a login that is no enrolled account's passes numbers that prove none.
        (Whose::Unsaid, _) | (Whose::Nobody, None) => (stored, in_use.doubt.is_none()),
        (Whose::Nobody, Some(account)) => {
            let elsewhere = answered(remembered, account)
                .is_some_and(|reading| contradicts(reading, passed, now));
            ((!elsewhere).then_some(account), false)
        }
    };
    let others = state
        .accounts
        .iter()
        .filter(|a| a.provider() == ProviderId::Claude)
        .filter(|a| current.is_none_or(|c| c.id != a.id))
        .map(|account| {
            let reading = remembered.get(&account.id);
            Entry {
                label: account.label.clone(),
                shares: reading.map_or_else(Shares::default, |r| shares_of(&r.windows, now)),
                age: reading
                    .and_then(|r| r.observed_at)
                    .map(|at| now - at)
                    .filter(|age| *age > FRESH_FOR),
            }
        })
        .collect();
    let known = match whose {
        Whose::Nobody => None,
        Whose::Proven(_) | Whose::Unsaid => current.and_then(|a| remembered.get(&a.id)),
    };
    let folded = known.and_then(|known| crate::usage::moved(known, moved, now));
    let (windows, at) = match folded.as_ref().or(known) {
        Some(reading) => (
            reading.windows.as_slice(),
            reading.observed_at.unwrap_or(now),
        ),
        // Numbers that prove no account, or an account Pitboard has no reading of: what the
        // session passed is all there is to show, and it is the session's latest.
        None => (passed, now),
    };
    StatusLine {
        current: current.map(|a| a.label.clone()),
        sure,
        session: shares_of(windows, now),
        pace: paces_of(windows, at, now),
        others,
    }
}

/// One limit a session passed, as a window under the name Claude Code gives it. A session
/// says how much is used and when the window resets, and nothing else.
fn window(name: &str, limit: &Limit) -> Window {
    Window {
        kind: name.to_owned(),
        scope: None,
        percent: limit.used_percentage,
        resets_at: limit.resets_at,
        is_active: false,
        severity: None,
        length_seconds: crate::usage::anthropic_window_length(name),
    }
}

/// Every limit a run passed, as windows.
fn windows_of(run: &Run) -> Vec<Window> {
    run.limits
        .iter()
        .map(|(name, limit)| window(name, limit))
        .collect()
}

/// What this run of a session's status line was given: each limit the session passed.
fn run_of(input: &Value) -> Run {
    let limits = input.get("rate_limits");
    Run {
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

/// The limits `run` passed that moved since `before`, this session's run before it, as
/// windows to file ([`crate::usage::moved`] says what a reading takes of them).
///
/// A session passes the numbers of its last response every time its status line runs, and
/// one left idle passes the same ones for as long as it stays open. A share in them can be
/// older than an answer that lowered it since, as a banked reset on claude.ai does. A limit
/// that moved came with a response the session got since `before`, so only such a limit is
/// filed, and nothing from a session not seen before, which has nothing to compare with.
fn moved_since(run: &Run, before: Option<&Run>) -> Vec<Window> {
    let Some(before) = before else {
        return Vec::new();
    };
    run.limits
        .iter()
        .filter(|(name, limit)| before.limits.get(*name) != Some(*limit))
        .map(|(name, limit)| window(name, limit))
        .collect()
}

/// Reads Claude Code's session JSON. Never fails: a status bar has nowhere to show an
/// error, so whatever cannot be read is left out.
///
/// It also files what moved since the session's last run under the account its windows
/// prove it is on, where it moves a limit Anthropic gave that account. So the accounts a
/// person works in move between Pitboard's answers without anyone running `pitboard` by
/// hand, and every session and the menu bar show the newest numbers any of them has seen. It
/// still asks nobody anything: no network, no credential.
///
/// Without a `permit`, which a run as root or under sudo is not given, it writes nothing:
/// the session's last run is read and this one is not kept, and nothing is filed. The line
/// is drawn the same way from the files as they are.
pub fn read(ctx: &Context, permit: Option<crate::service::Permit>, input: &str) -> StatusLine {
    let input: Value = serde_json::from_str(input).unwrap_or(Value::Null);
    let state = crate::state::load(ctx).unwrap_or_default();
    // Whose login Claude Code has stored, as Anthropic last said, and whether its config has
    // moved since: files, and so something a status bar can afford to read after every
    // message.
    let in_use = crate::in_use::known(ctx, &state, ProviderId::Claude);
    let now = ctx.now();
    let remembered = crate::readings::load(ctx);
    let run = run_of(&input);
    let before = input
        .get("session_id")
        .and_then(Value::as_str)
        .and_then(|id| match permit {
            Some(permit) => crate::sessions::exchange(ctx, permit, id, &run),
            None => crate::sessions::last(ctx, id),
        });
    let passed = windows_of(&run);
    let moved = moved_since(&run, before.as_ref());
    let whose = whose(&state, &remembered, &passed, now);
    if let (Some(permit), Whose::Proven(account)) = (permit, &whose)
        && !moved.is_empty()
    {
        crate::readings::moved(ctx, permit, &account.id, &moved);
    }
    line(&state, &in_use, &remembered, &whose, &passed, &moved, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Owner;
    use crate::in_use::InUse;
    use crate::service::Permit;
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
            replaced_at: None,
            label: label.into(),
            id: format!("{label}-uuid"),
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

    /// Anthropic's answer about the five-hour and weekly limits, each as its share and when it
    /// resets, taken at `observed_at`.
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
            answered_at: Some(observed_at),
            lists_every_limit: true,
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

    /// `label`'s login, as one of [`state`]'s accounts holds it.
    fn owner(label: &str) -> Owner {
        Owner {
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            organization_uuid: "o".into(),
        }
    }

    /// What files say of whose login Claude Code has stored once Anthropic has named
    /// `owner`'s, with nothing moved since. `None`: nothing was ever recorded.
    fn known(owner: Option<Owner>) -> Known {
        let Some(owner) = owner else {
            return Known {
                last: None,
                doubt: Some(crate::in_use::Doubt::NeverEstablished),
                own_record: None,
            };
        };
        Known {
            last: Some(InUse {
                named: Some(crate::state::new_id(ProviderId::Claude, &owner)),
                owner: Some(owner.clone()),
                login: "refresh".into(),
                known_at: NOW,
            }),
            doubt: None,
            own_record: Some(owner),
        }
    }

    /// The line for a session that passed nothing, with no reading of any account.
    fn passing_nothing(accounts: &State, in_use: &Known) -> StatusLine {
        line(
            accounts,
            in_use,
            &HashMap::new(),
            &Whose::Unsaid,
            &[],
            &[],
            NOW,
        )
    }

    /// The line a session draws from `input` just after a response, with `work`'s login the
    /// one Claude Code has stored: its run before passed nothing, so everything it passes has
    /// moved.
    fn after_a_response(input: &Value, remembered: &HashMap<String, Snapshot>) -> StatusLine {
        let accounts = state();
        let run = run_of(input);
        let passed = windows_of(&run);
        let moved = moved_since(&run, Some(&Run::default()));
        let whose = whose(&accounts, remembered, &passed, NOW);
        line(
            &accounts,
            &known(Some(owner("work"))),
            remembered,
            &whose,
            &passed,
            &moved,
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

    /// Claude Code's config naming `label`'s account as signed in, as a `/login` leaves it,
    /// with nobody having asked Anthropic since.
    fn sign_in(ctx: &Context, label: &str) {
        names(ctx, &owner(label));
    }

    /// Claude Code's config naming `owner`'s account.
    fn names(ctx: &Context, owner: &Owner) {
        std::fs::write(
            ctx.home().join(".claude.json"),
            json!({"oauthAccount": {
                "accountUuid": owner.account_uuid,
                "emailAddress": owner.email,
                "organizationUuid": owner.organization_uuid,
            }})
            .to_string(),
        )
        .expect("a Claude Code config");
    }

    /// `label`'s login put in use as a switch to it leaves things: Anthropic said at `ctx`'s
    /// time that it is the login Claude Code has stored, and Claude Code's config names it.
    fn using(ctx: &Context, label: &str) {
        let accounts = crate::state::load(ctx).expect("an account index");
        let owner = accounts
            .accounts
            .iter()
            .find(|a| a.label == label)
            .expect("an enrolled account")
            .owner();
        stores(ctx, owner);
    }

    /// `owner`'s login stored, enrolled or not: Anthropic said so at `ctx`'s time, and Claude
    /// Code's config names it.
    fn stores(ctx: &Context, owner: Owner) {
        let mut accounts = crate::state::load(ctx).expect("an account index");
        names(ctx, &owner);
        let found = InUse {
            login: format!("{}-refresh", owner.account_uuid),
            owner: Some(owner),
            known_at: ctx.now(),
            named: crate::in_use::named(ctx, ProviderId::Claude),
        };
        accounts.identified(ProviderId::Claude, found, ctx.now());
        crate::state::save(ctx, Permit::for_a_test(), &accounts).expect("an account index");
    }

    /// Session `id`'s status line run with `input`, as Claude Code runs it.
    fn run(ctx: &Context, id: &str, input: &Value) -> StatusLine {
        let mut input = input.clone();
        input["session_id"] = json!(id);
        read(ctx, Some(Permit::for_a_test()), &input.to_string())
    }

    /// Session `id` starting: its status line runs before it has had a response, with no
    /// limits to pass.
    fn open(ctx: &Context, id: &str) {
        run(ctx, id, &json!({}));
    }

    fn recorded(ctx: &Context) -> Snapshot {
        recorded_for(ctx, "work").expect("a reading")
    }

    fn recorded_for(ctx: &Context, label: &str) -> Option<Snapshot> {
        crate::readings::load(ctx).remove(&format!("{label}-uuid"))
    }

    /// What Claude Code passes a session on a plan whose sessions pass the five-hour limit
    /// alone.
    fn five_hour_alone(used: f64, resets_at: i64) -> Value {
        json!({"rate_limits": {
            "five_hour": {"used_percentage": used, "resets_at": resets_at}
        }})
    }

    #[test]
    fn names_the_account_in_use_and_what_every_account_has_left() {
        let input = json!({"rate_limits": {
            "five_hour": {"used_percentage": 46.4, "resets_at": NOW + 600},
            "seven_day": {"used_percentage": 70.0, "resets_at": NOW + DAY}
        }});
        let remembered = HashMap::from([
            (
                "work-uuid".to_string(),
                timed((40.0, NOW + 600), (65.0, NOW + DAY), NOW - 60),
            ),
            (
                "personal-uuid".to_string(),
                reading(12.0, 40.0, NOW - 60, NOW + 2 * HOUR),
            ),
            (
                "side-uuid".to_string(),
                reading(90.0, 88.0, NOW - 3 * 3_600, NOW - 1),
            ),
        ]);
        let line = after_a_response(&input, &remembered);
        assert_eq!(line.current.as_deref(), Some("work"));
        assert!(line.sure, "its windows are work's");
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

    /// The account in use says how fast it is going on each limit, from its numbers and the
    /// clock: a five-hour limit at 46% ten minutes from its reset is under pace, and a week at
    /// 70% two days in is over. Nothing is said with nothing to go on.
    #[test]
    fn the_account_in_use_says_its_pace_on_each_limit() {
        let input = json!({"rate_limits": {
            "five_hour": {"used_percentage": 46.4, "resets_at": NOW + 600},
            "seven_day": {"used_percentage": 70.0, "resets_at": NOW + 5 * DAY}
        }});
        let shown = after_a_response(&input, &HashMap::new());
        assert_eq!(shown.pace.five_hour, Some(Standing::Under));
        assert!(
            matches!(shown.pace.weekly, Some(Standing::Over { .. })),
            "{:?}",
            shown.pace
        );
        assert_eq!(
            passing_nothing(&state(), &known(None)).pace,
            Paces::default()
        );
    }

    #[test]
    fn what_is_unknown_is_left_unknown_not_zero() {
        let line = passing_nothing(&state(), &known(None));
        assert_eq!(line.current, None);
        assert!(!line.sure, "nobody has asked whose login is stored");
        assert_eq!(line.session, Shares::default());
        assert!(line.others.iter().all(|e| e.shares == Shares::default()));
        assert_eq!(line.others.len(), 3);
    }

    /// Claude Code runs this, so the line is about Claude Code's accounts. A Codex account
    /// whose identity matches the one in use, or whose reading holds the session's windows,
    /// is not the session's account, and one that shares a label is not an account this
    /// session could be switched to.
    #[test]
    fn another_tools_accounts_are_not_on_claude_codes_line() {
        let codex = |label: &str, uuid: &str| Account {
            last_used_at: None,
            replaced_at: None,
            label: label.into(),
            id: uuid.into(),
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

        let found = passing_nothing(&s, &known(Some(owner("work"))));
        assert_eq!(found.current.as_deref(), Some("work"));
        let others: Vec<&str> = found.others.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(others, ["personal", "side"]);

        let codex_work = s.accounts.last().expect("codex/work").owner();
        let found = passing_nothing(&s, &known(Some(codex_work)));
        assert_eq!(
            found.current, None,
            "a Codex identity names no Claude Code account"
        );

        let reading = timed((20.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        let windows = reading.windows.clone();
        let remembered = HashMap::from([("codex-work".to_string(), reading)]);
        let shown = line(
            &s,
            &known(Some(owner("work"))),
            &remembered,
            &whose(&s, &remembered, &windows, NOW),
            &windows,
            &windows,
            NOW,
        );
        assert_eq!(shown.current.as_deref(), Some("work"));
        assert!(
            !shown.sure,
            "a Codex reading proves nothing of a Claude Code session"
        );
        let others: Vec<&str> = shown.others.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(others, ["personal", "side"]);
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
            after_a_response(input, remembered).session
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

    /// A login is one person in one organisation, and the account in use is the one whose
    /// login Anthropic last said Claude Code has stored, with the person's other one beside
    /// it. Claude Code's config naming the other organisation since, as a process started on
    /// it leaves the config, does not change which account that is. The label says it may
    /// have, until a read asks again.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn the_account_in_use_is_the_one_anthropic_last_named() {
        let (ctx, _scratch) = machine("organisations");
        let mut accounts = state();
        accounts.upsert(Account {
            last_used_at: None,
            replaced_at: None,
            label: "team".into(),
            id: "work-uuid_team-org".into(),
            account_uuid: "work-uuid".into(),
            email: "work@example.com".into(),
            detail: crate::state::Detail::Claude {
                organization_uuid: "team-org".into(),
                oauth_account: json!({}),
            },
            parked: None,
        });
        crate::state::save(&ctx, Permit::for_a_test(), &accounts).expect("an account index");
        using(&ctx, "team");

        let shown = run(&ctx, "pane", &json!({}));
        assert_eq!(shown.current.as_deref(), Some("team"));
        assert!(shown.sure);
        let others: Vec<&str> = shown.others.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(others, ["work", "personal", "side"]);

        sign_in(&ctx, "work");
        let moved = run(&ctx, "pane", &json!({}));
        assert_eq!(moved.current.as_deref(), Some("team"));
        assert!(!moved.sure);
    }

    /// Offered whenever a response moves it, however recently something was recorded, and
    /// kept only when it is newer. It used to be recorded only when what was remembered was
    /// a quarter of an hour old, and then over whatever was there.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn the_sessions_numbers_are_recorded_when_they_are_newer() {
        let (ctx, _scratch) = machine("records");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
            &[("work-uuid".into(), reading(22.0, 6.0, NOW - 60, NOW + 600))],
        );
        run(&ctx, "busy", &session(22.0, 6.0, NOW + 600));
        run(&ctx, "slow", &session(18.0, 4.0, NOW + 600));

        let busy = run(&ctx, "busy", &session(25.0, 7.0, NOW + 600));
        assert_eq!(busy.session, shares(25.0, 7.0));
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(25.0, 7.0));

        let slow = run(&ctx, "slow", &session(20.0, 5.0, NOW + 600));
        assert_eq!(slow.session, shares(25.0, 7.0));
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(25.0, 7.0));
    }

    /// A session knows how much is used and when it resets. Which limit the account is
    /// working against is Anthropic's to say, and the menu bar shows the limit it names:
    /// recorded as a session's guess, a weekly limit at 70% took the menu bar from the
    /// five-hour limit that binds.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_session_moves_the_numbers_and_leaves_what_only_anthropic_says() {
        let (ctx, _scratch) = machine("leaves");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let mut answered = reading(22.0, 6.0, NOW - 60, NOW + 600);
        answered.windows[0].severity = Some("normal".into());
        answered.windows[0].is_active = true;
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
            &[("work-uuid".into(), answered)],
        );

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

    /// Forgetting an account takes its reading with it, and that reading was all that showed
    /// its windows to be its own. A session still on it proves nobody once it is gone, and
    /// what it passes is filed under no other account, the one in use included.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_session_holding_a_forgotten_accounts_numbers_is_not_recorded_as_the_one_in_use() {
        let (ctx, _scratch) = machine("forgotten");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
            &[
                (
                    "work-uuid".into(),
                    timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60),
                ),
                (
                    "personal-uuid".into(),
                    timed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY), NOW - 60),
                ),
            ],
        );
        using(&at(&ctx, NOW - HOUR), "work");
        open(&ctx, "pane");
        let on_personal = run(
            &ctx,
            "pane",
            &passed((96.0, NOW + 4 * HOUR), (61.0, NOW + 5 * DAY)),
        );
        assert_eq!(on_personal.current.as_deref(), Some("personal"));

        let mut accounts = crate::state::load(&ctx).expect("an account index");
        accounts.accounts.retain(|a| a.label != "personal");
        crate::state::save(&ctx, Permit::for_a_test(), &accounts).expect("an account index");
        crate::readings::forget(&ctx, Permit::for_a_test(), "personal-uuid");

        let forgotten = run(
            &ctx,
            "pane",
            &passed((97.0, NOW + 4 * HOUR), (62.0, NOW + 5 * DAY)),
        );
        assert_eq!(forgotten.current, None, "work is in other windows");
        assert_eq!(forgotten.session, shares(97.0, 62.0));
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(10.0, 30.0));
    }

    /// A session Pitboard has not seen before passes the numbers of whatever response it had
    /// last, and within a window those can be older than an answer that lowered a share
    /// since, as a banked reset does. They are left out, and what its next response moves is
    /// filed.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_session_seen_for_the_first_time_offers_nothing_until_its_next_response() {
        let (ctx, _scratch) = machine("first");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let works = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        crate::readings::answered(&ctx, Permit::for_a_test(), &[("work-uuid".into(), works)]);

        let first = run(
            &ctx,
            "pane",
            &passed((50.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY)),
        );
        assert_eq!(first.current.as_deref(), Some("work"));
        assert_eq!(
            first.session,
            shares(10.0, 30.0),
            "the pane shows work's own"
        );
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(10.0, 30.0));

        let next = run(
            &ctx,
            "pane",
            &passed((51.0, NOW + 2 * HOUR), (32.0, NOW + 3 * DAY)),
        );
        assert_eq!(next.session, shares(51.0, 32.0));
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(51.0, 32.0));
    }

    /// Pitboard has no reading of an account it has not asked Anthropic about, so a session's
    /// numbers prove no account and are filed nowhere. The pane shows what its session
    /// passed, from its first run on and while it passes the same again, where it drew
    /// nothing at all.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_session_on_an_account_pitboard_has_not_read_shows_its_own_numbers() {
        let (ctx, _scratch) = machine("unread");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let numbers = passed((12.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY));
        assert_eq!(
            run(&ctx, "new", &numbers).session,
            shares(12.0, 31.0),
            "a session not seen before"
        );
        open(&ctx, "pane");
        for _ in 0..2 {
            assert_eq!(run(&ctx, "pane", &numbers).session, shares(12.0, 31.0));
        }
        assert!(crate::readings::load(&ctx).is_empty());
    }

    /// A `/login` in Claude Code rewrites its config at once, and a session takes as long to
    /// follow it as a switch, so its next response can still be the account before's with
    /// the account after named. The windows show whose the response is: work's five-hour
    /// window is over, and the one passed is personal's, not work's next.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_response_from_the_account_before_a_login_is_known_by_its_windows() {
        let (ctx, _scratch) = machine("login");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
            &[
                (
                    "work-uuid".into(),
                    timed((10.0, NOW - 60), (30.0, NOW + 3 * DAY), NOW - HOUR),
                ),
                (
                    "personal-uuid".into(),
                    timed((95.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY), NOW - 60),
                ),
            ],
        );
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
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(0.0, 30.0));
        assert_eq!(
            recorded_for(&ctx, "personal").map(|reading| shares_of(&reading.windows, NOW)),
            Some(shares(96.0, 61.0))
        );
    }

    /// A window whose reset has passed says nothing about now, and a pane left open past it
    /// can be holding any account's. Recorded, it put the share of a window that was over
    /// into a reading, where the menu bar and the command line showed it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_window_past_its_reset_is_not_recorded() {
        let (ctx, _scratch) = machine("passed");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let works = timed((50.0, NOW - 60), (30.0, NOW + 3 * DAY), NOW - HOUR);
        crate::readings::answered(&ctx, Permit::for_a_test(), &[("work-uuid".into(), works)]);
        open(&ctx, "pane");
        run(
            &ctx,
            "pane",
            &passed((90.0, NOW - 60), (40.0, NOW + 3 * DAY)),
        );
        let said: Vec<f64> = recorded(&ctx).windows.iter().map(|w| w.percent).collect();
        assert_eq!(said, [50.0, 40.0]);
    }

    /// A session can hold a response from just before its five-hour window reset, one that
    /// moved the weekly limit too, and run after the reset. Its numbers then leave the
    /// five-hour window out, and the reading lost it: the pane showed no five-hour share
    /// rather than none used, `pitboard status` and the menu bar lost the row, and with only
    /// the weekly window left to time it by, the app waited a hundred minutes to ask again
    /// rather than three.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_window_that_resets_stays_in_the_reading_with_nothing_used() {
        let (ctx, _scratch) = machine("reset");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let works = timed((40.0, NOW - 10), (30.0, NOW + 3 * DAY), NOW - HOUR);
        crate::readings::answered(&ctx, Permit::for_a_test(), &[("work-uuid".into(), works)]);
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
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(0.0, 31.0));
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

    /// A session goes on with the login it holds after a switch: for up to half a minute, or
    /// until its login is next renewed while a file sits behind the keychain. What it passes
    /// is in the windows of the account before, and is recorded for that account however
    /// long it takes. Recorded as the account switched to, as Claude Code's config names it
    /// at once, its later resets stood over every answer Anthropic gave about that one.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_session_still_on_the_account_before_a_switch_goes_on_recording_it() {
        let (ctx, _scratch) = machine("before-a-switch");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let personals = timed((50.0, NOW + 4 * HOUR), (60.0, NOW + 5 * DAY), NOW - 60);
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
            &[
                (
                    "work-uuid".into(),
                    timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60),
                ),
                ("personal-uuid".into(), personals.clone()),
            ],
        );
        using(&at(&ctx, NOW - HOUR), "work");
        run(
            &ctx,
            "pane",
            &passed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY)),
        );

        using(&ctx, "personal");
        for (after, five_hour, weekly) in [(10, 12.0, 31.0), (10 * 60, 14.0, 32.0)] {
            let shown = run(
                &at(&ctx, NOW + after),
                "pane",
                &passed((five_hour, NOW + 2 * HOUR), (weekly, NOW + 3 * DAY)),
            );
            assert_eq!(shown.current.as_deref(), Some("work"), "{after} s on");
            assert!(shown.sure, "{after} s on");
            assert_eq!(shown.session, shares(five_hour, weekly));
            assert_eq!(
                shares_of(&recorded(&ctx).windows, NOW),
                shares(five_hour, weekly),
                "{after} s on"
            );
        }
        assert_eq!(
            recorded_for(&ctx, "personal"),
            Some(personals),
            "the account switched to keeps its own"
        );
    }

    /// Numbers in windows no account's answered reading holds prove nobody: a session on an
    /// account nobody enrolled, as Claude Code's own `/login` can leave one, or on one
    /// Pitboard has not asked Anthropic about. They are shown as the session's and filed
    /// nowhere. The label is the account in use's, marked, unless that account's reading
    /// rules the session out, as other windows of those limits do.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_window_no_account_holds_is_shown_for_the_session_and_filed_nowhere() {
        let (ctx, _scratch) = machine("nobodys");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        using(&at(&ctx, NOW - HOUR), "work");
        open(&ctx, "pane");
        let unread = run(
            &ctx,
            "pane",
            &passed((97.0, NOW + 4 * HOUR), (80.0, NOW + 6 * DAY)),
        );
        assert_eq!(unread.current.as_deref(), Some("work"));
        assert!(!unread.sure);
        assert_eq!(unread.session, shares(97.0, 80.0));
        assert!(crate::readings::load(&ctx).is_empty());

        let works = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
            &[("work-uuid".into(), works.clone())],
        );
        let elsewhere = run(
            &ctx,
            "pane",
            &passed((98.0, NOW + 4 * HOUR), (81.0, NOW + 6 * DAY)),
        );
        assert_eq!(elsewhere.current, None, "work is in other windows");
        assert!(!elsewhere.sure);
        assert_eq!(elsewhere.session, shares(98.0, 81.0));
        assert_eq!(
            crate::readings::load(&ctx),
            HashMap::from([("work-uuid".to_string(), works)])
        );
    }

    /// Where the login Anthropic last named is one nobody enrolled, as Claude Code's own
    /// `/login` can leave it, numbers that prove no enrolled account say nothing against it.
    /// The line says `unenrolled` with the session's own numbers, as it did before the session
    /// passed any, and files them nowhere. Once Claude Code's config has moved since, it can
    /// name nobody.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_session_on_a_login_nobody_enrolled_stays_unenrolled() {
        let (ctx, _scratch) = machine("guest");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let works = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
            &[("work-uuid".into(), works.clone())],
        );
        stores(&at(&ctx, NOW - HOUR), owner("guest"));

        let opened = run(&ctx, "pane", &json!({}));
        assert_eq!((opened.current, opened.sure), (None, true));

        let shown = run(
            &ctx,
            "pane",
            &passed((3.0, NOW + 4 * HOUR), (31.0, NOW + 6 * DAY)),
        );
        assert_eq!(shown.current, None);
        assert!(
            shown.sure,
            "nothing puts the login Anthropic named in doubt"
        );
        assert_eq!(shown.session, shares(3.0, 31.0));
        assert_eq!(
            crate::readings::load(&ctx),
            HashMap::from([("work-uuid".to_string(), works)])
        );

        sign_in(&ctx, "personal");
        let moved = run(
            &ctx,
            "pane",
            &passed((4.0, NOW + 4 * HOUR), (32.0, NOW + 6 * DAY)),
        );
        assert_eq!((moved.current, moved.sure), (None, false));
        assert_eq!(moved.session, shares(4.0, 32.0));
    }

    /// A session's two windows come from one response, so where one is an account's own the
    /// other is too. An account's next five-hour window has a reset no reading holds yet, as
    /// its windows start when it is first used in them: it is taken on the word of the
    /// weekly window passed with it. A five-hour window passed alone, which no reading holds,
    /// is nobody's.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_new_five_hour_window_is_taken_on_the_weekly_windows_word() {
        let (ctx, _scratch) = machine("next");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
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
        using(&at(&ctx, NOW - HOUR), "work");
        open(&ctx, "seat");
        for used in [2.0, 3.0] {
            let alone = run(&ctx, "seat", &five_hour_alone(used, NOW + 5 * HOUR));
            assert!(!alone.sure);
        }
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(0.0, 40.0));

        using(&ctx, "personal");
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
        assert_eq!(shown.current.as_deref(), Some("work"));
        assert_eq!(shown.session, shares(2.0, 41.0));
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(2.0, 41.0));
    }

    /// Two accounts' windows can reset within a minute of each other. Numbers in a window
    /// both hold prove neither, and are filed under neither. A weekly window passed with them
    /// that only one holds tells them apart.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn two_accounts_holding_one_window_file_nothing() {
        let (ctx, _scratch) = machine("coincident");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let works = timed((20.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        let personals = timed((50.0, NOW + 2 * HOUR + 30), (60.0, NOW + 5 * DAY), NOW - 60);
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
            &[
                ("work-uuid".into(), works.clone()),
                ("personal-uuid".into(), personals.clone()),
            ],
        );
        using(&at(&ctx, NOW - HOUR), "work");
        run(&ctx, "pane", &five_hour_alone(55.0, NOW + 2 * HOUR));
        let either = run(&ctx, "pane", &five_hour_alone(56.0, NOW + 2 * HOUR));
        assert!(!either.sure);
        assert_eq!(
            crate::readings::load(&ctx),
            HashMap::from([
                ("work-uuid".to_string(), works),
                ("personal-uuid".to_string(), personals),
            ])
        );

        let told = run(
            &ctx,
            "pane",
            &passed((25.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY)),
        );
        assert_eq!(told.current.as_deref(), Some("work"));
        assert!(told.sure);
        assert_eq!(shares_of(&recorded(&ctx).windows, NOW), shares(25.0, 31.0));
    }

    /// An answer lists every limit its account has, so a Team seat whose answer has no
    /// weekly limit for all models does not have one. A session that passes one is not on
    /// the seat, though its five-hour window resets within a minute of the seat's. Filed
    /// under the seat in use, a Max session's share was what the automatic switch judged.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_session_passing_a_limit_an_account_does_not_have_is_not_on_it() {
        let (ctx, _scratch) = machine("seat");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let limit = |kind: &str, scope: Option<&str>, percent: f64, resets_at: i64| Window {
            kind: kind.into(),
            scope: scope.map(str::to_owned),
            percent,
            resets_at: Some(resets_at),
            is_active: false,
            severity: None,
            length_seconds: None,
        };
        let seats = Snapshot {
            windows: vec![
                limit("session", None, 20.0, NOW + 2 * HOUR + 30),
                limit("weekly_scoped", Some("Fable"), 10.0, NOW + 4 * DAY),
            ],
            observed_at: Some(NOW - 60),
            answered_at: Some(NOW - 60),
            lists_every_limit: true,
            source: Source::Remembered,
        };
        crate::readings::answered(
            &ctx,
            Permit::for_a_test(),
            &[("work-uuid".into(), seats.clone())],
        );
        using(&at(&ctx, NOW - HOUR), "work");
        open(&ctx, "max");
        for (five_hour, weekly) in [(60.0, 40.0), (61.0, 41.0)] {
            let shown = run(
                &ctx,
                "max",
                &passed((five_hour, NOW + 2 * HOUR), (weekly, NOW + 4 * DAY)),
            );
            assert_eq!(
                shown.current, None,
                "work has no weekly limit for all models"
            );
            assert!(!shown.sure);
            assert_eq!(shown.session, shares(five_hour, weekly));
        }
        assert_eq!(recorded(&ctx), seats);

        open(&ctx, "seat");
        let own = run(&ctx, "seat", &five_hour_alone(21.0, NOW + 2 * HOUR));
        assert_eq!(own.current.as_deref(), Some("work"));
        assert!(own.sure);
        assert_eq!(recorded(&ctx).windows[0].percent, 21.0);
    }

    /// A reading an older Pitboard wrote can hold numbers it filed under the wrong account,
    /// and does not say when Anthropic last answered for that account. Its windows prove
    /// nothing, so nothing is filed until Pitboard has asked Anthropic about that account.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_reading_an_older_pitboard_wrote_is_matched_against_nothing() {
        let (ctx, _scratch) = machine("older");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        let mut older = timed((10.0, NOW + 2 * HOUR), (30.0, NOW + 3 * DAY), NOW - 60);
        older.answered_at = None;
        older.lists_every_limit = false;
        std::fs::write(
            crate::home::dir(&ctx).join("usage.json"),
            serde_json::to_string(&HashMap::from([("work-uuid", &older)]))
                .expect("readings as JSON"),
        )
        .expect("readings an older Pitboard wrote");
        using(&at(&ctx, NOW - HOUR), "work");
        open(&ctx, "pane");

        let shown = run(
            &ctx,
            "pane",
            &passed((12.0, NOW + 2 * HOUR), (31.0, NOW + 3 * DAY)),
        );
        assert_eq!(shown.current.as_deref(), Some("work"));
        assert!(!shown.sure);
        assert_eq!(shown.session, shares(12.0, 31.0));
        assert_eq!(recorded(&ctx), older);
    }

    /// Claude Code's config naming another account than when Anthropic last named the login
    /// it has stored is what a sign-in leaves. The account in use is still the one Anthropic
    /// named until a read asks again, and the label says it may not be.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn the_label_is_marked_while_the_config_has_moved_since_the_last_answer() {
        let (ctx, _scratch) = machine("config-moved");
        crate::state::save(&ctx, Permit::for_a_test(), &state()).expect("an account index");
        using(&ctx, "work");
        let settled = run(&ctx, "pane", &json!({}));
        assert_eq!(settled.current.as_deref(), Some("work"));
        assert!(settled.sure);

        sign_in(&ctx, "personal");
        let moved = run(&ctx, "pane", &json!({}));
        assert_eq!(moved.current.as_deref(), Some("work"));
        assert!(!moved.sure);
    }
}
