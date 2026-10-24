//! `pitboard warm`: which accounts Pitboard warms up, and when. What a warm-up is, and what it
//! never does, is [`pitboard_core::duty::warm`]'s.

use crate::ui::{BOLD, DIM, columns, pad, paint};
use crate::watch::epoch;
use crate::{Report, changed};
use clap::Subcommand;
use pitboard_core::duty::warm::{self, Overview, Range, Standing};
use pitboard_core::service::Pitboard;
use pitboard_core::switch::warm::WarmUp;
use pitboard_core::words;
use serde_json::{Value, json};

#[derive(Subcommand)]
pub(crate) enum WarmCommand {
    /// Start an account's five-hour period now, whatever the span, through one short request
    /// Claude Code makes with that account's login
    Now {
        /// The label the account was enrolled under, such as `work`
        label: String,
    },
    /// Warm up an account from now on, within the span `range` sets
    On {
        /// The label the account was enrolled under, such as `work`
        label: String,
    },
    /// Stop warming up an account
    Off {
        /// The label the account was enrolled under
        label: String,
    },
    /// Warm up accounts only between these times of each day, in this computer's time zone
    Range {
        /// When to start, such as 06:00
        #[arg(value_name = "FROM")]
        from: String,
        /// When to stop, such as 22:00; 00:00 and 24:00 are midnight, and a time before FROM runs
        /// past it
        #[arg(value_name = "TO")]
        to: String,
    },
}

pub(crate) fn run(pitboard: &Pitboard, what: Option<&WarmCommand>) -> Report {
    match what {
        None => overview(pitboard),
        Some(WarmCommand::Now { label }) => now(pitboard, label),
        Some(WarmCommand::On { label }) => turn(pitboard, label, true),
        Some(WarmCommand::Off { label }) => turn(pitboard, label, false),
        Some(WarmCommand::Range { from, to }) => match Range::parse(from, to) {
            Ok(range) => range_set(pitboard, range),
            Err(why) => Report::failed(
                Some("warm"),
                pitboard_core::error::Error::Usage(format!("`{from} {to}`: {why}")),
            ),
        },
    }
}

pub(crate) fn range_json(range: Range) -> Value {
    json!({ "from": warm::clock(range.from()), "to": warm::clock(range.to()) })
}

/// That `account` was warmed up, and until when its period runs where that is known.
pub(crate) fn warmed_line(account: &str, until: Option<i64>) -> String {
    let account = paint(BOLD, account);
    match until {
        Some(until) => format!(
            "Warmed up {account}: its 5-hour period runs until {}.",
            pitboard_core::time::moment(until, epoch())
        ),
        None => format!(
            "Warmed up {account}. Anthropic has not said yet when its 5-hour period resets."
        ),
    }
}

fn overview(pitboard: &Pitboard) -> Report {
    let overview = match pitboard.warm_overview() {
        Ok(overview) => overview,
        Err(error) => return Report::failed(Some("warm"), error),
    };
    let data = json!({
        "range": range_json(overview.range),
        "halted": overview.halted.as_ref().map(|halt| json!({ "reason": halt.code() })),
        "accounts": overview.accounts.iter().map(|choice| {
            let mut account = json!({ "label": choice.account, "on": choice.on });
            if let Some(standing) = &choice.standing {
                merge(&mut account, standing_json(standing));
            }
            account
        }).collect::<Vec<_>>(),
    });
    Report::done("warm", data, said(&overview))
}

fn merge(into: &mut Value, from: Value) {
    if let (Some(into), Value::Object(from)) = (into.as_object_mut(), from) {
        into.extend(from);
    }
}

/// Where an account turned on stands, as `--json` gives it: `state`, with when its next
/// warm-up is due as `next_at` where that is known, and why it is held back as `reason`.
fn standing_json(standing: &Standing) -> Value {
    match standing {
        Standing::Due => json!({ "state": "due" }),
        Standing::Running { until, next } => {
            json!({ "state": "running", "until": until, "next_at": next })
        }
        Standing::Resting { at } => json!({ "state": "resting", "next_at": at }),
        Standing::Waiting { until } => json!({ "state": "waiting", "next_at": until }),
        Standing::Held(hold) => {
            json!({ "state": "held", "reason": hold.code(), "until": hold.until() })
        }
    }
}

/// The span, and each Claude Code account with whether it is warmed up and where it stands,
/// as a person reads it.
fn said(overview: &Overview) -> String {
    if overview.accounts.is_empty() {
        return "No Claude Code account is enrolled, so there is nothing to warm up.\n".into();
    }
    let width = overview
        .accounts
        .iter()
        .map(|choice| columns(&choice.account))
        .max()
        .unwrap_or_default();
    let mut text = format!(
        "Pitboard warms up these accounts from {}, while the app or `pitboard watch` runs.\n",
        overview.range
    );
    let now = epoch();
    let moment = |at| pitboard_core::time::moment(at, now);
    for choice in &overview.accounts {
        let name = paint(BOLD, pad(&choice.account, width));
        match &choice.standing {
            Some(standing) => text.push_str(&format!(
                "  {name}  on   {}\n",
                words::warm_standing(standing, &moment)
            )),
            None => text.push_str(&format!("  {name}  {}\n", paint(DIM, "off"))),
        }
    }
    if let Some(halt) = &overview.halted {
        text.push_str(&format!("{}\n", words::warming_halted(halt)));
    } else if !overview.accounts.iter().any(|choice| choice.on) {
        text.push_str("Turn it on for an account with `pitboard warm on <account>`.\n");
    }
    text
}

fn now(pitboard: &Pitboard, label: &str) -> Report {
    changed("warm", pitboard.warm_now(label), |warmed| {
        let moment = |at| pitboard_core::time::moment(at, epoch());
        match warmed {
            WarmUp::Started { account, until } => (
                json!({ "label": account, "outcome": "started", "until": until }),
                format!("{}\n", warmed_line(&account, until)),
            ),
            WarmUp::Running { account, until } => (
                json!({ "label": account, "outcome": "running", "until": until }),
                format!(
                    "{}'s 5-hour period runs already, until {}.\n",
                    paint(BOLD, &account),
                    moment(until)
                ),
            ),
            WarmUp::Nothing | WarmUp::Stands(_) => (
                json!({ "label": label, "outcome": "nothing" }),
                "Nothing was warmed up.\n".to_owned(),
            ),
        }
    })
}

fn turn(pitboard: &Pitboard, label: &str, on: bool) -> Report {
    changed("warm", pitboard.warm_turn(label, on), |changed| {
        (
            json!({ "label": label, "on": on, "changed": changed }),
            turned(label, on, changed),
        )
    })
}

fn turned(label: &str, on: bool, changed: bool) -> String {
    match (on, changed) {
        (true, true) => format!(
            "Pitboard warms up {} from now on, within the span `pitboard warm` shows.\n",
            paint(BOLD, label)
        ),
        (true, false) => format!("{} was warmed up already.\n", paint(BOLD, label)),
        (false, true) => format!("Pitboard no longer warms up {}.\n", paint(BOLD, label)),
        (false, false) => format!("{} was not warmed up.\n", paint(BOLD, label)),
    }
}

fn range_set(pitboard: &Pitboard, range: Range) -> Report {
    changed("warm", pitboard.warm_range(range), |range| {
        (
            json!({ "range": range_json(range) }),
            format!("Pitboard warms up accounts from {range}.\n"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_core::duty::warm::{Choice, Halt};

    fn overview(halted: Option<Halt>, on: bool) -> Overview {
        Overview {
            range: Range::DEFAULT,
            halted,
            accounts: vec![Choice {
                account: "work".into(),
                qualified: "claude/work".into(),
                on,
                standing: on.then_some(Standing::Due),
                now: Standing::Due,
            }],
        }
    }

    #[test]
    fn a_halt_is_said_under_the_accounts_on_every_system() {
        let said = anstream::adapter::strip_str(&said(&overview(Some(Halt::ProgramMissing), true)))
            .to_string();
        assert!(
            said.starts_with("Pitboard warms up these accounts from 06:00 to 22:00"),
            "{said}"
        );
        assert!(
            said.ends_with(
                "\nPitboard is not warming up accounts: Claude Code is not installed where \
                 Pitboard looks.\n"
            ),
            "{said}"
        );
    }

    #[test]
    fn warm_on_and_off_say_what_they_changed() {
        let plain = |said: String| anstream::adapter::strip_str(&said).to_string();
        assert_eq!(
            plain(turned("work", true, true)),
            "Pitboard warms up work from now on, within the span `pitboard warm` shows.\n"
        );
        assert_eq!(
            plain(turned("work", true, false)),
            "work was warmed up already.\n"
        );
        assert_eq!(
            plain(turned("work", false, true)),
            "Pitboard no longer warms up work.\n"
        );
        assert_eq!(
            plain(turned("work", false, false)),
            "work was not warmed up.\n"
        );
    }
}
