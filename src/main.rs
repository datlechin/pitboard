use anstream::{ColorChoice, eprintln, print, println};
use anstyle::{AnsiColor, Style};
use clap::{CommandFactory, Parser, Subcommand};
use pitboard::context::Context;
use pitboard::error::Error;
use pitboard::switch::{Enrolled, Outcome, Renewal, Settled, SignIn};
use pitboard::ui::{BOLD, paint};
use pitboard::{audit, doctor, state, status, statusline, switch};
use serde_json::{Value, json};
use std::io::Read;
use std::process::ExitCode;

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
}

impl Report {
    fn done(command: &'static str, data: Value, human: String) -> Report {
        Report {
            command: Some(command),
            result: Ok(data),
            warnings: Vec::new(),
            human,
            exit: 0,
        }
    }

    fn failed(command: Option<&'static str>, error: Error) -> Report {
        Report {
            command,
            exit: error.exit_code(),
            result: Err(error),
            warnings: Vec::new(),
            human: String::new(),
        }
    }
}

fn warning(code: &str, message: impl std::fmt::Display) -> Value {
    json!({ "code": code, "message": message.to_string() })
}

fn parks_pending(count: usize) -> Value {
    warning(
        "parks_pending_removal",
        format!(
            "{count} parked login(s) no longer in use could not be removed yet; \
             pitboard tries again on its next change"
        ),
    )
}

fn emit(report: Report, as_json: bool) -> ExitCode {
    if as_json {
        let (data, error) = match &report.result {
            Ok(data) => (data.clone(), Value::Null),
            Err(e) => (
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

fn status(ctx: &Context) -> Report {
    // Parked logins whose access has lapsed are renewed first, so every account is asked live.
    let renewals = switch::renew_parked(ctx);
    for (label, outcome) in &renewals {
        audit::record(ctx, "renew", label, outcome.code());
    }
    // Unreadable is not the same as empty: reporting it as empty would tell the user their
    // enrolled logins are gone.
    let state = match state::load(ctx) {
        Ok(s) => s,
        Err(e) => return Report::failed(Some("status"), e),
    };
    let report = status::gather(ctx, &state);
    let mut done = Report::done(
        "status",
        status::render_json(&report),
        status::render_human(&report),
    );
    done.warnings.extend(
        renewals
            .iter()
            .filter_map(|(label, outcome)| match outcome {
                Renewal::Refused => Some(warning(
                    outcome.code(),
                    format!(
                        "Anthropic no longer accepts the parked login for `{label}`. Run \
                     `pitboard enroll {label} --sign-in` to sign in to it again."
                    ),
                )),
                Renewal::Failed(e) => Some(warning(e.code(), e)),
                Renewal::Renewed | Renewal::Deferred => None,
            }),
    );
    done
}

fn doctor(ctx: &Context) -> Report {
    let diagnosis = doctor::run(ctx);
    let healthy = doctor::healthy(&diagnosis.checks);
    Report {
        // A failed check means an assumption pitboard relies on no longer holds.
        exit: if healthy { 0 } else { 3 },
        ..Report::done(
            "doctor",
            doctor::render_json(&diagnosis),
            doctor::render_human(&diagnosis.checks),
        )
    }
}

/// Claude Code reads the line through a pipe and draws its colours, so they are kept even
/// though stdout is not a terminal, unless `NO_COLOR` asks otherwise. The JSON form is plain.
fn statusline(ctx: &Context) -> Report {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let line = statusline::run(ctx, &input);
    if std::env::var_os("NO_COLOR").is_none() {
        ColorChoice::Always.write_global();
    }
    Report::done(
        "statusline",
        json!({ "line": anstream::adapter::strip_str(&line).to_string() }),
        format!("{line}\n"),
    )
}

/// Runs a command that changes state once any interrupted switch is settled. What settling
/// found is reported whether or not the command then succeeds.
fn changing(
    ctx: &Context,
    command: &'static str,
    label: &str,
    run: impl FnOnce(Settled) -> Report,
) -> Report {
    let (settled, recovered) = match switch::settle(ctx) {
        Ok(settled) => settled,
        Err(e) => {
            audit::record(ctx, command, label, e.code());
            return Report::failed(Some(command), e);
        }
    };
    let recovered = recovered.map(|r| {
        audit::record(ctx, "recover", &r.to, r.code());
        warning(r.code(), &r)
    });
    let mut report = run(settled);
    report.warnings.splice(0..0, recovered);
    report
}

/// The browser sign-in runs before pitboard takes its lock, so a person taking their time in
/// a browser never holds up a switch.
fn enroll_signing_in(ctx: &Context, label: &str) -> Report {
    let who = state::load(ctx)
        .ok()
        .and_then(|s| s.get(label).map(|a| a.email.clone()))
        .unwrap_or_else(|| "the account to add".to_string());
    eprintln!(
        "Opening Claude Code's sign-in. Sign in as {who}; the account in use now stays signed in."
    );
    match switch::sign_in(ctx) {
        Ok(login) => changing(ctx, "enroll", label, |s| enroll(ctx, s, label, Some(login))),
        Err(e) => {
            audit::record(ctx, "enroll", label, e.code());
            Report::failed(Some("enroll"), e)
        }
    }
}

fn enroll(ctx: &Context, settled: Settled, label: &str, signed_in: Option<SignIn>) -> Report {
    let outcome = switch::enroll(settled, label, signed_in);
    audit::record(
        ctx,
        "enroll",
        label,
        outcome.as_ref().map_or_else(|e| e.code(), |_| "ok"),
    );
    let name = paint(BOLD, label);
    let (kind, email, human) = match outcome {
        Ok(Enrolled::Current { email }) => {
            let human = format!(
                "Enrolled {name} ({email}), the account signed in now.\n\
                 Add another without signing out of it: pitboard enroll <label> --sign-in\n"
            );
            ("current", email, human)
        }
        Ok(Enrolled::SignedIn { email }) => {
            let human =
                format!("Enrolled {name} ({email}). Switch to it with: pitboard use {label}\n");
            ("signed_in", email, human)
        }
        Ok(Enrolled::Renewed { email }) => {
            let human = format!("Renewed {name} ({email}): its parked login is a fresh one.\n");
            ("renewed", email, human)
        }
        Err(e) => return Report::failed(Some("enroll"), e),
    };
    Report::done(
        "enroll",
        json!({ "label": label, "email": email, "enrolled": kind }),
        human,
    )
}

fn use_account(ctx: &Context, settled: Settled, label: &str) -> Report {
    let outcome = switch::switch(settled, label);
    audit::record(
        ctx,
        "use",
        label,
        match &outcome {
            Ok(Outcome::Switched { .. }) => "ok",
            Ok(Outcome::AlreadyActive { .. }) => "already_active",
            Err(e) => e.code(),
        },
    );
    match outcome {
        Ok(Outcome::AlreadyActive { label }) => Report::done(
            "use",
            json!({ "to": label, "changed": false }),
            format!("{} is already signed in.\n", paint(BOLD, &label)),
        ),
        Ok(Outcome::Switched {
            from,
            to,
            parked,
            config_warning,
            parks_pending: pending,
        }) => {
            let mut report = Report::done(
                "use",
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
            );
            report.warnings.extend(
                config_warning
                    .iter()
                    .map(|e| warning(e.code(), e))
                    .chain((pending > 0).then(|| parks_pending(pending))),
            );
            report
        }
        Err(e) => Report::failed(Some("use"), e),
    }
}

fn forget(ctx: &Context, settled: Settled, label: &str) -> Report {
    let outcome = switch::forget(settled, label);
    audit::record(
        ctx,
        "forget",
        label,
        outcome.as_ref().map_or_else(|e| e.code(), |_| "ok"),
    );
    match outcome {
        Ok((email, pending)) => {
            let mut report = Report::done(
                "forget",
                json!({ "label": label, "email": email }),
                format!("Forgot {} ({email}).\n", paint(BOLD, label)),
            );
            report
                .warnings
                .extend((pending > 0).then(|| parks_pending(pending)));
            report
        }
        Err(e) => Report::failed(Some("forget"), e),
    }
}

fn rename(ctx: &Context, settled: Settled, from: &str, to: &str) -> Report {
    let outcome = switch::rename(settled, from, to);
    audit::record(
        ctx,
        "rename",
        &format!("{from} -> {to}"),
        outcome.as_ref().map_or_else(|e| e.code(), |_| "ok"),
    );
    match outcome {
        Ok(email) => Report::done(
            "rename",
            json!({ "from": from, "to": to, "email": email }),
            format!(
                "Renamed {} to {} ({email}).\n",
                paint(BOLD, from),
                paint(BOLD, to)
            ),
        ),
        Err(e) => Report::failed(Some("rename"), e),
    }
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
    let ctx = Context::from_env();
    let report = match cli.command.unwrap_or(Command::Status) {
        Command::Status => status(&ctx),
        Command::Doctor => doctor(&ctx),
        Command::Statusline => statusline(&ctx),
        Command::Enroll {
            label,
            sign_in: true,
        } => enroll_signing_in(&ctx, &label),
        Command::Enroll { label, .. } => {
            changing(&ctx, "enroll", &label, |s| enroll(&ctx, s, &label, None))
        }
        Command::Use { label } => changing(&ctx, "use", &label, |s| use_account(&ctx, s, &label)),
        Command::Forget { label } => changing(&ctx, "forget", &label, |s| forget(&ctx, s, &label)),
        Command::Rename { from, to } => {
            changing(&ctx, "rename", &from, |s| rename(&ctx, s, &from, &to))
        }
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
