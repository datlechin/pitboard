//! `pitboard doctor`: check, on this machine, that what Pitboard relies on about Claude Code
//! still holds, and say which assumption broke when one has. Gathering is kept apart from
//! judging so every judgement can be tested.
//!
//! Codex has a section of its own, after everything about Claude Code, and only on a
//! machine where Codex has been run or has accounts enrolled. Its checks are coded
//! `codex_...` so a program can tell them from Claude Code's, which read exactly as they did
//! before there was a second tool.

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

use crate::context::Context;
use crate::error::Error;
use crate::host::{Access, Kind, Os};
use crate::provider::ProviderId;
use crate::provider::claude::live as claude_live;
use crate::provider::claude::paths as claude_paths;
use crate::provider::claude::{daemon, slot};
use crate::state::State;
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

/// Everything read from the machine, so judging it touches nothing.
pub(crate) struct Facts {
    /// The system these were read on, which is what decides some of the judgements.
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
    /// What the live login costs against the store's ceiling, where there is one.
    pub(crate) credential_cost: Option<store::Cost>,
    // `PITBOARD_NO_ARGV=1`, which nothing on Windows has an argument line to refuse.
    pub(crate) no_argv: bool,
    /// What is taking up the room, largest first: (what it is, bytes).
    pub(crate) credential_parts: Vec<(String, usize)>,
    pub(crate) home: PathBuf,
    pub(crate) home_access: Option<Access>,
    // Each login on disk somebody else can read: its path, whether it is a folder, its access.
    pub(crate) readable_by_others: Vec<(String, Kind, Access)>,
    pub(crate) machine_id_known: bool,
    pub(crate) windows: Option<WindowsHomes>,
    /// `CLAUDE_CODE_HOVER_REST`, which switches on the successor credential backend.
    pub(crate) hover_rest_env: bool,
    /// Claude Code's supervisor daemon, where one has ever run for this slot.
    pub(crate) daemon: Option<daemon::Daemon>,
    /// Names Pitboard wrote down before creating a park and has not resolved yet.
    pub(crate) pending_parks: Vec<String>,
    /// Which Claude Code is installed here, read off disk.
    pub(crate) claude_version: Option<String>,
    // The `claude` program Pitboard finds where it looks for one.
    pub(crate) claude_program: Option<PathBuf>,
    /// Every reason a session here would authenticate as something other than the stored
    /// login, read from settings files as well as from this process's environment.
    pub(crate) auth_overrides: Vec<crate::settings::Override>,
    // Each managed `forceLoginOrgUUID` a session takes, and where, and the enrolled accounts none
    // of them lets Claude Code keep.
    pub(crate) organisation_pins: Vec<String>,
    pub(crate) pinned_out: Vec<String>,
    /// Accounts Pitboard is not asking Anthropic about yet, and for how long: (uuid, seconds).
    pub(crate) asking_held: Vec<(String, i64)>,
    pub(crate) state: Result<State, Error>,
    /// Whose login Claude Code has stored, as Anthropic last said, where the account list
    /// can be read.
    pub(crate) in_use: Option<crate::in_use::Known>,
    /// Each enrolled account's parked login, read back from the vault.
    pub(crate) parks: Vec<ParkFact>,
    pub(crate) interrupted: bool,
    /// The refusal the next change would make over the interrupted switch, where that
    /// change could not finish it and that can be told without asking anybody.
    pub(crate) stuck: Option<String>,
    /// What is read about Codex here, for the section that is about it.
    pub(crate) codex: CodexFacts,
    /// Whether Claude Code is on this machine at all: installed, run once, signed in, or
    /// holding enrolled accounts. A machine that uses only Codex is not told Claude Code is
    /// broken.
    pub claude_present: bool,
    // Claude Desktop's, where an account of it is enrolled or a folder is set aside for it.
    pub desktop: Option<DesktopFacts>,
    /// The daily renewal schedule, where this home has one installed.
    pub(crate) schedule: Option<ScheduleFact>,
    pub(crate) watch_service: Option<WatchServiceFact>,
    /// How Pitboard's requests leave this machine, as its environment says.
    pub(crate) network: NetworkFact,
    /// Whether this is the app's doctor, whose environment is the one the system started the
    /// app with, not a shell's: the schedule's runs are started the same way, and given a
    /// shell's proxy only where it was installed from one.
    pub(crate) in_the_app: bool,
    /// Whether this process runs as the person themselves, as the host says, which is what
    /// the gate every change passes asks.
    pub(crate) elevation: crate::host::Elevation,
    /// Claude Code's fallback file, where it is behind the keychain holding the login in use.
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
        desktop: state
            .as_ref()
            .ok()
            .and_then(|s| desktop::desktop_facts(ctx, s)),
        // Read as the next change reads it, without the one question it may put to a
        // service: `doctor` sends no request.
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

// Each command to run, in backticks, or `None` where Pitboard can say none.
fn commands(said: &[String]) -> Option<String> {
    match said {
        [] => None,
        [one] => Some(format!("`{one}`")),
        [all @ .., last] => Some(format!("`{}` and `{last}`", all.join("`, `"))),
    }
}

// The end of a sentence of advice, from `: ` to its full stop.
fn running(said: &[String]) -> String {
    commands(said).map_or_else(|| ".".into(), |commands| format!(": {commands}."))
}

#[cfg(test)]
pub(crate) fn evaluate(facts: &Facts) -> Vec<Check> {
    evaluate_with(facts, &crate::time::moment)
}

/// Every check, each moment it names written as `moment` writes it, given the moment and when
/// the checks are made.
pub(crate) fn evaluate_with(facts: &Facts, moment: &dyn Fn(i64, i64) -> String) -> Vec<Check> {
    let mut checks = Vec::new();
    // Claude Code's own checks, where there is a Claude Code, or where there is no other
    // tool either: a new machine is told what to do first, as it always was. A machine
    // that uses only Codex is not told to run a program it does not use.
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
    // Claude Code's accounts here; every other tool's go in its own section, so a program
    // reading codes can tell them apart and Claude Code's column is Claude Code's alone.
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
    // Only where there is a Codex to say something about. A machine without it reads exactly
    // as it did before Pitboard knew Codex existed.
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

/// The checks that are about Claude Code's own files and settings, rather than about
/// Pitboard or the machine. Named once, so a new one is added here or is always shown.
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

// The tool whose section a check is in; None for Claude Code's and Pitboard's own.
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

/// The code an account's check goes under: Claude Code's as it always was, and every other
/// tool's in that tool's own namespace, so `codex_` is enough to find everything about
/// Codex.
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

/// A parked login this close to expiring is worth renewing now.
pub(crate) const RENEW_WITHIN: i64 = 3 * 86_400;

/// Whether a parked login is due to be renewed by when its refresh token expires, at
/// `refresh_expires_at`: within `RENEW_WITHIN`, and not yet. One that has expired cannot
/// be renewed, and its account needs a sign-in. A renewal run, doctor's parked login check,
/// the standing `pitboard status` gives an account and the column form of a parked login's
/// life all ask this, so they agree on when a login is due. A run also renews a login whose
/// access token has lapsed, which `switch::renew` decides.
pub fn renewal_due(refresh_expires_at: i64, now: i64) -> bool {
    (1..RENEW_WITHIN).contains(&(refresh_expires_at - now))
}

fn read_from(tool: ProviderId, os: Os) -> &'static str {
    crate::assumptions::verified_against(tool, crate::assumptions::Platform::of(os))
}

/// The checks, and where Claude Code's files were found, for a program to read rather than
/// parse out of the checks' wording.
#[derive(Debug)]
pub struct Diagnosis {
    pub checks: Vec<Check>,
    pub environment: Value,
    /// What must not leave this machine in a report. A person reading their own diagnosis
    /// should see their own email; the thing they paste somewhere else should not carry it.
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
            // Codex's, beside Claude Code's rather than among them, so nothing a program
            // already reads here moves.
            "codex": {
                "home": facts.codex.home,
                "present": facts.codex.present,
                "backend": facts.codex.backend,
                // Unknown for a store Pitboard does not read, rather than a guess.
                "login_present": (facts.codex.backend == "file")
                    .then_some(facts.codex.auth_access.is_some()),
                "version": facts.codex.version,
            },
            "desktop": desktop::desktop_environment(ctx, &facts),
        }),
    }
}

/// What doctor says where a home the environment names is empty or relative
/// ([`crate::home::check_absolute`]): how this process runs, where that fails, and the
/// `homes` check, failed, and nothing else. Every other check reads files under one of the
/// homes, and a relative one would have it read whatever is under the folder doctor was run
/// from.
fn unplaced(ctx: &Context, variable: &str, path: &std::path::Path) -> Diagnosis {
    let holds = if path.as_os_str().is_empty() {
        "empty".to_string()
    } else {
        format!("`{}`, which is not a full path", path.display())
    };
    let homes = fail(
        "homes",
        "homes",
        format!("{variable} is {holds}"),
        "Set it to a full path, or unset it. Pitboard checks nothing else until then: \
         everything it reads is under these folders.",
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

/// No check failed. Warnings are advice; a failure means an assumption broke.
pub fn healthy(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.level != Level::Fail)
}
