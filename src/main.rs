use clap::{CommandFactory, Parser, Subcommand};
use pitboard::error::Error;
use pitboard::switch::{Enrolled, Outcome};
use pitboard::{audit, doctor, state, status, switch};
use serde_json::{Value, json};
use std::process::ExitCode;

/// Bumped only when a field changes shape. Adding a field or an error code is not a
/// breaking change for a consumer; renaming or removing one is.
const CONTRACT: u32 = 1;

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
    /// What is signed in, and how much of it is left (the default)
    Status,
    /// Add an account: the one signed in now, or with --sign-in, another one
    Enroll {
        /// A short name for this account, such as `personal` or `work`
        label: String,
        /// Sign in to a different account through Claude Code's own sign-in, without
        /// signing out of the one in use now
        #[arg(long)]
        sign_in: bool,
    },
    /// Sign in as an enrolled account
    Use {
        /// The label the account was enrolled under
        label: String,
    },
    /// Drop an account and the logins parked for it
    Forget {
        /// The label to drop
        label: String,
    },
    /// Check that pitboard's model of Claude Code still holds on this machine
    Doctor,
    /// Print shell completions
    #[command(hide = true)]
    Completions { shell: clap_complete::Shell },
    /// Print the man page
    #[command(hide = true)]
    Manpage,
}

/// What a command produced, before it is rendered for a person or a program.
struct Report {
    command: &'static str,
    result: Result<Value, Error>,
    warnings: Vec<Value>,
    human: String,
    exit: u8,
}

fn warning(error: &Error) -> Value {
    json!({ "code": error.code(), "message": error.to_string() })
}

fn emit(report: Report, as_json: bool) -> ExitCode {
    let name = env!("CARGO_BIN_NAME");
    if as_json {
        let envelope = match &report.result {
            Ok(data) => json!({
                "v": CONTRACT,
                "command": report.command,
                "ok": report.exit == 0,
                "data": data,
                "warnings": report.warnings,
                "error": null,
            }),
            Err(e) => json!({
                "v": CONTRACT,
                "command": report.command,
                "ok": false,
                "data": null,
                "warnings": report.warnings,
                "error": { "code": e.code(), "message": e.to_string() },
            }),
        };
        println!("{envelope}");
    } else {
        match &report.result {
            Ok(_) => print!("{}", report.human),
            Err(e) => eprintln!("{name}: {e}"),
        }
        for w in &report.warnings {
            eprintln!("note: {}", w["message"].as_str().unwrap_or_default());
        }
    }
    ExitCode::from(report.exit)
}

fn failure(command: &'static str, error: Error) -> Report {
    Report {
        command,
        exit: error.exit_code(),
        result: Err(error),
        warnings: Vec::new(),
        human: String::new(),
    }
}

fn status() -> Report {
    // Unreadable is not the same as empty: reporting it as empty would tell the user their
    // enrolled logins are gone.
    let state = match state::load() {
        Ok(s) => s,
        Err(e) => return failure("status", e),
    };
    let report = status::gather(&state);
    Report {
        command: "status",
        human: format!("\n{}", status::render_human(&report)),
        result: Ok(status::render_json(&report)),
        warnings: Vec::new(),
        exit: 0,
    }
}

fn doctor() -> Report {
    let checks = doctor::run();
    let healthy = doctor::healthy(&checks);
    Report {
        command: "doctor",
        human: format!("\n{}", doctor::render_human(&checks)),
        result: Ok(doctor::render_json(&checks)),
        warnings: Vec::new(),
        // A failed check means an assumption about Claude Code no longer holds.
        exit: if healthy { 0 } else { 3 },
    }
}

fn enroll(label: &str, sign_in: bool) -> Report {
    let outcome = switch::enroll(label, sign_in);
    audit::record(
        "enroll",
        label,
        outcome.as_ref().map_or_else(|e| e.code(), |_| "ok"),
    );
    match outcome {
        Ok(Enrolled::Current { email }) => Report {
            command: "enroll",
            human: format!(
                "enrolled {email}, the account signed in now, as `{label}`\n\n\
                 To add another account without signing out of this one:\n  \
                 pitboard enroll <label> --sign-in\n"
            ),
            result: Ok(json!({ "label": label, "email": email, "signed_in_now": true })),
            warnings: Vec::new(),
            exit: 0,
        },
        Ok(Enrolled::SignedIn { email }) => Report {
            command: "enroll",
            human: format!(
                "enrolled {email} as `{label}`; switch to it with `pitboard use {label}`\n"
            ),
            result: Ok(json!({ "label": label, "email": email, "signed_in_now": false })),
            warnings: Vec::new(),
            exit: 0,
        },
        Err(e) => failure("enroll", e),
    }
}

fn use_account(label: &str) -> Report {
    let outcome = switch::switch(label);
    audit::record(
        "use",
        label,
        match &outcome {
            Ok(Outcome::Switched { .. }) => "ok",
            Ok(Outcome::AlreadyActive { .. }) => "already_active",
            Err(e) => e.code(),
        },
    );
    match outcome {
        Ok(Outcome::AlreadyActive { label }) => Report {
            command: "use",
            human: format!("`{label}` is already signed in\n"),
            result: Ok(json!({ "to": label, "changed": false })),
            warnings: Vec::new(),
            exit: 0,
        },
        Ok(Outcome::Switched {
            from,
            to,
            parked,
            config_warning,
            stuck_generations,
        }) => {
            let mut warnings: Vec<Value> = config_warning.iter().map(warning).collect();
            if !stuck_generations.is_empty() {
                warnings.push(json!({
                    "code": "stale_parks_remain",
                    "message": format!(
                        "{} old parked login(s) for `{from}` could not be removed; \
                         harmless, run `pitboard doctor` to check",
                        stuck_generations.len()
                    ),
                }));
            }
            Report {
                command: "use",
                human: format!(
                    "signed in as `{to}`, parked `{from}`'s previous login\n\
                     a Claude Code session already running picks this up within {} seconds\n",
                    switch::ADOPTION_CEILING_SECONDS
                ),
                result: Ok(json!({
                    "from": from,
                    "to": to,
                    "changed": true,
                    "parked_at": parked.parked_at,
                    "adoption_ceiling_seconds": switch::ADOPTION_CEILING_SECONDS,
                })),
                warnings,
                exit: 0,
            }
        }
        Err(e) => failure("use", e),
    }
}

fn forget(label: &str) -> Report {
    let outcome = switch::forget(label);
    audit::record(
        "forget",
        label,
        outcome.as_ref().map_or_else(|e| e.code(), |_| "ok"),
    );
    match outcome {
        Ok((email, stuck)) => Report {
            command: "forget",
            human: format!("forgot `{label}` ({email})\n"),
            result: Ok(json!({ "label": label, "email": email })),
            warnings: if stuck.is_empty() {
                Vec::new()
            } else {
                vec![json!({
                    "code": "stale_parks_remain",
                    "message": format!(
                        "{} parked login(s) for `{label}` are still in the keychain; \
                         harmless, run `pitboard doctor` to check",
                        stuck.len()
                    ),
                })]
            },
            exit: 0,
        },
        Err(e) => failure("forget", e),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let report = match cli.command.unwrap_or(Command::Status) {
        Command::Status => status(),
        Command::Doctor => doctor(),
        Command::Enroll { label, sign_in } => enroll(&label, sign_in),
        Command::Use { label } => use_account(&label),
        Command::Forget { label } => forget(&label),
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
    fn no_arguments_means_status() {
        assert!(Cli::try_parse_from(["pitboard"]).unwrap().command.is_none());
    }
}
