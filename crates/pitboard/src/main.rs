use anstream::{ColorChoice, eprintln, print, println};
use anstyle::{AnsiColor, Style};
use clap::{CommandFactory, Parser, Subcommand};
use pitboard_core::context::Context;
use pitboard_core::doctor;
use pitboard_core::error::Error;
use pitboard_core::service::{Changing, Done, Failed, Pitboard, Warning};
use pitboard_core::switch::{self, Enrolled, Outcome};
use serde_json::{Value, json};
use std::io::{IsTerminal, Read, Write};
use std::process::ExitCode;
use ui::{BOLD, DIM, WARN, paint};

mod render;
mod ui;

/// Bumped only when a field changes shape. Adding a field or an error code is not a
/// breaking change for a consumer; renaming or removing one is.
const CONTRACT: u32 = 1;

const ERROR: Style = AnsiColor::Red.on_default().bold();
const WARNING: Style = AnsiColor::Yellow.on_default().bold();

/// Park and restore your own Claude Code logins, and see what each one has left.
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
    Status,
    /// Add an account: the one signed in now, or with --sign-in, another one
    Enroll {
        /// A short name for this account, such as `personal` or `work`
        #[arg(value_parser = new_label)]
        label: String,
        /// Sign in through Claude Code's own sign-in, without signing out of the account in
        /// use. For an enrolled label, this renews its parked login.
        #[arg(long)]
        sign_in: bool,
    },
    /// Switch Claude Code to an enrolled account
    Use {
        /// The label the account was enrolled under
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
    /// Give up on an interrupted switch that cannot be finished, keeping every login
    Abandon,
    /// What pitboard has changed, and when
    Log {
        /// How many changes to show
        #[arg(short = 'n', long, default_value_t = 20)]
        lines: usize,
    },
    /// Delete every parked login and pitboard's own files
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
    /// Check that what pitboard relies on still holds on this machine
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
fn new_label(text: &str) -> Result<String, String> {
    if text.is_empty() || text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("a label must be one word, such as `personal` or `work`".into());
    }
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
                json!({ "code": e.code(), "message": e.to_string() }),
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
        Err(Failed {
            error,
            warnings: found,
        }) => Report {
            warnings: warnings(&found),
            ..Report::failed(Some(command), error)
        },
    }
}

fn status(pitboard: &Pitboard) -> Report {
    match pitboard.status() {
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
        // A failed check means an assumption pitboard relies on no longer holds.
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
    let who = pitboard
        .account(label)
        .map_or_else(|| "the account to add".to_string(), |a| a.email);
    eprintln!(
        "Opening Claude Code's sign-in. Sign in as {who}; the account in use now stays signed in."
    );
    match pitboard.sign_in(label) {
        Ok(login) => enrolled(label, pitboard.enroll_signed_in(label, login)),
        Err(e) => Report::failed(Some("enroll"), e),
    }
}

fn enrolled(label: &str, outcome: Changing<Enrolled>) -> Report {
    let name = paint(BOLD, label);
    changed("enroll", outcome, |enrolled| {
        let (kind, email, human) = match enrolled {
            Enrolled::Current { email } => {
                let human = format!(
                    "Enrolled {name} ({email}), the account signed in now.\n\
                     Add another without signing out of it: pitboard enroll <label> --sign-in\n"
                );
                ("current", email, human)
            }
            Enrolled::SignedIn { email } => {
                let human =
                    format!("Enrolled {name} ({email}). Switch to it with: pitboard use {label}\n");
                ("signed_in", email, human)
            }
            Enrolled::Renewed { email } => {
                let human = format!("Renewed {name} ({email}): its parked login is a fresh one.\n");
                ("renewed", email, human)
            }
        };
        (
            json!({ "label": label, "email": email, "enrolled": kind }),
            human,
        )
    })
}

fn use_account(pitboard: &Pitboard, label: &str) -> Report {
    changed("use", pitboard.switch_to(label), |outcome| match outcome {
        Outcome::AlreadyActive { label } => (
            json!({ "to": label, "changed": false }),
            format!("{} is already signed in.\n", paint(BOLD, &label)),
        ),
        Outcome::Switched { from, to, parked } => (
            json!({
                "from": from,
                "to": to,
                "changed": true,
                "parked_at": parked.parked_at,
                "adoption_ceiling_seconds": switch::ADOPTION_CEILING_SECONDS,
            }),
            format!(
                "Switched to {}; {} is parked.\n\
                 Claude Code sessions already running follow within {} seconds.\n",
                paint(BOLD, &to),
                paint(BOLD, &from),
                switch::ADOPTION_CEILING_SECONDS
            ),
        ),
    })
}

fn forget(pitboard: &Pitboard, label: &str) -> Report {
    changed("forget", pitboard.forget(label), |email| {
        (
            json!({ "label": label, "email": email }),
            format!("Forgot {} ({email}).\n", paint(BOLD, label)),
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

fn log(pitboard: &Pitboard, lines: usize) -> Report {
    let entries = pitboard.log(lines);
    let width = entries.iter().map(|e| e.verb.len()).max().unwrap_or(0);
    let human = if entries.is_empty() {
        "pitboard has not changed anything yet.\n".to_string()
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
            "Removed {} parked login(s). Claude Code's login is untouched.\n",
            removed.parks
        );
        if removed.pending > 0 {
            human.push_str(&format!(
                "{} could not be deleted, so ~/.pitboard was kept; run `pitboard \
                 uninstall` again.\n",
                removed.pending
            ));
        } else if removed.home_removed {
            human.push_str(
                "~/.pitboard is gone. Uninstall the binary itself with your \
                            package manager.\n",
            );
        }
        (
            json!({
                "parks_removed": removed.parks,
                "parks_pending": removed.pending,
                "home_removed": removed.home_removed,
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
    let report = match cli.command.unwrap_or(Command::Status) {
        Command::Status => status(&pitboard),
        Command::Doctor => doctor(&pitboard),
        Command::Statusline => statusline(&pitboard),
        Command::Enroll {
            label,
            sign_in: true,
        } => enroll_signing_in(&pitboard, &label),
        Command::Enroll { label, .. } => enrolled(&label, pitboard.enroll_current(&label)),
        Command::Use { label } => use_account(&pitboard, &label),
        Command::Forget { label, yes } => {
            // The way back is a browser sign-in for that account, which is the cost
            // pitboard exists to spare people. Asked only where there is someone to ask:
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
        Command::Log { lines } => log(&pitboard, lines),
        Command::Uninstall { yes } => {
            if !yes
                && !cli.json
                && std::io::stdin().is_terminal()
                && std::io::stderr().is_terminal()
            {
                eprint!(
                    "Delete every parked login and ~/.pitboard? The account you are signed \
                     in to stays signed in; the others need a browser sign-in again. [y/N] "
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
            return match clap_mangen::Man::new(Cli::command()).render(&mut std::io::stdout()) {
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

    #[test]
    fn no_arguments_means_status() {
        assert!(Cli::try_parse_from(["pitboard"]).unwrap().command.is_none());
    }
}
