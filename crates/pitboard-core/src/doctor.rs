//! `pitboard doctor`: check, on this machine, that what pitboard relies on about Claude Code
//! still holds, and say which assumption broke when one has. Gathering is kept apart from
//! judging so every judgement can be tested.
//!
//! Codex has a section of its own, after everything about Claude Code, and only on a
//! machine where Codex has been run or has accounts enrolled. Its checks are coded
//! `codex_...` so a program can tell them from Claude Code's, which read exactly as they did
//! before there was a second tool.

use crate::context::Context;
use crate::error::Error;
use crate::provider::ProviderId;
use crate::provider::claude::daemon;
use crate::provider::claude::live as claude_live;
use crate::provider::claude::paths as claude;
use crate::provider::claude::slot;
use crate::provider::codex::paths as codex;
use crate::state::{Park, State};
use crate::{home, park, store, switch, time, usage};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

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
pub struct Facts {
    pub security_tool: Option<String>,
    pub config_path: PathBuf,
    pub config: Result<Value, Error>,
    pub identity: Option<claude::Identity>,
    pub service: String,
    pub account: String,
    pub default_slot: bool,
    pub storage_dir: String,
    pub backend: Result<store::Backend, store::Error>,
    pub credential_file: PathBuf,
    pub credential: Result<Option<Value>, store::Error>,
    /// What the live login costs against the store's ceiling, where there is one.
    pub credential_cost: Option<store::Cost>,
    /// What is taking up the room, largest first: (what it is, bytes).
    pub credential_parts: Vec<(String, usize)>,
    pub home: PathBuf,
    pub home_mode: Option<u32>,
    /// Anything on this disk holding a login that somebody other than the owner can read:
    /// (path, mode). Empty on a machine with a keychain and a tidy plaintext fallback,
    /// and the whole security story on a machine without one.
    pub readable_by_others: Vec<(String, u32)>,
    pub machine_id_known: bool,
    /// `CLAUDE_CODE_HOVER_REST`, which switches on the successor credential backend.
    pub hover_rest_env: bool,
    /// Claude Code's supervisor daemon, where one has ever run for this slot.
    pub daemon: Option<daemon::Daemon>,
    /// Names pitboard wrote down before creating a park and has not resolved yet.
    pub pending_parks: Vec<String>,
    /// Which Claude Code is installed here, read off disk.
    pub claude_version: Option<String>,
    /// Every reason a session here would authenticate as something other than the stored
    /// login, read from settings files as well as from this process's environment.
    pub auth_overrides: Vec<crate::settings::Override>,
    /// Accounts pitboard is not asking Anthropic about yet, and for how long: (uuid, seconds).
    pub asking_held: Vec<(String, i64)>,
    pub state: Result<State, Error>,
    /// Each enrolled account's parked login, read back from the vault.
    pub parks: Vec<ParkFact>,
    pub interrupted: bool,
    /// What is read about Codex here, for the section that is about it.
    pub codex: CodexFacts,
    /// Whether Claude Code is on this machine at all: installed, run once, signed in, or
    /// holding enrolled accounts. A machine that uses only Codex is not told Claude Code is
    /// broken.
    pub claude_present: bool,
    pub now: i64,
}

/// What is read about Codex CLI on this machine.
///
/// Read without running it and without touching a keychain item Codex created for itself:
/// its home, its configuration, the one file it keeps its login in by default, and which
/// `codex` processes are running.
pub struct CodexFacts {
    /// `CODEX_HOME`, or `~/.codex`.
    pub home: PathBuf,
    /// Whether that directory exists. It does once Codex has been run here, and not before.
    pub present: bool,
    /// How many Codex accounts pitboard has enrolled.
    pub enrolled: usize,
    /// Where Codex is configured to keep its login, in Codex's own words:
    /// `cli_auth_credentials_store`'s `file`, `keyring`, `auto` or `ephemeral`, or `secrets`
    /// for a keychain store with `[features] secret_auth_storage`, which Codex has no one
    /// word for.
    pub backend: &'static str,
    /// Where the default store keeps it.
    pub auth_file: PathBuf,
    /// That file's mode, where there is such a file.
    pub auth_mode: Option<u32>,
    /// Whose login the file holds, or why that could not be told. `Ok(None)` for no file,
    /// and for a store pitboard does not read.
    pub login: Result<Option<CodexLogin>, CodexLoginTrouble>,
    /// Where the `codex` pitboard would run is, where it is anywhere.
    pub program: Option<PathBuf>,
    /// Which Codex that is, read off the path it is installed at. `None` both where there
    /// is no `codex` and where its path does not say; [`CodexFacts::program`] tells which.
    pub version: Option<String>,
    /// Every `codex` running as this user, by pid. `None` where that could not be asked.
    pub running: Option<Vec<u32>>,
}

/// Whose a Codex login is, as its own ID token says. Nothing in here is a secret: the
/// fingerprint is a handle on the refresh token, never the token.
pub struct CodexLogin {
    pub email: String,
    pub account_id: String,
    /// Empty where the login holds no refresh token.
    pub fingerprint: String,
}

/// Why Codex's login names no account pitboard can handle.
///
/// Two answers rather than one, because they call for different things. A login signed in
/// some way pitboard does not switch, such as with an API key, is somebody's choice and
/// nothing is wrong with it; a login that cannot be read, or that mixes two accounts, is.
#[derive(Debug)]
pub enum CodexLoginTrouble {
    /// Signed in, and not with an account pitboard parks or switches. Says why, in Codex's
    /// terms.
    NotAnAccount(String),
    /// Unreadable, or read and not one account's login.
    Unusable(String),
}

pub struct ParkFact {
    /// Which tool the account belongs to, which is what decides how it is named back to a
    /// person: bare for Claude Code, `codex/work` for Codex.
    pub provider: ProviderId,
    pub label: String,
    /// The name a command on this machine takes for it: qualified where another tool has an
    /// account of the same name, since a bare one would then be ambiguous.
    pub name: String,
    pub active: bool,
    /// When this account was last switched to, where that is recorded.
    pub last_used_at: Option<i64>,
    pub park: Option<Park>,
    /// Why it cannot be read back, if it cannot.
    pub unreadable: Option<String>,
}

impl ParkFact {
    /// The account's name as a command here would take it.
    fn typed(&self) -> String {
        self.name.clone()
    }
}

fn park_facts(ctx: &Context, state: &State) -> Vec<ParkFact> {
    // Who each tool's own record says is signed in, asked once per tool and only of a tool
    // that has accounts here. Offline for every tool: Claude Code's config, a Codex login's
    // own claims. Deciding it from Claude Code's config alone read a signed-in Codex
    // account as one with nothing parked to switch to.
    let recorded: std::collections::BTreeMap<ProviderId, Option<String>> = ProviderId::ALL
        .iter()
        .filter(|&&which| state.accounts.iter().any(|a| a.provider() == which))
        .map(|&which| {
            let found = crate::provider::of(which).recorded_identity(ctx);
            (which, found.map(|id| id.account_id))
        })
        .collect();
    state
        .accounts
        .iter()
        .map(|a| ParkFact {
            provider: a.provider(),
            label: a.label.clone(),
            name: state.typed(&a.key()),
            last_used_at: a.last_used_at,
            // What the account's own tool says, when it says anything. pitboard's own
            // record of its last switch says nothing about a sign-in made elsewhere.
            active: match recorded.get(&a.provider()).and_then(Option::as_deref) {
                Some(uuid) => a.account_uuid == uuid,
                None => state.active_for(a.provider()) == Some(a.label.as_str()),
            },
            park: a.parked.clone(),
            unreadable: a.parked.as_ref().and_then(|p| {
                park::load(ctx, &a.key(), p).err().map(|e| match e {
                    Error::ParkedCredentialMissing { .. } => "missing from the vault".into(),
                    Error::ParkedCredentialCorrupt { detail, .. } => detail,
                    other => other.to_string(),
                })
            }),
        })
        .collect()
}

pub fn gather(ctx: &Context) -> Facts {
    let config = claude::load_config(ctx);
    let state = crate::state::load(ctx);
    let service = claude::live_service(ctx);
    let home = home::dir(ctx);
    let identity = config.as_ref().ok().and_then(claude::identity);
    Facts {
        security_tool: cfg!(target_os = "macos")
            .then(|| store::SECURITY.to_string())
            .filter(|p| std::fs::metadata(p).is_ok()),
        config_path: claude::config_file(ctx),
        identity: identity.clone(),
        config,
        account: slot::account_name(ctx),
        default_slot: claude::is_default_slot(ctx),
        storage_dir: claude::storage_dir(ctx),
        backend: store::resolve(&claude_live::chain(ctx), &service),
        credential_file: claude_live::credential_file(ctx),
        credential_cost: store::read_raw(&claude_live::chain(ctx), &service)
            .ok()
            .flatten()
            .and_then(|raw| store::cost(&claude_live::chain(ctx), &service, &raw)),
        credential_parts: store::read(&claude_live::chain(ctx), &service)
            .ok()
            .flatten()
            .map(|doc| parts_of(&doc))
            .unwrap_or_default(),
        credential: store::read(&claude_live::chain(ctx), &service),
        home_mode: mode_of(&home),
        readable_by_others: loose_logins(ctx),
        home,
        machine_id_known: crate::state::machine_id() != "unknown",
        hover_rest_env: ctx.hover_rest,
        daemon: daemon::read(ctx),
        pending_parks: crate::pending::outstanding(ctx),
        claude_version: claude::installed_version(ctx),
        auth_overrides: crate::settings::overrides(ctx),
        asking_held: crate::budget::holds(ctx),
        parks: state
            .as_ref()
            .map(|s| park_facts(ctx, s))
            .unwrap_or_default(),
        codex: codex_facts(ctx, state.as_ref().ok()),
        claude_present: claude::config_file(ctx).exists()
            || claude::program(ctx).is_some()
            || store::read_raw(&claude_live::chain(ctx), &service)
                .is_ok_and(|found| found.is_some())
            || state.as_ref().is_ok_and(|s| {
                s.accounts
                    .iter()
                    .any(|a| a.provider() == ProviderId::Claude)
            }),
        state,
        interrupted: switch::interrupted(ctx),
        service,
        now: ctx.now(),
    }
}

/// What is taking up the room in a credential document, largest first.
///
/// A login that will not fit is almost never the login: on one real machine the OAuth block
/// was 506 bytes and eleven MCP server tokens were 3679. Saying "8503 of 4032 bytes" leaves
/// a person to guess which of those to do something about, and the answer is in the
/// document pitboard has already read.
fn parts_of(document: &Value) -> Vec<(String, usize)> {
    let weigh = |value: &Value| serde_json::to_string(value).map(|s| s.len()).unwrap_or(0);
    let Some(root) = document.as_object() else {
        return Vec::new();
    };
    let mut parts: Vec<(String, usize)> = root
        .iter()
        .flat_map(|(key, value)| match (key.as_str(), value.as_object()) {
            // The usual culprit, and the one a person can act on server by server.
            ("mcpOAuth", Some(servers)) if servers.len() > 1 => servers
                .iter()
                .map(|(server, held)| (format!("mcpOAuth {server}"), weigh(held)))
                .collect(),
            _ => vec![(key.clone(), weigh(value))],
        })
        .collect();
    parts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    parts
}

fn mode_of(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(std::fs::metadata(path).ok()?.permissions().mode() & 0o777)
}

/// Every file on this machine that holds a usable login and is not private to its owner.
///
/// Claude Code has no keyring backend outside macOS and Windows, so on Linux its own login
/// is a plaintext file it chmods to 0600, and pitboard's parked logins are plaintext files
/// beside it. That is not pitboard weakening anything, but it does mean the only thing
/// between a parked OAuth token and everyone else with an account on the machine is a mode
/// bit, and a mode bit is something a backup restore, a `cp`, an rsync or a careless umask
/// quietly changes. So it is looked at rather than assumed.
fn loose_logins(ctx: &Context) -> Vec<(String, u32)> {
    // Group and other, read or write. Anything there is somebody who is not the owner.
    const SHARED: u32 = 0o077;
    let mut loose = Vec::new();
    let mut look = |path: std::path::PathBuf| {
        if let Some(mode) = mode_of(&path)
            && mode & SHARED != 0
        {
            loose.push((path.display().to_string(), mode));
        }
    };
    look(claude_live::credential_file(ctx));
    let vault = store::vault_dir(ctx);
    look(vault.clone());
    if let Ok(entries) = std::fs::read_dir(&vault) {
        let mut parks: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
        parks.sort();
        for park in parks {
            look(park);
        }
    }
    loose
}

/// What is read about Codex here: its home, its configuration, the file it keeps its login
/// in by default, what is installed and what is running.
fn codex_facts(ctx: &Context, state: Option<&State>) -> CodexFacts {
    let backend = codex::backend(ctx);
    let auth_file = codex::auth_file(ctx);
    let home = codex::home(ctx);
    // The program pitboard would run, as the context names it: an app started from Finder
    // has no shell `PATH` and names it itself, and a test names one of its own.
    let program = crate::provider::find_program(ctx.codex_program());
    CodexFacts {
        present: home.is_dir(),
        enrolled: state.map_or(0, |s| {
            s.accounts
                .iter()
                .filter(|a| a.provider() == ProviderId::Codex)
                .count()
        }),
        backend: backend_name(backend),
        auth_mode: mode_of(&auth_file),
        // Read only from the default store. A keychain item Codex created for itself
        // trusts the `codex` binary alone, and reading it would put a permission prompt in
        // front of somebody who only asked for a diagnosis.
        login: if backend == codex::Backend::File {
            codex_login(ctx)
        } else {
            Ok(None)
        },
        version: program.as_deref().and_then(codex_version),
        program,
        running: codex_processes(ctx),
        auth_file,
        home,
    }
}

/// Codex's own word for where it keeps its login, as `cli_auth_credentials_store` spells
/// it in `config.toml`.
///
/// Every store by name, with no catch-all: a store this build does not know is a compile
/// error here, rather than a report calling it something the configuration does not say.
fn backend_name(backend: codex::Backend) -> &'static str {
    match backend {
        codex::Backend::File => "file",
        codex::Backend::Keyring => "keyring",
        codex::Backend::Either => "auto",
        codex::Backend::Ephemeral => "ephemeral",
        // A keychain store with `[features] secret_auth_storage`: an encrypted file whose key
        // is in the keychain. Codex has no one word for it, so it gets the name of the
        // directory it keeps the file in.
        codex::Backend::Secrets => "secrets",
    }
}

/// Whose login Codex's file holds, from the login's own ID token and with no network call.
fn codex_login(ctx: &Context) -> Result<Option<CodexLogin>, CodexLoginTrouble> {
    let tool = crate::provider::of(ProviderId::Codex);
    let unusable = |e: crate::provider::ProviderError| CodexLoginTrouble::Unusable(e.to_string());
    let Some(credential) = tool.read_live(ctx).map_err(unusable)? else {
        return Ok(None);
    };
    // Whether it is one account's login at all, which is the question a park asks of it.
    tool.slice(&credential.raw).map_err(|e| match e {
        crate::provider::ProviderError::Unsupported { reason, .. } => {
            CodexLoginTrouble::NotAnAccount(reason)
        }
        other => unusable(other),
    })?;
    let found = tool.identify(ctx, &credential).map_err(unusable)?;
    Ok(Some(CodexLogin {
        email: found.email,
        account_id: found.account_id,
        fingerprint: tool.fingerprint(&credential.raw),
    }))
}

/// Which Codex `program` is, read off the path it resolves to and never by running it.
///
/// Running `codex --version` would start the program this is trying to describe. Each way
/// Codex is installed puts the version within two directories of the program it resolves
/// to, and only there is looked at: the standalone installer's
/// `releases/0.154.0-<target>/bin/codex`, Homebrew's `Caskroom/codex/0.154.0/`, and npm's
/// `@openai/codex/package.json` beside the `bin` it runs from. Directory names are read
/// before any file is opened, so a standalone install inside `~/.codex` is named without
/// reading anything in it, and nothing further up the path is ever read.
fn codex_version(program: &std::path::Path) -> Option<String> {
    let resolved = std::fs::canonicalize(program).ok()?;
    let near: Vec<&std::path::Path> = resolved.ancestors().skip(1).take(2).collect();
    near.iter()
        .find_map(|dir| {
            dir.file_name()
                .and_then(|n| n.to_str())
                .and_then(|name| name.split('-').next())
                .filter(|leading| looks_like_a_version(leading))
                .map(str::to_owned)
        })
        .or_else(|| {
            near.iter()
                .find_map(|dir| codex_package_version(&dir.join("package.json")))
        })
}

fn looks_like_a_version(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn codex_package_version(path: &std::path::Path) -> Option<String> {
    let json: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if json.get("name")?.as_str()? != "@openai/codex" {
        return None;
    }
    Some(json.get("version")?.as_str()?.to_string())
}

/// Every `codex` running as this user, by pid.
///
/// Asked of `pgrep`, with a deadline, because macOS offers no way to list processes that
/// does not mean either a helper or a system call pitboard does not otherwise make.
#[cfg(target_os = "macos")]
fn codex_processes(ctx: &Context) -> Option<Vec<u32>> {
    let mut command = std::process::Command::new("/usr/bin/pgrep");
    command.arg("-x");
    if let Some(user) = ctx.user.as_deref().filter(|u| !u.is_empty()) {
        command.args(["-u", user]);
    }
    command.arg("codex");
    let out =
        crate::process::output_within(command, b"", std::time::Duration::from_secs(2)).ok()?;
    match out.status.code() {
        // Found some, or found none: both answers.
        Some(0 | 1) => Some(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|line| line.trim().parse().ok())
                .collect(),
        ),
        _ => None,
    }
}

/// Every `codex` running as this user, by pid, read out of `/proc`.
#[cfg(not(target_os = "macos"))]
fn codex_processes(_ctx: &Context) -> Option<Vec<u32>> {
    use std::os::unix::fs::MetadataExt;
    let me = std::fs::metadata("/proc/self").ok()?.uid();
    let mut found: Vec<u32> = std::fs::read_dir("/proc")
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let owner = entry.metadata().ok()?.uid();
            let name = std::fs::read_to_string(entry.path().join("comm")).ok()?;
            (owner == me && name.trim() == "codex").then_some(pid)
        })
        .collect();
    found.sort_unstable();
    Some(found)
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

pub fn evaluate(facts: &Facts) -> Vec<Check> {
    let mut checks = Vec::new();
    // Claude Code's own checks, where there is a Claude Code, or where there is no other
    // tool either: a new machine is told what to do first, as it always was. A machine
    // that uses only Codex is not told to run a program it does not use.
    let codex_here = facts.codex.present || facts.codex.enrolled > 0;
    let claude_here = facts.claude_present || !codex_here;

    if cfg!(target_os = "macos") {
        checks.push(match &facts.security_tool {
            Some(path) => ok("security_tool", "security tool", path.clone()),
            None => fail(
                "security_tool",
                "security tool",
                format!("{} is missing", store::SECURITY),
                "pitboard reads the keychain the same way Claude Code does. Without it, nothing works.",
            ),
        });
    }

    checks.push(match &facts.config {
        Ok(v) => ok(
            "config_file",
            "config file",
            format!(
                "{}  ({} keys)",
                facts.config_path.display(),
                v.as_object().map_or(0, serde_json::Map::len)
            ),
        ),
        Err(e) => fail(
            "config_file",
            "config file",
            e.to_string(),
            "Run `claude` once.",
        ),
    });

    checks.push(match &facts.identity {
        Some(id) => ok(
            "identity",
            "identity",
            format!("{}  ·  org {}", id.email, id.organization_uuid),
        ),
        None => warn(
            "identity",
            "identity",
            "Claude Code has not recorded a signed-in account",
            "It writes this after its first successful call. Run `claude` once.",
        ),
    });

    checks.push(ok(
        "slot",
        "slot",
        if facts.default_slot {
            format!(
                "default  ·  {}  ·  account {}",
                facts.service, facts.account
            )
        } else {
            format!(
                "{}  ·  account {}  (selected by {})",
                facts.service, facts.account, facts.storage_dir
            )
        },
    ));

    // A login that grows past the ceiling cannot be switched at all, and it grows by things
    // done elsewhere, so it is worth saying before the day it refuses.
    if let Some(price) = facts.credential_cost {
        let (bytes, limit) = (price.needs, price.limit);
        checks.push(if price.refused() {
            // The refusal was asked for. Say what it will cost when the day comes rather
            // than only on the day itself.
            fail(
                "credential_size",
                "login size",
                format!(
                    "{bytes} of {limit} bytes, and PITBOARD_NO_ARGV refuses that{}",
                    biggest(&facts.credential_parts)
                ),
                "There is no third way to write a login this size. Sign out of MCP servers \
                 you no longer use to make it smaller, or unset PITBOARD_NO_ARGV and accept \
                 the argument-line write.",
            )
        } else if price.over() {
            // Not a fault: `security` takes this much of a command from stdin and no more,
            // and the argument line is the only other way it offers. Claude Code writes
            // this same login that way itself on every refresh.
            warn(
                "credential_size",
                "login size",
                format!(
                    "{bytes} of {limit} bytes: written on the argument line{}",
                    biggest(&facts.credential_parts)
                ),
                "A login this size can only be written by passing it as an argument, where \
                 a process running as you could read it while the call lasts. Signing out \
                 of MCP servers you no longer use makes it smaller; PITBOARD_NO_ARGV=1 \
                 refuses the switch instead.",
            )
        } else {
            ok(
                "credential_size",
                "login size",
                format!("{bytes} of {limit} bytes"),
            )
        });
    }

    checks.push(match &facts.backend {
        Ok(store::Backend::Keychain) => ok("credential_store", "credential store", "keychain"),
        Ok(store::Backend::File) => {
            let detail = format!("plaintext file  ·  {}", facts.credential_file.display());
            if cfg!(target_os = "macos") {
                warn(
                    "credential_store",
                    "credential store",
                    detail,
                    "Claude Code fell back to a file, which means a keychain write failed at some point.",
                )
            } else {
                ok("credential_store", "credential store", detail)
            }
        }
        // The one wrong diagnosis in this file. If Claude Code's config names somebody as
        // signed in, "nothing is signed in" is not an observation, it is pitboard looking
        // in the wrong place, and it is the failure that would follow Claude Code moving
        // where it keeps a login.
        Ok(store::Backend::Absent) if facts.identity.is_some() => fail(
            "credential_store",
            "credential store",
            format!(
                "Claude Code's config says {} is signed in, and no store pitboard reads \
                 holds that login",
                facts
                    .identity
                    .as_ref()
                    .map(|i| i.email.as_str())
                    .unwrap_or("somebody")
            ),
            "pitboard will not write a login where nobody reads it. Check for a pitboard \
             update; if there is none, this is worth reporting.",
        ),
        Ok(store::Backend::Absent) => warn(
            "credential_store",
            "credential store",
            "no credential in any backend",
            "Nothing is signed in for this slot.",
        ),
        Err(e) => fail(
            "credential_store",
            "credential store",
            e.to_string(),
            "Treat this as unknown, never as empty.",
        ),
    });

    checks.push(judge_credential(facts));

    checks.push(
        match (
            facts
                .config
                .as_ref()
                .ok()
                .and_then(usage::from_config_cache),
            &facts.identity,
        ) {
            (Some(s), Some(id)) if s.account_uuid.as_deref() == Some(id.account_uuid.as_str()) => {
                ok(
                    "usage_cache",
                    "usage cache",
                    format!("{} windows, measured for this account", s.windows.len()),
                )
            }
            (Some(_), Some(_)) => warn(
                "usage_cache",
                "usage cache",
                "cached for a different account",
                "Its numbers are ignored rather than shown, which is why status may look empty.",
            ),
            (Some(s), None) => ok(
                "usage_cache",
                "usage cache",
                format!("{} windows", s.windows.len()),
            ),
            (None, _) => ok(
                "usage_cache",
                "usage cache",
                "absent; Claude Code writes it after a call that reports usage",
            ),
        },
    );

    checks.push(match facts.home_mode {
        None => ok(
            "home",
            "pitboard home",
            format!("{} (not created yet)", facts.home.display()),
        ),
        Some(0o700) => ok("home", "pitboard home", facts.home.display().to_string()),
        Some(mode) => warn(
            "home",
            "pitboard home",
            format!("{} is mode {mode:o}", facts.home.display()),
            format!(
                "Park names contain account identifiers, so only you should read it: \
                 `chmod 700 {}`.",
                facts.home.display()
            ),
        ),
    });

    checks.push(if facts.readable_by_others.is_empty() {
        ok(
            "private_on_disk",
            "logins on disk",
            "nothing holding a login is readable by anyone else",
        )
    } else {
        let names: Vec<String> = facts
            .readable_by_others
            .iter()
            .map(|(path, mode)| format!("{path} is mode {mode:o}"))
            .collect();
        fail(
            "private_on_disk",
            "logins on disk",
            names.join("; "),
            format!(
                "These hold usable OAuth tokens in plain text, which is how Claude Code                  stores them where there is no keychain. Anyone else on this machine can                  read them: `chmod go-rwx {}`.",
                facts
                    .readable_by_others
                    .iter()
                    .map(|(path, _)| path.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        )
    });

    checks.push(match &facts.state {
        Ok(state) => ok(
            "state",
            "accounts",
            match state.accounts.len() {
                0 => "none enrolled yet".to_string(),
                1 => "1 enrolled".to_string(),
                n => format!("{n} enrolled"),
            },
        ),
        Err(e) => fail(
            "state",
            "accounts",
            e.to_string(),
            match e {
                // The one unreadable state that has a command of its own.
                Error::StateWrongMachine { .. } => {
                    "Run `pitboard adopt` to keep these accounts on this computer. The \
                     logins they came with are dropped, because a login belongs to the \
                     computer that signed in."
                }
                _ => "pitboard will not switch until its account list can be read.",
            },
        ),
    });
    // Claude Code's accounts here; every other tool's go in its own section, so a program
    // reading codes can tell them apart and Claude Code's column is Claude Code's alone.
    let (claude_parks, codex_parks): (Vec<&ParkFact>, Vec<&ParkFact>) = facts
        .parks
        .iter()
        .partition(|p| p.provider == ProviderId::Claude);
    checks.extend(claude_parks.iter().map(|p| judge_park(p, facts.now)));
    if facts.interrupted {
        checks.push(warn(
            "interrupted_switch",
            "interrupted switch",
            "a switch did not finish",
            "The next `pitboard use`, `enroll` or `forget` finishes it before anything else.",
        ));
    }
    if let Ok(state) = &facts.state
        && !state.discarded.is_empty()
    {
        checks.push(warn(
            "discarded",
            "old parked logins",
            format!("{} waiting to be deleted", state.discarded.len()),
            "pitboard deletes them on its next change; if they stay, check the keychain is unlocked.",
        ));
    }

    if !facts.machine_id_known {
        checks.push(warn(
            "machine_id",
            "machine id",
            "this machine has no stable identifier",
            "pitboard cannot tell this machine from another that also lacks one, so it cannot \
             refuse state copied between them. Never copy ~/.pitboard between machines.",
        ));
    }

    checks.push(judge_storage_v5(facts));
    checks.push(judge_daemon(facts));
    checks.push(judge_pending(facts));
    checks.push(judge_claude_version(facts));
    checks.push(judge_auth(facts));
    checks.push(judge_asking(facts));
    checks.extend(
        claude_parks
            .iter()
            .filter_map(|p| judge_dormant(p, facts.now)),
    );
    // Only where there is a Codex to say something about. A machine that has never run it
    // reads exactly as it did before pitboard knew Codex existed.
    if codex_here || !codex_parks.is_empty() {
        checks.extend(judge_codex(&facts.codex, &codex_parks, facts.now));
    }
    if !claude_here {
        checks.retain(|check| !CLAUDE_CODES_OWN.contains(&check.code));
    }
    checks
}

/// The checks that are about Claude Code's own files and settings, rather than about
/// pitboard or the machine. Named once, so a new one is added here or is always shown.
const CLAUDE_CODES_OWN: &[&str] = &[
    "config_file",
    "identity",
    "slot",
    "credential_size",
    "credential_store",
    "credential",
    "usage_cache",
    "storage_v5",
    "daemon",
    "claude_version",
    "auth",
];

/// The code an account's check goes under: Claude Code's as it always was, and every other
/// tool's in that tool's own namespace, so `codex_` is enough to find everything about
/// Codex.
fn account_code(provider: ProviderId, claude: &'static str, codex: &'static str) -> &'static str {
    match provider {
        ProviderId::Claude => claude,
        ProviderId::Codex => codex,
    }
}

/// A refresh token's own life, from a real renewal answer: thirty days. An account that has
/// been parked for longer than that without being switched to has had its login kept alive
/// purely by pitboard, through at least one whole token lifetime, for nobody.
const A_TOKEN_LIFETIME: i64 = 30 * 86_400;

/// An account nobody has come back to.
///
/// pitboard renews a parked login for as long as the account is enrolled, so one enrolled
/// once and never used again keeps a live, continuously rotated refresh token on this
/// machine indefinitely. That is a defensible thing to do and an indefensible thing to do
/// silently. Nothing is dropped on a timer pitboard chose: the threshold here is the
/// token's own lifetime, and all it does is say so.
fn judge_dormant(park: &ParkFact, now: i64) -> Option<Check> {
    if park.active || park.park.is_none() {
        return None;
    }
    let dormant_for = now - park.last_used_at?;
    if dormant_for < A_TOKEN_LIFETIME {
        return None;
    }
    Some(warn(
        account_code(park.provider, "dormant_account", "codex_dormant_account"),
        format!("account {}", park.typed()),
        format!(
            "not switched to for {}; pitboard has kept its login alive that whole time",
            time::span(dormant_for)
        ),
        format!(
            "Every `pitboard` renews it, so its refresh token is rotated and kept live on \
             this machine for as long as it stays enrolled. If you are not coming back to \
             it, `pitboard forget {}` deletes the login and the record.",
            park.typed()
        ),
    ))
}

fn judge_credential(facts: &Facts) -> Check {
    match &facts.credential {
        Ok(Some(doc)) => {
            let keys: Vec<&str> = doc
                .as_object()
                .map(|o| o.keys().map(String::as_str).collect())
                .unwrap_or_default();
            // What `/logout` leaves: the account's keys gone, the machine's still there. That
            // is nobody signed in, which the switch and enrolment already read it as.
            if doc.get("claudeAiOauth").is_none() {
                return warn(
                    "credential",
                    "credential",
                    format!("signed out; the document keeps only {keys:?}"),
                    "Nothing is signed in for this slot.",
                );
            }
            let Some(oauth) = doc.get("claudeAiOauth").and_then(Value::as_object) else {
                return fail(
                    "credential",
                    "credential",
                    format!("claudeAiOauth is not an object; the keys are {keys:?}"),
                    "The credential's shape changed. Do not switch accounts until this is understood.",
                );
            };
            let fingerprint = oauth
                .get("refreshToken")
                .and_then(Value::as_str)
                .map(store::fingerprint)
                .unwrap_or_else(|| "none".into());
            let days = oauth
                .get("refreshTokenExpiresAt")
                .and_then(Value::as_i64)
                .map_or(-1, |ms| (ms / 1000 - facts.now) / 86_400);
            let detail = format!("refresh {fingerprint}  ·  {days} days left  ·  keys {keys:?}");
            if days < 3 {
                warn(
                    "credential",
                    "credential",
                    detail,
                    "This login expires soon and will need signing in again.",
                )
            } else {
                ok("credential", "credential", detail)
            }
        }
        Ok(None) => warn(
            "credential",
            "credential",
            "nothing stored",
            "Nothing is signed in for this slot.",
        ),
        Err(e) => fail(
            "credential",
            "credential",
            e.to_string(),
            "Do not write to the store while this is failing.",
        ),
    }
}

/// A parked login this close to expiring is worth renewing now.
pub const RENEW_WITHIN: i64 = 3 * 86_400;

fn judge_park(fact: &ParkFact, now: i64) -> Check {
    let code = account_code(fact.provider, "parked_login", "codex_parked_login");
    let name = format!("account {}", fact.typed());
    let renew = format!("Run `pitboard enroll {} --sign-in`.", fact.typed());
    let Some(park) = &fact.park else {
        return if fact.active {
            ok(code, name, "signed in; parked when you switch away")
        } else {
            warn(code, name, "nothing parked to switch to", renew)
        };
    };
    if let Some(why) = &fact.unreadable {
        return fail(
            code,
            name,
            format!("its parked login is unusable: {why}"),
            renew,
        );
    }
    match park.refresh_expires_at {
        Some(at) if at <= now => warn(
            code,
            name,
            format!("its parked login expired {}", time::moment(at, now)),
            renew,
        ),
        Some(at) if at - now < RENEW_WITHIN => warn(
            code,
            name,
            format!("its parked login expires in {}", time::span(at - now)),
            renew,
        ),
        Some(at) => ok(
            code,
            name,
            format!("parked, good for {}", time::span(at - now)),
        ),
        None => ok(code, name, "parked"),
    }
}

/// Claude Code's successor credential backend.
///
/// Measured in 2.1.278: the flag does not move the login out of the keychain. The live
/// chain is built as keychain-with-plaintext-fallback either way, and the flag only decides
/// what backs the fallback half, and only for a caller that hands a backend in. An ordinary
/// `claude` hands none in, so the fallback stays `<storage dir>/.credentials.json`. The
/// combination worth saying something about is the flag on *and* the login living in the
/// fallback, because that is the one case where what pitboard reads may not be what a
/// session reads.
fn judge_storage_v5(facts: &Facts) -> Check {
    let flag_on = facts
        .config
        .as_ref()
        .ok()
        .and_then(|c| c.get("cachedGrowthBookFeatures"))
        .and_then(|f| f.get("tengu_hover_rest"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !(facts.hover_rest_env || flag_on) {
        return ok("storage_v5", "storage v5", "inactive");
    }
    match facts.backend {
        Ok(store::Backend::File) => warn(
            "storage_v5",
            "storage v5",
            "switched on, and this login is in the fallback store",
            "pitboard reads the plaintext file. If Claude Code was given a backend of its \
             own, that is not the same file. Check for an update before switching.",
        ),
        _ => ok(
            "storage_v5",
            "storage v5",
            "switched on; the keychain is still where the login is",
        ),
    }
}

/// The three things taking up the most room, for a check that would otherwise leave a
/// person guessing which of them to do something about.
fn biggest(parts: &[(String, usize)]) -> String {
    let named: Vec<String> = parts
        .iter()
        .take(3)
        .map(|(what, bytes)| format!("{what} {bytes}"))
        .collect();
    if named.is_empty() {
        String::new()
    } else {
        format!(". Mostly: {}", named.join(", "))
    }
}

/// Whether pitboard is waiting before asking Anthropic about anything.
///
/// Ordinarily nothing is waiting: an account is asked about whenever its tightest limit
/// could have moved by a percentage point, and that is the floor rather than a wait. A wait
/// means Anthropic asked for less traffic or could not be reached, and a person watching a
/// number not move deserves to know which.
fn judge_asking(facts: &Facts) -> Check {
    // Which services pitboard asks, named by the tools that have accounts here, and which
    // of them are being held back: a hold is a service's answer, so blaming the wrong one
    // sends somebody to look at a service that is answering normally.
    let tool_of = |uuid: &str| {
        facts
            .state
            .as_ref()
            .ok()
            .and_then(|s| s.owner_of_park(uuid))
            .map(crate::state::Account::provider)
    };
    let services = |tools: &mut Vec<ProviderId>| {
        tools.sort();
        tools.dedup();
        if tools.is_empty() {
            tools.push(ProviderId::Claude);
        }
        tools
            .iter()
            .map(|t| t.service())
            .collect::<Vec<_>>()
            .join(" and ")
    };
    let mut asked: Vec<ProviderId> = facts
        .state
        .as_ref()
        .map(|s| {
            s.accounts
                .iter()
                .map(crate::state::Account::provider)
                .collect()
        })
        .unwrap_or_default();
    let name = format!("asking {}", services(&mut asked));
    let mut holding: Vec<ProviderId> = facts
        .asking_held
        .iter()
        .filter_map(|(uuid, _)| tool_of(uuid))
        .collect();
    let holders = services(&mut holding);
    match facts.asking_held.len() {
        0 => ok("asking", name, "nothing is being held back"),
        n => {
            let longest = facts
                .asking_held
                .iter()
                .map(|(_, seconds)| *seconds)
                .max()
                .unwrap_or_default();
            warn(
                "asking",
                name,
                format!(
                    "{n} account(s) not being asked about for up to {}",
                    time::span(longest)
                ),
                format!(
                    "{holders} asked for less traffic, or could not be reached. The numbers \
                     shown are the last ones measured until then; `pitboard status --fresh` \
                     does not override a wait {holders} asked for."
                ),
            )
        }
    }
}

/// Whether moving the stored login would change anything a session sees.
///
/// Claude Code resolves this from layered settings, so a managed policy or a line in a
/// person's own `settings.json` can make every switch pitboard performs a no-op. Read from
/// files rather than from this process's environment, because an app launched from Finder
/// has no environment to read and is the surface most likely to be used on a machine that
/// needs the answer.
fn judge_auth(facts: &Facts) -> Check {
    if facts.auth_overrides.is_empty() {
        return ok(
            "auth_source",
            "what a session authenticates with",
            "the stored login, which is what pitboard moves",
        );
    }
    let named: Vec<String> = facts
        .auth_overrides
        .iter()
        .map(ToString::to_string)
        .collect();
    warn(
        "auth_source",
        "what a session authenticates with",
        format!("something else: {}", named.join("; ")),
        "Claude Code here authenticates with that rather than with the stored login, so \
         switching accounts changes nothing a session would notice. Remove it, or accept \
         that pitboard is moving a login nothing reads.",
    )
}

/// Which Claude Code is installed, against which one pitboard's facts were read.
///
/// Stated rather than warned about. Claude Code ships several times a week, so a mismatch
/// is the ordinary state of the world within days of a release and warning about it would
/// be noise on every machine. What is worth a warning is an assumption that has actually
/// stopped holding, which is a probe's job and not a version number's.
fn judge_claude_version(facts: &Facts) -> Check {
    let verified = crate::provider::claude::assumptions::VERIFIED_AGAINST;
    match facts.claude_version.as_deref() {
        None => ok(
            "claude_version",
            "Claude Code build",
            format!("not found here; pitboard's facts were read from {verified}"),
        ),
        Some(installed) if installed == verified => ok(
            "claude_version",
            "Claude Code build",
            format!("{installed}, which is what pitboard's facts were read from"),
        ),
        Some(installed) => ok(
            "claude_version",
            "Claude Code build",
            format!("{installed} installed; pitboard's facts were read from {verified}"),
        ),
    }
}

/// A name written down before a park was created, still unresolved. Ordinarily there is
/// nothing here: the next change resolves every one of them. What is left is an item that
/// could not be read, which on macOS is a locked keychain and nothing worse.
fn judge_pending(facts: &Facts) -> Check {
    match facts.pending_parks.len() {
        0 => ok("pending_parks", "parks being reclaimed", "none outstanding"),
        n => warn(
            "pending_parks",
            "parks being reclaimed",
            format!("{n} could not be read this time"),
            "pitboard wrote these names down before creating a login in them and cannot \
             read them back to find out what is there. Unlock the keychain and run any \
             pitboard command; it resolves them before doing anything else.",
        ),
    }
}

/// Claude Code's supervisor daemon is a second writer of the login, on a schedule nobody
/// typed. It takes the same write lock, so it cannot write underneath a switch, but a
/// person reading a diagnosis should be able to see that it is there.
fn judge_daemon(facts: &Facts) -> Check {
    let Some(d) = &facts.daemon else {
        return ok("claude_daemon", "Claude Code daemon", "none has run here");
    };
    let version = d
        .version
        .as_deref()
        .map(|v| format!("Claude Code {v}"))
        .unwrap_or_else(|| "an unrecorded version".into());
    if d.running {
        ok(
            "claude_daemon",
            "Claude Code daemon",
            format!("running, {version}, pid {}", d.pid),
        )
    } else {
        ok(
            "claude_daemon",
            "Claude Code daemon",
            format!("not running; {version} ran here last"),
        )
    }
}

/// Codex's section: where it keeps its login, whether pitboard can read it, its accounts,
/// and what a switch cannot reach.
///
/// Every code starts `codex_`. Three kinds of finding, judged differently:
///
/// - A fault, such as a login nobody can read. It stops pitboard handling an account it has
///   enrolled, so it fails where there are Codex accounts, and is a warning where there are
///   none, because something is wrong with Codex even if nothing pitboard does is broken.
/// - A choice, such as a keychain store or an API key. Nothing is wrong with it. Where Codex
///   accounts are enrolled it is still said: a keychain store puts every one of them out of
///   reach, which fails, and an API key only means there is nothing to switch from until
///   somebody signs in with an account, which is worth a look. Where none are, it is stated
///   and nothing more, so somebody who uses pitboard for Claude Code alone is not handed a
///   warning about a setting they chose and pitboard has no business with.
/// - A fact, such as how many sessions are running, which is only ever stated.
fn judge_codex(facts: &CodexFacts, parks: &[&ParkFact], now: i64) -> Vec<Check> {
    let mut checks = Vec::new();
    let enrolled = facts.enrolled > 0 || !parks.is_empty();
    let broken = |code, name: &str, detail: String, advice: String| {
        if enrolled {
            fail(code, name, detail, advice)
        } else {
            warn(code, name, detail, advice)
        }
    };
    let file = facts.backend == "file";
    let config = facts.home.join("config.toml");
    let described = match facts.backend {
        "ephemeral" => "in memory only (cli_auth_credentials_store = \"ephemeral\")".to_string(),
        "secrets" => "secrets (cli_auth_credentials_store with secret_auth_storage in \
                      config.toml)"
            .to_string(),
        other => format!("{other} (cli_auth_credentials_store in config.toml)"),
    };
    checks.push(match facts.backend {
        "file" => ok(
            "codex_backend",
            "Codex login store",
            format!("file  ·  {}", facts.auth_file.display()),
        ),
        // A choice, and one that leaves nothing pitboard can park or switch.
        other if enrolled => fail(
            "codex_backend",
            "Codex login store",
            described,
            match other {
                "ephemeral" => format!(
                    "Codex keeps nothing at rest, so there is no login pitboard can park or \
                     switch. Remove the setting from {} to use Codex's default file store.",
                    config.display()
                ),
                _ => format!(
                    "pitboard reads only Codex's default file store, auth.json, and will not \
                     touch the keychain item Codex created for itself, because every read of \
                     it would ask you for permission, so no enrolled Codex account can be \
                     switched to. Remove the setting from {} to use the file store, then sign \
                     in with `codex login`.",
                    config.display()
                ),
            },
        ),
        _ => ok(
            "codex_backend",
            "Codex login store",
            format!("{described}; pitboard switches Codex accounts only in the file store"),
        ),
    });

    // The file and what is in it are only worth a word where the file is the store: a
    // keychain store deletes it on purpose.
    if file {
        checks.push(match facts.auth_mode {
            None if enrolled => warn(
                "codex_auth_file",
                "Codex login file",
                format!(
                    "{} is absent: nothing is signed in to Codex",
                    facts.auth_file.display()
                ),
                "Sign in with `codex`, or put an enrolled account back with \
                 `pitboard use codex/<label>`.",
            ),
            None => ok(
                "codex_auth_file",
                "Codex login file",
                "absent; nothing is signed in to Codex",
            ),
            Some(mode) if mode & 0o077 != 0 => warn(
                "codex_auth_file",
                "Codex login file",
                format!("{} is mode {mode:o}", facts.auth_file.display()),
                format!(
                    "It holds a usable login in plain text, and Codex sets 0600 only when it \
                     creates the file, never on a later write: `chmod 600 {}`.",
                    facts.auth_file.display()
                ),
            ),
            Some(mode) => ok(
                "codex_auth_file",
                "Codex login file",
                format!("{}  ·  mode {mode:o}", facts.auth_file.display()),
            ),
        });
        match &facts.login {
            Ok(Some(login)) => checks.push(ok(
                "codex_login",
                "Codex login",
                format!(
                    "{}  ·  account {}  ·  refresh {}",
                    login.email,
                    login.account_id,
                    if login.fingerprint.is_empty() {
                        "none"
                    } else {
                        login.fingerprint.as_str()
                    }
                ),
            )),
            // No file, which the check above has already said.
            Ok(None) => {}
            // Signed in on purpose some way pitboard does not switch. Nothing to sign in
            // again for, and nothing broken.
            Err(CodexLoginTrouble::NotAnAccount(why)) if enrolled => checks.push(warn(
                "codex_login",
                "Codex login",
                why.clone(),
                "pitboard parks and switches Codex's ChatGPT sign-ins, so `pitboard use \
                 codex/<label>` refuses while Codex is signed in this way rather than replace \
                 a login it has nowhere to park. `codex login` signs in with a ChatGPT \
                 account.",
            )),
            Err(CodexLoginTrouble::NotAnAccount(why)) => {
                checks.push(ok("codex_login", "Codex login", why.clone()));
            }
            Err(CodexLoginTrouble::Unusable(why)) => checks.push(broken(
                "codex_login",
                "Codex login",
                format!("it cannot be used: {why}"),
                "pitboard will not park or switch a Codex login it cannot read as one \
                 account's. Signing in with `codex` again writes a fresh one."
                    .into(),
            )),
        }
    }

    // Each Codex account, the way Claude Code's are judged, in this section and under this
    // section's codes.
    checks.extend(parks.iter().map(|p| judge_park(p, now)));
    checks.extend(parks.iter().filter_map(|p| judge_dormant(p, now)));

    checks.push(judge_codex_version(facts));
    checks.push(ok(
        "codex_running",
        "running Codex",
        match facts.running.as_deref() {
            None => "could not tell".to_string(),
            Some([]) => "none".to_string(),
            Some(pids) => format!(
                "{} running (pid {}); each keeps using the account it started with until \
                 it is restarted",
                pids.len(),
                some_of(pids)
            ),
        },
    ));
    checks
}

/// A few pids and how many more, because somebody with twenty sessions open needs to know
/// there are twenty, not which twenty.
fn some_of(pids: &[u32]) -> String {
    const SHOWN: usize = 3;
    let named: Vec<String> = pids.iter().take(SHOWN).map(u32::to_string).collect();
    match pids.len().saturating_sub(SHOWN) {
        0 => named.join(", "),
        more => format!("{} and {more} more", named.join(", ")),
    }
}

/// Which Codex is installed, against which one pitboard's facts were read. Stated rather
/// than warned about, for the reason Claude Code's is.
fn judge_codex_version(facts: &CodexFacts) -> Check {
    let verified = crate::provider::codex::assumptions::VERIFIED_AGAINST;
    ok(
        "codex_version",
        "Codex build",
        match (facts.program.as_deref(), facts.version.as_deref()) {
            (None, _) => format!("not found here; pitboard's facts were read from {verified}"),
            // Installed some way that does not put the version in its path, such as behind
            // a version manager's shim. Found is not the same as missing.
            (Some(program), None) => format!(
                "{}, whose path does not say which version it is; pitboard's facts were \
                 read from {verified}",
                program.display()
            ),
            (Some(_), Some(installed)) if installed == verified => {
                format!("{installed}, which is what pitboard's facts were read from")
            }
            (Some(_), Some(installed)) => {
                format!("{installed} installed; pitboard's facts were read from {verified}")
            }
        },
    )
}

/// The checks, and where Claude Code's files were found, for a program to read rather than
/// parse out of the checks' wording.
pub struct Diagnosis {
    pub checks: Vec<Check>,
    pub environment: Value,
    /// What must not leave this machine in a report. A person reading their own diagnosis
    /// should see their own email; the thing they paste somewhere else should not carry it.
    pub redaction: crate::redact::Sheet,
}

pub fn run(ctx: &Context) -> Diagnosis {
    let facts = gather(ctx);
    Diagnosis {
        redaction: redaction_for(ctx, &facts),
        checks: evaluate(&facts),
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
                // Unknown for a store pitboard does not read, rather than a guess.
                "login_present": (facts.codex.backend == "file")
                    .then_some(facts.codex.auth_mode.is_some()),
                "version": facts.codex.version,
            },
        }),
    }
}

/// Everything in a diagnosis that names a person or an account.
///
/// The salt is this machine and this moment, so identifiers line up inside one report and
/// two reports from one machine do not line up with each other. It is never printed.
fn redaction_for(ctx: &Context, facts: &Facts) -> crate::redact::Sheet {
    let mut sheet = crate::redact::Sheet::new(
        format!("{}:{}", crate::state::machine_id(), ctx.now_millis()),
        ctx.home().to_string_lossy(),
    )
    .hide(facts.account.clone(), "user");

    if let Some(id) = &facts.identity {
        sheet = sheet
            .hide(id.email.clone(), "email")
            .hide(id.account_uuid.clone(), "account")
            .hide(id.organization_uuid.clone(), "org");
        if let Some(name) = &id.organization_name {
            sheet = sheet.hide(name.clone(), "org");
        }
    }
    if let Ok(state) = &facts.state {
        for account in &state.accounts {
            sheet = sheet
                .hide(account.email.clone(), "email")
                .hide(account.account_uuid.clone(), "account")
                .hide(
                    account
                        .claude()
                        .map_or_else(String::new, |c| c.organization_uuid.to_string()),
                    "org",
                );
            // A Codex account's workspace names the organisation it belongs to.
            if let crate::state::Detail::Codex {
                workspace_id: Some(workspace),
                ..
            } = &account.detail
            {
                sheet = sheet.hide(workspace.clone(), "org");
            }
        }
    }
    // Codex's live login, which need not be an account anybody enrolled.
    if let Ok(Some(login)) = &facts.codex.login {
        sheet = sheet
            .hide(login.email.clone(), "email")
            .hide(login.account_id.clone(), "account")
            .hide(login.fingerprint.clone(), "login");
    }
    // A fingerprint is not a token, and it still identifies one login across reports.
    if let Ok(Some(doc)) = &facts.credential
        && let Some(fingerprint) = doc
            .get("claudeAiOauth")
            .map(crate::provider::claude::document::fingerprint_of)
            .filter(|f| !f.is_empty())
    {
        sheet = sheet.hide(fingerprint, "login");
    }
    for park in &facts.parks {
        if let Some(held) = &park.park {
            sheet = sheet
                .hide(held.refresh_fingerprint.clone(), "login")
                .hide(held.service.clone(), "park");
        }
    }
    sheet
}

/// No check failed. Warnings are advice; a failure means an assumption broke.
pub fn healthy(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.level != Level::Fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            security_tool: Some("/usr/bin/security".into()),
            config_path: PathBuf::from("/home/x/.claude.json"),
            config: Ok(json!({"oauthAccount": {}})),
            identity: Some(claude::Identity {
                email: "a@b.c".into(),
                account_uuid: "acc".into(),
                organization_uuid: "org".into(),
                organization_name: None,
                subscription: None,
                rate_limit_tier: None,
            }),
            credential_parts: Vec::new(),
            credential_cost: Some(store::Cost {
                needs: 900,
                limit: 4032,
                second_route: true,
            }),
            service: "Claude Code-credentials".into(),
            account: "someone".into(),
            default_slot: true,
            storage_dir: "/home/x/.claude".into(),
            backend: Ok(store::Backend::Keychain),
            credential_file: PathBuf::from("/home/x/.claude/.credentials.json"),
            credential: Ok(Some(json!({"claudeAiOauth": {
                "refreshToken": "r",
                "refreshTokenExpiresAt": 2_000_000_000_000i64
            }}))),
            home: PathBuf::from("/home/x/.pitboard"),
            home_mode: Some(0o700),
            readable_by_others: Vec::new(),
            machine_id_known: true,
            hover_rest_env: false,
            daemon: None,
            pending_parks: Vec::new(),
            claude_version: Some(crate::provider::claude::assumptions::VERIFIED_AGAINST.into()),
            auth_overrides: Vec::new(),
            asking_held: Vec::new(),
            state: Ok(State::default()),
            parks: Vec::new(),
            interrupted: false,
            codex: no_codex(),
            claude_present: true,
            now: NOW,
        }
    }

    /// A machine that has never run Codex.
    fn no_codex() -> CodexFacts {
        CodexFacts {
            home: PathBuf::from("/home/x/.codex"),
            present: false,
            enrolled: 0,
            backend: "file",
            auth_file: PathBuf::from("/home/x/.codex/auth.json"),
            auth_mode: None,
            login: Ok(None),
            program: None,
            version: None,
            running: Some(Vec::new()),
        }
    }

    /// Codex's section for a machine with no Codex accounts parked.
    fn codex_checks(codex: &CodexFacts) -> Vec<Check> {
        judge_codex(codex, &[], NOW)
    }

    /// A machine whose Codex is signed in, with its login where it should be.
    fn with_codex() -> CodexFacts {
        CodexFacts {
            present: true,
            auth_mode: Some(0o600),
            login: Ok(Some(CodexLogin {
                email: "w@example.com".into(),
                account_id: "work-account".into(),
                fingerprint: "0123456789abcdef".into(),
            })),
            program: Some(PathBuf::from("/usr/local/bin/codex")),
            version: Some(crate::provider::codex::assumptions::VERIFIED_AGAINST.into()),
            ..no_codex()
        }
    }

    const NOW: i64 = 1_789_935_600;

    fn parked(label: &str, refresh_expires_at: Option<i64>) -> ParkFact {
        ParkFact {
            provider: ProviderId::Claude,
            last_used_at: None,
            label: label.into(),
            name: crate::state::Key::new(ProviderId::Claude, label).typed(),
            active: false,
            park: Some(Park {
                service: format!("pitboard-park-{label}-1"),
                parked_at: NOW - 86_400,
                refresh_fingerprint: "f".into(),
                access_expires_at: None,
                refresh_expires_at,
            }),
            unreadable: None,
        }
    }

    /// A Codex account's park, named as a command here would take it.
    fn codex_parked(label: &str, refresh_expires_at: Option<i64>) -> ParkFact {
        ParkFact {
            provider: ProviderId::Codex,
            name: crate::state::Key::new(ProviderId::Codex, label).typed(),
            ..parked(label, refresh_expires_at)
        }
    }

    fn named<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks.iter().find(|c| c.name == name).expect(name)
    }

    fn check<'a>(checks: &'a [Check], code: &str) -> &'a Check {
        checks.iter().find(|c| c.code == code).expect(code)
    }

    #[test]
    fn a_healthy_machine_reports_no_failures() {
        let checks = evaluate(&facts());
        assert!(checks.iter().all(|c| c.level != Level::Fail));
        assert!(healthy(&checks));
    }

    /// What the check above is given, read off a real disk.
    ///
    /// Everything else here hands `evaluate` its facts, so nothing until now had ever
    /// looked at a file's mode. A gatherer that returns an empty list whatever the disk
    /// says would pass every one of those tests.
    #[test]
    fn a_world_readable_park_is_found_on_the_disk() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "pitboard-doctor-modes-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _guard = Scratch(root.clone());

        let ctx = Context::new(root.clone()).with_pitboard_home(root.join(".pitboard"));
        let vault = store::vault_dir(&ctx);
        std::fs::create_dir_all(&vault).expect("a vault");
        std::fs::set_permissions(&vault, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            loose_logins(&ctx).is_empty(),
            "a private vault is not loose"
        );

        let park = vault.join("pitboard-park-x.json");
        std::fs::write(&park, "{}").expect("a park");
        std::fs::set_permissions(&park, std::fs::Permissions::from_mode(0o644)).unwrap();
        let found = loose_logins(&ctx);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].1, 0o644);
        assert!(found[0].0.ends_with("pitboard-park-x.json"), "{found:?}");

        std::fs::set_permissions(&park, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(loose_logins(&ctx).is_empty(), "0600 is private");
    }

    /// On a machine with no keychain every parked login is a plaintext OAuth token in a
    /// file, and the only thing between it and everyone else with an account here is a
    /// mode bit. A backup restore, a `cp -r`, an rsync or a careless umask changes one
    /// quietly, and nothing else in pitboard would ever mention it.
    #[test]
    fn a_login_anyone_on_this_machine_can_read_is_a_failure() {
        let mut f = facts();
        assert_eq!(check(&evaluate(&f), "private_on_disk").level, Level::Ok);

        f.readable_by_others = vec![
            ("/home/a/.pitboard/vault/pitboard-park-x.json".into(), 0o644),
            ("/home/a/.claude/.credentials.json".into(), 0o640),
        ];
        let checks = evaluate(&f);
        let found = check(&checks, "private_on_disk");
        assert_eq!(found.level, Level::Fail);
        assert!(found.detail.contains("mode 644"), "{}", found.detail);
        assert!(found.detail.contains("mode 640"), "{}", found.detail);
        // The advice has to be something a person can run, not a description of a problem.
        assert!(found.advice.contains("chmod go-rwx"), "{}", found.advice);
        assert!(!healthy(&checks));
    }

    /// A document whose `claudeAiOauth` is there and is not an object is a shape that
    /// moved, which nothing should be written into until it is understood.
    #[test]
    fn a_credential_whose_login_is_not_an_object_is_a_failure() {
        let mut f = facts();
        f.credential = Ok(Some(json!({"claudeAiOauth": "something else"})));
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "credential").level, Level::Fail);
        assert!(!healthy(&checks));
    }

    #[test]
    fn an_unreadable_store_fails_rather_than_reporting_nothing_stored() {
        let mut f = facts();
        f.credential = Err(store::Error::Unreadable("security exited 1".into()));
        f.backend = Err(store::Error::Unreadable("security exited 1".into()));
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "credential").level, Level::Fail);
        assert_eq!(check(&checks, "credential_store").level, Level::Fail);
    }

    #[test]
    fn a_login_about_to_expire_is_flagged() {
        let mut f = facts();
        f.credential = Ok(Some(json!({"claudeAiOauth": {
            "refreshToken": "r",
            "refreshTokenExpiresAt": (f.now + 86_400) * 1000
        }})));
        assert_eq!(check(&evaluate(&f), "credential").level, Level::Warn);
    }

    #[test]
    fn a_loose_home_directory_is_flagged() {
        let mut f = facts();
        f.home_mode = Some(0o755);
        let checks = evaluate(&f);
        let home = check(&checks, "home");
        assert_eq!(home.level, Level::Warn);
        assert!(home.detail.contains("755"));
    }

    #[test]
    fn the_successor_backend_alone_does_not_move_the_login() {
        let server_on = || {
            let mut f = facts();
            f.config = Ok(json!({"cachedGrowthBookFeatures": {"tengu_hover_rest": true}}));
            f
        };
        let env_on = || {
            let mut f = facts();
            f.hover_rest_env = true;
            f
        };
        for mut f in [server_on(), env_on()] {
            // On the keychain, the flag changes nothing pitboard reads.
            let checks = evaluate(&f);
            assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
            // In the fallback, it is the one case worth saying something about.
            f.backend = Ok(store::Backend::File);
            let checks = evaluate(&f);
            assert_eq!(check(&checks, "storage_v5").level, Level::Warn);
        }
    }

    #[test]
    fn the_successor_backend_is_quiet_when_it_is_off() {
        let mut f = facts();
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
        // The fallback store on its own is not the successor backend.
        f.backend = Ok(store::Backend::File);
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
    }

    /// The failure that would follow Claude Code moving where it keeps a login: not an
    /// absence, a mismatch, and the difference is what decides whether the advice is "sign
    /// in" or "pitboard is looking in the wrong place".
    #[test]
    fn a_config_that_names_somebody_signed_in_with_no_login_anywhere_is_a_failure() {
        let mut f = facts();
        f.backend = Ok(store::Backend::Absent);
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(store.level, Level::Fail);
        assert!(store.detail.contains("a@b.c"));

        // Nobody signed in at all is an ordinary state with an ordinary answer.
        f.identity = None;
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(store.level, Level::Warn);
        assert!(store.advice.contains("Nothing is signed in"));
    }

    #[test]
    fn a_login_past_the_ceiling_reads_differently_when_the_way_past_it_is_refused() {
        let mut f = facts();
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "credential_size").level, Level::Ok);

        f.credential_cost = Some(store::Cost {
            needs: 8503,
            limit: 4032,
            second_route: true,
        });
        let checks = evaluate(&f);
        let written = check(&checks, "credential_size");
        assert_eq!(written.level, Level::Warn);
        assert!(written.detail.contains("argument line"));

        f.credential_cost = Some(store::Cost {
            needs: 8503,
            limit: 4032,
            second_route: false,
        });
        let checks = evaluate(&f);
        let refused = check(&checks, "credential_size");
        assert_eq!(
            refused.level,
            Level::Fail,
            "there is no third way to write it"
        );
        assert!(refused.detail.contains("PITBOARD_NO_ARGV"));
    }

    /// An account nobody has come back to keeps a live, continuously rotated refresh token
    /// on this machine for as long as it stays enrolled. Nothing is dropped on a timer
    /// pitboard chose; the threshold is the token's own lifetime and all it does is say so.
    /// A login that will not fit is almost never the login. On one real machine the OAuth
    /// block was 506 bytes and eleven MCP server tokens were 3679, and the check said
    /// "8503 of 4032 bytes" and left the person to guess which of those to act on.
    #[test]
    fn a_login_too_big_says_what_is_taking_up_the_room() {
        let mut f = facts();
        f.credential_cost = Some(store::Cost {
            needs: 8503,
            limit: 4032,
            second_route: true,
        });
        f.credential_parts = parts_of(&json!({
            "claudeAiOauth": {"refreshToken": "r"},
            "mcpOAuth": {
                "one": {"token": "x".repeat(300)},
                "two": {"token": "y".repeat(200)},
                "three": {"token": "z".repeat(100)},
            },
        }));

        let checks = evaluate(&f);
        let size = check(&checks, "credential_size");
        assert!(size.detail.contains("mcpOAuth one"), "{}", size.detail);
        assert!(size.detail.contains("mcpOAuth two"), "{}", size.detail);
        assert!(
            !size.detail.contains("claudeAiOauth"),
            "three is enough to act on, and the login itself is never the problem: {}",
            size.detail
        );
    }

    #[test]
    fn what_takes_up_the_room_is_listed_largest_first_and_named_per_server() {
        let parts = parts_of(&json!({
            "claudeAiOauth": {"refreshToken": "r"},
            "mcpOAuth": {"small": {"t": "x"}, "large": {"t": "y".repeat(500)}},
        }));
        let names: Vec<&str> = parts.iter().map(|(what, _)| what.as_str()).collect();
        assert_eq!(names[0], "mcpOAuth large", "largest first: {names:?}");
        assert!(names.contains(&"claudeAiOauth"));
        assert!(
            names.contains(&"mcpOAuth small"),
            "each server is named, because that is what a person signs out of"
        );

        // One server is not worth breaking apart; the key says it already.
        let single = parts_of(&json!({"mcpOAuth": {"only": {"t": "x"}}}));
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].0, "mcpOAuth");
    }

    #[test]
    fn an_account_nobody_has_come_back_to_is_said_out_loud() {
        let mut f = facts();
        f.parks = vec![ParkFact {
            provider: ProviderId::Claude,
            label: "work".into(),
            name: crate::state::Key::new(ProviderId::Claude, "work").typed(),
            active: false,
            last_used_at: Some(f.now - 31 * 86_400),
            park: Some(Park {
                service: "pitboard-park-acc-1".into(),
                parked_at: f.now - 31 * 86_400,
                refresh_fingerprint: "f".into(),
                access_expires_at: Some(f.now + 3600),
                refresh_expires_at: Some(f.now + 30 * 86_400),
            }),
            unreadable: None,
        }];
        let checks = evaluate(&f);
        let dormant = check(&checks, "dormant_account");
        assert_eq!(dormant.level, Level::Warn);
        assert!(dormant.advice.contains("pitboard forget work"));

        // A month is the token's own life. Inside it, there is nothing to say.
        f.parks[0].last_used_at = Some(f.now - 29 * 86_400);
        assert!(
            !evaluate(&f).iter().any(|c| c.code == "dormant_account"),
            "an account used within a token's lifetime is not dormant"
        );

        // Neither is one that is signed in, nor one holding nothing.
        f.parks[0].last_used_at = Some(f.now - 400 * 86_400);
        f.parks[0].active = true;
        assert!(!evaluate(&f).iter().any(|c| c.code == "dormant_account"));
        f.parks[0].active = false;
        f.parks[0].park = None;
        assert!(!evaluate(&f).iter().any(|c| c.code == "dormant_account"));
    }

    #[test]
    fn the_daemon_is_reported_without_being_a_problem() {
        let mut f = facts();
        let checks = evaluate(&f);
        assert!(check(&checks, "claude_daemon").detail.contains("none"));

        f.daemon = Some(daemon::Daemon {
            pid: 4321,
            version: Some("2.1.278".into()),
            started_at: Some(1_790_079_766_317),
            origin: Some("transient".into()),
            launch_target: None,
            running: true,
        });
        let checks = evaluate(&f);
        let running = check(&checks, "claude_daemon");
        assert_eq!(running.level, Level::Ok);
        assert!(running.detail.contains("running"));
        assert!(running.detail.contains("2.1.278"));
        assert!(running.detail.contains("4321"));

        f.daemon.as_mut().expect("set above").running = false;
        let checks = evaluate(&f);
        let stopped = check(&checks, "claude_daemon");
        assert_eq!(stopped.level, Level::Ok);
        assert!(stopped.detail.contains("not running"));
    }

    #[test]
    fn every_failure_and_warning_tells_the_user_something() {
        let mut f = facts();
        f.credential = Ok(Some(json!({"slackTag": {}})));
        f.home_mode = Some(0o755);
        for c in evaluate(&f) {
            if c.level != Level::Ok {
                assert!(!c.advice.is_empty(), "{} has no advice", c.code);
            }
        }
    }

    #[test]
    fn a_machine_without_a_stable_identifier_is_flagged() {
        let mut f = facts();
        f.machine_id_known = false;
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "machine_id").level, Level::Warn);
        assert!(evaluate(&facts()).iter().all(|c| c.code != "machine_id"));
    }

    #[test]
    fn each_check_is_told_apart_by_code_and_name() {
        let mut f = facts();
        f.parks = vec![parked("work", None), parked("personal", None)];
        let checks = evaluate(&f);
        let mut keys: Vec<(&str, &str)> =
            checks.iter().map(|c| (c.code, c.name.as_str())).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }

    #[test]
    fn every_parked_login_is_judged_and_a_way_back_is_offered() {
        let mut f = facts();
        let mut unusable = parked("broken", Some(NOW + 30 * 86_400));
        unusable.unreadable = Some("missing from the vault".into());
        f.parks = vec![
            parked("fine", Some(NOW + 20 * 86_400)),
            parked("soon", Some(NOW + 86_400)),
            parked("gone", Some(NOW - 1)),
            unusable,
            ParkFact {
                provider: ProviderId::Claude,
                last_used_at: None,
                label: "empty".into(),
                name: crate::state::Key::new(ProviderId::Claude, "empty").typed(),
                active: false,
                park: None,
                unreadable: None,
            },
            ParkFact {
                provider: ProviderId::Claude,
                last_used_at: None,
                label: "live".into(),
                name: crate::state::Key::new(ProviderId::Claude, "live").typed(),
                active: true,
                park: None,
                unreadable: None,
            },
        ];
        let checks = evaluate(&f);
        for (name, level) in [
            ("account fine", Level::Ok),
            ("account soon", Level::Warn),
            ("account gone", Level::Warn),
            ("account broken", Level::Fail),
            ("account empty", Level::Warn),
            ("account live", Level::Ok),
        ] {
            let c = named(&checks, name);
            assert_eq!(c.level, level, "{name}: {}", c.detail);
            if level != Level::Ok {
                let label = name.trim_start_matches("account ");
                assert!(
                    c.advice
                        .contains(&format!("pitboard enroll {label} --sign-in"))
                );
            }
        }
    }

    #[test]
    fn an_interrupted_switch_and_leftover_parks_are_reported() {
        let mut f = facts();
        f.interrupted = true;
        f.state = Ok(State {
            discarded: vec!["pitboard-park-x-1".into()],
            ..State::default()
        });
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "interrupted_switch").level, Level::Warn);
        assert_eq!(check(&checks, "discarded").level, Level::Warn);
        assert!(healthy(&checks), "neither stops pitboard working");
    }

    /// The advice has to be something a person can type and have it act on the right
    /// account. `pitboard enroll work --sign-in` about a Codex account would enroll a
    /// Claude Code one.
    #[test]
    fn advice_names_a_codex_account_the_way_it_is_typed() {
        let mut f = facts();
        let mut codex = codex_parked("work", Some(NOW - 1));
        codex.last_used_at = Some(NOW - 400 * 86_400);
        // Both tools have a `work`, so a bare `work` would be refused as ambiguous, and the
        // name gathered for Claude Code's is the qualified one.
        let mut claude = parked("work", Some(NOW - 1));
        claude.name = "claude/work".into();
        claude.last_used_at = Some(NOW - 400 * 86_400);
        f.parks = vec![claude, codex];
        let checks = evaluate(&f);

        let codex_park = named(&checks, "account codex/work");
        assert!(
            codex_park
                .advice
                .contains("`pitboard enroll codex/work --sign-in`"),
            "{}",
            codex_park.advice
        );
        let claude_park = named(&checks, "account claude/work");
        assert!(
            claude_park
                .advice
                .contains("`pitboard enroll claude/work --sign-in`"),
            "a name a command here takes, which a bare `work` is not: {}",
            claude_park.advice
        );

        let dormant: Vec<&Check> = checks
            .iter()
            .filter(|c| c.code.ends_with("dormant_account"))
            .collect();
        assert_eq!(dormant.len(), 2);
        assert!(
            dormant.iter().any(|c| c.name == "account codex/work"
                && c.advice.contains("`pitboard forget codex/work`"))
        );
        assert!(dormant.iter().any(|c| c.name == "account claude/work"
            && c.advice.contains("`pitboard forget claude/work`")));
    }

    /// Whether an account is the one signed in is its own tool's question. Asked of Claude
    /// Code's config for every account, a signed-in Codex account read as one with nothing
    /// parked to switch to, and the advice was to sign in again.
    #[test]
    fn an_account_is_active_by_its_own_tools_record() {
        use crate::store::memory::MemoryHost;

        let root = std::env::temp_dir().join(format!(
            "pitboard-doctor-active-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch home");
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _guard = Scratch(root.clone());
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_codex_home(root.join("codex").to_string_lossy().into())
            .with_memory_stores(MemoryHost::new());
        std::fs::write(
            root.join(".claude.json"),
            json!({"oauthAccount": {
                "accountUuid": "alpha-uuid",
                "emailAddress": "a@example.com",
                "organizationUuid": "org",
            }})
            .to_string(),
        )
        .expect("a Claude Code config");

        let account =
            |label: &str, uuid: &str, detail: crate::state::Detail| crate::state::Account {
                label: label.into(),
                account_uuid: uuid.into(),
                email: format!("{label}@example.com"),
                parked: None,
                last_used_at: None,
                detail,
            };
        let claude = || crate::state::Detail::Claude {
            organization_uuid: "org".into(),
            oauth_account: json!({}),
        };
        let codex = || crate::state::Detail::Codex {
            workspace_id: None,
            plan: None,
        };
        let mut state = State {
            accounts: vec![
                account("alpha", "alpha-uuid", claude()),
                account("work", "work-acc", codex()),
                account("home", "home-acc", codex()),
            ],
            ..State::default()
        };
        state.set_active(ProviderId::Codex, Some("home".into()));
        let active = |facts: &[ParkFact]| -> Vec<String> {
            facts
                .iter()
                .filter(|p| p.active)
                .map(ParkFact::typed)
                .collect()
        };

        // Nothing signed in to Codex: pitboard's own record of its last switch stands in.
        assert_eq!(active(&park_facts(&ctx, &state)), ["alpha", "codex/home"]);

        // Codex's login names `work`, whatever pitboard last recorded.
        let live = crate::provider::of(ProviderId::Codex)
            .live(&ctx)
            .expect("Codex keeps its login in a file here");
        let login = json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "id_token": crate::provider::jwt::unsigned(&json!({
                    "email": "work@example.com",
                    "https://api.openai.com/auth": {"chatgpt_account_id": "work-acc"},
                })),
                "access_token": "a",
                "refresh_token": "r",
                "account_id": "work-acc",
            },
        });
        store::write_raw(&live.chain, &live.service, &login.to_string()).expect("a login");
        assert_eq!(active(&park_facts(&ctx, &state)), ["alpha", "codex/work"]);
    }

    /// A machine that has never run Codex reads exactly as it did before pitboard knew
    /// Codex existed; one that has gets a section of its own, and nothing of Claude Code's
    /// moves.
    #[test]
    fn codex_has_a_section_only_where_there_is_a_codex() {
        let without = evaluate(&facts());
        assert!(without.iter().all(|c| !c.code.starts_with("codex_")));

        let mut f = facts();
        f.codex = with_codex();
        let with = evaluate(&f);
        let claude = |checks: &[Check]| -> Vec<(String, String, String)> {
            checks
                .iter()
                .filter(|c| !c.code.starts_with("codex_"))
                .map(|c| (c.code.to_string(), c.detail.clone(), c.advice.clone()))
                .collect()
        };
        assert_eq!(claude(&without), claude(&with), "Claude Code's checks move");
        for code in [
            "codex_backend",
            "codex_auth_file",
            "codex_login",
            "codex_version",
            "codex_running",
        ] {
            assert_eq!(check(&with, code).level, Level::Ok, "{code}");
        }
        assert!(healthy(&with));
        let login = check(&with, "codex_login");
        assert!(login.detail.contains("w@example.com"), "{}", login.detail);
        assert!(
            login.detail.contains("0123456789abcdef"),
            "{}",
            login.detail
        );

        // Accounts enrolled on a machine whose Codex home has gone still get the section.
        f.codex = CodexFacts {
            enrolled: 1,
            ..no_codex()
        };
        let checks = evaluate(&f);
        let file = check(&checks, "codex_auth_file");
        assert_eq!(file.level, Level::Warn);
        assert!(
            file.advice.contains("pitboard use codex/"),
            "{}",
            file.advice
        );
    }

    /// A keychain store is Codex's to use and pitboard's to leave alone, so it is said
    /// rather than read, and a store in memory holds nothing to switch. Each is a choice,
    /// not a fault: it fails for somebody with Codex accounts enrolled, because every one of
    /// them is out of reach, and is only stated for anybody else. It used to be a warning
    /// whoever it was, so a person with Codex accounts saw a healthy report on a machine
    /// where `pitboard use codex/...` refused, and a person with none was handed something
    /// to look at about a setting they chose.
    #[test]
    fn each_codex_store_is_judged_for_what_pitboard_can_do_with_it() {
        let judged = |backend: &'static str, enrolled: usize| {
            let codex = CodexFacts {
                backend,
                enrolled,
                ..with_codex()
            };
            codex_checks(&codex)
        };

        for store in ["keyring", "auto", "secrets", "ephemeral"] {
            let checks = judged(store, 1);
            let found = check(&checks, "codex_backend");
            assert_eq!(found.level, Level::Fail, "{store}");
            assert!(found.detail.contains(store), "{}", found.detail);
            assert!(
                found.advice.contains("config.toml"),
                "says which setting to remove: {}",
                found.advice
            );
            assert!(!healthy(&checks), "{store}");
            assert!(
                checks.iter().all(|c| c.code != "codex_login"),
                "a store other than the file is not read, so there is nothing to say about \
                 its login"
            );

            let checks = judged(store, 0);
            let found = check(&checks, "codex_backend");
            assert_eq!(
                found.level,
                Level::Ok,
                "nothing pitboard does is broken for somebody with no Codex accounts: {store}"
            );
            assert!(found.detail.contains(store), "{}", found.detail);
            assert!(found.detail.contains("file store"), "{}", found.detail);
        }
        for store in ["keyring", "auto", "secrets"] {
            let found = judged(store, 1);
            let advice = &check(&found, "codex_backend").advice;
            assert!(advice.contains("will not touch"), "{advice}");
        }
    }

    /// Signed in with an API key is somebody's choice, and nothing about it is broken or
    /// unreadable. It used to be reported as a login that "cannot be read", failing the
    /// whole report for anybody with Codex accounts and telling them to sign in with
    /// `codex` again.
    #[test]
    fn a_codex_login_with_an_api_key_is_a_choice_rather_than_a_fault() {
        let api_key = || CodexFacts {
            login: Err(CodexLoginTrouble::NotAnAccount(
                "Codex is signed in with an API key rather than a ChatGPT account, so there \
                 is no account login to park or switch"
                    .into(),
            )),
            ..with_codex()
        };

        let checks = codex_checks(&api_key());
        let login = check(&checks, "codex_login");
        assert_eq!(
            login.level,
            Level::Ok,
            "stated, for somebody with no Codex accounts"
        );
        assert!(login.detail.contains("API key"), "{}", login.detail);
        assert!(checks.iter().all(|c| c.level == Level::Ok));

        let checks = codex_checks(&CodexFacts {
            enrolled: 2,
            ..api_key()
        });
        let login = check(&checks, "codex_login");
        assert_eq!(login.level, Level::Warn);
        assert!(healthy(&checks), "nothing is broken");
        assert!(!login.detail.contains("cannot"), "{}", login.detail);
        assert!(login.advice.contains("`codex login`"), "{}", login.advice);
        assert!(
            !login.advice.contains("again"),
            "nothing to sign in again for: {}",
            login.advice
        );
    }

    /// Every check about a Codex account is in Codex's section and under a Codex code, so
    /// `codex_` finds everything about Codex. A Codex account's park used to be judged among
    /// Claude Code's, under Claude Code's codes and in Claude Code's column.
    #[test]
    fn a_codex_account_is_judged_in_codex_section_under_a_codex_code() {
        let claude_only = {
            let mut f = facts();
            f.parks = vec![parked("work", Some(NOW + 20 * 86_400))];
            f.codex = with_codex();
            evaluate(&f)
        };

        let mut f = facts();
        let mut codex = codex_parked("work", Some(NOW - 1));
        codex.last_used_at = Some(NOW - 400 * 86_400);
        f.parks = vec![parked("work", Some(NOW + 20 * 86_400)), codex];
        f.codex = CodexFacts {
            enrolled: 1,
            ..with_codex()
        };
        let checks = evaluate(&f);

        let about_codex: Vec<&Check> = checks
            .iter()
            .filter(|c| c.name.contains("codex/"))
            .collect();
        let codes: Vec<&str> = about_codex.iter().map(|c| c.code).collect();
        assert_eq!(codes, ["codex_parked_login", "codex_dormant_account"]);
        let section = checks
            .iter()
            .position(|c| c.code == "codex_backend")
            .expect("a Codex section");
        let first = checks
            .iter()
            .position(|c| c.name.contains("codex/"))
            .unwrap();
        assert!(first > section, "after the Codex heading");

        let claude = |checks: &[Check]| -> Vec<(&'static str, String)> {
            checks
                .iter()
                .filter(|c| !c.code.starts_with("codex_"))
                .map(|c| (c.code, c.name.clone()))
                .collect()
        };
        assert_eq!(
            claude(&checks),
            claude(&claude_only),
            "Claude Code's section is Claude Code's accounts alone"
        );
    }

    /// Found and not named is not missing. A `codex` behind a version manager's shim runs
    /// perfectly well from a path that says nothing about its version.
    #[test]
    fn a_codex_whose_version_cannot_be_read_is_not_called_missing() {
        let found = CodexFacts {
            program: Some(PathBuf::from("/home/x/.volta/bin/codex")),
            version: None,
            ..with_codex()
        };
        let version = check(&codex_checks(&found), "codex_version").detail.clone();
        assert!(!version.contains("not found"), "{version}");
        assert!(version.contains("/home/x/.volta/bin/codex"), "{version}");

        let missing = CodexFacts {
            program: None,
            version: None,
            ..with_codex()
        };
        let version = check(&codex_checks(&missing), "codex_version")
            .detail
            .clone();
        assert!(version.starts_with("not found here"), "{version}");
    }

    /// Each way Codex is installed, laid out in a scratch directory, and a shim that names
    /// nothing. Only the two directories above the program are looked at, names first, so a
    /// standalone install is named without opening any file inside it.
    #[test]
    fn a_version_is_read_out_of_each_way_codex_is_installed() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "pitboard-doctor-codex-version-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _guard = Scratch(root.clone());
        let place = |at: &str| {
            let path = root.join(at);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
            path
        };

        let standalone =
            place("codex/packages/standalone/releases/0.154.0-aarch64-apple-darwin/bin/codex");
        // A package.json beside it that would say otherwise, to show it is never opened.
        std::fs::write(
            standalone.with_file_name("package.json"),
            r#"{"name": "@openai/codex", "version": "9.9.9"}"#,
        )
        .unwrap();
        let link = root.join("bin/codex");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&standalone, &link).unwrap();
        assert_eq!(codex_version(&link).as_deref(), Some("0.154.0"));

        let cask = place("Caskroom/codex/0.153.2/codex-aarch64-apple-darwin");
        assert_eq!(codex_version(&cask).as_deref(), Some("0.153.2"));

        let npm = place("lib/node_modules/@openai/codex/bin/codex.js");
        std::fs::write(
            root.join("lib/node_modules/@openai/codex/package.json"),
            r#"{"name": "@openai/codex", "version": "0.150.1"}"#,
        )
        .unwrap();
        assert_eq!(codex_version(&npm).as_deref(), Some("0.150.1"));

        // Three levels up is too far: nothing further than two directories is read.
        let shim = place("volta/bin/volta-shim");
        std::fs::write(
            root.join("package.json"),
            r#"{"name": "@openai/codex", "version": "1.2.3"}"#,
        )
        .unwrap();
        assert_eq!(codex_version(&shim), None);
    }

    #[test]
    fn a_codex_login_anybody_can_read_or_nobody_can_parse_is_said() {
        let mut codex = with_codex();
        codex.auth_mode = Some(0o644);
        let checks = codex_checks(&codex);
        let file = check(&checks, "codex_auth_file");
        assert_eq!(file.level, Level::Warn);
        assert!(file.detail.contains("mode 644"), "{}", file.detail);
        assert!(file.advice.contains("chmod 600"), "{}", file.advice);

        codex.auth_mode = Some(0o600);
        codex.login = Err(CodexLoginTrouble::Unusable(
            "its id token is not readable".into(),
        ));
        assert_eq!(
            check(&codex_checks(&codex), "codex_login").level,
            Level::Warn
        );
        codex.enrolled = 2;
        let checks = codex_checks(&codex);
        let login = check(&checks, "codex_login");
        assert_eq!(login.level, Level::Fail);
        assert!(login.detail.contains("not readable"), "{}", login.detail);
    }

    /// A running codex holds the account it started with for as long as it runs, which is
    /// the one thing a switch cannot reach. Said, never warned about: running it is the
    /// point of having it.
    #[test]
    fn running_codex_processes_are_reported_as_a_fact() {
        let mut codex = with_codex();
        codex.running = Some(vec![4321, 99]);
        let checks = codex_checks(&codex);
        let running = check(&checks, "codex_running");
        assert_eq!(running.level, Level::Ok);
        assert!(running.detail.contains("4321, 99"), "{}", running.detail);
        assert!(running.detail.contains("restarted"), "{}", running.detail);

        codex.running = Some((1..=23).collect());
        let many = check(&codex_checks(&codex), "codex_running").detail.clone();
        assert!(many.starts_with("23 running"), "{many}");
        assert!(many.contains("1, 2, 3 and 20 more"), "{many}");

        codex.running = None;
        assert!(
            check(&codex_checks(&codex), "codex_running")
                .detail
                .contains("could not tell")
        );
    }

    #[test]
    fn every_codex_failure_and_warning_tells_the_user_something() {
        for backend in ["file", "keyring", "auto", "ephemeral"] {
            let codex = CodexFacts {
                backend,
                enrolled: 1,
                auth_mode: Some(0o666),
                login: Err(CodexLoginTrouble::Unusable("unreadable".into())),
                ..with_codex()
            };
            for c in codex_checks(&codex) {
                assert!(c.code.starts_with("codex_"), "{}", c.code);
                if c.level != Level::Ok {
                    assert!(!c.advice.is_empty(), "{} has no advice", c.code);
                }
            }
        }
    }

    /// What a pasted report must not carry, now that it can carry a Codex login too.
    #[test]
    fn a_codex_login_is_redacted_from_a_report() {
        let mut f = facts();
        f.codex = with_codex();
        let ctx = Context::new(PathBuf::from("/home/x"));
        let sheet = redaction_for(&ctx, &f);
        let login = check(&evaluate(&f), "codex_login").detail.clone();
        let hidden = sheet.over(&login);
        for secret in ["w@example.com", "work-account", "0123456789abcdef"] {
            assert!(login.contains(secret), "{login}");
            assert!(!hidden.contains(secret), "{hidden}");
        }
    }

    /// What the section is judged on, read off a real disk: a scratch Codex home with a
    /// login in it, in the file Codex keeps it in.
    #[test]
    fn codex_facts_are_read_off_the_disk() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "pitboard-doctor-codex-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _guard = Scratch(root.clone());
        let home = root.join("codex");
        // A `codex` of this test's own. Looked up on `PATH`, it would be this machine's, and
        // the standalone installer keeps that inside the real `~/.codex`.
        let program = root.join("bin/codex");
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_codex_home(home.to_string_lossy().into())
            .with_codex_program(program.clone());

        let absent = codex_facts(&ctx, None);
        assert!(!absent.present);
        assert_eq!(absent.backend, "file");
        assert_eq!(absent.auth_mode, None);
        assert!(matches!(absent.login, Ok(None)));
        assert_eq!(absent.program, None, "the one named, and it is not there");
        assert_eq!(absent.version, None);

        let installed = root.join("releases/0.154.0-aarch64-apple-darwin/bin/codex");
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::write(&installed, "").unwrap();
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&installed, &program).unwrap();
        let found = codex_facts(&ctx, None);
        assert_eq!(found.program.as_deref(), Some(program.as_path()));
        assert_eq!(found.version.as_deref(), Some("0.154.0"));

        std::fs::create_dir_all(&home).expect("a Codex home");
        let auth = home.join("auth.json");
        std::fs::write(
            &auth,
            json!({
                "auth_mode": "chatgpt",
                "tokens": {
                    "id_token": crate::provider::jwt::unsigned(&json!({
                        "email": "w@example.com",
                        "https://api.openai.com/auth": {"chatgpt_account_id": "work-acc"},
                    })),
                    "access_token": "a",
                    "refresh_token": "r",
                    "account_id": "work-acc",
                },
            })
            .to_string(),
        )
        .expect("a login");
        std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(0o644)).unwrap();
        let found = codex_facts(&ctx, None);
        assert!(found.present);
        assert_eq!(found.auth_mode, Some(0o644));
        let login = found.login.expect("readable").expect("there");
        assert_eq!(login.email, "w@example.com");
        assert_eq!(login.fingerprint.len(), 16);

        // Signed in with an API key: a choice, said in Codex's terms.
        std::fs::write(
            &auth,
            json!({"auth_mode": "apikey", "OPENAI_API_KEY": "sk-not-a-real-key"}).to_string(),
        )
        .unwrap();
        match codex_facts(&ctx, None).login {
            Err(CodexLoginTrouble::NotAnAccount(why)) => {
                assert!(why.contains("API key"), "{why}");
                assert!(!why.contains("sk-"), "never the key itself: {why}");
            }
            Err(other) => panic!("an API key is not an account: {other:?}"),
            Ok(_) => panic!("an API key is not an account"),
        }

        // One account's tokens under another's id, which a running codex leaves when it
        // refreshes in the middle of a switch: not one account's login.
        let mixed = json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "id_token": crate::provider::jwt::unsigned(&json!({
                    "email": "w@example.com",
                    "https://api.openai.com/auth": {"chatgpt_account_id": "work-acc"},
                })),
                "access_token": "a",
                "refresh_token": "r",
                "account_id": "home-acc",
            },
        });
        std::fs::write(&auth, mixed.to_string()).unwrap();
        assert!(
            matches!(
                codex_facts(&ctx, None).login,
                Err(CodexLoginTrouble::Unusable(_))
            ),
            "a login mixing two accounts is one pitboard cannot use"
        );

        std::fs::write(
            home.join("config.toml"),
            "cli_auth_credentials_store = \"ephemeral\"\n",
        )
        .expect("a config");
        let ephemeral = codex_facts(&ctx, None);
        assert_eq!(ephemeral.backend, "ephemeral");
        assert!(
            matches!(ephemeral.login, Ok(None)),
            "a store pitboard does not handle is not read"
        );

        // A keychain store with the encrypted-file feature is named as what it is, never as
        // plain `keyring`, which the configuration does not say.
        std::fs::write(
            home.join("config.toml"),
            "cli_auth_credentials_store = \"auto\"\n[features]\nsecret_auth_storage = true\n",
        )
        .expect("a config");
        assert_eq!(codex_facts(&ctx, None).backend, "secrets");
    }

    #[test]
    fn a_version_is_read_out_of_the_path_codex_is_installed_at() {
        assert!(looks_like_a_version("0.154.0"));
        assert!(!looks_like_a_version("releases"));
        assert!(!looks_like_a_version("0.154"));
        assert!(!looks_like_a_version("v0.154.0"));
    }

    /// A machine that uses only Codex is not told Claude Code is broken, nor to run a
    /// program it does not use. Everything about pitboard itself is still checked.
    #[test]
    fn a_machine_with_only_codex_is_not_judged_on_claude_code() {
        let mut facts = facts();
        facts.claude_present = false;
        facts.config = Err(crate::error::Error::ClaudeConfigMissing {
            path: PathBuf::from("/nowhere/.claude.json"),
        });
        facts.codex.present = true;
        let checks = evaluate(&facts);
        for own in CLAUDE_CODES_OWN {
            assert!(
                checks.iter().all(|c| c.code != *own),
                "{own} is about Claude Code, which is not here"
            );
        }
        assert!(
            checks.iter().any(|c| c.code == "state"),
            "pitboard's own still is"
        );
        assert!(checks.iter().any(|c| c.code.starts_with("codex_")));
    }

    /// With neither tool present, a new machine is told what to do first, as it always was.
    #[test]
    fn a_machine_with_neither_tool_still_hears_about_claude_code() {
        let mut facts = facts();
        facts.claude_present = false;
        let checks = evaluate(&facts);
        assert!(checks.iter().any(|c| c.code == "config_file"));
    }

    /// What `/logout` leaves is nobody signed in, not a login whose shape moved.
    #[test]
    fn a_signed_out_credential_is_said_to_be_one() {
        let mut facts = facts();
        facts.credential = Ok(Some(serde_json::json!({"mcpOAuth": {"server": {}}})));
        let check = judge_credential(&facts);
        assert!(matches!(check.level, Level::Warn), "{}", check.detail);
        assert!(check.detail.contains("signed out"), "{}", check.detail);
    }
}
