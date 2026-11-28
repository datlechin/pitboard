//! Starting again what still runs a tool on the login a switch left, where the tool's own
//! command does it ([`Remedy::Run`]): Codex's background app server, which
//! `codex app-server daemon restart` stops and starts again (`codex_app_server_daemon`,
//! `codex_app_server_daemon_restart`).
//!
//! It runs under Pitboard's lock, settled first, so the server never starts on a login an
//! interrupted switch left half moved, and no switch moves the login while it starts. Codex
//! bounds every step of its restart itself, so nothing here stops it part way, which would
//! leave the server stopped.

use super::{Error, Result, Settled, StillHolding, still_holding, still_running};
use crate::context::Context;
use crate::holder::{Holding, Remedy};
use crate::permit::Permit;
use crate::provider::{self, ProviderId, codex};
use crate::warning::Warning;
use std::process::{Command, Stdio};

/// Runs, once each, the command of every kind of `which`'s holders running now whose remedy
/// is one, then says what still runs on `from`'s login as a switch away from it says it: the
/// kinds only the person can start again. Where none of those kinds runs, nothing is run.
pub(crate) fn restart(
    settled: Settled,
    which: ProviderId,
    from: &str,
) -> Result<(Option<Warning>, Vec<Warning>)> {
    let Settled {
        _exclusive,
        ctx,
        permit,
        ..
    } = settled;
    if let StillHolding::These(holding) = still_holding(&ctx, which) {
        let mut ran: Vec<&str> = Vec::new();
        for held in &holding {
            let Remedy::Run { arguments, .. } = held.holder.remedy else {
                continue;
            };
            if !ran.contains(&arguments) {
                run(&ctx, permit, which, held, arguments)?;
                ran.push(arguments);
            }
        }
    }
    // Each kind started again holds the login in use now, though the process list still
    // names it.
    let left = match still_holding(&ctx, which) {
        StillHolding::These(holding) => {
            let left: Vec<Holding> = holding
                .into_iter()
                .filter(|held| !matches!(held.holder.remedy, Remedy::Run { .. }))
                .collect();
            if left.is_empty() {
                StillHolding::Nothing
            } else {
                StillHolding::These(left)
            }
        }
        other => other,
    };
    Ok((still_running(which, from, left), Vec::new()))
}

// The gate refuses an elevated process before a Permit is made, so a remedy to run from a
// terminal that is not elevated is run as it says. What the command prints on success is one
// JSON object Pitboard has no use for; what it prints on failure is its reason.
fn run(
    ctx: &Context,
    permit: Permit,
    which: ProviderId,
    held: &Holding,
    arguments: &str,
) -> Result<()> {
    let spelled = format!("{} {arguments}", held.program);
    let failed = |detail: String| Error::RestartFailed {
        tool: which,
        command: spelled.clone(),
        holder: held.phrase(),
        detail,
    };
    let mut command =
        provider::command(ctx, permit, which).map_err(|error| failed(error.to_string()))?;
    command
        .args(arguments.split(' '))
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    in_its_home(ctx, which, &mut command);
    let output = command
        .output()
        .map_err(|error| failed(error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(failed(
            reason(&output.stderr).unwrap_or_else(|| output.status.to_string()),
        ))
    }
}

// Pointed at the home this context names, which a test's differs from this process's.
fn in_its_home(ctx: &Context, which: ProviderId, command: &mut Command) {
    match which {
        ProviderId::Codex => {
            command.env("CODEX_HOME", codex::paths::home(ctx));
        }
        ProviderId::Claude => {
            if let Some(dir) = &ctx.claude_config_dir {
                command.env("CLAUDE_CONFIG_DIR", dir);
            }
        }
        // Claude Desktop's holder is its app, which no command of its own starts again.
        ProviderId::Desktop => {}
    }
}

// Codex returns its error from `main`, so it is said as `Error: ` and the reason, then any
// `Caused by:` lines, after whatever warning it printed on the way
// (`codex_app_server_daemon_restart`). Without that line, the last thing it said is the
// reason. Kept to a line a person can read in an alert.
const REASON_LIMIT: usize = 240;

fn reason(stderr: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stderr);
    let line = text
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("Error: "))
        .or_else(|| text.lines().rfind(|line| !line.trim().is_empty()))?
        .trim();
    Some(match line.char_indices().nth(REASON_LIMIT) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_owned(),
    })
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test reads what its stand-in recorded"
)]
mod tests {
    use super::*;
    use crate::stand_in::{self, Script, Step};
    use crate::switch::harness::codex_machine;
    use crate::switch::settle;

    const DAEMON: &str = "/Users/a/.codex/packages/app-server-daemon/releases/0.172.0-aarch64-apple-darwin/bin/codex";

    fn settled(m: &crate::switch::harness::Rig) -> Settled {
        settle(&m.ctx, Permit::for_a_test(), Some(ProviderId::Codex))
            .expect("nothing to recover")
            .0
    }

    // A `codex` that records how it was run, then takes `then`.
    fn codex(m: &mut crate::switch::harness::Rig, then: Vec<Step>) -> std::path::PathBuf {
        let calls = m.ctx_home().join("calls.jsonl");
        let bin = m.ctx_home().join("bin");
        std::fs::create_dir_all(&bin).expect("a folder for programs");
        let program = bin.join(format!("codex{}", std::env::consts::EXE_SUFFIX));
        let mut steps = vec![Step::Records {
            at: calls.clone(),
            variables: vec!["CODEX_HOME".into()],
        }];
        steps.extend(then);
        stand_in::install(&program, &Script::Plays { args: None, steps }).expect("a scratch codex");
        m.ctx = m.ctx.clone().with_codex_program(program);
        calls
    }

    fn records(calls: &std::path::Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(calls)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).expect("a record"))
            .collect()
    }

    /// The app server is started again in the home Pitboard switched, once though Codex
    /// recorded it twice, and what only the person can start again is still said.
    #[test]
    fn the_app_server_is_restarted_in_the_switched_home_and_sessions_are_still_said() {
        let mut m = codex_machine("restart-runs");
        let calls = codex(&mut m, Vec::new());
        m.mem.runs_at("codex", &[DAEMON, "codex"]);

        let (left, _) = restart(settled(&m), ProviderId::Codex, "codex/here").expect("restarted");

        let records = records(&calls);
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(
            records[0]["args"],
            serde_json::json!(["app-server", "daemon", "restart"])
        );
        assert_eq!(
            records[0]["variables"]["CODEX_HOME"],
            m.ctx_home().join(".codex").to_str().expect("a path")
        );
        let left = left.expect("a session still runs").to_string();
        assert!(left.starts_with("1 `codex` session started"), "{left}");
        assert!(!left.contains("app server"), "{left}");
    }

    /// With nothing else on the old login, nothing is left to say.
    #[test]
    fn nothing_is_left_to_say_once_the_app_server_was_all_that_ran() {
        let mut m = codex_machine("restart-alone");
        codex(&mut m, Vec::new());
        m.mem.runs_at("codex", &[DAEMON]);

        let (left, _) = restart(settled(&m), ProviderId::Codex, "codex/here").expect("restarted");

        assert!(left.is_none(), "{left:?}");
    }

    /// A server that stopped since the switch is not started: `restart` would start one the
    /// person no longer runs.
    #[test]
    fn a_server_that_stopped_since_the_switch_is_not_started() {
        let mut m = codex_machine("restart-stopped");
        let calls = codex(&mut m, Vec::new());
        m.mem.runs_at("codex", &["codex"]);

        let (left, _) = restart(settled(&m), ProviderId::Codex, "codex/here").expect("nothing run");

        assert!(records(&calls).is_empty());
        assert!(left.is_some_and(|left| left.code() == "sessions_still_running"));
    }

    /// Codex's own reason is said, with the command that failed.
    #[test]
    fn a_failed_restart_says_codexs_reason() {
        let mut m = codex_machine("restart-fails");
        codex(
            &mut m,
            vec![
                Step::Warns("warning: something on the way\n".into()),
                Step::Warns(
                    "Error: app server is running but is not managed by codex app-server daemon\n"
                        .into(),
                ),
                Step::Exits(1),
            ],
        );
        m.mem.runs_at("codex", &[DAEMON]);

        let error = restart(settled(&m), ProviderId::Codex, "codex/here").expect_err("it failed");

        assert_eq!(error.code(), "restart_failed");
        let said = error.to_string();
        assert!(said.contains("`codex app-server daemon restart`"), "{said}");
        assert!(
            said.contains("app server is running but is not managed by codex app-server daemon"),
            "{said}"
        );
        assert!(!said.contains("Error: "), "{said}");
    }

    #[test]
    fn a_reason_is_the_last_line_said_and_no_longer_than_an_alert_holds() {
        assert_eq!(reason(b""), None);
        assert_eq!(reason(b"\n  \n"), None);
        assert_eq!(reason(b"one\ntwo\n\n").as_deref(), Some("two"));
        assert_eq!(
            reason(b"warning: on the way\nError: the reason\n\nCaused by:\n    deeper\n")
                .as_deref(),
            Some("the reason")
        );
        let long = "x".repeat(REASON_LIMIT + 10);
        let cut = reason(long.as_bytes()).expect("a reason");
        assert_eq!(cut.chars().count(), REASON_LIMIT + 1);
        assert!(cut.ends_with('…'));
    }
}
