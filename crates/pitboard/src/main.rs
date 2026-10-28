use anstream::{ColorChoice, eprint, eprintln, print, println};
use anstyle::{AnsiColor, Style};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use pitboard_core::context::Context;
use pitboard_core::doctor;
use pitboard_core::error::Error;
use pitboard_core::host::{Os, Vault};
use pitboard_core::provider::{Adoption, SignsIn};
use pitboard_core::schedule::{Job, Location};
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
mod vault;
mod warm;
mod watch;

/// Bumped only when a field changes shape. Adding a field or an error code is not a
/// breaking change for a consumer; renaming or removing one is.
const CONTRACT: u32 = 1;

const ERROR: Style = AnsiColor::Red.on_default().bold();
const WARNING: Style = AnsiColor::Yellow.on_default().bold();

#[derive(Parser)]
#[command(
    name = "pitboard",
    bin_name = "pitboard",
    version,
    about = ABOUT,
    max_term_width = 100,
    after_help = examples()
)]
struct Cli {
    /// Print JSON instead of text
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

// Pitboard switches Claude Desktop on macOS alone, so no other system's help names it.
const ABOUT: &str = match pitboard_core::host::OS {
    Os::MacOs => {
        "Switch Claude Code, Codex and Claude Desktop between your own accounts without signing \
         out, and see what each Claude Code and Codex account has left"
    }
    Os::Linux | Os::Windows => {
        "Switch Claude Code and Codex between your own accounts without signing out, and see \
         what each account has left"
    }
};

// Styled as clap styles `Commands:` and the commands under it.
fn examples() -> String {
    let styles = clap::builder::Styles::styled();
    let (heading, literal) = (styles.get_header(), styles.get_literal());
    let mut said = format!("{heading}Examples:{heading:#}");
    for (typed, does) in [
        (
            "pitboard enroll personal",
            "Add the account Claude Code is signed in to",
        ),
        (
            "pitboard enroll work --sign-in",
            "Sign in to another account and add it",
        ),
        ("pitboard use work", "Switch Claude Code to work"),
    ] {
        said.push_str(&format!("\n  {literal}{typed:<30}{literal:#}  {does}"));
    }
    said
}

const LABEL_TO_ENROLL: &str = match pitboard_core::host::OS {
    Os::MacOs => {
        "A short name for this account, such as `personal` or `work`. `codex/work` names a Codex \
         account, `desktop/work` a Claude Desktop one; a bare name means Claude Code"
    }
    Os::Linux | Os::Windows => {
        "A short name for this account, such as `personal` or `work`. `codex/work` names a Codex \
         account; a bare name means Claude Code"
    }
};

const LABEL_TO_USE: &str = match pitboard_core::host::OS {
    Os::MacOs => {
        "The label the account was enrolled under, such as `work`, `codex/work` or `desktop/work`"
    }
    Os::Linux | Os::Windows => {
        "The label the account was enrolled under, such as `work` or `codex/work`"
    }
};

// Claude Code keeps its login in the file alone on Linux, so nothing is left behind there.
const STOW: &str = match pitboard_core::host::OS {
    Os::MacOs => {
        "Put away the login Claude Code left in a file behind the keychain, then delete the file"
    }
    Os::Linux => {
        "Put away the login Claude Code left in a file behind the store it keeps its login in, \
         then delete the file"
    }
    Os::Windows => {
        "Put away the login Claude Code left in a file behind Credential Manager, then delete \
         the file"
    }
};

// One way of keeping them on macOS and Windows, two on Linux.
const VAULT: &str = match pitboard_core::host::OS {
    Os::MacOs => "Where parked logins are kept: in the login keychain",
    Os::Linux => "Where parked logins are kept: in the login keyring, or in 0600 files",
    Os::Windows => "Where parked logins are kept: in files sealed to your Windows account",
};

#[derive(Subcommand)]
enum Command {
    /// What is signed in, and how much each Claude Code and Codex account has left (the default)
    Status {
        /// Answer from what was last measured, without asking anyone
        #[arg(long)]
        offline: bool,
        /// Ask about every account, even one asked about moments ago
        #[arg(long, conflicts_with = "offline")]
        fresh: bool,
    },
    /// Switch a tool to an enrolled account
    Use {
        #[arg(help = LABEL_TO_USE)]
        label: String,
        /// Where the service refuses the login in use for good, put this account's login in
        /// its place without asking; needed without a terminal or with `--json`
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Add an account: the one signed in now, or with --sign-in, another one
    Enroll {
        #[arg(value_parser = label_to_enroll, help = LABEL_TO_ENROLL)]
        label: String,
        /// Sign in to another account first, through the tool's own sign-in
        #[arg(long)]
        sign_in: bool,
    },
    /// Change the label an account is enrolled under
    Rename {
        /// The label it has now
        from: String,
        /// The label it should have
        #[arg(value_parser = new_label)]
        to: String,
    },
    /// Drop an account and its parked login
    Forget {
        /// The label to drop
        label: String,
        /// Do not ask first; needed without a terminal or with `--json`
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Switch Claude Code before the account in use runs out, and warm up accounts
    #[command(after_help = watch::terms())]
    Watch {
        #[command(flatten)]
        share: watch::Share,
        /// Decide once whether to switch, from the usage Pitboard last measured, asking no
        /// service for usage, and stop. Warms nothing up
        #[arg(long)]
        once: bool,
        // Given by `--scheduled`, as `renew`'s is.
        #[arg(skip)]
        scheduled: bool,
        #[command(subcommand)]
        what: Option<watch::WatchCommand>,
    },
    /// Start an account's five-hour period early, so it resets sooner
    Warm {
        #[command(subcommand)]
        what: Option<warm::WarmCommand>,
    },
    /// One line for Claude Code's status bar; reads its session JSON on stdin
    Statusline,
    /// Renew every parked login that is due, and nothing else
    Renew {
        // Given by `--scheduled`, which only the parser knows (`parser`).
        #[arg(skip)]
        scheduled: bool,
    },
    /// Keep parked logins alive without running anything yourself
    Schedule {
        #[command(subcommand)]
        what: ScheduleCommand,
    },
    #[command(about = VAULT)]
    Vault {
        #[command(subcommand)]
        what: Option<vault::VaultCommand>,
    },
    /// What Pitboard has changed, and when
    Log {
        /// How many changes to show
        #[arg(short = 'n', long, default_value_t = 20)]
        lines: usize,
    },
    /// Check that what Pitboard relies on still holds on this machine
    Doctor,
    /// Delete every parked login this Pitboard wrote, the daily renewal schedule, the watch
    /// service and Pitboard's own files
    Uninstall {
        /// Do not ask first; needed without a terminal or with `--json`
        #[arg(short = 'y', long)]
        yes: bool,
    },
    #[command(about = STOW)]
    Stow {
        /// Do not ask first; needed without a terminal or with `--json`
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Ask the credential store what parked logins are here, and account for every one
    Repair,
    /// Give up on an interrupted switch that cannot be finished, keeping every login
    Abandon,
    /// Take over a Pitboard directory another computer wrote, keeping the accounts
    Adopt,
    /// Print a shell completion script
    Completions { shell: clap_complete::Shell },
    // Only the parser knows `manpage` (`parser`).
    #[command(skip)]
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
            Command::Vault { .. } => "vault",
            Command::Log { .. } => "log",
            Command::Uninstall { .. } => "uninstall",
            Command::Rename { .. } => "rename",
            Command::Doctor => "doctor",
            Command::Statusline => "statusline",
            Command::Watch { .. } => "watch",
            Command::Warm { .. } => "warm",
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

    // `doctor` says so in a check, and `renew`'s gate refuses for the homes it renews in.
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

// A new name or an existing one, which a label written by 0.1.x may hold a slash in: the core
// tells which against the state file.
fn label_to_enroll(text: &str) -> Result<String, String> {
    use pitboard_core::label::{self, Unusable};
    match label::check(text) {
        Ok(()) | Err(Unusable::Separator) => Ok(text.to_string()),
        Err(Unusable::Empty | Unusable::NotOneWord) => Err(label::ONE_WORD.into()),
    }
}

fn new_label(text: &str) -> Result<String, String> {
    pitboard_core::label::choose(text)?;
    Ok(text.to_string())
}

/// What a command produced, before it is rendered for a person or a program.
struct Report {
    command: Option<&'static str>,
    result: Result<Value, Error>,
    warnings: Vec<Value>,
    // what a refused change had done before it refused, said ahead of the refusal
    said_first: Vec<Value>,
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
            said_first: Vec::new(),
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
            said_first: Vec::new(),
            human: String::new(),
            failure: None,
        }
    }

    /// A change that failed, with what it found on the way.
    fn refused(command: &'static str, failed: Failed) -> Report {
        let (first, rest) = failed
            .warnings
            .split_at(failed.said_first.min(failed.warnings.len()));
        Report {
            warnings: warnings(rest),
            said_first: warnings(first),
            ..Report::failed(Some(command), failed.error)
        }
    }
}

fn emit(report: &Report, as_json: bool) -> ExitCode {
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
        let said: Vec<&Value> = report.said_first.iter().chain(&report.warnings).collect();
        let envelope = json!({
            "v": CONTRACT,
            "command": report.command,
            "ok": report.result.is_ok() && report.exit == 0,
            "data": data,
            "warnings": said,
            "error": error,
        });
        println!("{}", json::ascii(&envelope));
    } else {
        let warn = |w: &Value| {
            eprintln!(
                "{} {}",
                paint(WARNING, "warning:"),
                w["message"].as_str().unwrap_or_default()
            );
        };
        // A refusal's "Nothing has changed" is about itself, so what came before is said first.
        report.said_first.iter().for_each(warn);
        match &report.result {
            Ok(_) => print!("{}", report.human),
            Err(e) => eprintln!("{} {e}", paint(ERROR, "error:")),
        }
        report.warnings.iter().for_each(warn);
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
            diagnosis.report(),
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
    let signed_in = match tool.signs_in() {
        SignsIn::ByItsProgram => {
            if let Err(refused) = pitboard.ready_to_sign_in(label) {
                return Report::refused("enroll", refused);
            }
            eprintln!(
                "Opening {}'s sign-in. Sign in as {who}; the account in use now stays signed in.",
                tool.name()
            );
            if let Some(running) = pitboard.running_before_sign_in(label) {
                eprint!("{running}");
            }
            pitboard.sign_in(label)
        }
        // Each line as the wait says it, until the app has quit signed in.
        SignsIn::InItsApp => pitboard.sign_in_watched(label).and_then(|watched| {
            let said = watched.said();
            while let Some(line) = said.next() {
                eprint!("{line}");
            }
            watched.finish().map_err(Failed::refused)
        }),
    };
    match signed_in {
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
        let human =
            pitboard_core::words::enrolled_lines(provider, &enrolled, &name, &to_use, &another);
        let (kind, email) = match enrolled {
            Enrolled::Current { email } => ("current", email),
            Enrolled::SignedIn { email } => ("signed_in", email),
            Enrolled::Renewed { email } => ("renewed", email),
            Enrolled::InUse { email, .. } => ("in_use", email),
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

/// `use`, and where the login in use is one its service refuses for good, what `confirm` says
/// to putting `label`'s login in its place. `None` where the answer was no.
fn use_account(pitboard: &Pitboard, label: &str, confirm: Confirm) -> Option<Report> {
    let switched = match pitboard.switch_to(label) {
        Err(failed) => match (&failed.error, confirm) {
            (Error::LiveLoginRefused { .. }, Confirm::Go) => pitboard.replace_with(label),
            (
                Error::LiveLoginRefused {
                    tool,
                    label: typed,
                    detail,
                },
                Confirm::Ask,
            ) => {
                let question = pitboard_core::words::replace_question(*tool, typed, detail);
                if !agreed(&question) {
                    return None;
                }
                pitboard.replace_with(label)
            }
            _ => Err(failed),
        },
        switched => switched,
    };
    Some(switched_report(switched))
}

fn switched_report(switched: Changing<Outcome>) -> Report {
    changed("use", switched, |outcome| match outcome {
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
        Outcome::Installed {
            provider,
            to,
            adoption,
        } => {
            let (seconds, follows, adoption_json) = followed(provider, &adoption);
            (
                json!({
                    "from": null,
                    "to": to,
                    "provider": provider,
                    "changed": true,
                    "parked_at": null,
                    "adoption_ceiling_seconds": seconds,
                    "adoption": adoption_json,
                }),
                format!(
                    "Switched {} to {}.\n{follows}",
                    provider.name(),
                    paint(BOLD, &to)
                ),
            )
        }
    })
}

// A number of seconds only for a tool whose sessions follow by themselves, never a zero that
// reads as "at once".
fn followed(
    provider: pitboard_core::provider::ProviderId,
    adoption: &Adoption,
) -> (Option<u32>, String, Value) {
    let said =
        pitboard_core::words::after_a_switch(adoption, &format!("{} sessions", provider.name()));
    match adoption {
        Adoption::PollingWithin(seconds) => (
            Some(*seconds),
            format!("{said}.\n"),
            json!({ "follows": "polling", "within_seconds": seconds }),
        ),
        Adoption::AtRenewal { file } => (
            None,
            format!("{said}. `pitboard stow` puts it away.\n"),
            json!({ "follows": "renewal", "path": file }),
        ),
        Adoption::RestartRequired { program, .. } => (
            None,
            format!("{said}. It will not pick the switch up on its own.\n"),
            json!({ "follows": "restart", "program": program }),
        ),
        Adoption::NextOpen { app } => (
            None,
            format!("{said}.\n"),
            json!({ "follows": "open", "app": app }),
        ),
    }
}

fn forget(pitboard: &Pitboard, label: &str) -> Report {
    changed("forget", pitboard.forget(label), |email| {
        (
            json!({ "label": label, "email": email }),
            format!(
                "Forgot {}.\n",
                pitboard_core::words::with_email(&paint(BOLD, label), &email)
            ),
        )
    })
}

/// What `pitboard stow` finds, asks and does: the login left in the file behind Claude Code's
/// store and what putting it away would do with it, then, once `confirm` says yes, putting it
/// away while the file still holds what was found. A login of an account nobody enrolled is
/// refused as the look found it, saying whose it is, and nothing is asked or changed. `None`
/// where the answer was no; where nobody could be asked, `confirm`'s refusal.
fn stow(pitboard: &Pitboard, confirm: impl FnOnce(&str) -> Result<bool, Error>) -> Option<Report> {
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
    match confirm(&left_question(&left)) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(refused) => return Some(Report::failed(Some("stow"), refused)),
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

fn left_question(left: &pitboard_core::switch::Left) -> String {
    let lines = pitboard_core::words::left_lines(left);
    format!("{}\nPut it away? [y/N] ", lines.join("\n"))
}

fn stowed_lines(stowed: &pitboard_core::switch::Stowed) -> String {
    pitboard_core::words::stowed_lines(stowed)
        .into_iter()
        .map(|line| format!("{line}\n"))
        .collect()
}

fn abandon(pitboard: &Pitboard) -> Report {
    let gave_up = match pitboard.abandon_recovery() {
        Ok(gave_up) => gave_up,
        Err(error) => return Report::failed(Some("abandon"), error),
    };
    let mut human = String::new();
    let mut data = json!({ "abandoned": gave_up.switch.is_some() });
    if let Some(a) = &gave_up.switch {
        data["from"] = json!(a.from);
        data["to"] = json!(a.to);
        data["logins_kept"] = json!(a.kept);
        let from = if a.from.is_empty() {
            String::new()
        } else {
            format!(" from {}", paint(BOLD, &a.from))
        };
        human.push_str(&format!(
            "Gave up on the interrupted switch{from} to {}. {} login(s) kept; nothing was \
             deleted. Run `pitboard` to see who is signed in.\n",
            paint(BOLD, &a.to),
            a.kept
        ));
    }
    for given_up in &gave_up.moves {
        human.push_str(&format!("{given_up}\n"));
    }
    if gave_up.switch.is_none() && gave_up.moves.is_empty() && gave_up.warnings.is_empty() {
        human.push_str("There is no interrupted switch to give up on.\n");
    }
    data["moves_given_up"] = gave_up
        .moves
        .iter()
        .map(|given_up| {
            json!({
                "provider": given_up.tool,
                "label": pitboard_core::state::Key::new(given_up.tool, given_up.label.clone())
                    .typed(),
                "parked": given_up.parked,
                "set_aside": given_up.set_aside,
                "left": given_up.left,
                "slot": given_up.slot,
            })
        })
        .collect();
    Report {
        warnings: warnings(&gave_up.warnings),
        ..Report::done("abandon", data, human)
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

// Where it is, under the key that says what it is: `path` for a file, `task` for Task Scheduler.
pub(crate) fn located(installed: Value, location: &Location, rest: Value) -> Value {
    let mut said = serde_json::Map::new();
    said.insert("installed".into(), installed);
    match location {
        Location::File(path) => said.insert("path".into(), json!(path)),
        Location::Task(task) => said.insert("task".into(), json!(task)),
    };
    if let Value::Object(rest) = rest {
        said.extend(rest);
    }
    Value::Object(said)
}

fn schedule(pitboard: &Pitboard, what: &ScheduleCommand) -> Report {
    use pitboard_core::schedule::Installed;
    // From any home but the default one, the schedule is not this home's, and installing
    // it is refused until PITBOARD_HOME is unset.
    let renews_this_home = pitboard.schedule_renews_this_home();
    let default = pitboard.shown(&pitboard.default_directory());
    let renews_only = format!(
        "The renewal schedule renews only the parked logins in {default}, not those in the \
         directory PITBOARD_HOME names."
    );
    let say = |installed: &Installed| match installed {
        Installed::Yes {
            location,
            every_seconds,
        } => (
            located(
                json!(true),
                location,
                json!({"every_seconds": every_seconds}),
            ),
            format!(
                "Parked logins are renewed every {} by this computer's own scheduler.\n{}\n{}",
                pitboard_core::words::span(i64::from(*every_seconds)),
                location,
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
                        "{renews_only} To turn it on for {default}, unset PITBOARD_HOME, \
                         then run `pitboard schedule install`."
                    )
                }
            ),
        ),
        Installed::Unsupported => (
            json!({"installed": false, "supported": false}),
            format!(
                "{}\n",
                pitboard_core::words::no_scheduler(pitboard_core::host::OS, Job::Renew)
            ),
        ),
    };
    match what {
        ScheduleCommand::Status => {
            let (data, human) = say(&pitboard.schedule());
            Report::done("schedule", data, human)
        }
        ScheduleCommand::Install => match pitboard.schedule_install() {
            Err(error) => Report::failed(Some("schedule"), error),
            Ok(location) => Report::done(
                "schedule",
                located(json!(true), &location, json!({})),
                format!(
                    "Parked logins will be renewed daily.\n{location}\n\nIt renews your own \
                     parked logins and does nothing else: it never switches account and \
                     never asks Anthropic for usage.\n"
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
                .chain(&a.set_aside)
                .map(|label| format!("  pitboard enroll {label} --sign-in\n"))
                .collect();
            let set_aside = if a.set_aside.is_empty() {
                String::new()
            } else {
                format!("{}\n", pitboard_core::words::adopted_folders(&a.set_aside))
            };
            Report::done(
                "adopt",
                json!({
                    "adopted": true,
                    "accounts": a.accounts,
                    "logins_dropped": a.logins_dropped,
                    "set_aside": a.set_aside,
                }),
                format!(
                    "Took over this directory: {} account(s) kept.\n\
                     {} parked login(s) dropped, because a login belongs to the computer \
                     that signed in.\n{set_aside}{}",
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
        let place = pitboard.vault().ok().and_then(|kept| kept.place);
        let human = repaired(place, &r, &pitboard.parks_folder());
        (
            json!({
                "given_back": r.given_back.iter().map(|(label, service)| json!({
                    "label": label,
                    "service": service,
                })).collect::<Vec<_>>(),
                "deleted": r.deleted,
                "strangers": r.strangers,
                "unreadable": r.unreadable,
                "set_aside": r.set_aside,
            }),
            human,
        )
    })
}

// A park nobody can account for may be another Pitboard's in a vault every home shares, and a
// vault of files is this Pitboard directory's alone.
fn repaired(
    vault: Option<Vault>,
    r: &pitboard_core::switch::Reclaimed,
    parks: &std::path::Path,
) -> String {
    if r.is_empty() {
        return "Every parked login here is accounted for.\n".to_string();
    }
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
        let why = match vault {
            Some(Vault::Keychain) => {
                "the keychain is shared by the whole machine, and they may be another Pitboard's"
            }
            Some(Vault::SecretService) => {
                "the login keyring is shared by every Pitboard directory of your session, and \
                 they may be another Pitboard's"
            }
            Some(Vault::Files | Vault::SealedFiles) | None => {
                "Pitboard deletes only a parked login it wrote down"
            }
        };
        said.push_str(&format!(
            "{} parked login(s) here belong to no account Pitboard knows and were \
             not written down by this one. Left alone: {why}.\n",
            r.strangers.len()
        ));
    }
    if !r.unreadable.is_empty() {
        let next = match vault {
            Some(Vault::Keychain) => " Unlock the keychain and run this again.",
            Some(Vault::SecretService) => {
                " Run this again in your desktop session, where signing in unlocks the login \
                 keyring."
            }
            // A sealed park does not open in a logon without this account's key, as over SSH.
            Some(Vault::SealedFiles) => " Run this again from a desktop session.",
            Some(Vault::Files) | None => "",
        };
        said.push_str(&format!(
            "{} could not be read this time and were left alone.{next}\n",
            r.unreadable.len()
        ));
    }
    if !r.set_aside.is_empty() {
        said.push_str(&format!(
            "Set aside {} folder(s) in {} that hold what no account here can take. \
             Nothing was deleted; `pitboard doctor` lists them.\n",
            r.set_aside.len(),
            parks.display()
        ));
    }
    said
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

fn uninstall_question(pitboard: &Pitboard) -> String {
    format!(
        "Delete every parked login this Pitboard wrote, the daily renewal schedule, the watch \
         service and {}? Each tool stays signed in to the account in use; the others need \
         signing in again. [y/N] ",
        pitboard.shown(&pitboard.directory())
    )
}

fn uninstall(pitboard: &Pitboard, os: Os) -> Report {
    let folder = pitboard.shown(&pitboard.directory());
    changed("uninstall", pitboard.uninstall(), |removed| {
        let mut human = format!(
            "Removed {} parked login(s). The login each tool is signed in with is untouched.\n",
            removed.parks
        );
        if removed.set_aside > 0 {
            human.push_str(&format!(
                "Deleted {} folder(s) set aside.\n",
                removed.set_aside
            ));
        }
        if removed.watch_service_removed {
            human.push_str("Stopped the watch service.\n");
        }
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
                "{} could not be deleted yet, so {folder} was kept; run `pitboard uninstall` \
                 again.\n",
                removed.pending
            ));
        } else if removed.warming_up {
            human.push_str(&format!(
                "Another Pitboard is warming up an account, in {}, so {folder} was kept; run \
                 `pitboard uninstall` again once it is done.\n",
                warm::warmers(os)
            ));
        } else if !removed.home_kept_by.is_empty() {
            let held: Vec<String> = removed
                .home_kept_by
                .iter()
                .map(|path| pitboard.shown(path))
                .collect();
            human.push_str(&format!(
                "{} could not be removed, so {folder} was kept; run `pitboard uninstall` again \
                 once no program has it open.\n",
                held.join(", ")
            ));
        } else if removed.home_removed {
            human.push_str(&format!(
                "{folder} is gone. Remove Pitboard itself the way you installed it.\n"
            ));
        }
        let kept_by: Vec<String> = removed
            .home_kept_by
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        (
            json!({
                "parks_removed": removed.parks,
                "parks_pending": removed.pending,
                "parks_left": removed.left,
                "home_removed": removed.home_removed,
                "home_kept_by": kept_by,
                "warming_up": removed.warming_up,
                "schedule_removed": removed.schedule_removed,
                "watch_service_removed": removed.watch_service_removed,
                "set_aside_removed": removed.set_aside,
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
                "Renamed {} to {}.\n",
                paint(BOLD, from),
                pitboard_core::words::with_email(&paint(BOLD, to), &email)
            ),
        )
    })
}

/// What a command that deletes something does before it acts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Confirm {
    /// `--yes`, or a run that may change nothing, as root or under sudo, whose answer would
    /// only be refused.
    Go,
    /// Standard input and standard error are a terminal, and the output is for a person.
    Ask,
    /// Nobody to ask: a pipe, a script, a tool, or `--json`, which is no `--yes`.
    Refuse,
}

impl Confirm {
    fn of(pitboard: &Pitboard, yes: bool, json: bool, terminal: bool) -> Confirm {
        if yes || pitboard.permit().is_err() {
            Confirm::Go
        } else if terminal && !json {
            Confirm::Ask
        } else {
            Confirm::Refuse
        }
    }

    /// Whether `command` goes ahead: `false` where the person said no.
    fn ask(self, command: &'static str, question: &str) -> Result<bool, Error> {
        match self {
            Confirm::Go => Ok(true),
            Confirm::Ask => Ok(agreed(question)),
            Confirm::Refuse => Err(Error::ConfirmationNeeded { command }),
        }
    }
}

fn terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Asks `question` on standard error: yes only for `y`, `Y` or `yes`.
fn agreed(question: &str) -> bool {
    eprint!("{question}");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    matches!(answer.trim(), "y" | "Y" | "yes")
}

// The command line with what completions leave out, since clap_complete writes even a hidden
// argument or command into them: `--scheduled`, each job's marker, and `manpage`.
fn parser() -> clap::Command {
    let marker = clap::Arg::new("scheduled")
        .long("scheduled")
        .action(clap::ArgAction::SetTrue)
        .hide(true);
    Cli::command()
        .mut_subcommands(|sub| match sub.get_name() {
            "renew" => sub.arg(marker.clone()),
            "watch" => sub.arg(marker.clone().conflicts_with("once")),
            _ => sub,
        })
        .subcommand(
            clap::Command::new("manpage")
                .about("Print the man page")
                .hide(true),
        )
}

fn command_line<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut matches = parser().try_get_matches_from(args)?;
    // A run's own options beside `watch`'s subcommand, which clap's conflict rule would refuse
    // the global `--json` beside too.
    if let Some(("watch", watch)) = matches.subcommand()
        && let Some((what, _)) = watch.subcommand()
        && let Some(given) = ["at", "once", "scheduled"]
            .into_iter()
            .find(|id| watch.value_source(id) == Some(clap::parser::ValueSource::CommandLine))
    {
        let mut cli = parser();
        cli.build();
        let message = format!("the subcommand '{what}' cannot be used with '--{given}'");
        let kind = clap::error::ErrorKind::ArgumentConflict;
        return Err(match cli.find_subcommand_mut("watch") {
            Some(watch) => watch.error(kind, message),
            None => cli.error(kind, message),
        });
    }
    let marked = matches
        .subcommand()
        .is_some_and(|(_, given)| matches!(given.try_get_one::<bool>("scheduled"), Ok(Some(true))));
    let manpage = matches.subcommand_name() == Some("manpage");
    let mut cli =
        Cli::from_arg_matches_mut(&mut matches).map_err(|e| e.format(&mut Cli::command()))?;
    if let Some(Command::Renew { scheduled } | Command::Watch { scheduled, .. }) = &mut cli.command
    {
        *scheduled = marked;
    }
    if manpage {
        cli.command = Some(Command::Manpage);
    }
    Ok(cli)
}

/// A usage error keeps clap's own rendering, unless the caller asked for JSON, which is
/// promised for every outcome.
fn parse() -> Result<Cli, ExitCode> {
    command_line(std::env::args_os()).map_err(|e| {
        let wants_json = std::env::args_os().any(|arg| arg == "--json");
        if wants_json && e.use_stderr() {
            let message = e.render().to_string().trim().to_string();
            emit(&Report::failed(None, Error::Usage(message)), true)
        } else {
            let _ = e.print();
            ExitCode::from(e.exit_code().clamp(0, 255) as u8)
        }
    })
}

fn main() -> ExitCode {
    pitboard_core::release::set_version(env!("CARGO_PKG_VERSION"));
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
        return emit(&Report::failed(Some(command.name()), refused), cli.json);
    }
    let ctx = Context::from_env();
    // A job's run says so by its marker, rather than by what the system passes it.
    let ctx = match command {
        Command::Renew { scheduled: true } => ctx.started_by_the_schedule(),
        Command::Watch {
            scheduled: true, ..
        } => ctx.started_by_the_watch_service(),
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
        return emit(&Report::failed(Some(command.name()), refused), cli.json);
    }
    let report = match command {
        Command::Status { offline, fresh } => status(&pitboard, offline, fresh),
        Command::Doctor => doctor(&pitboard),
        Command::Statusline => statusline(&pitboard),
        Command::Watch {
            what: Some(ref what),
            ..
        } => watch::service(&pitboard, what),
        Command::Watch {
            share,
            once: false,
            scheduled,
            ..
        } => return watch::run(&pitboard, share.threshold(), scheduled, cli.json),
        Command::Watch { share, .. } => watch::once(&pitboard, share.threshold()),
        Command::Enroll {
            label,
            sign_in: true,
        } => enroll_signing_in(&pitboard, &label),
        Command::Enroll { label, .. } => {
            enrolled(&pitboard, &label, pitboard.enroll_current(&label))
        }
        Command::Use { label, yes } => {
            let confirm = Confirm::of(&pitboard, yes, cli.json, terminal());
            match use_account(&pitboard, &label, confirm) {
                Some(report) => report,
                None => return ExitCode::SUCCESS,
            }
        }
        Command::Forget { label, yes } => {
            let confirm = match Confirm::of(&pitboard, yes, cli.json, terminal()) {
                // A label that names no account gets its own error. The core resolves the label
                // again, unlocked, so this relies on nobody enrolling it in between.
                Confirm::Refuse if pitboard.account(&label).is_none() => Confirm::Go,
                confirm => confirm,
            };
            // The way back is a sign-in for that account, which is the cost Pitboard exists to
            // spare people.
            let question = pitboard_core::words::forget_question(
                pitboard.account(&label).map(|a| a.provider()),
                &label,
            );
            match confirm.ask("forget", &question) {
                Ok(true) => forget(&pitboard, &label),
                Ok(false) => return ExitCode::SUCCESS,
                Err(refused) => Report::failed(Some("forget"), refused),
            }
        }
        Command::Stow { yes } => {
            // A session signing in with the file is signed out once it is gone.
            let confirm = Confirm::of(&pitboard, yes, cli.json, terminal());
            match stow(&pitboard, |question| confirm.ask("stow", question)) {
                Some(report) => report,
                None => return ExitCode::SUCCESS,
            }
        }
        Command::Abandon => abandon(&pitboard),
        Command::Repair => repair(&pitboard),
        Command::Adopt => adopt(&pitboard),
        Command::Renew { .. } => renew(&pitboard),
        Command::Schedule { ref what } => schedule(&pitboard, what),
        Command::Vault { ref what } => vault::run(&pitboard, what.as_ref()),
        Command::Log { lines } => log(&pitboard, lines),
        Command::Uninstall { yes } => {
            let question = uninstall_question(&pitboard);
            match Confirm::of(&pitboard, yes, cli.json, terminal()).ask("uninstall", &question) {
                Ok(true) => uninstall(&pitboard, pitboard_core::host::OS),
                Ok(false) => return ExitCode::SUCCESS,
                Err(refused) => Report::failed(Some("uninstall"), refused),
            }
        }
        Command::Rename { from, to } => rename(&pitboard, &from, &to),
        Command::Warm { ref what } => warm::run(&pitboard, what.as_ref()),
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
    emit(&report, cli.json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_definition_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    // clap names the program after how it was started, `pitboard.exe` on Windows.
    #[test]
    fn a_usage_error_names_pitboard_however_it_was_started() {
        for started in [
            "pitboard",
            r"C:\Tools\pitboard.exe",
            "/usr/local/bin/pitboard",
        ] {
            let refused = Cli::try_parse_from([started, "use"])
                .err()
                .expect("a usage error")
                .render()
                .to_string();
            assert!(
                refused.contains("Usage: pitboard use <LABEL>"),
                "{started}: {refused}"
            );
            assert!(!refused.contains("pitboard.exe"), "{started}: {refused}");
        }
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
    fn what_a_refused_change_did_first_is_said_ahead_of_it() {
        use pitboard_core::provider::ProviderId;
        use pitboard_core::switch::{Ended, MoveSettled};
        let tool = ProviderId::Desktop;
        let failed = Failed {
            error: Error::Usage("refused".into()),
            warnings: vec![
                Warning::MoveSettled(MoveSettled {
                    tool,
                    label: "home".into(),
                    half: pitboard_core::state::Half::Arrive,
                    ended: Ended::Finished,
                }),
                Warning::SetAside {
                    tool,
                    path: "/Users/dana/.pitboard/parks/pitboard-aside-1".into(),
                    label: Some("desktop/home".into()),
                    why: pitboard_core::state::SetAside::Superseded,
                },
                Warning::LooksSignedOut {
                    tool,
                    label: "desktop/work".into(),
                },
            ],
            said_first: 2,
        };
        let codes = |said: &[Value]| {
            said.iter()
                .map(|w| w["code"].as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
        };
        let refused = Report::refused("use", failed);
        assert_eq!(
            codes(&refused.said_first),
            ["interrupted_move_finished", "set_aside"]
        );
        assert_eq!(codes(&refused.warnings), ["looks_signed_out"]);
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
             /Users/x/.claude/.credentials.json is there. `pitboard stow` puts it away.\n"
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
    fn stow_refuses_a_login_of_an_account_not_enrolled_as_the_look_found_it() {
        use pitboard_core::provider::Owner;
        use pitboard_core::testing::{FixedClock, ScratchMachine, live_service};
        use std::sync::Arc;
        const NOW: i64 = 1_760_000_000;
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let ScratchMachine {
            ctx,
            host: mem,
            api,
        } = ScratchMachine::at(&home, |ctx| {
            ctx.with_pitboard_home(home.join(".pitboard"))
                .with_clock(Arc::new(FixedClock::at(NOW)))
        });
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
        use pitboard_core::host::SecretStore;
        use pitboard_core::switch::{Foreseen, Kept, Left, Stowed};
        let path = std::path::PathBuf::from("/Users/x/.claude/.credentials.json");
        let parked = Kept::ParkedNow {
            label: "work".into(),
        };
        let left = Left {
            path: path.clone(),
            store: SecretStore::Keychain,
            seen: "0123456789abcdef".into(),
            login: Foreseen::Kept(parked),
            dropped: vec!["mcpOAuth".into()],
        };
        let question = left_question(&left);
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
        let on_windows = left_question(&Left {
            store: SecretStore::CredentialManager,
            ..left
        });
        assert!(
            on_windows.contains("Claude Code's document in Credential Manager keeps its own.")
                && on_windows.contains("such as one in a sign-in over SSH with a key, is signed"),
            "{on_windows}"
        );
        let said = stowed_lines(&Stowed {
            path,
            store: SecretStore::Keychain,
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

    // Keychain items the whole Mac shares, Secret Service items every home of a Linux session
    // shares, and files in the vault of this Pitboard directory, which nothing locks.
    #[test]
    fn repair_says_why_it_left_parked_logins_alone_in_the_words_of_its_vault() {
        let left = pitboard_core::switch::Reclaimed {
            strangers: vec!["pitboard-park-a-1".into()],
            unreadable: vec!["pitboard-park-b-1".into()],
            ..Default::default()
        };
        let said = |vault| repaired(Some(vault), &left, std::path::Path::new("/x/parks"));
        let stranger = "1 parked login(s) here belong to no account Pitboard knows and were not \
                        written down by this one. Left alone:";
        assert_eq!(
            said(Vault::Keychain),
            format!(
                "{stranger} the keychain is shared by the whole machine, and they may be another \
                 Pitboard's.\n\
                 1 could not be read this time and were left alone. Unlock the keychain and run \
                 this again.\n"
            )
        );
        assert_eq!(
            said(Vault::SecretService),
            format!(
                "{stranger} the login keyring is shared by every Pitboard directory of your \
                 session, and they may be another Pitboard's.\n\
                 1 could not be read this time and were left alone. Run this again in your \
                 desktop session, where signing in unlocks the login keyring.\n"
            )
        );
        assert_eq!(
            said(Vault::SealedFiles),
            format!(
                "{stranger} Pitboard deletes only a parked login it wrote down.\n\
                 1 could not be read this time and were left alone. Run this again from a \
                 desktop session.\n"
            )
        );
        assert_eq!(
            said(Vault::Files),
            format!(
                "{stranger} Pitboard deletes only a parked login it wrote down.\n\
                 1 could not be read this time and were left alone.\n"
            )
        );
    }

    /// A command that deletes something asks at a terminal, for a person. With nobody to ask,
    /// `--json` included, it needs `--yes`. As root or under sudo it goes on to that refusal.
    #[test]
    fn a_command_that_deletes_asks_at_a_terminal_and_otherwise_needs_yes() {
        use pitboard_core::host::Elevation;
        use pitboard_core::testing::MemoryHost;
        let mem = MemoryHost::new();
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let pitboard = Pitboard::new(
            Context::new(home.clone())
                .with_pitboard_home(home.join(".pitboard"))
                .with_memory_stores(std::sync::Arc::clone(&mem)),
        );
        for (yes, json, terminal, confirm) in [
            (false, false, true, Confirm::Ask),
            (false, true, true, Confirm::Refuse),
            (false, false, false, Confirm::Refuse),
            (false, true, false, Confirm::Refuse),
            (true, false, true, Confirm::Go),
            (true, true, false, Confirm::Go),
        ] {
            assert_eq!(
                Confirm::of(&pitboard, yes, json, terminal),
                confirm,
                "yes {yes}, json {json}, terminal {terminal}"
            );
        }
        let refused = Confirm::Refuse.ask("stow", "?").expect_err("refused");
        assert_eq!(
            (refused.code(), refused.exit_code()),
            ("confirmation_needed", 2)
        );
        assert_eq!(
            refused.to_string(),
            "`pitboard stow` asks before it deletes anything, and can ask only in a terminal \
             without `--json`. Nothing was changed. Run it in a terminal, or add `--yes`."
        );

        mem.runs_with(Elevation::Elevated { why: "under sudo" });
        assert_eq!(Confirm::of(&pitboard, false, true, false), Confirm::Go);
    }

    // Each said `~/.pitboard` whatever `PITBOARD_HOME` named.
    #[test]
    fn uninstall_names_the_folder_it_asks_about_and_deletes() {
        use pitboard_core::testing::MemoryHost;
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let elsewhere = home.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("a Pitboard folder");
        let pitboard = Pitboard::new(
            Context::new(home.clone())
                .with_pitboard_home(elsewhere.clone())
                .with_memory_stores(MemoryHost::new()),
        );
        let named = match pitboard_core::host::OS {
            Os::Windows => elsewhere.display().to_string(),
            Os::MacOs | Os::Linux => std::path::Path::new("~")
                .join("elsewhere")
                .display()
                .to_string(),
        };
        let asked = uninstall_question(&pitboard);
        assert!(
            asked.starts_with(&format!(
                "Delete every parked login this Pitboard wrote, the daily renewal schedule, the \
                 watch service and {named}? "
            )),
            "{asked}"
        );
        let said = uninstall(&pitboard, pitboard_core::host::OS).human;
        assert!(
            said.ends_with(&format!(
                "{named} is gone. Remove Pitboard itself the way you installed it.\n"
            )),
            "{said}"
        );
        assert!(!elsewhere.exists());
    }

    // A warm-up's tool runs in a home inside Pitboard's directory while it holds `warm.lock`.
    #[test]
    fn uninstall_keeps_the_folder_a_warm_up_runs_in_and_says_so() {
        use pitboard_core::testing::MemoryHost;
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let folder = home.join(".pitboard");
        std::fs::create_dir_all(&folder).expect("a Pitboard folder");
        let pitboard = Pitboard::new(
            Context::new(home.clone())
                .with_pitboard_home(folder.clone())
                .with_memory_stores(MemoryHost::new()),
        );
        let running = std::fs::File::create(folder.join("warm.lock")).expect("a lock");
        running.lock().expect("held");
        for (os, warmers) in [
            (Os::MacOs, "in the app or `pitboard watch`"),
            (Os::Windows, "in the app or `pitboard watch`"),
            (Os::Linux, "in `pitboard watch`"),
        ] {
            let said = uninstall(&pitboard, os);
            assert!(
                said.human.ends_with(&format!(
                    "Another Pitboard is warming up an account, {warmers}, so {} was kept; run \
                     `pitboard uninstall` again once it is done.\n",
                    pitboard.shown(&folder)
                )),
                "{os:?}: {}",
                said.human
            );
        }
        assert!(folder.join("warm.lock").exists());
        drop(running);
        let said = uninstall(&pitboard, pitboard_core::host::OS);
        assert!(said.human.contains("is gone"), "{}", said.human);
        assert!(!folder.exists());
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

    // What each job runs, as the core writes it into the job, is a command line this one takes.
    #[test]
    fn each_jobs_marker_is_a_run_this_command_line_takes() {
        let run = |args: &[&str]| {
            command_line(std::iter::once("pitboard").chain(args.iter().copied()))
                .expect("parsed")
                .command
        };
        assert!(matches!(
            run(&pitboard_core::schedule::SCHEDULED_RUN),
            Some(Command::Renew { scheduled: true })
        ));
        assert!(matches!(
            run(&["renew"]),
            Some(Command::Renew { scheduled: false })
        ));
        let watched = [&pitboard_core::schedule::WATCH_RUN[..], &["--at", "90"]].concat();
        assert!(matches!(
            run(&watched),
            Some(Command::Watch {
                scheduled: true,
                once: false,
                what: None,
                ..
            })
        ));
        assert!(matches!(
            run(&["watch"]),
            Some(Command::Watch {
                scheduled: false,
                ..
            })
        ));
        assert!(
            command_line(["pitboard", "watch", "--scheduled", "--once"]).is_err(),
            "a run of the service watches until it is stopped"
        );
        assert!(command_line(["pitboard", "status", "--scheduled"]).is_err());
    }

    // `--json` goes anywhere; what only a run of `watch` takes is refused beside a subcommand,
    // which reads its own `--at`.
    #[test]
    fn watch_takes_json_anywhere_and_its_runs_options_only_without_a_subcommand() {
        for args in [
            &["watch", "--json", "install"][..],
            &["watch", "install", "--json"],
            &["--json", "watch", "status"],
            &["watch", "--json", "uninstall"],
        ] {
            let cli = command_line(std::iter::once("pitboard").chain(args.iter().copied()))
                .unwrap_or_else(|e| panic!("{args:?}: {e}"));
            assert!(cli.json, "{args:?}");
        }
        let cli = command_line(["pitboard", "watch", "install", "--at", "80"]).expect("parsed");
        let Some(Command::Watch {
            what: Some(watch::WatchCommand::Install { share }),
            ..
        }) = cli.command
        else {
            panic!("watch install");
        };
        assert_eq!(share.threshold().percent(), 80);
        for args in [
            &["watch", "--at", "80", "install"][..],
            &["watch", "--once", "status"],
            &["watch", "--scheduled", "uninstall"],
        ] {
            let refused = command_line(std::iter::once("pitboard").chain(args.iter().copied()))
                .err()
                .unwrap_or_else(|| panic!("{args:?} parsed"));
            assert_eq!(
                refused.kind(),
                clap::error::ErrorKind::ArgumentConflict,
                "{args:?}"
            );
        }
    }

    #[test]
    fn the_jobs_markers_are_in_no_help_and_manpage_in_no_completions() {
        use clap_complete::Shell;
        for shell in [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::Elvish,
            Shell::PowerShell,
        ] {
            let mut out = Vec::new();
            clap_complete::generate(shell, &mut Cli::command(), "pitboard", &mut out);
            let file = String::from_utf8(out).expect("text");
            assert!(!file.contains("scheduled"), "{shell}: {file}");
            assert!(!file.contains("manpage"), "{shell}: {file}");
        }
        for args in [
            &["manpage"][..],
            &["manpage", "--json"],
            &["--json", "manpage"],
        ] {
            let cli = command_line(std::iter::once("pitboard").chain(args.iter().copied()))
                .unwrap_or_else(|e| panic!("{args:?}: {e}"));
            assert!(matches!(cli.command, Some(Command::Manpage)), "{args:?}");
            assert_eq!(cli.json, args.contains(&"--json"), "{args:?}");
        }
        for job in ["renew", "watch"] {
            let mut cli = parser();
            let help = cli
                .find_subcommand_mut(job)
                .expect(job)
                .render_long_help()
                .to_string();
            assert!(!help.contains("scheduled"), "{help}");
        }
    }

    // The refusal to install says the same.
    #[test]
    fn with_no_scheduler_both_status_commands_say_why_in_the_cores_words() {
        use pitboard_core::testing::MemoryHost;
        let scratch = tempfile::tempdir().expect("a scratch home");
        let host = MemoryHost::new();
        host.without_a_scheduler();
        let pitboard =
            Pitboard::new(Context::new(scratch.path().to_path_buf()).with_memory_stores(host));
        let why = |job| {
            format!(
                "{}\n",
                pitboard_core::words::no_scheduler(pitboard_core::host::OS, job)
            )
        };
        assert_eq!(
            schedule(&pitboard, &ScheduleCommand::Status).human,
            why(Job::Renew)
        );
        assert_eq!(
            watch::service(&pitboard, &watch::WatchCommand::Status).human,
            why(Job::Watch)
        );
    }

    // A file under `path`, as before; Task Scheduler's task under `task`, with no `path`.
    #[test]
    fn the_schedule_says_where_it_is_by_what_it_is() {
        let plist = "/Users/x/Library/LaunchAgents/com.datlechin.pitboard.renew.plist";
        let file = located(
            json!(true),
            &Location::File(std::path::PathBuf::from(plist)),
            json!({"every_seconds": 86_400}),
        );
        assert_eq!(
            file.to_string(),
            format!(r#"{{"installed":true,"path":"{plist}","every_seconds":86400}}"#)
        );
        let task = located(
            json!(true),
            &Location::Task(r"\Pitboard\Renew S-1-5-21-1".into()),
            json!({}),
        );
        assert_eq!(
            task.to_string(),
            r#"{"installed":true,"task":"\\Pitboard\\Renew S-1-5-21-1"}"#
        );
    }

    /// `schedule status` run while `PITBOARD_HOME` names another directory says that the
    /// schedule renews only the parked logins in `~/.pitboard`. With nothing installed it
    /// says to unset `PITBOARD_HOME` before `pitboard schedule install`, which is refused
    /// until then, rather than to run it as it is. From the default home it says what it
    /// said. On a machine in memory, whose scheduler asks nobody.
    #[test]
    fn schedule_status_from_another_home_says_what_the_schedule_renews() {
        use pitboard_core::testing::MemoryHost;
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its folder")).expect("made");
        std::fs::write(&program, "").expect("a command line");
        let ctx = Context::new(home.clone())
            .with_schedule_program(program)
            .with_memory_stores(MemoryHost::new());
        let default = Pitboard::new(ctx.clone());
        let elsewhere = Pitboard::new(ctx.with_pitboard_home(home.join("elsewhere")));
        let said = |pitboard: &Pitboard| schedule(pitboard, &ScheduleCommand::Status).human;
        let renews_only = &format!(
            "renews only the parked logins in {}, not those in the directory PITBOARD_HOME names",
            default.shown(&default.default_directory())
        );

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
    }

    /// A build that may do nothing here, a Windows build of a release before Windows is
    /// released, answers only what prints a generated file, `completions` and `manpage`,
    /// besides `--version` and `--help`, which clap answers before a command is read. Every
    /// other command asks the release gate first, reading included.
    #[test]
    fn every_command_but_a_generated_file_asks_whether_this_build_may_run() {
        let asks = |args: &[&str]| {
            let cli = command_line(std::iter::once("pitboard").chain(args.iter().copied()))
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

    // The long help of every command a person can ask about, each under the way it is typed,
    // in a terminal as wide as Pitboard wraps it at.
    fn every_help() -> String {
        every_help_at(100)
    }

    fn every_help_at(columns: usize) -> String {
        fn each(command: &mut clap::Command, typed: &str, said: &mut String) {
            said.push_str(&format!("== {typed}\n{}\n", command.render_long_help()));
            let names: Vec<String> = command
                .get_subcommands()
                .filter(|sub| sub.get_name() != "help")
                .map(|sub| sub.get_name().to_string())
                .collect();
            for name in names {
                let sub = command.find_subcommand_mut(&name).expect("listed");
                each(sub, &format!("{typed} {name}"), said);
            }
        }
        let mut cli = parser().term_width(columns);
        cli.build();
        let mut said = String::new();
        each(&mut cli, "pitboard", &mut said);
        said
    }

    #[test]
    fn every_help_fits_an_80_column_terminal() {
        let every = every_help_at(80);
        let over: Vec<&str> = every
            .lines()
            .filter(|line| ui::columns(line) > 80)
            .collect();
        assert!(over.is_empty(), "{over:#?}");
    }

    #[test]
    // windows: macOS and Linux print this help; Windows' has the tests below
    #[cfg(target_os = "macos")]
    fn the_help_reads_as_it_did_on_macos() {
        insta::assert_snapshot!("help", every_help());
    }

    #[test]
    // windows: macOS and Linux print this help; Windows' has the tests below
    #[cfg(target_os = "linux")]
    fn the_help_reads_as_it_did_on_linux() {
        insta::assert_snapshot!("help_on_linux", every_help());
    }

    // Claude Code's login file on Windows sits behind Credential Manager.
    #[test]
    #[cfg(windows)]
    fn the_windows_help_promises_warm_ups_and_names_no_keychain() {
        let every = every_help();
        assert!(!every.contains("keychain"), "{every}");
        let read = every.split_whitespace().collect::<Vec<_>>().join(" ");
        for said in [
            "watch Switch Claude Code before the account in use runs out, and warm up accounts",
            "warm Start an account's five-hour period early, so it resets sooner",
            "now Start an account's five-hour period now, whatever the span, through one short \
             request Claude Code makes with that account's login",
            "stow Put away the login Claude Code left in a file behind Credential Manager, then \
             delete the file",
        ] {
            assert!(read.contains(said), "{said}: {every}");
        }
    }

    // Every command and argument as clap holds them once built, hidden ones too. The version
    // is left out: every release changes it.
    fn tree(command: &clap::Command) -> Value {
        let argument = |arg: &clap::Arg| {
            json!({
                "id": arg.get_id().as_str(),
                "long": arg.get_long(),
                "short": arg.get_short().map(String::from),
                "aliases": arg.get_all_aliases().unwrap_or_default(),
                "short_aliases": arg
                    .get_all_short_aliases()
                    .unwrap_or_default()
                    .into_iter()
                    .map(String::from)
                    .collect::<Vec<_>>(),
                "value_names": arg
                    .get_value_names()
                    .unwrap_or_default()
                    .iter()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>(),
                "num_args": arg.get_num_args().map(|range| range.to_string()),
                "action": format!("{:?}", arg.get_action()),
                "index": arg.get_index(),
                "required": arg.is_required_set(),
                "global": arg.is_global_set(),
                "hidden": arg.is_hide_set(),
                "possible_values": arg
                    .get_possible_values()
                    .iter()
                    .map(|value| value.get_name())
                    .collect::<Vec<_>>(),
                "default_values": arg
                    .get_default_values()
                    .iter()
                    .map(|value| value.to_string_lossy())
                    .collect::<Vec<_>>(),
                "conflicts_with": command
                    .get_arg_conflicts_with(arg)
                    .iter()
                    .map(|other| other.get_id().as_str())
                    .collect::<Vec<_>>(),
                "help": arg.get_help().map(ToString::to_string),
            })
        };
        json!({
            "name": command.get_name(),
            "aliases": command.get_all_aliases().collect::<Vec<_>>(),
            "hidden": command.is_hide_set(),
            "about": command.get_about().map(ToString::to_string),
            "arguments": command.get_arguments().map(argument).collect::<Vec<_>>(),
            "commands": command.get_subcommands().map(tree).collect::<Vec<_>>(),
        })
    }

    fn command_tree() -> Value {
        let mut cli = parser();
        cli.build();
        tree(&cli)
    }

    // What `main` prints for `args`, a help or a usage error, in a terminal 80 columns wide.
    fn said(args: &[&str]) -> String {
        let line = std::iter::once("pitboard").chain(args.iter().copied());
        let error = match parser().term_width(80).try_get_matches_from(line.clone()) {
            Ok(_) => command_line(line).err(),
            Err(error) => Some(error),
        };
        error
            .unwrap_or_else(|| panic!("{args:?} is a command line"))
            .render()
            .to_string()
    }

    fn shown(args: &[&str]) -> String {
        format!("$ pitboard {}\n{}", args.join(" "), said(args))
    }

    // Every command a person can ask about, by the words that name it.
    fn commands() -> Vec<Vec<String>> {
        fn each(command: &clap::Command, path: &mut Vec<String>, all: &mut Vec<Vec<String>>) {
            all.push(path.clone());
            for sub in command.get_subcommands() {
                if sub.get_name() != "help" {
                    path.push(sub.get_name().to_string());
                    each(sub, path, all);
                    path.pop();
                }
            }
        }
        let mut cli = parser();
        cli.build();
        let mut all = Vec::new();
        each(&cli, &mut Vec::new(), &mut all);
        all
    }

    fn typed<'a>(before: &[&'a str], path: &'a [String], after: &[&'a str]) -> Vec<&'a str> {
        let path = path.iter().map(String::as_str);
        before
            .iter()
            .copied()
            .chain(path)
            .chain(after.iter().copied())
            .collect()
    }

    fn every_help_typed() -> String {
        commands()
            .iter()
            .map(|path| shown(&typed(&[], path, &["--help"])))
            .collect()
    }

    const MISTYPED: &[&[&str]] = &[
        &["list"],
        &["lst"],
        &["stat"],
        &["doctr"],
        &["use"],
        &["--jsno"],
        &["status", "--verbose"],
        &["watch", "install", "--at", "40"],
        &["watch", "install", "--once"],
        &["completions", "foo"],
        &["use", "work", "extra"],
        &["log", "-n", "x"],
    ];

    fn every_usage_error() -> String {
        MISTYPED.iter().map(|args| shown(args)).collect()
    }

    // As a release writes each shell's script.
    fn completions(shell: clap_complete::Shell) -> String {
        let mut out = Vec::new();
        clap_complete::generate(shell, &mut Cli::command(), "pitboard", &mut out);
        String::from_utf8(out).expect("text")
    }

    #[test]
    fn every_help_and_usage_error_fits_80_columns() {
        let shown = every_help_typed() + &every_usage_error();
        let over: Vec<&str> = shown
            .lines()
            .filter(|line| ui::columns(line) > 80)
            .collect();
        assert!(over.is_empty(), "{over:#?}");
    }

    // No argument has a long help, so clap prints the one layout however help is asked for.
    #[test]
    fn dash_h_and_the_help_command_print_what_dash_dash_help_does() {
        for path in commands() {
            let help = said(&typed(&[], &path, &["--help"]));
            assert_eq!(said(&typed(&[], &path, &["-h"])), help, "{path:?}");
            assert_eq!(said(&typed(&["help"], &path, &[])), help, "{path:?}");
        }
    }

    // The bash script names no command's about, so Git Bash on Windows reads the same one.
    #[test]
    fn the_usage_errors_and_the_bash_completions_read_alike_on_every_system() {
        insta::assert_snapshot!("usage_errors", every_usage_error());
        insta::assert_snapshot!("completions_bash", completions(clap_complete::Shell::Bash));
    }

    #[test]
    // windows: macOS and Linux print this help, tree and these scripts; Windows' are the test below
    #[cfg(target_os = "macos")]
    fn the_command_tree_help_and_zsh_fish_and_elvish_completions_read_as_they_did_on_macos() {
        insta::assert_json_snapshot!("command_tree", command_tree());
        insta::assert_snapshot!("help_in_80_columns", every_help_typed());
        insta::assert_snapshot!("completions_zsh", completions(clap_complete::Shell::Zsh));
        insta::assert_snapshot!("completions_fish", completions(clap_complete::Shell::Fish));
        insta::assert_snapshot!(
            "completions_elvish",
            completions(clap_complete::Shell::Elvish)
        );
    }

    #[test]
    // windows: macOS and Linux print this help, tree and these scripts; Windows' are the test below
    #[cfg(target_os = "linux")]
    fn the_command_tree_help_and_zsh_fish_and_elvish_completions_read_as_they_did_on_linux() {
        insta::assert_json_snapshot!("command_tree_on_linux", command_tree());
        insta::assert_snapshot!("help_in_80_columns_on_linux", every_help_typed());
        insta::assert_snapshot!(
            "completions_zsh_on_linux",
            completions(clap_complete::Shell::Zsh)
        );
        insta::assert_snapshot!(
            "completions_fish_on_linux",
            completions(clap_complete::Shell::Fish)
        );
        insta::assert_snapshot!(
            "completions_elvish_on_linux",
            completions(clap_complete::Shell::Elvish)
        );
    }

    #[test]
    #[cfg(windows)]
    fn the_command_tree_help_and_powershell_completions_read_as_they_did_on_windows() {
        insta::assert_json_snapshot!("command_tree_on_windows", command_tree());
        insta::assert_snapshot!("help_in_80_columns_on_windows", every_help_typed());
        insta::assert_snapshot!(
            "completions_powershell_on_windows",
            completions(clap_complete::Shell::PowerShell)
        );
    }

    // Pitboard switches Claude Desktop on macOS alone, and only macOS keeps a login in a keychain.
    #[test]
    fn the_help_names_claude_desktop_only_on_macos() {
        let help = |command: Option<&str>| {
            let mut cli = Cli::command();
            let said = match command {
                Some(name) => cli
                    .find_subcommand_mut(name)
                    .expect(name)
                    .render_long_help(),
                None => cli.render_long_help(),
            };
            said.to_string()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };
        let enroll = help(Some("enroll"));
        match pitboard_core::host::OS {
            Os::MacOs => {
                assert!(
                    enroll.contains(
                        "`codex/work` names a Codex account, `desktop/work` a Claude Desktop \
                         one; a bare name means Claude Code"
                    ),
                    "{enroll}"
                );
                assert!(help(Some("use")).contains("`desktop/work`"));
                assert!(help(None).contains(
                    "Switch Claude Code, Codex and Claude Desktop between your own accounts \
                     without signing out, and see what each Claude Code and Codex account has \
                     left"
                ));
                assert!(
                    help(Some("status"))
                        .contains("how much each Claude Code and Codex account has left")
                );
                assert!(help(Some("stow")).contains("in a file behind the keychain"));
            }
            Os::Linux | Os::Windows => {
                let every = every_help();
                for named in ["Desktop", "desktop/", "keychain"] {
                    assert!(!every.contains(named), "{named}: {every}");
                }
                assert!(
                    enroll.contains(
                        "`codex/work` names a Codex account; a bare name means Claude Code"
                    ),
                    "{enroll}"
                );
                assert!(help(Some("use")).contains("such as `work` or `codex/work`"));
                assert!(help(None).contains(
                    "Switch Claude Code and Codex between your own accounts without signing out, \
                     and see what each account has left"
                ));
            }
        }
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
            "pitboard vault status",
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
