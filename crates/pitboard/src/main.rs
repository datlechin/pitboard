use anstream::{ColorChoice, eprintln, print, println};
use anstyle::{AnsiColor, Style};
use clap::{CommandFactory, Parser, Subcommand};
use pitboard_core::autoswitch::Threshold;
use pitboard_core::context::Context;
use pitboard_core::doctor;
use pitboard_core::error::Error;
use pitboard_core::provider::Adoption;
use pitboard_core::service::{Changing, Done, Failed, Pitboard, Warning};
use pitboard_core::switch::{Enrolled, Outcome, Renewal};
use serde_json::{Value, json};
use std::io::{IsTerminal, Read, Write};
use std::process::ExitCode;
use ui::{BOLD, DIM, WARN, paint};

mod json;
mod manpage;
mod render;
mod ui;
mod watch;

/// Bumped only when a field changes shape. Adding a field or an error code is not a
/// breaking change for a consumer; renaming or removing one is.
const CONTRACT: u32 = 1;

const ERROR: Style = AnsiColor::Red.on_default().bold();
const WARNING: Style = AnsiColor::Yellow.on_default().bold();

/// Park and restore your own logins of Claude Code and Codex, and see what each one has left.
#[derive(Parser)]
#[command(name = "pitboard", version)]
struct Cli {
    /// Machine-readable output: the same versioned JSON envelope for every command,
    /// whether it succeeds or fails
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// What is signed in, and how much each account has left (the default)
    Status {
        /// Answer from what was last measured, without asking anyone
        #[arg(long)]
        offline: bool,
        /// Ask about every account, even one asked about moments ago
        #[arg(long, conflicts_with = "offline")]
        fresh: bool,
    },
    /// Add an account: the one signed in now, or with --sign-in, another one
    Enroll {
        /// A short name for this account, such as `personal` or `work`. `codex/work` names a
        /// Codex account; a bare name means Claude Code
        #[arg(value_parser = label_to_enroll)]
        label: String,
        /// Sign in through the tool's own sign-in, without signing out of the account in
        /// use. For the account in use, this puts its new login in use; for another enrolled
        /// label, it renews its parked login.
        #[arg(long)]
        sign_in: bool,
    },
    /// Switch a tool to an enrolled account
    Use {
        /// The label the account was enrolled under, such as `work` or `codex/work`
        label: String,
    },
    /// Drop an account and its parked login
    Forget {
        /// The label to drop
        label: String,
        /// Do not ask first
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Put away the login Claude Code left in a file behind the keychain, then delete the file
    Stow {
        /// Do not ask first
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Give up on an interrupted switch that cannot be finished, keeping every login
    Abandon,
    /// Ask the credential store what parked logins are here, and account for every one
    Repair,
    /// Take over a Pitboard directory another computer wrote, keeping the accounts
    Adopt,
    /// Renew every parked login that is due, and nothing else
    Renew {
        // The marker the daily renewal schedule's job runs this with on Linux, where what
        // systemd passes a job is not to be relied on. Hidden from help and the man page,
        // since a person has no reason to type it, but clap_complete writes hidden
        // arguments into every shell's completions, so the line below is what they show,
        // as written: it has no Markdown for that reason.
        /// Renew as the daily renewal schedule does: ~/.pitboard, whatever PITBOARD_HOME says
        #[arg(long, hide = true)]
        scheduled: bool,
    },
    /// Keep parked logins alive without running anything yourself
    Schedule {
        #[command(subcommand)]
        what: ScheduleCommand,
    },
    /// What Pitboard has changed, and when
    Log {
        /// How many changes to show
        #[arg(short = 'n', long, default_value_t = 20)]
        lines: usize,
    },
    /// Delete every parked login this Pitboard wrote, the daily renewal schedule and
    /// Pitboard's own files
    Uninstall {
        /// Do not ask first
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Change the label an account is enrolled under
    Rename {
        /// The label it has now
        from: String,
        /// The label it should have
        #[arg(value_parser = new_label)]
        to: String,
    },
    /// Check that what Pitboard relies on still holds on this machine
    Doctor,
    /// One line for Claude Code's status bar; reads its session JSON on stdin
    Statusline,
    /// Switch Claude Code to another of your accounts before the one in use runs out, for as
    /// long as this runs
    Watch {
        /// Switch once a limit of the account in use reaches this share, from 50 to 99
        #[arg(
            long,
            value_name = "PERCENT",
            default_value_t = Threshold::DEFAULT.percent(),
            value_parser = clap::value_parser!(u8)
                .range(i64::from(Threshold::LOWEST)..=i64::from(Threshold::HIGHEST)),
        )]
        at: u8,
        /// Decide once from the usage Pitboard last measured, asking no service for usage,
        /// and stop
        #[arg(long)]
        once: bool,
    },
    /// Print a shell completion script
    Completions { shell: clap_complete::Shell },
    /// Print the man page
    #[command(hide = true)]
    Manpage,
}

impl Command {
    /// The name the command's envelope gives, as each command's own report names it.
    fn name(&self) -> &'static str {
        match self {
            Command::Status { .. } => "status",
            Command::Enroll { .. } => "enroll",
            Command::Use { .. } => "use",
            Command::Forget { .. } => "forget",
            Command::Stow { .. } => "stow",
            Command::Abandon => "abandon",
            Command::Repair => "repair",
            Command::Adopt => "adopt",
            Command::Renew { .. } => "renew",
            Command::Schedule { .. } => "schedule",
            Command::Log { .. } => "log",
            Command::Uninstall { .. } => "uninstall",
            Command::Rename { .. } => "rename",
            Command::Doctor => "doctor",
            Command::Statusline => "statusline",
            Command::Watch { .. } => "watch",
            Command::Completions { .. } | Command::Manpage => "generate",
        }
    }

    /// Whether the command asks first whether this build may do anything here: a Windows
    /// build of a release before Pitboard for Windows is released may not
    /// ([`pitboard_core::release`]). Every command does but the two that print a generated
    /// file; `--version` and `--help` are answered before a command is read.
    fn asks_whether_this_build_may_run(&self) -> bool {
        !matches!(self, Command::Completions { .. } | Command::Manpage)
    }

    /// Whether the command is refused here where a home the environment names is not a
    /// full path. Every one reads or writes under the homes but the two that print a
    /// generated file; `doctor` says so as a check of its own and checks nothing else; and
    /// `renew` is refused by the gate it asks, of the homes it renews in, since a run of the
    /// schedule renews `~/.pitboard` whatever `PITBOARD_HOME` says, empty or relative too.
    fn refuses_a_home_that_is_not_a_full_path(&self) -> bool {
        !matches!(
            self,
            Command::Completions { .. }
                | Command::Manpage
                | Command::Doctor
                | Command::Renew { .. }
        )
    }
}

/// A label is typed on the command line from then on, so it cannot be empty or hold spaces.
/// A name somebody is choosing for a new account, optionally saying which tool it is for.
///
/// `pitboard enroll codex/personal --sign-in` names both. A bare `personal` means the
/// default tool, so every command written before there was more than one still means what
/// it meant.
/// What `enroll` takes: a new name, or an account's existing one, which a label written by
/// 0.1.x may hold a slash in. Which of the two it is, the core decides against the state
/// file, which this parser cannot see; what can be refused here is only what no label ever
/// was.
fn label_to_enroll(text: &str) -> Result<String, String> {
    if text.is_empty() || text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("a label must be one word, such as `personal` or `work`".into());
    }
    Ok(text.to_string())
}

fn new_label(text: &str) -> Result<String, String> {
    if text.is_empty() || text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("a label must be one word, such as `personal` or `work`".into());
    }
    pitboard_core::label::choose(text)?;
    Ok(text.to_string())
}

/// What a command produced, before it is rendered for a person or a program.
struct Report {
    command: Option<&'static str>,
    result: Result<Value, Error>,
    warnings: Vec<Value>,
    human: String,
    exit: u8,
    /// A command that produced a report and still failed. The envelope then carries both
    /// what was found and why the exit code is not zero.
    failure: Option<(&'static str, String)>,
}

impl Report {
    fn done(command: &'static str, data: Value, human: String) -> Report {
        Report {
            command: Some(command),
            result: Ok(data),
            warnings: Vec::new(),
            human,
            exit: 0,
            failure: None,
        }
    }

    fn failed(command: Option<&'static str>, error: Error) -> Report {
        Report {
            command,
            exit: error.exit_code(),
            result: Err(error),
            warnings: Vec::new(),
            human: String::new(),
            failure: None,
        }
    }

    /// A change that failed, with what it found on the way.
    fn refused(command: &'static str, failed: Failed) -> Report {
        Report {
            warnings: warnings(&failed.warnings),
            ..Report::failed(Some(command), failed.error)
        }
    }
}

fn emit(report: Report, as_json: bool) -> ExitCode {
    if as_json {
        let (data, error) = match (&report.result, &report.failure) {
            (Ok(data), None) => (data.clone(), Value::Null),
            (Ok(data), Some((code, message))) => {
                (data.clone(), json!({ "code": code, "message": message }))
            }
            (Err(e), _) => (
                Value::Null,
                json!({
                    "code": e.code(),
                    "message": e.to_string(),
                    // What went wrong underneath, where Anthropic was asked. The code says
                    // what Pitboard was doing; this says whether asking again is worth
                    // anything.
                    "cause": e.cause().map(|c| json!({
                        "code": c.code(),
                        "worth_retrying": c.worth_retrying(),
                    })),
                }),
            ),
        };
        let envelope = json!({
            "v": CONTRACT,
            "command": report.command,
            "ok": report.result.is_ok() && report.exit == 0,
            "data": data,
            "warnings": report.warnings,
            "error": error,
        });
        println!("{}", json::ascii(&envelope));
    } else {
        match &report.result {
            Ok(_) => print!("{}", report.human),
            Err(e) => eprintln!("{} {e}", paint(ERROR, "error:")),
        }
        for w in &report.warnings {
            eprintln!(
                "{} {}",
                paint(WARNING, "warning:"),
                w["message"].as_str().unwrap_or_default()
            );
        }
    }
    ExitCode::from(report.exit)
}

fn warnings(list: &[Warning]) -> Vec<Value> {
    list.iter()
        .map(|w| json!({ "code": w.code(), "message": w.to_string() }))
        .collect()
}

/// A change as a report. Its warnings are kept whether or not it succeeded.
fn changed<T>(
    command: &'static str,
    outcome: Changing<T>,
    render: impl FnOnce(T) -> (Value, String),
) -> Report {
    match outcome {
        Ok(Done {
            value,
            warnings: found,
        }) => {
            let (data, human) = render(value);
            Report {
                warnings: warnings(&found),
                ..Report::done(command, data, human)
            }
        }
        Err(failed) => Report::refused(command, failed),
    }
}

fn status(pitboard: &Pitboard, offline: bool, fresh: bool) -> Report {
    let read = if offline {
        pitboard.status_offline()
    } else {
        pitboard.status(fresh)
    };
    match read {
        Ok(Done {
            value,
            warnings: found,
        }) => Report {
            warnings: warnings(&found),
            ..Report::done(
                "status",
                render::status::json(&value),
                render::status::human(&value),
            )
        },
        Err(e) => Report::failed(Some("status"), e),
    }
}

fn doctor(pitboard: &Pitboard) -> Report {
    let diagnosis = pitboard.doctor();
    let healthy = doctor::healthy(&diagnosis.checks);
    let failed = diagnosis
        .checks
        .iter()
        .filter(|c| c.level == doctor::Level::Fail)
        .count();
    Report {
        // A failed check means an assumption Pitboard relies on no longer holds.
        exit: if healthy { 0 } else { 3 },
        failure: (!healthy).then(|| {
            (
                "checks_failed",
                format!("{failed} check(s) failed; see data.checks"),
            )
        }),
        ..Report::done(
            "doctor",
            render::doctor::json(&diagnosis),
            render::doctor::human(&diagnosis.checks),
        )
    }
}

/// Claude Code reads the line through a pipe and draws its colours, so they are kept even
/// though stdout is not a terminal, unless `NO_COLOR` asks otherwise. The JSON form is plain.
#[allow(
    clippy::disallowed_methods,
    reason = "NO_COLOR is about this process's own output, and is on context::READ"
)]
fn statusline(pitboard: &Pitboard) -> Report {
    let mut input = String::new();
    // Claude Code pipes the session in. Typed at a prompt there is nothing to read, and
    // waiting for a terminal that will never send anything reads as a hung command.
    if !std::io::stdin().is_terminal() {
        let _ = std::io::stdin().read_to_string(&mut input);
    }
    let line = render::statusline::human(&pitboard.statusline(&input));
    if std::env::var_os("NO_COLOR").is_none() {
        ColorChoice::Always.write_global();
    }
    Report::done(
        "statusline",
        json!({ "line": anstream::adapter::strip_str(&line).to_string() }),
        format!("{line}\n"),
    )
}

fn enroll_signing_in(pitboard: &Pitboard, label: &str) -> Report {
    // Refused before the sign-in is announced, as root or under sudo, where none opens.
    if let Err(refused) = pitboard.permit() {
        return Report::failed(Some("enroll"), refused);
    }
    // The account this would sign in to again, of the tool the label is for: another
    // tool's account of the same name is somebody else to sign in as.
    let existing = pitboard.account_to_enroll(label);
    let who = existing
        .as_ref()
        .map_or_else(|| "the account to add".to_string(), |a| a.email.clone());
    let tool = existing.as_ref().map_or_else(
        || {
            pitboard_core::label::choose(label)
                .map(|chosen| chosen.provider)
                .unwrap_or(pitboard_core::label::DEFAULT)
        },
        pitboard_core::state::Account::provider,
    );
    eprintln!(
        "Opening {}'s sign-in. Sign in as {who}; the account in use now stays signed in.",
        tool.name()
    );
    match pitboard.sign_in(label) {
        Ok(login) => enrolled(pitboard, label, pitboard.enroll_signed_in(label, login)),
        Err(failed) => Report::refused("enroll", failed),
    }
}

fn enrolled(pitboard: &Pitboard, label: &str, outcome: Changing<Enrolled>) -> Report {
    let name = paint(BOLD, label);
    // Asked after the change, so the account it names is the one just enrolled, and the
    // name is one a command here takes: qualified where another tool shares the label.
    let provider = pitboard.account_to_enroll(label).map_or_else(
        || {
            pitboard_core::label::choose(label)
                .map(|chosen| chosen.provider)
                .unwrap_or(pitboard_core::label::DEFAULT)
        },
        |account| account.provider(),
    );
    let to_use = pitboard.name_to_type(label);
    // Another account of the same tool, typed the way this one was.
    let another = pitboard_core::state::Key::new(provider, "<label>").typed();
    changed("enroll", outcome, |enrolled| {
        let (kind, email, human) = match enrolled {
            Enrolled::Current { email } => {
                let human = format!(
                    "Enrolled {name} ({email}), the account signed in now.\n\
                     Add another without signing out of it: pitboard enroll {another} \
                     --sign-in\n"
                );
                ("current", email, human)
            }
            Enrolled::SignedIn { email } => {
                let human = format!(
                    "Enrolled {name} ({email}). Switch to it with: pitboard use {to_use}\n"
                );
                ("signed_in", email, human)
            }
            Enrolled::Renewed { email } => {
                let human = format!("Renewed {name} ({email}): its parked login is a fresh one.\n");
                ("renewed", email, human)
            }
            Enrolled::InUse { email, again } => {
                let human = if again {
                    format!(
                        "Signed in to {name} ({email}) again. Its new login is the one in use \
                         now.\n"
                    )
                } else {
                    format!(
                        "Enrolled {name} ({email}), the account signed in now. Its new login is \
                         the one in use.\n"
                    )
                };
                ("in_use", email, human)
            }
        };
        (
            json!({
                "label": label,
                "provider": provider,
                "email": email,
                "enrolled": kind,
            }),
            human,
        )
    })
}

fn use_account(pitboard: &Pitboard, label: &str) -> Report {
    changed("use", pitboard.switch_to(label), |outcome| match outcome {
        Outcome::AlreadyActive {
            label,
            config_updated,
        } => {
            let named = if config_updated {
                format!(
                    " Claude Code's config named another account, and now names {}.",
                    paint(BOLD, &label)
                )
            } else {
                String::new()
            };
            (
                json!({ "to": label, "changed": false, "config_updated": config_updated }),
                format!("{} is already signed in.{named}\n", paint(BOLD, &label)),
            )
        }
        Outcome::Switched {
            provider,
            from,
            to,
            parked,
            adoption,
        } => {
            let (seconds, follows, adoption_json) = followed(provider, &adoption);
            (
                json!({
                    "from": from,
                    "to": to,
                    "provider": provider,
                    "changed": true,
                    "parked_at": parked.parked_at,
                    "adoption_ceiling_seconds": seconds,
                    "adoption": adoption_json,
                }),
                format!(
                    "Switched to {}; {} is parked.\n{follows}",
                    paint(BOLD, &to),
                    paint(BOLD, &from),
                ),
            )
        }
    })
}

/// What becomes of sessions already running after a switch: within how many seconds they
/// follow, where they do, said as a sentence and for `--json`.
///
/// The tool's own answer, not a constant. A number of seconds is only ever shown for a tool
/// that really does follow on its own within them, and one whose sessions follow at their
/// login's next renewal, or that needs restarting, has no number at all rather than a zero
/// that reads as "at once".
fn followed(
    provider: pitboard_core::provider::ProviderId,
    adoption: &Adoption,
) -> (Option<u32>, String, Value) {
    match adoption {
        Adoption::PollingWithin(seconds) => (
            Some(*seconds),
            format!(
                "{} sessions already running follow within {seconds} seconds.\n",
                provider.name()
            ),
            json!({ "follows": "polling", "within_seconds": seconds }),
        ),
        Adoption::AtRenewal { file } => (
            None,
            format!(
                "{} sessions already running {}: {} is there. `pitboard doctor` says more.\n",
                provider.name(),
                pitboard_core::words::kept_until_renewed(),
                file.display()
            ),
            json!({ "follows": "renewal", "path": file }),
        ),
        Adoption::RestartRequired { program, .. } => (
            None,
            format!(
                "Restart any running `{program}` for this to take effect. \
                 It will not pick the switch up on its own.\n"
            ),
            json!({ "follows": "restart", "program": program }),
        ),
    }
}

fn forget(pitboard: &Pitboard, label: &str) -> Report {
    changed("forget", pitboard.forget(label), |email| {
        (
            json!({ "label": label, "email": email }),
            format!("Forgot {} ({email}).\n", paint(BOLD, label)),
        )
    })
}

/// What `pitboard stow` finds, asks and does: the login left in the file behind Claude Code's
/// store and what putting it away would do with it, then, once `confirm` says yes, putting it
/// away while the file still holds what was found. A login of an account nobody enrolled is
/// refused as the look found it, saying whose it is, and nothing is asked or changed. `None`
/// where the answer was no.
fn stow(pitboard: &Pitboard, confirm: impl FnOnce(&str) -> bool) -> Option<Report> {
    let left = match pitboard.left_login() {
        Err(error) => return Some(Report::failed(Some("stow"), error)),
        Ok(None) => {
            return Some(Report::done(
                "stow",
                json!({ "stowed": false }),
                format!("{}\n", pitboard_core::words::nothing_left()),
            ));
        }
        Ok(Some(left)) => left,
    };
    if let Some(refused) = left.refusal() {
        return Some(Report::failed(Some("stow"), refused));
    }
    if !confirm(&left_question(&left)) {
        return None;
    }
    Some(changed("stow", pitboard.stow(&left.seen), |stowed| {
        (
            json!({
                "stowed": true,
                "path": stowed.path,
                "login": stowed.kept.code(),
                "account": stowed.kept.account(),
                "dropped": stowed.dropped,
            }),
            stowed_lines(&stowed),
        )
    }))
}

/// The question `pitboard stow` asks before it puts the file away: what it holds, what goes
/// with it, and what becomes of Claude Code's sessions.
fn left_question(left: &pitboard_core::switch::Left) -> String {
    let lines = pitboard_core::words::left_lines(left);
    format!("{}\nPut it away? [y/N] ", lines.join("\n"))
}

/// What `pitboard stow` says it did, a line each.
fn stowed_lines(stowed: &pitboard_core::switch::Stowed) -> String {
    pitboard_core::words::stowed_lines(stowed)
        .into_iter()
        .map(|line| format!("{line}\n"))
        .collect()
}

fn abandon(pitboard: &Pitboard) -> Report {
    match pitboard.abandon_recovery() {
        Err(error) => Report::failed(Some("abandon"), error),
        Ok(None) => Report::done(
            "abandon",
            json!({ "abandoned": false }),
            "There is no interrupted switch to give up on.\n".into(),
        ),
        Ok(Some(a)) => Report::done(
            "abandon",
            json!({
                "abandoned": true,
                "from": a.from,
                "to": a.to,
                "logins_kept": a.kept,
            }),
            format!(
                "Gave up on the interrupted switch from {} to {}. {} login(s) kept; \
                 nothing was deleted. Run `pitboard` to see who is signed in.\n",
                paint(BOLD, &a.from),
                paint(BOLD, &a.to),
                a.kept
            ),
        ),
    }
}

#[derive(Subcommand)]
enum ScheduleCommand {
    /// Ask this computer's own scheduler to renew parked logins daily
    Install,
    /// Say whether it is installed
    Status,
    /// Take it away
    Uninstall,
}

fn renew(pitboard: &Pitboard) -> Report {
    let outcomes = match pitboard.renew() {
        Ok(outcomes) => outcomes,
        Err(error) => return Report::failed(Some("renew"), error),
    };
    let renewed = outcomes
        .iter()
        .filter(|(_, r)| matches!(r, Renewal::Renewed))
        .count();
    let human = format!(
        "{}\n",
        pitboard_core::words::renewal_note(outcomes.len(), renewed)
    );
    Report::done(
        "renew",
        json!({
            "accounts": outcomes.iter().map(|(key, outcome)| json!({
                "label": key.typed(),
                "provider": key.provider,
                "outcome": outcome.code(),
            })).collect::<Vec<_>>(),
            "renewed": renewed,
        }),
        human,
    )
}

fn schedule(pitboard: &Pitboard, what: &ScheduleCommand) -> Report {
    use pitboard_core::schedule::Installed;
    // From any home but the default one, the schedule is not this home's, and installing
    // it is refused until PITBOARD_HOME is unset.
    let renews_this_home = pitboard.schedule_renews_this_home();
    let renews_only = "The renewal schedule renews only the parked logins in ~/.pitboard, not \
                       those in the directory PITBOARD_HOME names.";
    let say = |installed: &Installed| match installed {
        Installed::Yes {
            path,
            every_seconds,
        } => (
            json!({
                "installed": true,
                "path": path,
                "every_seconds": every_seconds,
            }),
            format!(
                "Parked logins are renewed every {} by this computer's own scheduler.\n{}\n{}",
                pitboard_core::words::span(i64::from(*every_seconds)),
                path.display(),
                if renews_this_home {
                    String::new()
                } else {
                    format!("{renews_only}\n")
                }
            ),
        ),
        Installed::No => (
            json!({"installed": false}),
            format!(
                "Nothing is keeping parked logins alive here. They are renewed when you run \
                 `pitboard`, and otherwise not.\n{}\n",
                if renews_this_home {
                    "Run `pitboard schedule install` to change that.".to_string()
                } else {
                    format!(
                        "{renews_only} To turn it on for ~/.pitboard, unset PITBOARD_HOME, \
                         then run `pitboard schedule install`."
                    )
                }
            ),
        ),
        Installed::Unsupported => (
            json!({"installed": false, "supported": false}),
            "This computer has no scheduler Pitboard knows how to write.\n".to_string(),
        ),
    };
    match what {
        ScheduleCommand::Status => {
            let (data, human) = say(&pitboard.schedule());
            Report::done("schedule", data, human)
        }
        ScheduleCommand::Install => match pitboard.schedule_install() {
            Err(error) => Report::failed(Some("schedule"), error),
            Ok(path) => Report::done(
                "schedule",
                json!({"installed": true, "path": path}),
                format!(
                    "Parked logins will be renewed daily.\n{}\n\nIt renews your own \
                     parked logins and does nothing else: it never switches account and \
                     never asks Anthropic for usage.\n",
                    path.display()
                ),
            ),
        },
        ScheduleCommand::Uninstall => match pitboard.schedule_uninstall() {
            Err(error) => Report::failed(Some("schedule"), error),
            Ok(removed) => Report::done(
                "schedule",
                json!({"installed": false, "removed": removed}),
                if removed {
                    "Stopped renewing parked logins on a schedule. They are renewed when \
                     you run `pitboard`, and otherwise not.\n"
                        .to_string()
                } else {
                    "There was nothing scheduled.\n".to_string()
                },
            ),
        },
    }
}

fn adopt(pitboard: &Pitboard) -> Report {
    match pitboard.adopt() {
        Err(error) => Report::failed(Some("adopt"), error),
        Ok(None) => Report::done(
            "adopt",
            json!({ "adopted": false }),
            "This Pitboard directory was already written on this computer.\n".into(),
        ),
        Ok(Some(a)) => {
            let ways_back: Vec<String> = a
                .logins_dropped
                .iter()
                .map(|label| format!("  pitboard enroll {label} --sign-in\n"))
                .collect();
            Report::done(
                "adopt",
                json!({
                    "adopted": true,
                    "accounts": a.accounts,
                    "logins_dropped": a.logins_dropped,
                }),
                format!(
                    "Took over this directory: {} account(s) kept.\n\
                     {} parked login(s) dropped, because a login belongs to the computer \
                     that signed in.\n{}",
                    a.accounts.len(),
                    a.logins_dropped.len(),
                    if ways_back.is_empty() {
                        String::new()
                    } else {
                        format!("Sign in to each again:\n{}", ways_back.concat())
                    }
                ),
            )
        }
    }
}

fn repair(pitboard: &Pitboard) -> Report {
    changed("repair", pitboard.repair(), |r| {
        let human = if r.is_empty() {
            "Every parked login here is accounted for.\n".to_string()
        } else {
            let mut said = String::new();
            for (label, _) in &r.given_back {
                said.push_str(&format!(
                    "Gave {} back a parked login that nothing named.\n",
                    paint(BOLD, label)
                ));
            }
            if !r.deleted.is_empty() {
                said.push_str(&format!(
                    "Deleted {} parked login(s) Pitboard wrote down here and nothing \
                     recorded.\n",
                    r.deleted.len()
                ));
            }
            if !r.strangers.is_empty() {
                said.push_str(&format!(
                    "{} parked login(s) here belong to no account Pitboard knows and were \
                     not written down by this one. Left alone: the keychain is shared by \
                     the whole machine, and they may be another Pitboard's.\n",
                    r.strangers.len()
                ));
            }
            if !r.unreadable.is_empty() {
                said.push_str(&format!(
                    "{} could not be read this time and were left alone. Unlock the \
                     keychain and run this again.\n",
                    r.unreadable.len()
                ));
            }
            said
        };
        (
            json!({
                "given_back": r.given_back.iter().map(|(label, service)| json!({
                    "label": label,
                    "service": service,
                })).collect::<Vec<_>>(),
                "deleted": r.deleted,
                "strangers": r.strangers,
                "unreadable": r.unreadable,
            }),
            human,
        )
    })
}

fn log(pitboard: &Pitboard, lines: usize) -> Report {
    let entries = pitboard.log(lines);
    let width = entries.iter().map(|e| e.verb.len()).max().unwrap_or(0);
    let human = if entries.is_empty() {
        "Pitboard has not changed anything yet.\n".to_string()
    } else {
        entries
            .iter()
            .map(|e| {
                format!(
                    "{}  {}  {}  {}\n",
                    paint(DIM, &e.at),
                    ui::pad(&e.verb, width),
                    e.subject,
                    paint(if e.outcome == "ok" { DIM } else { WARN }, &e.outcome),
                )
            })
            .collect()
    };
    Report::done(
        "log",
        json!({
            "entries": entries
                .iter()
                .map(|e| json!({
                    "at": e.at,
                    "caller": e.caller,
                    "verb": e.verb,
                    "subject": e.subject,
                    "outcome": e.outcome,
                }))
                .collect::<Vec<_>>(),
        }),
        human,
    )
}

fn uninstall(pitboard: &Pitboard) -> Report {
    changed("uninstall", pitboard.uninstall(), |removed| {
        let mut human = format!(
            "Removed {} parked login(s). The login each tool is signed in with is untouched.\n",
            removed.parks
        );
        if removed.schedule_removed {
            human.push_str("Turned off the daily renewal schedule.\n");
        }
        if removed.left > 0 {
            human.push_str(&format!(
                "Left {} parked login(s) that `pitboard repair` found and this Pitboard did \
                 not write, because they may be another Pitboard's.\n",
                removed.left
            ));
        }
        if removed.pending > 0 {
            human.push_str(&format!(
                "{} could not be deleted yet, so ~/.pitboard was kept; run `pitboard \
                 uninstall` again.\n",
                removed.pending
            ));
        } else if removed.home_removed {
            human.push_str(
                "~/.pitboard is gone. Remove Pitboard itself the way you installed it.\n",
            );
        }
        (
            json!({
                "parks_removed": removed.parks,
                "parks_pending": removed.pending,
                "parks_left": removed.left,
                "home_removed": removed.home_removed,
                "schedule_removed": removed.schedule_removed,
            }),
            human,
        )
    })
}

fn rename(pitboard: &Pitboard, from: &str, to: &str) -> Report {
    changed("rename", pitboard.rename(from, to), |email| {
        (
            json!({ "from": from, "to": to, "email": email }),
            format!(
                "Renamed {} to {} ({email}).\n",
                paint(BOLD, from),
                paint(BOLD, to)
            ),
        )
    })
}

/// Whether a command that deletes something asks first: only where there is someone to ask.
/// `--yes`, `--json`, a pipe and a script go straight through, and so does a run that may
/// change nothing, as root or under sudo, whose answer would only be refused.
fn asks(pitboard: &Pitboard, yes: bool, json: bool) -> bool {
    !yes && !json
        && pitboard.permit().is_ok()
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal()
}

/// Asks `question` on standard error: yes only for `y`, `Y` or `yes`.
fn agreed(question: &str) -> bool {
    eprint!("{question}");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    matches!(answer.trim(), "y" | "Y" | "yes")
}

/// A usage error keeps clap's own rendering, unless the caller asked for JSON, which is
/// promised for every outcome.
fn parse() -> Result<Cli, ExitCode> {
    Cli::try_parse().map_err(|e| {
        let wants_json = std::env::args_os().any(|arg| arg == "--json");
        if wants_json && e.use_stderr() {
            let message = e.render().to_string().trim().to_string();
            emit(Report::failed(None, Error::Usage(message)), true)
        } else {
            let _ = e.print();
            ExitCode::from(e.exit_code().clamp(0, 255) as u8)
        }
    })
}

fn main() -> ExitCode {
    let cli = match parse() {
        Ok(cli) => cli,
        Err(exit) => return exit,
    };
    let command = cli.command.unwrap_or(Command::Status {
        offline: false,
        fresh: false,
    });
    // A build that may do nothing here says so before it reads anything, its environment
    // included. The core refuses it too, at the gate every change passes and wherever it
    // reads Pitboard's accounts.
    if command.asks_whether_this_build_may_run()
        && let Err(refused) = pitboard_core::release::check()
    {
        return emit(Report::failed(Some(command.name()), refused), cli.json);
    }
    let ctx = Context::from_env();
    // The schedule's job on Linux says so by the marker, rather than by what systemd passes.
    let ctx = match command {
        Command::Renew { scheduled: true } => ctx.started_by_the_schedule(),
        _ => ctx,
    };
    let pitboard = Pitboard::new(ctx);
    // A home that is empty or relative is refused before a command reads or writes under
    // it, rather than taken to be under the folder this was run from. The core refuses it
    // too, wherever it reads Pitboard's accounts or is asked to change anything; this also
    // covers `log`, `schedule status` and the status line. `renew` is left to the core,
    // which asks of the homes a run of the schedule renews in.
    if command.refuses_a_home_that_is_not_a_full_path()
        && let Err(refused) = pitboard.check_homes()
    {
        return emit(Report::failed(Some(command.name()), refused), cli.json);
    }
    let report = match command {
        Command::Status { offline, fresh } => status(&pitboard, offline, fresh),
        Command::Doctor => doctor(&pitboard),
        Command::Statusline => statusline(&pitboard),
        Command::Watch { at, once } => {
            // The parser takes only a share there can be.
            let threshold = Threshold::new(at).unwrap_or_default();
            if !once {
                return watch::run(&pitboard, threshold, cli.json);
            }
            watch::once(&pitboard, threshold)
        }
        Command::Enroll {
            label,
            sign_in: true,
        } => enroll_signing_in(&pitboard, &label),
        Command::Enroll { label, .. } => {
            enrolled(&pitboard, &label, pitboard.enroll_current(&label))
        }
        Command::Use { label } => use_account(&pitboard, &label),
        Command::Forget { label, yes } => {
            // The way back is a browser sign-in for that account, which is the cost
            // Pitboard exists to spare people.
            if asks(&pitboard, yes, cli.json)
                && !agreed(&format!(
                    "Forget {label} and delete its parked login? Adding it again needs a \
                     browser sign-in. [y/N] "
                ))
            {
                return ExitCode::SUCCESS;
            }
            forget(&pitboard, &label)
        }
        Command::Stow { yes } => {
            // A session signing in with the file is signed out once it is gone.
            let ask = asks(&pitboard, yes, cli.json);
            match stow(&pitboard, |question| !ask || agreed(question)) {
                Some(report) => report,
                None => return ExitCode::SUCCESS,
            }
        }
        Command::Abandon => abandon(&pitboard),
        Command::Repair => repair(&pitboard),
        Command::Adopt => adopt(&pitboard),
        Command::Renew { .. } => renew(&pitboard),
        Command::Schedule { ref what } => schedule(&pitboard, what),
        Command::Log { lines } => log(&pitboard, lines),
        Command::Uninstall { yes } => {
            if asks(&pitboard, yes, cli.json)
                && !agreed(
                    "Delete every parked login this Pitboard wrote, the daily renewal schedule \
                     and ~/.pitboard? The account you are signed in to stays signed in; the \
                     others need a browser sign-in again. [y/N] ",
                )
            {
                return ExitCode::SUCCESS;
            }
            uninstall(&pitboard)
        }
        Command::Rename { from, to } => rename(&pitboard, &from, &to),
        // These write a file for a shell or for man, not a report, so there is no envelope
        // to put them in. Asking for one is a command line that cannot be satisfied.
        Command::Completions { .. } | Command::Manpage if cli.json => Report {
            exit: 2,
            failure: Some((
                "output_is_not_a_report",
                "this command writes a generated file to stdout, so it has no JSON form".into(),
            )),
            ..Report::done("generate", json!({}), String::new())
        },
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                env!("CARGO_BIN_NAME"),
                &mut std::io::stdout(),
            );
            return ExitCode::SUCCESS;
        }
        Command::Manpage => {
            return match manpage::render(Cli::command(), &mut std::io::stdout()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::FAILURE,
            };
        }
    };
    emit(report, cli.json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_definition_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn a_mistyped_flag_is_rejected_rather_than_ignored() {
        assert!(Cli::try_parse_from(["pitboard", "--jsno", "status"]).is_err());
        assert!(Cli::try_parse_from(["pitboard", "status", "--verbose"]).is_err());
        assert!(Cli::try_parse_from(["pitboard", "status", "extra", "words"]).is_err());
    }

    #[test]
    fn json_is_accepted_before_or_after_the_subcommand() {
        assert!(
            Cli::try_parse_from(["pitboard", "--json", "use", "work"])
                .unwrap()
                .json
        );
        assert!(
            Cli::try_parse_from(["pitboard", "use", "work", "--json"])
                .unwrap()
                .json
        );
    }

    /// A switch made while `.credentials.json` sits behind the keychain says when running
    /// sessions take it, and why, with no number of seconds: when a session's login is next
    /// renewed is the session's.
    #[test]
    fn a_switch_sessions_take_at_renewal_says_so_with_no_seconds() {
        let file = std::path::PathBuf::from("/Users/x/.claude/.credentials.json");
        let (seconds, follows, adoption) = followed(
            pitboard_core::provider::ProviderId::Claude,
            &Adoption::AtRenewal { file },
        );
        assert_eq!(seconds, None);
        assert_eq!(
            follows,
            "Claude Code sessions already running keep the account they are on until their \
             login is next renewed, or until they are started again: \
             /Users/x/.claude/.credentials.json is there. `pitboard doctor` says more.\n"
        );
        assert_eq!(
            adoption,
            json!({"follows": "renewal", "path": "/Users/x/.claude/.credentials.json"})
        );
    }

    /// A login of an account nobody enrolled is refused as the look found it: nothing is
    /// asked, and putting it away is not tried, so the file stays and the activity log has no
    /// line of it. On a machine in memory, whose Anthropic answers from a script.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn stow_refuses_a_login_of_an_account_not_enrolled_as_the_look_found_it() {
        use pitboard_core::api::Owner;
        use pitboard_core::testing::{FixedClock, MemoryHost, ScriptedApi, live_service};
        use std::sync::Arc;
        const NOW: i64 = 1_760_000_000;
        let home = std::env::temp_dir().join(format!(
            "pitboard-cli-stow-not-enrolled-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let (mem, api) = (MemoryHost::new(), ScriptedApi::new());
        let ctx = Context::new(home.clone())
            .with_pitboard_home(home.join(".pitboard"))
            .with_memory_stores(Arc::clone(&mem))
            .with_scripted_api(Arc::clone(&api))
            .with_clock(Arc::new(FixedClock::at(NOW)));
        let service = live_service(&ctx);
        let login = |who: &str| {
            api.owned_by(
                &format!("access-{who}"),
                Owner {
                    account_uuid: who.into(),
                    email: format!("{who}@example.com"),
                    organization_uuid: format!("org-{who}"),
                },
            );
            json!({"claudeAiOauth": {
                "accessToken": format!("access-{who}"),
                "refreshToken": format!("refresh-{who}"),
                "expiresAt": (NOW + 3600) * 1000,
            }})
            .to_string()
        };
        mem.live().plant(&service, &login("here"));
        let file = mem.file_at(home.join(".claude").join(".credentials.json"));
        let left = login("stranger");
        file.plant(&service, &left);
        let pitboard = Pitboard::new(ctx);

        let report = stow(&pitboard, |_| panic!("nothing is asked")).expect("a refusal");

        let logged = pitboard.log(10);
        let _ = std::fs::remove_dir_all(&home);
        assert!(
            matches!(&report.result, Err(error) if error.code() == "left_login_not_enrolled"),
            "{:?}",
            report.result
        );
        assert_eq!(file.peek(&service), Some(left));
        assert!(logged.is_empty(), "{logged:?}");
    }

    /// `pitboard stow` asks before it deletes the file: what it holds and what putting it away
    /// does with that, the keys that go with it, and what becomes of Claude Code's sessions.
    /// Once it has, it says the same of what it did.
    #[test]
    fn stow_asks_saying_what_the_file_holds_and_says_what_it_did() {
        use pitboard_core::switch::{Foreseen, Kept, Left, Stowed};
        let path = std::path::PathBuf::from("/Users/x/.claude/.credentials.json");
        let parked = Kept::ParkedNow {
            label: "work".into(),
        };
        let question = left_question(&Left {
            path: path.clone(),
            seen: "0123456789abcdef".into(),
            login: Foreseen::Kept(parked.clone()),
            dropped: vec!["mcpOAuth".into()],
        });
        assert_eq!(
            question,
            "/Users/x/.claude/.credentials.json holds `work`'s login, and Pitboard holds no \
             parked login of `work` to keep in its place. Putting it away parks this one for \
             `work`, then deletes the file.\n\
             It also holds 1 other key, `mcpOAuth`, which goes with the file: Pitboard does not \
             move it, and Claude Code's document in the keychain keeps its own.\n\
             Once the file is gone, Claude Code sessions already running follow a switch within \
             33 seconds again, and one that signed in with its login, such as one over SSH, is \
             signed out.\n\
             Put it away? [y/N] "
        );
        let said = stowed_lines(&Stowed {
            path,
            kept: Kept::NoLogin,
            dropped: vec!["mcpOAuth".into(), "coworkRemoteDevice".into()],
        });
        assert_eq!(
            said,
            "Deleted /Users/x/.claude/.credentials.json, which held no Claude Code login.\n\
             It also held 2 other keys, `mcpOAuth` and `coworkRemoteDevice`, which went with the \
             file: Pitboard does not move them, and Claude Code's document in the keychain keeps \
             its own.\n\
             Once the file is gone, Claude Code sessions already running follow a switch within \
             33 seconds again.\n"
        );
        assert!(matches!(
            Cli::try_parse_from(["pitboard", "stow", "-y"])
                .expect("parsed")
                .command,
            Some(Command::Stow { yes: true })
        ));
    }

    #[test]
    fn a_new_label_is_one_word() {
        assert!(Cli::try_parse_from(["pitboard", "rename", "a", "personal"]).is_ok());
        for bad in ["", "two words", "tab\there"] {
            assert!(
                Cli::try_parse_from(["pitboard", "rename", "a", bad]).is_err(),
                "{bad:?}"
            );
            assert!(
                Cli::try_parse_from(["pitboard", "enroll", bad]).is_err(),
                "{bad:?}"
            );
        }
    }

    /// What the schedule's job runs, as the core writes it into the systemd unit, is a
    /// command line this one takes, and the marker in it is said only where it is given.
    /// It is hidden: a person has no reason to type it.
    #[test]
    fn the_schedules_marker_is_a_run_of_renew_this_command_line_takes() {
        let scheduled = std::iter::once("pitboard").chain(pitboard_core::schedule::SCHEDULED_RUN);
        assert!(matches!(
            Cli::try_parse_from(scheduled).expect("parsed").command,
            Some(Command::Renew { scheduled: true })
        ));
        assert!(matches!(
            Cli::try_parse_from(["pitboard", "renew"])
                .expect("parsed")
                .command,
            Some(Command::Renew { scheduled: false })
        ));
        let help = Cli::command()
            .find_subcommand_mut("renew")
            .expect("renew")
            .render_long_help()
            .to_string();
        assert!(!help.contains("--scheduled"), "{help}");
    }

    /// The shells' completions offer `--scheduled`, as they offer the hidden `manpage`:
    /// clap_complete writes hidden arguments and commands into every shell's file. Where a
    /// shell shows a description, it is the marker's help, which says what such a run
    /// renews, since that is all a person who picks it gets, in plain text, since a shell
    /// shows Markdown as written.
    #[test]
    fn the_completions_offer_the_schedules_marker_and_say_what_it_renews() {
        use clap_complete::Shell;
        let said = "Renew as the daily renewal schedule does";
        for (shell, offered, described) in [
            (Shell::Bash, "--scheduled", false),
            (Shell::Zsh, "'--scheduled[", true),
            (Shell::Fish, "-l scheduled", true),
        ] {
            let mut out = Vec::new();
            clap_complete::generate(shell, &mut Cli::command(), "pitboard", &mut out);
            let file = String::from_utf8(out).expect("text");
            let line = file
                .lines()
                .find(|line| line.contains(offered))
                .unwrap_or_else(|| panic!("{shell}: {file}"));
            assert_eq!(line.contains(said), described, "{shell}: {line}");
            assert!(
                !line.contains('`'),
                "{shell} shows Markdown as written: {line}"
            );
            assert!(
                file.contains("manpage"),
                "{shell}: as the hidden command is"
            );
            assert!(!file.contains("systemd"), "{shell}: {file}");
        }
    }

    /// `schedule status` run while `PITBOARD_HOME` names another directory says that the
    /// schedule renews only the parked logins in `~/.pitboard`. With nothing installed it
    /// says to unset `PITBOARD_HOME` before `pitboard schedule install`, which is refused
    /// until then, rather than to run it as it is. From the default home it says what it
    /// said. On a machine in memory, whose scheduler asks nobody.
    #[test]
    #[cfg_attr(windows, ignore = "W25: Task Scheduler")]
    fn schedule_status_from_another_home_says_what_the_schedule_renews() {
        use pitboard_core::testing::MemoryHost;
        let home = std::env::temp_dir().join(format!(
            "pitboard-cli-schedule-status-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its folder")).expect("made");
        std::fs::write(&program, "").expect("a command line");
        let ctx = Context::new(home.clone())
            .with_schedule_program(program)
            .with_memory_stores(MemoryHost::new());
        let default = Pitboard::new(ctx.clone());
        let elsewhere = Pitboard::new(ctx.with_pitboard_home(home.join("elsewhere")));
        let said = |pitboard: &Pitboard| schedule(pitboard, &ScheduleCommand::Status).human;
        let renews_only = "renews only the parked logins in ~/.pitboard, not those in the \
                           directory PITBOARD_HOME names";

        let here = said(&default);
        assert!(
            here.ends_with("Run `pitboard schedule install` to change that.\n"),
            "{here}"
        );
        assert!(!here.contains(renews_only), "{here}");
        let there = said(&elsewhere);
        assert!(there.starts_with("Nothing is keeping"), "{there}");
        assert!(
            !there.contains("Run `pitboard schedule install`"),
            "{there}"
        );
        assert!(there.contains(renews_only), "{there}");
        assert!(
            there.ends_with("unset PITBOARD_HOME, then run `pitboard schedule install`.\n"),
            "{there}"
        );

        default
            .schedule_install()
            .expect("installed from the default home");
        let here = said(&default);
        assert!(
            here.starts_with("Parked logins are renewed every"),
            "{here}"
        );
        assert!(!here.contains(renews_only), "{here}");
        let there = said(&elsewhere);
        assert!(
            there.starts_with("Parked logins are renewed every"),
            "{there}"
        );
        assert!(there.contains(renews_only), "{there}");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A build that may do nothing here, a Windows build of a release before Windows is
    /// released, answers only what prints a generated file, `completions` and `manpage`,
    /// besides `--version` and `--help`, which clap answers before a command is read. Every
    /// other command asks the release gate first, reading included.
    #[test]
    fn every_command_but_a_generated_file_asks_whether_this_build_may_run() {
        let asks = |args: &[&str]| {
            let cli = Cli::try_parse_from(std::iter::once("pitboard").chain(args.iter().copied()))
                .unwrap_or_else(|e| panic!("{args:?}: {e}"));
            cli.command
                .unwrap_or(Command::Status {
                    offline: false,
                    fresh: false,
                })
                .asks_whether_this_build_may_run()
        };
        for args in [
            &[][..],
            &["status", "--offline"],
            &["enroll", "work"],
            &["enroll", "codex/work", "--sign-in"],
            &["use", "work"],
            &["forget", "work", "-y"],
            &["stow", "-y"],
            &["abandon"],
            &["repair"],
            &["adopt"],
            &["renew"],
            &["renew", "--scheduled"],
            &["schedule", "install"],
            &["schedule", "status"],
            &["schedule", "uninstall"],
            &["log"],
            &["uninstall", "-y"],
            &["rename", "work", "job"],
            &["doctor"],
            &["statusline"],
        ] {
            assert!(asks(args), "{args:?}");
        }
        assert!(!asks(&["completions", "bash"]));
        assert!(!asks(&["manpage"]));
    }

    #[test]
    fn no_arguments_means_status() {
        assert!(Cli::try_parse_from(["pitboard"]).unwrap().command.is_none());
    }

    /// Only `pitboard.1` is installed, so the page sets out every command itself. It used
    /// to list `pitboard-status(1)` and the like, pages nobody installed.
    #[test]
    fn the_man_page_sets_out_every_command_itself() {
        let mut out = Vec::new();
        manpage::render(Cli::command(), &mut out).unwrap();
        let page = String::from_utf8(out).unwrap();
        assert!(
            !page.contains("(1)"),
            "a reference to another page:\n{page}"
        );
        for typed in [
            "pitboard status",
            "pitboard enroll",
            "pitboard use",
            "pitboard forget",
            "pitboard stow",
            "pitboard abandon",
            "pitboard repair",
            "pitboard adopt",
            "pitboard renew",
            "pitboard schedule install",
            "pitboard schedule status",
            "pitboard schedule uninstall",
            "pitboard log",
            "pitboard uninstall",
            "pitboard rename",
            "pitboard doctor",
            "pitboard statusline",
            "pitboard completions",
        ] {
            assert!(
                page.contains(&format!("\\fB{typed}\\fR")),
                "{typed} missing:\n{page}"
            );
        }
        for option in [r"\-\-offline", r"\-\-sign\-in", r"\-y", r"\-\-lines"] {
            assert!(page.contains(option), "{option} missing:\n{page}");
        }
        assert!(page.contains("[possible values: bash"), "{page}");
        assert!(page.contains("[default: 20]"), "{page}");
        assert!(
            !page.contains("pitboard manpage"),
            "the hidden command is shown"
        );
        assert!(!page.contains("pitboard help"), "clap's own help is shown");
        assert!(
            !page.contains("scheduled"),
            "the schedule's marker is shown"
        );
        assert!(
            page.contains(".SH SYNOPSIS") && page.contains(".SH VERSION"),
            "{page}"
        );
    }
}
