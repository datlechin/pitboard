//! `pitboard watch`: switching Claude Code by itself, and warming up the accounts turned on
//! for it, for as long as somebody leaves it running in a terminal, as the app does, or from
//! each sign-in on, where `pitboard watch install` had the system's own service manager run it
//! ([`pitboard_core::watch_service`]).
//!
//! In a terminal it stops with the terminal or with Ctrl-C, and starts nothing that outlives
//! it: running it, or installing it, is the opt-in. What it decides, and what it never does, is
//! [`pitboard_core::duty::autoswitch`]'s and [`pitboard_core::duty::warm`]'s.
//!
//! It asks the services about every account as often as the app does, and sees what the
//! status lines and the other front ends record as it is written, as the app does, so it costs
//! Anthropic no more than the app would. It waits on Pitboard's directory between reads and
//! decisions ([`pitboard_core::duty::written`]), and looks every 2 seconds where nothing can
//! watch it.

use crate::{Report, emit, followed, located, print_report, ui};
use clap::Subcommand;
use pitboard_core::duty::autoswitch::{Auto, Blind, Look, Policy, Skip, Threshold, Why};
use pitboard_core::duty::cadence::{DECIDE_EVERY_SECONDS, READ_EVERY_SECONDS, Refusals};
use pitboard_core::duty::warm::{Overview, Standing};
use pitboard_core::error::{self, Error};
use pitboard_core::host::Os;
use pitboard_core::provider::ProviderId;
use pitboard_core::service::{Changing, Done, Failed, Pitboard};
use pitboard_core::switch::warm::{WarmUp, told_apart};
use pitboard_core::usage::{Order, Window};
use pitboard_core::watch_service::Switching;
use pitboard_core::words;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::process::ExitCode;
use std::time::Duration;

/// The share `watch` and its service switch at, or that they never switch.
#[derive(clap::Args)]
pub(crate) struct Share {
    /// Switch once a five-hour limit reaches this share, from 50 to 99; weekly limits use it too unless --weekly-at is set
    #[arg(
        long,
        value_name = "PERCENT",
        default_value_t = Threshold::DEFAULT.percent(),
        value_parser = clap::value_parser!(u8)
            .range(i64::from(Threshold::LOWEST)..=i64::from(Threshold::HIGHEST)),
    )]
    at: u8,
    /// The share for weekly limits, from 50 to 99, or off; defaults to --at
    #[arg(long, value_name = "PERCENT|off")]
    weekly_at: Option<WeeklyShare>,
    /// Which account to switch to first: `most-left`, the one with the most room, or
    /// `resets-soonest`, the one whose week resets soonest, even before the share
    #[arg(
        long,
        value_name = "ORDER",
        value_enum,
        default_value_t = First::MostLeft,
        hide_possible_values = true
    )]
    order: First,
    /// Never switch, and only warm up the accounts `pitboard warm on` turned on
    #[arg(long, conflicts_with_all = ["at", "weekly_at", "order"])]
    no_switch: bool,
}

#[derive(Clone)]
enum WeeklyShare {
    At(Threshold),
    Off,
}

impl std::str::FromStr for WeeklyShare {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "off" {
            return Ok(Self::Off);
        }
        value
            .parse::<u8>()
            .ok()
            .and_then(Threshold::new)
            .map(Self::At)
            .ok_or_else(|| "weekly share must be from 50 to 99, or off".into())
    }
}

/// [`Order`], as `--order` takes it.
#[derive(Clone, Copy, clap::ValueEnum)]
enum First {
    MostLeft,
    ResetsSoonest,
}

impl Share {
    // The parser takes only a share there can be.
    pub(crate) fn policy(&self) -> Policy {
        Policy {
            threshold: Threshold::new(self.at).unwrap_or_default(),
            weekly: match self.weekly_at {
                Some(WeeklyShare::At(at)) => Some(at),
                Some(WeeklyShare::Off) => None,
                None => Threshold::new(self.at),
            },
            order: match self.order {
                First::MostLeft => Order::MostLeft,
                First::ResetsSoonest => Order::ResetsSoonest,
            },
        }
    }

    pub(crate) fn switching(&self) -> Switching {
        if self.no_switch {
            Switching::Never
        } else {
            Switching::At(self.policy())
        }
    }
}

// What a line of `watch`'s JSON says it does at a limit: the share and the order, or none, and
// whether it switches, none where an installed service does not say.
fn switching_json(switching: Option<Switching>) -> Value {
    let policy = switching.and_then(Switching::policy);
    json!({
        "threshold": policy.map(|policy| policy.threshold.percent()),
        "weekly_threshold": policy.and_then(|policy| policy.weekly).map(Threshold::percent),
        "order": policy.map(|policy| policy.order.code()),
        "switches": switching.map(|switching| switching != Switching::Never),
    })
}

#[derive(Subcommand)]
pub(crate) enum WatchCommand {
    /// Have this computer's own service manager run `pitboard watch` from now on and at each
    /// sign-in
    Install {
        #[command(flatten)]
        share: Share,
    },
    /// Say whether it is installed
    Status,
    /// Stop it and take it away
    Uninstall,
}

/// Installs, reports or takes away the watch service, as `schedule` does the renewal schedule.
pub(crate) fn service(pitboard: &Pitboard, what: &WatchCommand) -> Report {
    use pitboard_core::watch_service::Installed;
    let default = pitboard.shown(&pitboard.default_directory());
    let watches_only = format!(
        "The watch service watches only the accounts in {default}, not those in the directory \
         PITBOARD_HOME names."
    );
    let this_home = pitboard.schedule_renews_this_home();
    match what {
        WatchCommand::Install { share } => {
            let switching = share.switching();
            match pitboard.watch_service_install(switching) {
                Err(error) => Report::failed(Some("watch"), error),
                Ok(location) => {
                    let linger = match pitboard_core::host::OS {
                        Os::Linux => {
                            "Your user's systemd runs it while you are signed in. To keep it \
                             running when you are not, run `loginctl enable-linger`.\n"
                        }
                        Os::MacOs | Os::Windows => "",
                    };
                    Report::done(
                        "watch",
                        located(json!(true), &location, switching_json(Some(switching))),
                        match switching {
                            Switching::At(policy) => format!(
                                "Pitboard watches Claude Code in the background from now on, and \
                                 at each sign-in. It switches once {}{}, and warms up the accounts `pitboard warm on` \
                                 turned on.\n{}{location}\n\n{}\n",
                                words::limit_policy(policy),
                                words::first_by(policy.order),
                                linger,
                                terms()
                            ),
                            Switching::Never => format!(
                                "Pitboard watches Claude Code in the background from now on, and \
                                 at each sign-in. It never switches, and only warms up the \
                                 accounts `pitboard warm on` turned on.\n{linger}{location}\n\n\
                                 {}\n",
                                terms()
                            ),
                        },
                    )
                }
            }
        }
        WatchCommand::Status => {
            let (data, human) = match pitboard.watch_service() {
                Installed::Yes {
                    location,
                    switching,
                } => (
                    located(json!(true), &location, switching_json(switching)),
                    format!(
                        "Pitboard watches Claude Code in the background{}.\n{location}\n{}",
                        match switching {
                            Some(Switching::At(policy)) => format!(
                                ", and switches once {}{}",
                                words::limit_policy(policy),
                                words::first_by(policy.order)
                            ),
                            Some(Switching::Never) => ", and never switches: it only warms up the \
                                                       accounts `pitboard warm on` turned on"
                                .to_string(),
                            None => String::new(),
                        },
                        if this_home {
                            String::new()
                        } else {
                            format!("{watches_only}\n")
                        }
                    ),
                ),
                Installed::No => (
                    json!({ "installed": false }),
                    format!(
                        "Nothing watches Claude Code in the background here.\n{}\n",
                        if this_home {
                            "Run `pitboard watch install` to change that.".to_string()
                        } else {
                            format!(
                                "{watches_only} To install it for {default}, unset \
                                 PITBOARD_HOME, then run `pitboard watch install`."
                            )
                        }
                    ),
                ),
                Installed::Unsupported => (
                    json!({ "installed": false, "supported": false }),
                    format!(
                        "{}\n",
                        words::no_scheduler(
                            pitboard_core::host::OS,
                            pitboard_core::schedule::Job::Watch
                        )
                    ),
                ),
            };
            Report::done("watch", data, human)
        }
        WatchCommand::Uninstall => match pitboard.watch_service_uninstall() {
            Err(error) => Report::failed(Some("watch"), error),
            Ok(removed) => Report::done(
                "watch",
                json!({ "installed": false, "removed": removed }),
                if removed {
                    "Stopped watching Claude Code in the background.\n".to_string()
                } else {
                    "Nothing was watching Claude Code in the background.\n".to_string()
                },
            ),
        },
    }
}

pub(crate) fn terms() -> String {
    format!(
        "{}: {}",
        words::TERMS_MAY_COVER_THIS,
        words::IS_THIS_ALLOWED_URL
    )
}

/// Decides once from the usage Pitboard last measured, and says what it came to. Nobody is
/// asked for usage; a decision under the lock may ask Anthropic whose login Claude Code has
/// stored, as a switch does.
pub(crate) fn once(pitboard: &Pitboard, policy: Policy) -> Report {
    said(pitboard.auto_switch(policy), policy, false)
}

/// Watches until it is stopped, saying the account it watches whenever its last line said
/// something else, every switch, and each thing that stops one once. A refusal is asked again
/// no sooner than the core waits after as many failed attempts, as the app does. `scheduled`
/// is a run of the watch service, which no Ctrl-C stops. Where `switching` says it never
/// switches, it only reads and warms up.
pub(crate) fn run(
    pitboard: &Pitboard,
    switching: Switching,
    scheduled: bool,
    as_json: bool,
) -> ExitCode {
    if let Err(refused) = pitboard.permit() {
        return emit(&Report::failed(Some("watch"), refused), as_json);
    }
    let warming = pitboard.warm_overview().ok();
    print_report(
        &Report::done(
            "watch",
            watching_json(switching, warming.as_ref()),
            started(switching, warming.as_ref(), scheduled),
        ),
        as_json,
    );
    let mut told = Told::default();
    let mut refusals = Refusals::default();
    let mut warm_told = WarmTold::default();
    let mut warm_refusals = Refusals::default();
    let mut written = pitboard.written();
    let (mut read_at, mut decided_at, mut seen) = (None::<i64>, 0, None);
    loop {
        let now = epoch();
        if read_at.is_none_or(|at| now - at >= READ_EVERY_SECONDS || now < at) {
            // What a read could not learn stays as it was measured, which the next decision
            // reads: nothing to say about it here.
            let _ = pitboard.status(false);
            read_at = Some(now);
        }
        let stamps = Some((pitboard.changed_at(), pitboard.readings_changed_at()));
        let due = refusals.due(instant(decided_at), every());
        if stamps != seen || instant(now) >= due || now < decided_at {
            (seen, decided_at) = (stamps, now);
            let outcome = switching.policy().and_then(|policy| {
                decided(
                    refusals.waits(instant(now)),
                    || pitboard.auto_look(policy),
                    || pitboard.auto_switch(policy),
                )
                .map(|outcome| (outcome, policy))
            });
            if let Some((outcome, policy)) = outcome {
                match &outcome {
                    Ok(_) => refusals.ended(),
                    Err(_) => {
                        refusals.refused(instant(now));
                    }
                }
                let say = |outcome| said(outcome, policy, true);
                if matches!(&outcome, Err(failed) if ends_watching(&failed.error)) {
                    return emit(&say(outcome), as_json);
                }
                if let Some(report) = told.untold(outcome, say) {
                    print_report(&report, as_json);
                }
            }
            // Nothing a warm-up comes to ends watching: switching goes on, and the warm-up is
            // tried again once its refusals have waited.
            if !warm_refusals.waits(instant(now)) {
                let outcome = pitboard.warm_up();
                match &outcome {
                    Ok(_) => warm_refusals.ended(),
                    Err(_) => {
                        warm_refusals.refused(instant(now));
                    }
                }
                for report in warm_told.untold(outcome) {
                    print_report(&report, as_json);
                }
            }
        }
        // Whatever it came to, the loop looks at when each was last written again.
        let _ = written.wait(waits(
            instant(epoch()),
            read_at,
            refusals.due(instant(decided_at), every()),
        ));
    }
}

/// The line `watch` starts with: what it does at a limit, what it warms up, and how it stops.
fn started(switching: Switching, warming: Option<&Overview>, scheduled: bool) -> String {
    let warmed = warming.map(warms).unwrap_or_default();
    let stop = if scheduled {
        "Stop it with `pitboard watch uninstall`."
    } else {
        "Stop with Ctrl-C."
    };
    match switching {
        Switching::At(policy) => format!(
            "Watching Claude Code. Pitboard switches to another of your accounts once \
             {}{}{}. {}\n",
            words::limit_policy(policy),
            words::first_by(policy.order),
            warmed,
            stop
        ),
        Switching::Never if warming.is_some_and(|overview| !warms_any(overview)) => format!(
            "Watching Claude Code. Pitboard never switches accounts, and no account is turned on \
             for warm-ups: turn one on with `pitboard warm on <account>`. {stop}\n"
        ),
        Switching::Never => {
            format!("Watching Claude Code. Pitboard never switches accounts{warmed}. {stop}\n")
        }
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
        Err(error) => Some(Err(Failed::refused(error))),
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
    /// What of `outcome` has not been said since the last switch, recorded as said: what `say`
    /// makes of it, with only the warnings not said yet, or nothing where it says nothing new.
    /// A switch is always said, and after it everything may be said again: what stopped the
    /// last one may stop the next. The account it watches is said whenever the last line said
    /// something else. A reason or a wait is said once for what the core tells it apart by
    /// ([`Auto::told_apart`]), and an error once for its code. A warning not said yet is said
    /// with the line of the decision that found it, though that line was said before. One that
    /// went away and came back is not said again before the next switch: only a decision
    /// under the lock finds warnings, and none may come between to find it gone.
    fn untold(
        &mut self,
        outcome: Changing<Auto>,
        say: impl FnOnce(Changing<Auto>) -> Report,
    ) -> Option<Report> {
        let (untold, watching) = match &outcome {
            Ok(done) => match &done.value {
                Auto::Switched { .. } => {
                    *self = Told::default();
                    (true, None)
                }
                Auto::Watching { account, .. } => (
                    self.watching.as_ref() != Some(account),
                    Some(account.clone()),
                ),
                value => (
                    value.told_apart().is_none_or(|key| self.said.insert(key)),
                    None,
                ),
            },
            Err(failed) => (
                self.said.insert(format!("error/{}", failed.error.code())),
                None,
            ),
        };
        let mut report = say(outcome);
        report
            .said_first
            .retain(|warning| self.warned.insert(warning.to_string()));
        report
            .warnings
            .retain(|warning| self.warned.insert(warning.to_string()));
        if !untold && report.said_first.is_empty() && report.warnings.is_empty() {
            return None;
        }
        self.watching = watching;
        Some(report)
    }
}

/// The `watching` event's data: the share it switches at, or none, whether it switches, and
/// the accounts it warms up with the span of the day, where any is turned on.
fn watching_json(switching: Switching, warming: Option<&Overview>) -> Value {
    let mut data = json!({
        "event": "watching",
        "threshold": switching.at().map(Threshold::percent),
        "weekly_threshold": switching.policy().and_then(|policy| policy.weekly).map(Threshold::percent),
        "order": switching.policy().map(|policy| policy.order.code()),
        "switches": switching != Switching::Never,
    });
    if let Some(overview) = warming.filter(|overview| warms_any(overview)) {
        data["warm"] = json!({
            "range": crate::warm::range_json(overview.range),
            "accounts": overview
                .accounts
                .iter()
                .filter(|choice| choice.on)
                .map(|choice| choice.account.clone())
                .collect::<Vec<_>>(),
        });
    }
    data
}

/// What it warms up, as the start line's last clause says it: ", and warms up work and
/// personal from 06:00 to 22:00", or nothing where no account is turned on.
fn warms(overview: &Overview) -> String {
    if !warms_any(overview) {
        return String::new();
    }
    let on: Vec<String> = overview
        .accounts
        .iter()
        .filter(|choice| choice.on)
        .map(|choice| ui::paint(ui::BOLD, &choice.account))
        .collect();
    format!(
        ", and warms up {} from {}",
        words::listed(on),
        overview.range
    )
}

fn warms_any(overview: &Overview) -> bool {
    overview.accounts.iter().any(|choice| choice.on)
}

/// What `watch` has said of warming up: each reason it held an account back or could not warm
/// it up, by account, said once until that account warms up, and why nothing is warmed up here,
/// said once until it stops being so.
#[derive(Default)]
struct WarmTold {
    by_account: HashMap<String, HashSet<String>>,
    halted: Option<String>,
    errors: pitboard_core::switch::warm::Told,
}

impl WarmTold {
    /// What of `outcome` has not been said, as the reports `watch` prints of it.
    fn untold(&mut self, outcome: Changing<WarmUp>) -> Vec<Report> {
        let moment = |at| pitboard_core::time::moment(at, epoch());
        let done = match outcome {
            Ok(done) => done,
            Err(failed) => {
                return if self.errors.untold(&told_apart(&failed.error)) {
                    vec![Report::refused("watch", failed)]
                } else {
                    Vec::new()
                };
            }
        };
        match done.value {
            WarmUp::Started { account, until } => {
                self.by_account.remove(&account);
                self.errors.warmed(&account);
                let line = crate::warm::warmed_line(&account, until);
                vec![Report::done(
                    "watch",
                    json!({ "event": "warmed_up", "account": account, "until": until }),
                    stamp(&format!("{line}\n")),
                )]
            }
            WarmUp::Running { .. } | WarmUp::Nothing => Vec::new(),
            WarmUp::Stands(overview) => self.stands(&overview, &moment),
        }
    }

    /// Why nothing is warmed up here, and why each account turned on is held back, where
    /// not said yet.
    fn stands(&mut self, overview: &Overview, moment: &dyn Fn(i64) -> String) -> Vec<Report> {
        let mut reports = Vec::new();
        let turned_on = overview.accounts.iter().any(|choice| choice.on);
        match overview.halted.as_ref().filter(|_| turned_on) {
            Some(halt) => {
                if self.halted.as_deref() != Some(halt.code()) {
                    self.halted = Some(halt.code().to_owned());
                    reports.push(Report::done(
                        "watch",
                        json!({ "event": "warm_up_halted", "reason": halt.code() }),
                        stamp(&format!("{}\n", words::warming_halted(halt))),
                    ));
                }
                return reports;
            }
            None => self.halted = None,
        }
        for choice in &overview.accounts {
            let Some(Standing::Held(hold)) = &choice.standing else {
                continue;
            };
            let until = hold.until();
            let key = format!("held/{}", hold.told_apart());
            if !self
                .by_account
                .entry(choice.account.clone())
                .or_default()
                .insert(key)
            {
                continue;
            }
            let standing = Standing::Held(hold.clone());
            reports.push(Report::done(
                "watch",
                json!({
                    "event": "warm_up_held",
                    "account": choice.account,
                    "reason": hold.code(),
                    "until": until,
                }),
                stamp(&format!(
                    "{} {}, so Pitboard is not warming it up for now.\n",
                    ui::paint(ui::BOLD, &choice.account),
                    words::warm_standing(&standing, moment)
                )),
            ));
        }
        reports
    }
}

/// What deciding came to, as a report with what it found on the way: `stamped` puts the time
/// before its line, for a terminal left open for hours.
fn said(outcome: Changing<Auto>, policy: impl Into<Policy>, stamped: bool) -> Report {
    let policy = policy.into();
    crate::changed("watch", outcome, |value| {
        let (data, line) = event(value, policy);
        (data, if stamped { stamp(&line) } else { line })
    })
}

/// `line` after the local time.
fn stamp(line: &str) -> String {
    format!(
        "{} {line}",
        ui::paint(ui::DIM, pitboard_core::time::local(epoch(), "%H:%M:%S"))
    )
}

/// What `value` says, as `--json` gives it and as a person reads it.
fn event(value: Auto, policy: impl Into<Policy>) -> (Value, String) {
    let policy = policy.into();
    let moment = |at| pitboard_core::time::moment(at, epoch());
    let percent = policy.threshold.percent();
    let triggered = match &value {
        Auto::Waiting { limit, .. } | Auto::Skipped { limit, .. } => Some(limit),
        Auto::Switched { why, .. } => why.limit(),
        Auto::Watching { .. } | Auto::NotWatching { .. } => None,
    };
    let at = triggered.map_or(Some(percent), |limit| policy.shares().at(limit));
    let (mut data, line) = match value {
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
                    "threshold": at,
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
                "threshold": at,
                "from": from,
                "limit": limit_json(&limit),
                "until": until,
            }),
            format!(
                "{}\n",
                words::auto_waiting(
                    &ui::paint(ui::BOLD, &from),
                    &words::share_of_limit(&limit),
                    &moment(until)
                )
            ),
        ),
        Auto::Switched {
            from,
            to,
            why,
            adoption,
        } => {
            let (_, follows, adoption_json) = followed(ProviderId::Claude, &adoption);
            let (from_bold, to_bold) = (ui::paint(ui::BOLD, &from), ui::paint(ui::BOLD, &to));
            let mut data = json!({
                "event": "switched",
                "threshold": at,
                "from": from,
                "to": to,
                "limit": why.limit().map(limit_json),
                "adoption": adoption_json,
            });
            let because = match why {
                Why::Limit(limit) => {
                    data["reason"] = json!("limit");
                    words::has_used(&from_bold, &words::share_of_limit(&limit))
                }
                Why::ResetsFirst {
                    to: ahead,
                    from: behind,
                } => {
                    data["reason"] = json!("resets_first");
                    data["to_resets_at"] = json!(ahead.resets_at());
                    data["from_resets_at"] = json!(behind.resets_at());
                    match (ahead.resets_at(), behind.resets_at()) {
                        (Some(to_at), Some(from_at)) => words::resets_at_before(
                            &to_bold,
                            &moment(to_at),
                            &from_bold,
                            &moment(from_at),
                        ),
                        (Some(_), None) => words::resets_before(&to_bold, &from_bold),
                        (None, _) => words::no_weekly_limit(&to_bold),
                    }
                }
            };
            (
                data,
                format!(
                    "{} {follows}",
                    words::auto_switched(&from_bold, &to_bold, &because)
                ),
            )
        }
        Auto::Skipped { from, limit, why } => {
            let until = match &why {
                Skip::Settling { until } => Some(*until),
                _ => None,
            };
            let line = words::auto_skipped(
                &ui::paint(ui::BOLD, &from),
                &words::share_of_limit(&limit),
                &words::not_switching(&why, policy),
                until.map(moment).as_deref(),
            );
            let data = match &why {
                // No room has an event of its own, as it always had.
                Skip::NoRoom { unread } => json!({
                    "event": "no_room",
                    "threshold": at,
                    "from": from,
                    "limit": limit_json(&limit),
                    "unread": unread,
                }),
                Skip::Settling { until } => json!({
                    "event": "skipped",
                    "threshold": at,
                    "from": from,
                    "limit": limit_json(&limit),
                    "reason": why.code(),
                    "until": until,
                }),
                _ => json!({
                    "event": "skipped",
                    "threshold": at,
                    "from": from,
                    "limit": limit_json(&limit),
                    "reason": why.code(),
                }),
            };
            (data, format!("{line}\n"))
        }
        Auto::NotWatching { why } => {
            let mut data = json!({
                "event": "not_watching",
                "threshold": at,
                "reason": why.code(),
            });
            let mut line = words::not_watching(&why);
            let mut until = None;
            match &why {
                // The command line has a command for giving up on it, which the app has a
                // button for in its place.
                Blind::SwitchInterrupted => {
                    line.push_str(", or give up on it with `pitboard abandon`")
                }
                Blind::NotEnrolled { email } => data["email"] = json!(email),
                Blind::NoReading { account } => data["account"] = json!(account),
                Blind::Unidentified { detail, until: at } => {
                    data["detail"] = json!(detail);
                    data["until"] = json!(at);
                    until = Some(moment(*at));
                }
                _ => {}
            }
            (
                data,
                format!("{}\n", words::auto_not_watching(&line, until.as_deref())),
            )
        }
    };
    data["weekly_threshold"] = json!(policy.weekly.map(Threshold::percent));
    (data, line)
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

/// `epoch` as the instant a front end's [`Refusals`] are measured on.
fn instant(epoch: i64) -> Duration {
    Duration::from_secs(epoch.unsigned_abs())
}

/// How often it decides though nothing was written, as the core says every front end does.
fn every() -> Duration {
    Duration::from_secs(DECIDE_EVERY_SECONDS.unsigned_abs())
}

/// How long the loop waits at `now`, having last read at `read_at`, with its next decision
/// `due`: until the earlier of the next read and that decision, and no longer than a
/// decision's every, so a clock put back holds neither up.
fn waits(now: Duration, read_at: Option<i64>, due: Duration) -> Duration {
    let read = read_at.map_or(now, |at| {
        instant(at) + Duration::from_secs(READ_EVERY_SECONDS.unsigned_abs())
    });
    read.min(due).saturating_sub(now).min(every())
}

/// Seconds since 1970, for when to read and decide next, and for writing a moment as a person
/// reads it. Not what is decided from: the core reads its own clock.
pub(crate) fn epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_core::provider::{Adoption, Held};
    use pitboard_core::service::Warning;

    /// `watch install` says what the service does, where it went and the terms' sentence;
    /// `status` reads it back with its share, and from another home says whose accounts it
    /// watches; `uninstall` takes it away once. On a machine in memory, whose scheduler asks
    /// nobody.
    #[test]
    fn the_service_is_installed_reported_and_taken_away() {
        use pitboard_core::context::Context;
        use pitboard_core::testing::MemoryHost;
        let scratch = tempfile::tempdir().expect("a scratch home");
        let home = scratch.path().to_path_buf();
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its folder")).expect("made");
        let launcher = pitboard_core::testing::launcher(&program);
        for file in launcher.iter().chain([&program]) {
            std::fs::write(file, "").expect("a command line");
        }
        let ctx = Context::new(home.clone())
            .with_schedule_program(program)
            .with_memory_stores(MemoryHost::new());
        let default = Pitboard::new(ctx.clone());
        let elsewhere = Pitboard::new(ctx.with_pitboard_home(home.join("elsewhere")));
        let at = |percent: &str| {
            let parsed = <Share as clap::FromArgMatches>::from_arg_matches(
                &<Share as clap::Args>::augment_args(clap::Command::new("watch"))
                    .get_matches_from(["watch", "--at", percent]),
            );
            parsed.expect("a share")
        };
        let run = |pitboard: &Pitboard, what: WatchCommand| service(pitboard, &what);

        let none = run(&default, WatchCommand::Status);
        assert_eq!(
            none.human,
            "Nothing watches Claude Code in the background here.\nRun `pitboard watch install` \
             to change that.\n"
        );
        assert_eq!(
            none.result.as_ref().expect("read"),
            &json!({"installed": false})
        );

        let installed = run(&default, WatchCommand::Install { share: at("90") });
        let data = installed.result.as_ref().expect("installed").clone();
        assert_eq!(data["installed"], true);
        assert_eq!(data["threshold"], 90);
        assert_eq!(data["switches"], true);
        assert!(
            data["path"].is_string() || data["task"].is_string(),
            "{data}"
        );
        assert!(
            installed.human.starts_with(
                "Pitboard watches Claude Code in the background from now on, and at each \
                 sign-in. It switches once a limit reaches 90%, and warms \
                 up the accounts `pitboard warm on` turned on.\n"
            ),
            "{}",
            installed.human
        );
        assert!(
            installed.human.ends_with(&format!("\n\n{}\n", terms())),
            "{}",
            installed.human
        );
        assert_eq!(
            installed.human.contains(
                "Your user's systemd runs it while you are signed in. To keep it running when \
                 you are not, run `loginctl enable-linger`.\n"
            ),
            pitboard_core::host::OS == Os::Linux,
            "{}",
            installed.human
        );

        let here = run(&default, WatchCommand::Status);
        assert!(
            here.human.starts_with(
                "Pitboard watches Claude Code in the background, and switches once a limit reaches 90%.\n"
            ),
            "{}",
            here.human
        );
        let data = here.result.expect("read");
        assert_eq!(
            (&data["threshold"], &data["switches"]),
            (&json!(90), &json!(true))
        );
        let there = run(&elsewhere, WatchCommand::Status).human;
        assert!(
            there.ends_with(&format!(
                "The watch service watches only the accounts in {}, not those in the directory \
                 PITBOARD_HOME names.\n",
                default.shown(&default.default_directory())
            )),
            "{there}"
        );
        let refused = run(&elsewhere, WatchCommand::Install { share: at("95") });
        assert_eq!(
            refused.result.expect_err("refused").code(),
            "schedule_not_default_home"
        );

        let removed = run(&default, WatchCommand::Uninstall);
        assert_eq!(
            removed.human,
            "Stopped watching Claude Code in the background.\n"
        );
        assert_eq!(
            removed.result.expect("removed"),
            json!({"installed": false, "removed": true})
        );
        assert_eq!(
            run(&default, WatchCommand::Uninstall).human,
            "Nothing was watching Claude Code in the background.\n"
        );
    }

    /// `watch install --no-switch` says the service never switches, and `status` reads that
    /// back, its share null; installed again with a share, it switches at that one. On a
    /// machine in memory, whose scheduler asks nobody.
    #[test]
    fn a_service_that_never_switches_says_so_and_reads_back_so() {
        use pitboard_core::context::Context;
        use pitboard_core::testing::MemoryHost;
        let scratch = tempfile::tempdir().expect("a scratch home");
        let home = scratch.path().to_path_buf();
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its folder")).expect("made");
        let launcher = pitboard_core::testing::launcher(&program);
        for file in launcher.iter().chain([&program]) {
            std::fs::write(file, "").expect("a command line");
        }
        let pitboard = Pitboard::new(
            Context::new(home)
                .with_schedule_program(program)
                .with_memory_stores(MemoryHost::new()),
        );
        let share = |given: &[&str]| {
            let parsed = <Share as clap::FromArgMatches>::from_arg_matches(
                &<Share as clap::Args>::augment_args(clap::Command::new("watch"))
                    .get_matches_from(std::iter::once("watch").chain(given.iter().copied())),
            );
            parsed.expect("a share")
        };
        let install = |given: &[&str]| {
            service(
                &pitboard,
                &WatchCommand::Install {
                    share: share(given),
                },
            )
        };
        let read = || {
            let report = service(&pitboard, &WatchCommand::Status);
            let data = report.result.expect("read");
            (
                report.human,
                data["threshold"].clone(),
                data["switches"].clone(),
            )
        };

        let installed = install(&["--no-switch"]);
        let data = installed.result.as_ref().expect("installed");
        assert_eq!(
            (&data["installed"], &data["threshold"], &data["switches"]),
            (&json!(true), &Value::Null, &json!(false))
        );
        assert!(
            installed.human.starts_with(
                "Pitboard watches Claude Code in the background from now on, and at each \
                 sign-in. It never switches, and only warms up the accounts `pitboard warm on` \
                 turned on.\n"
            ),
            "{}",
            installed.human
        );
        let (human, threshold, switches) = read();
        assert!(
            human.starts_with(
                "Pitboard watches Claude Code in the background, and never switches: it only \
                 warms up the accounts `pitboard warm on` turned on.\n"
            ),
            "{human}"
        );
        assert_eq!((threshold, switches), (Value::Null, json!(false)));

        install(&["--at", "80"]).result.expect("installed again");
        let (human, threshold, switches) = read();
        assert!(human.contains("reaches 80%"), "{human}");
        assert_eq!((threshold, switches), (json!(80), json!(true)));
        install(&["--no-switch"]).result.expect("and again");
        assert_eq!(read().2, json!(false));
    }

    #[test]
    fn watch_starts_by_saying_whether_it_switches() {
        assert_eq!(
            watching_json(Switching::At(Policy::default()), None),
            json!({"event": "watching", "threshold": 95, "weekly_threshold": 95, "order": "most-left", "switches": true})
        );
        let soonest = Policy {
            order: Order::ResetsSoonest,
            ..Policy::default()
        };
        assert_eq!(
            watching_json(Switching::At(soonest), None)["order"],
            json!("resets-soonest")
        );
        assert_eq!(
            watching_json(Switching::Never, None),
            json!({"event": "watching", "threshold": null, "weekly_threshold": null, "order": null, "switches": false})
        );
    }

    /// A switch made before the share names no limit, says why in `reason`, and gives when both
    /// accounts' weeks reset; one at the share says `limit`.
    #[test]
    fn an_early_switch_has_no_limit_a_reason_and_both_resets() {
        use pitboard_core::usage::Week;
        let switched = |why| Auto::Switched {
            from: "work".into(),
            to: "soon".into(),
            why,
            adoption: Adoption::PollingWithin(33),
        };
        let (data, line) = event(
            switched(Why::ResetsFirst {
                to: Week::Resets(1_800_086_400),
                from: Week::Resets(1_800_432_000),
            }),
            Threshold::DEFAULT,
        );
        assert_eq!(
            (
                &data["reason"],
                &data["limit"],
                &data["to_resets_at"],
                &data["from_resets_at"]
            ),
            (
                &json!("resets_first"),
                &Value::Null,
                &json!(1_800_086_400),
                &json!(1_800_432_000)
            )
        );
        let plain = |line: &str| anstream::adapter::strip_str(line).to_string();
        assert!(plain(&line).contains("soon's week resets at "), "{line}");
        let (seat, line) = event(
            switched(Why::ResetsFirst {
                to: Week::Free,
                from: Week::Resets(1_800_432_000),
            }),
            Threshold::DEFAULT,
        );
        assert_eq!(seat["to_resets_at"], Value::Null);
        assert!(plain(&line).contains("soon has no weekly limit"), "{line}");
        let (at_the_share, _) = event(switched(Why::Limit(limit(9_000))), Threshold::DEFAULT);
        assert_eq!(at_the_share["reason"], json!("limit"));
        assert_eq!(at_the_share["limit"]["resets_at"], json!(9_000));
    }

    /// The loop waits until the next read or the next decision, whichever comes first, a write
    /// to Pitboard's directory waking it sooner, and never longer than a decision's every, so
    /// a clock put back holds nothing up for longer. Never `LOOK_EVERY_SECONDS`: that is the
    /// wait's own, and only where nothing watches the directory.
    #[test]
    fn the_loop_waits_until_it_next_reads_or_decides() {
        const NOW: i64 = 1_760_000_000;
        let secs = Duration::from_secs;
        let now = instant(NOW);
        assert_eq!(
            waits(now, Some(NOW - 100), now + every()),
            every(),
            "just decided"
        );
        assert_eq!(
            waits(now, Some(NOW - READ_EVERY_SECONDS + 10), now + every()),
            secs(10),
            "a read comes first"
        );
        let mut refusals = Refusals::default();
        refusals.refused(now - secs(55));
        assert_eq!(
            waits(now, Some(NOW), refusals.due(now - secs(10), every())),
            secs(5),
            "as a refusal's wait ends"
        );
        assert_eq!(
            waits(now, Some(NOW - READ_EVERY_SECONDS - 1), now - secs(5)),
            Duration::ZERO,
            "both are due"
        );
        assert_eq!(
            waits(now, None, now + every()),
            Duration::ZERO,
            "nothing read yet"
        );
        assert_eq!(
            waits(now, Some(NOW + 3_600), now + secs(3_630)),
            every(),
            "a clock put back an hour"
        );
    }

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
        assert!(ends_watching(&Error::SystemTooOld { build: Some(17763) }));
        assert!(ends_watching(&Error::SystemTooOld { build: None }));
        assert!(!ends_watching(&Error::SignedInAccountChanged));
        assert!(!ends_watching(&Error::ParkedLoginExpired {
            label: "spare".into()
        }));
    }

    impl Told {
        /// What `watch` prints of `outcome`, where it says anything not said since the last
        /// switch.
        fn heard(&mut self, outcome: Changing<Auto>) -> Option<Report> {
            self.untold(outcome, |outcome| said(outcome, Threshold::DEFAULT, false))
        }

        /// Whether `value` says anything not said since the last switch.
        fn says(&mut self, value: Auto) -> bool {
            self.heard(found(value, Vec::new())).is_some()
        }
    }

    /// What `watch` prints of `value`.
    fn told(value: Auto) -> Report {
        said(found(value, Vec::new()), Threshold::DEFAULT, false)
    }

    /// A decision that found `warnings` came to `value`.
    fn found(value: Auto, warnings: Vec<Warning>) -> Changing<Auto> {
        Ok(Done { value, warnings })
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

    fn blind(why: Blind) -> Auto {
        Auto::NotWatching { why }
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

    fn skipped(resets_at: i64, why: Skip) -> Auto {
        Auto::Skipped {
            from: "work".into(),
            limit: limit(resets_at),
            why,
        }
    }

    /// Each reason not to switch away from a limit has its code, and no room its own event,
    /// with the accounts Anthropic gave no reading of. Each is said once for the limit and its
    /// reset.
    #[test]
    fn a_reason_not_to_switch_is_said_once_for_a_limit_and_its_reset() {
        let no_room = told(skipped(
            9_000,
            Skip::NoRoom {
                unread: vec!["spare".into()],
            },
        ));
        let data = no_room.result.as_ref().expect("an event");
        assert_eq!(
            (&data["event"], &data["unread"], &data["reason"]),
            (&json!("no_room"), &json!(["spare"]), &Value::Null)
        );
        assert_eq!(
            plain(&no_room),
            "work has used 97% of its 5-hour limit. Pitboard is not switching: no other Claude \
             Code account has room below 95% in every limit, and there is no reading of spare \
             from Anthropic yet.\n"
        );
        for (why, code) in [
            (Skip::AlreadyLeft, "already_switched"),
            (Skip::GaveUp, "attempts_spent"),
            (Skip::Settling { until: 9_000 }, "settling"),
        ] {
            let report = told(skipped(9_000, why));
            let data = report.result.as_ref().expect("an event");
            assert_eq!(
                (&data["event"], &data["reason"]),
                (&json!("skipped"), &json!(code))
            );
        }
        let settling = told(skipped(
            9_000,
            Skip::Settling {
                until: 1_760_000_300,
            },
        ));
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
        let waiting_at = |limit: Window, until| Auto::Waiting {
            from: "work".into(),
            limit,
            until,
        };
        let waiting = |until| waiting_at(limit(9_000), until);
        let report = told(waiting(1_760_000_060));
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
            store: pitboard_core::host::SecretStore::Keychain,
            held: Held::Login,
        };
        let overridden = || Warning::AuthOverridden {
            tool: ProviderId::Claude,
            names: vec!["ANTHROPIC_API_KEY".into()],
        };
        let mut said = Told::default();
        assert!(said.says(watching("work")));
        let again = said
            .heard(found(watching("work"), vec![fallback()]))
            .expect("a warning not said yet");
        assert_eq!(plain(&again), "Watching work.\n");
        assert_eq!(warned(&again), ["fallback_login"]);
        assert!(
            said.heard(found(watching("work"), vec![fallback()]))
                .is_none(),
            "said once"
        );
        let more = said
            .heard(found(watching("work"), vec![fallback(), overridden()]))
            .expect("another warning");
        assert_eq!(warned(&more), ["auth_overridden"], "and only that one");

        assert!(said.says(skipped(9_000, Skip::GaveUp)));
        let refused = Err(Failed {
            error: Error::SignedInAccountChanged,
            warnings: vec![fallback()],
            said_first: 0,
        });
        assert!(said.heard(refused).is_some(), "an error is said once");
        let refused_again = Err(Failed {
            error: Error::SignedInAccountChanged,
            warnings: vec![
                overridden(),
                Warning::LockCompromised {
                    tool: ProviderId::Claude,
                },
            ],
            said_first: 0,
        });
        let refused_again = said
            .heard(refused_again)
            .expect("with a warning not said yet");
        assert_eq!(warned(&refused_again), ["lock_compromised"]);
        assert!(
            said.says(watching("work")),
            "the last line said something else"
        );

        let switched = found(
            Auto::Switched {
                from: "work".into(),
                to: "personal".into(),
                why: Why::Limit(limit(9_000)),
                adoption: Adoption::PollingWithin(33),
            },
            vec![fallback()],
        );
        let switched = said.heard(switched).expect("a switch");
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

    /// A reason Pitboard cannot judge whether to switch at all is an event of its own, with
    /// no account or limit, and says what to do where the command line has a command for it.
    #[test]
    fn a_reason_it_cannot_watch_is_an_event_of_its_own() {
        let interrupted = told(blind(Blind::SwitchInterrupted));
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

        let unread = told(blind(Blind::NoReading {
            account: "work".into(),
        }));
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

        let stranger = told(blind(Blind::NotEnrolled {
            email: "me@example.com".into(),
        }));
        let data = stranger.result.as_ref().expect("an event");
        assert_eq!(
            (&data["reason"], &data["email"]),
            (&json!("not_enrolled"), &json!("me@example.com"))
        );

        let unidentified = told(blind(Blind::Unidentified {
            detail: "could not reach Anthropic: no route to host".into(),
            until: 1_760_000_060,
        }));
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

    mod warming {
        use super::*;
        use pitboard_core::duty::warm::{Choice, Halt, Hold, Range};

        fn overview(halted: Option<Halt>, held: Option<Hold>) -> Overview {
            Overview {
                range: Range::DEFAULT,
                halted,
                accounts: vec![Choice {
                    account: "work".into(),
                    qualified: "claude/work".into(),
                    on: true,
                    standing: held.map(Standing::Held),
                    now: Standing::Due,
                }],
            }
        }

        fn stands(overview: Overview) -> Changing<WarmUp> {
            Ok(Done {
                value: WarmUp::Stands(Box::new(overview)),
                warnings: Vec::new(),
            })
        }

        fn events(reports: &[Report]) -> Vec<String> {
            reports
                .iter()
                .map(|report| report.result.as_ref().expect("an event")["event"].to_string())
                .collect()
        }

        /// Why an account is held back is said once until it warms up; why nothing is warmed
        /// up here once until it stops being so.
        #[test]
        fn each_reason_is_said_once_until_it_ends() {
            let mut told = WarmTold::default();
            let held = || stands(overview(None, Some(Hold::NoReading)));
            assert_eq!(events(&told.untold(held())), ["\"warm_up_held\""]);
            assert!(told.untold(held()).is_empty());

            let halted = || stands(overview(Some(Halt::ProgramMissing), None));
            assert_eq!(events(&told.untold(halted())), ["\"warm_up_halted\""]);
            assert!(told.untold(halted()).is_empty());
            told.untold(stands(overview(None, None)));
            assert_eq!(
                events(&told.untold(halted())),
                ["\"warm_up_halted\""],
                "said again once it came back"
            );

            let warmed = Ok(Done {
                value: WarmUp::Started {
                    account: "work".into(),
                    until: Some(1_760_000_000),
                },
                warnings: Vec::new(),
            });
            assert_eq!(events(&told.untold(warmed)), ["\"warmed_up\""]);
            assert_eq!(
                events(&told.untold(held())),
                ["\"warm_up_held\""],
                "said again after it warmed up"
            );
        }

        // A `watch` that warms up nothing has nothing to say of it.
        #[test]
        fn why_nothing_is_warmed_up_is_not_said_with_no_account_turned_on() {
            let mut told = WarmTold::default();
            let mut off = overview(Some(Halt::ProgramMissing), None);
            off.accounts[0].on = false;
            assert!(told.untold(stands(off)).is_empty());
            assert_eq!(
                events(&told.untold(stands(overview(Some(Halt::ProgramMissing), None)))),
                ["\"warm_up_halted\""]
            );
        }

        #[test]
        fn watch_starts_by_saying_what_it_warms_up_where_an_account_is_turned_on() {
            let mut off = overview(None, None);
            off.accounts[0].on = false;
            assert_eq!(warms(&off), "");
            assert_eq!(
                watching_json(Switching::At(Policy::default()), Some(&off)).get("warm"),
                None
            );
            let on = overview(Some(Halt::ProgramMissing), None);
            let said = anstream::adapter::strip_str(&warms(&on)).to_string();
            assert_eq!(said, ", and warms up work from 06:00 to 22:00");
            let never = |warming| {
                anstream::adapter::strip_str(&started(Switching::Never, warming, false)).to_string()
            };
            assert_eq!(
                never(Some(&on)),
                "Watching Claude Code. Pitboard never switches accounts, and warms up work from \
                 06:00 to 22:00. Stop with Ctrl-C.\n"
            );
            assert_eq!(
                never(Some(&off)),
                "Watching Claude Code. Pitboard never switches accounts, and no account is turned \
                 on for warm-ups: turn one on with `pitboard warm on <account>`. Stop with \
                 Ctrl-C.\n"
            );
            assert_eq!(
                never(None),
                "Watching Claude Code. Pitboard never switches accounts. Stop with Ctrl-C.\n"
            );
            assert_eq!(
                watching_json(Switching::At(Policy::default()), Some(&on))["warm"]["accounts"],
                json!(["work"])
            );
        }

        #[test]
        fn a_failure_of_one_account_is_said_once_until_it_warms_up() {
            let mut told = WarmTold::default();
            let failed = || {
                Err(Failed::refused(Error::WarmUpFailed {
                    label: "work".into(),
                    why: pitboard_core::duty::warm::Failure::NotStarted,
                }))
            };
            assert_eq!(told.untold(failed()).len(), 1);
            assert!(told.untold(failed()).is_empty());
        }

        /// A limit used up is said once, though each answer may give its reset a second apart.
        #[test]
        fn a_limit_used_up_is_said_once_whatever_second_its_reset_is_given() {
            let mut told = WarmTold::default();
            let used_up = |resets_at| {
                let limit = Window {
                    kind: "weekly_all".into(),
                    scope: None,
                    percent: 100.0,
                    resets_at: Some(resets_at),
                    is_active: true,
                    severity: None,
                    length_seconds: None,
                };
                stands(overview(None, Some(Hold::LimitUsedUp { limit })))
            };
            assert_eq!(told.untold(used_up(1_760_000_000)).len(), 1);
            assert!(told.untold(used_up(1_760_000_001)).is_empty());
        }

        /// Another error is said once for its words, which name the account it is of.
        #[test]
        fn another_error_is_said_once_for_each_account_it_names() {
            let mut told = WarmTold::default();
            let expired = |label: &str| {
                Err(Failed::refused(Error::ParkedLoginRefused {
                    tool: ProviderId::Claude,
                    label: label.into(),
                }))
            };
            assert_eq!(told.untold(expired("work")).len(), 1);
            assert!(told.untold(expired("work")).is_empty());
            assert_eq!(told.untold(expired("personal")).len(), 1);
        }
    }
}
