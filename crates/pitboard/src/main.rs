use anstream::{ColorChoice, eprintln, print, println};
use anstyle::{AnsiColor, Style};
use clap::{CommandFactory, Parser, Subcommand};
use pitboard_core::context::Context;
use pitboard_core::doctor;
use pitboard_core::error::Error;
use pitboard_core::provider::{Adoption, ProviderId};
use pitboard_core::service::{Changing, Done, Failed, Pitboard, Warning};
use pitboard_core::switch::{Enrolled, Outcome, Renewal};
use serde_json::{Value, json};
use std::io::{IsTerminal, Read, Write};
use std::process::ExitCode;
use ui::{BOLD, DIM, WARN, paint};

mod manpage;
mod render;
mod ui;

/// Bumped only when a field changes shape. Adding a field or an error code is not a
/// breaking change for a consumer; renaming or removing one is.
const CONTRACT: u32 = 1;

const ERROR: Style = AnsiColor::Red.on_default().bold();
const WARNING: Style = AnsiColor::Yellow.on_default().bold();

/// Park and restore your own logins of Claude Code, Codex and Claude Desktop, and see what
/// each one has left.
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
        /// Codex account and `desktop/work` a Claude Desktop one; a bare name means Claude
        /// Code
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
        /// The label the account was enrolled under, such as `work` or `codex/work`; with
        /// --signed-out, the tool, such as `desktop`
        label: String,
        /// Park the account in use and leave the tool signed out, so the next account to
        /// sign in to it can be enrolled. For Claude Desktop, which has no sign-in of its
        /// own for Pitboard to open
        #[arg(long)]
        signed_out: bool,
    },
    /// Drop an account and its parked login
    Forget {
        /// The label to drop
        label: String,
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
    Renew,
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
    /// Claude Desktop's own settings
    Desktop {
        #[command(subcommand)]
        what: DesktopCommand,
    },
    /// Check that what Pitboard relies on still holds on this machine
    Doctor,
    /// One line for Claude Code's status bar; reads its session JSON on stdin
    Statusline,
    /// Print a shell completion script
    Completions { shell: clap_complete::Shell },
    /// Print the man page
    #[command(hide = true)]
    Manpage,
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
        println!("{envelope}");
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
    // The account this would sign in to again, of the tool the label is for: another
    // tool's account of the same name is somebody else to sign in as.
    let existing = pitboard.account_to_enroll(label);
    let who = existing
        .as_ref()
        .map(|a| a.email.clone())
        .filter(|email| !email.is_empty())
        .unwrap_or_else(|| "the account to add".to_string());
    let tool = existing.as_ref().map_or_else(
        || {
            pitboard_core::label::choose(label)
                .map(|chosen| chosen.provider)
                .unwrap_or(pitboard_core::label::DEFAULT)
        },
        pitboard_core::state::Account::provider,
    );
    // Claude Desktop has no sign-in Pitboard can open, and the core says so; saying it
    // was being opened first would be a line the next one contradicts.
    if tool != ProviderId::Desktop {
        eprintln!(
            "Opening {}'s sign-in. Sign in as {who}; the account in use now stays signed in.",
            tool.name()
        );
    }
    match pitboard.sign_in(label) {
        Ok(login) => enrolled(pitboard, label, pitboard.enroll_signed_in(label, login)),
        Err(failed) => Report::refused("enroll", failed),
    }
}

/// ` (email)` after an account's name, or nothing for an account with no email, as a
/// Claude Desktop account has: an empty pair of brackets says nothing.
fn bracketed(email: &str) -> String {
    if email.is_empty() {
        String::new()
    } else {
        format!(" ({email})")
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
            // Claude Desktop adds its next account by being signed out, not by a sign-in
            // of Pitboard's, so there is no `--sign-in` to offer.
            Enrolled::Current { email } if provider == ProviderId::Desktop => {
                let human = format!(
                    "Enrolled {} for {}, the account signed in now.\n",
                    paint(BOLD, bare(label)),
                    provider.name()
                );
                ("current", email, human)
            }
            Enrolled::Current { email } => {
                let human = format!(
                    "Enrolled {name}{}, the account signed in now.\n\
                     Add another without signing out of it: pitboard enroll {another} \
                     --sign-in\n",
                    bracketed(&email)
                );
                ("current", email, human)
            }
            Enrolled::SignedIn { email } => {
                let human = format!(
                    "Enrolled {name}{}. Switch to it with: pitboard use {to_use}\n",
                    bracketed(&email)
                );
                ("signed_in", email, human)
            }
            Enrolled::Renewed { email } => {
                let human = format!(
                    "Renewed {name}{}: its parked login is a fresh one.\n",
                    bracketed(&email)
                );
                ("renewed", email, human)
            }
            Enrolled::InUse { email, again } => {
                let human = if again {
                    format!(
                        "Signed in to {name}{} again. Its new login is the one in use now.\n",
                        bracketed(&email)
                    )
                } else {
                    format!(
                        "Enrolled {name}{}, the account signed in now. Its new login is the one \
                         in use.\n",
                        bracketed(&email)
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

/// What the switch says of when sessions already running follow it: the tool's own answer,
/// not a constant. A number of seconds is only ever shown for a tool that really does
/// follow on its own within them, and a tool that needs restarting has no number at all
/// rather than a zero that reads as "at once".
fn adoption_said(provider: ProviderId, adoption: Adoption) -> (Option<u32>, String, Value) {
    match adoption {
        Adoption::PollingWithin(seconds) => (
            Some(seconds),
            format!(
                "{} sessions already running follow within {seconds} seconds.\n",
                provider.name()
            ),
            json!({ "follows": "polling", "within_seconds": seconds }),
        ),
        Adoption::RestartRequired { program, .. } => (
            None,
            format!(
                "Restart any running `{program}` for this to take effect. \
                 It will not pick the switch up on its own.\n"
            ),
            json!({ "follows": "restart", "program": program }),
        ),
        // An app that reads its login only when it starts was quit before the switch, so
        // there is nothing left running to restart.
        Adoption::NextLaunch { program } => (
            None,
            format!("Open {program} to use it.\n"),
            json!({ "follows": "next_launch", "program": program }),
        ),
    }
}

/// A label as somebody reads it, without the tool it was typed with: `personal` for
/// `desktop/personal`. Only for a sentence that names the tool itself.
fn bare(typed: &str) -> &str {
    match typed.split_once(pitboard_core::label::SEPARATOR) {
        Some((tool, label)) if ProviderId::parse(tool).is_some() => label,
        _ => typed,
    }
}

fn use_account(pitboard: &Pitboard, label: &str) -> Report {
    switched(pitboard.switch_to(label))
}

/// `pitboard use desktop --signed-out`: the label is the tool to sign out.
fn use_signed_out(pitboard: &Pitboard, tool: &str) -> Report {
    match ProviderId::parse(tool) {
        Some(which) => switched(pitboard.switch_to_signed_out(which)),
        None => Report::failed(
            Some("use"),
            Error::ProviderUnknown {
                typed: tool.to_string(),
                known: ProviderId::ALL
                    .iter()
                    .map(|p| p.code().to_string())
                    .collect(),
            },
        ),
    }
}

fn switched(outcome: Changing<Outcome>) -> Report {
    changed("use", outcome, |outcome| match outcome {
        Outcome::AlreadyActive { label } => (
            json!({ "to": label, "changed": false }),
            format!("{} is already signed in.\n", paint(BOLD, &label)),
        ),
        // A sign-out that found nobody signed in: said as one, with nothing parked.
        Outcome::AlreadySignedOut { provider } => (
            json!({
                "to": null,
                "signed_out": true,
                "provider": provider,
                "changed": false,
            }),
            format!("{} is already signed out.\n", provider.name()),
        ),
        Outcome::Switched {
            provider,
            from,
            to,
            parked,
            adoption,
        } => {
            let (seconds, follows, adoption_json) = adoption_said(provider, adoption);
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
                if provider == ProviderId::Desktop {
                    // One app, so the sentence names it, and the labels without it.
                    format!(
                        "Switched {} to {} (was {}).\n{follows}",
                        provider.name(),
                        paint(BOLD, bare(&to)),
                        paint(BOLD, bare(&from)),
                    )
                } else {
                    format!(
                        "Switched to {}; {} is parked.\n{follows}",
                        paint(BOLD, &to),
                        paint(BOLD, &from),
                    )
                },
            )
        }
        // Nothing was signed in, so nothing was parked.
        Outcome::Installed {
            provider,
            to,
            adoption,
        } => {
            let (seconds, follows, adoption_json) = adoption_said(provider, adoption);
            (
                json!({
                    "from": null,
                    "to": to,
                    "provider": provider,
                    "changed": true,
                    "adoption_ceiling_seconds": seconds,
                    "adoption": adoption_json,
                }),
                format!("Switched to {}.\n{follows}", paint(BOLD, &to)),
            )
        }
        // Only an app whose login is a folder is signed out this way, and the next
        // account is whoever signs in to it next, so the way on is the app's own sign-in.
        Outcome::SignedOut {
            provider,
            from,
            parked,
            adoption,
        } => {
            let (seconds, _, adoption_json) = adoption_said(provider, adoption);
            let next = pitboard_core::state::Key::new(provider, "<label>").typed();
            (
                json!({
                    "from": from,
                    "to": null,
                    "signed_out": true,
                    "provider": provider,
                    "changed": true,
                    "parked_at": parked.parked_at,
                    "adoption_ceiling_seconds": seconds,
                    "adoption": adoption_json,
                }),
                format!(
                    "Parked {}. {} is signed out.\n\
                     Open {program}, sign in to the other account, quit {program}, then run\n  \
                     pitboard enroll {next}\n",
                    paint(BOLD, bare(&from)),
                    provider.name(),
                    program = provider.program(),
                ),
            )
        }
    })
}

fn forget(pitboard: &Pitboard, label: &str) -> Report {
    changed("forget", pitboard.forget(label), |email| {
        (
            json!({ "label": label, "email": email }),
            format!("Forgot {}{}.\n", paint(BOLD, label), bracketed(&email)),
        )
    })
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
    let outcomes = pitboard.renew();
    let renewed = outcomes
        .iter()
        .filter(|(_, r)| matches!(r, Renewal::Renewed))
        .count();
    // A login Pitboard cannot renew is said on a line of its own with when it lapses,
    // because the only way to keep it is to sign in to it again before then.
    let lapsing: Vec<String> = outcomes
        .iter()
        .filter_map(|(key, outcome)| match outcome {
            Renewal::NotRenewable { expires_at, .. } => Some(format!(
                "{}: not renewable; {}\n",
                key.typed(),
                match expires_at {
                    Some(at) => format!(
                        "its sign-in lapses {}",
                        pitboard_core::time::local(*at, "%F")
                    ),
                    None => "when its sign-in lapses is not known".to_string(),
                }
            )),
            _ => None,
        })
        .collect();
    let due = outcomes.len() - lapsing.len();
    let mut human = if due == 0 && !lapsing.is_empty() {
        String::new()
    } else {
        format!("{}\n", pitboard_core::words::renewal_note(due, renewed))
    };
    human.push_str(&lapsing.concat());
    Report::done(
        "renew",
        json!({
            "accounts": outcomes.iter().map(|(key, outcome)| {
                let mut said = json!({
                    "label": key.typed(),
                    "provider": key.provider,
                    "outcome": outcome.code(),
                });
                if let Renewal::NotRenewable { expires_at, .. } = outcome {
                    said["expires_at"] = json!(expires_at);
                }
                said
            }).collect::<Vec<_>>(),
            "renewed": renewed,
        }),
        human,
    )
}

#[derive(Subcommand)]
enum DesktopCommand {
    /// Ask claude.ai how much each Claude Desktop account has left, which needs Claude's
    /// key. Off until you turn it on; switching accounts never needs it
    LiveUsage {
        #[command(subcommand)]
        what: LiveUsageCommand,
    },
}

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
enum LiveUsageCommand {
    /// Read Claude's key once, which macOS asks your login password for, and ask claude.ai
    /// from then on
    Enable,
    /// Stop asking claude.ai, and forget the key
    Disable,
    /// Say whether it is on, and whether macOS lets Pitboard read the key
    Status,
}

/// What `enable` is about to do, said before macOS asks, because macOS's own question does
/// not say why Pitboard wants the key or what "Always Allow" lets through.
const LIVE_USAGE_EXPLAINED: &str = "\
Pitboard will read Claude's encryption key once to ask claude.ai how much each
Claude Desktop account has left. macOS will ask for your login password:
choose \"Always Allow\" so it does not ask again.
Always Allow lets any program that runs /usr/bin/security read this key.
Switching accounts never needs it. Turn it off with `pitboard desktop live-usage disable`.
";

fn live_usage(pitboard: &Pitboard, what: LiveUsageCommand, as_json: bool) -> Report {
    use pitboard_core::status::Approval;
    let turned = match what {
        LiveUsageCommand::Status => Ok(pitboard.live_usage()),
        LiveUsageCommand::Enable => {
            // Said only to somebody at a terminal, who is the one macOS is about to ask. A
            // script gets the envelope and nothing in front of it.
            if !as_json && std::io::stderr().is_terminal() {
                eprint!("{LIVE_USAGE_EXPLAINED}");
            }
            pitboard.enable_live_usage()
        }
        LiveUsageCommand::Disable => pitboard.disable_live_usage(),
    };
    let state = match turned {
        Ok(state) => state,
        Err(error) => return Report::failed(Some("desktop"), error),
    };
    let human = match (what, state.enabled, state.approval) {
        (LiveUsageCommand::Enable, true, Approval::Granted) => "Live usage is on.\n".to_string(),
        (LiveUsageCommand::Disable, false, _) => "Live usage is off. Claude Desktop's usage is \
            read from its own history, and Pitboard has forgotten Claude's key.\n"
            .to_string(),
        (_, false, _) => "Live usage is off: Claude Desktop's usage is read from its own \
            history. Turn it on with `pitboard desktop live-usage enable`.\n"
            .to_string(),
        (_, true, Approval::Granted) => format!(
            "Live usage is on: Claude Desktop's usage is asked of claude.ai{}.\n",
            state.last_ok_at.map_or_else(String::new, |at| format!(
                ", last answered {}",
                pitboard_core::time::moment(at, unix_now())
            ))
        ),
        (_, true, _) => format!(
            "Live usage is paused: macOS has not let Pitboard read Claude's key{}. Run \
             `pitboard desktop live-usage enable` to ask again.\n",
            state
                .reason
                .as_deref()
                .map_or_else(String::new, |reason| format!(" ({reason})"))
        ),
    };
    Report::done(
        "desktop",
        json!({ "live_usage": render::status::live_usage_json(&state) }),
        human,
    )
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn schedule(pitboard: &Pitboard, what: &ScheduleCommand) -> Report {
    use pitboard_core::schedule::Installed;
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
                "Parked logins are renewed every {} by this computer's own scheduler.\n{}\n",
                pitboard_core::words::span(i64::from(*every_seconds)),
                path.display()
            ),
        ),
        Installed::No => (
            json!({"installed": false}),
            "Nothing is keeping parked logins alive here. They are renewed when you run \
             `pitboard`, and otherwise not.\nRun `pitboard schedule install` to change \
             that.\n"
                .to_string(),
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
                "{} could not be deleted, so ~/.pitboard was kept; run `pitboard \
                 uninstall` again.\n",
                removed.pending
            ));
        } else if removed.home_removed {
            human.push_str(
                "~/.pitboard is gone. Remove Pitboard itself the way you installed it.\n",
            );
        }
        for dir in &removed.kept {
            human.push_str(&format!(
                "Kept {}: it holds Claude Desktop data Pitboard never deletes. Look through \
                 it, then remove it yourself.\n",
                dir.display()
            ));
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
                "Renamed {} to {}{}.\n",
                paint(BOLD, from),
                paint(BOLD, to),
                bracketed(&email)
            ),
        )
    })
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
    let pitboard = Pitboard::new(Context::from_env());
    let report = match cli.command.unwrap_or(Command::Status {
        offline: false,
        fresh: false,
    }) {
        Command::Status { offline, fresh } => status(&pitboard, offline, fresh),
        Command::Doctor => doctor(&pitboard),
        Command::Statusline => statusline(&pitboard),
        Command::Enroll {
            label,
            sign_in: true,
        } => enroll_signing_in(&pitboard, &label),
        Command::Enroll { label, .. } => {
            enrolled(&pitboard, &label, pitboard.enroll_current(&label))
        }
        Command::Use {
            label,
            signed_out: true,
        } => use_signed_out(&pitboard, &label),
        Command::Use { label, .. } => use_account(&pitboard, &label),
        Command::Forget { label, yes } => {
            // The way back is a browser sign-in for that account, which is the cost
            // Pitboard exists to spare people. Asked only where there is someone to ask:
            // a pipe, a script and --json go straight through.
            if !yes
                && !cli.json
                && std::io::stdin().is_terminal()
                && std::io::stderr().is_terminal()
            {
                eprint!(
                    "Forget {} and delete its parked login? \
                     Adding it again needs a browser sign-in. [y/N] ",
                    label
                );
                let _ = std::io::stderr().flush();
                let mut answer = String::new();
                let _ = std::io::stdin().read_line(&mut answer);
                if !matches!(answer.trim(), "y" | "Y" | "yes") {
                    return ExitCode::SUCCESS;
                }
            }
            forget(&pitboard, &label)
        }
        Command::Abandon => abandon(&pitboard),
        Command::Repair => repair(&pitboard),
        Command::Adopt => adopt(&pitboard),
        Command::Renew => renew(&pitboard),
        Command::Schedule { ref what } => schedule(&pitboard, what),
        Command::Log { lines } => log(&pitboard, lines),
        Command::Uninstall { yes } => {
            if !yes
                && !cli.json
                && std::io::stdin().is_terminal()
                && std::io::stderr().is_terminal()
            {
                eprint!(
                    "Delete every parked login this Pitboard wrote, the daily renewal \
                     schedule and ~/.pitboard? The account you are signed in to stays signed \
                     in; the others need a browser sign-in again. [y/N] "
                );
                let _ = std::io::stderr().flush();
                let mut answer = String::new();
                let _ = std::io::stdin().read_line(&mut answer);
                if !matches!(answer.trim(), "y" | "Y" | "yes") {
                    return ExitCode::SUCCESS;
                }
            }
            uninstall(&pitboard)
        }
        Command::Rename { from, to } => rename(&pitboard, &from, &to),
        Command::Desktop {
            what: DesktopCommand::LiveUsage { what },
        } => live_usage(&pitboard, what, cli.json),
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

    /// Claude Desktop has no sign-in of its own to open, so a second account is added by
    /// signing the app out and enrolling whoever signs in next.
    #[test]
    fn claude_desktop_is_signed_out_and_enrolled_from_the_command_line() {
        let cli = Cli::try_parse_from(["pitboard", "use", "desktop", "--signed-out"]).unwrap();
        assert!(
            matches!(
                cli.command,
                Some(Command::Use { ref label, signed_out: true }) if label == "desktop"
            ),
            "the tool is what is signed out"
        );
        let cli = Cli::try_parse_from(["pitboard", "use", "desktop/work"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Use {
                signed_out: false,
                ..
            })
        ));
        let cli = Cli::try_parse_from(["pitboard", "enroll", "desktop/work"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Enroll { ref label, sign_in: false }) if label == "desktop/work"
        ));
    }

    #[test]
    fn live_usage_is_turned_on_off_and_read_by_name() {
        for (typed, expected) in [
            ("enable", LiveUsageCommand::Enable),
            ("disable", LiveUsageCommand::Disable),
            ("status", LiveUsageCommand::Status),
        ] {
            let cli = Cli::try_parse_from(["pitboard", "desktop", "live-usage", typed]).unwrap();
            match cli.command {
                Some(Command::Desktop {
                    what: DesktopCommand::LiveUsage { what },
                }) => assert_eq!(what, expected, "{typed}"),
                _ => panic!("{typed} parsed as something else"),
            }
        }
        // Turning it on is a choice, so it is never what a bare command does.
        assert!(Cli::try_parse_from(["pitboard", "desktop", "live-usage"]).is_err());
        assert!(Cli::try_parse_from(["pitboard", "desktop"]).is_err());
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
            // roff writes a hyphen escaped.
            r"pitboard desktop live\-usage enable",
            r"pitboard desktop live\-usage disable",
            r"pitboard desktop live\-usage status",
        ] {
            assert!(
                page.contains(&format!("\\fB{typed}\\fR")),
                "{typed} missing:\n{page}"
            );
        }
        for option in [
            r"\-\-offline",
            r"\-\-sign\-in",
            r"\-\-signed\-out",
            r"\-y",
            r"\-\-lines",
        ] {
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
            page.contains(".SH SYNOPSIS") && page.contains(".SH VERSION"),
            "{page}"
        );
    }
}
