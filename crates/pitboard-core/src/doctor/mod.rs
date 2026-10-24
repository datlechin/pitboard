//! `pitboard doctor`: whether what Pitboard relies on here still holds, and which assumption broke.

mod claude;
mod codex;
mod desktop;
mod machine;
mod network;
mod parks;
mod redact;
mod schedule;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests;
mod windows;
mod words;

use crate::context::Context;
use crate::error::Error;
use crate::host::{Access, Kind, Os};
use crate::provider::ProviderId;
use crate::provider::claude::live as claude_live;
use crate::provider::claude::paths as claude_paths;
use crate::provider::claude::{daemon, slot};
use crate::state::State;
pub use crate::state::renewal_due;
use crate::{home, store, switch};
use claude::FallbackLogin;
use codex::CodexFacts;
pub use desktop::DesktopFacts;
use network::NetworkFact;
use parks::ParkFact;
use schedule::{ScheduleFact, WatchServiceFact};
use serde_json::{Value, json};
use std::path::PathBuf;
use windows::{StatusLineFact, WindowsHomes, WslFact};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug)]
pub struct Check {
    /// Stable, snake_case, safe for a program to branch on.
    pub code: &'static str,
    pub name: String,
    pub level: Level,
    pub detail: String,
    /// What the user should do. Empty when there is nothing to do.
    pub advice: String,
}

// Everything read from the machine, so judging it touches nothing.
pub(crate) struct Facts {
    pub(crate) os: Os,
    // The person's Windows SID, which advice to make a file private names.
    pub(crate) sid: Option<String>,
    pub(crate) security_tool: Option<String>,
    pub(crate) config_path: PathBuf,
    pub(crate) config: Result<Value, Error>,
    pub(crate) config_account: Option<claude_paths::ConfigAccount>,
    pub(crate) service: String,
    pub(crate) account: String,
    pub(crate) default_slot: bool,
    pub(crate) storage_dir: String,
    pub(crate) backend: Result<store::Backend, store::Error>,
    pub(crate) vault: store::Backend,
    pub(crate) credential_file: PathBuf,
    pub(crate) credential: Result<Option<Value>, store::Error>,
    // Against the store's ceiling, where it has one.
    pub(crate) credential_cost: Option<store::Cost>,
    // `PITBOARD_NO_ARGV=1`, which nothing on Windows has an argument line to refuse.
    pub(crate) no_argv: bool,
    // (what it is, bytes), largest first.
    pub(crate) credential_parts: Vec<(String, usize)>,
    pub(crate) home: PathBuf,
    pub(crate) home_access: Option<Access>,
    // Each login on disk somebody else can read: its path, whether it is a folder, its access.
    pub(crate) readable_by_others: Vec<(String, Kind, Access)>,
    pub(crate) machine_id_known: bool,
    pub(crate) windows: Option<WindowsHomes>,
    // `CLAUDE_CODE_HOVER_REST`, which turns on the successor credential backend.
    pub(crate) hover_rest_env: bool,
    pub(crate) daemon: Option<daemon::Daemon>,
    // Names written down before a park was made, and not resolved yet.
    pub(crate) pending_parks: Vec<String>,
    pub(crate) claude_version: Option<String>,
    pub(crate) claude_program: Option<PathBuf>,
    // From settings files as well as this process's environment.
    pub(crate) auth_overrides: Vec<crate::settings::Override>,
    // Each managed `forceLoginOrgUUID`, and where.
    pub(crate) organisation_pins: Vec<String>,
    // The enrolled accounts those leave out.
    pub(crate) pinned_out: Vec<String>,
    // (uuid, seconds) of each account not asked about yet.
    pub(crate) asking_held: Vec<(String, i64)>,
    pub(crate) state: Result<State, Error>,
    pub(crate) in_use: Option<crate::in_use::Known>,
    pub(crate) parks: Vec<ParkFact>,
    pub(crate) interrupted: bool,
    // The refusal of the next change, over an interrupted switch it cannot finish.
    pub(crate) stuck: Option<String>,
    pub(crate) codex: CodexFacts,
    // Installed, run, signed in or enrolled: a machine with only Codex is not judged on it.
    pub claude_present: bool,
    // Where a Claude Desktop account is enrolled or a folder is set aside for one.
    pub desktop: Option<DesktopFacts>,
    pub(crate) desktop_app: desktop::Installed,
    pub(crate) schedule: Option<ScheduleFact>,
    pub(crate) watch_service: Option<WatchServiceFact>,
    pub(crate) network: NetworkFact,
    // The app runs with the environment the system started it with, as the schedule's runs do.
    pub(crate) in_the_app: bool,
    pub(crate) elevation: crate::host::Elevation,
    // Claude Code's file, where it sits behind the store holding the login in use.
    pub(crate) fallback_login: Option<FallbackLogin>,
    pub(crate) floor: crate::host::Floor,
    // Windows' alone, where a shell decides whether `pitboard statusline` is found.
    pub(crate) status_line: Option<StatusLineFact>,
    // Windows' alone.
    pub(crate) windows_settings: Option<crate::host::WindowsSettings>,
    // Windows' alone: each `pitboard.exe` a shell finds on PATH, the one it runs first.
    pub(crate) pitboards: Vec<PathBuf>,
    // Linux's alone, inside WSL.
    pub(crate) wsl: Option<WslFact>,
    pub(crate) now: i64,
}

pub(crate) fn gather(ctx: &Context) -> Facts {
    let pitboards = match ctx.os {
        Os::Windows => windows::pitboards(ctx),
        Os::MacOs | Os::Linux => Vec::new(),
    };
    let config = claude_paths::load_config(ctx);
    let state = crate::state::load(ctx);
    let service = claude_paths::live_service(ctx);
    let home = home::dir(ctx);
    let config_account = config.as_ref().ok().and_then(claude_paths::config_account);
    let (organisation_pins, pinned_out) = claude::login_pins(ctx, state.as_ref().ok());
    let found = claude::found(ctx, &service);
    let claude_build = desktop::desktop_build(ctx);
    let desktop = state
        .as_ref()
        .ok()
        .and_then(|s| desktop::desktop_facts(ctx, s, claude_build.clone()));
    let desktop_app = desktop::installed(ctx, desktop.as_ref(), claude_build);
    Facts {
        os: crate::host::OS,
        sid: ctx.sid.clone(),
        security_tool: crate::host::OS
            .secrets_tool()
            .filter(|p| std::fs::metadata(p).is_ok())
            .map(str::to_string),
        config_path: claude_paths::config_file(ctx),
        config_account,
        config,
        account: slot::account_name(ctx),
        default_slot: claude_paths::is_default_slot(ctx),
        storage_dir: claude_paths::storage_dir(ctx),
        backend: found.backend,
        vault: store::vault_kind(ctx),
        credential_file: claude_live::credential_file(ctx),
        credential_cost: found.cost,
        no_argv: !ctx.argv_fallback(),
        credential_parts: found.parts,
        credential: found.credential,
        home_access: crate::host::fs::access(&home),
        readable_by_others: machine::loose_logins(ctx),
        home,
        machine_id_known: crate::state::machine_id() != crate::state::UNKNOWN_MACHINE,
        windows: (ctx.os == Os::Windows).then(|| WindowsHomes {
            home: ctx.home().to_path_buf(),
            default_pitboard_home: ctx.default_pitboard_home(),
            local_app_data: crate::host::user::local_app_data(),
            profile: ctx.profile.clone(),
            codex_home_set: ctx.codex_home().is_some(),
            roams: crate::host::user::profile_roams(),
        }),
        hover_rest_env: ctx.hover_rest,
        daemon: daemon::read(ctx),
        pending_parks: crate::pending::outstanding(ctx, state.as_ref().ok()),
        claude_version: claude_paths::installed_version(ctx),
        claude_program: claude_paths::program(ctx),
        auth_overrides: crate::settings::overrides(ctx),
        organisation_pins,
        pinned_out,
        asking_held: crate::usage::budget::holds(ctx),
        in_use: state
            .as_ref()
            .ok()
            .map(|s| crate::in_use::known(ctx, s, ProviderId::Claude)),
        parks: state
            .as_ref()
            .map(|s| parks::park_facts(ctx, s))
            .unwrap_or_default(),
        codex: codex::codex_facts(ctx, state.as_ref().ok()),
        desktop,
        desktop_app,
        // As the next change reads it, without its question: `doctor` sends no request.
        stuck: state
            .as_ref()
            .ok()
            .and_then(|s| switch::stuck(ctx, s, switch::Asking::Nobody))
            .map(|refusal| refusal.to_string()),
        claude_present: claude_paths::config_file(ctx).exists()
            || claude_paths::program(ctx).is_some()
            || found.stored
            || state.as_ref().is_ok_and(|s| {
                s.accounts
                    .iter()
                    .any(|a| a.provider() == ProviderId::Claude)
            }),
        state,
        interrupted: switch::interrupted(ctx),
        service,
        schedule: schedule::schedule_fact(ctx),
        watch_service: schedule::watch_service_fact(ctx),
        network: network::network_fact(&ctx.proxy),
        in_the_app: ctx.caller == "app",
        elevation: ctx.host().elevation(ctx),
        fallback_login: claude::fallback_login(ctx),
        floor: ctx.host().floor(),
        status_line: (ctx.os == Os::Windows).then(|| StatusLineFact {
            git_bash: claude_paths::git_bash(ctx),
            powershell: claude_paths::powershell(ctx),
            pitboard: pitboards.first().cloned(),
            runs_pitboard: crate::settings::status_line_command(ctx)
                .is_some_and(|command| windows::runs_bare_pitboard(&command)),
        }),
        windows_settings: (ctx.os == Os::Windows).then(|| ctx.host().windows_settings()),
        pitboards,
        wsl: windows::wsl_fact(ctx),
        now: ctx.now(),
    }
}

fn ok(code: &'static str, name: impl Into<String>, detail: impl Into<String>) -> Check {
    Check {
        code,
        name: name.into(),
        level: Level::Ok,
        detail: detail.into(),
        advice: String::new(),
    }
}

fn warn(
    code: &'static str,
    name: impl Into<String>,
    detail: impl Into<String>,
    advice: impl Into<String>,
) -> Check {
    Check {
        code,
        name: name.into(),
        level: Level::Warn,
        detail: detail.into(),
        advice: advice.into(),
    }
}

fn fail(
    code: &'static str,
    name: impl Into<String>,
    detail: impl Into<String>,
    advice: impl Into<String>,
) -> Check {
    Check {
        code,
        name: name.into(),
        level: Level::Fail,
        detail: detail.into(),
        advice: advice.into(),
    }
}

#[cfg(test)]
pub(crate) fn evaluate(facts: &Facts) -> Vec<Check> {
    evaluate_with(facts, &crate::time::moment)
}

// `moment` writes each moment a check names, given it and now.
pub(crate) fn evaluate_with(facts: &Facts, moment: &dyn Fn(i64, i64) -> String) -> Vec<Check> {
    let mut checks = Vec::new();
    // A new machine is told about Claude Code first; one with only Codex is not.
    let codex_here =
        facts.codex.present || facts.codex.program.is_some() || facts.codex.enrolled > 0;
    let claude_here = facts.claude_present || !(codex_here || facts.desktop.is_some());

    // First, in the order the gate asks them: they are why every change here is refused.
    checks.extend(machine::too_old(facts.floor));
    checks.extend(machine::elevated(
        facts.os,
        facts.elevation,
        facts
            .windows_settings
            .as_ref()
            .and_then(|settings| settings.admin_approval),
    ));
    checks.extend(machine::judge_security_tool(facts));
    checks.push(claude::judge_config(facts));
    checks.push(claude::judge_identity(facts));
    checks.extend(claude::judge_in_use(facts, moment));
    checks.push(claude::judge_slot(facts));
    checks.extend(claude::judge_size(facts));
    checks.extend(claude::judge_no_argv(facts));
    checks.push(claude::judge_store(facts));
    checks.push(claude::judge_credential(facts));
    checks.extend(claude::judge_fallback_login(facts));
    checks.push(machine::judge_home(facts));
    checks.push(machine::judge_private_on_disk(facts));
    let lost = parks::lost_parks(facts);
    checks.extend(parks::judge_vault(facts, &lost));
    checks.push(machine::judge_state(facts));
    // Every other tool's accounts are judged in its own section.
    let judged = |p: &&ParkFact| !lost.iter().any(|gone| std::ptr::eq(*gone, *p));
    let claude_parks: Vec<&ParkFact> = facts
        .parks
        .iter()
        .filter(|p| p.provider == ProviderId::Claude)
        .filter(judged)
        .collect();
    let codex_parks: Vec<&ParkFact> = facts
        .parks
        .iter()
        .filter(|p| p.provider == ProviderId::Codex)
        .filter(judged)
        .collect();
    checks.extend(
        claude_parks
            .iter()
            .map(|p| parks::judge_park(p, facts.now, moment)),
    );
    checks.extend(machine::judge_interrupted(facts));
    checks.extend(machine::judge_discarded(facts));
    checks.extend(machine::judge_machine_id(facts));
    checks.extend(windows::judge_windows_homes(facts));
    checks.extend(windows::judge_windows_settings(facts));
    checks.extend(windows::judge_wsl(facts.wsl.as_ref()));
    checks.push(claude::judge_storage_v5(facts));
    checks.push(claude::judge_daemon(facts));
    checks.push(machine::judge_pending(facts));
    checks.extend(schedule::judge_schedule(facts));
    checks.extend(schedule::judge_schedule_proxy(facts));
    checks.extend(schedule::judge_watch_service(facts));
    checks.push(claude::judge_claude_version(facts));
    checks.extend(windows::judge_status_line(facts.status_line.as_ref()));
    checks.push(claude::judge_auth(facts));
    checks.extend(claude::judge_login_restrictions(facts));
    checks.push(machine::judge_asking(facts));
    checks.push(network::judge_network(&facts.network));
    checks.extend(
        claude_parks
            .iter()
            .filter_map(|p| parks::judge_dormant(p, facts.now)),
    );
    // A machine without Codex reads as it did before Pitboard knew Codex.
    if codex_here || !codex_parks.is_empty() {
        checks.extend(codex::judge_codex_installer_home(facts));
        checks.extend(codex::judge_codex(
            facts.os,
            facts.sid.as_deref(),
            &facts.codex,
            &codex_parks,
            facts.now,
            moment,
        ));
    }
    if let (Some(desktop), Ok(state)) = (&facts.desktop, &facts.state) {
        checks.extend(match (facts.os, &desktop.root) {
            (Os::MacOs, _) | (Os::Linux, Ok(_)) => {
                desktop::judge_desktop(facts.os, desktop, state, &facts.parks, facts.now, moment)
            }
            (Os::Linux, Err(_)) | (Os::Windows, _) => vec![desktop::judge_desktop_off_macos(state)],
        });
    }
    if !claude_here {
        checks.retain(|check| !CLAUDE_CODES_OWN.contains(&check.code));
    }
    checks
}

// Left out on a machine without Claude Code: a new check of Claude Code's goes here.
const CLAUDE_CODES_OWN: &[&str] = &[
    "config_file",
    "identity",
    "in_use",
    "slot",
    "credential_size",
    "credential_store",
    "credential",
    "fallback_login",
    "storage_v5",
    "claude_daemon",
    "claude_version",
    "status_line",
    "auth_source",
    "login_restrictions",
];

/// The tool whose section a check is in; `None` for Claude Code's and Pitboard's own.
pub fn section(code: &str) -> Option<ProviderId> {
    ProviderId::ALL
        .iter()
        .copied()
        .filter(|tool| *tool != crate::label::DEFAULT)
        .find(|tool| {
            code.strip_prefix(tool.code())
                .is_some_and(|rest| rest.starts_with('_'))
        })
}

// Claude Code's codes as they always were, every other tool's under its own prefix.
fn account_code(
    provider: ProviderId,
    claude: &'static str,
    codex: &'static str,
    desktop: &'static str,
) -> &'static str {
    match provider {
        ProviderId::Claude => claude,
        ProviderId::Codex => codex,
        ProviderId::Desktop => desktop,
    }
}

fn read_from(tool: ProviderId, os: Os) -> &'static str {
    crate::assumptions::verified_against(tool, crate::assumptions::Platform::of(os))
}

/// The checks, and where Claude Code's files were found, for a program to read.
#[derive(Debug)]
pub struct Diagnosis {
    pub checks: Vec<Check>,
    pub environment: Value,
    /// What must not leave this machine in a report a person pastes elsewhere.
    pub redaction: crate::redact::Sheet,
}

pub(crate) fn run(ctx: &Context, moment: &dyn Fn(i64, i64) -> String) -> Diagnosis {
    if let Err(Error::HomeNotAbsolute { variable, path }) = home::check_absolute(ctx) {
        return unplaced(ctx, variable, &path);
    }
    let facts = gather(ctx);
    Diagnosis {
        redaction: redact::redaction_for(ctx, &facts),
        checks: evaluate_with(&facts, moment),
        environment: json!({
            "config_file": facts.config_path,
            "storage_dir": facts.storage_dir,
            "credential_service": facts.service,
            "credential_store": facts.backend.as_ref().map_or("unreadable", |b| b.name()),
            "home": facts.home,
            // Beside Claude Code's, so nothing a program already reads moves.
            "codex": {
                "home": facts.codex.home,
                "present": facts.codex.present,
                "backend": facts.codex.backend.word(),
                // Unknown for a store Pitboard does not read, rather than a guess.
                "login_present": (facts.codex.backend == crate::provider::codex::paths::Backend::File)
                    .then_some(facts.codex.auth_access.is_some()),
                "version": facts.codex.version,
            },
            "desktop": {
                "data_folder": facts.desktop_app.folder,
                "present": facts.desktop_app.present,
                "build": facts.desktop_app.build,
            },
        }),
    }
}

// Every other check reads under a home, which a relative one would put wherever doctor runs.
fn unplaced(ctx: &Context, variable: &str, path: &std::path::Path) -> Diagnosis {
    let homes = fail(
        "homes",
        "homes",
        words::not_a_full_path(variable, path),
        words::SET_A_FULL_PATH,
    );
    Diagnosis {
        checks: machine::too_old(ctx.host().floor())
            .into_iter()
            .chain(machine::elevated(
                crate::host::OS,
                ctx.host().elevation(ctx),
                ctx.host().windows_settings().admin_approval,
            ))
            .chain(std::iter::once(homes))
            .collect(),
        environment: json!({}),
        redaction: crate::redact::Sheet::of(
            ctx,
            format!("{}:{}", crate::state::machine_id(), ctx.now_millis()),
        ),
    }
}

/// No check failed: a warning is advice, a failure an assumption that broke.
pub fn healthy(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.level != Level::Fail)
}
