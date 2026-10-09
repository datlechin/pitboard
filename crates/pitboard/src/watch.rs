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
use pitboard_core::autoswitch::{Auto, Blind, DECIDE_EVERY_SECONDS, Skip, Threshold};
use pitboard_core::error::Error;
use pitboard_core::provider::ProviderId;
use pitboard_core::service::{Changing, Done, Pitboard};
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
    match said(pitboard.auto_switch(threshold), threshold, false) {
        Said::Idle(report) | Said::Event(report) | Said::Stop(report) => report,
    }
}

/// Watches until it is stopped, saying each switch, and each thing that stops one, once.
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
    let mut told: HashSet<String> = HashSet::new();
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
        if written != seen || now - decided_at >= DECIDE_EVERY_SECONDS || now < decided_at {
            (seen, decided_at) = (written, now);
            match said(pitboard.auto_switch(threshold), threshold, true) {
                Said::Idle(_) => {}
                Said::Stop(report) => return emit(report, as_json),
                Said::Event(report) => {
                    if untold(&mut told, &report) {
                        emit(report, as_json);
                    }
                }
            }
        }
        std::thread::sleep(LOOK_EVERY);
    }
}

/// Whether this report says something not said since the last switch, recording that it
/// has been said. A switch is always said, and after it everything may be said again: what
/// stopped the last one may stop the next. A reason not to switch away from a limit is told
/// apart by the account, the limit and the reset the core recorded it under. A wait is told
/// apart by the account and when it ends, which each failed attempt moves, and not by the
/// limit it names: the wait is the moment's, and that limit may be one nothing was recorded
/// for, whose reset each answer can give a second apart. A reason it cannot watch is told
/// apart by the account or the cause it names, and not by when it asks again.
fn untold(told: &mut HashSet<String>, report: &Report) -> bool {
    let key = match &report.result {
        Ok(data) if data["event"] == "switched" => {
            told.clear();
            return true;
        }
        Ok(data) => {
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
        Err(error) => format!("error/{}", error.code()),
    };
    told.insert(key)
}

enum Said {
    /// Nothing to do: said only where somebody asked once.
    Idle(Report),
    Event(Report),
    /// Something no amount of watching mends, such as running as root: said, and the end.
    Stop(Report),
}

/// What a decision came to, as a report: `stamped` puts the time before each line, for a
/// terminal left open for hours.
fn said(outcome: Changing<Auto>, threshold: Threshold, stamped: bool) -> Said {
    let at = |line: String| {
        if stamped {
            format!(
                "{} {line}",
                ui::paint(ui::DIM, pitboard_core::time::local(epoch(), "%H:%M:%S"))
            )
        } else {
            line
        }
    };
    let value = match outcome {
        Ok(Done { value, .. }) => value,
        Err(failed) if ends_watching(&failed.error) => {
            return Said::Stop(Report::refused("watch", failed));
        }
        Err(failed) => return Said::Event(Report::refused("watch", failed)),
    };
    let percent = threshold.percent();
    let report = match value {
        Auto::Watching {
            account,
            nearest,
            as_of,
            held_until,
        } => {
            return Said::Idle(Report::done(
                "watch",
                json!({
                    "event": "idle",
                    "threshold": percent,
                    "account": account,
                    "limit": nearest.as_ref().map(limit_json),
                    "as_of": as_of,
                    "held_until": held_until,
                }),
                "Nothing to switch now.\n".into(),
            ));
        }
        Auto::Waiting { from, limit, until } => Report::done(
            "watch",
            json!({
                "event": "waiting",
                "threshold": percent,
                "from": from,
                "limit": limit_json(&limit),
                "until": until,
            }),
            at(format!(
                "{} has used {}. Pitboard tries again at {}.\n",
                ui::paint(ui::BOLD, &from),
                words::share_of_limit(&limit),
                pitboard_core::time::moment(until, epoch()),
            )),
        ),
        Auto::Switched {
            from,
            to,
            limit,
            adoption,
        } => {
            let (_, follows, adoption_json) = followed(ProviderId::Claude, &adoption);
            Report::done(
                "watch",
                json!({
                    "event": "switched",
                    "threshold": percent,
                    "from": from,
                    "to": to,
                    "limit": limit_json(&limit),
                    "adoption": adoption_json,
                }),
                at(format!(
                    "Switched Claude Code from {} to {}: {from} has used {}. {follows}",
                    ui::paint(ui::BOLD, &from),
                    ui::paint(ui::BOLD, &to),
                    words::share_of_limit(&limit),
                )),
            )
        }
        Auto::Skipped { from, limit, why } => {
            let used = format!(
                "{} has used {}",
                ui::paint(ui::BOLD, &from),
                words::share_of_limit(&limit)
            );
            let reason = words::not_switching(&why, threshold);
            let (data, line) = match &why {
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
                        pitboard_core::time::moment(*until, epoch())
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
            };
            Report::done("watch", data, at(line))
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
                    line = format!(
                        "{line}. It asks again at {}",
                        pitboard_core::time::moment(*until, epoch())
                    );
                }
                Blind::CustomOauth | Blind::NothingSignedIn => {}
            }
            Report::done(
                "watch",
                data,
                at(format!("Pitboard is not switching Claude Code: {line}.\n")),
            )
        }
    };
    Said::Event(report)
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

    /// What `watch` prints of `value`, where that is an event.
    fn told(value: Auto) -> Report {
        let looked = Ok(Done {
            value,
            warnings: Vec::new(),
        });
        let Said::Event(report) = said(looked, Threshold::DEFAULT, false) else {
            panic!("an event");
        };
        report
    }

    fn event(why: Blind) -> Report {
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

        let mut said = HashSet::new();
        assert!(untold(&mut said, &skipped(9_000, Skip::GaveUp)));
        assert!(!untold(&mut said, &skipped(9_000, Skip::GaveUp)));
        assert!(untold(&mut said, &skipped(9_000, Skip::AlreadyLeft)));
        assert!(
            untold(&mut said, &skipped(27_000, Skip::GaveUp)),
            "the next window"
        );
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
        let mut said = HashSet::new();
        assert!(untold(&mut said, &waiting(1_760_000_060)));
        assert!(!untold(&mut said, &waiting(1_760_000_060)));
        assert!(
            !untold(&mut said, &waiting_at(limit(9_001), 1_760_000_060)),
            "a reset a second off"
        );
        let weekly = Window {
            kind: "weekly_all".into(),
            ..limit(600_000)
        };
        assert!(
            !untold(&mut said, &waiting_at(weekly, 1_760_000_060)),
            "another limit tried at the same end"
        );
        assert!(untold(&mut said, &waiting(1_760_000_180)));
    }

    /// With nothing to do, `--once` says which account it watches, its fullest limit, when that
    /// was read and until when Anthropic holds Pitboard off asking again.
    #[test]
    fn nothing_to_do_names_the_account_it_watches() {
        let looked = Ok(Done {
            value: Auto::Watching {
                account: "work".into(),
                nearest: Some(limit(9_000)),
                as_of: Some(1_760_000_000),
                held_until: None,
            },
            warnings: Vec::new(),
        });
        let Said::Idle(report) = said(looked, Threshold::DEFAULT, false) else {
            panic!("nothing to do");
        };
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
        assert_eq!(report.human, "Nothing to switch now.\n");
    }

    /// A reason Pitboard cannot judge whether to switch at all is an event of its own, with
    /// no account or limit, and says what to do where the command line has a command for it.
    #[test]
    fn a_reason_it_cannot_watch_is_an_event_of_its_own() {
        let interrupted = event(Blind::SwitchInterrupted);
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

        let unread = event(Blind::NoReading {
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

        let stranger = event(Blind::NotEnrolled {
            email: "me@example.com".into(),
        });
        let data = stranger.result.as_ref().expect("an event");
        assert_eq!(
            (&data["reason"], &data["email"]),
            (&json!("not_enrolled"), &json!("me@example.com"))
        );

        let unidentified = event(Blind::Unidentified {
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
        let mut told = HashSet::new();
        let unidentified = |until| {
            event(Blind::Unidentified {
                detail: "could not reach Anthropic: no route to host".into(),
                until,
            })
        };
        assert!(untold(&mut told, &unidentified(60)));
        assert!(!untold(&mut told, &unidentified(180)));
        assert!(untold(
            &mut told,
            &event(Blind::NotEnrolled {
                email: "me@example.com".into()
            })
        ));
        assert!(untold(
            &mut told,
            &event(Blind::NotEnrolled {
                email: "you@example.com".into()
            })
        ));
    }
}
