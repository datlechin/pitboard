//! Switching Claude Code by itself, before the account in use runs out.
//!
//! An agent working through a task stops when its account reaches a limit, while another of
//! the same person's accounts may sit there with room. So, where somebody asks for it,
//! Pitboard switches Claude Code to another of their accounts once any limit of the one in
//! use reaches a share they chose, 95% unless they chose otherwise. Before the limit and not
//! at it: a session already running follows a switch within about half a minute
//! ([`crate::switch::ADOPTION_CEILING_SECONDS`]), and one that has reached its limit has
//! already stopped. While a file sits behind the keychain, a running session follows only
//! at its login's next renewal, and a switch is still made, for every session started after
//! it and for each running one then; the switch says so.
//!
//! Asked for, and never by default: the menu bar app's setting, or `pitboard watch` running
//! in a terminal. This is the one part of Pitboard that picks an account without anybody
//! choosing it, so it is something a person turns on knowing what it is.
//!
//! Claude Code only. A running `codex` never follows a switch, so switching it would leave
//! the agent it was meant to keep going on the account that ran out.
//!
//! It decides from what Pitboard already knows, the readings every front end and every
//! status line record, and asks nobody about usage to decide. It judges only a reading
//! Anthropic gave. The account it watches is the one whose login Claude Code has stored, as
//! Anthropic last said. Where the files leave that in doubt, it is asked under the lock, as a
//! switch asks it, before anything is decided; where it cannot be told, nothing is decided
//! until it can. Every limit at the share is judged, and each reason it does not switch away
//! from one is said and recorded once for that limit and its reset. What it never does:
//!
//! - switch to an account without room below the share in every limit it has;
//! - switch back by itself. The next time the account in use reaches the share, the best
//!   account there is is chosen again, which may be the one it left;
//! - switch away from one limit of an account twice before that limit resets. Somebody who
//!   put it back in use meant to;
//! - switch away from an account within [`SETTLING_SECONDS`] of its being put in use, by
//!   anyone;
//! - try more than [`ATTEMPTS`] times for one limit before it resets, counting only the
//!   attempts that failed for a reason trying again will not mend. One that might, such as
//!   Anthropic out of reach, is tried again later each time, up to [`RETRY_MOST_SECONDS`].

use crate::api::Owner;
use crate::context::Context;
use crate::provider::ProviderId;
use crate::service::Permit;
use crate::state::{Account, Key, State};
use crate::status::Row;
use crate::usage::{self, Snapshot, Window, same_reset};
use crate::{atomic, budget, home, in_use};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The share of a limit at which Pitboard switches, as a whole percentage.
///
/// Never 100: an account at its limit has already stopped whatever was working in it, and
/// the app already says so then. Never below 50, where half of every account would be left
/// unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Threshold(u8);

impl Threshold {
    pub const LOWEST: u8 = 50;
    pub const HIGHEST: u8 = 99;
    pub const DEFAULT: Threshold = Threshold(95);

    /// `None` outside [`Threshold::LOWEST`] to [`Threshold::HIGHEST`].
    pub fn new(percent: u8) -> Option<Threshold> {
        (Threshold::LOWEST..=Threshold::HIGHEST)
            .contains(&percent)
            .then_some(Threshold(percent))
    }

    /// The nearest share there can be, for one kept by an earlier or a later version.
    pub fn clamped(percent: i64) -> Threshold {
        let lowest = i64::from(Threshold::LOWEST);
        let highest = i64::from(Threshold::HIGHEST);
        Threshold(u8::try_from(percent.clamp(lowest, highest)).unwrap_or(Threshold::DEFAULT.0))
    }

    pub const fn percent(self) -> u8 {
        self.0
    }

    /// Whether a limit `percent` used has reached the share as it is drawn.
    pub(crate) fn reached(self, percent: f64) -> bool {
        usage::reached(percent, self.0)
    }

    fn reached_by(self, window: &Window, now: i64) -> bool {
        self.reached(window.used(now))
    }
}

impl Default for Threshold {
    fn default() -> Threshold {
        Threshold::DEFAULT
    }
}

/// How long an account put in use is left alone, by anyone's switch: as long as the app
/// waits between reads of every account, so the account switched to has been asked about
/// once before anything could switch away from it again. A switch that turned out to land
/// on an account fuller than its last reading said is then corrected, and two switches are
/// never made from one look at the numbers.
pub const SETTLING_SECONDS: i64 = 300;

/// How long after an attempt that came to nothing Pitboard tries again: long enough for a
/// service that could not be reached, or a lock Claude Code held, to have come back. Twice
/// as long after each attempt in a row that failed for a reason trying again may mend.
pub const RETRY_SECONDS: i64 = 60;

/// The longest it waits between attempts, however long the outage: as long as a usage check
/// waits on a service that cannot be reached at most (`budget`).
pub const RETRY_MOST_SECONDS: i64 = 900;

/// Attempts at switching away from one limit of one account before it resets, of those that
/// failed for a reason trying again will not mend: a switch refused this often for such a
/// reason is not going to be made.
pub const ATTEMPTS: u32 = 3;

/// How often a front end that switches by itself decides though nothing was written: an
/// account put in use stops settling, and the wait after a failed attempt ends, with nobody
/// writing anything.
pub const DECIDE_EVERY_SECONDS: i64 = 30;

/// How long to wait after `waits` failures in a row for a reason trying again may mend: of
/// attempts, of asking whose login Claude Code has stored, or of a front end's refusals that
/// came before anything here could record them.
pub fn retry_after(waits: u32) -> i64 {
    (RETRY_SECONDS << waits.saturating_sub(1).min(4)).min(RETRY_MOST_SECONDS)
}

/// What Pitboard switches by itself, as it decided from what it knows.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The account in use, and its id, under which what was tried for its limits is kept.
    pub from: Key,
    pub from_id: String,
    /// The account to switch to.
    pub to: Key,
    /// The limit of `from` that reached the share, as last read, of the window the ledger
    /// keeps it under.
    pub limit: Window,
}

/// A limit of the account in use at the share that Pitboard does not switch away from, and
/// why.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hold {
    /// The account in use, and its id, under which what was said of its limits is kept.
    pub(crate) from: Key,
    pub(crate) from_id: String,
    /// The limit, as last read, of the window the ledger keeps it under.
    pub(crate) limit: Window,
    pub(crate) why: Skip,
}

impl Hold {
    /// As a front end is told it.
    pub(crate) fn skipped(self, state: &State) -> Auto {
        Auto::Skipped {
            from: state.typed(&self.from),
            limit: self.limit,
            why: self.why,
        }
    }
}

/// What Pitboard would do about the Claude Code account in use, from what it knows now.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Decision {
    /// No limit has reached the share. `nearest` is the fullest of those still running, in
    /// the reading taken at `as_of`.
    Below {
        nearest: Option<Window>,
        as_of: Option<i64>,
    },
    /// Anthropic gave no reading of the account in use, so nothing says what it has used.
    NoReading,
    /// An attempt at switching away from a limit at the share came to nothing a moment ago,
    /// and the next, at `limit`, is not made before `until`.
    Waiting {
        limit: Window,
        until: i64,
    },
    Hold(Hold),
    Switch(Plan),
}

/// What switching by itself came to, for a front end to say.
#[derive(Debug)]
pub enum Auto {
    /// Nothing to do: no limit of `account`, the account in use as it is typed, has reached
    /// the share. `nearest` is its fullest limit still running, in the reading taken at
    /// `as_of`. Where Anthropic is not asked about it again before a time, `held_until` is
    /// that time, in epoch seconds, and the reading is no newer until then.
    Watching {
        account: String,
        nearest: Option<Window>,
        as_of: Option<i64>,
        held_until: Option<i64>,
    },
    /// `limit` of `from` reached the share, and the next attempt at switching away from it is
    /// not made before `until`, in epoch seconds: one at a limit of `from` came to nothing a
    /// moment ago.
    Waiting {
        from: String,
        limit: Window,
        until: i64,
    },
    /// Claude Code switched from `from` to `to`, as each is typed, because `limit` of
    /// `from` reached the share.
    Switched {
        from: String,
        to: String,
        limit: Window,
        adoption: crate::provider::Adoption,
    },
    /// `limit` of `from` reached the share, and Pitboard will not switch, for `why`.
    Skipped {
        from: String,
        limit: Window,
        why: Skip,
    },
    /// Pitboard cannot judge whether to switch, whatever the numbers say, for `why`.
    NotWatching { why: Blind },
}

impl Auto {
    /// What tells this apart from another, for every front end that says each once: a reason
    /// not to switch away from a limit by [`Skip::told_apart`], a reason nothing can be judged
    /// by [`Blind::told_apart`]. A wait is told apart by the account and when it ends, which
    /// each failed attempt moves, and not by the limit it names: the wait is the moment's, and
    /// that limit may be one nothing was recorded for, whose reset each answer can give a
    /// second apart. Nothing tells a switch apart, nor watching, whose numbers move with every
    /// reading: a front end paces those as it will.
    pub fn told_apart(&self) -> Option<String> {
        match self {
            Auto::Watching { .. } | Auto::Switched { .. } => None,
            Auto::Waiting {
                from,
                limit: _,
                until,
            } => Some(format!("waiting/{from}/{until}")),
            Auto::Skipped { from, limit, why } => Some(why.told_apart(from, limit)),
            Auto::NotWatching { why } => Some(why.told_apart()),
        }
    }
}

/// Why Pitboard does not switch away from a limit at the share.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Skip {
    /// No other account has room below the share in every limit it has. `unread` are those
    /// it could switch to that Anthropic gave no reading of, as each is typed.
    NoRoom { unread: Vec<String> },
    /// Pitboard switched away from this limit once already before it resets, and the account
    /// was put back in use since.
    AlreadyLeft,
    /// [`ATTEMPTS`] attempts at switching away from this limit came to nothing for a reason
    /// trying again will not mend, and none is made again before it resets.
    GaveUp,
    /// The account was put in use less than [`SETTLING_SECONDS`] ago, and is not switched
    /// away from before `until`, in epoch seconds.
    Settling { until: i64 },
    /// Claude Code authenticates some other way, set by these, so a switch would change
    /// nothing its sessions use.
    Overridden(Vec<String>),
}

impl Skip {
    /// A stable code, for `--json` and for telling one apart from another.
    pub fn code(&self) -> &'static str {
        match self {
            Skip::NoRoom { .. } => "no_room",
            Skip::AlreadyLeft => "already_switched",
            Skip::GaveUp => "attempts_spent",
            Skip::Settling { .. } => "settling",
            Skip::Overridden(_) => "auth_overridden",
        }
    }

    /// What tells this reason not to switch away from `limit` of `from`, as `from` is typed,
    /// apart from another: the account, the limit, the reset the ledger recorded it under and
    /// its code, as the ledger records it once. Not what else it names, such as when settling
    /// ends.
    pub fn told_apart(&self, from: &str, limit: &Window) -> String {
        format!(
            "skipped/{from}/{}/{}/{}/{}",
            limit.kind,
            limit.scope.as_deref().unwrap_or_default(),
            limit.resets_at.unwrap_or_default(),
            self.code()
        )
    }
}

/// Why Pitboard cannot judge whether to switch Claude Code at all.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Blind {
    /// A switch was interrupted, and the change that finishes it is somebody's to make.
    SwitchInterrupted,
    /// Claude Code keeps its login under another name, which Pitboard does not act on.
    CustomOauth,
    /// Claude Code has no login stored.
    NothingSignedIn,
    /// The login Claude Code has stored is of an account nobody enrolled, which has `email`.
    NotEnrolled { email: String },
    /// Whose login Claude Code has stored could not be told, for `detail`, and is not asked
    /// again before `until`, in epoch seconds.
    Unidentified { detail: String, until: i64 },
    /// Anthropic gave no reading of `account`, the account in use as it is typed: none was
    /// taken yet, or the one Pitboard holds was written by an older Pitboard.
    NoReading { account: String },
}

impl Blind {
    /// A stable code, for `--json` and for telling one apart from another.
    pub fn code(&self) -> &'static str {
        match self {
            Blind::SwitchInterrupted => "switch_interrupted",
            Blind::CustomOauth => "custom_oauth_endpoint",
            Blind::NothingSignedIn => "nothing_signed_in",
            Blind::NotEnrolled { .. } => "not_enrolled",
            Blind::Unidentified { .. } => "not_identified",
            Blind::NoReading { .. } => "no_reading",
        }
    }

    /// What tells this reason apart from another: its code, and the email, the cause or the
    /// account it names. Not when it asks again, so a cause that stands is said once however
    /// often it is asked about.
    pub fn told_apart(&self) -> String {
        let named = match self {
            Blind::SwitchInterrupted | Blind::CustomOauth | Blind::NothingSignedIn => None,
            Blind::NotEnrolled { email } => Some(email),
            Blind::Unidentified { detail, until: _ } => Some(detail),
            Blind::NoReading { account } => Some(account),
        };
        match named {
            Some(named) => format!("not-watching/{}/{named}", self.code()),
            None => format!("not-watching/{}", self.code()),
        }
    }
}

/// What a look at the files alone came to.
#[derive(Debug)]
pub enum Look {
    /// This stands, and nothing is to be done under the lock.
    Stands(Box<Auto>),
    /// A switch may be due, or whose login Claude Code has stored is in doubt: only a
    /// decision under the lock can say ([`crate::service::Pitboard::auto_switch`]).
    Act,
}

/// The fuller of two limits first, then the one resetting sooner.
fn fuller(a: &Window, b: &Window, now: i64) -> Ordering {
    b.used(now)
        .total_cmp(&a.used(now))
        .then_with(|| sooner(a.resets_at, b.resets_at))
}

/// A known reset before an unknown one.
fn sooner(a: Option<i64>, b: Option<i64>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// A reading Anthropic gave.
fn answered(row: &Row) -> Option<&Snapshot> {
    row.usage.as_ref().filter(|reading| reading.answered())
}

/// What Pitboard would do about `current`, the Claude Code account in use, from `rows`, as
/// `state` and `ledger` stand at `now`.
///
/// Every limit at the share is judged, the furthest past it first. One already switched away
/// from, or tried for as often as it is tried, leaves the next to be judged on its own: where
/// none is left, it is said why of the furthest at once, since nothing will be tried. Only one
/// left to try is held back, by the account settling or by a failed attempt's wait, and either
/// is said of it. That wait is of the last attempt at any limit, since what failed was the
/// moment and not the limit.
///
/// A limit is given with the reset the ledger keeps its window under, where it keeps one:
/// answers can give one window's reset a second apart, and the front ends tell one window
/// from the next by it, so they say once what was recorded once.
pub(crate) fn decide(
    state: &State,
    current: &Key,
    rows: &[Row],
    ledger: &Ledger,
    threshold: Threshold,
    now: i64,
) -> Decision {
    let Some((row, reading)) = rows
        .iter()
        .find(|row| row.key().as_ref() == Some(current))
        .and_then(|row| Some((row, answered(row)?)))
    else {
        return Decision::NoReading;
    };
    let mut reached: Vec<&Window> = reading
        .windows
        .iter()
        .filter(|window| threshold.reached_by(window, now))
        .collect();
    reached.sort_by(|a, b| fuller(a, b, now));
    let Some(&furthest) = reached.first() else {
        return Decision::Below {
            nearest: reading
                .windows
                .iter()
                .filter(|window| window.resets_at.is_none_or(|at| at > now))
                .min_by(|a, b| fuller(a, b, now))
                .cloned(),
            as_of: reading.observed_at,
        };
    };
    let tried = |limit: &Window| ledger.tried(&row.id, limit);
    let as_kept =
        |limit: &Window| tried(limit).map_or_else(|| limit.clone(), |tried| tried.window(limit));
    let hold = |limit: &Window, why| {
        Decision::Hold(Hold {
            from: current.clone(),
            from_id: row.id.clone(),
            limit: as_kept(limit),
            why,
        })
    };
    let open = reached.iter().copied().find(|&limit| {
        tried(limit).is_none_or(|tried| !tried.switched && tried.attempts < ATTEMPTS)
    });
    let Some(limit) = open else {
        let left = tried(furthest).is_some_and(|tried| tried.switched);
        return hold(
            furthest,
            if left {
                Skip::AlreadyLeft
            } else {
                Skip::GaveUp
            },
        );
    };
    let put_in_use = state
        .get(current)
        .and_then(|account| account.last_used_at)
        .filter(|at| (0..SETTLING_SECONDS).contains(&(now - at)));
    if let Some(at) = put_in_use {
        return hold(
            limit,
            Skip::Settling {
                until: at + SETTLING_SECONDS,
            },
        );
    }
    let waiting = reached
        .iter()
        .filter_map(|&window| tried(window).filter(|tried| !tried.switched))
        .map(|tried| tried.at + retry_after(tried.waits))
        .max()
        .filter(|&until| now < until);
    if let Some(until) = waiting {
        return Decision::Waiting {
            limit: as_kept(limit),
            until,
        };
    }
    let passed_over: Vec<&str> = reading
        .windows
        .iter()
        .filter_map(tried)
        .flat_map(|tried| tried.passed_over.iter().map(String::as_str))
        .collect();
    let goes = || {
        rows.iter().filter(|row| {
            row.provider == ProviderId::Claude
                && row.key().is_some_and(|key| key != *current)
                && row.switchable(now)
                && !passed_over.contains(&row.id.as_str())
        })
    };
    let best = usage::roomiest(goes().filter_map(|row| {
        let theirs = answered(row)?;
        let room = usage::room(
            &theirs.windows,
            theirs.lists_every_limit,
            &reading.windows,
            limit,
            threshold.percent(),
            now,
        )?;
        Some((row, room))
    }));
    match best.and_then(|(row, _)| row.key()) {
        Some(to) => Decision::Switch(Plan {
            from: current.clone(),
            from_id: row.id.clone(),
            to,
            limit: as_kept(limit),
        }),
        None => hold(
            limit,
            Skip::NoRoom {
                unread: goes()
                    .filter(|row| answered(row).is_none())
                    .filter_map(Row::key)
                    .map(|key| state.typed(&key))
                    .collect(),
            },
        ),
    }
}

/// What Pitboard has tried and said for each limit of each account that reached the share,
/// kept in Pitboard's directory between runs and shared by every front end that switches by
/// itself, with the last time whose login Claude Code has stored could not be told. Written
/// only under the lock every change takes, so two of them never act on one limit twice, nor
/// record one reason twice, nor ask twice in one wait. Nothing here is secret: account ids,
/// names of limits, times, counts, codes and why a login could not be told whose it is.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Ledger {
    limits: BTreeMap<String, Tried>,
    #[serde(skip_serializing_if = "Option::is_none")]
    asked: Option<Asked>,
}

/// Whose login Claude Code has stored, asked under the lock and not told, since it was last
/// told.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Asked {
    /// When it was last asked, in epoch seconds.
    at: i64,
    /// Times in a row it was not told, which each make the wait before the next longer.
    failures: u32,
    /// Why it was not told the last time, in a few words.
    detail: String,
}

/// One limit of one account, in the window that reset at `resets_at`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
struct Tried {
    /// When the limit resets, in epoch seconds, or 0 where that is not known.
    resets_at: i64,
    /// Switches away from it begun in this window, but for those that failed for a reason
    /// trying again may mend.
    attempts: u32,
    /// Attempts in a row that failed for a reason trying again may mend, which each make the
    /// wait before the next longer.
    #[serde(default)]
    waits: u32,
    /// When the last began.
    at: i64,
    /// Whether one of them switched.
    #[serde(default)]
    switched: bool,
    /// Accounts, by id, a switch to was refused for a reason of their own, such as a parked
    /// login that no longer works, so the next attempt goes elsewhere.
    #[serde(default)]
    passed_over: Vec<String>,
    /// The reasons Pitboard did not switch away from it that were said and recorded, by
    /// [`Skip::code`], with when each was.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    said: BTreeMap<String, i64>,
}

impl Tried {
    /// When anything was last kept of it.
    fn last(&self) -> i64 {
        self.said.values().copied().fold(self.at, i64::max)
    }

    /// Whether it is still kept at `now`: while a reading of its window may still run, since a
    /// later answer can give that window's reset up to a minute after the one it was kept
    /// under; or, where no reset is known, for a week after anything was last kept of it.
    fn kept(&self, now: i64) -> bool {
        usage::may_still_run(self.resets_at, now)
            || (self.resets_at == 0 && now - self.last() < 7 * 86_400)
    }

    /// `limit` with the reset this window of it was first kept under.
    fn window(&self, limit: &Window) -> Window {
        Window {
            resets_at: limit.resets_at.map(|_| self.resets_at),
            ..limit.clone()
        }
    }
}

fn path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("autoswitch.json")
}

/// A limit of an account, by the names a limit goes by.
fn entry(id: &str, limit: &Window) -> String {
    [
        id,
        usage::limit_name(&limit.kind),
        limit.scope.as_deref().unwrap_or_default(),
    ]
    .join("/")
}

impl Ledger {
    /// What is kept, without the windows that have reset since it was written, as the next
    /// write would leave them out. Nothing kept, and a record that cannot be read, are nothing
    /// tried: the record is written whole the next time anything is tried, and at worst one
    /// limit is tried once more.
    pub(crate) fn load(ctx: &Context) -> Ledger {
        let mut ledger: Ledger = std::fs::read_to_string(path(ctx))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        let now = ctx.now();
        ledger.limits.retain(|_, tried| tried.kept(now));
        ledger
    }

    /// Kept whole, without the windows that have reset.
    pub(crate) fn save(&mut self, ctx: &Context, permit: Permit) -> crate::error::Result<()> {
        let now = ctx.now();
        self.limits.retain(|_, tried| tried.kept(now));
        let body = serde_json::to_vec(self).map_err(|e| crate::error::Error::StateWriteFailed {
            path: path(ctx),
            source: std::io::Error::other(e),
        })?;
        atomic::write(permit, &path(ctx), &body, atomic::Perms::Secret).map_err(|source| {
            crate::error::Error::StateWriteFailed {
                path: path(ctx),
                source,
            }
        })
    }

    /// What was tried for this window of `limit`, where anything was.
    fn tried(&self, id: &str, limit: &Window) -> Option<&Tried> {
        self.limits
            .get(&entry(id, limit))
            .filter(|tried| same_reset(tried.resets_at, limit.resets_at.unwrap_or(0)))
    }

    /// What is kept of this window of `limit`, begun afresh where what is kept is of an
    /// earlier one.
    fn trying(&mut self, id: &str, limit: &Window) -> &mut Tried {
        let resets_at = limit.resets_at.unwrap_or(0);
        let tried = self.limits.entry(entry(id, limit)).or_default();
        if !same_reset(tried.resets_at, resets_at) {
            *tried = Tried {
                resets_at,
                ..Tried::default()
            };
        }
        tried
    }

    /// A switch for `plan` begins at `now`.
    pub(crate) fn attempt(&mut self, plan: &Plan, now: i64) {
        let tried = self.trying(&plan.from_id, &plan.limit);
        tried.attempts += 1;
        tried.at = now;
    }

    /// The switch for `plan` failed for a reason trying again may mend: not one of the
    /// attempts, and the next waits longer.
    pub(crate) fn waited(&mut self, plan: &Plan) {
        let tried = self.trying(&plan.from_id, &plan.limit);
        tried.attempts = tried.attempts.saturating_sub(1);
        tried.waits = tried.waits.saturating_add(1);
    }

    /// The switch for `plan` was made.
    pub(crate) fn switched(&mut self, plan: &Plan) {
        self.trying(&plan.from_id, &plan.limit).switched = true;
    }

    /// The switch for `plan` was refused over the account it would have switched to, and
    /// the next goes to another.
    pub(crate) fn pass_over(&mut self, plan: &Plan, to_id: &str) {
        let tried = self.trying(&plan.from_id, &plan.limit);
        tried.waits = 0;
        if !tried.passed_over.iter().any(|id| id == to_id) {
            tried.passed_over.push(to_id.to_owned());
        }
    }

    /// Whether why Pitboard does not switch away from `hold`'s limit was said in this window
    /// of it already.
    pub(crate) fn said(&self, hold: &Hold) -> bool {
        self.tried(&hold.from_id, &hold.limit)
            .is_some_and(|tried| tried.said.contains_key(hold.why.code()))
    }

    /// Why Pitboard does not switch away from `hold`'s limit is said at `now`. Whether it was
    /// not said in this window of it before, and so is to be recorded.
    pub(crate) fn say(&mut self, hold: &Hold, now: i64) -> bool {
        let said = &mut self.trying(&hold.from_id, &hold.limit).said;
        let code = hold.why.code();
        if said.contains_key(code) {
            return false;
        }
        said.insert(code.to_owned(), now);
        true
    }

    /// The times in a row whose login Claude Code has stored was not told, unless a record of
    /// whose it is, made at `known_at` by anyone who could ask, has come since and ended them.
    fn not_told_in_a_row(&self, known_at: Option<i64>) -> Option<&Asked> {
        self.asked
            .as_ref()
            .filter(|asked| !known_at.is_some_and(|known_at| known_at > asked.at))
    }

    /// Why whose login Claude Code has stored is not asked again at `now`: it was not told
    /// the last time, and the wait after that runs.
    pub(crate) fn unidentified(&self, known_at: Option<i64>, now: i64) -> Option<Blind> {
        let asked = self.not_told_in_a_row(known_at)?;
        let until = asked.at + retry_after(asked.failures);
        (now < until).then(|| Blind::Unidentified {
            detail: asked.detail.clone(),
            until,
        })
    }

    /// Whose login Claude Code has stored was asked at `now` and not told, for `detail`, with
    /// the record of whose it was last made at `known_at`: why nothing is judged, and until
    /// when.
    pub(crate) fn not_told(&mut self, detail: String, known_at: Option<i64>, now: i64) -> Blind {
        let failures = self
            .not_told_in_a_row(known_at)
            .map_or(0, |asked| asked.failures)
            + 1;
        self.asked = Some(Asked {
            at: now,
            failures,
            detail: detail.clone(),
        });
        Blind::Unidentified {
            detail,
            until: now + retry_after(failures),
        }
    }

    /// Whose login Claude Code has stored was told. Whether that changed what is kept.
    pub(crate) fn told(&mut self) -> bool {
        self.asked.take().is_some()
    }
}

/// What Pitboard would do now, from its files alone: no lock, no keychain and no network, so
/// a look that finds nothing to do costs nobody anything. The account in use is the one
/// Pitboard's record says Anthropic last named for the login Claude Code has stored. Where
/// that record is in doubt, a switch may be due, a reason not to switch was not yet recorded,
/// or another run is in the middle of a switch, only the decision under the lock can say,
/// from whose login is stored by then.
pub(crate) fn look(ctx: &Context, state: &State, threshold: Threshold) -> Look {
    let unfinished = crate::switch::unfinished(ctx);
    if let Some(why) = blind(ctx, unfinished) {
        return Look::Stands(Box::new(Auto::NotWatching { why }));
    }
    if unfinished.is_some() {
        return Look::Act;
    }
    let now = ctx.now();
    let ledger = Ledger::load(ctx);
    let known = in_use::known(ctx, state, ProviderId::Claude);
    let known_at = known.last.as_ref().map(|last| last.known_at);
    if let Some(why) = ledger.unidentified(known_at, now) {
        return Look::Stands(Box::new(Auto::NotWatching { why }));
    }
    if known.doubt.is_some() {
        return Look::Act;
    }
    let account = match watched(state, known.owner()) {
        Ok(account) => account,
        Err(why) => return Look::Stands(Box::new(Auto::NotWatching { why })),
    };
    match judge(ctx, state, account, &ledger, threshold, now) {
        Judged::Switch(_) => Look::Act,
        Judged::Hold(hold) if ledger.said(&hold) => Look::Stands(Box::new(hold.skipped(state))),
        Judged::Hold(_) => Look::Act,
        Judged::Stands(stands) => Look::Stands(Box::new(stands)),
    }
}

/// Why nothing of Claude Code's may be judged here, whatever its numbers: a switch waits to be
/// finished, which is a change for somebody to make, or Claude Code keeps its login where
/// Pitboard does not act. Asked before anything else, so the automatic switch never settles
/// either on its own. A switch another run is making waits on nobody, and is not one.
fn blind(ctx: &Context, unfinished: Option<crate::switch::Unfinished>) -> Option<Blind> {
    if unfinished == Some(crate::switch::Unfinished::Interrupted) {
        Some(Blind::SwitchInterrupted)
    } else if crate::settings::custom_oauth(ctx) {
        Some(Blind::CustomOauth)
    } else {
        None
    }
}

/// The account whose login Claude Code has stored, `owner`'s, or why it cannot be watched:
/// there is none, or nobody enrolled it.
pub(crate) fn watched<'a>(state: &'a State, owner: Option<&Owner>) -> Result<&'a Account, Blind> {
    let owner = owner.ok_or(Blind::NothingSignedIn)?;
    state
        .account_of(ProviderId::Claude, owner)
        .ok_or_else(|| Blind::NotEnrolled {
            email: owner.email.clone(),
        })
}

/// What a decision comes to for a front end.
#[derive(Debug)]
pub(crate) enum Judged {
    Switch(Plan),
    /// A reason not to switch away from a limit at the share, said and recorded once for
    /// that limit and its reset.
    Hold(Hold),
    /// Nothing to do, or nothing that can be done now.
    Stands(Auto),
}

/// What Pitboard would do about `account`, the Claude Code account in use, from the readings
/// it holds, as `state` and `ledger` stand at `now`. Where Claude Code signs in another way, a
/// switch would change nothing its sessions use.
pub(crate) fn judge(
    ctx: &Context,
    state: &State,
    account: &Account,
    ledger: &Ledger,
    threshold: Threshold,
    now: i64,
) -> Judged {
    let rows = crate::status::gather_offline(ctx, state).rows;
    let key = account.key();
    let typed = state.typed(&key);
    let plan = match decide(state, &key, &rows, ledger, threshold, now) {
        Decision::Below { nearest, as_of } => {
            return Judged::Stands(Auto::Watching {
                account: typed,
                nearest,
                as_of,
                held_until: budget::held_until(ctx, &account.id),
            });
        }
        Decision::NoReading => {
            return Judged::Stands(Auto::NotWatching {
                why: Blind::NoReading { account: typed },
            });
        }
        Decision::Waiting { limit, until } => {
            return Judged::Stands(Auto::Waiting {
                from: typed,
                limit,
                until,
            });
        }
        Decision::Hold(hold) => return Judged::Hold(hold),
        Decision::Switch(plan) => plan,
    };
    let overridden = crate::provider::of(ProviderId::Claude).overridden_by(ctx);
    if overridden.is_empty() {
        return Judged::Switch(plan);
    }
    Judged::Hold(Hold {
        from: plan.from,
        from_id: plan.from_id,
        limit: plan.limit,
        why: Skip::Overridden(overridden),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Park;
    use crate::switch::harness::{NOW, account, window};
    use crate::usage::Source;

    fn scoped(kind: &str, model: &str, percent: f64) -> Window {
        Window {
            scope: Some(model.into()),
            ..window(kind, percent)
        }
    }

    fn row(label: &str, signed_in: bool, windows: Vec<Window>) -> Row {
        Row {
            provider: ProviderId::Claude,
            label: Some(label.into()),
            email: format!("{label}@example.com"),
            id: label.into(),
            account_uuid: label.into(),
            organization_uuid: Some("org".into()),
            signed_in,
            parked: (!signed_in).then(|| Park {
                service: format!("pitboard-park-{label}-1"),
                parked_at: NOW,
                refresh_fingerprint: label.into(),
                access_expires_at: Some(NOW + 3600),
                refresh_expires_at: Some(NOW + 30 * 86_400),
            }),
            usage: Some(Snapshot {
                windows,
                observed_at: Some(NOW),
                answered_at: Some(NOW),
                lists_every_limit: true,
                source: Source::Remembered,
            }),
            stale: None,
            plan: None,
        }
    }

    fn state_of(rows: &[Row]) -> State {
        let mut state = State::default();
        for row in rows {
            state.accounts.push(account(
                row.label.as_deref().unwrap(),
                &row.account_uuid,
                row.parked.clone(),
            ));
        }
        state
    }

    /// What is decided about the account `rows` have in use.
    fn decided_with(
        state: &State,
        rows: &[Row],
        ledger: &Ledger,
        threshold: Threshold,
        now: i64,
    ) -> Decision {
        let current = rows
            .iter()
            .find(|row| row.signed_in)
            .and_then(Row::key)
            .expect("an account in use");
        decide(state, &current, rows, ledger, threshold, now)
    }

    fn decided(rows: &[Row]) -> Decision {
        decided_with(
            &state_of(rows),
            rows,
            &Ledger::default(),
            Threshold::DEFAULT,
            NOW,
        )
    }

    /// `work` was put in use at `at`.
    fn put_in_use(state: &mut State, at: i64) {
        let work = state.accounts.iter_mut().find(|a| a.label == "work");
        work.expect("enrolled").last_used_at = Some(at);
    }

    /// `work`, in use, not switched away from at `limit`, for `why`.
    fn held(limit: &Window, why: Skip) -> Decision {
        Decision::Hold(Hold {
            from: Key::new(ProviderId::Claude, "work"),
            from_id: "work".into(),
            limit: limit.clone(),
            why,
        })
    }

    /// Whether `decision` is that no other account has room.
    fn no_room(decision: &Decision) -> bool {
        matches!(
            decision,
            Decision::Hold(Hold {
                why: Skip::NoRoom { .. },
                ..
            })
        )
    }

    fn to(decision: &Decision) -> Option<&str> {
        match decision {
            Decision::Switch(plan) => Some(plan.to.label.as_str()),
            _ => None,
        }
    }

    fn work_at(percent: f64) -> Row {
        row(
            "work",
            true,
            vec![window("session", percent), window("weekly_all", 20.0)],
        )
    }

    fn spare() -> Row {
        row(
            "spare",
            false,
            vec![window("session", 10.0), window("weekly_all", 30.0)],
        )
    }

    /// Room too, but less of it than `spare` has.
    fn other() -> Row {
        row(
            "other",
            false,
            vec![window("session", 30.0), window("weekly_all", 30.0)],
        )
    }

    /// `work`, in use, with its five-hour limit at 96% and its weekly limit further past the
    /// share, at 98%.
    fn work_at_both() -> Row {
        row(
            "work",
            true,
            vec![window("session", 96.0), window("weekly_all", 98.0)],
        )
    }

    #[test]
    fn a_limit_below_the_share_is_left_alone_and_one_at_it_switches() {
        assert_eq!(
            decided(&[work_at(94.4), spare()]),
            Decision::Below {
                nearest: Some(window("session", 94.4)),
                as_of: Some(NOW)
            }
        );
        let decision = decided(&[work_at(95.0), spare()]);
        assert_eq!(to(&decision), Some("spare"));
        let Decision::Switch(plan) = decision else {
            unreachable!()
        };
        assert_eq!(
            (plan.from.label.as_str(), plan.from_id.as_str()),
            ("work", "work")
        );
        assert_eq!(plan.limit.kind, "session");
    }

    /// The share is judged as the app, `pitboard status` and the status line show it: a
    /// limit at 94.5 is drawn at 95%, and is at a share of 95%.
    #[test]
    fn a_limit_shown_at_the_share_is_at_the_share() {
        assert_eq!(to(&decided(&[work_at(94.5), spare()])), Some("spare"));
    }

    #[test]
    fn every_limit_the_account_reports_can_reach_the_share() {
        let weekly = row(
            "work",
            true,
            vec![window("session", 10.0), window("weekly_all", 97.0)],
        );
        let opus = row(
            "work",
            true,
            vec![
                window("session", 10.0),
                window("weekly_all", 10.0),
                scoped("weekly_scoped", "Opus", 96.0),
            ],
        );
        let named_as_a_session_names_them = row(
            "work",
            true,
            vec![window("five_hour", 99.0), window("seven_day", 10.0)],
        );
        for (case, work) in [
            ("weekly", weekly),
            ("opus", opus),
            ("five_hour", named_as_a_session_names_them),
        ] {
            let spare = row(
                "spare",
                false,
                vec![
                    window("session", 0.0),
                    window("weekly_all", 0.0),
                    scoped("weekly_scoped", "Opus", 0.0),
                ],
            );
            assert_eq!(to(&decided(&[work, spare])), Some("spare"), "{case}");
        }
    }

    #[test]
    fn the_limit_furthest_past_the_share_is_the_one_acted_on() {
        let Decision::Switch(plan) = decided(&[work_at_both(), spare()]) else {
            panic!("a switch")
        };
        assert_eq!(plan.limit.kind, "weekly_all");
    }

    /// A limit whose reset has passed counts as reset, and is not the one nearest the share.
    #[test]
    fn a_limit_whose_reset_has_passed_counts_as_reset() {
        let mut work = work_at(100.0);
        work.usage.as_mut().unwrap().windows[0].resets_at = Some(NOW);
        assert_eq!(
            decided(&[work, spare()]),
            Decision::Below {
                nearest: Some(window("weekly_all", 20.0)),
                as_of: Some(NOW)
            }
        );
    }

    #[test]
    fn an_account_in_use_with_no_reading_is_never_switched_from() {
        let mut work = work_at(99.0);
        work.usage = None;
        assert_eq!(decided(&[work, spare()]), Decision::NoReading);
    }

    /// A running `codex` never follows a switch, so Codex is never switched by itself, and
    /// never switched to: only Claude Code's account in use is judged, against Claude Code's
    /// accounts.
    #[test]
    fn codex_is_never_switched() {
        let codex = |mut row: Row| {
            row.provider = ProviderId::Codex;
            row
        };
        let home = row("home", true, vec![window("session", 10.0)]);
        let rows = [codex(work_at(99.0)), codex(spare()), home];
        let decision = decide(
            &state_of(&rows),
            &Key::new(ProviderId::Claude, "home"),
            &rows,
            &Ledger::default(),
            Threshold::DEFAULT,
            NOW,
        );
        assert!(matches!(decision, Decision::Below { .. }), "{decision:?}");
        assert_eq!(
            decided(&[work_at(99.0), codex(spare())]),
            held(&window("session", 99.0), Skip::NoRoom { unread: vec![] })
        );
    }

    #[test]
    fn an_account_with_any_limit_at_the_share_is_no_place_to_go() {
        let full_weekly = || {
            row(
                "spare",
                false,
                vec![window("session", 0.0), window("weekly_all", 95.0)],
            )
        };
        assert!(no_room(&decided(&[work_at(96.0), full_weekly()])));
        let roomy = row(
            "roomy",
            false,
            vec![window("session", 60.0), window("weekly_all", 10.0)],
        );
        assert_eq!(
            to(&decided(&[work_at(96.0), full_weekly(), roomy])),
            Some("roomy")
        );
    }

    /// An account to go to is judged as the one in use is: a limit drawn at 95% leaves it no
    /// room.
    #[test]
    fn an_account_with_a_limit_shown_at_the_share_is_no_place_to_go() {
        let spare_at = |weekly: f64| {
            row(
                "spare",
                false,
                vec![window("session", 0.0), window("weekly_all", weekly)],
            )
        };
        assert!(no_room(&decided(&[work_at(96.0), spare_at(94.5)])));
        assert_eq!(
            to(&decided(&[work_at(96.0), spare_at(94.4)])),
            Some("spare")
        );
    }

    #[test]
    fn an_account_that_cannot_be_switched_to_is_passed_over() {
        let mut expired = spare();
        expired.parked.as_mut().unwrap().refresh_expires_at = Some(NOW);
        assert!(no_room(&decided(&[work_at(96.0), expired])));
    }

    #[test]
    fn an_account_on_a_plan_without_that_limit_is_a_place_to_go() {
        let work = row(
            "work",
            true,
            vec![window("session", 20.0), window("weekly_all", 96.0)],
        );
        let seat = row(
            "seat",
            false,
            vec![
                window("session", 10.0),
                scoped("weekly_scoped", "Fable", 0.0),
            ],
        );
        assert_eq!(to(&decided(&[work, seat])), Some("seat"));
    }

    #[test]
    fn a_limit_the_other_account_does_not_report_is_one_it_does_not_have() {
        let no_weekly = row("spare", false, vec![window("session", 0.0)]);
        assert_eq!(to(&decided(&[work_at(96.0), no_weekly])), Some("spare"));
        let mut unread = spare();
        unread.usage = None;
        assert!(no_room(&decided(&[work_at(96.0), unread])));
    }

    /// Only an answer read whole says which limits an account has. One in the older shape,
    /// or with a row Pitboard could not read, says nothing of a limit it leaves out, which
    /// may be at 100%. The older shape names no model's limit, so it is judged by the
    /// five-hour and weekly limits, unless a model's limit is the one at the share.
    #[test]
    fn a_limit_left_out_of_a_reading_that_may_leave_limits_out_is_not_known_whichever_it_is() {
        let older = |windows| {
            let mut older = row("spare", false, windows);
            older.usage.as_mut().unwrap().lists_every_limit = false;
            older
        };
        assert!(
            no_room(&decided(&[
                work_at(96.0),
                older(vec![window("seven_day", 10.0)])
            ])),
            "the limit at the share"
        );
        assert!(
            no_room(&decided(&[
                work_at(96.0),
                older(vec![window("five_hour", 30.0)])
            ])),
            "another limit"
        );
        let mut weekly_unread = row("spare", false, Vec::new());
        weekly_unread.usage = Some(usage::from_usage_object(
            &serde_json::json!({"limits": [
                {"kind": "session", "percent": 10.0, "scope": null},
                {"kind": "weekly_all", "percent": null, "scope": null},
            ]}),
            NOW,
        ));
        assert!(
            no_room(&decided(&[work_at(96.0), weekly_unread])),
            "a limit whose row was not read"
        );

        let both = || older(vec![window("five_hour", 30.0), window("seven_day", 10.0)]);
        assert_eq!(to(&decided(&[work_at(96.0), both()])), Some("spare"));
        let with_opus = |session: f64, opus: f64| {
            row(
                "work",
                true,
                vec![
                    window("session", session),
                    window("weekly_all", 20.0),
                    scoped("weekly_scoped", "Opus", opus),
                ],
            )
        };
        assert_eq!(
            to(&decided(&[with_opus(96.0, 40.0), both()])),
            Some("spare")
        );
        assert!(
            no_room(&decided(&[with_opus(10.0, 97.0), both()])),
            "a model's limit at the share"
        );
    }

    /// Plans limit different models apart, so an account on another plan has no limit for a
    /// model the account in use has one for, and is one to go to whichever limit ran out.
    #[test]
    fn a_limit_for_a_model_the_other_plan_does_not_have_is_not_counted_against_it() {
        let work = row(
            "work",
            true,
            vec![
                window("session", 97.0),
                window("weekly_all", 20.0),
                scoped("weekly_scoped", "Opus", 40.0),
            ],
        );
        assert_eq!(to(&decided(&[work, spare()])), Some("spare"));

        let opus_ran_out = row(
            "work",
            true,
            vec![
                window("session", 10.0),
                window("weekly_all", 20.0),
                scoped("weekly_scoped", "Opus", 97.0),
            ],
        );
        assert_eq!(to(&decided(&[opus_ran_out, spare()])), Some("spare"));

        let session_only = row("spare", false, vec![window("session", 10.0)]);
        let work = row(
            "work",
            true,
            vec![
                window("session", 97.0),
                scoped("weekly_scoped", "Opus", 40.0),
            ],
        );
        assert_eq!(to(&decided(&[work, session_only])), Some("spare"));
    }

    #[test]
    fn the_account_with_most_room_in_that_limit_is_chosen_then_the_least_full() {
        let less = || {
            row(
                "less",
                false,
                vec![window("session", 50.0), window("weekly_all", 10.0)],
            )
        };
        let more = row(
            "more",
            false,
            vec![window("session", 20.0), window("weekly_all", 80.0)],
        );
        assert_eq!(to(&decided(&[work_at(96.0), less(), more])), Some("more"));
        let tie = row(
            "tie",
            false,
            vec![window("session", 50.0), window("weekly_all", 5.0)],
        );
        assert_eq!(to(&decided(&[work_at(96.0), less(), tie])), Some("tie"));
        let same = row(
            "same",
            false,
            vec![window("session", 50.0), window("weekly_all", 10.0)],
        );
        assert_eq!(
            to(&decided(&[work_at(96.0), less(), same])),
            Some("less"),
            "the first of two alike"
        );
    }

    #[test]
    fn an_account_just_put_in_use_is_left_to_settle() {
        let rows = [work_at(96.0), spare()];
        let mut state = state_of(&rows);
        put_in_use(&mut state, NOW - SETTLING_SECONDS + 1);
        let ledger = Ledger::default();
        assert_eq!(
            decided_with(&state, &rows, &ledger, Threshold::DEFAULT, NOW),
            held(&window("session", 96.0), Skip::Settling { until: NOW + 1 })
        );
        put_in_use(&mut state, NOW - SETTLING_SECONDS);
        assert_eq!(
            to(&decided_with(
                &state,
                &rows,
                &ledger,
                Threshold::DEFAULT,
                NOW
            )),
            Some("spare")
        );
    }

    fn plan(rows: &[Row]) -> Plan {
        match decided(rows) {
            Decision::Switch(plan) => plan,
            other => panic!("a switch, not {other:?}"),
        }
    }

    #[test]
    fn a_limit_switched_away_from_is_switched_away_from_again_in_its_next_window() {
        let rows = [work_at(96.0), spare()];
        let state = state_of(&rows);
        let mut ledger = Ledger::default();
        let plan = plan(&rows);
        ledger.attempt(&plan, NOW - 3600);
        ledger.switched(&plan);

        let mut next_window = work_at(96.0);
        next_window.usage.as_mut().unwrap().windows[0].resets_at = Some(NOW + 3 * 3600);
        let rows = [next_window, spare()];
        assert_eq!(
            to(&decided_with(
                &state,
                &rows,
                &ledger,
                Threshold::DEFAULT,
                NOW
            )),
            Some("spare")
        );
    }

    #[test]
    fn a_failed_attempt_is_tried_again_after_a_minute() {
        let rows = [work_at(96.0), spare()];
        let state = state_of(&rows);
        let mut ledger = Ledger::default();
        let plan = plan(&rows);
        ledger.attempt(&plan, NOW);
        let at = |now| decided_with(&state, &rows, &ledger, Threshold::DEFAULT, now);
        let waiting = Decision::Waiting {
            limit: plan.limit.clone(),
            until: NOW + RETRY_SECONDS,
        };
        assert_eq!(at(NOW + RETRY_SECONDS - 1), waiting);
        assert_eq!(to(&at(NOW + RETRY_SECONDS)), Some("spare"));
        assert_eq!(at(NOW - 1), waiting, "a clock gone backwards waits");
    }

    /// A failure trying again may mend, such as Anthropic out of reach while the switch
    /// asks whose a login is, is not one of the three: an outage of a few minutes must not
    /// leave the account in use to run out. It waits longer after each, up to a quarter of an
    /// hour, so an outage of hours is not asked about every minute.
    #[test]
    fn a_failure_trying_again_may_mend_waits_longer_each_time_and_is_not_counted() {
        let rows = [work_at(96.0), spare()];
        let state = state_of(&rows);
        let mut ledger = Ledger::default();
        let plan = plan(&rows);
        let mut at = NOW;
        for wait in [60, 120, 240, 480, 900, 900] {
            ledger.attempt(&plan, at);
            ledger.waited(&plan);
            let decided = |now| decided_with(&state, &rows, &ledger, Threshold::DEFAULT, now);
            assert_eq!(
                decided(at + wait - 1),
                Decision::Waiting {
                    limit: plan.limit.clone(),
                    until: at + wait
                },
                "within {wait} s"
            );
            assert_eq!(to(&decided(at + wait)), Some("spare"), "after {wait} s");
            at += wait;
        }
    }

    #[test]
    fn an_account_a_switch_was_refused_over_is_passed_over_for_the_next() {
        let rows = [work_at(96.0), spare(), other()];
        let state = state_of(&rows);
        let mut ledger = Ledger::default();
        let plan = plan(&rows);
        assert_eq!(plan.to.label, "spare");
        ledger.attempt(&plan, NOW);
        ledger.pass_over(&plan, "spare");
        assert_eq!(
            to(&decided_with(
                &state,
                &rows,
                &ledger,
                Threshold::DEFAULT,
                NOW + RETRY_SECONDS
            )),
            Some("other")
        );
    }

    /// A login refused is refused whichever limit the switch was for, so the account is passed
    /// over for every limit of the account in use until the one it was refused for resets.
    #[test]
    fn an_account_refused_over_one_limit_is_passed_over_for_every_limit() {
        let rows = [work_at_both(), spare(), other()];
        let state = state_of(&rows);
        let weekly = plan(&rows);
        assert_eq!(
            (weekly.limit.kind.as_str(), weekly.to.label.as_str()),
            ("weekly_all", "spare")
        );
        let mut ledger = Ledger::default();
        ledger.attempt(&weekly, NOW);
        ledger.pass_over(&weekly, "spare");
        let after = NOW + RETRY_SECONDS;
        let next = |ledger: &Ledger| match decided_with(
            &state,
            &rows,
            ledger,
            Threshold::DEFAULT,
            after,
        ) {
            Decision::Switch(plan) => plan,
            other => panic!("a switch, not {other:?}"),
        };
        let again = next(&ledger);
        assert_eq!(
            (again.limit.kind.as_str(), again.to.label.as_str()),
            ("weekly_all", "other")
        );
        ledger.attempt(&again, after);
        ledger.switched(&again);
        let session = next(&ledger);
        assert_eq!(
            (session.limit.kind.as_str(), session.to.label.as_str()),
            ("session", "other"),
            "the account put back in use, its five-hour limit is not switched to spare either"
        );
    }

    /// A failed attempt holds back every limit at the share until its wait is over, since what
    /// failed was the moment, and the wait names the limit tried at its end: the five-hour
    /// limit waits on the weekly limit's last attempt, and no attempt is made at the weekly
    /// limit again. Named as the weekly limit, the wait promised a try at it that never came.
    #[test]
    fn a_failed_attempt_at_one_limit_holds_back_another() {
        let rows = [work_at_both(), spare()];
        let state = state_of(&rows);
        let weekly = plan(&rows);
        let session = Plan {
            limit: window("session", 96.0),
            ..weekly.clone()
        };
        let mut ledger = Ledger::default();
        for _ in 0..ATTEMPTS {
            ledger.attempt(&weekly, NOW);
        }
        let waiting = Decision::Waiting {
            limit: session.limit.clone(),
            until: NOW + RETRY_SECONDS,
        };
        let at =
            |ledger: &Ledger, now| decided_with(&state, &rows, ledger, Threshold::DEFAULT, now);
        assert_eq!(at(&ledger, NOW + RETRY_SECONDS - 1), waiting);
        let decision = at(&ledger, NOW + RETRY_SECONDS);
        assert!(
            matches!(&decision, Decision::Switch(plan) if plan.limit.kind == "session"),
            "{decision:?}"
        );

        ledger.attempt(&session, NOW - RETRY_SECONDS / 2);
        assert_eq!(
            at(&ledger, NOW + RETRY_SECONDS - 1),
            waiting,
            "the five-hour limit's own wait over first"
        );
    }

    /// A wait names its limit with the reset the ledger keeps that window under, as every limit
    /// a decision gives does: a later reading of the same window may give it a second off.
    #[test]
    fn a_wait_is_told_of_the_window_its_attempt_was_kept_under() {
        let rows = [work_at(96.0), spare()];
        let state = state_of(&rows);
        let plan = plan(&rows);
        let mut ledger = Ledger::default();
        ledger.attempt(&plan, NOW);
        let mut later = work_at(96.0);
        later.usage.as_mut().unwrap().windows[0].resets_at = Some(NOW + 3599);
        assert_eq!(
            decided_with(
                &state,
                &[later, spare()],
                &ledger,
                Threshold::DEFAULT,
                NOW + 1
            ),
            Decision::Waiting {
                limit: plan.limit.clone(),
                until: NOW + RETRY_SECONDS
            }
        );
    }

    /// A limit switched away from once is done with until it resets, and another limit of the
    /// same account at the share is judged on its own. Only the fullest was looked at, so a
    /// five-hour limit at the share went unswitched while the weekly limit already left stood.
    #[test]
    fn a_limit_already_left_does_not_hide_another_at_the_share() {
        let rows = [work_at_both(), spare()];
        let state = state_of(&rows);
        let weekly = plan(&rows);
        assert_eq!(weekly.limit.kind, "weekly_all");
        let mut ledger = Ledger::default();
        ledger.attempt(&weekly, NOW - 3600);
        ledger.switched(&weekly);
        let decision = decided_with(&state, &rows, &ledger, Threshold::DEFAULT, NOW);
        assert!(
            matches!(&decision, Decision::Switch(plan) if plan.limit.kind == "session"),
            "{decision:?}"
        );
    }

    /// A limit put back in use at the share after it was switched away from is not switched
    /// away from again before it resets, and that is said at once. Nothing is tried once the
    /// account has settled, so it was said to be settling first, and five minutes later as
    /// left.
    #[test]
    fn a_limit_put_back_at_the_share_is_said() {
        let rows = [work_at(96.0), spare()];
        let mut state = state_of(&rows);
        let plan = plan(&rows);
        let mut ledger = Ledger::default();
        ledger.attempt(&plan, NOW - 3600);
        ledger.switched(&plan);
        put_in_use(&mut state, NOW - 10);
        assert_eq!(
            decided_with(&state, &rows, &ledger, Threshold::DEFAULT, NOW),
            held(&plan.limit, Skip::AlreadyLeft)
        );
    }

    /// The third failed attempt is said at once, however long the wait after it would be:
    /// nothing is tried again before the limit resets. Said as a wait, it promised a try that
    /// never came, and held the reason back for up to a quarter of an hour.
    #[test]
    fn three_failed_attempts_are_said() {
        let rows = [work_at(96.0), spare()];
        let state = state_of(&rows);
        let plan = plan(&rows);
        let at =
            |ledger: &Ledger, now| decided_with(&state, &rows, ledger, Threshold::DEFAULT, now);
        let mut ledger = Ledger::default();
        for _ in 0..ATTEMPTS {
            ledger.attempt(&plan, NOW);
        }
        assert_eq!(
            at(&ledger, NOW + RETRY_SECONDS / 2),
            held(&plan.limit, Skip::GaveUp)
        );

        let mut ledger = Ledger::default();
        for _ in 0..4 {
            ledger.attempt(&plan, NOW - 3600);
            ledger.waited(&plan);
        }
        for _ in 0..ATTEMPTS {
            ledger.attempt(&plan, NOW);
        }
        assert_eq!(
            at(&ledger, NOW + 300),
            held(&plan.limit, Skip::GaveUp),
            "after failures that waiting may mend"
        );
    }

    /// Settling is said of the limit that would be tried once the account has settled, not of
    /// one already switched away from.
    #[test]
    fn an_account_put_in_use_at_the_share_is_said_to_be_settling() {
        let settling = |limit: Window| {
            held(
                &limit,
                Skip::Settling {
                    until: NOW - 10 + SETTLING_SECONDS,
                },
            )
        };
        let rows = [work_at(96.0), spare()];
        let mut state = state_of(&rows);
        put_in_use(&mut state, NOW - 10);
        assert_eq!(
            decided_with(&state, &rows, &Ledger::default(), Threshold::DEFAULT, NOW),
            settling(window("session", 96.0))
        );

        let rows = [work_at_both(), spare()];
        let weekly = plan(&rows);
        let mut ledger = Ledger::default();
        ledger.attempt(&weekly, NOW - 3600);
        ledger.switched(&weekly);
        assert_eq!(
            decided_with(&state, &rows, &ledger, Threshold::DEFAULT, NOW),
            settling(window("session", 96.0)),
            "the weekly limit already left"
        );
    }

    /// Only Anthropic's answer says which account numbers are of. A reading without one, as
    /// an older Pitboard wrote, is not judged, of the account in use or of one to go to.
    #[test]
    fn a_reading_anthropic_did_not_give_is_not_acted_on() {
        let unanswered = |mut row: Row| {
            row.usage.as_mut().expect("a reading").answered_at = None;
            row
        };
        assert_eq!(
            decided(&[unanswered(work_at(99.0)), spare()]),
            Decision::NoReading
        );
        assert_eq!(
            decided(&[work_at(99.0), unanswered(spare())]),
            held(
                &window("session", 99.0),
                Skip::NoRoom {
                    unread: vec!["spare".into()]
                }
            )
        );
    }

    #[test]
    fn no_room_names_the_accounts_not_asked_yet() {
        let mut unread = row("unread", false, Vec::new());
        unread.usage = None;
        let full = row(
            "full",
            false,
            vec![window("session", 96.0), window("weekly_all", 10.0)],
        );
        assert_eq!(
            decided(&[work_at(97.0), full, unread]),
            held(
                &window("session", 97.0),
                Skip::NoRoom {
                    unread: vec!["unread".into()]
                }
            )
        );
    }

    /// A reason not to switch away from a limit is said once for that limit and its reset, as
    /// the ledger records it: the limit's next window, another limit and another reason are
    /// each another.
    #[test]
    fn a_reason_not_to_switch_is_told_apart_by_its_limit_its_reset_and_its_code() {
        let limit = |kind: &str, scope: Option<&str>, resets_at: i64| Window {
            kind: kind.into(),
            scope: scope.map(str::to_owned),
            percent: 97.0,
            resets_at: Some(resets_at),
            is_active: true,
            severity: None,
            length_seconds: None,
        };
        let session = limit("session", None, 9_000);
        assert_eq!(
            Skip::GaveUp.told_apart("work", &session),
            "skipped/work/session//9000/attempts_spent"
        );
        let keys = [
            Skip::GaveUp.told_apart("work", &session),
            Skip::GaveUp.told_apart("work", &limit("session", None, 27_000)),
            Skip::GaveUp.told_apart("work", &limit("weekly_scoped", Some("Opus"), 9_000)),
            Skip::AlreadyLeft.told_apart("work", &session),
            Skip::GaveUp.told_apart("home", &session),
        ];
        let told_apart: std::collections::BTreeSet<&String> = keys.iter().collect();
        assert_eq!(told_apart.len(), keys.len(), "{keys:?}");
    }

    /// A reason nothing can be judged is told apart by its code and what it names, and not by
    /// when it asks again.
    #[test]
    fn a_reason_nothing_can_be_judged_is_told_apart_by_what_it_names() {
        let unidentified = |until| Blind::Unidentified {
            detail: "could not reach Anthropic".into(),
            until,
        };
        assert_eq!(
            unidentified(60).told_apart(),
            "not-watching/not_identified/could not reach Anthropic"
        );
        assert_eq!(
            unidentified(60).told_apart(),
            unidentified(180).told_apart()
        );
        assert_eq!(
            Blind::SwitchInterrupted.told_apart(),
            "not-watching/switch_interrupted"
        );
        let not_enrolled = |email: &str| Blind::NotEnrolled {
            email: email.into(),
        };
        assert_ne!(
            not_enrolled("me@example.com").told_apart(),
            not_enrolled("you@example.com").told_apart()
        );
    }

    /// A reason is recorded once for each window of a limit, and kept with the attempts until
    /// that window resets: the limit's next window at the share is said again.
    #[test]
    fn a_reason_is_said_once_for_each_window_of_a_limit() {
        let Decision::Hold(mut hold) = held(&window("session", 96.0), Skip::GaveUp) else {
            unreachable!()
        };
        let mut ledger = Ledger::default();
        assert!(!ledger.said(&hold));
        assert!(ledger.say(&hold, NOW));
        assert!(ledger.said(&hold));
        assert!(!ledger.say(&hold, NOW + 60), "said already");
        let no_room = Hold {
            why: Skip::NoRoom { unread: Vec::new() },
            ..hold.clone()
        };
        assert!(!ledger.said(&no_room), "another reason");

        hold.limit.resets_at = Some(NOW + 5 * 3600);
        assert!(!ledger.said(&hold), "the next window");
        assert!(ledger.say(&hold, NOW + 3600));
    }

    /// A reset under a minute off is the same window, and what is said of it is said with the
    /// reset it was kept under.
    #[test]
    fn a_reset_a_second_off_is_the_same_window_and_a_minute_off_is_not() {
        let rows = [work_at(96.0), spare()];
        let mut ledger = Ledger::default();
        let mut plan = plan(&rows);
        let resets = plan.limit.resets_at.unwrap();
        plan.limit.resets_at = Some(resets + 59);
        ledger.attempt(&plan, NOW - 3600);
        ledger.switched(&plan);
        let state = state_of(&rows);
        assert_eq!(
            decided_with(&state, &rows, &ledger, Threshold::DEFAULT, NOW),
            held(&plan.limit, Skip::AlreadyLeft)
        );
        plan.limit.resets_at = Some(resets + 61);
        let mut ledger = Ledger::default();
        ledger.attempt(&plan, NOW - 3600);
        ledger.switched(&plan);
        assert_eq!(
            to(&decided_with(
                &state,
                &rows,
                &ledger,
                Threshold::DEFAULT,
                NOW
            )),
            Some("spare")
        );
    }

    /// A window is kept until no reading of it can still run, a minute past the reset it was
    /// kept under, since a later answer can give that reset up to a minute later. Forgotten at
    /// its own reset, a limit already left was switched away from again when an answer gave
    /// it a second later.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_window_is_kept_while_a_reading_of_it_may_still_run() {
        let rows = [work_at(96.0), spare()];
        let state = state_of(&rows);
        let plan = plan(&rows);
        let kept = plan.limit.resets_at.expect("a reset");
        let mut m = crate::switch::harness::machine("autoswitch-kept-past-reset");
        let clock = std::sync::Arc::new(crate::time::FixedClock::at(NOW));
        m.ctx = m.ctx.clone().with_clock(clock.clone());
        let mut ledger = Ledger::default();
        ledger.attempt(&plan, NOW);
        ledger.switched(&plan);
        ledger.save(&m.ctx, Permit::for_a_test()).expect("saved");

        let mut a_second_later = work_at(96.0);
        a_second_later.usage.as_mut().unwrap().windows[0].resets_at = Some(kept + 1);
        let rows = [a_second_later, spare()];
        clock.advance(kept - NOW);
        assert_eq!(
            decided_with(
                &state,
                &rows,
                &Ledger::load(&m.ctx),
                Threshold::DEFAULT,
                kept
            ),
            held(&plan.limit, Skip::AlreadyLeft)
        );
        clock.advance(59);
        assert_eq!(
            Ledger::load(&m.ctx),
            ledger,
            "a reading of it may still run"
        );
        clock.advance(1);
        assert_eq!(Ledger::load(&m.ctx), Ledger::default(), "none can");
    }

    #[test]
    fn a_share_is_between_fifty_and_ninety_nine() {
        assert_eq!(Threshold::new(49), None);
        assert_eq!(Threshold::new(100), None);
        assert_eq!(Threshold::new(50).map(Threshold::percent), Some(50));
        assert_eq!(Threshold::clamped(120).percent(), 99);
        assert_eq!(Threshold::clamped(-3).percent(), 50);
        assert_eq!(Threshold::default().percent(), 95);
        let lower = Threshold::new(80).unwrap();
        let rows = [work_at(85.0), spare()];
        assert!(matches!(decided(&rows), Decision::Below { .. }));
        assert_eq!(
            to(&decided_with(
                &state_of(&rows),
                &rows,
                &Ledger::default(),
                lower,
                NOW
            )),
            Some("spare")
        );
    }
}
