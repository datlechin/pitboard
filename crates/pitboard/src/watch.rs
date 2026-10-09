//! `pitboard watch`: switching Claude Code by itself for as long as somebody leaves it
//! running in a terminal, as the app does with its setting on.
//!
//! A program in the foreground, which stops with its terminal or with Ctrl-C. Nothing
//! installs it and it starts nothing that outlives it: running it is the opt-in. What it
//! decides, and what it never does, is [`pitboard_core::autoswitch`]'s.
//!
//! It asks the services about every account as often as the app does, and looks at what the
//! status lines and the other front ends record as often as the app looks, so it costs
//! Anthropic no more than the app would.

use crate::{Report, emit, followed, ui};
use pitboard_core::autoswitch::{self, Auto, Blind, DECIDE_EVERY_SECONDS, Look, Skip, Threshold};
use pitboard_core::error::{self, Error};
use pitboard_core::provider::ProviderId;
use pitboard_core::service::{Changing, Done, Failed, Pitboard};
use pitboard_core::usage::Window;
use pitboard_core::words;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::process::ExitCode;
use std::time::Duration;

/// How often it looks at when the accounts and their readings were last written: as often as
/// the app looks. A look reads two files' times and nothing else.
const LOOK_EVERY: Duration = Duration::from_secs(2);

/// How often it asks the services about every account: as often as the app reads them. Each
/// account is asked only as often as its budget allows whichever front end asks.
const READ_EVERY: i64 = 300;

/// Decides once from the usage Pitboard last measured, and says what it came to. Nobody is
/// asked for usage; a decision under the lock may ask Anthropic whose login Claude Code has
/// stored, as a switch does.
pub fn once(pitboard: &Pitboard, threshold: Threshold) -> Report {
    said(pitboard.auto_switch(threshold), threshold, false)
}

/// Watches until it is stopped, saying the account it watches whenever its last line said
/// something else, every switch, and each thing that stops one once. A refusal is asked again
/// no sooner than the core waits after as many failed attempts, as the app does.
pub fn run(pitboard: &Pitboard, threshold: Threshold, as_json: bool) -> ExitCode {
    if let Err(refused) = pitboard.permit() {
        return emit(Report::failed(Some("watch"), refused), as_json);
    }
    emit(
        Report::done(
            "watch",
            json!({ "event": "watching", "threshold": threshold.percent() }),
            format!(
                "Watching Claude Code. Pitboard switches to another of your accounts once a \
                 limit of the one in use reaches {}%. Stop with Ctrl-C.\n",
                threshold.percent()
            ),
        ),
        as_json,
    );
    let mut told = Told::default();
    let mut refusals = Refusals::default();
    let (mut read_at, mut decided_at, mut seen) = (None::<i64>, 0, None);
    loop {
        let now = epoch();
        if read_at.is_none_or(|at| now - at >= READ_EVERY || now < at) {
            // What a read could not learn stays as it was measured, which the next decision
            // reads: nothing to say about it here.
            let _ = pitboard.status(false);
            read_at = Some(now);
        }
        let written = Some((pitboard.changed_at(), pitboard.readings_changed_at()));
        if written != seen || now >= refusals.due(decided_at) || now < decided_at {
            (seen, decided_at) = (written, now);
            let outcome = decided(
                refusals.wait(now),
                || pitboard.auto_look(threshold),
                || pitboard.auto_switch(threshold),
            );
            if let Some(outcome) = outcome {
                refusals.after(&outcome, now);
                let report = said(outcome, threshold, true);
                if matches!(&report.result, Err(error) if ends_watching(error)) {
                    return emit(report, as_json);
                }
                if let Some(report) = told.untold(report) {
                    emit(report, as_json);
                }
            }
        }
        std::thread::sleep(LOOK_EVERY);
    }
}

/// What deciding comes to now: `decide`, under the lock, or while `waiting` on a refusal only
/// `look`, which takes no lock and records nothing. Nothing comes of a look where only a
/// decision under the lock can say.
fn decided(
    waiting: bool,
    look: impl FnOnce() -> error::Result<Look>,
    decide: impl FnOnce() -> Changing<Auto>,
) -> Option<Changing<Auto>> {
    if !waiting {
        return Some(decide());
    }
    match look() {
        Ok(Look::Stands(value)) => Some(Ok(Done {
            value: *value,
            warnings: Vec::new(),
        })),
        Ok(Look::Act) => None,
        Err(error) => Some(Err(Failed {
            error,
            warnings: Vec::new(),
        })),
    }
}

/// Refusals of switching by itself in a row, and the wait after them, so the next decision
/// under the lock is no sooner than the core waits after as many failed attempts in a row. A
/// refusal the core raised before it could record anything would otherwise be asked again,
/// and written in the audit log, at every decision. One it recorded, after an attempt, is
/// followed by a look that stands on what the core recorded with it, its own wait or why it
/// tries no more, which ends the row.
#[derive(Default)]
struct Refusals {
    in_a_row: u32,
    /// When the last was.
    at: i64,
    /// Until when the next decision under the lock waits.
    until: i64,
}

impl Refusals {
    /// Whether a decision under the lock waits at `now`. A clock gone back before the last
    /// refusal waits on nothing.
    fn wait(&self, now: i64) -> bool {
        (self.at..self.until).contains(&now)
    }

    /// When to decide next after deciding at `at`: `DECIDE_EVERY_SECONDS` later, or as the
    /// wait ends where that comes first.
    fn due(&self, at: i64) -> i64 {
        let every = at + DECIDE_EVERY_SECONDS;
        if self.wait(at) {
            every.min(self.until)
        } else {
            every
        }
    }

    /// Counts what deciding at `at` came to: a refusal waits longer than the one before it,
    /// and anything else ends the row.
    fn after(&mut self, outcome: &Changing<Auto>, at: i64) {
        *self = match outcome {
            Ok(_) => Refusals::default(),
            Err(_) => {
                let in_a_row = self.in_a_row + 1;
                Refusals {
                    in_a_row,
                    at,
                    until: at + autoswitch::retry_after(in_a_row),
                }
            }
        };
    }
}

/// What `watch` has said since the last switch.
#[derive(Default)]
struct Told {
    /// The account the last line said it watches, while that is the last line.
    watching: Option<String>,
    /// Each reason, wait and error said, by what tells it apart.
    said: HashSet<String>,
    /// Each warning said, by its code and its words.
    warned: HashSet<String>,
}

impl Told {
    /// What of `report` has not been said since the last switch, recorded as said: `report`
    /// with only the warnings not said yet, or nothing where it says nothing new. A switch is
    /// always said, and after it everything may be said again: what stopped the last one may
    /// stop the next. The account it watches is said whenever the last line said something
    /// else. A reason, a wait or an error is said once. A warning not said yet is said with
    /// the line of the decision that found it, though that line was said before. One that
    /// went away and came back is not said again before the next switch: only a decision
    /// under the lock finds warnings, and none may come between to find it gone.
    fn untold(&mut self, mut report: Report) -> Option<Report> {
        let (untold, watching) = match &report.result {
            Ok(data) if data["event"] == "switched" => {
                *self = Told::default();
                (true, None)
            }
            Ok(data) if data["event"] == "idle" => {
                let account = data["account"].as_str().map(str::to_owned);
                (self.watching != account, account)
            }
            Ok(data) => (self.said.insert(told_apart(data)), None),
            Err(error) => (self.said.insert(format!("error/{}", error.code())), None),
        };
        report
            .warnings
            .retain(|warning| self.warned.insert(warning.to_string()));
        if !untold && report.warnings.is_empty() {
            return None;
        }
        self.watching = watching;
        Some(report)
    }
}

/// What tells a reason or a wait apart from another. A reason not to switch away from a limit
/// is told apart by the account, the limit and the reset the core recorded it under. A wait is
/// told apart by the account and when it ends, which each failed attempt moves, and not by the
/// limit it names: the wait is the moment's, and that limit may be one nothing was recorded
/// for, whose reset each answer can give a second apart. A reason it cannot watch is told
/// apart by the account or the cause it names, and not by when it asks again.
fn told_apart(data: &Value) -> String {
    let named = if data["event"] == "waiting" {
        vec![&data["event"], &data["from"], &data["until"]]
    } else {
        vec![
            &data["event"],
            &data["from"],
            &data["account"],
            &data["limit"]["kind"],
            &data["limit"]["scope"],
            &data["limit"]["resets_at"],
            &data["reason"],
            &data["email"],
            &data["detail"],
        ]
    };
    named
        .into_iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

/// What deciding came to, as a report with what it found on the way: `stamped` puts the time
/// before its line, for a terminal left open for hours.
fn said(outcome: Changing<Auto>, threshold: Threshold, stamped: bool) -> Report {
    crate::changed("watch", outcome, |value| {
        let (data, line) = event(value, threshold);
        (data, if stamped { stamp(line) } else { line })
    })
}

/// `line` after the local time.
fn stamp(line: String) -> String {
    format!(
        "{} {line}",
        ui::paint(ui::DIM, pitboard_core::time::local(epoch(), "%H:%M:%S"))
    )
}

/// What `value` says, as `--json` gives it and as a person reads it.
fn event(value: Auto, threshold: Threshold) -> (Value, String) {
    let moment = |at| pitboard_core::time::moment(at, epoch());
    let percent = threshold.percent();
    match value {
        Auto::Watching {
            account,
            nearest,
            as_of,
            held_until,
        } => {
            let line = words::watching(
                &ui::paint(ui::BOLD, &account),
                nearest.as_ref().map(words::share_of_limit).as_deref(),
                as_of.map(moment).as_deref(),
                held_until.map(moment).as_deref(),
            );
            (
                json!({
                    "event": "idle",
                    "threshold": percent,
                    "account": account,
                    "limit": nearest.as_ref().map(limit_json),
                    "as_of": as_of,
                    "held_until": held_until,
                }),
                format!("{line}\n"),
            )
        }
        Auto::Waiting { from, limit, until } => (
            json!({
                "event": "waiting",
                "threshold": percent,
                "from": from,
                "limit": limit_json(&limit),
                "until": until,
            }),
            format!(
                "{} has used {}. Pitboard tries again at {}.\n",
                ui::paint(ui::BOLD, &from),
                words::share_of_limit(&limit),
                moment(until),
            ),
        ),
        Auto::Switched {
            from,
            to,
            limit,
            adoption,
        } => {
            let (_, follows, adoption_json) = followed(ProviderId::Claude, &adoption);
            (
                json!({
                    "event": "switched",
                    "threshold": percent,
                    "from": from,
                    "to": to,
                    "limit": limit_json(&limit),
                    "adoption": adoption_json,
                }),
                format!(
                    "Switched Claude Code from {} to {}: {from} has used {}. {follows}",
                    ui::paint(ui::BOLD, &from),
                    ui::paint(ui::BOLD, &to),
                    words::share_of_limit(&limit),
                ),
            )
        }
        Auto::Skipped { from, limit, why } => {
            let used = format!(
                "{} has used {}",
                ui::paint(ui::BOLD, &from),
                words::share_of_limit(&limit)
            );
            let reason = words::not_switching(&why, threshold);
            match &why {
                // No room has an event of its own, as it always had.
                Skip::NoRoom { unread } => (
                    json!({
                        "event": "no_room",
                        "threshold": percent,
                        "from": from,
                        "limit": limit_json(&limit),
                        "unread": unread,
                    }),
                    format!("{used}, and {reason}.\n"),
                ),
                Skip::Settling { until } => (
                    json!({
                        "event": "skipped",
                        "threshold": percent,
                        "from": from,
                        "limit": limit_json(&limit),
                        "reason": why.code(),
                        "until": until,
                    }),
                    format!(
                        "{used}. Pitboard is not switching: {reason}. It decides again at {}.\n",
                        moment(*until)
                    ),
                ),
                Skip::AlreadyLeft | Skip::GaveUp | Skip::Overridden(_) => (
                    json!({
                        "event": "skipped",
                        "threshold": percent,
                        "from": from,
                        "limit": limit_json(&limit),
                        "reason": why.code(),
                    }),
                    format!("{used}. Pitboard is not switching: {reason}.\n"),
                ),
            }
        }
        Auto::NotWatching { why } => {
            let mut data = json!({
                "event": "not_watching",
                "threshold": percent,
                "reason": why.code(),
            });
            let mut line = words::not_watching(&why);
            match &why {
                // The command line has a command for giving up on it, which the app has a
                // button for in its place.
                Blind::SwitchInterrupted => {
                    line.push_str(", or give up on it with `pitboard abandon`")
                }
                Blind::NotEnrolled { email } => data["email"] = json!(email),
                Blind::NoReading { account } => data["account"] = json!(account),
                Blind::Unidentified { detail, until } => {
                    data["detail"] = json!(detail);
                    data["until"] = json!(until);
                    line = format!("{line}. It asks again at {}", moment(*until));
                }
                _ => {}
            }
            (
                data,
                format!("Pitboard is not switching Claude Code: {line}.\n"),
            )
        }
    }
}

fn limit_json(limit: &Window) -> Value {
    json!({
        "kind": limit.kind,
        "scope": limit.scope,
        "percent": limit.percent,
        "resets_at": limit.resets_at,
    })
}

/// A refusal watching longer cannot mend: this process may change nothing, or Pitboard's
/// own files cannot be read or written. Watched on, it would be refused again every look,
/// and recorded in the audit log each time.
fn ends_watching(error: &Error) -> bool {
    matches!(
        error,
        Error::Elevated { .. }
            | Error::SystemTooOld { .. }
            | Error::WindowsNotReleased
            | Error::HomeNotAbsolute { .. }
            | Error::HomeUnwritable { .. }
            | Error::StateWriteFailed { .. }
            | Error::StateOnSyncedDrive { .. }
            | Error::StateUnreadable { .. }
            | Error::StateCorrupt { .. }
            | Error::StateFromNewerVersion { .. }
            | Error::StateVersionUnknown { .. }
            | Error::StateNamesUnknownTool { .. }
            | Error::StateWrongMachine { .. }
    )
}

/// Seconds since 1970, for when to read and decide next. Not what is decided from: the core
/// reads its own clock.
fn epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_core::autoswitch::RETRY_MOST_SECONDS;
    use pitboard_core::provider::{Adoption, Held};
    use pitboard_core::service::Warning;

    /// What watching cannot mend ends it; what a later look may get past is said and watched
    /// on.
    #[test]
    fn only_what_watching_cannot_mend_ends_it() {
        let unwritable = Error::StateWriteFailed {
            path: "state.json".into(),
            source: std::io::Error::other("read-only"),
        };
        assert!(ends_watching(&unwritable));
        assert!(ends_watching(&Error::Elevated { why: None }));
        assert!(ends_watching(&Error::SystemTooOld { build: Some(22631) }));
        assert!(ends_watching(&Error::SystemTooOld { build: None }));
        assert!(!ends_watching(&Error::SignedInAccountChanged));
        assert!(!ends_watching(&Error::ParkedLoginExpired {
            label: "spare".into()
        }));
    }

    impl Told {
        /// Whether `report` says anything not said since the last switch.
        fn says(&mut self, report: Report) -> bool {
            self.untold(report).is_some()
        }
    }

    /// What `watch` prints of `value`.
    fn told(value: Auto) -> Report {
        found(value, Vec::new())
    }

    /// What `watch` prints of `value`, as a decision that found `warnings` came to it.
    fn found(value: Auto, warnings: Vec<Warning>) -> Report {
        said(Ok(Done { value, warnings }), Threshold::DEFAULT, false)
    }

    fn watching(account: &str) -> Auto {
        Auto::Watching {
            account: account.into(),
            nearest: None,
            as_of: None,
            held_until: None,
        }
    }

    /// The codes of the warnings `report` says.
    fn warned(report: &Report) -> Vec<&str> {
        report
            .warnings
            .iter()
            .filter_map(|warning| warning["code"].as_str())
            .collect()
    }

    fn blind(why: Blind) -> Report {
        told(Auto::NotWatching { why })
    }

    /// A line as it reads with its styles dropped, as anything but a terminal gets it.
    fn plain(report: &Report) -> String {
        anstream::adapter::strip_str(&report.human).to_string()
    }

    /// `work`'s five-hour limit at 97%, resetting at `resets_at`.
    fn limit(resets_at: i64) -> Window {
        Window {
            kind: "session".into(),
            scope: None,
            percent: 97.0,
            resets_at: Some(resets_at),
            is_active: true,
            severity: None,
            length_seconds: Some(5 * 3600),
        }
    }

    fn skipped(resets_at: i64, why: Skip) -> Report {
        told(Auto::Skipped {
            from: "work".into(),
            limit: limit(resets_at),
            why,
        })
    }

    /// Each reason not to switch away from a limit has its code, and no room its own event,
    /// with the accounts Anthropic gave no reading of. Each is said once for the limit and its
    /// reset.
    #[test]
    fn a_reason_not_to_switch_is_said_once_for_a_limit_and_its_reset() {
        let no_room = skipped(
            9_000,
            Skip::NoRoom {
                unread: vec!["spare".into()],
            },
        );
        let data = no_room.result.as_ref().expect("an event");
        assert_eq!(
            (&data["event"], &data["unread"], &data["reason"]),
            (&json!("no_room"), &json!(["spare"]), &Value::Null)
        );
        assert_eq!(
            plain(&no_room),
            "work has used 97% of its 5-hour limit, and no other Claude Code account has room \
             below 95% in every limit, and there is no reading of spare from Anthropic yet.\n"
        );
        for (why, code) in [
            (Skip::AlreadyLeft, "already_switched"),
            (Skip::GaveUp, "attempts_spent"),
            (Skip::Settling { until: 9_000 }, "settling"),
        ] {
            let report = skipped(9_000, why);
            let data = report.result.as_ref().expect("an event");
            assert_eq!(
                (&data["event"], &data["reason"]),
                (&json!("skipped"), &json!(code))
            );
        }
        let settling = skipped(
            9_000,
            Skip::Settling {
                until: 1_760_000_300,
            },
        );
        assert_eq!(
            settling.result.as_ref().expect("an event")["until"],
            1_760_000_300
        );
        assert!(
            plain(&settling).starts_with(
                "work has used 97% of its 5-hour limit. Pitboard is not switching: the account \
                 was put in use less than 5 minutes ago. It decides again at "
            ),
            "{}",
            plain(&settling)
        );

        let mut said = Told::default();
        assert!(said.says(skipped(9_000, Skip::GaveUp)));
        assert!(!said.says(skipped(9_000, Skip::GaveUp)));
        assert!(said.says(skipped(9_000, Skip::AlreadyLeft)));
        assert!(said.says(skipped(27_000, Skip::GaveUp)), "the next window");
    }

    /// A wait after an attempt that came to nothing says when it ends, once for each end,
    /// whichever limit it names and whatever that limit's reset.
    #[test]
    fn a_wait_is_said_once_for_when_it_ends() {
        let waiting_at = |limit: Window, until| {
            told(Auto::Waiting {
                from: "work".into(),
                limit,
                until,
            })
        };
        let waiting = |until| waiting_at(limit(9_000), until);
        let report = waiting(1_760_000_060);
        let data = report.result.as_ref().expect("an event");
        assert_eq!(
            (&data["event"], &data["from"], &data["until"]),
            (&json!("waiting"), &json!("work"), &json!(1_760_000_060))
        );
        assert!(
            plain(&report)
                .starts_with("work has used 97% of its 5-hour limit. Pitboard tries again at "),
            "{}",
            plain(&report)
        );
        let mut said = Told::default();
        assert!(said.says(waiting(1_760_000_060)));
        assert!(!said.says(waiting(1_760_000_060)));
        assert!(
            !said.says(waiting_at(limit(9_001), 1_760_000_060)),
            "a reset a second off"
        );
        let weekly = Window {
            kind: "weekly_all".into(),
            ..limit(600_000)
        };
        assert!(
            !said.says(waiting_at(weekly, 1_760_000_060)),
            "another limit tried at the same end"
        );
        assert!(said.says(waiting(1_760_000_180)));
    }

    /// With nothing to do, it says which account it watches, its fullest limit, when that was
    /// read and until when Anthropic holds Pitboard off asking again.
    #[test]
    fn nothing_to_do_names_the_account_it_watches() {
        let watching = |held_until| {
            told(Auto::Watching {
                account: "work".into(),
                nearest: Some(limit(9_000)),
                as_of: Some(1_760_000_000),
                held_until,
            })
        };
        let report = watching(None);
        let data = report.result.as_ref().expect("an event");
        assert_eq!(
            (
                &data["event"],
                &data["account"],
                &data["limit"]["kind"],
                &data["as_of"],
                &data["held_until"]
            ),
            (
                &json!("idle"),
                &json!("work"),
                &json!("session"),
                &json!(1_760_000_000),
                &Value::Null
            )
        );
        let at = |epoch| pitboard_core::time::moment(epoch, super::epoch());
        assert_eq!(
            plain(&report),
            format!(
                "Watching work: 97% of its 5-hour limit, as of {}.\n",
                at(1_760_000_000)
            )
        );
        assert_eq!(
            plain(&watching(Some(1_760_003_600))),
            format!(
                "Watching work: 97% of its 5-hour limit, as of {}. Anthropic holds Pitboard off \
                 asking again until {}.\n",
                at(1_760_000_000),
                at(1_760_003_600)
            )
        );
    }

    /// The account it watches is said whenever the last line said something else: another
    /// account, a reason, a wait or an error. Said, it is not said again.
    #[test]
    fn the_account_it_watches_is_said_when_that_changes() {
        let watching = |account| told(watching(account));
        let mut said = Told::default();
        assert!(said.says(watching("work")));
        assert!(!said.says(watching("work")));
        assert!(said.says(watching("personal")));
        assert!(said.says(watching("work")), "a switch back by hand");
        assert!(said.says(skipped(9_000, Skip::GaveUp)));
        assert!(said.says(watching("work")), "watching again");
        assert!(!said.says(skipped(9_000, Skip::GaveUp)), "told once");
        assert!(
            !said.says(watching("work")),
            "the last line still names work"
        );
    }

    /// A warning a decision found is said once until the next switch, with the line of the
    /// decision that found it, though that line was said before: a sign-in that leaves a login
    /// in a file puts whose login is stored in doubt, and the decision that settles it comes to
    /// the account already watched.
    #[test]
    fn a_warning_is_said_once_with_the_line_of_the_decision_that_found_it() {
        let fallback = || Warning::FallbackLogin {
            tool: ProviderId::Claude,
            path: "/Users/me/.claude/.credentials.json".into(),
            held: Held::Login,
        };
        let overridden = || Warning::AuthOverridden {
            tool: ProviderId::Claude,
            names: vec!["ANTHROPIC_API_KEY".into()],
        };
        let mut said = Told::default();
        assert!(said.says(told(watching("work"))));
        let again = said
            .untold(found(watching("work"), vec![fallback()]))
            .expect("a warning not said yet");
        assert_eq!(plain(&again), "Watching work.\n");
        assert_eq!(warned(&again), ["fallback_login"]);
        assert!(
            !said.says(found(watching("work"), vec![fallback()])),
            "said once"
        );
        let more = said
            .untold(found(watching("work"), vec![fallback(), overridden()]))
            .expect("another warning");
        assert_eq!(warned(&more), ["auth_overridden"], "and only that one");

        assert!(said.says(skipped(9_000, Skip::GaveUp)));
        let refused = Report::refused(
            "watch",
            Failed {
                error: Error::SignedInAccountChanged,
                warnings: vec![fallback()],
            },
        );
        assert!(said.says(refused), "an error is said once");
        let refused_again = Report::refused(
            "watch",
            Failed {
                error: Error::SignedInAccountChanged,
                warnings: vec![
                    overridden(),
                    Warning::LockCompromised {
                        tool: ProviderId::Claude,
                    },
                ],
            },
        );
        let refused_again = said
            .untold(refused_again)
            .expect("with a warning not said yet");
        assert_eq!(warned(&refused_again), ["lock_compromised"]);
        assert!(
            said.says(told(watching("work"))),
            "the last line said something else"
        );

        let switched = found(
            Auto::Switched {
                from: "work".into(),
                to: "personal".into(),
                limit: limit(9_000),
                adoption: Adoption::PollingWithin(33),
            },
            vec![fallback()],
        );
        let switched = said.untold(switched).expect("a switch");
        assert_eq!(
            warned(&switched),
            ["fallback_login"],
            "said again after a switch"
        );
    }

    /// While a refusal's wait runs only the look is asked, and nothing comes of it where only a
    /// decision under the lock can say. Otherwise only the decision is made, which looks for
    /// itself.
    #[test]
    fn while_a_refusal_waits_only_the_look_is_asked() {
        let under_the_lock = || -> Changing<Auto> { panic!("a decision under the lock") };
        assert!(decided(true, || Ok(Look::Act), under_the_lock).is_none());
        let stands = decided(
            true,
            || Ok(Look::Stands(Box::new(watching("work")))),
            under_the_lock,
        );
        assert!(
            matches!(stands, Some(Ok(Done { value: Auto::Watching { account, .. }, .. })) if account == "work")
        );
        let unread = decided(true, || Err(Error::SignedInAccountChanged), under_the_lock);
        assert!(matches!(unread, Some(Err(_))));

        let made = decided(
            false,
            || panic!("a look of its own"),
            || {
                Ok(Done {
                    value: watching("personal"),
                    warnings: Vec::new(),
                })
            },
        );
        assert!(
            matches!(made, Some(Ok(Done { value: Auto::Watching { account, .. }, .. })) if account == "personal")
        );
    }

    /// After a refusal the next decision under the lock waits as long as the core waits after
    /// as many failed attempts in a row. The look goes on meanwhile, and anything it or a
    /// decision comes to ends the row, as the core's own wait after an attempt it recorded
    /// does.
    #[test]
    fn a_refusal_is_tried_again_only_after_the_cores_wait() {
        const AT: i64 = 1_760_000_000;
        let refused = || {
            Err(Failed {
                error: Error::SignedInAccountChanged,
                warnings: Vec::new(),
            })
        };
        let mut refusals = Refusals::default();
        assert!(!refusals.wait(AT));
        assert_eq!(refusals.due(AT), AT + DECIDE_EVERY_SECONDS);

        refusals.after(&refused(), AT);
        assert!(refusals.wait(AT + 59));
        assert!(!refusals.wait(AT + 60));
        assert!(
            !refusals.wait(AT - 1),
            "a clock gone backwards waits on nothing"
        );
        assert_eq!(
            refusals.due(AT),
            AT + DECIDE_EVERY_SECONDS,
            "it looks meanwhile"
        );
        assert_eq!(
            refusals.due(AT + 45),
            AT + 60,
            "and decides as the wait ends"
        );

        refusals.after(&refused(), AT + 60);
        assert!(refusals.wait(AT + 179));
        assert!(!refusals.wait(AT + 180));
        for _ in 0..8 {
            refusals.after(&refused(), AT);
        }
        assert!(refusals.wait(AT + RETRY_MOST_SECONDS - 1));
        assert!(
            !refusals.wait(AT + RETRY_MOST_SECONDS),
            "at most a quarter of an hour"
        );

        let recorded = Auto::Waiting {
            from: "work".into(),
            limit: limit(9_000),
            until: AT + 260,
        };
        let looked = decided(
            refusals.wait(AT + 200),
            || Ok(Look::Stands(Box::new(recorded))),
            || panic!("a decision under the lock while it waits"),
        );
        refusals.after(&looked.expect("what the look stands on"), AT + 200);
        assert!(!refusals.wait(AT + 200));
        refusals.after(&refused(), AT + 200);
        assert!(
            !refusals.wait(AT + 260),
            "a row ended starts again at a minute"
        );
    }

    /// A reason Pitboard cannot judge whether to switch at all is an event of its own, with
    /// no account or limit, and says what to do where the command line has a command for it.
    #[test]
    fn a_reason_it_cannot_watch_is_an_event_of_its_own() {
        let interrupted = blind(Blind::SwitchInterrupted);
        let data = interrupted.result.as_ref().expect("an event");
        assert_eq!(
            (&data["event"], &data["reason"], &data["threshold"]),
            (
                &json!("not_watching"),
                &json!("switch_interrupted"),
                &json!(95)
            )
        );
        assert_eq!(
            interrupted.human,
            "Pitboard is not switching Claude Code: a switch was interrupted, and the next \
             change you make finishes it, or give up on it with `pitboard abandon`.\n"
        );

        let unread = blind(Blind::NoReading {
            account: "work".into(),
        });
        let data = unread.result.as_ref().expect("an event");
        assert_eq!(
            (&data["reason"], &data["account"]),
            (&json!("no_reading"), &json!("work"))
        );
        assert_eq!(
            unread.human,
            "Pitboard is not switching Claude Code: there is no reading of work from Anthropic \
             yet.\n"
        );

        let stranger = blind(Blind::NotEnrolled {
            email: "me@example.com".into(),
        });
        let data = stranger.result.as_ref().expect("an event");
        assert_eq!(
            (&data["reason"], &data["email"]),
            (&json!("not_enrolled"), &json!("me@example.com"))
        );

        let unidentified = blind(Blind::Unidentified {
            detail: "could not reach Anthropic: no route to host".into(),
            until: 1_760_000_060,
        });
        let data = unidentified.result.as_ref().expect("an event");
        assert_eq!(
            (&data["reason"], &data["detail"], &data["until"]),
            (
                &json!("not_identified"),
                &json!("could not reach Anthropic: no route to host"),
                &json!(1_760_000_060)
            )
        );
        assert!(
            unidentified.human.starts_with(
                "Pitboard is not switching Claude Code: whose login Claude Code has stored \
                 could not be told (could not reach Anthropic: no route to host). It asks \
                 again at "
            ),
            "{}",
            unidentified.human
        );
    }

    /// Each reason, and what it names, is said once: two runs that cannot tell whose login is
    /// stored for one cause are one reason, whenever each asks again.
    #[test]
    fn a_reason_it_cannot_watch_is_said_once_for_what_it_names() {
        let mut told = Told::default();
        let unidentified = |until| {
            blind(Blind::Unidentified {
                detail: "could not reach Anthropic: no route to host".into(),
                until,
            })
        };
        assert!(told.says(unidentified(60)));
        assert!(!told.says(unidentified(180)));
        assert!(told.says(blind(Blind::NotEnrolled {
            email: "me@example.com".into()
        })));
        assert!(told.says(blind(Blind::NotEnrolled {
            email: "you@example.com".into()
        })));
    }
}
