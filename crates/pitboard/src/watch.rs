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
use pitboard_core::autoswitch::{Auto, Skip, Threshold};
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

/// How often it decides though nothing was written: an account put in use stops settling,
/// and a failed attempt may be tried again, with nobody writing anything.
const DECIDE_EVERY: i64 = 30;

/// Decides once from what Pitboard last measured, asking nobody, and says what it came to.
pub fn once(pitboard: &Pitboard, threshold: Threshold) -> Report {
    match said(pitboard.auto_switch(threshold), threshold, false) {
        Said::Event(report) | Said::Stop(report) => report,
        Said::Idle => Report::done(
            "watch",
            json!({ "event": "idle", "threshold": threshold.percent() }),
            "Nothing to switch now.\n".into(),
        ),
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
        if written != seen || now - decided_at >= DECIDE_EVERY || now < decided_at {
            (seen, decided_at) = (written, now);
            match said(pitboard.auto_switch(threshold), threshold, true) {
                Said::Idle => {}
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
/// stopped the last one may stop the next.
fn untold(told: &mut HashSet<String>, report: &Report) -> bool {
    let key = match &report.result {
        Ok(data) if data["event"] == "switched" => {
            told.clear();
            return true;
        }
        Ok(data) => [
            &data["event"],
            &data["from"],
            &data["limit"]["kind"],
            &data["limit"]["scope"],
            &data["limit"]["resets_at"],
            &data["reason"],
        ]
        .map(Value::to_string)
        .join("/"),
        Err(error) => format!("error/{}", error.code()),
    };
    told.insert(key)
}

enum Said {
    Idle,
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
        Auto::Idle => return Said::Idle,
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
        Auto::NoRoom { from, limit } => Report::done(
            "watch",
            json!({
                "event": "no_room",
                "threshold": percent,
                "from": from,
                "limit": limit_json(&limit),
            }),
            at(format!(
                "{} has used {}, and no other Claude Code account has room below {percent}% in \
                 every limit.\n",
                ui::paint(ui::BOLD, &from),
                words::share_of_limit(&limit),
            )),
        ),
        Auto::Skipped { from, limit, why } => {
            // The command line has a command for giving up on it, which the app has a
            // button for in its place.
            let what = match why {
                Skip::SwitchInterrupted => format!(
                    "{}, or give up on it with `pitboard abandon`",
                    words::not_switching(&why)
                ),
                _ => words::not_switching(&why),
            };
            Report::done(
                "watch",
                json!({
                    "event": "skipped",
                    "threshold": percent,
                    "from": from,
                    "limit": limit_json(&limit),
                    "reason": why.code(),
                }),
                at(format!(
                    "{} has used {}. Pitboard is not switching: {}.\n",
                    ui::paint(ui::BOLD, &from),
                    words::share_of_limit(&limit),
                    what,
                )),
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
}
