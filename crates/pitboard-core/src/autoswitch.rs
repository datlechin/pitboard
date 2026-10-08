//! Switching Claude Code by itself, before the account in use runs out.
//!
//! An agent working through a task stops when its account reaches a limit, while another of
//! the same person's accounts may sit there with room. So, where somebody asks for it,
//! Pitboard switches Claude Code to another of their accounts once any limit of the one in
//! use reaches a share they chose, 95% unless they chose otherwise. Before the limit and not
//! at it: a session already running follows a switch within about half a minute
//! ([`crate::switch::ADOPTION_CEILING_SECONDS`]), and one that has reached its limit has
//! already stopped.
//!
//! Asked for, and never by default: the menu bar app's setting, or `pitboard watch` running
//! in a terminal. This is the one part of Pitboard that picks an account without anybody
//! choosing it, so it is something a person turns on knowing what it is.
//!
//! Claude Code only. A running `codex` never follows a switch, so switching it would leave
//! the agent it was meant to keep going on the account that ran out.
//!
//! It decides from what Pitboard already knows, the readings every front end and every
//! status line record, and asks nobody anything to decide. What it never does:
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

use crate::context::Context;
use crate::provider::ProviderId;
use crate::service::Permit;
use crate::state::{Key, State};
use crate::status::Row;
use crate::usage::{self, Snapshot, Window, same_reset};
use crate::{atomic, home};
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
    pub fn reached(self, percent: f64) -> bool {
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

/// How long to wait after an attempt, `waits` attempts in a row having failed for a reason
/// trying again may mend.
fn retry_after(waits: u32) -> i64 {
    (RETRY_SECONDS << waits.saturating_sub(1).min(4)).min(RETRY_MOST_SECONDS)
}

/// What Pitboard switches by itself, as it decided from what it knows.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The account in use, and its id, which the switch checks is still the one signed in.
    pub from: Key,
    pub from_id: String,
    /// The account to switch to.
    pub to: Key,
    /// The limit of `from` that reached the share, as last read.
    pub limit: Window,
}

impl Plan {
    /// Whether `other` is this switch: the same accounts, for the same window of the same
    /// limit, however far that limit has moved since.
    pub(crate) fn same(&self, other: &Plan) -> bool {
        self.from == other.from
            && self.from_id == other.from_id
            && self.to == other.to
            && self.limit.same_limit(&other.limit)
            && same_reset(
                self.limit.resets_at.unwrap_or(0),
                other.limit.resets_at.unwrap_or(0),
            )
    }
}

/// What Pitboard would do about the Claude Code account in use, from what it knows now.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Decision {
    /// Nothing: no limit has reached the share, or Pitboard has already acted on it, or is
    /// waiting to.
    Stay,
    Switch(Plan),
    /// A limit reached the share, and no other account has room.
    NoRoom {
        from: Key,
        limit: Window,
    },
}

/// What a look came to, for a front end to say.
#[derive(Debug)]
pub enum Auto {
    /// Nothing to do now.
    Idle,
    /// Claude Code switched from `from` to `to`, as each is typed, because `limit` of
    /// `from` reached the share.
    Switched {
        from: String,
        to: String,
        limit: Window,
        adoption: crate::provider::Adoption,
    },
    /// `limit` of `from` reached the share, and no other account has room below it in every
    /// limit.
    NoRoom { from: String, limit: Window },
    /// `limit` of `from` reached the share, and Pitboard will not switch, for `why`.
    Skipped {
        from: String,
        limit: Window,
        why: Skip,
    },
}

/// Why Pitboard does not switch, where it would have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// A switch was interrupted, and the change that finishes it is somebody's to make.
    SwitchInterrupted,
    /// Claude Code keeps its login under another name, which Pitboard does not act on.
    CustomOauth,
    /// Claude Code authenticates some other way, set by these, so a switch would change
    /// nothing its sessions use.
    Overridden(Vec<String>),
}

impl Skip {
    /// A stable code, for `--json` and for telling one apart from another.
    pub fn code(&self) -> &'static str {
        match self {
            Skip::SwitchInterrupted => "switch_interrupted",
            Skip::CustomOauth => "custom_oauth_endpoint",
            Skip::Overridden(_) => "auth_overridden",
        }
    }
}

/// The limit of `reading` furthest past the share, or the one resetting sooner of two as far.
fn reached(reading: &Snapshot, threshold: Threshold, now: i64) -> Option<&Window> {
    reading
        .windows
        .iter()
        .filter(|window| threshold.reached_by(window, now))
        .min_by(|a, b| {
            b.used(now)
                .total_cmp(&a.used(now))
                .then_with(|| sooner(a.resets_at, b.resets_at))
        })
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

/// What Pitboard would do about the Claude Code account in use, from `rows`, as `state`
/// and `ledger` stand at `now`.
pub(crate) fn decide(
    state: &State,
    rows: &[Row],
    ledger: &Ledger,
    threshold: Threshold,
    now: i64,
) -> Decision {
    let claude = |row: &&Row| row.provider == ProviderId::Claude && row.label.is_some();
    let Some(current) = rows.iter().filter(claude).find(|row| row.signed_in) else {
        return Decision::Stay;
    };
    let (Some(from), Some(reading)) = (current.key(), current.usage.as_ref()) else {
        return Decision::Stay;
    };
    let Some(limit) = reached(reading, threshold, now) else {
        return Decision::Stay;
    };
    let settling = state
        .get(&from)
        .and_then(|account| account.last_used_at)
        .is_some_and(|at| (0..SETTLING_SECONDS).contains(&(now - at)));
    let tried = ledger.tried(&current.id, limit);
    let waiting = tried.is_some_and(|tried| {
        tried.switched || tried.attempts >= ATTEMPTS || now - tried.at < retry_after(tried.waits)
    });
    if settling || waiting {
        return Decision::Stay;
    }
    let passed_over = tried.map_or(&[][..], |tried| tried.passed_over.as_slice());
    let best = usage::roomiest(
        rows.iter()
            .filter(claude)
            .filter(|row| row.switchable(now))
            .filter(|row| !passed_over.contains(&row.id))
            .filter_map(|row| {
                let theirs = row.usage.as_ref()?;
                let room = usage::room(
                    &theirs.windows,
                    theirs.lists_every_limit,
                    &reading.windows,
                    limit,
                    threshold.percent(),
                    now,
                )?;
                Some((row, room))
            }),
    );
    match best.and_then(|(row, _)| row.key()) {
        Some(to) => Decision::Switch(Plan {
            from,
            from_id: current.id.clone(),
            to,
            limit: limit.clone(),
        }),
        None => Decision::NoRoom {
            from,
            limit: limit.clone(),
        },
    }
}

/// What Pitboard has tried for each limit of each account that reached the share, kept in
/// Pitboard's directory between runs and shared by every front end that switches by
/// itself. Written only under the lock every change takes, so two of them never act on one
/// limit twice. Nothing here is secret: account ids, names of limits, times and counts.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Ledger {
    #[serde(default)]
    limits: BTreeMap<String, Tried>,
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
    /// What is kept. Nothing kept, and a record that cannot be read, are nothing tried: the
    /// record is written whole the next time anything is tried, and at worst one limit is
    /// tried once more.
    pub(crate) fn load(ctx: &Context) -> Ledger {
        std::fs::read_to_string(path(ctx))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Kept whole, without the windows that have reset.
    pub(crate) fn save(&mut self, ctx: &Context, permit: Permit) -> crate::error::Result<()> {
        let now = ctx.now();
        self.limits.retain(|_, tried| {
            tried.resets_at > now || (tried.resets_at == 0 && now - tried.at < 7 * 86_400)
        });
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

    fn trying(&mut self, plan: &Plan) -> &mut Tried {
        let resets_at = plan.limit.resets_at.unwrap_or(0);
        let tried = self
            .limits
            .entry(entry(&plan.from_id, &plan.limit))
            .or_default();
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
        let tried = self.trying(plan);
        tried.attempts += 1;
        tried.at = now;
    }

    /// The switch for `plan` failed for a reason trying again may mend: not one of the
    /// attempts, and the next waits longer.
    pub(crate) fn waited(&mut self, plan: &Plan) {
        let tried = self.trying(plan);
        tried.attempts = tried.attempts.saturating_sub(1);
        tried.waits = tried.waits.saturating_add(1);
    }

    /// The switch for `plan` was made.
    pub(crate) fn switched(&mut self, plan: &Plan) {
        self.trying(plan).switched = true;
    }

    /// The switch for `plan` was refused over the account it would have switched to, and
    /// the next goes to another.
    pub(crate) fn pass_over(&mut self, plan: &Plan, to_id: &str) {
        let tried = self.trying(plan);
        tried.waits = 0;
        if !tried.passed_over.iter().any(|id| id == to_id) {
            tried.passed_over.push(to_id.to_owned());
        }
    }
}

/// What to do after a look: switch for this plan, or say this.
#[derive(Debug)]
pub(crate) enum Next {
    Switch(Plan),
    Say(Auto),
}

/// What Pitboard would do now, from its files alone: no lock, no keychain and no network, so
/// a look that finds nothing to do costs nobody anything. What it decides is decided again
/// under the lock before anything moves.
pub(crate) fn look(ctx: &Context, state: &State, threshold: Threshold) -> Next {
    let rows = crate::status::gather_offline(ctx, state).rows;
    let plan = match decide(state, &rows, &Ledger::load(ctx), threshold, ctx.now()) {
        Decision::Stay => return Next::Say(Auto::Idle),
        Decision::NoRoom { from, limit } => {
            return Next::Say(Auto::NoRoom {
                from: state.typed(&from),
                limit,
            });
        }
        Decision::Switch(plan) => plan,
    };
    let overridden = crate::provider::of(ProviderId::Claude).overridden_by(ctx);
    let why = if crate::switch::interrupted(ctx) {
        Skip::SwitchInterrupted
    } else if crate::settings::custom_oauth(ctx) {
        Skip::CustomOauth
    } else if !overridden.is_empty() {
        Skip::Overridden(overridden)
    } else {
        return Next::Switch(plan);
    };
    Next::Say(Auto::Skipped {
        from: state.typed(&plan.from),
        limit: plan.limit,
        why,
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

    fn decided(rows: &[Row]) -> Decision {
        decide(
            &state_of(rows),
            rows,
            &Ledger::default(),
            Threshold::DEFAULT,
            NOW,
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

    #[test]
    fn a_limit_below_the_share_is_left_alone_and_one_at_it_switches() {
        assert_eq!(decided(&[work_at(94.4), spare()]), Decision::Stay);
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
        let work = row(
            "work",
            true,
            vec![window("session", 96.0), window("weekly_all", 98.0)],
        );
        let Decision::Switch(plan) = decided(&[work, spare()]) else {
            panic!("a switch")
        };
        assert_eq!(plan.limit.kind, "weekly_all");
    }

    #[test]
    fn a_limit_whose_reset_has_passed_counts_as_reset() {
        let mut work = work_at(100.0);
        work.usage.as_mut().unwrap().windows[0].resets_at = Some(NOW);
        assert_eq!(decided(&[work, spare()]), Decision::Stay);
    }

    #[test]
    fn an_account_in_use_with_no_reading_is_never_switched_from() {
        let mut work = work_at(99.0);
        work.usage = None;
        assert_eq!(decided(&[work, spare()]), Decision::Stay);
    }

    /// A running `codex` never follows a switch, so Codex is never switched by itself.
    #[test]
    fn codex_is_never_switched() {
        let mut work = work_at(99.0);
        let mut other = spare();
        work.provider = ProviderId::Codex;
        other.provider = ProviderId::Codex;
        assert_eq!(decided(&[work, other]), Decision::Stay);
        assert_eq!(
            decided(&[work_at(99.0), {
                let mut codex = spare();
                codex.provider = ProviderId::Codex;
                codex
            }]),
            Decision::NoRoom {
                from: Key::new(ProviderId::Claude, "work"),
                limit: window("session", 99.0)
            }
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
        assert!(matches!(
            decided(&[work_at(96.0), full_weekly()]),
            Decision::NoRoom { .. }
        ));
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
        assert!(matches!(
            decided(&[work_at(96.0), spare_at(94.5)]),
            Decision::NoRoom { .. }
        ));
        assert_eq!(
            to(&decided(&[work_at(96.0), spare_at(94.4)])),
            Some("spare")
        );
    }

    #[test]
    fn an_account_that_cannot_be_switched_to_is_passed_over() {
        let mut expired = spare();
        expired.parked.as_mut().unwrap().refresh_expires_at = Some(NOW);
        assert!(matches!(
            decided(&[work_at(96.0), expired]),
            Decision::NoRoom { .. }
        ));
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
        assert!(matches!(
            decided(&[work_at(96.0), unread]),
            Decision::NoRoom { .. }
        ));
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
        let no_room = |rows: &[Row]| matches!(decided(rows), Decision::NoRoom { .. });
        assert!(
            no_room(&[work_at(96.0), older(vec![window("seven_day", 10.0)])]),
            "the limit at the share"
        );
        assert!(
            no_room(&[work_at(96.0), older(vec![window("five_hour", 30.0)])]),
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
            no_room(&[work_at(96.0), weekly_unread]),
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
            no_room(&[with_opus(10.0, 97.0), both()]),
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
        let work = Key::new(ProviderId::Claude, "work");
        state.used(&work, NOW - SETTLING_SECONDS + 1);
        let ledger = Ledger::default();
        assert_eq!(
            decide(&state, &rows, &ledger, Threshold::DEFAULT, NOW),
            Decision::Stay
        );
        state.used(&work, NOW - SETTLING_SECONDS);
        assert_eq!(
            to(&decide(&state, &rows, &ledger, Threshold::DEFAULT, NOW)),
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
    fn a_limit_switched_away_from_is_not_switched_away_from_again_before_it_resets() {
        let rows = [work_at(96.0), spare()];
        let state = state_of(&rows);
        let mut ledger = Ledger::default();
        let plan = plan(&rows);
        ledger.attempt(&plan, NOW - 3600);
        ledger.switched(&plan);
        assert_eq!(
            decide(&state, &rows, &ledger, Threshold::DEFAULT, NOW),
            Decision::Stay
        );

        let mut next_window = work_at(96.0);
        next_window.usage.as_mut().unwrap().windows[0].resets_at = Some(NOW + 3 * 3600);
        let rows = [next_window, spare()];
        assert_eq!(
            to(&decide(&state, &rows, &ledger, Threshold::DEFAULT, NOW)),
            Some("spare")
        );
    }

    #[test]
    fn a_failed_attempt_is_tried_again_after_a_minute_and_at_most_three_times() {
        let rows = [work_at(96.0), spare()];
        let state = state_of(&rows);
        let mut ledger = Ledger::default();
        let plan = plan(&rows);
        ledger.attempt(&plan, NOW);
        let at = |now| decide(&state, &rows, &ledger, Threshold::DEFAULT, now);
        assert_eq!(at(NOW + RETRY_SECONDS - 1), Decision::Stay);
        assert_eq!(to(&at(NOW + RETRY_SECONDS)), Some("spare"));
        assert_eq!(at(NOW - 1), Decision::Stay, "a clock gone backwards waits");

        ledger.attempt(&plan, NOW);
        ledger.attempt(&plan, NOW);
        let at = |now| decide(&state, &rows, &ledger, Threshold::DEFAULT, now);
        assert_eq!(at(NOW + 3600), Decision::Stay);
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
            let decided = |now| decide(&state, &rows, &ledger, Threshold::DEFAULT, now);
            assert_eq!(decided(at + wait - 1), Decision::Stay, "within {wait} s");
            assert_eq!(to(&decided(at + wait)), Some("spare"), "after {wait} s");
            at += wait;
        }
    }

    #[test]
    fn an_account_a_switch_was_refused_over_is_passed_over_for_the_next() {
        let other = row(
            "other",
            false,
            vec![window("session", 30.0), window("weekly_all", 30.0)],
        );
        let rows = [work_at(96.0), spare(), other];
        let state = state_of(&rows);
        let mut ledger = Ledger::default();
        let plan = plan(&rows);
        assert_eq!(plan.to.label, "spare");
        ledger.attempt(&plan, NOW);
        ledger.pass_over(&plan, "spare");
        assert_eq!(
            to(&decide(
                &state,
                &rows,
                &ledger,
                Threshold::DEFAULT,
                NOW + RETRY_SECONDS
            )),
            Some("other")
        );
    }

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
            decide(&state, &rows, &ledger, Threshold::DEFAULT, NOW),
            Decision::Stay
        );
        plan.limit.resets_at = Some(resets + 61);
        let mut ledger = Ledger::default();
        ledger.attempt(&plan, NOW - 3600);
        ledger.switched(&plan);
        assert_eq!(
            to(&decide(&state, &rows, &ledger, Threshold::DEFAULT, NOW)),
            Some("spare")
        );
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
        assert_eq!(decided(&rows), Decision::Stay);
        assert_eq!(
            to(&decide(
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
