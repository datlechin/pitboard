//! `pitboard doctor`: check, on this machine, that what Pitboard relies on about Claude Code
//! still holds, and say which assumption broke when one has. Gathering is kept apart from
//! judging so every judgement can be tested.
//!
//! Codex has a section of its own, after everything about Claude Code, and only on a
//! machine where Codex has been run or has accounts enrolled. Its checks are coded
//! `codex_...` so a program can tell them from Claude Code's, which read exactly as they did
//! before there was a second tool.

use crate::context::Context;
use crate::error::Error;
use crate::host::task::{Drift, Task};
use crate::host::{Access, Kind, Os};
use crate::provider::claude::daemon;
use crate::provider::claude::live as claude_live;
use crate::provider::claude::paths as claude;
use crate::provider::claude::slot;
use crate::provider::codex::paths as codex;
use crate::provider::{LoginKind, ProviderId};
use crate::schedule::Fault;
use crate::service::Stored;
use crate::state::{Key, Park, State};
use crate::{home, park, store, switch, words};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::path::PathBuf;

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
    pub(crate) config_account: Option<claude::ConfigAccount>,
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

pub(crate) struct WslFact {
    pub(crate) distribution: Option<String>,
    // Codex's home, where it is on a drive of Windows'.
    pub(crate) codex_home_on_windows: Option<PathBuf>,
}

// The shell Claude Code runs the status line in, and the pitboard the PATH it hands that shell
// finds. Both shells take `pitboard.exe` for a bare `pitboard`.
pub(crate) struct StatusLineFact {
    pub(crate) git_bash: claude::GitBash,
    pub(crate) powershell: Option<claude::PowerShell>,
    pub(crate) pitboard: Option<PathBuf>,
    // Whether the settings Pitboard reads run a `pitboard` the shell has to find.
    pub(crate) runs_pitboard: bool,
}

/// How Pitboard's requests leave this machine: directly, or through the HTTP proxy its
/// environment names, read as ureq 3.4.2 reads one from a process's environment. A SOCKS
/// proxy, which Pitboard does not use yet, fails each request it would have carried.
pub(crate) struct NetworkFact {
    /// The proxy the environment names, where it names one Pitboard can read.
    pub(crate) proxy: Option<ProxyFact>,
    /// The variables before it set to something that is not a proxy's address, which
    /// Pitboard passes over, in the order it reads them.
    pub(crate) passed_over: Vec<&'static str>,
    // Each proxy variable set to anything, as the tools read the ones they read.
    pub(crate) set: Vec<&'static str>,
}

impl NetworkFact {
    fn sets(&self, any_of: &[&str]) -> Option<&'static str> {
        self.set.iter().copied().find(|name| any_of.contains(name))
    }
}

/// The proxy the environment names.
pub(crate) struct ProxyFact {
    /// The variable that names it.
    pub(crate) variable: &'static str,
    /// Its scheme, host and port, with `***` for a user name or a password it holds.
    pub(crate) address: String,
    /// Its host and port, `proxy.example.com:3128`, as `address` shows them: the part of it
    /// that can name the company whose network it is.
    pub(crate) server: String,
    /// Whether its host is loopback, this machine itself, which names nothing beyond it.
    pub(crate) on_loopback: bool,
    /// Whether it is a SOCKS proxy, which Pitboard does not use yet: a request to a host it
    /// does not exempt fails before anything is sent, and one to a host it exempts goes out
    /// directly.
    pub(crate) socks: bool,
    // Whether a request to a host Pitboard calls fails, as `Proxies::refuses_some` says.
    pub(crate) fails: bool,
    /// `NO_PROXY` or `no_proxy`, whichever is read, where either is set.
    pub(crate) exempting: Option<&'static str>,
    /// The hosts Pitboard calls that it exempts, which Pitboard reaches directly.
    pub(crate) exempt: Vec<String>,
}

/// The daily renewal schedule as it is installed, read from what Pitboard wrote: a file, or
/// on Windows the task as Task Scheduler keeps it.
pub(crate) struct ScheduleFact {
    pub(crate) location: crate::schedule::Location,
    // Task Scheduler's word on its runs and its settings; none from launchd or systemd.
    pub(crate) task: Option<Task>,
    /// The Pitboard it runs, where the file names one the way Pitboard writes it.
    pub(crate) program: Option<PathBuf>,
    /// Whether that Pitboard is still there to be run.
    pub(crate) program_found: bool,
    /// How its runs send requests, as the proxy variables its file gives them say: those of
    /// the Pitboard that installed it, read as this run's are ([`Facts::network`]).
    pub(crate) network: NetworkFact,
    /// Whether its runs send requests another way than this run does: through another proxy
    /// or none, with another user name or password, or with other hosts exempt.
    pub(crate) network_differs: bool,
}

pub(crate) struct WatchServiceFact {
    pub(crate) location: crate::schedule::Location,
    pub(crate) program: Option<PathBuf>,
    pub(crate) program_found: bool,
    pub(crate) at: Option<crate::duty::autoswitch::Threshold>,
    // Task Scheduler's word on its runs and its settings; none from launchd or systemd.
    pub(crate) task: Option<Task>,
}

/// What is read about Codex CLI on this machine.
///
/// Read without running it and without touching a keychain item Codex created for itself:
/// its home, its configuration, the one file it keeps its login in by default, and which
/// `codex` processes are running.
pub(crate) struct CodexFacts {
    /// `CODEX_HOME`, or `~/.codex`.
    pub(crate) home: PathBuf,
    /// Whether that directory exists. It does once Codex has been run here, and not before.
    pub(crate) present: bool,
    /// How many Codex accounts Pitboard has enrolled.
    pub(crate) enrolled: usize,
    /// Where Codex is configured to keep its login, in Codex's own words:
    /// `cli_auth_credentials_store`'s `file`, `keyring`, `auto` or `ephemeral`, or `secrets`
    /// for a keychain store with `[features] secret_auth_storage`, which Codex has no one
    /// word for. `unknown` where a layer Codex reads it from is there and cannot be read as
    /// Codex reads it, which stops Codex from starting.
    pub(crate) backend: &'static str,
    /// The setting that chose it, in words that follow it in brackets: `Codex's default`,
    /// `` `cli_auth_credentials_store = "keyring"` in /etc/codex/config.toml ``, or `pinned to
    /// `keyring` by /etc/codex/requirements.toml`. For `unknown`, what cannot be read.
    pub(crate) backend_setting: String,
    /// What would choose the file store instead, as a sentence with no full stop: the line to
    /// set and where, or who can. For `unknown`, what Codex does until what cannot be read is
    /// put right, as sentences with no last full stop. Empty for the file.
    pub(crate) backend_remedy: String,
    /// Where the default store keeps it.
    pub(crate) auth_file: PathBuf,
    /// Who can reach that file, where there is such a file.
    pub(crate) auth_access: Option<Access>,
    /// Whose login the file holds, or why that could not be told. `Ok(None)` for no file,
    /// and for a store Pitboard does not read.
    pub(crate) login: Result<Option<CodexLogin>, CodexLoginTrouble>,
    /// Where the `codex` Pitboard would run is, where it is anywhere.
    pub(crate) program: Option<PathBuf>,
    /// Which Codex that is, read off the path it is installed at. `None` both where there
    /// is no `codex` and where its path does not say; [`CodexFacts::program`] tells which.
    pub(crate) version: Option<String>,
    /// Every `codex` running as this user, by where it runs from. `None` where that could
    /// not be asked.
    pub(crate) running: Option<Vec<crate::holder::Holding>>,
    // Credential Manager's `cli|` and `secrets|` items of Codex's, counted by name while it
    // keeps its login in the file (`codex_windows_stores`).
    pub(crate) stored_elsewhere: (usize, usize),
    // Each setting that limits which logins Codex keeps, and where.
    pub(crate) restrictions: Vec<String>,
    pub(crate) restrictions_unreadable: bool,
    // The enrolled accounts whose logins Codex would sign out or leave unused.
    pub(crate) ruled_out: Vec<String>,
}

// What Windows says of the account's folders, beside the home Pitboard reads them from.
pub(crate) struct WindowsHomes {
    pub(crate) home: PathBuf,
    pub(crate) default_pitboard_home: PathBuf,
    pub(crate) local_app_data: Option<PathBuf>,
    pub(crate) profile: Option<PathBuf>,
    pub(crate) codex_home_set: bool,
    pub(crate) roams: bool,
}

/// Whose a Codex login is, as its own ID token says. Nothing in here is a secret: the
/// fingerprint is a handle on the refresh token, never the token.
pub(crate) struct CodexLogin {
    pub(crate) email: String,
    pub(crate) account_id: String,
    /// Empty where the login holds no refresh token.
    pub(crate) fingerprint: String,
}

/// Why Codex's login names no account Pitboard can handle.
///
/// Two answers rather than one, because they call for different things. A login signed in
/// some way Pitboard does not switch, such as with an API key, is somebody's choice and
/// nothing is wrong with it; a login that cannot be read, or that mixes two accounts, is.
#[derive(Debug)]
pub(crate) enum CodexLoginTrouble {
    /// Signed in, and not with an account Pitboard parks or switches. Says why, in Codex's
    /// terms.
    NotAnAccount(String),
    /// Unreadable, or read and not one account's login.
    Unusable(String),
    // `CODEX_HOME` names this, which is no folder, so Codex does not start: Windows' alone.
    NoHome(PathBuf),
}

pub(crate) struct ParkFact {
    /// Which tool the account belongs to, which is what decides how it is named back to a
    /// person: bare for Claude Code, `codex/work` for Codex.
    pub(crate) provider: ProviderId,
    /// The name a command on this machine takes for it: qualified where another tool has an
    /// account of the same name, since a bare one would then be ambiguous.
    pub(crate) name: String,
    /// Whether its login is the one its tool has stored, as far as files say.
    pub(crate) active: bool,
    /// When this account last came to be in use, where that is recorded.
    pub(crate) last_used_at: Option<i64>,
    pub(crate) park: Option<Park>,
    /// Why it cannot be read back, if it cannot.
    pub(crate) unreadable: Option<Unreadable>,
}

/// Why a parked login could not be read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unreadable {
    /// It is behind a keychain that is locked here, which says nothing about the login.
    Locked,
    /// A park sealed to this account that will not open in this logon, with
    /// `CryptUnprotectData`'s code where there was one: a lost key, not a missing or damaged
    /// file.
    Sealed(Option<i32>),
    /// Missing, damaged, or refused by the store, in words.
    Broken(String),
}

/// Claude Code's plaintext fallback file, there while the keychain holds the login in use.
pub(crate) struct FallbackLogin {
    pub(crate) path: PathBuf,
    /// A handle on the refresh token of the login it holds, never the token: `None` where
    /// it holds no login, empty where that login has no refresh token, and why where what
    /// it holds cannot be read.
    pub(crate) fingerprint: Result<Option<String>, store::Error>,
}

impl ParkFact {
    /// The account's name as a command here would take it.
    fn typed(&self) -> String {
        self.name.clone()
    }
}

fn park_facts(ctx: &Context, state: &State) -> Vec<ParkFact> {
    // Whose login each tool has stored, as its service last said, asked once per tool and
    // only of a tool that has accounts here, and from files alone: Pitboard's record, and a
    // Codex login's own claims. Never Claude Code's config, which a Claude Code process
    // started or signed in on another login can rewrite with its own account.
    let in_use: std::collections::BTreeMap<ProviderId, Option<crate::provider::Owner>> =
        ProviderId::ALL
            .iter()
            .filter(|&&which| state.accounts.iter().any(|a| a.provider() == which))
            .map(|&which| {
                let known = crate::in_use::known(ctx, state, which);
                (which, known.owner().cloned())
            })
            .collect();
    state
        .accounts
        .iter()
        .map(|a| ParkFact {
            provider: a.provider(),
            name: state.typed(&a.key()),
            last_used_at: a.last_used_at,
            active: in_use
                .get(&a.provider())
                .and_then(Option::as_ref)
                .is_some_and(|owner| a.owned_by(owner)),
            park: a.parked.clone(),
            unreadable: a.parked.as_ref().and_then(|p| {
                let tool = match crate::provider::of(a.provider()) {
                    LoginKind::Document(tool) => tool,
                    // Its own check reads the index and the folder: `desktop_parked_login`.
                    LoginKind::Folder(_) => return None,
                };
                park::load(ctx, tool, &a.key(), p).err().map(|e| match e {
                    Error::Store(store::Error::Locked) => Unreadable::Locked,
                    Error::Store(store::Error::Sealed { code, .. }) => Unreadable::Sealed(code),
                    Error::ParkedCredentialMissing { .. } => {
                        Unreadable::Broken("missing from the vault".into())
                    }
                    Error::ParkedCredentialCorrupt { detail, .. } => Unreadable::Broken(detail),
                    other => Unreadable::Broken(other.to_string()),
                })
            }),
        })
        .collect()
}

// Read from the index, the settings and the data folder's first-level names: no jar, no process.
pub struct DesktopFacts {
    root: Result<PathBuf, String>,
    // this account's own home, where `HOME` names another: only a release asks for it
    own_home: Option<PathBuf>,
    refused: Option<crate::provider::folder::Unfit>,
    present: bool,
    // the sidecar, and whether `config.json` is there
    settings:
        Option<Result<(crate::provider::folder::Sidecar, bool), crate::provider::folder::Unread>>,
    unread: Vec<String>,
    known: Option<crate::in_use::Known>,
    signed_out: bool,
    parks_dir: PathBuf,
    private: Option<crate::host::Private>,
    // whether `parks` is on the data folder's volume, where both could be asked
    one_volume: Option<bool>,
    // each folder park's name, and whether it is a folder there, or why it was not looked at
    parked_here: Vec<(String, Result<bool, String>)>,
    build: Option<String>,
}

fn claude_desktop() -> Option<&'static dyn crate::provider::folder::Folder> {
    match crate::provider::of(ProviderId::Desktop) {
        LoginKind::Folder(app) => Some(app),
        LoginKind::Document(_) => None,
    }
}

fn desktop_facts(ctx: &Context, state: &State) -> Option<DesktopFacts> {
    let tool = ProviderId::Desktop;
    let app = claude_desktop()?;
    let enrolled = crate::status::shown(state, tool);
    if !enrolled && !state.aside.iter().any(|aside| aside.tool == tool) {
        return None;
    }
    let root = app.root(ctx).map_err(|e| e.to_string());
    let mut facts = DesktopFacts {
        own_home: root
            .as_ref()
            .err()
            .and_then(|_| crate::provider::desktop::paths::home_elsewhere(ctx)),
        root: root.clone(),
        refused: None,
        present: false,
        settings: None,
        unread: Vec::new(),
        known: None,
        signed_out: false,
        parks_dir: store::folder::parks_dir(ctx),
        private: crate::host::fs::private_dir(&store::folder::parks_dir(ctx)).ok(),
        one_volume: root.as_ref().ok().and_then(|root| {
            crate::host::fs::same_volume(root, &store::folder::parks_dir(ctx)).ok()
        }),
        parked_here: state
            .accounts
            .iter()
            .filter_map(|a| a.parked.as_ref())
            .filter(|park| park.kind == crate::state::ParkKind::Folder)
            .map(|park| {
                let at = store::folder::at(ctx, &park.service);
                let folder = match std::fs::symlink_metadata(at) {
                    Ok(found) => Ok(found.is_dir()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                    Err(e) => Err(e.to_string()),
                };
                (park.service.clone(), folder)
            })
            .collect(),
        build: desktop_build(ctx),
    };
    if let Ok(root) = &root {
        facts.refused = app.refuses(root);
        facts.present = root.is_dir();
    }
    // Nothing inside Claude's data folder is read until one of its accounts is enrolled.
    if let (true, Ok(root)) = (enrolled, &root) {
        let settings = app.settings(root);
        let (known, signed_out) = switch::folder::judged(state, app, settings.as_ref().ok());
        facts.known = Some(known);
        facts.signed_out = signed_out.is_some();
        facts.settings = Some(settings.map(|read| (read.sidecar, read.file.is_some())));
        facts.unread = switch::folder::unread_parts(app, root).unwrap_or_default();
    }
    Some(facts)
}

pub(crate) fn gather(ctx: &Context) -> Facts {
    let pitboards = match ctx.os {
        Os::Windows => pitboards(ctx),
        Os::MacOs | Os::Linux => Vec::new(),
    };
    let config = claude::load_config(ctx);
    let state = crate::state::load(ctx);
    let service = claude::live_service(ctx);
    let home = home::dir(ctx);
    let config_account = config.as_ref().ok().and_then(claude::config_account);
    Facts {
        os: crate::host::OS,
        sid: ctx.sid.clone(),
        security_tool: crate::host::OS
            .secrets_tool()
            .filter(|p| std::fs::metadata(p).is_ok())
            .map(str::to_string),
        config_path: claude::config_file(ctx),
        config_account,
        config,
        account: slot::account_name(ctx),
        default_slot: claude::is_default_slot(ctx),
        storage_dir: claude::storage_dir(ctx),
        backend: store::resolve(&claude_live::chain(ctx), &service),
        vault: store::vault_kind(ctx),
        credential_file: claude_live::credential_file(ctx),
        credential_cost: store::read_raw(&claude_live::chain(ctx), &service)
            .ok()
            .flatten()
            .and_then(|raw| store::cost(&claude_live::chain(ctx), &service, &raw)),
        no_argv: !ctx.argv_fallback(),
        credential_parts: store::read(&claude_live::chain(ctx), &service)
            .ok()
            .flatten()
            .map(|doc| parts_of(&doc))
            .unwrap_or_default(),
        credential: store::read(&claude_live::chain(ctx), &service),
        home_access: crate::host::fs::access(&home),
        readable_by_others: loose_logins(ctx),
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
        claude_version: claude::installed_version(ctx),
        claude_program: claude::program(ctx),
        auth_overrides: crate::settings::overrides(ctx),
        asking_held: crate::usage::budget::holds(ctx),
        in_use: state
            .as_ref()
            .ok()
            .map(|s| crate::in_use::known(ctx, s, ProviderId::Claude)),
        parks: state
            .as_ref()
            .map(|s| park_facts(ctx, s))
            .unwrap_or_default(),
        codex: codex_facts(ctx, state.as_ref().ok()),
        desktop: state.as_ref().ok().and_then(|s| desktop_facts(ctx, s)),
        // Read as the next change reads it, without the one question it may put to a
        // service: `doctor` sends no request.
        stuck: state
            .as_ref()
            .ok()
            .and_then(|s| switch::stuck(ctx, s, switch::Asking::Nobody))
            .map(|refusal| refusal.to_string()),
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
        schedule: schedule_fact(ctx),
        watch_service: watch_service_fact(ctx),
        network: network_fact(&ctx.proxy),
        in_the_app: ctx.caller == "app",
        elevation: ctx.host().elevation(ctx),
        fallback_login: fallback_login(ctx),
        floor: ctx.host().floor(),
        status_line: (ctx.os == Os::Windows).then(|| StatusLineFact {
            git_bash: claude::git_bash(ctx),
            powershell: claude::powershell(ctx),
            pitboard: pitboards.first().cloned(),
            runs_pitboard: crate::settings::status_line_command(ctx)
                .is_some_and(|command| runs_bare_pitboard(&command)),
        }),
        windows_settings: (ctx.os == Os::Windows).then(|| ctx.host().windows_settings()),
        pitboards,
        wsl: wsl_fact(ctx),
        now: ctx.now(),
    }
}

// As Codex 0.163.0's Linux build tells WSL (`codex_wsl`).
fn wsl_fact(ctx: &Context) -> Option<WslFact> {
    let inside = ctx.os == Os::Linux && (ctx.wsl_distro.is_some() || ctx.host().kernel_is_wsl());
    let codex_home = codex::home(ctx);
    inside.then(|| WslFact {
        distribution: ctx.wsl_distro.clone().filter(|name| !name.is_empty()),
        codex_home_on_windows: ctx
            .host()
            .on_a_windows_drive(&codex_home)
            .then_some(codex_home),
    })
}

// One file two folders lead to, such as winget's link and its package, is one copy.
fn pitboards(ctx: &Context) -> Vec<PathBuf> {
    let lookup = crate::host::program::Lookup::new(Os::Windows, Some(".EXE".as_ref()));
    let mut files: Vec<PathBuf> = Vec::new();
    let mut copies = Vec::new();
    for found in crate::host::program::every_among(
        std::path::Path::new("pitboard"),
        std::env::split_paths(&ctx.search_path()),
        &lookup,
        crate::host::proc::can_run,
    ) {
        let file = std::fs::canonicalize(&found).unwrap_or_else(|_| found.clone());
        if !files.iter().any(|seen| Os::Windows.same_path(seen, &file)) {
            files.push(file);
            copies.push(found);
        }
    }
    copies
}

// Nothing where whether it is there cannot be told: an unreadable keychain is the
// `credential_store` check's to say.
fn fallback_login(ctx: &Context) -> Option<FallbackLogin> {
    let login = claude_live::login_behind(ctx)?;
    Some(FallbackLogin {
        path: claude_live::credential_file(ctx),
        fingerprint: login.map(|login| {
            login.map(|document| crate::provider::claude::document::fingerprint_of(&document))
        }),
    })
}

/// How the context's requests leave this machine, as its environment says.
fn network_fact(proxies: &crate::proxy::Proxies) -> NetworkFact {
    NetworkFact {
        proxy: proxies
            .variable()
            .zip(proxies.address())
            .map(|(variable, address)| ProxyFact {
                variable,
                address,
                server: proxies.server().unwrap_or_default(),
                on_loopback: proxies.on_loopback(),
                socks: proxies.socks(),
                fails: proxies.refuses_some(),
                exempting: proxies.exempting(),
                exempt: proxies.exempt_hosts(),
            }),
        passed_over: proxies.passed_over().to_vec(),
        set: proxies
            .variables()
            .iter()
            .filter(|(name, value)| !value.is_empty() && crate::proxy::NAMING.contains(name))
            .map(|(name, _)| *name)
            .collect(),
    }
}

/// The schedule, where this home has one.
fn schedule_fact(ctx: &Context) -> Option<ScheduleFact> {
    if !crate::schedule::serves(ctx) {
        return None;
    }
    let crate::schedule::Installed::Yes { location, .. } = crate::schedule::status(ctx) else {
        return None;
    };
    let program = crate::schedule::installed_program(ctx);
    let proxies = crate::schedule::installed_proxies(ctx).unwrap_or_default();
    Some(ScheduleFact {
        program_found: program.as_deref().is_some_and(std::path::Path::is_file),
        program,
        location,
        task: crate::schedule::task(ctx, crate::schedule::Job::Renew),
        network: network_fact(&proxies),
        network_differs: !proxies.same_way(&ctx.proxy),
    })
}

fn watch_service_fact(ctx: &Context) -> Option<WatchServiceFact> {
    if !crate::schedule::serves(ctx) {
        return None;
    }
    let crate::watch_service::Installed::Yes { location, at } = crate::watch_service::status(ctx)
    else {
        return None;
    };
    let program = crate::watch_service::installed_program(ctx);
    Some(WatchServiceFact {
        program_found: program.as_deref().is_some_and(std::path::Path::is_file),
        program,
        location,
        at,
        task: crate::watch_service::task(ctx),
    })
}

/// What is taking up the room in a credential document, largest first.
///
/// A login that will not fit is almost never the login: on one real machine the OAuth block
/// was 506 bytes and eleven MCP server tokens were 3679. Saying "8503 of 4032 bytes" leaves
/// a person to guess which of those to do something about, and the answer is in the
/// document Pitboard has already read.
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

// Every file holding a login that others may read, or, where it is sealed, change: who a file
// lets in is what a restore, a copy or a careless umask changes, and on Windows Claude Code's
// file takes its folder's access list (Claude Code 2.1.300, RF-C1b).
fn loose_logins(ctx: &Context) -> Vec<(String, Kind, Access)> {
    let mut loose = Vec::new();
    let mut look = |path: PathBuf| {
        if let Some(access) = crate::host::fs::access(&path)
            && access.shared
        {
            let kind = if path.is_dir() {
                Kind::Directory
            } else {
                Kind::File
            };
            loose.push((path.display().to_string(), kind, access));
        }
    };
    look(claude_live::credential_file(ctx));
    // A sealed park opens for this Windows account alone, but sealing stops reading, not
    // writing: another account with write access can corrupt or plant a park, so the vault and
    // its parks are still checked.
    let vault = store::vault_dir(ctx);
    look(vault.clone());
    if let Ok(entries) = std::fs::read_dir(&vault) {
        let mut parks: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
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
    let store = codex::store(ctx);
    let backend = store.backend;
    let auth_file = codex::auth_file(ctx);
    let home = codex::home(ctx);
    // The program Pitboard would run, as the context names it and where the context looks:
    // an app started from Finder has no shell `PATH` and passes the login shell's, and a
    // test names a program of its own.
    let program = crate::provider::program_of(ctx, ProviderId::Codex);
    // A layer Codex cannot read is the store's line to say.
    let restrictions = (backend != codex::Backend::Unknown).then(|| codex::restrictions(ctx));
    let unreadable = restrictions
        .as_ref()
        .is_some_and(|found| found.unreadable());
    let ruled_out = state.filter(|_| !unreadable).map_or_else(Vec::new, |s| {
        s.accounts
            .iter()
            .filter(|a| a.provider() == ProviderId::Codex)
            .filter_map(|a| {
                let workspace = Some(a.owner().organization_uuid).filter(|id| !id.is_empty());
                restrictions
                    .as_ref()?
                    .refuses(workspace.as_deref(), workspace.as_deref())
                    .map(|_| format!("`{}`", s.typed(&a.key())))
            })
            .collect()
    });
    CodexFacts {
        present: home.is_dir(),
        enrolled: state.map_or(0, |s| {
            s.accounts
                .iter()
                .filter(|a| a.provider() == ProviderId::Codex)
                .count()
        }),
        backend: backend.word(),
        backend_setting: match backend {
            codex::Backend::Unknown => store.why_unknown().unwrap_or_default(),
            _ => store.setting(),
        },
        backend_remedy: match backend {
            codex::Backend::File => String::new(),
            codex::Backend::Unknown => store.while_unknown().unwrap_or_default(),
            _ => store.to_use_the_file(
                words::spelling(ctx.os, ProviderId::Codex, program.as_deref())
                    .of(ProviderId::Codex),
            ),
        },
        auth_access: crate::host::fs::access(&auth_file),
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
        running: running_codex(ctx),
        stored_elsewhere: if backend == codex::Backend::File {
            stored_elsewhere(ctx)
        } else {
            (0, 0)
        },
        restrictions: restrictions.map_or_else(Vec::new, |found| found.said()),
        restrictions_unreadable: unreadable,
        ruled_out,
        auth_file,
        home,
    }
}

// A setting Pitboard does not read, such as an enterprise's cloud configuration, can still keep
// a session's login there. By name alone, so no item's secret is ever read.
fn stored_elsewhere(ctx: &Context) -> (usize, usize) {
    use crate::provider::names::{Family, family};
    let count = |prefix: &str, of: Family| {
        ctx.host().credential_names(prefix).map_or(0, |names| {
            names.iter().filter(|name| family(name) == Some(of)).count()
        })
    };
    (
        count("cli|", Family::CodexCli),
        count("secrets|", Family::CodexSecrets),
    )
}

/// What the file store's line adds: the one layer Pitboard leaves unread, and what it does.
const PROJECTS_NOT_READ: &str = "Pitboard does not read a project's own \
    `.codex/config.toml`, which applies only inside that project: in a trusted project that \
    sets `cli_auth_credentials_store`, Codex keeps its login somewhere else.";

/// Whose login Codex's file holds, from the login's own ID token and with no network call.
fn codex_login(ctx: &Context) -> Result<Option<CodexLogin>, CodexLoginTrouble> {
    let tool = crate::provider::codex();
    let unusable = |e: crate::provider::ProviderError| CodexLoginTrouble::Unusable(e.to_string());
    let Some(credential) = tool.read_live(ctx).map_err(|e| match e {
        crate::provider::ProviderError::CodexHomeMissing { path } => {
            CodexLoginTrouble::NoHome(path)
        }
        other => unusable(other),
    })?
    else {
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
        account_id: found.account_uuid,
        fingerprint: tool.fingerprint(&credential.raw),
    }))
}

// Never by running it, and no further than two folders up: names before any file is opened.
fn codex_version(program: &std::path::Path) -> Option<String> {
    let program =
        crate::host::program::npm_shim_target(program).unwrap_or_else(|| program.to_path_buf());
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
            near.iter().find_map(|dir| {
                version_where(&dir.join("package.json"), "name", "@openai/codex")
                    .or_else(|| version_where(&dir.join("codex-package.json"), "variant", "codex"))
            })
        })
}

fn looks_like_a_version(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

// The `version` of a JSON file whose `key` reads `expected`: `package.json`'s name
// `@openai/codex`, or `codex-package.json`'s variant `codex`.
fn version_where(path: &std::path::Path, key: &str, expected: &str) -> Option<String> {
    let json: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if json.get(key)?.as_str()? != expected {
        return None;
    }
    Some(json.get("version")?.as_str()?.to_string())
}

/// Every `codex` running as this user, by kind, asked the way a switch asks so the two
/// cannot disagree.
fn running_codex(ctx: &Context) -> Option<Vec<crate::holder::Holding>> {
    let crate::provider::Adoption::RestartRequired { program, holders } =
        crate::provider::codex().adoption(None)
    else {
        return Some(Vec::new());
    };
    crate::holder::find(ctx, ProviderId::Codex, program, holders)
}

/// The check that fails where the host says this process is not the person themselves
/// ([`crate::host::Host::elevation`]), or nobody could tell whether it is, since then Pitboard
/// changes nothing: `None` where it runs as the person themselves.
fn elevated(
    os: Os,
    elevation: crate::host::Elevation,
    admin_approval: Option<bool>,
) -> Option<Check> {
    let said = match elevation {
        crate::host::Elevation::Normal => return None,
        crate::host::Elevation::Elevated { why } => words::elevated(os, Some(why)),
        crate::host::Elevation::Unknown => words::elevated(os, None),
    };
    // `EnableLUA` 0 says it is User Account Control off, not the built-in Administrator.
    let way_out = match (elevation, admin_approval) {
        (crate::host::Elevation::Elevated { why }, Some(false))
            if why == crate::host::token::IN_EVERY_PROGRAM =>
        {
            words::ADMIN_APPROVAL_OFF
        }
        _ => said.way_out,
    };
    Some(fail(
        "elevated",
        "runs as",
        said.because,
        format!(
            "Pitboard changes nothing this way: it reads, and renews and writes nothing. {way_out}"
        ),
    ))
}

// Each only where it is so, as a person on Windows would not otherwise find out.
fn judge_windows_homes(facts: &Facts) -> Vec<Check> {
    let Some(windows) = &facts.windows else {
        return Vec::new();
    };
    let same = |a: &std::path::Path, b: &std::path::Path| Os::Windows.same_path(a, b);
    let mut checks = Vec::new();
    if let Some(local) = &windows.local_app_data
        && !same(&local.join("Pitboard"), &windows.default_pitboard_home)
    {
        checks.push(warn(
            "local_app_data",
            "local app data",
            format!(
                "Windows keeps this account's local app data in {}",
                local.display()
            ),
            format!(
                "Pitboard keeps its folder in {} whatever Windows says, so a run of the \
                 renewal schedule and one by hand use the same folder.",
                windows.default_pitboard_home.display()
            ),
        ));
    }
    if windows.roams {
        checks.push(warn(
            "roaming_profile",
            "roaming profile",
            "this Windows profile roams",
            "Claude Code's and Codex's logins in it go to every computer this account signs in \
             to, as their Credential Manager items do. Pitboard's own folder, in AppData\\Local, \
             stays on this one.",
        ));
    }
    checks
}

// Codex's home is in the known folder Profile, install.ps1's under `USERPROFILE`.
fn judge_codex_installer_home(facts: &Facts) -> Option<Check> {
    let windows = facts.windows.as_ref().filter(|w| !w.codex_home_set)?;
    let codex = |folder: &std::path::Path| folder.join(".codex");
    let profile = windows.profile.as_deref()?;
    (!Os::Windows.same_path(&codex(profile), &codex(&windows.home))).then(|| {
        warn(
            "codex_installer_home",
            "Codex's home",
            format!(
                "Codex keeps its login in {}, and its installer, install.ps1, in {}",
                codex(profile).display(),
                codex(&windows.home).display()
            ),
            "Set CODEX_HOME, so both use one folder.",
        )
    })
}

// What changes what Pitboard or a tool does on Windows, each only where it is so.
fn judge_windows_settings(facts: &Facts) -> Vec<Check> {
    let mut checks = Vec::new();
    let settings = facts.windows_settings.clone().unwrap_or_default();
    if !settings.wsl.is_empty() {
        // The ChatGPT app's WSL mode can hand a Linux codex this home (H9), not yet measured.
        checks.push(Check {
            advice: format!(
                "Claude Code and Codex installed there are installs of their own, which Pitboard \
                 on Windows does not switch. Claude Code there keeps its own login. Where Codex \
                 there uses {}, Codex's home here, a switch here changes its login too. WSL 2 \
                 runs them in a virtual machine, so Windows' list of processes does not show \
                 them running.",
                facts.codex.home.display()
            ),
            ..ok(
                "wsl",
                "WSL",
                format!("WSL holds {}", words::listed(settings.wsl)),
            )
        });
    }
    if let [first, rest @ ..] = facts.pitboards.as_slice()
        && !rest.is_empty()
    {
        let rest = rest.iter().map(|copy| copy.display().to_string()).collect();
        checks.push(Check {
            advice: "A shell runs the first, so updating another changes nothing there. Uninstall \
                     the copies you do not use."
                .into(),
            ..ok(
                "pitboard_copies",
                "copies of pitboard",
                format!("{} on PATH, then {}", first.display(), words::listed(rest)),
            )
        });
    }
    // hyper-util fills Codex's https proxy in from Windows' settings wherever HTTPS_PROXY is
    // unset or empty, whatever HTTP_PROXY and ALL_PROXY say (`codex_windows_system_proxy`).
    if let Some(server) = &settings.proxy
        && facts
            .network
            .sets(&["HTTPS_PROXY", "https_proxy"])
            .is_none()
    {
        let entries = system_proxies(server);
        let shown = entries
            .iter()
            .map(SystemProxy::masked)
            .collect::<Vec<_>>()
            .join(";");
        let codex = match entries.as_slice() {
            [one] if one.protocol.is_none() => "Codex sends its requests through it".to_string(),
            [one] => format!(
                "Codex reads the setting as one address, so it sends its requests to a proxy \
                 named {}, and they fail",
                one.masked()
            ),
            _ => format!(
                "Codex reads the setting as one address, which a proxy for each protocol is not, \
                 so its requests go {}",
                way(facts.network.sets(&["ALL_PROXY", "all_proxy"]))
            ),
        };
        // Claude Code reads no ALL_PROXY (`proxy_from_variables`).
        let claude = facts.network.sets(&["http_proxy", "HTTP_PROXY"]);
        let pitboard = facts.network.proxy.as_ref().map(|proxy| proxy.variable);
        let others = if claude.is_none() && pitboard.is_none() {
            "Claude Code and Pitboard follow only the proxy variables, so theirs go out directly"
                .to_string()
        } else {
            format!(
                "Claude Code's go {}, and Pitboard's {}",
                way(claude),
                way(pitboard)
            )
        };
        checks.push(Check {
            advice: format!(
                "{codex}. {others}: set HTTPS_PROXY where this network needs the proxy."
            ),
            ..ok(
                "system_proxy",
                "Windows proxy",
                format!("Windows' proxy settings name {shown}, and HTTPS_PROXY is not set"),
            )
        });
    }
    checks
}

fn way(variable: Option<&str>) -> String {
    match variable {
        Some(variable) => format!("through the one {variable} names"),
        None => "out directly".to_string(),
    }
}

// One entry of `ProxyServer`: `[protocol=][scheme://][user[:password]@]host:port`. Internet
// Options writes one entry for every protocol, or one a protocol, apart by `;`.
struct SystemProxy<'a> {
    protocol: Option<&'a str>,
    scheme: Option<&'a str>,
    signed_in: Option<bool>,
    server: &'a str,
}

impl SystemProxy<'_> {
    // As `proxy::shown` shows a variable's: `***` for a user name and a password.
    fn masked(&self) -> String {
        let protocol = self.protocol.map_or_else(String::new, |p| format!("{p}="));
        let scheme = self.scheme.map_or_else(String::new, |s| format!("{s}://"));
        let hidden = match self.signed_in {
            Some(true) => "***:***@",
            Some(false) => "***@",
            None => "",
        };
        format!("{protocol}{scheme}{hidden}{}", self.server)
    }
}

// Only a protocol Internet Options names, so a user name holding `=` stays hidden.
const PROTOCOLS: [&str; 4] = ["http", "https", "ftp", "socks"];

fn system_proxies(value: &str) -> Vec<SystemProxy<'_>> {
    let word = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric());
    value
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (protocol, rest) = match entry.split_once('=') {
                Some((protocol, rest))
                    if PROTOCOLS.iter().any(|p| p.eq_ignore_ascii_case(protocol)) =>
                {
                    (Some(protocol), rest)
                }
                _ => (None, entry),
            };
            let (scheme, rest) = match rest.split_once("://") {
                Some((scheme, rest)) if word(scheme) => (Some(scheme), rest),
                _ => (None, rest),
            };
            let (signed_in, server) = match rest.rsplit_once('@') {
                Some((user, server)) => (Some(user.contains(':')), server),
                None => (None, rest),
            };
            SystemProxy {
                protocol,
                scheme,
                signed_in,
                server,
            }
        })
        .collect()
}

fn judge_wsl(wsl: Option<&WslFact>) -> Option<Check> {
    let wsl = wsl?;
    let detail = match &wsl.distribution {
        Some(name) => format!("inside WSL, in {name}"),
        None => "inside WSL".to_string(),
    };
    let advice = match &wsl.codex_home_on_windows {
        None => "Pitboard here switches what is installed in this Linux. Claude Code and Codex \
                 installed on Windows keep their own logins, which Pitboard for Windows switches."
            .to_string(),
        // The ChatGPT app's WSL mode shares that folder with Codex on Windows (H9).
        Some(home) => {
            let (there, uses) = match on_windows(home) {
                Some(path) => (format!("{path} on Windows"), path),
                None => ("on a Windows drive".to_string(), "that folder".to_string()),
            };
            format!(
                "Pitboard here switches what is installed in this Linux. Claude Code installed \
                 on Windows keeps its own login, which Pitboard for Windows switches. Codex's \
                 home, {}, is {there}, where Codex here starts no shared background server. \
                 Where Codex on Windows uses {uses}, a switch here changes its login too.",
                home.display()
            )
        }
    };
    Some(Check {
        advice,
        ..ok("wsl", "WSL", detail)
    })
}

// `/mnt/c/x` is `C:\x`, as Codex's `win_path_to_wsl` maps the other way.
fn on_windows(path: &std::path::Path) -> Option<String> {
    let rest = path.to_str()?.strip_prefix("/mnt/")?;
    let mut chars = rest.chars();
    let drive = chars.next().filter(char::is_ascii_alphabetic)?;
    let tail = chars.as_str();
    if !(tail.is_empty() || tail.starts_with('/')) {
        return None;
    }
    Some(format!(
        "{}:\\{}",
        drive.to_ascii_uppercase(),
        tail.trim_start_matches('/').replace('/', "\\")
    ))
}

fn too_old(floor: crate::host::Floor) -> Option<Check> {
    let said = match floor {
        crate::host::Floor::Met => return None,
        crate::host::Floor::Below { build } => words::too_old(Some(build)),
        crate::host::Floor::Unknown => words::too_old(None),
    };
    Some(fail(
        "system_too_old",
        "runs on",
        said.because,
        format!(
            "Pitboard changes nothing here: it reads, and renews and writes nothing. {}",
            said.way_out
        ),
    ))
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

// Claude Code writes its config and its account once it has run.
fn run_claude_once(facts: &Facts) -> String {
    let found = facts.claude_program.as_deref();
    let program = words::spelling(facts.os, ProviderId::Claude, found).of(ProviderId::Claude);
    match found {
        Some(_) => format!("Run `{program}` once."),
        None => format!("Install Claude Code, then run `{program}` once."),
    }
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
    checks.extend(too_old(facts.floor));
    checks.extend(elevated(
        facts.os,
        facts.elevation,
        facts
            .windows_settings
            .as_ref()
            .and_then(|settings| settings.admin_approval),
    ));

    if let Some(tool) = facts.os.secrets_tool() {
        checks.push(match &facts.security_tool {
            Some(path) => ok("security_tool", "security tool", path.clone()),
            None => fail(
                "security_tool",
                "security tool",
                format!("{tool} is missing"),
                "Pitboard reads the keychain the same way Claude Code does. Without it, nothing works.",
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
        // Claude Code writes it on its first run, and a switch does without it.
        Err(Error::ClaudeConfigMissing { path, .. }) if facts.claude_program.is_none() => warn(
            "config_file",
            "config file",
            format!(
                "Claude Code is not installed where Pitboard looks, and {} does not exist",
                path.display()
            ),
            run_claude_once(facts),
        ),
        Err(missing @ Error::ClaudeConfigMissing { .. }) => warn(
            "config_file",
            "config file",
            missing.to_string(),
            run_claude_once(facts),
        ),
        Err(e) => fail(
            "config_file",
            "config file",
            e.to_string(),
            run_claude_once(facts),
        ),
    });

    checks.push(match &facts.config_account {
        Some(id) => ok(
            "identity",
            "identity",
            format!("{}  ·  org {}", id.email, id.organization_uuid),
        ),
        None => warn(
            "identity",
            "identity",
            "Claude Code has not recorded a signed-in account",
            format!(
                "It writes this after its first successful call. {}",
                run_claude_once(facts)
            ),
        ),
    });
    checks.extend(judge_in_use(facts, moment));

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
    if let (Some(price), Ok(store::Backend::CredentialManager)) =
        (facts.credential_cost, &facts.backend)
    {
        checks.push(judge_pieces(price, &facts.credential_parts));
    } else if let Some(price) = facts.credential_cost {
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

    if facts.no_argv && facts.os == Os::Windows {
        checks.push(warn(
            "no_argv",
            "PITBOARD_NO_ARGV",
            "set, and it changes nothing on Windows",
            "Claude Code's Credential Manager store takes a login of any size it writes in \
             pieces, never on an argument line. Unset it.",
        ));
    }

    checks.push(match &facts.backend {
        Ok(store::Backend::Keychain) => ok("credential_store", "credential store", "keychain"),
        // Enterprise persistence (`credman_attributes`).
        Ok(store::Backend::CredentialManager) => {
            let detail = format!(
                "Credential Manager  ·  {}/{}  ·  every logon of yours on this PC reads it",
                facts.service, facts.account
            );
            if facts.windows.as_ref().is_some_and(|homes| homes.roams) {
                warn(
                    "credential_store",
                    "credential store",
                    detail,
                    "Your profile roams, and Credential Manager takes Claude Code's login with it \
                     to every PC you sign in to, so a switch here changes the login there too.",
                )
            } else {
                ok("credential_store", "credential store", detail)
            }
        }
        Ok(store::Backend::File) => {
            let detail = format!("plaintext file  ·  {}", facts.credential_file.display());
            match facts.os {
                Os::MacOs => warn(
                    "credential_store",
                    "credential store",
                    detail,
                    "Claude Code fell back to a file, which means a keychain write failed at some point.",
                ),
                // The chain is the file on Windows only where Credential Manager holds no login.
                Os::Linux | Os::Windows => ok("credential_store", "credential store", detail),
            }
        }
        // No tool keeps its login in Pitboard's sealed files.
        Ok(store::Backend::Unknown | store::Backend::SealedFile) => fail(
            "credential_store",
            "credential store",
            "a store Pitboard does not read yet",
            "Treat this as unknown, never as empty.",
        ),
        // As a switch refuses with `live_credential_elsewhere`: what would follow Claude Code
        // moving where it keeps a login.
        Ok(store::Backend::Absent) => match &facts.config_account {
            Some(named) => fail(
                "credential_store",
                "credential store",
                words::named_but_not_found(facts.os, &named.email),
                "Pitboard will not write a login where nobody reads it. Check for a Pitboard \
                 update; if there is none, this is worth reporting.",
            ),
            None => warn(
                "credential_store",
                "credential store",
                "no credential in any backend",
                "Nothing is signed in for this slot.",
            ),
        },
        Err(e) => fail(
            "credential_store",
            "credential store",
            e.to_string(),
            match facts.os {
                Os::MacOs | Os::Linux => "Treat this as unknown, never as empty.",
                Os::Windows => {
                    "Treat this as unknown, never as empty. While nothing turns Claude Code's \
                     Credential Manager store on, its sessions read only .credentials.json, so a \
                     login left only in Credential Manager is signed out to them."
                }
            },
        ),
    });

    checks.push(judge_credential(facts));
    checks.extend(judge_fallback_login(facts));

    checks.push(match &facts.home_access {
        None => ok(
            "home",
            "Pitboard home",
            format!("{} (not created yet)", facts.home.display()),
        ),
        Some(access) if !access.shared => {
            ok("home", "Pitboard home", facts.home.display().to_string())
        }
        Some(access) => warn(
            "home",
            "Pitboard home",
            format!("{} is {}", facts.home.display(), access.described),
            format!(
                "Park names contain account identifiers, so only you should read it{}",
                running(&facts.os.make_private_commands(
                    Kind::Directory,
                    &[&facts.home.display().to_string()],
                    facts.sid.as_deref(),
                ))
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
            .map(|(path, _, access)| format!("{path} is {}", access.described))
            .collect();
        let sid = facts.sid.as_deref();
        let commands: Vec<String> = match facts.os {
            Os::MacOs | Os::Linux => facts.os.make_private_commands(
                Kind::Any,
                &facts
                    .readable_by_others
                    .iter()
                    .map(|(path, ..)| path.as_str())
                    .collect::<Vec<_>>(),
                sid,
            ),
            Os::Windows => facts
                .readable_by_others
                .iter()
                .flat_map(|(path, kind, _)| facts.os.make_private_commands(*kind, &[path], sid))
                .collect(),
        };
        fail(
            "private_on_disk",
            "logins on disk",
            names.join("; "),
            format!(
                "{}{}",
                match facts.os {
                    Os::MacOs | Os::Linux =>
                        "These hold usable OAuth tokens in plain text, which is how Claude \
                         Code stores them where there is no keychain. Anyone else on this \
                         machine can read them",
                    Os::Windows =>
                        "These hold a login: Claude Code's file in plain text, a sealed park \
                         only this account opens. Claude Code's file takes the access list of \
                         the folder it is in, and a seal stops reading, not writing. Anyone \
                         else on this PC can read or change them",
                },
                running(&commands),
            ),
        )
    });

    let lost = lost_parks(facts);
    checks.extend(judge_vault(facts, &lost));

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
                _ => "Pitboard will not switch until its account list can be read.",
            },
        ),
    });
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
            .map(|p| judge_park(p, facts.now, moment)),
    );
    if let Some(refusal) = &facts.stuck {
        checks.push(warn(
            "interrupted_switch",
            "interrupted switch",
            refusal.clone(),
            "If it still cannot be finished, `pitboard abandon` gives up on the switch and \
             keeps every login.",
        ));
    } else if facts.interrupted {
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
        // The two kinds apart: a folder is kept until its account is forgotten.
        let folders = state
            .discarded
            .iter()
            .filter(|d| d.kind == crate::state::ParkKind::Folder)
            .count();
        let vault = state.discarded.len() - folders;
        let (detail, advice) = match (vault, folders) {
            (vault, 0) => (
                format!("{vault} waiting to be deleted"),
                "Pitboard deletes them on its next change; if they stay, check the keychain is unlocked.",
            ),
            (0, folders) => (
                format!("{folders} folder(s) waiting to be deleted"),
                "Pitboard deletes them on its next change.",
            ),
            (vault, folders) => (
                format!("{vault} waiting to be deleted, and {folders} folder(s)"),
                "Pitboard deletes them on its next change; if they stay, check the keychain is unlocked.",
            ),
        };
        checks.push(warn("discarded", "old parked logins", detail, advice));
    }

    if !facts.machine_id_known {
        checks.push(warn(
            "machine_id",
            "machine id",
            "this machine has no stable identifier",
            format!(
                "Pitboard cannot tell this machine from another that also lacks one, so it \
                 cannot refuse state copied between them. Never copy {} between machines.",
                facts.home.display()
            ),
        ));
    } else if facts.os == Os::Windows {
        checks.push(Check {
            advice: "Pitboard tells this PC apart by it, and a copy of the PC keeps it, such as a \
                     virtual machine copied whole. Such a copy takes this PC's accounts for its \
                     own, and both renew the same parked logins: on a copy, forget each account \
                     and enrol it again with `--sign-in`."
                .into(),
            ..ok("machine_id", "machine id", "Windows' MachineGuid")
        });
    }

    checks.extend(judge_windows_homes(facts));
    checks.extend(judge_windows_settings(facts));
    checks.extend(judge_wsl(facts.wsl.as_ref()));
    checks.push(judge_storage_v5(facts));
    checks.push(judge_daemon(facts));
    checks.push(judge_pending(facts));
    checks.extend(judge_schedule(facts));
    checks.extend(judge_schedule_proxy(facts));
    checks.extend(judge_watch_service(facts));
    checks.push(judge_claude_version(facts));
    checks.extend(judge_status_line(facts.status_line.as_ref()));
    checks.push(judge_auth(facts));
    checks.push(judge_asking(facts));
    checks.push(judge_network(&facts.network));
    checks.extend(
        claude_parks
            .iter()
            .filter_map(|p| judge_dormant(p, facts.now)),
    );
    // Only where there is a Codex to say something about. A machine without it reads exactly
    // as it did before Pitboard knew Codex existed.
    if codex_here || !codex_parks.is_empty() {
        checks.extend(judge_codex_installer_home(facts));
        checks.extend(judge_codex(
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
                judge_desktop(facts.os, desktop, state, &facts.parks, facts.now, moment)
            }
            (Os::Linux, Err(_)) | (Os::Windows, _) => vec![judge_desktop_off_macos(state)],
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

/// A refresh token's own life, from a real renewal answer: thirty days. An account that has
/// been parked for longer than that without being switched to has had its login kept alive
/// purely by Pitboard, through at least one whole token lifetime, for nobody.
const A_TOKEN_LIFETIME: i64 = 30 * 86_400;

/// An account nobody has come back to.
///
/// Pitboard renews a parked login for as long as the account is enrolled, so one enrolled
/// once and never used again keeps a live, continuously rotated refresh token on this
/// machine indefinitely. That is a defensible thing to do and an indefensible thing to do
/// silently. Nothing is dropped on a timer Pitboard chose: the threshold here is the
/// token's own lifetime, and all it does is say so.
fn judge_dormant(park: &ParkFact, now: i64) -> Option<Check> {
    // The sentence below says Pitboard renews the login, which it does only for a document.
    match crate::provider::of(park.provider) {
        LoginKind::Document(_) => {}
        LoginKind::Folder(_) => return None,
    }
    if park.active || park.park.is_none() {
        return None;
    }
    let dormant_for = now - park.last_used_at?;
    if dormant_for < A_TOKEN_LIFETIME {
        return None;
    }
    Some(warn(
        account_code(
            park.provider,
            "dormant_account",
            "codex_dormant_account",
            "desktop_dormant_account",
        ),
        format!("account {}", park.typed()),
        format!(
            "not switched to for {}; Pitboard has kept its login alive that whole time",
            words::span(dormant_for)
        ),
        format!(
            "Every `pitboard` renews it, so its refresh token is rotated and kept live on \
             this machine for as long as it stays enrolled. If you are not coming back to \
             it, `pitboard forget {}` deletes the login and the record.",
            park.typed()
        ),
    ))
}

/// Whose login Claude Code has stored, as Anthropic last said, which every reader that does
/// not ask takes as the account in use, and whether something may have signed in since.
/// Doctor asks nobody, so a login renewed since, which Claude Code does every few hours, is
/// said and is no warning; Claude Code's config naming another account is, since only a
/// read that asks can say whether another login came with it. Nothing is said where nothing
/// is recorded and no Claude Code account is enrolled.
fn judge_in_use(facts: &Facts, moment: &dyn Fn(i64, i64) -> String) -> Option<Check> {
    let (code, name) = ("in_use", "in use");
    let known = facts.in_use.as_ref()?;
    let state = facts.state.as_ref().ok()?;
    let enrolled = facts.parks.iter().any(|p| p.provider == ProviderId::Claude);
    if known.last.is_none() && !enrolled {
        return None;
    }
    let ask = "Run `pitboard status`, which asks Anthropic whose login Claude Code has stored.";
    let Some(last) = &known.last else {
        return Some(warn(
            code,
            name,
            "Anthropic has not been asked whose login Claude Code has stored",
            ask,
        ));
    };
    let account = Stored::of(state, ProviderId::Claude, last.owner.as_ref());
    let in_use = account.said("no account");
    let config =
        Stored::of(state, ProviderId::Claude, known.own_record.as_ref()).said("no account");
    let when = moment(last.known_at, facts.now);
    let then = match &last.owner {
        Some(_) => format!("Anthropic named {in_use} {when}"),
        None => format!("Pitboard found no login stored {when}"),
    };
    match known.unconfirmed() {
        Some(crate::in_use::Doubt::NeverEstablished) => {
            return Some(warn(
                code,
                name,
                format!(
                    "Anthropic has not been asked whose login Claude Code has stored since \
                     Pitboard was updated; {in_use} is the account it last switched to"
                ),
                ask,
            ));
        }
        Some(crate::in_use::Doubt::NamedMoved) => {
            return Some(warn(
                code,
                name,
                format!(
                    "Claude Code's config has named {config} since {then}, so another login \
                     may be stored"
                ),
                ask,
            ));
        }
        Some(crate::in_use::Doubt::LoginUnreadable) | None => {}
    }
    if known.named_another(ProviderId::Claude).is_some() {
        return Some(warn(
            code,
            name,
            format!("{in_use}, as Anthropic said {when}, and Claude Code's config names {config}"),
            format!(
                "`/status` in Claude Code shows {config}. {}",
                account.how_to_name()
            ),
        ));
    }
    // The login stored now by its fingerprint, empty for a login with no refresh token:
    // `Some(None)` where none is stored, and `None` where the store cannot be read.
    let stored = match &facts.credential {
        Ok(Some(document)) if document.get("claudeAiOauth").is_some() => {
            Some(Some(crate::provider::claude_code().fingerprint(document)))
        }
        Ok(_) => Some(None),
        Err(_) => None,
    };
    let renewed = "the next `pitboard status` asks whose it is";
    Some(ok(
        code,
        name,
        match (&last.owner, stored) {
            (Some(_), Some(Some(now))) if now.is_empty() || now != last.login => format!(
                "{in_use}, as Anthropic said {when} of a login renewed or replaced since; \
                 {renewed}"
            ),
            (Some(_), Some(None)) => {
                format!("{in_use}, as Anthropic said {when}; no login is stored since")
            }
            (Some(_), _) => format!("{in_use}, as Anthropic said {when}"),
            (None, Some(Some(_))) => {
                format!("no login stored, as Pitboard found {when}; one is since, and {renewed}")
            }
            (None, _) => format!("no login stored, as Pitboard found {when}"),
        },
    ))
}

fn judge_credential(facts: &Facts) -> Check {
    match &facts.credential {
        Ok(Some(doc)) => {
            let keys: Vec<&str> = doc
                .as_object()
                .map(|o| o.keys().map(String::as_str).collect())
                .unwrap_or_default();
            if crate::provider::claude::document::signed_out(doc) {
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
            let Some(at) = crate::provider::claude_code()
                .expiry(doc)
                .refresh_expires_at
            else {
                return ok(
                    "credential",
                    "credential",
                    format!("refresh {fingerprint}  ·  keys {keys:?}"),
                );
            };
            let life = words::parked_life_column(at, facts.now);
            let detail = format!("refresh {fingerprint}  ·  {life}  ·  keys {keys:?}");
            if at <= facts.now {
                warn(
                    "credential",
                    "credential",
                    detail,
                    "This login has expired: sign in to Claude Code again.",
                )
            } else if renewal_due(at, facts.now) {
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
        // Read in 2.1.294 (the register's `locked_keychain_keeps_last_login`): a session
        // keeps serving the login it last read while the keychain is locked, and one that
        // read none reads the keychain as empty and falls through to the file.
        Err(store::Error::Locked) if facts.os == Os::Windows => fail(
            "credential",
            "credential",
            "Credential Manager cannot be reached from this logon",
            format!(
                "A network logon, as {}, has no Credential Manager. Run Pitboard from a desktop \
                 session. A Claude Code session started from this logon reads only {}.",
                words::SSH_SIGN_IN,
                facts.credential_file.display()
            ),
        ),
        Err(store::Error::Locked) => fail(
            "credential",
            "credential",
            "the keychain is locked, and cannot ask to be unlocked from here",
            format!(
                "Unlock it with `security unlock-keychain`, or run Pitboard from a desktop \
                 session. Until then a Claude Code session here keeps the login it last read \
                 and follows no switch{}",
                match facts
                    .fallback_login
                    .as_ref()
                    .map(|left| (left.path.display(), &left.fingerprint))
                {
                    Some((path, Ok(Some(_)))) => {
                        format!(", and one started here signs in with the login in {path}.")
                    }
                    Some((path, Err(_))) => format!(
                        ", and whether one started here signs in with what {path} holds cannot \
                         be told, since Pitboard could not read it."
                    ),
                    Some((_, Ok(None))) | None => ", and one started here is signed out.".into(),
                }
            ),
        ),
        // `claude auth login` writes nothing over it (`credman_move_lost_a_piece`).
        Err(store::Error::Broken { why, spelling }) => fail(
            "credential",
            "credential",
            format!("{why}, which Claude Code cannot read either"),
            format!(
                "Run `{} auth logout`, which deletes it, then sign in to Claude Code again. \
                 Signing in before that leaves it as it is.",
                spelling.of(ProviderId::Claude)
            ),
        ),
        Err(e) => fail(
            "credential",
            "credential",
            e.to_string(),
            "Do not write to the store while this is failing.",
        ),
    }
}

// What leaves the file and keeps it there: `locked_sign_in_writes_fallback` and
// `fallback_outlives_keychain_writes` on macOS, `credman_unreachable_writes_file` and
// `credman_migration` on Windows.
fn judge_fallback_login(facts: &Facts) -> Option<Check> {
    let left = facts.fallback_login.as_ref()?;
    let path = left.path.display();
    let running = format!(
        "While it is there, Claude Code sessions already running at a switch {}.",
        words::kept_until_renewed()
    );
    let cannot = words::out_of_reach(facts.os);
    let (could_not, such_as, as_login) = match facts.os {
        Os::MacOs | Os::Linux => (
            "the keychain could not be read",
            "in a session started over SSH".to_owned(),
            "`/login` over SSH does".to_owned(),
        ),
        Os::Windows => (
            "Credential Manager could not be reached",
            format!("in {}", words::SSH_SIGN_IN),
            format!("`/login` does in {}", words::SSH_SIGN_IN),
        ),
    };
    let (detail, advice) = match &left.fingerprint {
        Ok(Some(fingerprint)) => (
            format!(
                "{path}  ·  refresh {}",
                if fingerprint.is_empty() {
                    "none"
                } else {
                    fingerprint.as_str()
                }
            ),
            format!(
                "Claude Code signs in with this wherever it {cannot}, such as {such_as}, and no \
                 switch reaches it. A sign-in made where {could_not} leaves one, as {as_login}. \
                 {running} `pitboard stow` keeps this login for its account, where Pitboard \
                 holds no other it can switch to, then deletes the file, which leaves one login \
                 for every session."
            ),
        ),
        Ok(None) => (
            format!("{path}  ·  no login in it"),
            format!(
                "It holds no Claude Code login. {running} `pitboard stow` deletes it, which \
                 lets them follow a switch."
            ),
        ),
        Err(e) => (
            format!("{path}  ·  not read: {e}"),
            format!(
                "Pitboard could not read it, so whether it holds a login, which a session that \
                 {cannot} signs in with, cannot be told, and `pitboard stow` cannot put it away. \
                 {running} Deleting it lets them follow a switch, and takes whatever it holds \
                 with it: `{}`.",
                facts.os.remove_file_command(&left.path)
            ),
        ),
    };
    Some(warn("fallback_login", "fallback login", detail, advice))
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

// The code `CryptUnprotectData` returns after an administrator's password reset, as RF-D1d
// measured (run 38797729796), where the account's master key no longer opens the blob.
const DPAPI_KEY_RESET: i32 = 0x8009_000B_u32 as i32;

// Every sealed park lost at once, as after an administrator resets the password, is one
// finding. A seal that reads 0x8009000B for all of them points at a reset or a restored
// profile; any other code (a logon without the key, as over SSH) gets neutral wording.
fn judge_vault(facts: &Facts, lost: &[&ParkFact]) -> Option<Check> {
    if facts.vault != store::Backend::SealedFile {
        return None;
    }
    if lost.is_empty() {
        return Some(ok(
            "vault",
            "parked logins",
            "sealed to this Windows account, not plain text",
        ));
    }
    let again: Vec<String> = lost
        .iter()
        .map(|p| format!("pitboard enroll {} --sign-in", p.typed()))
        .collect();
    let a_reset = lost.iter().all(|p| {
        matches!(
            p.unreadable,
            Some(Unreadable::Sealed(Some(DPAPI_KEY_RESET)))
        )
    });
    let way_out = if a_reset {
        format!(
            "An administrator's reset of your Windows password, or a profile restored on \
             another PC, leaves them sealed with a key this account no longer has. Sign in to \
             each again{}",
            running(&again)
        )
    } else {
        format!(
            "They do not open in this logon. Run `pitboard doctor` from a desktop session \
             before signing in again{}",
            running(&again)
        )
    };
    Some(fail(
        "vault",
        "parked logins",
        format!(
            "none of the {} parked logins opens for this Windows account",
            lost.len()
        ),
        way_out,
    ))
}

// Two sealed parks or more in a sealed vault, each failing to open: a lost key, not a missing
// or damaged file, so a reset or a restored profile is told from a stray unreadable park.
fn lost_parks(facts: &Facts) -> Vec<&ParkFact> {
    let parked: Vec<&ParkFact> = facts
        .parks
        .iter()
        .filter(|p| {
            p.park
                .as_ref()
                .is_some_and(|park| park.kind == crate::state::ParkKind::Vault)
        })
        .collect();
    let all = parked.len() >= 2
        && parked
            .iter()
            .all(|p| matches!(p.unreadable, Some(Unreadable::Sealed(_))));
    if facts.vault == store::Backend::SealedFile && all {
        parked
    } else {
        Vec::new()
    }
}

fn judge_park(fact: &ParkFact, now: i64, moment: &dyn Fn(i64, i64) -> String) -> Check {
    let code = account_code(
        fact.provider,
        "parked_login",
        "codex_parked_login",
        "desktop_parked_login",
    );
    let name = format!("account {}", fact.typed());
    let renew = format!("Run `pitboard enroll {} --sign-in`.", fact.typed());
    let Some(park) = &fact.park else {
        return if fact.active {
            ok(code, name, "signed in; parked when you switch away")
        } else {
            warn(code, name, "nothing parked to switch to", renew)
        };
    };
    match &fact.unreadable {
        Some(Unreadable::Locked) => {
            return warn(
                code,
                name,
                "not read: the keychain is locked",
                "Unlock it, then run `pitboard doctor` again.",
            );
        }
        Some(Unreadable::Sealed(_)) => {
            return fail(
                code,
                name,
                "its parked login does not open in this logon",
                renew,
            );
        }
        Some(Unreadable::Broken(why)) => {
            return fail(
                code,
                name,
                format!("its parked login is unusable: {why}"),
                renew,
            );
        }
        None => {}
    }
    let Some(at) = park.refresh_expires_at else {
        return ok(code, name, "parked");
    };
    // The column `pitboard status` shows, after this check's own words.
    let life = words::parked_life_column(at, now);
    if !park.restorable_at(now) {
        let when = moment(at, now);
        warn(code, name, format!("its parked login {life} {when}"), renew)
    } else if renewal_due(at, now) {
        warn(code, name, format!("its parked login {life}"), renew)
    } else {
        ok(code, name, format!("parked, {life}"))
    }
}

/// Claude Code's successor credential backend.
///
/// Measured in 2.1.278: the flag does not move the login out of the keychain. The live
/// chain is built as keychain-with-plaintext-fallback either way, and the flag only decides
/// what backs the fallback half, and only for a caller that hands a backend in. An ordinary
/// `claude` hands none in, so the fallback stays `<storage dir>/.credentials.json`. The
/// combination worth saying something about is the flag on *and* the login living in the
/// fallback, because that is the one case where what Pitboard reads may not be what a
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
            "Pitboard reads the plaintext file. If Claude Code was given a backend of its \
             own, that is not the same file. Check for an update before switching.",
        ),
        _ => ok(
            "storage_v5",
            "storage v5",
            "switched on; the keychain is still where the login is",
        ),
    }
}

// On Windows: one item to 2400 bytes, 2400-character pieces of base64 to 256 (`credman_chunking`).
fn judge_pieces(price: store::Cost, parts: &[(String, usize)]) -> Check {
    let (bytes, limit) = (price.needs, price.limit);
    match crate::provider::claude::pieces::pieces_for(bytes) {
        None => fail(
            "credential_size",
            "login size",
            format!(
                "{}{}",
                words::past_credential_manager(bytes, limit),
                biggest(parts)
            ),
            "Neither Pitboard nor Claude Code can write a login this size there. Sign out of \
             MCP servers you no longer use to make it smaller.",
        ),
        Some(0) => ok(
            "credential_size",
            "login size",
            format!("{bytes} bytes, in one item"),
        ),
        Some(pieces) => ok(
            "credential_size",
            "login size",
            format!(
                "{bytes} bytes, in {pieces} pieces of {} characters",
                crate::provider::claude::pieces::PIECE
            ),
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

/// Whether Pitboard is waiting before asking Anthropic about anything.
///
/// Ordinarily nothing is waiting: an account is asked about whenever its tightest limit
/// could have moved by a percentage point, and that is the floor rather than a wait. A wait
/// means Anthropic asked for less traffic or could not be reached, and a person watching a
/// number not move deserves to know which.
fn judge_asking(facts: &Facts) -> Check {
    // Which services Pitboard asks, named by the tools that have accounts here, and which
    // of them are being held back: a hold is a service's answer, so blaming the wrong one
    // sends somebody to look at a service that is answering normally.
    let tool_of = |id: &str| {
        facts
            .state
            .as_ref()
            .ok()
            .and_then(|s| s.owner_of_park(id))
            .map(crate::state::Account::provider)
    };
    let services = |tools: &mut Vec<ProviderId>| {
        tools.sort();
        if tools.is_empty() {
            tools.push(ProviderId::Claude);
        }
        let mut named: Vec<&str> = Vec::new();
        for service in tools.iter().map(|t| t.service()) {
            if !named.contains(&service) {
                named.push(service);
            }
        }
        named.join(" and ")
    };
    let mut asked: Vec<ProviderId> = facts
        .state
        .as_ref()
        .map(|s| {
            s.accounts
                .iter()
                .map(crate::state::Account::provider)
                .filter(|tool| tool.reads_usage())
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
                    words::span(longest)
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

/// How Pitboard's requests leave this machine: directly, or through the HTTP proxy the
/// environment names, and from which variable, with the hosts Pitboard calls that the
/// variable exempting hosts sends around it. It warns where Pitboard passes over a variable
/// that holds no proxy's address, since its requests then go where the person may have meant
/// them not to.
///
/// A SOCKS proxy, which Pitboard does not use yet, is named in the words a request fails
/// with, then the proxy, and warns, since each request to a host Pitboard calls that the
/// variable exempting hosts does not name fails. With every one of them exempt, nothing
/// fails, and it does not warn for that.
fn judge_network(network: &NetworkFact) -> Check {
    const NAME: &str = "network";
    let (passed, set_right) = match network.passed_over.as_slice() {
        [] => (None, ""),
        [one] => (
            Some(format!(
                "{one} holds no proxy's address Pitboard can read, so it is passed over"
            )),
            "Set it to a proxy's address, such as http://proxy.example.com:8080, or unset it.",
        ),
        many => (
            Some(format!(
                "{} hold no proxy's address Pitboard can read, so they are passed over",
                words::listed(many.iter().map(|n| (*n).to_string()).collect())
            )),
            "Set each to a proxy's address, such as http://proxy.example.com:8080, or unset them.",
        ),
    };
    let Some(proxy) = &network.proxy else {
        return match passed {
            None => ok("network", NAME, "direct: no variable names a proxy"),
            Some(passed) => warn("network", NAME, format!("direct: {passed}"), set_right),
        };
    };
    let fails = proxy.fails;
    let mut detail = if !proxy.socks {
        format!("through {}, from {}", proxy.address, proxy.variable)
    } else {
        format!(
            "{}: {}, {}",
            if fails { "requests fail" } else { "direct" },
            crate::proxy::refused(proxy.variable),
            proxy.address
        )
    };
    for clause in exempting(proxy).iter().chain(&passed) {
        let _ = write!(detail, "; {clause}");
    }
    if fails {
        let (each, them) = if network.passed_over.is_empty() {
            (proxy.variable.to_string(), "it")
        } else {
            let mut names: Vec<String> = network
                .passed_over
                .iter()
                .map(|name| (*name).to_string())
                .collect();
            names.push(proxy.variable.to_string());
            (format!("each of {}", words::listed(names)), "them")
        };
        return warn(
            "network",
            NAME,
            detail,
            format!(
                "Pitboard goes through HTTP and HTTPS proxies only. For its requests to go \
                 out, set {each} to an HTTP proxy's address, such as \
                 http://proxy.example.com:8080, or unset {them}."
            ),
        );
    }
    match passed {
        None => ok("network", NAME, detail),
        Some(_) => warn("network", NAME, detail, set_right),
    }
}

/// What the variable exempting hosts does with the hosts Pitboard calls, where one is set:
/// "NO_PROXY exempts api.anthropic.com, which Pitboard reaches directly".
fn exempting(proxy: &ProxyFact) -> Option<String> {
    proxy.exempting.map(|exempting| {
        if proxy.exempt.is_empty() {
            format!("{exempting} exempts none of the hosts Pitboard calls")
        } else if proxy.exempt.len() == crate::proxy::called().len() {
            format!("{exempting} exempts every host Pitboard calls, which it reaches directly")
        } else {
            format!(
                "{exempting} exempts {}, which Pitboard reaches directly",
                words::listed(proxy.exempt.clone())
            )
        }
    })
}

// From files as well, since an app launched from Finder has no environment to read.
fn judge_auth(facts: &Facts) -> Check {
    if facts.auth_overrides.is_empty() {
        let read = if facts.fallback_login.is_some() {
            "the stored login, which is what Pitboard moves, once a session reads it again"
        } else {
            "the stored login, which is what Pitboard moves"
        };
        return ok("auth_source", "what a session authenticates with", read);
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
         that Pitboard is moving a login nothing reads.",
    )
}

/// Which Claude Code is installed, against which one Pitboard's facts were read.
///
/// Stated rather than warned about. Claude Code ships several times a week, so a mismatch
/// is the ordinary state of the world within days of a release and warning about it would
/// be noise on every machine. What is worth a warning is an assumption that has actually
/// stopped holding, which is a probe's job and not a version number's.
fn judge_claude_version(facts: &Facts) -> Check {
    let verified = read_from(ProviderId::Claude, facts.os);
    match facts.claude_version.as_deref() {
        None => ok(
            "claude_version",
            "Claude Code build",
            format!("not found here; Pitboard's facts were read from {verified}"),
        ),
        Some(installed) if installed == verified => ok(
            "claude_version",
            "Claude Code build",
            format!("{installed}, which is what Pitboard's facts were read from"),
        ),
        Some(installed) => ok(
            "claude_version",
            "Claude Code build",
            format!("{installed} installed; Pitboard's facts were read from {verified}"),
        ),
    }
}

fn read_from(tool: ProviderId, os: Os) -> &'static str {
    crate::assumptions::verified_against(tool, crate::assumptions::Platform::of(os))
}

// A path to pitboard.exe, or a script of the person's own, needs no PATH of Pitboard's.
fn runs_bare_pitboard(command: &str) -> bool {
    command
        .split_whitespace()
        .next()
        .map(|program| program.trim_matches(['"', '\'']).to_ascii_lowercase())
        .is_some_and(|program| program == "pitboard" || program == "pitboard.exe")
}

// Claude Code's PATH is the one this runs with, as the shell it starts takes it.
fn judge_status_line(line: Option<&StatusLineFact>) -> Option<Check> {
    let line = line?;
    let runs = match (&line.git_bash.found, &line.powershell) {
        (Some(bash), _) => format!("runs the status line in Git Bash, {}", bash.display()),
        (None, Some(claude::PowerShell::Seven(found))) => format!(
            "runs the status line in PowerShell 7, {}, finding no Git Bash",
            found.display()
        ),
        (None, Some(claude::PowerShell::Windows(found))) => format!(
            "runs the status line in Windows PowerShell, {}, finding no Git Bash",
            found.display()
        ),
        (None, None) => "finds neither Git Bash nor PowerShell to run the status line in".into(),
    };
    let passed_over = line
        .git_bash
        .passed_over
        .as_ref()
        .map_or_else(String::new, |named| {
            format!(
                "; it passes over {}, which CLAUDE_CODE_GIT_BASH_PATH names",
                named.display()
            )
        });
    Some(match &line.pitboard {
        Some(found) => ok(
            "status_line",
            "status line",
            format!(
                "Claude Code {runs}; `pitboard` there is {}{passed_over}",
                found.display()
            ),
        ),
        None if line.runs_pitboard => warn(
            "status_line",
            "status line",
            format!("Claude Code {runs}; no `pitboard` is on its PATH{passed_over}"),
            "Add the folder pitboard.exe is in to your PATH, then start Claude Code again.",
        ),
        None => ok(
            "status_line",
            "status line",
            format!("Claude Code {runs}; no `pitboard` is on its PATH{passed_over}"),
        ),
    })
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
            "Pitboard wrote these names down before creating a login in them and cannot \
             read them back to find out what is there. Unlock the keychain and run any \
             Pitboard command; it resolves them before doing anything else.",
        ),
    }
}

/// The schedule runs a Pitboard by its path, and the path can stop leading anywhere after
/// it was written: an upgrade that deletes the version it named, an app moved or thrown
/// away. The scheduler then fails once a day where nobody looks, and the parked logins it
/// was keeping alive run out. Nothing is said where there is no schedule.
///
/// An app up to 0.3.0 scheduled itself rather than a command line, and that app renews
/// nothing when it is started with `renew`, so that is said as well.
fn judge_schedule(facts: &Facts) -> Option<Check> {
    let again = again(facts.os);
    let schedule = facts.schedule.as_ref()?;
    let Some(program) = &schedule.program else {
        return Some(warn(
            "schedule",
            "renewal schedule",
            format!("{} does not say which Pitboard it runs", schedule.location),
            again,
        ));
    };
    match crate::schedule::fault(program, schedule.program_found) {
        Some(Fault::Gone) => {
            return Some(fail(
                "schedule",
                "renewal schedule",
                format!("runs {}, which is not there any more", program.display()),
                again,
            ));
        }
        Some(Fault::TheApp) => {
            return Some(fail(
                "schedule",
                "renewal schedule",
                format!(
                    "runs {}, which is the app itself and not a command line",
                    program.display()
                ),
                again,
            ));
        }
        None => {}
    }
    let runs = format!("daily  ·  runs {}", program.display());
    let Some(task) = &schedule.task else {
        return Some(ok("schedule", "renewal schedule", runs));
    };
    if !task.drifted.is_empty() {
        let drifted = task.drifted.iter().map(drift).collect();
        return Some(warn(
            "schedule",
            "renewal schedule",
            format!(
                "{runs}, but in Task Scheduler it {}",
                words::listed(drifted)
            ),
            again,
        ));
    }
    let ago = |at: i64| words::span(facts.now - at);
    Some(match (task.failed(), task.last_run) {
        (Some(result), at) => warn(
            "schedule",
            "renewal schedule",
            format!(
                "{runs}, and its last run{} ended {}",
                at.map(|at| format!(", {} ago,", ago(at)))
                    .unwrap_or_default(),
                ended(result)
            ),
            "Run `pitboard renew` to see what it says.",
        ),
        (None, Some(at)) => ok(
            "schedule",
            "renewal schedule",
            format!("{runs}  ·  last ran {} ago", ago(at)),
        ),
        (None, None) => ok(
            "schedule",
            "renewal schedule",
            format!("{runs}  ·  not run yet"),
        ),
    })
}

// Nothing is said where there is no service. A Pitboard gone from its path or a last run that
// failed stops every switch it would make, where nobody looks.
fn judge_watch_service(facts: &Facts) -> Option<Check> {
    let service = facts.watch_service.as_ref()?;
    let again = "Install it again with `pitboard watch install`.";
    let Some(program) = &service.program else {
        return Some(warn(
            "watch_service",
            "watch service",
            format!("{} does not say which Pitboard it runs", service.location),
            again,
        ));
    };
    if !service.program_found {
        return Some(fail(
            "watch_service",
            "watch service",
            format!("runs {}, which is not there any more", program.display()),
            again,
        ));
    }
    let runs = match service.at {
        Some(at) => format!(
            "from sign-in  ·  switches at {}%  ·  runs {}",
            at.percent(),
            program.display()
        ),
        None => format!("from sign-in  ·  runs {}", program.display()),
    };
    Some(match service.task.as_ref().and_then(Task::failed) {
        Some(result) => warn(
            "watch_service",
            "watch service",
            format!("{runs}, and its last run ended {}", ended(result)),
            "Run `pitboard watch` to see what it says.",
        ),
        None => ok("watch_service", "watch service", runs),
    })
}

// As Task Scheduler's own settings word each.
fn drift(drift: &Drift) -> String {
    match drift {
        Drift::OnAcPowerOnly => "starts only if the computer is on AC power",
        Drift::StopsOnBattery => "stops if the computer switches to battery power",
        Drift::SkipsMissedStarts => {
            "does not run as soon as possible after a scheduled start is missed"
        }
        Drift::Disabled => "is disabled",
    }
    .to_string()
}

// A program's exit code, or one of Task Scheduler's own results.
fn ended(result: i32) -> String {
    if (0..=255).contains(&result) {
        format!("with exit code {result}")
    } else {
        format!("with 0x{:08X}", result as u32)
    }
}

/// The system starts the schedule's runs with its own environment, not the person's shell,
/// so they are given the proxy variables of the Pitboard that installed it. Where those send
/// requests another way than this run does, through another proxy or none, with another user
/// name or password, or with other hosts exempt, that is said, credentials as `***`. Nothing
/// is said where they go the same way, or where there is no schedule.
///
/// From a terminal it is a warning: the terminal's proxy is the one the person works with,
/// and installing the schedule again from there gives it that proxy. Where that proxy is a
/// SOCKS one whose requests fail, the advice is to change the variable naming it first, so
/// the schedule is not given a proxy that fails its requests too. In the app it is no
/// warning.
/// The app is started with the system's environment as the schedule's runs are, which on
/// macOS names no proxy unless `launchctl setenv` set one, so a schedule installed from a terminal with
/// a proxy differs from it for good, and the app's switch, the one way it installs the
/// schedule, would give the schedule the app's proxy variables and take the terminal's away.
/// There the line says what each goes through, and how each way of installing it chooses.
fn judge_schedule_proxy(facts: &Facts) -> Option<Check> {
    let schedule = facts.schedule.as_ref()?;
    if !schedule.network_differs {
        return None;
    }
    let here = if facts.in_the_app {
        "the app"
    } else {
        "this run"
    };
    let installed = match &schedule.network.proxy {
        None => "the schedule was installed with no proxy".to_string(),
        Some(proxy) => format!("the schedule was installed with {}", proxy_named(proxy)),
    };
    // A SOCKS proxy is not gone through, so it is said to be given, and the network check
    // says what becomes of the requests.
    let this_run = match &facts.network.proxy {
        None => format!("{here} goes directly"),
        Some(proxy) if proxy.socks => format!("{here} is given {}", proxy_named(proxy)),
        Some(proxy) => format!("{here} goes through {}", proxy_named(proxy)),
    };
    let detail = match (&schedule.network.proxy, &facts.network.proxy) {
        // Everything shown is the same, so what differs is what is never shown.
        (Some(its), Some(mine)) if proxy_named(its) == proxy_named(mine) => format!(
            "the schedule was installed with {here}'s proxy, {}, but with another user name \
             or password",
            proxy_named(its)
        ),
        _ => format!("{installed}; {this_run}"),
    };
    if facts.in_the_app {
        return Some(Check {
            code: "schedule_proxy",
            name: "renewal schedule's proxy".into(),
            level: Level::Ok,
            detail,
            advice: "The app runs with the environment the system started it with, not a \
                     terminal's. Turning daily renewal off and on in Settings gives the \
                     schedule the app's proxy variables; `pitboard schedule install` in a \
                     terminal gives it that terminal's."
                .into(),
        });
    }
    // Installed again from here, the schedule would be given a SOCKS proxy that fails its
    // requests, so the variable is to be changed first.
    let advice = match &facts.network.proxy {
        Some(proxy) if proxy.fails => format!(
            "The schedule's runs are given the proxy variables it was installed with. \
             Installed again from here, they would be given this run's SOCKS proxy, which \
             Pitboard does not use yet, and their requests would fail. Set {} to an HTTP \
             proxy's address, or unset it, then install it again with `pitboard schedule \
             install`, which takes the proxy variables set where it runs.",
            proxy.variable
        ),
        _ => "The schedule's runs are given the proxy variables it was installed with. To give \
              them this run's, install it again from here with `pitboard schedule install`, \
              which takes the proxy variables set where it runs."
            .to_string(),
    };
    Some(warn(
        "schedule_proxy",
        "renewal schedule's proxy",
        detail,
        advice,
    ))
}

/// A proxy as the schedule's line names it: its address, the variable it came from, and the
/// hosts Pitboard calls that go around it, where any do.
fn proxy_named(proxy: &ProxyFact) -> String {
    let named = format!("{}, from {}", proxy.address, proxy.variable);
    match proxy.exempting {
        Some(exempting) if !proxy.exempt.is_empty() => format!(
            "{named}, {exempting} exempting {}",
            words::listed(proxy.exempt.clone())
        ),
        _ => named,
    }
}

/// How to write the schedule again.
fn again(os: Os) -> &'static str {
    if has_app(os) {
        "Turn daily renewal off and on again: in the app's Settings, or with \
         `pitboard schedule uninstall` and then `pitboard schedule install`."
    } else {
        "Turn daily renewal off and on again with `pitboard schedule uninstall` and then \
         `pitboard schedule install`."
    }
}

/// Whether Pitboard has an app for `os`, with Settings of its own.
fn has_app(os: Os) -> bool {
    match os {
        Os::MacOs | Os::Windows => true,
        Os::Linux => false,
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

/// Codex's section: where it keeps its login, whether Pitboard can read it, its accounts,
/// and what a switch cannot reach.
///
/// Every code starts `codex_`. Three kinds of finding, judged differently:
///
/// - A fault, such as a login nobody can read. It stops Pitboard handling an account it has
///   enrolled, so it fails where there are Codex accounts, and is a warning where there are
///   none, because something is wrong with Codex even if nothing Pitboard does is broken.
/// - A choice, such as a keychain store or an API key. Nothing is wrong with it. Where Codex
///   accounts are enrolled it is still said: a keychain store puts every one of them out of
///   reach, which fails, and an API key only means there is nothing to switch from until
///   somebody signs in with an account, which is worth a look. Where none are, it is stated
///   and nothing more, so somebody who uses Pitboard for Claude Code alone is not handed a
///   warning about a setting they chose and Pitboard has no business with.
/// - A fact, such as how many sessions are running, which is only ever stated.
fn judge_codex(
    os: Os,
    sid: Option<&str>,
    facts: &CodexFacts,
    parks: &[&ParkFact],
    now: i64,
    moment: &dyn Fn(i64, i64) -> String,
) -> Vec<Check> {
    let mut checks = Vec::new();
    let enrolled = facts.enrolled > 0 || !parks.is_empty();
    let program =
        words::spelling(os, ProviderId::Codex, facts.program.as_deref()).of(ProviderId::Codex);
    let broken = |code, name: &str, detail: String, advice: String| {
        if enrolled {
            fail(code, name, detail, advice)
        } else {
            warn(code, name, detail, advice)
        }
    };
    let file = facts.backend == "file";
    let setting = &facts.backend_setting;
    let described = match facts.backend {
        "ephemeral" => format!("in memory only ({setting})"),
        "unknown" => format!("cannot tell: {setting}"),
        other => format!("{other} ({setting})"),
    };
    checks.push(match facts.backend {
        "file" => Check {
            advice: PROJECTS_NOT_READ.into(),
            ..ok(
                "codex_backend",
                "Codex login store",
                format!("file ({setting})  ·  {}", facts.auth_file.display()),
            )
        },
        // Not a choice: a layer Codex reads stops it from starting.
        "unknown" => broken(
            "codex_backend",
            "Codex login store",
            described,
            format!(
                "{}. Pitboard parks and switches no Codex login while it cannot tell where \
                 Codex keeps it.",
                facts.backend_remedy
            ),
        ),
        // A choice, and one that leaves nothing Pitboard can park or switch.
        other if enrolled => fail(
            "codex_backend",
            "Codex login store",
            described,
            match other {
                "ephemeral" => format!(
                    "Codex keeps nothing at rest, so there is no login Pitboard can park or \
                     switch. {}.",
                    facts.backend_remedy
                ),
                _ => format!(
                    "{}, so no enrolled Codex account can be switched to. {}.",
                    crate::provider::codex::layers::file_store_only(os),
                    facts.backend_remedy
                ),
            },
        ),
        _ => ok(
            "codex_backend",
            "Codex login store",
            format!("{described}; Pitboard switches Codex accounts only in the file store"),
        ),
    });

    if !facts.restrictions.is_empty() {
        let name = "Codex login restrictions";
        let said = facts.restrictions.join("; ");
        checks.push(if facts.restrictions_unreadable {
            broken(
                "codex_login_restrictions",
                name,
                said,
                "Codex does not start until that is put right, and Pitboard switches no Codex \
                 account while which logins Codex keeps cannot be told."
                    .into(),
            )
        } else if facts.ruled_out.is_empty() {
            ok("codex_login_restrictions", name, said)
        } else {
            warn(
                "codex_login_restrictions",
                name,
                said,
                format!(
                    "Codex signs out of or does not use the logins of {}, so Pitboard does not \
                     put them in use.",
                    words::listed(facts.ruled_out.clone())
                ),
            )
        });
    }

    // The file and what is in it are only worth a word where the file is the store: a
    // keychain store deletes it on purpose.
    if file {
        checks.push(match &facts.auth_access {
            None if enrolled => warn(
                "codex_auth_file",
                "Codex login file",
                format!(
                    "{} is absent: nothing is signed in to Codex",
                    facts.auth_file.display()
                ),
                format!(
                    "Put one of your enrolled accounts in use with `pitboard use \
                     codex/<label>`, or sign in with `{program} login`."
                ),
            ),
            None => ok(
                "codex_auth_file",
                "Codex login file",
                "absent; nothing is signed in to Codex",
            ),
            Some(access) if access.shared => warn(
                "codex_auth_file",
                "Codex login file",
                format!("{} is {}", facts.auth_file.display(), access.described),
                format!(
                    "{}{}",
                    match os {
                        // Codex 0.162.1's file store sets its mode under `cfg(unix)` alone.
                        Os::MacOs | Os::Linux =>
                            "It holds a usable login in plain text, and Codex sets 0600 only \
                             when it creates the file, never on a later write",
                        Os::Windows =>
                            "It holds a usable login in plain text, and Codex gives it no \
                             access list of its own, so it takes its folder's",
                    },
                    running(&os.make_private_commands(
                        Kind::File,
                        &[&facts.auth_file.display().to_string()],
                        sid,
                    ))
                ),
            ),
            Some(access) => ok(
                "codex_auth_file",
                "Codex login file",
                format!("{}  ·  {}", facts.auth_file.display(), access.described),
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
            // Signed in on purpose some way Pitboard does not switch. Nothing to sign in
            // again for, and nothing broken.
            Err(CodexLoginTrouble::NotAnAccount(why)) if enrolled => checks.push(warn(
                "codex_login",
                "Codex login",
                why.clone(),
                format!(
                    "Pitboard parks and switches Codex's ChatGPT sign-ins, so `pitboard use \
                     codex/<label>` refuses while Codex is signed in this way rather than \
                     replace a login it has nowhere to park. `{program} login` signs in with a \
                     ChatGPT account."
                ),
            )),
            Err(CodexLoginTrouble::NotAnAccount(why)) => {
                checks.push(ok("codex_login", "Codex login", why.clone()));
            }
            Err(CodexLoginTrouble::Unusable(why)) => checks.push(broken(
                "codex_login",
                "Codex login",
                format!("it cannot be used: {why}"),
                format!(
                    "Pitboard will not park or switch a Codex login it cannot read as one \
                     account's. Signing in with `{program}` again writes a fresh one."
                ),
            )),
            Err(CodexLoginTrouble::NoHome(path)) => checks.push(broken(
                "codex_login",
                "Codex login",
                crate::error::codex_home_no_folder(path),
                format!("{}.", crate::error::CODEX_HOME_REMEDY),
            )),
        }
    }

    let (cli, secrets) = facts.stored_elsewhere;
    if file && cli + secrets > 0 {
        let items = [(cli, "`cli|`"), (secrets, "`secrets|`")]
            .into_iter()
            .filter(|&(count, _)| count > 0)
            .map(|(count, prefix)| match count {
                1 => format!("1 {prefix} item"),
                many => format!("{many} {prefix} items"),
            })
            .collect();
        checks.push(Check {
            advice: "Codex keeps its login there where a setting Pitboard does not read, such as \
                     an enterprise's cloud configuration in a session of `codex`, chooses \
                     `keyring` or `auto`. A `secrets|` item also keeps the key of Codex's MCP \
                     logins. Pitboard switches only auth.json."
                .into(),
            ..ok(
                "codex_credential_manager",
                "Codex in Credential Manager",
                words::listed(items),
            )
        });
    }

    // Each Codex account, the way Claude Code's are judged, in this section and under this
    // section's codes.
    checks.extend(parks.iter().map(|p| judge_park(p, now, moment)));
    checks.extend(parks.iter().filter_map(|p| judge_dormant(p, now)));

    checks.push(judge_codex_version(os, facts));
    checks.push(judge_codex_running(facts.running.as_deref()));
    checks
}

/// What is running Codex, said and never warned about: running it is the point of having
/// it. What makes each kind take a switch is the advice, since that differs by kind.
fn judge_codex_running(running: Option<&[crate::holder::Holding]>) -> Check {
    let (detail, advice) = match running {
        None => ("could not tell".to_string(), String::new()),
        Some([]) => ("none".to_string(), String::new()),
        Some(holding) => (
            format!(
                "{}; each keeps using the account it started with until it is started again",
                crate::holder::described_with_pids(holding)
            ),
            crate::holder::remedies(holding, "to take a switch"),
        ),
    };
    Check {
        advice,
        ..ok("codex_running", "running Codex", detail)
    }
}

/// Which Codex is installed, against which one Pitboard's facts were read. Stated rather
/// than warned about, for the reason Claude Code's is.
fn judge_codex_version(os: Os, facts: &CodexFacts) -> Check {
    let verified = read_from(ProviderId::Codex, os);
    ok(
        "codex_version",
        "Codex build",
        match (facts.program.as_deref(), facts.version.as_deref()) {
            (None, _) => format!("not found here; Pitboard's facts were read from {verified}"),
            // Installed some way that does not put the version in its path, such as behind
            // a version manager's shim. Found is not the same as missing.
            (Some(program), None) => format!(
                "{}, whose path does not say which version it is; Pitboard's facts were \
                 read from {verified}",
                program.display()
            ),
            (Some(_), Some(installed)) if installed == verified => {
                format!("{installed}, which is what Pitboard's facts were read from")
            }
            (Some(_), Some(installed)) => {
                format!("{installed} installed; Pitboard's facts were read from {verified}")
            }
        },
    )
}

// An index from a Mac can hold Claude Desktop's accounts, which nothing on Windows or Linux
// changes. Only a build for tests gives Linux a Claude data folder.
fn judge_desktop_off_macos(state: &State) -> Check {
    let held: Vec<String> = state
        .accounts
        .iter()
        .filter(|account| account.provider() == ProviderId::Desktop)
        .map(|account| format!("`{}`", state.typed(&account.key())))
        .collect();
    let holds = match held.is_empty() {
        true => "folders set aside for Claude Desktop".to_string(),
        false => words::listed(held),
    };
    Check {
        advice: format!("This index holds {holds}. Nothing here changes them."),
        ..ok(
            "desktop_data_folder",
            "Claude data folder",
            crate::provider::desktop::paths::MACOS_ONLY,
        )
    }
}

// Claude Desktop's section, after Codex's.
fn judge_desktop(
    os: Os,
    facts: &DesktopFacts,
    state: &State,
    parks: &[ParkFact],
    now: i64,
    moment: &dyn Fn(i64, i64) -> String,
) -> Vec<Check> {
    use crate::provider::folder::{Sidecar, Unfit, Unread};
    let mut checks = Vec::new();
    let folder = "Claude data folder";
    match &facts.root {
        Err(reason) => {
            let advice = facts.own_home.as_ref().map_or_else(String::new, |own| {
                format!(
                    "Run Pitboard with `HOME` unset, or set to {}.",
                    own.display()
                )
            });
            checks.push(fail("desktop_data_folder", folder, reason, advice));
        }
        Ok(root) => {
            let shown = root.display();
            checks.push(match facts.refused {
                Some(unfit @ Unfit::Link) => fail(
                    "desktop_data_folder",
                    folder,
                    format!("{shown} {unfit}"),
                    "Pitboard moves no Claude Desktop login through a link.",
                ),
                Some(unfit @ Unfit::ThirdParty(_)) => fail(
                    "desktop_data_folder",
                    folder,
                    format!("{shown} {unfit}"),
                    "Pitboard moves no Claude Desktop login for a third-party deployment.",
                ),
                Some(unfit @ Unfit::ResetWaits) => fail(
                    "desktop_data_folder",
                    folder,
                    format!("{shown} {unfit}"),
                    "Open Claude to let the reset finish, then quit it with Command-Q.",
                ),
                None if facts.present => ok("desktop_data_folder", folder, shown.to_string()),
                None => warn(
                    "desktop_data_folder",
                    folder,
                    format!("{shown} does not exist"),
                    "Run `pitboard enroll desktop/<label> --sign-in`, then open Claude and sign \
                     in: Pitboard waits for it to quit.",
                ),
            });
        }
    }
    if let (Ok(root), Some(settings)) = (&facts.root, &facts.settings) {
        let name = "Claude settings";
        checks.push(match settings {
            Ok((Sidecar::Valid, _)) => ok(
                "desktop_settings",
                name,
                "config.json read, with a settings write Claude left, which Pitboard reads as \
                 Claude will",
            ),
            Ok((Sidecar::Stale, _)) => ok(
                "desktop_settings",
                name,
                "config.json read; a stale settings write beside it, which Claude drops when it \
                 opens",
            ),
            Ok((Sidecar::None, true)) => ok("desktop_settings", name, "config.json read"),
            Ok((Sidecar::None, false)) => ok("desktop_settings", name, "no config.json yet"),
            Err(Unread::Unknown { what, then }) => fail(
                "desktop_settings",
                name,
                format!("{} {what}", root.display()),
                then.said().trim_end(),
            ),
            Err(Unread::Unreadable(detail)) => fail("desktop_settings", name, detail.clone(), ""),
        });
        if facts.present {
            let name = "Claude data folder layout";
            checks.push(match facts.unread.as_slice() {
                [] => ok(
                    "desktop_layout",
                    name,
                    "each part of the account's data is one Pitboard moves",
                ),
                [part] => warn(
                    "desktop_layout",
                    name,
                    format!(
                        "holds `{part}`, a part of the account's data this Pitboard has not read"
                    ),
                    "Pitboard starts no move of a Claude Desktop login while it is there. Update \
                     Pitboard, or report it at https://github.com/datlechin/pitboard/issues if no \
                     release reads it yet.",
                ),
                parts => warn(
                    "desktop_layout",
                    name,
                    format!(
                        "holds {}, parts of the account's data this Pitboard has not read",
                        words::listed(parts.iter().map(|part| format!("`{part}`")).collect())
                    ),
                    "Pitboard starts no move of a Claude Desktop login while they are there. \
                     Update Pitboard, or report them at \
                     https://github.com/datlechin/pitboard/issues if no release reads them yet.",
                ),
            });
        }
    }
    checks.extend(judge_desktop_login(facts, state, now, moment));
    let in_use = state.account_in_use(ProviderId::Desktop).map(|a| a.key());
    for fact in parks.iter().filter(|p| p.provider == ProviderId::Desktop) {
        checks.push(judge_desktop_park(facts, fact, now));
    }
    let typed = |label: &str| state.typed(&Key::new(ProviderId::Desktop, label));
    if let Some(moving) = state.moving(ProviderId::Desktop) {
        checks.push(warn(
            "desktop_move",
            "interrupted Claude Desktop move",
            format!(
                "a move of `{}`'s login was interrupted {}",
                typed(&moving.label),
                moment(moving.started_at, now)
            ),
            "Keep Claude closed: the next change to a Claude Desktop account settles it first. \
             If it cannot, `pitboard abandon` keeps every folder and moves on.",
        ));
    }
    if let Some(awaited) = state.awaiting(ProviderId::Desktop)
        && let Some(put_back) = state.by_id(ProviderId::Desktop, &awaited.put_back)
    {
        let put_back = state.typed(&put_back.key());
        checks.push(warn(
            "desktop_awaiting",
            "awaited sign-in",
            format!(
                "Claude is signed out for a sign-in since {}, and `{put_back}` is parked",
                moment(awaited.since, now)
            ),
            format!(
                "Sign in inside Claude, quit it with Command-Q, then run `pitboard enroll {}`. \
                 To go back, run `pitboard use {put_back}`.",
                typed(&awaited.label)
            ),
        ));
    }
    let parks_dir = facts.parks_dir.display();
    match facts.private {
        Some(crate::host::Private::Missing) | None => {}
        Some(found) => {
            let name = "parked Claude Desktop logins";
            checks.push(match found {
                crate::host::Private::Yes if facts.one_volume == Some(false) => fail(
                    "desktop_parks_folder",
                    name,
                    format!(
                        "{parks_dir} is on another volume than {}",
                        facts.root.as_ref().map_or_else(
                            |_| "Claude's data".into(),
                            |root| root.display().to_string()
                        )
                    ),
                    "Keep Pitboard's folder on the volume your home is on.",
                ),
                crate::host::Private::Yes => ok(
                    "desktop_parks_folder",
                    name,
                    format!("private  ·  {parks_dir}"),
                ),
                crate::host::Private::Shared => fail(
                    "desktop_parks_folder",
                    name,
                    format!("{parks_dir} can be reached by others"),
                    // Claude Desktop is macOS only, so no SID is ever named.
                    commands(&os.make_private_commands(
                        Kind::Directory,
                        &[&parks_dir.to_string()],
                        None,
                    ))
                    .map_or_else(String::new, |commands| format!("Run {commands}.")),
                ),
                crate::host::Private::Link => fail(
                    "desktop_parks_folder",
                    name,
                    format!("{parks_dir} is a link"),
                    "Pitboard parks no Claude Desktop login through a link.",
                ),
                crate::host::Private::NotOwn => fail(
                    "desktop_parks_folder",
                    name,
                    format!("{parks_dir} is not yours"),
                    "Pitboard parks no Claude Desktop login in a folder another user owns.",
                ),
                crate::host::Private::NotADirectory | crate::host::Private::Missing => fail(
                    "desktop_parks_folder",
                    name,
                    format!("{parks_dir} is not a folder"),
                    "Pitboard parks no Claude Desktop login there.",
                ),
            });
        }
    }
    let asides: Vec<&crate::state::Aside> = state
        .aside
        .iter()
        .filter(|aside| aside.tool == ProviderId::Desktop)
        .collect();
    if !asides.is_empty() {
        let (holding, parts): (Vec<&crate::state::Aside>, Vec<&crate::state::Aside>) =
            asides.iter().partition(|aside| aside.login.is_some());
        let name = "folders set aside";
        let n = asides.len();
        let account = |aside: &crate::state::Aside| {
            aside
                .account
                .as_deref()
                .and_then(|id| state.owner_of_park(id))
        };
        let kept_beside = |aside: &crate::state::Aside| {
            account(aside).is_some_and(|a| a.parked.is_some() || in_use.as_ref() == Some(&a.key()))
        };
        let said = |aside: &crate::state::Aside| {
            let when = moment(aside.at, now);
            let typed = account(aside).map(|a| state.typed(&a.key()));
            match (aside.why, typed) {
                (crate::state::SetAside::AnotherMachine, Some(typed)) => {
                    format!("`{typed}`'s login from another Mac")
                }
                (crate::state::SetAside::Superseded, Some(typed)) if kept_beside(aside) => {
                    format!("`{typed}`'s login from before a sign-in again, set aside {when}")
                }
                (
                    crate::state::SetAside::Superseded | crate::state::SetAside::GivenUp,
                    Some(typed),
                ) => format!("`{typed}`'s earlier login, set aside {when}"),
                _ => format!("a login no account here holds, set aside {when}"),
            }
        };
        // No session: an account's earlier keys, the parts of its login a give-up left, or
        // what Claude made while signed out.
        let mut rest: Vec<String> = parts
            .iter()
            .filter_map(|aside| {
                let when = moment(aside.at, now);
                match (aside.why, account(aside).map(|a| state.typed(&a.key()))) {
                    (_, Some(typed)) => {
                        Some(format!("parts of `{typed}`'s login, set aside {when}"))
                    }
                    (crate::state::SetAside::InTheWay, None) => None,
                    (_, None) => Some(format!(
                        "parts of a login no account here holds, set aside {when}"
                    )),
                }
            })
            .collect();
        let only_signed_out = rest.is_empty();
        if rest.len() < parts.len() {
            rest.push("what Claude made while signed out".into());
        }
        let mut detail = match holding.as_slice() {
            [] => format!("{n} in {parks_dir}, none holding a login"),
            held => format!(
                "{n} in {parks_dir}, {} holding a login: {}",
                held.len(),
                words::listed(held.iter().map(|aside| said(aside)).collect())
            ),
        };
        if !rest.is_empty() {
            let (whose, only) = match holding.is_empty() {
                true if only_signed_out => ("they hold", "only "),
                true => ("they hold", ""),
                false => ("the rest hold", ""),
            };
            detail.push_str(&format!("; {whose} {only}{}", words::listed(rest)));
        }
        checks.push(if holding.iter().all(|aside| kept_beside(aside)) {
            ok("desktop_set_aside", name, detail)
        } else {
            warn(
                "desktop_set_aside",
                name,
                detail,
                "`pitboard repair` gives one back to its account where it can. `pitboard \
                 forget` of an account deletes those set aside for it, and `pitboard \
                 uninstall` deletes them all.",
            )
        });
    }
    checks.push(judge_desktop_build(facts));
    checks
}

fn judge_desktop_login(
    facts: &DesktopFacts,
    state: &State,
    now: i64,
    moment: &dyn Fn(i64, i64) -> String,
) -> Option<Check> {
    let known = facts.known.as_ref()?;
    let name = "Claude Desktop in use";
    let tool = ProviderId::Desktop;
    let enroll = "Open Claude, check which account it shows, quit it with Command-Q, then run \
                  `pitboard enroll desktop/<label>` under that account's name.";
    let Some(last) = &known.last else {
        return Some(warn(
            "desktop_login",
            name,
            "Pitboard has not read who is signed in to Claude",
            "The next change to a Claude Desktop account reads it, with Claude closed.",
        ));
    };
    let when = moment(last.known_at, now);
    let who = Stored::of(state, tool, last.owner.as_ref());
    Some(match known.doubt {
        Some(crate::in_use::Doubt::LoginUnreadable) => warn(
            "desktop_login",
            name,
            format!(
                "Claude's settings could not be read, so this is as Pitboard last read it {when}"
            ),
            "`Claude settings` says why.",
        ),
        Some(crate::in_use::Doubt::NamedMoved) => warn(
            "desktop_login",
            name,
            words::desktop_named(&Stored::of(state, tool, known.own_record.as_ref()), &who),
            enroll,
        ),
        _ if facts.signed_out => warn(
            "desktop_login",
            name,
            format!(
                "{}, as Pitboard last read it {when}; both token caches are emptied since, so \
                 Claude looks signed out",
                who.said("nobody")
            ),
            "The next change to a Claude Desktop account reads who is signed in, with Claude \
             closed.",
        ),
        _ => match state.account_in_use(tool) {
            Some(account) if account.parked.is_some() => {
                let typed = state.typed(&account.key());
                warn(
                    "desktop_login",
                    name,
                    format!("`{typed}` is recorded in use and also holds a parked login"),
                    format!(
                        "Open Claude, check which account it shows, and quit it with \
                         Command-Q. If it shows `{typed}`, run `pitboard enroll {typed}`: its \
                         older parked login is set aside, and nothing is deleted. If it shows \
                         another account, run `pitboard enroll desktop/<label>` under that \
                         account's name."
                    ),
                )
            }
            _ => ok(
                "desktop_login",
                name,
                format!("{}, as Pitboard last read it {when}", who.said("nobody")),
            ),
        },
    })
}

fn judge_desktop_park(facts: &DesktopFacts, fact: &ParkFact, now: i64) -> Check {
    let name = format!("account {}", fact.typed());
    let renew = format!("Run `pitboard enroll {} --sign-in`.", fact.typed());
    let code = "desktop_parked_login";
    let Some(park) = &fact.park else {
        return if fact.active {
            ok(code, name, "signed in; parked when you switch away")
        } else {
            warn(code, name, "nothing parked to switch to", renew)
        };
    };
    let here = facts
        .parked_here
        .iter()
        .find(|(service, _)| *service == park.service)
        .map(|(_, folder)| folder);
    match here {
        Some(Ok(true)) => {}
        // What Pitboard could not look at is still there: no sign-in, as `use` asks none.
        Some(Err(e)) => {
            return warn(
                code,
                name,
                format!(
                    "not read: Pitboard could not look at it in {} ({e})",
                    facts.parks_dir.display()
                ),
                format!(
                    "Make {}, and each folder it is in, readable to you, then run `pitboard \
                     doctor` again.",
                    facts.parks_dir.display()
                ),
            );
        }
        Some(Ok(false)) | None => {
            return fail(
                code,
                name,
                format!(
                    "its parked login is missing from {}",
                    facts.parks_dir.display()
                ),
                format!(
                    "Run `pitboard repair`. If it gives nothing back, run `pitboard enroll {} \
                     --sign-in`.",
                    fact.typed()
                ),
            );
        }
    }
    let Some(at) = park.session_expires_at else {
        return ok(code, name, "parked");
    };
    // The column `pitboard status` shows, after this check's own words.
    let life = words::session_life_column(at, now);
    if at > now {
        ok(code, name, format!("parked, {life}"))
    } else {
        warn(
            code,
            name,
            format!("parked, {life}"),
            format!(
                "If Claude asks you to sign in once you use it, run `pitboard enroll {} \
                 --sign-in`.",
                fact.typed()
            ),
        )
    }
}

// Said, never a gate: Pitboard checks the folder before each move, not the build.
fn judge_desktop_build(facts: &DesktopFacts) -> Check {
    use crate::provider::desktop::assumptions::{
        READ_FROM_CLAUDE_3, READ_FROM_THE_ARCHIVE, READ_FROM_THE_NEWER_BUNDLE, VERIFIED_AGAINST,
    };
    match &facts.build {
        Some(build) => ok(
            "desktop_build",
            "Claude",
            format!(
                "Claude {build}; Pitboard's facts were read from {VERIFIED_AGAINST}, \
                 {READ_FROM_THE_ARCHIVE}, {READ_FROM_THE_NEWER_BUNDLE} and {READ_FROM_CLAUDE_3}, \
                 and it checks the folder before each move, not the build"
            ),
        ),
        None => ok("desktop_build", "Claude", "not found in /Applications"),
    }
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
        redaction: redaction_for(ctx, &facts),
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
            "desktop": desktop_environment(ctx, &facts),
        }),
    }
}

// On every machine, so its shape is one: whether the folder is a directory, never what it holds.
fn desktop_environment(ctx: &Context, facts: &Facts) -> Value {
    let (root, build) = match &facts.desktop {
        Some(desktop) => (desktop.root.clone().ok(), desktop.build.clone()),
        None => (
            claude_desktop().and_then(|app| app.root(ctx).ok()),
            desktop_build(ctx),
        ),
    };
    json!({
        "data_folder": root,
        "present": root.as_ref().is_some_and(|root| root.is_dir()),
        "build": build,
    })
}

// Claude Desktop is macOS's alone.
fn desktop_build(ctx: &Context) -> Option<String> {
    match ctx.os {
        Os::MacOs => ctx
            .host()
            .app_version(std::path::Path::new("/Applications/Claude.app")),
        Os::Linux | Os::Windows => None,
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
        checks: too_old(ctx.host().floor())
            .into_iter()
            .chain(elevated(
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

/// Everything in a diagnosis that names a person or an account.
///
/// The salt is this machine and this moment, so identifiers line up inside one report and
/// two reports from one machine do not line up with each other. It is never printed.
fn redaction_for(ctx: &Context, facts: &Facts) -> crate::redact::Sheet {
    let mut sheet = crate::redact::Sheet::of(
        ctx,
        format!("{}:{}", crate::state::machine_id(), ctx.now_millis()),
    );
    if facts.account != slot::FALLBACK_ACCOUNT {
        sheet = sheet.hide(facts.account.clone(), "user");
    }

    if let Some(id) = &facts.config_account {
        sheet = sheet
            .hide(id.email.clone(), "email")
            .hide(id.account_uuid.clone(), "account")
            .hide(id.organization_uuid.clone(), "org");
        if let Some(name) = &id.organization_name {
            sheet = sheet.hide(name.clone(), "org");
        }
    }
    // Whose login is stored, as Anthropic last named it, and whom Claude Code's config names,
    // which need not be accounts anybody enrolled.
    let named = facts
        .in_use
        .iter()
        .flat_map(|known| known.owner().into_iter().chain(&known.own_record));
    for owner in named {
        sheet = sheet
            .hide(owner.email.clone(), "email")
            .hide(owner.account_uuid.clone(), "account")
            .hide(owner.organization_uuid.clone(), "org");
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
    if let Some(fingerprint) = facts
        .fallback_login
        .as_ref()
        .and_then(|left| left.fingerprint.as_ref().ok().cloned().flatten())
        .filter(|fingerprint| !fingerprint.is_empty())
    {
        sheet = sheet.hide(fingerprint, "login");
    }
    for park in &facts.parks {
        if let Some(held) = &park.park {
            sheet = sheet
                .hide(held.login.clone(), "login")
                .hide(held.service.clone(), "park");
        }
    }
    // Claude Desktop's session as last read, and each folder set aside with its session.
    if let Ok(state) = &facts.state {
        if let Some(record) = state.in_use(ProviderId::Desktop) {
            sheet = sheet.hide(record.login.clone(), "login");
        }
        for aside in &state.aside {
            sheet = sheet
                .hide(aside.name.clone(), "park")
                .hide(aside.login.clone().unwrap_or_default(), "login");
        }
    }
    // Windows' proxy names a network as a variable's does: `host:port`, or one per protocol.
    let system_proxy = facts
        .windows_settings
        .as_ref()
        .and_then(|settings| settings.proxy.as_deref());
    for entry in system_proxy.map(system_proxies).unwrap_or_default() {
        sheet = sheet.hide(entry.server.to_owned(), "proxy");
    }
    // A proxy's host can name the company whose network it is, as an organisation's name
    // does. Its port goes with it, so that a host that is also a word, such as `proxy`, is
    // hidden where it is the host and nowhere else. One on loopback names nothing. The one
    // the schedule was installed with is hidden the same way.
    let schedules = facts
        .schedule
        .as_ref()
        .and_then(|s| s.network.proxy.as_ref());
    for proxy in facts.network.proxy.iter().chain(schedules) {
        if !proxy.on_loopback {
            sheet = sheet.hide(proxy.server.clone(), "proxy");
        }
    }
    sheet
}

/// No check failed. Warnings are advice; a failure means an assumption broke.
pub fn healthy(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.level != Level::Fail)
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::schedule::Location;
    use crate::service::Permit;
    use crate::time;
    use std::sync::Arc;

    /// What a mode looks like as access, the way a Unix system describes one.
    fn mode(mode: u32) -> Access {
        Access {
            shared: mode & 0o077 != 0,
            described: format!("mode {mode:o}"),
        }
    }

    /// A network whose requests go out directly, as no variable names a proxy.
    fn direct() -> NetworkFact {
        NetworkFact {
            proxy: None,
            passed_over: Vec::new(),
            set: Vec::new(),
        }
    }

    fn desktop() -> DesktopFacts {
        DesktopFacts {
            one_volume: Some(true),
            root: Ok(PathBuf::from(
                "/Users/dana/Library/Application Support/Claude",
            )),
            own_home: None,
            refused: None,
            present: true,
            settings: Some(Ok((crate::provider::folder::Sidecar::None, true))),
            unread: Vec::new(),
            known: None,
            signed_out: false,
            parks_dir: PathBuf::from("/Users/dana/.pitboard/parks"),
            private: Some(crate::host::Private::Yes),
            parked_here: Vec::new(),
            build: None,
        }
    }

    fn desktop_account(label: &str, parked: Option<Park>) -> crate::state::Account {
        crate::state::Account {
            label: label.into(),
            id: format!("desktop_{label}"),
            account_uuid: label.into(),
            email: String::new(),
            parked,
            last_used_at: None,
            replaced_at: None,
            plan: None,
            detail: crate::state::Detail::Desktop {},
        }
    }

    fn folder_park(service: &str, session_expires_at: Option<i64>) -> Park {
        Park {
            service: service.into(),
            kind: crate::state::ParkKind::Folder,
            parked_at: NOW - 86_400,
            login: "f".into(),
            access_expires_at: None,
            refresh_expires_at: None,
            session_expires_at,
        }
    }

    #[test]
    fn section_names_the_tool_whose_checks_these_are() {
        for code in [
            "claude_version",
            "claude_daemon",
            "credential",
            "state",
            "schedule_proxy",
            "system_too_old",
            "desktop",
            "codexy",
        ] {
            assert_eq!(section(code), None, "{code}");
        }
        for code in ["codex_backend", "codex_login", "codex_parked_login"] {
            assert_eq!(section(code), Some(ProviderId::Codex), "{code}");
        }
        let mut state = State::default();
        state
            .accounts
            .push(desktop_account("work", Some(folder_park("p", None))));
        let checks = judge_desktop(Os::MacOs, &desktop(), &state, &[], NOW, &time::moment);
        assert!(!checks.is_empty());
        for check in checks {
            assert_eq!(
                section(check.code),
                Some(ProviderId::Desktop),
                "{}",
                check.code
            );
        }
    }

    fn said(checks: &[Check], code: &str) -> Vec<(Level, String)> {
        checks
            .iter()
            .filter(|c| c.code == code)
            .map(|c| (c.level, c.detail.clone()))
            .collect()
    }

    #[test]
    fn an_interrupted_move_and_a_parks_folder_on_another_volume_are_said() {
        let mut state = State::default();
        state.accounts.push(desktop_account("work", None));
        state.moving.insert(
            "desktop".into(),
            crate::state::Moving {
                half: crate::state::Half::Leave,
                started_at: NOW - 300,
                id: "desktop_work".into(),
                label: "work".into(),
                park: "pitboard-park-desktop_work-1".into(),
                login: "f0".into(),
                items: vec!["Cookies".into()],
                keys: std::collections::BTreeMap::new(),
                asides: std::collections::BTreeMap::new(),
                slot: String::new(),
                awaited: None,
            },
        );
        let mut facts = desktop();
        facts.one_volume = Some(false);
        let checks = judge_desktop(Os::MacOs, &facts, &state, &[], NOW, &time::moment);
        let moved = said(&checks, "desktop_move");
        assert_eq!(moved.len(), 1, "{moved:?}");
        assert_eq!(moved[0].0, Level::Warn);
        assert!(
            moved[0]
                .1
                .starts_with("a move of `desktop/work`'s login was interrupted"),
            "{moved:?}"
        );
        assert_eq!(
            said(&checks, "desktop_parks_folder"),
            [(
                Level::Fail,
                "/Users/dana/.pitboard/parks is on another volume than /Users/dana/Library/\
                 Application Support/Claude"
                    .into()
            )]
        );

        state.moving.clear();
        facts.one_volume = Some(true);
        let checks = judge_desktop(Os::MacOs, &facts, &state, &[], NOW, &time::moment);
        assert!(said(&checks, "desktop_move").is_empty());
        assert_eq!(said(&checks, "desktop_parks_folder")[0].0, Level::Ok);
    }

    #[test]
    fn claude_desktops_checks_say_what_the_index_and_the_settings_say() {
        use crate::provider::folder::{Sidecar, Unfit, Unread};
        let mut state = State::default();
        state.accounts.push(desktop_account("work", None));
        state.accounts.push(desktop_account(
            "home",
            Some(folder_park("home-1", Some(NOW + 90_000))),
        ));
        state.accounts.push(desktop_account(
            "old",
            Some(folder_park("old-1", Some(NOW - 5))),
        ));
        state
            .accounts
            .push(desktop_account("lost", Some(folder_park("lost-1", None))));
        state.accounts.push(desktop_account("none", None));
        let work = crate::in_use::InUse {
            owner: Some(state.accounts[0].owner()),
            login: "f0".into(),
            known_at: NOW - 60,
            named: Some("desktop_work".into()),
        };
        state.in_use.insert("desktop".into(), work.clone());
        let parks: Vec<ParkFact> = state
            .accounts
            .iter()
            .map(|a| ParkFact {
                provider: ProviderId::Desktop,
                name: format!("desktop/{}", a.label),
                active: a.label == "work",
                last_used_at: None,
                park: a.parked.clone(),
                unreadable: None,
            })
            .collect();
        let mut facts = desktop();
        facts.known = Some(crate::in_use::Known {
            last: Some(work),
            doubt: None,
            own_record: Some(state.accounts[0].owner()),
        });
        facts.parked_here = vec![("home-1".into(), Ok(true)), ("old-1".into(), Ok(true))];
        facts.build = Some("2.31226.1".into());
        let checks = judge_desktop(Os::MacOs, &facts, &state, &parks, NOW, &time::moment);
        let codes: Vec<&str> = checks.iter().map(|c| c.code).collect();
        assert_eq!(
            codes,
            [
                "desktop_data_folder",
                "desktop_settings",
                "desktop_layout",
                "desktop_login",
                "desktop_parked_login",
                "desktop_parked_login",
                "desktop_parked_login",
                "desktop_parked_login",
                "desktop_parked_login",
                "desktop_parks_folder",
                "desktop_build",
            ]
        );
        assert_eq!(
            said(&checks, "desktop_parked_login"),
            [
                (Level::Ok, "signed in; parked when you switch away".into()),
                (Level::Ok, "parked, sign-in may lapse in 1d 1h".into()),
                (Level::Warn, "parked, sign-in may have lapsed".into()),
                (
                    Level::Fail,
                    "its parked login is missing from /Users/dana/.pitboard/parks".into()
                ),
                (Level::Warn, "nothing parked to switch to".into()),
            ]
        );
        assert_eq!(
            said(&checks, "desktop_settings"),
            [(Level::Ok, "config.json read".into())]
        );
        let login = said(&checks, "desktop_login");
        assert!(
            login[0]
                .1
                .starts_with("`desktop/work`, as Pitboard last read it"),
            "{login:?}"
        );
        assert_eq!(
            said(&checks, "desktop_build")[0].1,
            "Claude 2.31226.1; Pitboard's facts were read from 2.19675.0, 2.7032.0, \
             2.42304.0 and 3.6556.2, and it checks the folder before each move, not the build"
        );
        assert!(checks.iter().all(|c| !c.code.contains("dormant")));

        facts.unread = vec!["CdmStorage.db-wal".into(), "Cookies-wal".into()];
        facts.settings = Some(Err(Unread::Unknown {
            what: "has a settings file, `config.json`, that is not valid JSON".into(),
            then: crate::error::Then::CopyAside,
        }));
        facts.private = Some(crate::host::Private::Shared);
        facts.signed_out = true;
        facts.build = None;
        let checks = judge_desktop(Os::MacOs, &facts, &state, &parks, NOW, &time::moment);
        assert_eq!(
            said(&checks, "desktop_layout"),
            [(
                Level::Warn,
                "holds `CdmStorage.db-wal` and `Cookies-wal`, parts of the account's data this \
                 Pitboard has not read"
                    .into()
            )]
        );
        let settings = checks
            .iter()
            .find(|c| c.code == "desktop_settings")
            .unwrap();
        assert_eq!(settings.level, Level::Fail);
        assert!(
            settings.advice.starts_with("Claude resets a settings file"),
            "{}",
            settings.advice
        );
        assert!(
            settings.advice.ends_with("then run this again."),
            "{}",
            settings.advice
        );
        assert_eq!(said(&checks, "desktop_parks_folder")[0].0, Level::Fail);
        assert_eq!(
            check(&checks, "desktop_parks_folder").advice,
            "Run `chmod 700 /Users/dana/.pitboard/parks`."
        );
        assert!(
            said(&checks, "desktop_login")[0]
                .1
                .contains("looks signed out")
        );
        assert_eq!(
            said(&checks, "desktop_build")[0].1,
            "not found in /Applications"
        );

        for (unfit, detail, advice) in [
            (
                Unfit::Link,
                "/Users/dana/Library/Application Support/Claude is a link",
                "Pitboard moves no Claude Desktop login through a link.",
            ),
            (
                Unfit::ThirdParty("Claude-3p"),
                "/Users/dana/Library/Application Support/Claude is not the one Claude runs \
                 from: it runs as a third-party deployment, from `Claude-3p` beside it",
                "Pitboard moves no Claude Desktop login for a third-party deployment.",
            ),
            (
                Unfit::ResetWaits,
                "/Users/dana/Library/Application Support/Claude is waiting for Reset App Data, \
                 which deletes what it holds the next time Claude opens",
                "Open Claude to let the reset finish, then quit it with Command-Q.",
            ),
        ] {
            facts.refused = Some(unfit);
            let checks = judge_desktop(Os::MacOs, &facts, &state, &parks, NOW, &time::moment);
            let folder = checks
                .iter()
                .find(|c| c.code == "desktop_data_folder")
                .unwrap();
            assert_eq!(
                (folder.level, folder.detail.as_str(), folder.advice.as_str()),
                (Level::Fail, detail, advice)
            );
        }
        facts.refused = None;

        facts.settings = Some(Ok((Sidecar::Valid, true)));
        facts.root = Err("`HOME` names another folder than this account's own".into());
        facts.own_home = Some(PathBuf::from("/Users/dana"));
        let checks = judge_desktop(Os::MacOs, &facts, &state, &parks, NOW, &time::moment);
        let folder = checks
            .iter()
            .find(|c| c.code == "desktop_data_folder")
            .unwrap();
        assert_eq!(
            (folder.level, folder.detail.as_str(), folder.advice.as_str()),
            (
                Level::Fail,
                "`HOME` names another folder than this account's own",
                "Run Pitboard with `HOME` unset, or set to /Users/dana."
            )
        );
        assert!(said(&checks, "desktop_settings").is_empty());
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn claude_desktops_facts_come_from_one_read_of_its_settings() {
        use crate::provider::folder::Sidecar;
        let m = switch::harness::desktop_machine("doctor-facts");
        let state = crate::state::load(&m.ctx).expect("state");
        let (facts, reads) =
            crate::fault::counting("settings.read", || desktop_facts(&m.ctx, &state));
        let facts = facts.expect("an account is enrolled");
        assert_eq!(reads, 1);
        assert_eq!(facts.settings, Some(Ok((Sidecar::None, true))));
        let known = facts.known.expect("judged");
        assert_eq!(known.doubt, None);
        assert_eq!(
            known.own_record.map(|owner| owner.account_uuid).as_deref(),
            Some(switch::harness::WORK.uuid)
        );
        assert!(!facts.signed_out);

        std::fs::remove_file(m.ctx_home().join("claude/data/config.json")).expect("removed");
        let facts = desktop_facts(&m.ctx, &state).expect("an account is enrolled");
        assert_eq!(facts.settings, Some(Ok((Sidecar::None, false))));
        let checks = judge_desktop(
            Os::MacOs,
            &facts,
            &state,
            &[],
            switch::harness::NOW,
            &time::moment,
        );
        assert_eq!(
            said(&checks, "desktop_settings"),
            [(Level::Ok, "no config.json yet".into())]
        );
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn doctor_says_the_build_of_the_claude_installed() {
        let mut m = switch::harness::desktop_machine("doctor-build");
        m.ctx.os = Os::MacOs;
        m.mem.app_at(
            std::path::Path::new("/Applications/Claude.app"),
            "2.31226.1",
        );
        let diagnosis = run(&m.ctx, &time::moment);
        let build = diagnosis
            .checks
            .iter()
            .find(|c| c.code == "desktop_build")
            .expect("the build is said");
        assert!(
            build.detail.starts_with("Claude 2.31226.1; "),
            "{}",
            build.detail
        );
        assert_eq!(diagnosis.environment["desktop"]["build"], "2.31226.1");
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn a_park_doctor_cannot_look_at_is_never_said_missing() {
        let m = switch::harness::desktop_pair("doctor-park-unread");
        let state = crate::state::load(&m.ctx).expect("state");
        let parks_dir = store::folder::parks_dir(&m.ctx);
        crate::host::fs::testing::deny_reading(&parks_dir).expect("parks shut");
        let facts = desktop_facts(&m.ctx, &state);
        crate::host::fs::testing::make_private(&parks_dir).expect("parks open again");
        let facts = facts.expect("an account is enrolled");
        let parks = park_facts(&m.ctx, &state);
        let checks = judge_desktop(
            Os::MacOs,
            &facts,
            &state,
            &parks,
            switch::harness::NOW,
            &time::moment,
        );
        let home = checks
            .iter()
            .find(|c| c.code == "desktop_parked_login" && c.name == "account desktop/home")
            .map(|c| (c.level, c.detail.clone(), c.advice.clone()))
            .expect("home's park is checked");
        assert_eq!(home.0, Level::Warn, "{home:?}");
        assert!(
            home.1.starts_with(&format!(
                "not read: Pitboard could not look at it in {} (",
                parks_dir.display()
            )),
            "{home:?}"
        );
        assert!(!home.2.contains("--sign-in"), "{home:?}");
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn a_data_folder_is_found_where_only_a_folder_set_aside_is_left() {
        use crate::state::{Aside, SetAside};
        let m = switch::harness::desktop_pair("doctor-aside-only");
        let mut state = crate::state::load(&m.ctx).expect("state");
        state
            .accounts
            .retain(|account| account.provider() != ProviderId::Desktop);
        state.aside.push(Aside {
            name: "pitboard-aside-1".into(),
            tool: ProviderId::Desktop,
            at: NOW,
            why: SetAside::Orphan,
            account: None,
            login: None,
        });
        let facts = desktop_facts(&m.ctx, &state).expect("a folder set aside shows Claude Desktop");
        let parks = park_facts(&m.ctx, &state);
        let checks = judge_desktop(
            Os::MacOs,
            &facts,
            &state,
            &parks,
            switch::harness::NOW,
            &time::moment,
        );
        let folder = checks
            .iter()
            .find(|c| c.code == "desktop_data_folder")
            .map(|c| (c.level, c.detail.clone()))
            .expect("the data folder is checked");
        assert_eq!(folder.0, Level::Ok, "{folder:?}");
    }

    #[test]
    fn a_claude_desktop_login_is_redacted_from_a_report() {
        use crate::state::{Aside, SetAside};
        let uuid = "6f1c0a52-1111-4a6e-9d0b-2f6c1a9e7b10";
        let kept = format!("pitboard-park-desktop_{uuid}-1");
        let mut state = State::default();
        state.in_use.insert(
            "desktop".into(),
            crate::in_use::InUse {
                owner: None,
                login: "0123456789abcdef".into(),
                known_at: NOW,
                named: None,
            },
        );
        state.aside.push(Aside {
            name: kept.clone(),
            tool: ProviderId::Desktop,
            at: NOW,
            why: SetAside::Orphan,
            account: None,
            login: Some("fedcba9876543210".into()),
        });
        let mut f = facts();
        f.state = Ok(state);
        let ctx = Context::new(PathBuf::from("/home/x"));
        let text = format!("read 0123456789abcdef in use; {kept} holds fedcba9876543210");
        let hidden = redaction_for(&ctx, &f).over(&text);
        for secret in ["0123456789abcdef", "fedcba9876543210", kept.as_str(), uuid] {
            assert!(text.contains(secret), "{text}");
            assert!(!hidden.contains(secret), "{hidden}");
        }
    }

    #[test]
    fn a_folder_set_aside_warns_only_where_its_account_holds_nothing_else() {
        use crate::state::{Aside, SetAside};
        let aside = |name: &str, why, account: Option<&str>, login: Option<&str>| Aside {
            name: name.into(),
            tool: ProviderId::Desktop,
            at: NOW - 3_600,
            why,
            account: account.map(str::to_owned),
            login: login.map(str::to_owned),
        };
        let mut state = State::default();
        state.accounts.push(desktop_account("work", None));
        state
            .accounts
            .push(desktop_account("home", Some(folder_park("home-2", None))));
        state.accounts.push(desktop_account("lost", None));
        state.in_use.insert(
            "desktop".into(),
            crate::in_use::InUse {
                owner: Some(state.accounts[0].owner()),
                login: "f".into(),
                known_at: NOW,
                named: None,
            },
        );
        let set_aside = |state: &State| {
            let checks = judge_desktop(Os::MacOs, &desktop(), state, &[], NOW, &time::moment);
            said(&checks, "desktop_set_aside")
        };
        assert!(set_aside(&state).is_empty(), "none set aside, nothing said");

        state
            .aside
            .push(aside("pitboard-aside-1", SetAside::InTheWay, None, None));
        let none = set_aside(&state);
        assert_eq!(none[0].0, Level::Ok);
        assert_eq!(
            none[0].1,
            "1 in /Users/dana/.pitboard/parks, none holding a login; they hold only what Claude \
             made while signed out"
        );

        // Parts of an account's login: its earlier keys, or what a give-up left.
        state.aside.push(aside(
            "home-0",
            SetAside::Superseded,
            Some("desktop_home"),
            None,
        ));
        state.aside.push(aside(
            "work-0",
            SetAside::GivenUp,
            Some("desktop_work"),
            None,
        ));
        state.aside.push(aside(
            "gone-0",
            SetAside::GivenUp,
            Some("desktop_gone"),
            None,
        ));
        let parts = set_aside(&state);
        assert_eq!(parts[0].0, Level::Ok);
        for said in [
            "4 in /Users/dana/.pitboard/parks, none holding a login",
            "parts of `desktop/home`'s login, set aside",
            "parts of `desktop/work`'s login, set aside",
            "parts of a login no account here holds, set aside",
            "what Claude made while signed out",
        ] {
            assert!(parts[0].1.contains(said), "{said}: {parts:?}");
        }
        assert!(!parts[0].1.contains("only what Claude made"), "{parts:?}");
        state.aside.truncate(1);

        state.aside.push(aside(
            "home-1",
            SetAside::Superseded,
            Some("desktop_home"),
            Some("a"),
        ));
        state.aside.push(aside(
            "work-1",
            SetAside::Superseded,
            Some("desktop_work"),
            Some("b"),
        ));
        let kept = set_aside(&state);
        assert_eq!(
            kept[0].0,
            Level::Ok,
            "its account holds a newer park, or is in use"
        );
        assert!(
            kept[0]
                .1
                .starts_with("3 in /Users/dana/.pitboard/parks, 2 holding a login: ")
        );
        assert!(
            kept[0]
                .1
                .contains("`desktop/home`'s login from before a sign-in again, set aside"),
            "{kept:?}"
        );
        assert!(
            kept[0]
                .1
                .ends_with("; the rest hold what Claude made while signed out"),
            "{kept:?}"
        );

        state.aside.push(aside(
            "lost-1",
            SetAside::AnotherMachine,
            Some("desktop_lost"),
            Some("c"),
        ));
        state
            .aside
            .push(aside("x-1", SetAside::Orphan, None, Some("d")));
        let warned = set_aside(&state);
        assert_eq!(warned[0].0, Level::Warn);
        assert!(
            warned[0]
                .1
                .contains("`desktop/lost`'s login from another Mac"),
            "{warned:?}"
        );
        assert!(
            warned[0]
                .1
                .contains("a login no account here holds, set aside"),
            "{warned:?}"
        );
    }

    // Claude's settings name an account since Pitboard last read Claude signed out: doctor
    // says it as the read's warning does.
    #[test]
    fn claude_desktop_named_since_signed_out_reads_as_the_warning_does() {
        let mut state = State::default();
        state.accounts.push(desktop_account("work", None));
        let nobody = crate::in_use::InUse {
            owner: None,
            login: String::new(),
            known_at: NOW - 60,
            named: None,
        };
        state.in_use.insert("desktop".into(), nobody.clone());
        let mut facts = desktop();
        facts.known = Some(crate::in_use::Known {
            last: Some(nobody),
            doubt: Some(crate::in_use::Doubt::NamedMoved),
            own_record: Some(state.accounts[0].owner()),
        });
        let checks = judge_desktop(Os::MacOs, &facts, &state, &[], NOW, &time::moment);
        let login = said(&checks, "desktop_login");
        let warned = crate::service::Warning::InUseUnconfirmed {
            tool: ProviderId::Desktop,
            last: Stored::Nothing,
            named: Stored::Account("desktop/work".into()),
            asked: true,
        }
        .to_string();
        assert_eq!(
            login[0].1,
            "Claude's settings have named `desktop/work` since Pitboard last read Claude signed \
             out, so somebody may have signed in"
        );
        assert!(warned.starts_with(&login[0].1), "{warned}");
    }

    // Each service Pitboard asks is named once, and Claude Desktop, which it asks nothing,
    // adds none.
    #[test]
    fn asking_names_each_service_it_asks_once() {
        let account = |label: &str, detail| crate::state::Account {
            label: label.into(),
            id: label.into(),
            account_uuid: label.into(),
            email: String::new(),
            parked: None,
            last_used_at: None,
            replaced_at: None,
            plan: None,
            detail,
        };
        let claude = || crate::state::Detail::Claude {
            organization_uuid: "org".into(),
            oauth_account: json!({}),
        };
        let codex = || crate::state::Detail::Codex { workspace_id: None };
        let desktop = || crate::state::Detail::Desktop {};
        let named = |accounts: Vec<crate::state::Account>| {
            let mut f = facts();
            f.state = Ok(State {
                accounts,
                ..State::default()
            });
            judge_asking(&f).name
        };
        assert_eq!(named(vec![account("work", desktop())]), "asking Anthropic");
        assert_eq!(
            named(vec![account("alpha", claude()), account("work", desktop())]),
            "asking Anthropic"
        );
        assert_eq!(
            named(vec![
                account("alpha", claude()),
                account("main", codex()),
                account("work", desktop()),
            ]),
            "asking Anthropic and OpenAI"
        );
    }

    fn facts() -> Facts {
        Facts {
            os: crate::host::OS,
            sid: None,
            security_tool: Some("/usr/bin/security".into()),
            config_path: PathBuf::from("/home/x/.claude.json"),
            config: Ok(json!({"oauthAccount": {}})),
            config_account: Some(claude::ConfigAccount {
                email: "a@b.c".into(),
                account_uuid: "acc".into(),
                organization_uuid: "org".into(),
                organization_name: None,
            }),
            credential_parts: Vec::new(),
            credential_cost: Some(store::Cost {
                needs: 900,
                limit: 4032,
                second_route: true,
            }),
            no_argv: false,
            service: "Claude Code-credentials".into(),
            account: "someone".into(),
            default_slot: true,
            storage_dir: "/home/x/.claude".into(),
            backend: Ok(store::Backend::Keychain),
            vault: store::Backend::Keychain,
            credential_file: PathBuf::from("/home/x/.claude/.credentials.json"),
            credential: Ok(Some(json!({"claudeAiOauth": {
                "refreshToken": "r",
                "refreshTokenExpiresAt": 2_000_000_000_000i64
            }}))),
            home: PathBuf::from("/home/x/.pitboard"),
            home_access: Some(mode(0o700)),
            readable_by_others: Vec::new(),
            machine_id_known: true,
            windows: None,
            hover_rest_env: false,
            daemon: None,
            pending_parks: Vec::new(),
            claude_version: Some(crate::provider::claude::assumptions::VERIFIED_AGAINST.into()),
            claude_program: Some(PathBuf::from("/opt/homebrew/bin/claude")),
            auth_overrides: Vec::new(),
            asking_held: Vec::new(),
            state: Ok(State::default()),
            in_use: Some(in_use_as_named()),
            parks: Vec::new(),
            interrupted: false,
            stuck: None,
            codex: no_codex(),
            desktop: None,
            claude_present: true,
            schedule: None,
            watch_service: None,
            network: direct(),
            in_the_app: false,
            elevation: crate::host::Elevation::Normal,
            fallback_login: None,
            floor: crate::host::Floor::Met,
            status_line: None,
            windows_settings: None,
            pitboards: Vec::new(),
            wsl: None,
            now: NOW,
        }
    }

    /// What [`facts`] holds of whose login Claude Code has stored: the login in its
    /// keychain, `acc`'s in `org`, as Anthropic named it a minute ago, with the config naming
    /// it too.
    fn in_use_as_named() -> crate::in_use::Known {
        let owner = crate::provider::Owner {
            account_uuid: "acc".into(),
            email: "a@b.c".into(),
            organization_uuid: "org".into(),
        };
        let stored = json!({"claudeAiOauth": {"refreshToken": "r"}});
        crate::in_use::Known {
            last: Some(crate::in_use::InUse {
                owner: Some(owner.clone()),
                login: crate::provider::document(ProviderId::Claude).fingerprint(&stored),
                known_at: NOW - 60,
                named: Some(crate::state::new_id(ProviderId::Claude, &owner)),
            }),
            doubt: None,
            own_record: Some(owner),
        }
    }

    fn no_codex() -> CodexFacts {
        CodexFacts {
            home: PathBuf::from("/home/x/.codex"),
            present: false,
            enrolled: 0,
            backend: "file",
            backend_setting: "Codex's default".into(),
            backend_remedy: String::new(),
            auth_file: PathBuf::from("/home/x/.codex/auth.json"),
            auth_access: None,
            login: Ok(None),
            program: None,
            version: None,
            running: Some(Vec::new()),
            stored_elsewhere: (0, 0),
            restrictions: Vec::new(),
            restrictions_unreadable: false,
            ruled_out: Vec::new(),
        }
    }

    const SID: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";

    // On Windows two `icacls` a path, and none where the person's SID could not be read.
    #[test]
    fn advice_names_a_command_only_where_there_is_one() {
        assert_eq!(
            running(&Os::MacOs.make_private_commands(Kind::File, &["/a"], Some(SID))),
            ": `chmod 600 /a`."
        );
        assert_eq!(
            running(&Os::Linux.make_private_commands(Kind::Directory, &["/a", "/b"], None)),
            ": `chmod 700 /a /b`."
        );
        let windows =
            Os::Windows.make_private_commands(Kind::File, &[r"C:\a", r"C:\b c\"], Some(SID));
        assert_eq!(
            windows,
            [r"C:\a", r"C:\b c"]
                .map(|path| crate::host::acl::icacls(false, path, SID))
                .concat()
        );
        assert_eq!(
            running(&windows),
            format!(
                ": `{}`, `{}`, `{}` and `{}`.",
                windows[0], windows[1], windows[2], windows[3]
            )
        );
        assert!(
            Os::Windows
                .make_private_commands(Kind::File, &[r"C:\a"], None)
                .is_empty()
        );
        // No one `icacls` line serves a file and a folder alike.
        assert!(
            Os::Windows
                .make_private_commands(Kind::Any, &[r"C:\a"], Some(SID))
                .is_empty()
        );
        assert_eq!(running(&[]), ".");
    }

    fn open_to_users() -> Access {
        Access {
            shared: true,
            described: "open to Users, you, SYSTEM and Administrators".into(),
        }
    }

    #[test]
    fn a_login_others_on_a_windows_pc_can_read_is_made_private_one_path_at_a_time() {
        let mut f = facts();
        f.os = Os::Windows;
        f.sid = Some(SID.into());
        let (claude, vault, park) = (
            r"C:\Users\dana\.claude\.credentials.json",
            r"C:\Users\dana\AppData\Local\Pitboard\vault",
            r"C:\Users\dana\AppData\Local\Pitboard\vault\pitboard-park-x.json",
        );
        f.readable_by_others = vec![
            (claude.into(), Kind::File, open_to_users()),
            (vault.into(), Kind::Directory, open_to_users()),
            (park.into(), Kind::File, open_to_users()),
        ];
        let checks = evaluate(&f);
        let found = check(&checks, "private_on_disk");
        assert_eq!(found.level, Level::Fail);
        assert_eq!(
            found.detail,
            format!(
                "{claude} is open to Users, you, SYSTEM and Administrators; {vault} is open to \
                 Users, you, SYSTEM and Administrators; {park} is open to Users, you, SYSTEM and \
                 Administrators"
            )
        );
        // A folder's inheritance on a file leaves it open to nobody, its owner too.
        let [
            [claude, claude_grant],
            [vault, vault_grant],
            [park, park_grant],
        ] = [(claude, false), (vault, true), (park, false)]
            .map(|(path, folder)| crate::host::acl::icacls(folder, path, SID));
        assert_eq!(
            found.advice,
            format!(
                "These hold a login: Claude Code's file in plain text, a sealed park only this \
                 account opens. Claude Code's file takes the access list of the folder it is \
                 in, and a seal stops reading, not writing. Anyone else on this PC can read or \
                 change them: `{claude}`, `{claude_grant}`, `{vault}`, `{vault_grant}`, \
                 `{park}` and `{park_grant}`."
            )
        );
        f.sid = None;
        assert!(
            check(&evaluate(&f), "private_on_disk")
                .advice
                .ends_with("can read or change them."),
            "no command without the SID"
        );
    }

    #[test]
    fn a_pitboard_folder_others_on_a_windows_pc_can_read_says_so() {
        let mut f = facts();
        f.os = Os::Windows;
        f.sid = Some(SID.into());
        f.home = PathBuf::from(r"C:\Users\dana\AppData\Local\Pitboard");
        f.home_access = Some(open_to_users());
        let checks = evaluate(&f);
        let home = check(&checks, "home");
        assert_eq!(home.level, Level::Warn);
        assert_eq!(
            home.detail,
            r"C:\Users\dana\AppData\Local\Pitboard is open to Users, you, SYSTEM and Administrators"
        );
        let [reset, grant] = crate::host::acl::icacls(true, &f.home.display().to_string(), SID);
        assert_eq!(
            home.advice,
            format!(
                "Park names contain account identifiers, so only you should read it: `{reset}` \
                 and `{grant}`."
            )
        );
    }

    /// Codex's section for a machine with no Codex accounts parked.
    fn codex_checks(codex: &CodexFacts) -> Vec<Check> {
        judge_codex(crate::host::OS, None, codex, &[], NOW, &time::moment)
    }

    /// A machine whose Codex is signed in, with its login where it should be.
    fn with_codex() -> CodexFacts {
        CodexFacts {
            present: true,
            auth_access: Some(mode(0o600)),
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
            name: Key::new(ProviderId::Claude, label).typed(),
            active: false,
            park: Some(Park {
                service: format!("pitboard-park-{label}-1"),
                parked_at: NOW - 86_400,
                kind: crate::state::ParkKind::Vault,
                login: "f".into(),
                access_expires_at: None,
                refresh_expires_at,
                session_expires_at: None,
            }),
            unreadable: None,
        }
    }

    /// A Codex account's park, named as a command here would take it.
    fn codex_parked(label: &str, refresh_expires_at: Option<i64>) -> ParkFact {
        ParkFact {
            provider: ProviderId::Codex,
            name: Key::new(ProviderId::Codex, label).typed(),
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

    // Read off a real disk: a gatherer that found nothing would pass every facts test.
    #[test]
    fn a_world_readable_park_is_found_on_the_disk() {
        use crate::host::fs::testing;

        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();

        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_memory_stores(crate::host::memory::MemoryHost::new());
        let vault = store::vault_dir(&ctx);
        crate::host::fs::create_private_dir(Permit::for_a_test(), &vault).expect("a vault");
        assert!(
            loose_logins(&ctx).is_empty(),
            "a private vault is not loose"
        );

        let park = vault.join("pitboard-park-x.json");
        std::fs::write(&park, "{}").expect("a park");
        testing::open_to_others(&park).expect("opened to others");
        let found = loose_logins(&ctx);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(Some(&found[0].2), crate::host::fs::access(&park).as_ref());
        assert!(found[0].0.ends_with("pitboard-park-x.json"), "{found:?}");
        assert_eq!(found[0].1, Kind::File);

        testing::make_private(&park).expect("made private");
        assert!(loose_logins(&ctx).is_empty(), "a private park is not loose");

        testing::open_to_others(&vault).expect("opened to others");
        let found = loose_logins(&ctx);
        let folder = found
            .iter()
            .find(|(path, ..)| *path == vault.display().to_string())
            .expect("the vault");
        assert_eq!(folder.1, Kind::Directory, "{found:?}");
    }

    // A seal stops others reading a park, not writing it: a vault or park another account can
    // change is still a finding.
    #[test]
    fn a_sealed_park_others_can_change_is_reported() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_memory_stores(crate::host::memory::MemoryHost::sealed());
        let vault = store::vault_dir(&ctx);
        crate::host::fs::create_private_dir(Permit::for_a_test(), &vault).expect("a vault");
        let park = vault.join("pitboard-park-x.json");
        std::fs::write(&park, "sealed").expect("a park");
        crate::host::fs::testing::open_to_others(&park).expect("opened to others");
        let found = loose_logins(&ctx);
        assert!(
            found
                .iter()
                .any(|(path, ..)| *path == park.display().to_string()),
            "a sealed park others can change: {found:?}"
        );

        crate::host::fs::testing::make_private(&park).expect("made private");
        crate::host::fs::testing::open_to_others(&vault).expect("opened to others");
        let found = loose_logins(&ctx);
        assert!(
            found
                .iter()
                .any(|(path, kind, _)| *path == vault.display().to_string()
                    && *kind == Kind::Directory),
            "the opened vault: {found:?}"
        );
    }

    /// On a machine with no keychain every parked login is a plaintext OAuth token in a
    /// file, and the only thing between it and everyone else with an account here is a
    /// mode bit. A backup restore, a `cp -r`, an rsync or a careless umask changes one
    /// quietly, and nothing else in Pitboard would ever mention it.
    #[test]
    fn a_login_anyone_on_this_machine_can_read_is_a_failure() {
        let mut f = facts();
        f.os = Os::Linux;
        assert_eq!(check(&evaluate(&f), "private_on_disk").level, Level::Ok);

        f.readable_by_others = vec![
            (
                "/home/a/.pitboard/vault/pitboard-park-x.json".into(),
                Kind::File,
                mode(0o644),
            ),
            (
                "/home/a/.claude/.credentials.json".into(),
                Kind::File,
                mode(0o640),
            ),
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

    /// A keychain locked where it cannot ask to be unlocked, as over SSH, is one thing to
    /// fix, said once with what fixes it and what Claude Code does meanwhile. Every parked
    /// login behind it used to fail as well, each advised to sign in again for nothing.
    #[test]
    fn a_locked_keychain_is_one_failure_and_sends_nobody_to_sign_in_again() {
        let mut f = facts();
        f.os = Os::MacOs;
        f.credential = Err(store::Error::Locked);
        let mut behind = parked("work", Some(NOW + 20 * 86_400));
        behind.unreadable = Some(Unreadable::Locked);
        f.parks = vec![behind];

        let checks = evaluate(&f);
        let credential = check(&checks, "credential");
        assert_eq!(credential.level, Level::Fail);
        assert_eq!(
            credential.detail,
            "the keychain is locked, and cannot ask to be unlocked from here"
        );
        assert!(
            credential.advice.contains("`security unlock-keychain`")
                && credential.advice.contains("keeps the login it last read"),
            "{}",
            credential.advice
        );
        let park = named(&checks, "account work");
        assert_eq!(park.level, Level::Warn, "{}", park.detail);
        assert_eq!(park.detail, "not read: the keychain is locked");
        assert!(!park.advice.contains("--sign-in"), "{}", park.advice);
        assert_eq!(
            checks.iter().filter(|c| c.level == Level::Fail).count(),
            1,
            "one cause, one failure"
        );

        // Where a login is left in the fallback file, a session started here signs in
        // with it, and the advice says so.
        f.fallback_login = Some(FallbackLogin {
            path: f.credential_file.clone(),
            fingerprint: Ok(Some("0123456789abcdef".into())),
        });
        let advice = check(&evaluate(&f), "credential").advice.clone();
        assert!(
            advice.contains("signs in with the login in /home/x/.claude/.credentials.json"),
            "{advice}"
        );
    }

    /// A login left in `.credentials.json` behind the one in the keychain is what a Claude
    /// Code that cannot read the keychain signs in with, and no switch reaches it.
    #[test]
    fn a_login_left_behind_the_keychain_is_said_with_what_reads_it() {
        let mut f = facts();
        f.os = Os::MacOs;
        assert!(evaluate(&f).iter().all(|c| c.code != "fallback_login"));

        f.fallback_login = Some(FallbackLogin {
            path: f.credential_file.clone(),
            fingerprint: Ok(Some("0123456789abcdef".into())),
        });
        let checks = evaluate(&f);
        let said = check(&checks, "fallback_login");
        assert_eq!(said.level, Level::Warn);
        assert_eq!(said.name, "fallback login");
        assert_eq!(
            said.detail,
            "/home/x/.claude/.credentials.json  ·  refresh 0123456789abcdef"
        );
        assert!(
            said.advice.contains("over SSH")
                && said.advice.contains(
                    "While it is there, Claude Code sessions already running at a switch keep \
                     the account they are on until their login is next renewed"
                )
                && said.advice.ends_with(
                    "`pitboard stow` keeps this login for its account, where Pitboard holds no \
                     other it can switch to, then deletes the file, which leaves one login for \
                     every session."
                ),
            "{}",
            said.advice
        );
        assert!(!said.advice.contains("rm "), "{}", said.advice);
        assert!(healthy(&checks), "nothing Pitboard does is stopped by it");
    }

    /// A running session watches the file by its being there, so one with no login in it
    /// holds sessions to their account after a switch all the same. It is said, with what
    /// lets them follow, and the login check's advice does not send a session to it.
    #[test]
    fn a_file_behind_the_keychain_with_no_login_in_it_is_said_with_what_it_holds_back() {
        let mut f = facts();
        f.fallback_login = Some(FallbackLogin {
            path: f.credential_file.clone(),
            fingerprint: Ok(None),
        });
        let checks = evaluate(&f);
        let said = check(&checks, "fallback_login");
        assert_eq!(said.level, Level::Warn);
        assert_eq!(
            said.detail,
            "/home/x/.claude/.credentials.json  ·  no login in it"
        );
        assert_eq!(
            said.advice,
            "It holds no Claude Code login. While it is there, Claude Code sessions already \
             running at a switch keep the account they are on until their login is next \
             renewed, or until they are started again. `pitboard stow` deletes it, which lets \
             them follow a switch."
        );
        assert!(
            check(&checks, "auth_source")
                .detail
                .ends_with(", once a session reads it again"),
            "{}",
            check(&checks, "auth_source").detail
        );

        f.os = Os::MacOs;
        f.credential = Err(store::Error::Locked);
        let advice = check(&evaluate(&f), "credential").advice.clone();
        assert!(
            advice.ends_with("one started here is signed out."),
            "{advice}"
        );
    }

    /// A running session looks at the file and never reads it to decide, so one Pitboard
    /// cannot read holds sessions to their account after a switch all the same. It is said
    /// with why, without guessing whether a login is in it, and so is the login check's
    /// advice for a session that cannot read the keychain.
    #[test]
    fn a_file_behind_the_keychain_that_cannot_be_read_is_said_with_why() {
        let mut f = facts();
        f.os = Os::MacOs;
        f.fallback_login = Some(FallbackLogin {
            path: f.credential_file.clone(),
            fingerprint: Err(store::Error::Unreadable(
                "cannot read /home/x/.claude/.credentials.json: Permission denied".into(),
            )),
        });
        let checks = evaluate(&f);
        let said = check(&checks, "fallback_login");
        assert_eq!(said.level, Level::Warn);
        assert_eq!(
            said.detail,
            "/home/x/.claude/.credentials.json  ·  not read: the credential store could not be \
             read: cannot read /home/x/.claude/.credentials.json: Permission denied"
        );
        assert_eq!(
            said.advice,
            "Pitboard could not read it, so whether it holds a login, which a session that \
             cannot read the keychain signs in with, cannot be told, and `pitboard stow` cannot \
             put it away. While it is there, Claude Code sessions already running at a switch \
             keep the account they are on until their login is next renewed, or until they are \
             started again. Deleting it lets them follow a switch, and takes whatever it holds \
             with it: `rm '/home/x/.claude/.credentials.json'`."
        );
        assert!(healthy(&checks), "nothing Pitboard does is stopped by it");

        f.credential = Err(store::Error::Locked);
        let advice = check(&evaluate(&f), "credential").advice.clone();
        assert!(
            advice.ends_with(
                "whether one started here signs in with what /home/x/.claude/.credentials.json \
                 holds cannot be told, since Pitboard could not read it."
            ),
            "{advice}"
        );
    }

    // Behind Credential Manager, a network logon is what reads the file, and PowerShell deletes
    // it, with each single quote in the path doubled.
    #[test]
    fn a_file_behind_credential_manager_is_said_in_the_words_of_windows() {
        let mut f = facts();
        f.os = Os::Windows;
        let path = PathBuf::from(r"C:\Users\o'neil\.claude\.credentials.json");
        f.fallback_login = Some(FallbackLogin {
            path: path.clone(),
            fingerprint: Ok(Some("0123456789abcdef".into())),
        });
        let said = check(&evaluate(&f), "fallback_login").advice.clone();
        assert!(
            said.starts_with(
                "Claude Code signs in with this wherever it cannot reach Credential Manager, \
                 such as in a sign-in over SSH with a key, and no switch reaches it. A sign-in \
                 made where Credential Manager could not be reached leaves one, as `/login` does \
                 in a sign-in over SSH with a key. While it is there,"
            ),
            "{said}"
        );

        f.fallback_login = Some(FallbackLogin {
            path,
            fingerprint: Err(store::Error::Unreadable("access is denied".into())),
        });
        let said = check(&evaluate(&f), "fallback_login").advice.clone();
        assert!(
            said.starts_with(
                "Pitboard could not read it, so whether it holds a login, which a session that \
                 cannot reach Credential Manager signs in with, cannot be told"
            ),
            "{said}"
        );
        assert!(
            said.ends_with(
                r"with it: `Remove-Item -LiteralPath 'C:\Users\o''neil\.claude\.credentials.json'`."
            ),
            "{said}"
        );
        assert_eq!(
            Os::Windows.remove_file_command(std::path::Path::new("C:\\it\u{2019}s")),
            "Remove-Item -LiteralPath 'C:\\it\u{2019}\u{2019}s'"
        );
        assert_eq!(
            Os::MacOs
                .remove_file_command(std::path::Path::new("/Users/x/.claude/.credentials.json")),
            "rm '/Users/x/.claude/.credentials.json'"
        );
        assert_eq!(
            Os::Linux.remove_file_command(std::path::Path::new(
                "/home/x/Application Support/it's/.credentials.json"
            )),
            r"rm '/home/x/Application Support/it'\''s/.credentials.json'"
        );
    }

    /// What the checks above are given, read off the stores: the file while the keychain
    /// holds the login in use, with the login in it, none, or why it cannot be read, and
    /// nothing where the file is the store in use.
    #[test]
    fn a_login_left_behind_the_keychain_is_found_in_its_file() {
        use crate::host::memory::MemoryHost;
        use crate::store::memory::Fault;

        let host = MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(Arc::clone(&host));
        let service = claude::live_service(&ctx);
        let file = host.file_at(claude_live::credential_file(&ctx));
        let login = |token: &str| json!({"claudeAiOauth": {"refreshToken": token}}).to_string();

        host.live().plant(&service, &login("in use"));
        assert!(fallback_login(&ctx).is_none());

        file.plant(&service, &login("left"));
        let found = fallback_login(&ctx).expect("a login behind the keychain's");
        assert_eq!(found.path, claude_live::credential_file(&ctx));
        assert_eq!(
            found.fingerprint.expect("readable"),
            Some(store::fingerprint("left"))
        );

        file.plant(&service, &json!({"mcpOAuth": {}}).to_string());
        let found = fallback_login(&ctx).expect("what `/logout` leaves is still there");
        assert_eq!(
            found.fingerprint.expect("readable"),
            None,
            "and holds no login"
        );

        file.plant(&service, &login("left"));
        file.fault(
            &service,
            Fault::UnreadableContents("permission denied".into()),
        );
        let found = fallback_login(&ctx).expect("what cannot be read is still there");
        assert!(
            matches!(found.fingerprint, Err(store::Error::Unreadable(_))),
            "and what it holds is not guessed"
        );
        file.heal(&service);

        host.live().delete_everything();
        assert!(
            fallback_login(&ctx).is_none(),
            "the file is the login in use when the keychain holds none"
        );
    }

    /// Read from the vault, a park behind a locked keychain is locked, not broken: nothing
    /// is known of it but that.
    #[test]
    fn a_park_behind_a_locked_keychain_is_read_as_locked() {
        use crate::host::memory::MemoryHost;
        use crate::store::memory::Fault;

        let host = MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(Arc::clone(&host));
        let state = State {
            accounts: vec![crate::state::Account {
                label: "work".into(),
                id: "work-uuid".into(),
                account_uuid: "work-uuid".into(),
                email: "work@example.com".into(),
                parked: parked("work", Some(NOW + 20 * 86_400)).park,
                last_used_at: None,
                replaced_at: None,
                plan: None,
                detail: crate::state::Detail::Claude {
                    organization_uuid: "org".into(),
                    oauth_account: json!({}),
                },
            }],
            ..State::default()
        };
        assert_eq!(
            park_facts(&ctx, &state)[0].unreadable,
            Some(Unreadable::Broken("missing from the vault".into()))
        );
        host.vault().fault_all(Fault::Locked);
        assert_eq!(
            park_facts(&ctx, &state)[0].unreadable,
            Some(Unreadable::Locked)
        );
    }

    /// The login in use is judged by when its refresh token expires as a parked one is, and
    /// said in the same words: due within `RENEW_WITHIN`, and expired once that has passed. One
    /// that gives no date has none to say, which is no warning.
    #[test]
    fn the_login_in_use_is_judged_by_when_its_refresh_token_expires_as_a_park_is() {
        let credential = |f: &mut Facts, expires: Option<i64>| {
            let mut oauth = json!({"refreshToken": "r"});
            if let Some(at) = expires {
                oauth["refreshTokenExpiresAt"] = json!(at * 1000);
            }
            f.credential = Ok(Some(json!({ "claudeAiOauth": oauth })));
            let checks = evaluate(f);
            let said = check(&checks, "credential");
            (said.level, said.detail.clone(), said.advice.clone())
        };
        let mut f = facts();
        let now = f.now;
        let (level, detail, _) = credential(&mut f, Some(now + RENEW_WITHIN));
        assert_eq!(level, Level::Ok);
        assert!(detail.contains("  ·  good for 3d 0h  ·  "), "{detail}");
        let (level, detail, advice) = credential(&mut f, Some(now + RENEW_WITHIN - 1));
        assert_eq!(level, Level::Warn);
        assert!(detail.contains("  ·  expires in 2d 23h  ·  "), "{detail}");
        assert_eq!(
            advice,
            "This login expires soon and will need signing in again."
        );
        let (level, detail, advice) = credential(&mut f, Some(now - 60));
        assert_eq!(level, Level::Warn);
        assert!(detail.contains("  ·  expired  ·  "), "{detail}");
        assert_eq!(
            advice,
            "This login has expired: sign in to Claude Code again."
        );
        let (level, detail, _) = credential(&mut f, None);
        assert_eq!(level, Level::Ok);
        assert!(!detail.contains("days"), "{detail}");
    }

    #[test]
    fn a_loose_home_directory_is_flagged() {
        let mut f = facts();
        f.home_access = Some(mode(0o755));
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
            // On the keychain, the flag changes nothing Pitboard reads.
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

    // On macOS the file is a fallback after a keychain write failed. On Windows the chain is
    // the file only where nothing turns Credential Manager on and it holds no login.
    #[test]
    fn a_file_store_is_the_whole_answer_on_linux_and_windows() {
        let mut f = facts();
        f.backend = Ok(store::Backend::File);
        for (os, level, advice) in [
            (Os::Linux, Level::Ok, ""),
            (
                Os::MacOs,
                Level::Warn,
                "Claude Code fell back to a file, which means a keychain write failed at some \
                 point.",
            ),
            (Os::Windows, Level::Ok, ""),
        ] {
            f.os = os;
            let checks = evaluate(&f);
            let store = check(&checks, "credential_store");
            assert_eq!(
                (store.level, store.advice.as_str()),
                (level, advice),
                "{os:?}"
            );
            assert!(store.detail.starts_with("plaintext file"), "{os:?}");
        }
    }

    /// The failure that would follow Claude Code moving where it keeps a login: not an
    /// absence, a mismatch, and the difference is what decides whether the advice is "sign
    /// in" or "Pitboard is looking in the wrong place".
    #[test]
    fn a_config_that_names_somebody_signed_in_with_no_login_anywhere_is_a_failure() {
        let mut f = facts();
        f.backend = Ok(store::Backend::Absent);
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(store.level, Level::Fail);
        assert!(store.detail.contains("a@b.c"));

        // Nobody signed in at all is an ordinary state with an ordinary answer.
        f.config_account = None;
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(store.level, Level::Warn);
        assert!(store.advice.contains("Nothing is signed in"));
    }

    // Enterprise persistence (`credman_attributes`): the login roams with a roaming profile.
    #[test]
    fn credential_manager_is_named_by_its_item_and_a_roaming_profile_is_warned_of() {
        let mut f = facts();
        f.os = Os::Windows;
        f.backend = Ok(store::Backend::CredentialManager);
        f.account = "claude-code-user".into();
        f.windows = Some(WindowsHomes {
            home: PathBuf::from(r"C:\Users\dana"),
            default_pitboard_home: PathBuf::from(r"C:\Users\dana\AppData\Local\Pitboard"),
            local_app_data: None,
            profile: None,
            codex_home_set: false,
            roams: false,
        });
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(store.level, Level::Ok);
        assert_eq!(
            store.detail,
            "Credential Manager  ·  Claude Code-credentials/claude-code-user  ·  every logon of \
             yours on this PC reads it"
        );
        if let Some(homes) = f.windows.as_mut() {
            homes.roams = true;
        }
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(store.level, Level::Warn);
        assert!(
            store.advice.starts_with("Your profile roams"),
            "{}",
            store.advice
        );
    }

    #[test]
    fn a_login_in_credential_manager_is_counted_in_pieces_and_257_fail() {
        let mut f = facts();
        f.os = Os::Windows;
        f.backend = Ok(store::Backend::CredentialManager);
        let size = |f: &Facts, needs: usize| {
            let mut f_cost = f.credential_cost.expect("a cost");
            f_cost.needs = needs;
            judge_pieces(f_cost, &f.credential_parts)
        };
        f.credential_cost = Some(store::Cost {
            needs: 900,
            limit: 460_800,
            second_route: false,
        });
        assert_eq!(
            check(&evaluate(&f), "credential_size").detail,
            "900 bytes, in one item"
        );
        let pieces = size(&f, 2401);
        assert_eq!(pieces.level, Level::Ok);
        assert_eq!(pieces.detail, "2401 bytes, in 2 pieces of 2400 characters");
        let past = size(&f, 460_801);
        assert_eq!(past.level, Level::Fail);
        assert_eq!(
            past.detail,
            "460801 bytes, past the 460800 Claude Code keeps in Credential Manager"
        );
    }

    #[test]
    fn pitboard_no_argv_is_said_to_change_nothing_on_windows() {
        let mut f = facts();
        f.no_argv = true;
        f.os = Os::MacOs;
        assert!(evaluate(&f).iter().all(|c| c.code != "no_argv"));
        f.os = Os::Windows;
        let checks = evaluate(&f);
        let said = check(&checks, "no_argv");
        assert_eq!(said.level, Level::Warn);
        assert_eq!(said.detail, "set, and it changes nothing on Windows");
    }

    // A network logon, as over sshd with a key, has no credential set: 1312 (MC-net).
    #[test]
    fn credential_manager_out_of_this_logons_reach_is_said_in_its_own_words() {
        let mut f = facts();
        f.os = Os::Windows;
        f.credential_file = PathBuf::from(r"C:\Users\dana\.claude\.credentials.json");
        f.credential = Err(store::Error::Locked);
        let checks = evaluate(&f);
        let said = check(&checks, "credential");
        assert_eq!(said.level, Level::Fail);
        assert_eq!(
            said.detail,
            "Credential Manager cannot be reached from this logon"
        );
        assert!(
            said.advice.ends_with(
                r"A Claude Code session started from this logon reads only C:\Users\dana\.claude\.credentials.json."
            ),
            "{}",
            said.advice
        );
        assert!(!said.advice.contains("keychain"));
    }

    #[test]
    fn a_login_credential_manager_holds_that_nothing_turns_on_is_said_to_sign_sessions_out() {
        let mut f = facts();
        f.os = Os::Windows;
        f.backend = Err(store::Error::Unreadable(
            "Credential Manager holds a login".into(),
        ));
        let checks = evaluate(&f);
        let said = check(&checks, "credential_store");
        assert_eq!(said.level, Level::Fail);
        assert!(
            said.advice.ends_with(
                "its sessions read only .credentials.json, so a login left only in Credential \
                 Manager is signed out to them."
            ),
            "{}",
            said.advice
        );
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
    /// Pitboard chose; the threshold is the token's own lifetime and all it does is say so.
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
            name: Key::new(ProviderId::Claude, "work").typed(),
            active: false,
            last_used_at: Some(f.now - 31 * 86_400),
            park: Some(Park {
                service: "pitboard-park-acc-1".into(),
                parked_at: f.now - 31 * 86_400,
                kind: crate::state::ParkKind::Vault,
                login: "f".into(),
                access_expires_at: Some(f.now + 3600),
                refresh_expires_at: Some(f.now + 30 * 86_400),
                session_expires_at: None,
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
    fn a_schedule_is_judged_by_whether_the_pitboard_it_runs_is_still_there() {
        let mut f = facts();
        assert!(
            evaluate(&f).iter().all(|c| c.code != "schedule"),
            "nothing is said where there is no schedule"
        );

        f.schedule = Some(ScheduleFact {
            location: Location::File(PathBuf::from(
                "/home/x/Library/LaunchAgents/com.datlechin.pitboard.renew.plist",
            )),
            task: None,
            program: Some(PathBuf::from("/opt/homebrew/bin/pitboard")),
            program_found: true,
            network: direct(),
            network_differs: false,
        });
        let checks = evaluate(&f);
        let found = check(&checks, "schedule");
        assert_eq!(found.level, Level::Ok);
        assert!(found.detail.contains("/opt/homebrew/bin/pitboard"));

        f.schedule.as_mut().expect("set above").program_found = false;
        let checks = evaluate(&f);
        let gone = check(&checks, "schedule");
        assert_eq!(gone.level, Level::Fail, "every renewal from now on fails");
        assert!(gone.detail.contains("/opt/homebrew/bin/pitboard"));
        assert_eq!(gone.advice, again(crate::host::OS));

        f.schedule = Some(ScheduleFact {
            location: Location::File(PathBuf::from(
                "/home/x/Library/LaunchAgents/com.datlechin.pitboard.renew.plist",
            )),
            task: None,
            program: Some(PathBuf::from(
                "/Applications/Pitboard.app/Contents/MacOS/Pitboard",
            )),
            program_found: true,
            network: direct(),
            network_differs: false,
        });
        let checks = evaluate(&f);
        let the_app = check(&checks, "schedule");
        assert_eq!(
            the_app.level,
            Level::Fail,
            "0.3.0's app renews nothing when started with `renew`"
        );
        assert!(
            the_app.detail.contains("the app itself"),
            "{}",
            the_app.detail
        );
        f.schedule.as_mut().expect("set above").program = Some(PathBuf::from(
            "/Applications/Pitboard.app/Contents/Helpers/pitboard",
        ));
        assert_eq!(
            check(&evaluate(&f), "schedule").level,
            Level::Ok,
            "the command line an app comes with is a command line"
        );

        f.schedule.as_mut().expect("set above").program = None;
        let checks = evaluate(&f);
        let unread = check(&checks, "schedule");
        assert_eq!(unread.level, Level::Warn);
        assert!(!unread.advice.is_empty());
    }

    #[test]
    fn the_watch_service_is_judged_by_its_pitboard_and_its_last_run() {
        let mut f = facts();
        assert!(
            evaluate(&f).iter().all(|c| c.code != "watch_service"),
            "nothing is said where there is no service"
        );
        let service = |program: Option<&str>, task: Option<Task>| WatchServiceFact {
            location: Location::File(PathBuf::from(
                "/home/x/Library/LaunchAgents/com.datlechin.pitboard.watch.plist",
            )),
            program: program.map(PathBuf::from),
            program_found: true,
            at: crate::duty::autoswitch::Threshold::new(90),
            task,
        };

        f.watch_service = Some(service(Some("/opt/homebrew/bin/pitboard"), None));
        let checks = evaluate(&f);
        let found = check(&checks, "watch_service");
        assert_eq!(found.level, Level::Ok);
        assert_eq!(
            found.detail,
            "from sign-in  ·  switches at 90%  ·  runs /opt/homebrew/bin/pitboard"
        );
        assert_eq!(found.name, "watch service");
        assert_eq!(section("watch_service"), None, "Pitboard's own");

        f.watch_service.as_mut().expect("set above").program_found = false;
        let checks = evaluate(&f);
        let gone = check(&checks, "watch_service");
        assert_eq!(gone.level, Level::Fail, "it switches nothing from now on");
        assert_eq!(
            gone.advice,
            "Install it again with `pitboard watch install`."
        );

        f.watch_service = Some(service(None, None));
        assert_eq!(check(&evaluate(&f), "watch_service").level, Level::Warn);

        let ran = Task {
            last_run: Some(NOW - 60),
            last_result: Some(1),
            drifted: Vec::new(),
        };
        f.watch_service = Some(service(Some(r"C:\bin\pitboard.exe"), Some(ran)));
        let checks = evaluate(&f);
        let failed = check(&checks, "watch_service");
        assert_eq!(failed.level, Level::Warn);
        assert!(
            failed
                .detail
                .ends_with("and its last run ended with exit code 1"),
            "{}",
            failed.detail
        );
    }

    #[test]
    fn a_task_is_judged_by_its_settings_and_its_last_run() {
        let mut f = facts();
        f.os = Os::Windows;
        let program = r"C:\Users\x\scoop\apps\pitboard\current\pitboard.exe";
        let task = |last_run: Option<i64>, last_result: i32, drifted: Vec<Drift>| ScheduleFact {
            location: Location::Task(r"\Pitboard\Renew S-1-5-21-1".into()),
            task: Some(Task {
                last_run,
                last_result: Some(last_result),
                drifted,
            }),
            program: Some(PathBuf::from(program)),
            program_found: true,
            network: direct(),
            network_differs: false,
        };
        let judged = |f: &Facts| {
            evaluate(f)
                .into_iter()
                .find(|c| c.code == "schedule")
                .expect("the schedule's line")
        };

        f.schedule = Some(task(None, 0x0004_1303, Vec::new()));
        let fresh = judged(&f);
        assert_eq!(fresh.level, Level::Ok);
        assert_eq!(
            fresh.detail,
            format!("daily  ·  runs {program}  ·  not run yet")
        );

        let an_hour_ago = Some(f.now - 3_660);
        for fine in [0, 0x0004_1301] {
            f.schedule = Some(task(an_hour_ago, fine, Vec::new()));
            let ran = judged(&f);
            assert_eq!(ran.level, Level::Ok, "{fine:#x}");
            assert_eq!(
                ran.detail,
                format!("daily  ·  runs {program}  ·  last ran 1h 01m ago")
            );
        }

        for (result, ended) in [
            (1, "with exit code 1"),
            (0x8007_10E0_u32 as i32, "with 0x800710E0"),
        ] {
            f.schedule = Some(task(an_hour_ago, result, Vec::new()));
            let failed = judged(&f);
            assert_eq!(failed.level, Level::Warn);
            assert_eq!(
                failed.detail,
                format!("daily  ·  runs {program}, and its last run, 1h 01m ago, ended {ended}")
            );
            assert!(
                failed.advice.contains("`pitboard renew`"),
                "{}",
                failed.advice
            );
        }

        f.schedule = Some(task(
            an_hour_ago,
            0,
            vec![Drift::OnAcPowerOnly, Drift::Disabled],
        ));
        let drifted = judged(&f);
        assert_eq!(drifted.level, Level::Warn);
        assert_eq!(
            drifted.detail,
            format!(
                "daily  ·  runs {program}, but in Task Scheduler it starts only if the computer \
                 is on AC power and is disabled"
            )
        );
        assert_eq!(drifted.advice, again(Os::Windows));
    }

    /// The way back works from the command line everywhere, and names the app only where
    /// there is one.
    #[test]
    fn a_broken_schedule_is_written_again_by_whatever_this_machine_has() {
        for os in [Os::MacOs, Os::Windows] {
            let with_app = again(os);
            assert!(
                with_app.contains("the app's Settings")
                    && with_app.contains("pitboard schedule install"),
                "{os:?}: {with_app}"
            );
        }
        let linux = again(Os::Linux);
        assert!(linux.contains("pitboard schedule install"), "{linux}");
        assert!(!linux.contains("app"), "there is no app here: {linux}");
    }

    /// What the check above is given, read off a real disk: a schedule written the way
    /// `pitboard schedule install` writes it, whose Pitboard is then taken away.
    #[test]
    fn a_schedule_whose_pitboard_is_gone_is_found_on_the_disk() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();

        let program = root.join("bin/pitboard");
        let ctx = Context::new(root.clone())
            .with_memory_stores(crate::host::memory::MemoryHost::new())
            .with_schedule_program(program.clone());
        assert!(schedule_fact(&ctx).is_none(), "nothing installed yet");

        std::fs::create_dir_all(root.join("bin")).expect("a bin");
        std::fs::write(&program, "").expect("a Pitboard");
        crate::schedule::install(&ctx, Permit::for_a_test()).expect("installed");
        let fact = schedule_fact(&ctx).expect("installed");
        assert_eq!(fact.program.as_deref(), Some(program.as_path()));
        assert!(fact.program_found);

        std::fs::remove_file(&program).expect("taken away");
        assert!(!schedule_fact(&ctx).expect("still installed").program_found);

        assert!(
            schedule_fact(&ctx.with_pitboard_home(root.join("elsewhere"))).is_none(),
            "a Pitboard pointed at another home has no schedule"
        );
    }

    // Through the pretend Task Scheduler, on any system; a reinstall is the fix doctor names.
    #[test]
    fn a_task_changed_in_task_scheduler_is_read_where_it_is_kept() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let home = scratch.path().to_string_lossy().into_owned();
        let program = scratch.path().join("bin").join("pitboard.exe");
        std::fs::create_dir_all(program.parent().expect("its folder")).expect("a bin");
        std::fs::write(&program, "").expect("a Pitboard");
        let env: crate::context::Environment =
            [("HOME", home.as_str()), ("USERPROFILE", home.as_str())]
                .into_iter()
                .collect();
        let ctx = Context::for_command_line(&env)
            .with_memory_stores(crate::host::memory::MemoryHost::scheduling_tasks(
                "S-1-5-21-1",
            ))
            .with_schedule_program(program);
        crate::schedule::install(&ctx, Permit::for_a_test()).expect("installed");
        let fresh = Task {
            last_run: None,
            last_result: Some(0x0004_1303),
            drifted: Vec::new(),
        };
        let read = |ctx: &Context| schedule_fact(ctx).expect("installed").task;
        assert_eq!(read(&ctx), Some(fresh.clone()));

        let kept = crate::schedule::path(&ctx).expect("the task's file");
        let xml = std::fs::read_to_string(&kept).expect("written");
        std::fs::write(
            &kept,
            xml.replace(
                "<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>",
                "<StopIfGoingOnBatteries>true</StopIfGoingOnBatteries>",
            ),
        )
        .expect("changed by hand");
        assert_eq!(
            read(&ctx).map(|task| task.drifted),
            Some(vec![Drift::StopsOnBattery])
        );

        crate::schedule::install(&ctx, Permit::for_a_test()).expect("installed again");
        assert_eq!(read(&ctx), Some(fresh));
    }

    /// A schedule that runs a Pitboard still there, installed with `network`.
    fn scheduled(network: NetworkFact, network_differs: bool) -> ScheduleFact {
        ScheduleFact {
            location: Location::File(PathBuf::from(
                "/home/x/.config/systemd/user/pitboard-renew.timer",
            )),
            task: None,
            program: Some(PathBuf::from("/usr/local/bin/pitboard")),
            program_found: true,
            network,
            network_differs,
        }
    }

    /// A network whose requests go through `proxy`.
    fn by(proxy: ProxyFact) -> NetworkFact {
        NetworkFact {
            set: vec![proxy.variable],
            proxy: Some(proxy),
            passed_over: Vec::new(),
        }
    }

    /// The schedule's runs are given the proxy variables of the Pitboard that installed it.
    /// Where those send requests another way than this run's, doctor says so, with what each
    /// goes through and the user name and password as `***`, and says how to give the
    /// schedule this run's. Where they go the same way, nothing is said.
    #[test]
    fn a_schedule_whose_proxy_differs_from_this_runs_is_said() {
        let mut f = facts();
        f.schedule = Some(scheduled(direct(), false));
        assert!(
            evaluate(&f).iter().all(|c| c.code != "schedule_proxy"),
            "nothing is said where they go the same way"
        );

        f.schedule = Some(scheduled(by(through(None, &[])), true));
        let checks = evaluate(&f);
        let line = check(&checks, "schedule_proxy");
        assert_eq!(
            (line.level, line.name.as_str(), line.detail.as_str()),
            (
                Level::Warn,
                "renewal schedule's proxy",
                "the schedule was installed with http://***:***@proxy.example.com:3128, from \
                 HTTPS_PROXY; this run goes directly"
            )
        );
        assert!(
            line.advice.contains("`pitboard schedule install`"),
            "{}",
            line.advice
        );

        let on_loopback = || ProxyFact {
            variable: "ALL_PROXY",
            address: "http://127.0.0.1:3128".into(),
            server: "127.0.0.1:3128".into(),
            on_loopback: true,
            ..through(Some("NO_PROXY"), &["chatgpt.com"])
        };
        f.network = by(on_loopback());
        f.schedule = Some(scheduled(direct(), true));
        assert_eq!(
            check(&evaluate(&f), "schedule_proxy").detail,
            "the schedule was installed with no proxy; this run goes through \
             http://127.0.0.1:3128, from ALL_PROXY, NO_PROXY exempting chatgpt.com"
        );

        // A SOCKS proxy, which Pitboard does not use yet, is given, not gone through. Installing
        // the schedule again from here would give it that proxy, and its requests would fail
        // too, so the advice is to change the variable first.
        let socks = || ProxyFact {
            address: "socks5h://127.0.0.1:1080".into(),
            server: "127.0.0.1:1080".into(),
            socks: true,
            fails: true,
            ..on_loopback()
        };
        f.network = by(socks());
        let checks = evaluate(&f);
        let line = check(&checks, "schedule_proxy");
        assert_eq!(
            (line.detail.as_str(), line.advice.as_str()),
            (
                "the schedule was installed with no proxy; this run is given \
                 socks5h://127.0.0.1:1080, from ALL_PROXY, NO_PROXY exempting chatgpt.com",
                "The schedule's runs are given the proxy variables it was installed with. \
                 Installed again from here, they would be given this run's SOCKS proxy, which \
                 Pitboard does not use yet, and their requests would fail. Set ALL_PROXY to an \
                 HTTP proxy's address, or unset it, then install it again with `pitboard \
                 schedule install`, which takes the proxy variables set where it runs."
            )
        );
        // With every host Pitboard calls exempt from it, nothing fails, and installing the
        // schedule again from here is the advice as for any other proxy.
        f.network = by(ProxyFact {
            exempt: [
                "api.anthropic.com",
                "platform.claude.com",
                "auth.openai.com",
                "chatgpt.com",
            ]
            .map(String::from)
            .to_vec(),
            fails: false,
            ..socks()
        });
        let checks = evaluate(&f);
        let advice = &check(&checks, "schedule_proxy").advice;
        assert!(
            advice.starts_with(
                "The schedule's runs are given the proxy variables it was installed with. To \
                 give them this run's, install it again from here"
            ),
            "{advice}"
        );

        f.network = by(through(None, &[]));
        f.schedule = Some(scheduled(by(through(None, &[])), true));
        assert_eq!(
            check(&evaluate(&f), "schedule_proxy").detail,
            "the schedule was installed with this run's proxy, \
             http://***:***@proxy.example.com:3128, from HTTPS_PROXY, but with another user \
             name or password"
        );
    }

    /// In the app, whose environment is the system's and names no proxy a shell set, a
    /// schedule installed from a terminal with a proxy differs from it for good, and the
    /// app's switch would take that proxy away. So the line is no warning there: it says what
    /// each goes through, and how each way of installing the schedule chooses its proxy.
    #[test]
    fn in_the_app_a_schedules_other_proxy_is_said_without_a_warning() {
        let mut f = facts();
        f.in_the_app = true;
        f.schedule = Some(scheduled(direct(), false));
        assert!(evaluate(&f).iter().all(|c| c.code != "schedule_proxy"));

        f.schedule = Some(scheduled(by(through(None, &[])), true));
        let checks = evaluate(&f);
        let line = check(&checks, "schedule_proxy");
        assert_eq!(
            (line.level, line.name.as_str(), line.detail.as_str()),
            (
                Level::Ok,
                "renewal schedule's proxy",
                "the schedule was installed with http://***:***@proxy.example.com:3128, from \
                 HTTPS_PROXY; the app goes directly"
            )
        );
        for said in ["Settings", "`pitboard schedule install` in a terminal"] {
            assert!(line.advice.contains(said), "{said}: {}", line.advice);
        }

        f.network = by(through(None, &[]));
        assert_eq!(
            check(&evaluate(&f), "schedule_proxy").detail,
            "the schedule was installed with the app's proxy, \
             http://***:***@proxy.example.com:3128, from HTTPS_PROXY, but with another user \
             name or password"
        );
    }

    /// What the line above is given, read off a real disk: a schedule `pitboard schedule
    /// install` wrote with a proxy, against runs with the same proxy, the same one with
    /// another password, and none. The report a person pastes hides the proxy's host, as it
    /// does this run's.
    #[test]
    fn the_schedules_proxy_is_read_from_what_install_wrote() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let program = root.join("bin/pitboard");
        std::fs::create_dir_all(root.join("bin")).expect("a bin");
        std::fs::write(&program, "").expect("a Pitboard");
        let machine = crate::host::memory::MemoryHost::new();
        let home = root.to_string_lossy().into_owned();
        let run = |proxy: Option<&str>| {
            let env: crate::context::Environment =
                [("HOME", home.as_str()), ("USERPROFILE", home.as_str())]
                    .into_iter()
                    .chain(proxy.map(|address| ("HTTPS_PROXY", address)))
                    .collect();
            Context::for_command_line(&env)
                .with_memory_stores(Arc::clone(&machine))
                .with_schedule_program(program.clone())
        };
        let gathered = |ctx: &Context| {
            let mut f = facts();
            f.schedule = schedule_fact(ctx);
            f.network = network_fact(&ctx.proxy);
            f
        };
        let installer = run(Some("http://alice:s3cret@proxy.example.com:3128"));
        crate::schedule::install(&installer, Permit::for_a_test()).expect("installed");

        let same = gathered(&installer);
        assert!(!same.schedule.as_ref().expect("installed").network_differs);
        assert!(evaluate(&same).iter().all(|c| c.code != "schedule_proxy"));

        let changed = gathered(&run(Some("http://alice:changed@proxy.example.com:3128")));
        let detail = check(&evaluate(&changed), "schedule_proxy").detail.clone();
        assert!(
            detail.ends_with("but with another user name or password"),
            "{detail}"
        );
        for secret in ["alice", "s3cret", "changed"] {
            assert!(!detail.contains(secret), "{secret}: {detail}");
        }

        let direct = run(None);
        let f = gathered(&direct);
        let detail = check(&evaluate(&f), "schedule_proxy").detail.clone();
        assert_eq!(
            detail,
            "the schedule was installed with http://***:***@proxy.example.com:3128, from \
             HTTPS_PROXY; this run goes directly"
        );
        let hidden = redaction_for(&direct, &f).over(&detail);
        assert!(
            hidden.starts_with("the schedule was installed with http://***:***@<proxy ")
                && !hidden.contains("proxy.example.com"),
            "{hidden}"
        );
    }

    #[test]
    fn every_failure_and_warning_tells_the_user_something() {
        let mut f = facts();
        f.credential = Ok(Some(json!({"slackTag": {}})));
        f.home_access = Some(mode(0o755));
        for c in evaluate(&f) {
            if c.level != Level::Ok {
                assert!(!c.advice.is_empty(), "{} has no advice", c.code);
            }
        }
    }

    /// Advice is read as sentences, and a string broken across lines in the source without
    /// its continuation keeps the indentation as a run of spaces in the middle of one.
    #[test]
    fn advice_reads_as_it_was_written() {
        let mut f = facts();
        f.credential = Ok(Some(json!({"slackTag": {}})));
        f.home_access = Some(mode(0o755));
        f.readable_by_others = vec![(
            "/home/a/.claude/.credentials.json".into(),
            Kind::File,
            mode(0o644),
        )];
        for c in evaluate(&f) {
            assert!(!c.advice.contains("  "), "{}: {:?}", c.code, c.advice);
        }
    }

    /// A proxy `HTTPS_PROXY` names, exempting the hosts Pitboard calls that `exempt` lists.
    fn through(exempting: Option<&'static str>, exempt: &[&str]) -> ProxyFact {
        ProxyFact {
            variable: "HTTPS_PROXY",
            address: "http://***:***@proxy.example.com:3128".into(),
            server: "proxy.example.com:3128".into(),
            on_loopback: false,
            socks: false,
            fails: false,
            exempting,
            exempt: exempt.iter().map(|host| (*host).to_string()).collect(),
        }
    }

    /// The network check, for a network that is `proxy`, passing over `passed_over`.
    fn network(proxy: Option<ProxyFact>, passed_over: &[&'static str]) -> Check {
        let mut f = facts();
        f.network = NetworkFact {
            set: passed_over
                .iter()
                .copied()
                .chain(proxy.as_ref().map(|proxy| proxy.variable))
                .collect(),
            proxy,
            passed_over: passed_over.to_vec(),
        };
        let checks = evaluate(&f);
        let found = check(&checks, "network");
        Check {
            code: found.code,
            name: found.name.clone(),
            level: found.level,
            detail: found.detail.clone(),
            advice: found.advice.clone(),
        }
    }

    /// doctor says how Pitboard's requests leave this machine: directly, or through which
    /// proxy and from which variable, and which of the hosts Pitboard calls go around it. It
    /// had no such check, while ureq read the variables itself, so nobody could tell.
    #[test]
    fn the_network_check_names_the_proxy_and_the_variable_it_came_from() {
        let direct = network(None, &[]);
        assert_eq!(
            (direct.level, direct.name.as_str(), direct.detail.as_str()),
            (Level::Ok, "network", "direct: no variable names a proxy")
        );
        for (exempting, exempt, said) in [
            (None, &[][..], ""),
            (
                Some("NO_PROXY"),
                &[],
                "; NO_PROXY exempts none of the hosts Pitboard calls",
            ),
            (
                Some("no_proxy"),
                &["api.anthropic.com"],
                "; no_proxy exempts api.anthropic.com, which Pitboard reaches directly",
            ),
            (
                Some("NO_PROXY"),
                &["api.anthropic.com", "platform.claude.com"],
                "; NO_PROXY exempts api.anthropic.com and platform.claude.com, which Pitboard \
                 reaches directly",
            ),
            (
                Some("NO_PROXY"),
                &[
                    "api.anthropic.com",
                    "platform.claude.com",
                    "auth.openai.com",
                    "chatgpt.com",
                ],
                "; NO_PROXY exempts every host Pitboard calls, which it reaches directly",
            ),
        ] {
            let check = network(Some(through(exempting, exempt)), &[]);
            assert_eq!(check.level, Level::Ok, "{said}");
            assert_eq!(
                check.detail,
                format!("through http://***:***@proxy.example.com:3128, from HTTPS_PROXY{said}")
            );
            assert!(check.advice.is_empty(), "{}", check.advice);
        }
    }

    /// The network check of a context read from `pairs`, and nothing else of the environment
    /// but a home.
    fn network_read_from(pairs: &[(&'static str, &str)]) -> Check {
        let env: crate::context::Environment = pairs
            .iter()
            .copied()
            .chain([("HOME", "/nowhere")])
            .collect();
        let fact = network_fact(&Context::for_command_line(&env).proxy);
        network(fact.proxy, &fact.passed_over)
    }

    /// A SOCKS proxy, which Pitboard does not use yet, is a warning. The check says so in the
    /// words a request fails with, naming the variable that named the proxy, then the proxy,
    /// and says what to set. A host the variable exempting hosts names still goes out
    /// directly; with every host Pitboard calls exempt, nothing fails, and it is no warning.
    ///
    /// It named a SOCKS proxy as one Pitboard went through, like any other, while Pitboard
    /// spoke SOCKS itself.
    #[test]
    fn a_socks_proxy_is_a_warning_naming_its_variable() {
        let socks = network_read_from(&[("all_proxy", "socks5://127.0.0.1:7890")]);
        assert_eq!(
            (socks.level, socks.detail.as_str(), socks.advice.as_str()),
            (
                Level::Warn,
                "requests fail: Pitboard does not use SOCKS proxies yet, and all_proxy names \
                 one, socks5://127.0.0.1:7890",
                "Pitboard goes through HTTP and HTTPS proxies only. For its requests to go \
                 out, set all_proxy to an HTTP proxy's address, such as \
                 http://proxy.example.com:8080, or unset it."
            )
        );

        let some = network_read_from(&[
            ("ALL_PROXY", "socks5h://alice:s3cret@p.example"),
            ("NO_PROXY", "chatgpt.com"),
        ]);
        assert_eq!(
            (some.level, some.detail.as_str()),
            (
                Level::Warn,
                "requests fail: Pitboard does not use SOCKS proxies yet, and ALL_PROXY names \
                 one, socks5h://***:***@p.example:1080; NO_PROXY exempts chatgpt.com, which \
                 Pitboard reaches directly"
            )
        );

        let passed = network_read_from(&[
            ("ALL_PROXY", "not a proxy"),
            ("HTTPS_PROXY", "socks://p.example"),
        ]);
        assert_eq!(
            (passed.level, passed.detail.as_str(), passed.advice.as_str()),
            (
                Level::Warn,
                "requests fail: Pitboard does not use SOCKS proxies yet, and HTTPS_PROXY \
                 names one, socks5://p.example:1080; ALL_PROXY holds no proxy's address \
                 Pitboard can read, so it is passed over",
                "Pitboard goes through HTTP and HTTPS proxies only. For its requests to go \
                 out, set each of ALL_PROXY and HTTPS_PROXY to an HTTP proxy's address, such \
                 as http://proxy.example.com:8080, or unset them."
            )
        );

        let every = network_read_from(&[("HTTPS_PROXY", "socks4://10.0.0.1"), ("no_proxy", "*")]);
        assert_eq!(
            (every.level, every.detail.as_str(), every.advice.as_str()),
            (
                Level::Ok,
                "direct: Pitboard does not use SOCKS proxies yet, and HTTPS_PROXY names one, \
                 socks4://10.0.0.1:1080; no_proxy exempts every host Pitboard calls, which it \
                 reaches directly",
                ""
            )
        );
    }

    /// Where Pitboard passes over a variable that holds no proxy's address, the check warns
    /// and says what to set.
    #[test]
    fn a_variable_pitboard_cannot_read_is_a_warning() {
        let passed = network(None, &["HTTPS_PROXY"]);
        assert_eq!(passed.level, Level::Warn);
        assert_eq!(
            passed.detail,
            "direct: HTTPS_PROXY holds no proxy's address Pitboard can read, so it is passed \
             over"
        );
        assert_eq!(
            passed.advice,
            "Set it to a proxy's address, such as http://proxy.example.com:8080, or unset it."
        );
        let both = ProxyFact {
            variable: "HTTP_PROXY",
            address: "http://proxy.example.com:8080".into(),
            ..through(None, &[])
        };
        let passed = network(Some(both), &["ALL_PROXY", "HTTPS_PROXY"]);
        assert_eq!(passed.level, Level::Warn);
        assert_eq!(
            passed.detail,
            "through http://proxy.example.com:8080, from HTTP_PROXY; ALL_PROXY and HTTPS_PROXY \
             hold no proxy's address Pitboard can read, so they are passed over"
        );
        assert_eq!(
            passed.advice,
            "Set each to a proxy's address, such as http://proxy.example.com:8080, or unset \
             them."
        );
    }

    /// The report a person pastes shows a digest in place of the proxy's host and port,
    /// which can name the company whose network it is, as it does an organisation's name.
    /// A proxy on loopback names nothing beyond this machine, and is shown. The report a
    /// person reads on their own machine hides neither.
    /// Claude Code's Windows build names its Credential Manager item with a literal, as its
    /// macOS build does where no login name passes its screen. A report hides a person's login
    /// name there, and shows the literal, which names nobody.
    #[test]
    fn the_literal_claude_code_names_its_item_with_is_shown_in_a_report() {
        let ctx = Context::new(PathBuf::from("/home/x"));
        let mut f = facts();
        f.account = "claude-code-user".into();
        let detail = "Credential Manager  ·  Claude Code-credentials-0123abcd/claude-code-user";
        assert_eq!(redaction_for(&ctx, &f).over(detail), detail);
        f.account = "dana".into();
        let hidden = redaction_for(&ctx, &f).over("Claude Code-credentials, account dana");
        assert!(!hidden.contains("dana"), "{hidden}");
    }

    #[test]
    fn a_proxys_host_is_redacted_from_a_report_unless_it_is_on_loopback() {
        let ctx = Context::new(PathBuf::from("/home/x"));
        let mut f = facts();
        f.network.proxy = Some(through(None, &[]));
        let detail = check(&evaluate(&f), "network").detail.clone();
        let hidden = redaction_for(&ctx, &f).over(&detail);
        assert!(detail.contains("proxy.example.com:3128"), "{detail}");
        assert!(!hidden.contains("proxy.example.com"), "{hidden}");
        assert!(!hidden.contains("3128"), "{hidden}");
        assert!(
            hidden.starts_with("through http://***:***@<proxy ")
                && hidden.contains("from HTTPS_PROXY"),
            "{hidden}"
        );

        f.network.proxy = Some(ProxyFact {
            address: "http://127.0.0.1:7890".into(),
            server: "127.0.0.1:7890".into(),
            on_loopback: true,
            ..through(None, &[])
        });
        let detail = check(&evaluate(&f), "network").detail.clone();
        assert_eq!(redaction_for(&ctx, &f).over(&detail), detail);
    }

    /// What doctor is given is read from the context, as the agent's proxy is, with the
    /// user name and password in the address hidden, and the hosts exempt judged by ureq.
    #[test]
    fn the_network_is_read_from_the_context() {
        let env: crate::context::Environment = [
            ("HOME", "/nowhere"),
            ("HTTPS_PROXY", "http://alice:s3cret@proxy.example.com:3128"),
            ("NO_PROXY", ".anthropic.com,localhost"),
        ]
        .into_iter()
        .collect();
        let fact = network_fact(&Context::for_command_line(&env).proxy);
        let proxy = fact.proxy.expect("a proxy");
        assert_eq!(proxy.variable, "HTTPS_PROXY");
        assert_eq!(proxy.address, "http://***:***@proxy.example.com:3128");
        assert_eq!(proxy.server, "proxy.example.com:3128");
        assert!(!proxy.on_loopback && !proxy.socks);
        assert_eq!(proxy.exempting, Some("NO_PROXY"));
        assert_eq!(proxy.exempt, ["api.anthropic.com"]);
        assert!(fact.passed_over.is_empty());

        let unit = network_fact(&Context::for_unit_test().proxy);
        assert!(unit.proxy.is_none() && unit.passed_over.is_empty());

        for (address, shown, on_loopback) in [
            ("socks5://localhost:7890", "socks5://localhost:7890", true),
            ("socks4://127.0.0.1", "socks4://127.0.0.1:1080", true),
            ("socks5h://[::1]:1080", "socks5h://[::1]:1080", true),
            ("socks4a://10.0.0.1", "socks4a://10.0.0.1:1080", false),
            ("socks://p.example", "socks5://p.example:1080", false),
            (
                "https://Proxy.Example.com",
                "https://Proxy.Example.com:443",
                false,
            ),
        ] {
            let env: crate::context::Environment = [("HOME", "/nowhere"), ("ALL_PROXY", address)]
                .into_iter()
                .collect();
            let proxy = network_fact(&Context::for_command_line(&env).proxy)
                .proxy
                .expect("a proxy");
            assert_eq!(
                (proxy.address.as_str(), proxy.on_loopback, proxy.socks),
                (shown, on_loopback, shown.starts_with("socks")),
                "{address}"
            );
        }
    }

    #[test]
    fn a_machine_without_a_stable_identifier_is_flagged() {
        let mut f = facts();
        f.machine_id_known = false;
        f.home = PathBuf::from("/elsewhere/pitboard");
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "machine_id").level, Level::Warn);
        assert!(
            check(&checks, "machine_id")
                .advice
                .ends_with("Never copy /elsewhere/pitboard between machines."),
            "the folder in use"
        );
        for os in [Os::MacOs, Os::Linux] {
            f.os = os;
            f.machine_id_known = true;
            assert!(
                evaluate(&f).iter().all(|c| c.code != "machine_id"),
                "{os:?}"
            );
        }
    }

    // The sysprep page names the SID it replaces, and not the MachineGuid machine-uid reads.
    #[test]
    fn a_windows_pc_is_told_a_copy_of_it_keeps_its_machine_guid() {
        let mut f = facts();
        f.os = Os::Windows;
        let checks = evaluate(&f);
        let note = check(&checks, "machine_id");
        assert_eq!(
            (note.level, note.detail.as_str(), note.advice.as_str()),
            (
                Level::Ok,
                "Windows' MachineGuid",
                "Pitboard tells this PC apart by it, and a copy of the PC keeps it, such as a \
                 virtual machine copied whole. Such a copy takes this PC's accounts for its own, \
                 and both renew the same parked logins: on a copy, forget each account and enrol \
                 it again with `--sign-in`."
            )
        );
        f.machine_id_known = false;
        assert_eq!(check(&evaluate(&f), "machine_id").level, Level::Warn);
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
        unusable.unreadable = Some(Unreadable::Broken("missing from the vault".into()));
        f.parks = vec![
            parked("fine", Some(NOW + 20 * 86_400)),
            parked("soon", Some(NOW + 86_400)),
            parked("gone", Some(NOW - 1)),
            unusable,
            ParkFact {
                provider: ProviderId::Claude,
                last_used_at: None,
                name: Key::new(ProviderId::Claude, "empty").typed(),
                active: false,
                park: None,
                unreadable: None,
            },
            ParkFact {
                provider: ProviderId::Claude,
                last_used_at: None,
                name: Key::new(ProviderId::Claude, "live").typed(),
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
        // A parked login's life is said in the column `pitboard status` shows, after the
        // check's own words, and an expired one with when it expired.
        assert_eq!(
            named(&checks, "account fine").detail,
            "parked, good for 20d 0h"
        );
        assert_eq!(
            named(&checks, "account soon").detail,
            "its parked login expires in 1d 0h"
        );
        assert_eq!(
            named(&checks, "account gone").detail,
            format!("its parked login expired {}", time::moment(NOW - 1, NOW))
        );
    }

    #[test]
    fn parks_sealed_to_the_account_are_said_so_and_all_lost_at_once_is_one_finding() {
        let mut f = facts();
        f.parks = vec![
            parked("work", None),
            codex_parked("home", None),
            ParkFact {
                park: None,
                ..parked("empty", None)
            },
        ];
        assert!(evaluate(&f).iter().all(|c| c.code != "vault"), "a keychain");

        f.vault = store::Backend::SealedFile;
        let checks = evaluate(&f);
        let vault = check(&checks, "vault");
        assert_eq!(
            (vault.level, vault.name.as_str(), vault.detail.as_str()),
            (
                Level::Ok,
                "parked logins",
                "sealed to this Windows account, not plain text"
            )
        );

        let lost = Some(Unreadable::Sealed(Some(DPAPI_KEY_RESET)));
        f.parks[0].unreadable = lost.clone();
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "vault").level, Level::Ok, "one of two");
        assert_eq!(named(&checks, "account work").level, Level::Fail);
        assert!(
            named(&checks, "account work")
                .advice
                .contains("pitboard enroll work --sign-in")
        );

        f.parks[1].unreadable = lost;
        let checks = evaluate(&f);
        let vault = check(&checks, "vault");
        assert_eq!(vault.level, Level::Fail);
        assert_eq!(
            vault.detail,
            "none of the 2 parked logins opens for this Windows account"
        );
        assert!(
            vault.advice.ends_with(
                "Sign in to each again: `pitboard enroll work --sign-in` and \
                 `pitboard enroll codex/home --sign-in`."
            ),
            "{}",
            vault.advice
        );
        for code in ["parked_login", "codex_parked_login"] {
            assert!(
                checks
                    .iter()
                    .all(|c| c.code != code || c.level != Level::Fail),
                "{code} is in the one finding"
            );
        }
        assert_eq!(
            named(&checks, "account empty").level,
            Level::Warn,
            "nothing parked"
        );
    }

    // Parks missing from the vault are not a lost key, so doctor does not tell the person to
    // sign every account in again; each keeps its own per-park finding.
    #[test]
    fn missing_sealed_parks_are_not_a_reset_finding() {
        let mut f = facts();
        f.vault = store::Backend::SealedFile;
        f.parks = vec![parked("work", None), codex_parked("home", None)];
        let missing = Some(Unreadable::Broken("missing from the vault".into()));
        f.parks[0].unreadable = missing.clone();
        f.parks[1].unreadable = missing;
        let checks = evaluate(&f);
        assert_eq!(
            check(&checks, "vault").level,
            Level::Ok,
            "missing is not a lost key"
        );
        assert_eq!(named(&checks, "account work").level, Level::Fail);
        assert_eq!(named(&checks, "account codex/home").level, Level::Fail);
    }

    // A sealed park that will not open in this logon (a network logon over SSH, not the reset
    // code) gets neutral wording, not "an administrator reset your password".
    #[test]
    fn sealed_parks_lost_to_this_logon_get_neutral_wording() {
        let mut f = facts();
        f.vault = store::Backend::SealedFile;
        f.parks = vec![parked("work", None), codex_parked("home", None)];
        let sealed = Some(Unreadable::Sealed(Some(5)));
        f.parks[0].unreadable = sealed.clone();
        f.parks[1].unreadable = sealed;
        let checks = evaluate(&f);
        let vault = check(&checks, "vault");
        assert_eq!(vault.level, Level::Fail);
        assert!(
            vault.advice.contains("from a desktop session"),
            "{}",
            vault.advice
        );
        assert!(!vault.advice.contains("administrator"), "{}", vault.advice);
    }

    /// A parked login is due to be renewed from three days before its refresh token expires
    /// until it does. Past that it cannot be renewed, so it is not due: its account needs a
    /// sign-in instead.
    #[test]
    fn a_parked_login_is_due_from_three_days_before_it_expires() {
        assert!(!renewal_due(NOW + RENEW_WITHIN, NOW));
        assert!(renewal_due(NOW + RENEW_WITHIN - 1, NOW));
        assert!(renewal_due(NOW + 1, NOW));
        assert!(!renewal_due(NOW, NOW));
        assert!(!renewal_due(NOW - 60, NOW));
    }

    #[test]
    fn an_interrupted_switch_and_leftover_parks_are_reported() {
        let mut f = facts();
        f.interrupted = true;
        f.state = Ok(State {
            discarded: vec![crate::state::Dropped {
                service: "pitboard-park-x-1".into(),
                kind: crate::state::ParkKind::Vault,
                forgotten: false,
            }],
            ..State::default()
        });
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "interrupted_switch").level, Level::Warn);
        assert_eq!(check(&checks, "discarded").level, Level::Warn);
        assert!(healthy(&checks), "neither stops Pitboard working");
    }

    /// A switch the next change cannot finish is said in the words that change would be
    /// refused with, with the way out, and not as one the next change finishes, which is
    /// what the check said of every interrupted switch.
    #[test]
    fn a_switch_the_next_change_cannot_finish_says_why_and_the_way_out() {
        let mut f = facts();
        f.interrupted = true;
        f.stuck = Some("an earlier switch from `work` to `personal` was interrupted".into());
        let checks = evaluate(&f);
        let said = check(&checks, "interrupted_switch");
        assert_eq!(said.level, Level::Warn);
        assert_eq!(
            said.detail,
            "an earlier switch from `work` to `personal` was interrupted"
        );
        assert!(
            said.advice.contains("`pitboard abandon`"),
            "{}",
            said.advice
        );
        assert_eq!(
            checks
                .iter()
                .filter(|c| c.code == "interrupted_switch")
                .count(),
            1,
            "said once"
        );
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

    /// Whether an account is the one signed in is its own tool's question: Claude Code's from
    /// what Anthropic last said of its login, which the config naming another does not move,
    /// and Codex's from its login's own claims. Asked of Claude Code's config for every
    /// account, a signed-in Codex account read as one with nothing parked to switch to, and
    /// the advice was to sign in again.
    #[test]
    fn an_account_is_active_by_its_own_tools_record() {
        use crate::host::memory::MemoryHost;

        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_codex_home(root.join("codex").to_string_lossy().into())
            .with_memory_stores(MemoryHost::new());
        // Codex does not start on Windows where `CODEX_HOME` names no folder.
        std::fs::create_dir_all(root.join("codex")).expect("a Codex home");
        std::fs::write(
            root.join(".claude.json"),
            json!({"oauthAccount": {
                "accountUuid": "beta-uuid",
                "emailAddress": "b@example.com",
                "organizationUuid": "org",
            }})
            .to_string(),
        )
        .expect("a Claude Code config");

        let account =
            |label: &str, uuid: &str, detail: crate::state::Detail| crate::state::Account {
                label: label.into(),
                id: uuid.into(),
                account_uuid: uuid.into(),
                email: format!("{label}@example.com"),
                parked: None,
                last_used_at: None,
                replaced_at: None,
                plan: None,
                detail,
            };
        let claude = || crate::state::Detail::Claude {
            organization_uuid: "org".into(),
            oauth_account: json!({}),
        };
        let codex = || crate::state::Detail::Codex { workspace_id: None };
        let mut state = State {
            accounts: vec![
                account("alpha", "alpha-uuid", claude()),
                account("beta", "beta-uuid", claude()),
                account("work", "work-acc", codex()),
                account("home", "home-acc", codex()),
            ],
            ..State::default()
        };
        let alpha = crate::in_use::InUse::of(&state.accounts[0], "alpha-refresh", NOW);
        state.identified(ProviderId::Claude, alpha, NOW);
        let home = crate::in_use::InUse::of(&state.accounts[3], "home-refresh", NOW);
        state.identified(ProviderId::Codex, home, NOW);
        let active = |facts: &[ParkFact]| -> Vec<String> {
            facts
                .iter()
                .filter(|p| p.active)
                .map(ParkFact::typed)
                .collect()
        };

        // Nothing signed in to Codex: nobody, whatever Pitboard last recorded.
        assert_eq!(active(&park_facts(&ctx, &state)), ["alpha"]);

        // A Codex login that cannot be read: Pitboard's record of whose it was stands in.
        let live = crate::provider::document(ProviderId::Codex)
            .live(&ctx)
            .expect("Codex keeps its login in a file here");
        store::write_raw(
            &live.chain,
            Permit::for_a_test(),
            &live.service,
            "{\"auth_mo",
        )
        .expect("half a login");
        assert_eq!(active(&park_facts(&ctx, &state)), ["alpha", "codex/home"]);

        // Codex's login names `work`, whatever Pitboard last recorded.
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
        store::write_raw(
            &live.chain,
            Permit::for_a_test(),
            &live.service,
            &login.to_string(),
        )
        .expect("a login");
        assert_eq!(active(&park_facts(&ctx, &state)), ["alpha", "codex/work"]);
    }

    /// The accounts `facts` has in use, by the name a command takes.
    fn in_use(facts: &Facts) -> Vec<String> {
        facts
            .parks
            .iter()
            .filter(|p| p.active)
            .map(ParkFact::typed)
            .collect()
    }

    /// The app says a moment on the person's own clock, which the command line's 24 hours
    /// are not, so a front end can write each moment a check names.
    #[test]
    fn each_moment_a_check_names_is_written_as_the_front_end_writes_it() {
        use crate::switch::harness::{NOW, machine};
        let m = machine("doctor-in-use-moment");
        let diagnosis = crate::service::Pitboard::new(m.ctx.clone())
            .doctor_with(&|at, now| format!("at {at} of {now}"));

        assert_eq!(
            check(&diagnosis.checks, "in_use").detail,
            format!("`here`, as Anthropic said at {NOW} of {NOW}")
        );
    }

    /// The account in use is the one Anthropic last named for the login stored, known by
    /// that login's fingerprint, and doctor says when it was named. Claude Code's config
    /// naming another, once a read has found the login is still the one named, is said, and
    /// moves nothing.
    #[test]
    fn the_account_in_use_is_the_one_anthropic_named() {
        use crate::switch::harness::{NOW, config_names, machine};
        let m = machine("doctor-in-use");
        let at = time::moment(NOW, NOW);

        let facts = gather(&m.ctx);
        let checks = evaluate(&facts);
        let said = check(&checks, "in_use");
        assert_eq!(said.level, Level::Ok);
        assert_eq!(said.detail, format!("`here`, as Anthropic said {at}"));
        assert_eq!(in_use(&facts), ["here"]);

        config_names(&m, "there");
        crate::service::Pitboard::new(m.ctx.clone())
            .status(false)
            .expect("a read");
        let facts = gather(&m.ctx);
        let checks = evaluate(&facts);
        let said = check(&checks, "in_use");
        assert_eq!(said.level, Level::Warn);
        assert_eq!(
            said.detail,
            format!("`here`, as Anthropic said {at}, and Claude Code's config names `there`")
        );
        assert_eq!(
            said.advice,
            "`/status` in Claude Code shows `there`. `pitboard use here` writes `here` into the \
             config."
        );
        assert_eq!(in_use(&facts), ["here"]);
    }

    /// Claude Code renews its login by itself, and a renewed login has another fingerprint.
    /// That is what happens every few hours, so it is no warning: the next read asks whose it
    /// is. Doctor asks nobody.
    #[test]
    fn a_renewed_login_is_not_a_warning() {
        use crate::switch::harness::{NOW, document, machine};
        let m = machine("doctor-renewed");
        m.sign_in(&document("here-renewed"));

        let checks = evaluate(&gather(&m.ctx));
        let said = check(&checks, "in_use");

        assert_eq!(said.level, Level::Ok);
        assert_eq!(
            said.detail,
            format!(
                "`here`, as Anthropic said {} of a login renewed or replaced since; the next \
                 `pitboard status` asks whose it is",
                time::moment(NOW, NOW)
            )
        );
        assert_eq!(m.api.calls(), 0);
    }

    /// Claude Code's config has named another account since Anthropic last named the login
    /// stored, as a sign-in leaves it and another Claude Code process starting on another
    /// login can. Something may have signed in, and only Anthropic can say whose the login is
    /// now.
    #[test]
    fn a_config_that_moved_since_is_a_warning() {
        use crate::switch::harness::{NOW, config_names, machine};
        let m = machine("doctor-config-moved");
        config_names(&m, "there");

        let facts = gather(&m.ctx);
        let checks = evaluate(&facts);
        let said = check(&checks, "in_use");

        assert_eq!(said.level, Level::Warn);
        assert_eq!(
            said.detail,
            format!(
                "Claude Code's config has named `there` since Anthropic named `here` {}, so \
                 another login may be stored",
                time::moment(NOW, NOW)
            )
        );
        assert_eq!(
            said.advice,
            "Run `pitboard status`, which asks Anthropic whose login Claude Code has stored."
        );
        assert_eq!(in_use(&facts), ["here"]);
        assert_eq!(m.api.calls(), 0);
    }

    // A machine without Codex reads exactly as it did before Pitboard knew Codex existed; one
    // with it gets a section of its own, and nothing of Claude Code's moves.
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

        // Accounts enrolled on a machine whose Codex home has gone still get the section, and
        // a switch puts a parked one in use while nothing is signed in.
        f.codex = CodexFacts {
            enrolled: 1,
            ..no_codex()
        };
        let checks = evaluate(&f);
        let file = check(&checks, "codex_auth_file");
        assert_eq!(file.level, Level::Warn);
        assert!(file.advice.contains("`codex login`"), "{}", file.advice);
        assert!(
            file.advice.contains("`pitboard use codex/<label>`"),
            "{}",
            file.advice
        );

        // Windows PowerShell 5.1 refuses the `codex.ps1` beside npm's `codex.cmd`.
        f.os = Os::Windows;
        f.codex.program = Some(PathBuf::from("C:/Users/x/AppData/Roaming/npm/codex.cmd"));
        let checks = evaluate(&f);
        let file = check(&checks, "codex_auth_file");
        assert!(file.advice.contains("`codex.cmd login`"), "{}", file.advice);
    }

    // The session's This PC listed no Codex section for a Codex installed with npm and never
    // run, though it lists Claude Code's for a Claude Code in the same state.
    #[test]
    fn codex_installed_and_never_run_has_its_section() {
        let mut f = facts();
        f.codex = CodexFacts {
            program: Some(PathBuf::from(r"C:\Users\x\AppData\Roaming\npm\codex.cmd")),
            ..no_codex()
        };
        let checks = evaluate(&f);
        for code in ["codex_backend", "codex_auth_file", "codex_version"] {
            assert_eq!(check(&checks, code).level, Level::Ok, "{code}");
        }
        assert!(healthy(&checks));
    }

    /// A keychain store is Codex's to use and Pitboard's to leave alone, so it is said
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
                backend_setting: format!(
                    "`cli_auth_credentials_store = \"{backend}\"` in /home/x/.codex/config.toml"
                ),
                backend_remedy: "To use the file store, set `cli_auth_credentials_store = \
                                 \"file\"` in /home/x/.codex/config.toml, then sign in again \
                                 with `codex login`"
                    .into(),
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
                found.detail.contains("/home/x/.codex/config.toml"),
                "says which file set it: {}",
                found.detail
            );
            assert!(
                found.advice.contains(
                    "set `cli_auth_credentials_store = \"file\"` in /home/x/.codex/config.toml"
                ),
                "says the line and where it goes: {}",
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
                "nothing Pitboard does is broken for somebody with no Codex accounts: {store}"
            );
            assert!(found.detail.contains(store), "{}", found.detail);
            assert!(found.detail.contains("file store"), "{}", found.detail);
        }
        let leaves = match crate::host::OS {
            Os::MacOs | Os::Linux => "does not read an item",
            Os::Windows => "does not follow Codex",
        };
        for store in ["keyring", "auto", "secrets"] {
            let found = judged(store, 1);
            let advice = &check(&found, "codex_backend").advice;
            assert!(advice.contains(leaves), "{advice}");
        }
    }

    /// The file store's line says which setting chose it, and that a project's own config is
    /// not read, since in a trusted project that sets the store Codex keeps its login
    /// somewhere else.
    #[test]
    fn the_file_store_says_what_chose_it_and_what_is_not_read() {
        let checks = codex_checks(&with_codex());
        let found = check(&checks, "codex_backend");
        assert_eq!(found.level, Level::Ok);
        assert_eq!(
            found.detail,
            "file (Codex's default)  ·  /home/x/.codex/auth.json"
        );
        assert!(
            found
                .advice
                .contains("does not read a project's own `.codex/config.toml`"),
            "{}",
            found.advice
        );
    }

    /// A store nobody can tell stops Codex itself, so it is broken rather than a choice: a
    /// failure where Codex accounts are enrolled, and a warning where none are.
    #[test]
    fn a_codex_store_nobody_can_tell_is_broken() {
        let unknown = |enrolled| CodexFacts {
            backend: "unknown",
            backend_setting: "/etc/codex/config.toml is not TOML Codex can read, at line 3".into(),
            backend_remedy: "Codex 0.160.0 does not start until that is put right".into(),
            enrolled,
            ..with_codex()
        };
        for (enrolled, level) in [(1, Level::Fail), (0, Level::Warn)] {
            let checks = codex_checks(&unknown(enrolled));
            let found = check(&checks, "codex_backend");
            assert_eq!(found.level, level);
            assert_eq!(
                found.detail,
                "cannot tell: /etc/codex/config.toml is not TOML Codex can read, at line 3"
            );
            assert_eq!(
                found.advice,
                "Codex 0.160.0 does not start until that is put right. Pitboard parks and \
                 switches no Codex login while it cannot tell where Codex keeps it."
            );
            assert!(checks.iter().all(|c| c.code != "codex_login"));
        }
    }

    // Codex 0.164.0 signs out of a login from a workspace its configuration leaves out
    // (`codex_login_restrictions`), so the setting is named with the accounts it rules out.
    #[test]
    fn codexs_login_restrictions_are_named_with_the_accounts_they_rule_out() {
        use crate::switch::harness::codex_account;
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        let mut state = State::default();
        state.accounts.push(codex_account("here", "here", None));
        state.accounts.push(codex_account("there", "there", None));
        let judged = || codex_checks(&codex_facts(&ctx, Some(&state)));
        assert!(
            judged()
                .iter()
                .all(|c| c.code != "codex_login_restrictions")
        );

        let config = crate::provider::codex::layers::system_file(&ctx, "config.toml");
        host.administers(config.clone(), "forced_chatgpt_workspace_id = \"here\"\n");
        let checks = judged();
        let found = check(&checks, "codex_login_restrictions");
        assert_eq!(found.level, Level::Warn);
        assert_eq!(
            found.detail,
            format!(
                "`forced_chatgpt_workspace_id` set to `here` in {}",
                config.display()
            )
        );
        assert_eq!(
            found.advice,
            "Codex signs out of or does not use the logins of `codex/there`, so Pitboard does \
             not put them in use."
        );

        host.administers(
            config.clone(),
            "forced_chatgpt_workspace_id = [\"here\", \"there\"]\n",
        );
        assert_eq!(
            check(&judged(), "codex_login_restrictions").level,
            Level::Ok
        );

        host.administers(config, "forced_login_method = \"sso\"\n");
        let checks = judged();
        let found = check(&checks, "codex_login_restrictions");
        assert_eq!(found.level, Level::Fail, "Codex accounts are enrolled");
        assert!(
            found.advice.starts_with("Codex does not start"),
            "{}",
            found.advice
        );
    }

    /// What this machine's folder of Codex's configuration and the person's own config say is
    /// gathered as Codex reads it, and a store a requirement pins is said to be one no line of
    /// the person's own changes. On Windows a keyring store is the secret store by default.
    #[test]
    fn a_store_an_administrator_pinned_is_gathered_and_said() {
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        let required = crate::provider::codex::layers::system_file(&ctx, "requirements.toml");
        let named = required.display().to_string();
        host.administers(
            required.clone(),
            "cli_auth_credentials_store = \"keyring\"\n",
        );
        let codex = CodexFacts {
            enrolled: 1,
            ..codex_facts(&ctx, None)
        };
        let (backend, setting) = match crate::host::OS {
            Os::MacOs | Os::Linux => ("keyring", format!("pinned to `keyring` by {named}")),
            Os::Windows => (
                "secrets",
                format!("pinned to `keyring` by {named}, with `secret_auth_storage` on by default"),
            ),
        };
        assert_eq!(codex.backend, backend);
        assert_eq!(codex.backend_setting, setting);
        let checks = codex_checks(&codex);
        let found = check(&checks, "codex_backend");
        assert_eq!(found.level, Level::Fail);
        assert_eq!(found.detail, format!("{backend} ({setting})"));
        assert!(
            found.advice.ends_with(&format!(
                "No line in your own config.toml can change it: {named} pins it, and only an \
                 administrator can change that."
            )),
            "{}",
            found.advice
        );

        host.administers(required, "x = \n");
        let unknown = codex_facts(&ctx, None);
        assert_eq!(unknown.backend, "unknown");
        assert_eq!(
            unknown.backend_setting,
            format!("{named} is not TOML Codex can read, at line 1")
        );
        assert_eq!(
            unknown.backend_remedy,
            "Codex 0.160.0 does not start until that is put right"
        );
        assert!(matches!(unknown.login, Ok(None)), "nothing is read");
    }

    #[test]
    fn a_store_codex_uses_only_when_told_to_is_said_in_each_systems_words() {
        let keyring = CodexFacts {
            backend: "keyring",
            backend_setting: "`cli_auth_credentials_store = \"keyring\"` in config.toml".into(),
            backend_remedy: "To use the file store, set it".into(),
            enrolled: 1,
            ..with_codex()
        };
        let advice = |os| {
            check(
                &judge_codex(os, None, &keyring, &[], NOW, &time::moment),
                "codex_backend",
            )
            .advice
            .clone()
        };
        for os in [Os::MacOs, Os::Linux] {
            assert_eq!(
                advice(os),
                "Pitboard handles Codex's file store, auth.json, and does not read an item \
                 Codex created for itself, because every read would ask you for permission, so \
                 no enrolled Codex account can be switched to. To use the file store, set it."
            );
        }
        assert_eq!(
            advice(Os::Windows),
            "Pitboard handles Codex's file store, auth.json, and does not follow Codex into a \
             store it uses only when told to, so no enrolled Codex account can be switched to. \
             To use the file store, set it."
        );
    }

    // By name alone, so no item's hex or secret is said, and only while Codex keeps the file.
    #[test]
    fn codexs_items_in_credential_manager_are_a_note_counted_by_name() {
        let host = crate::host::memory::MemoryHost::new();
        let home = tempfile::tempdir().expect("a Codex home, which Windows' Codex needs");
        let ctx = Context::for_unit_test()
            .with_codex_home(home.path().to_string_lossy().into_owned())
            .with_memory_stores(host.clone());
        host.keeps_credentials(&[
            "CLI|0123456789abcdef.Codex Auth",
            "secrets|0123456789abcdef.codex",
            "secrets|0123456789abcdef|gateway-oauth.codex",
            "cli|0123456789abcdef.Codex MCP Credentials",
            "Claude Code-credentials",
            "GitHub - https://github.com",
        ]);
        let facts = codex_facts(&ctx, None);
        assert_eq!(facts.stored_elsewhere, (1, 2));
        let checks = codex_checks(&facts);
        let note = check(&checks, "codex_credential_manager");
        assert_eq!(note.level, Level::Ok);
        assert_eq!(note.detail, "1 `cli|` item and 2 `secrets|` items");
        assert!(note.advice.ends_with("Pitboard switches only auth.json."));
        assert!(!format!("{}{}", note.detail, note.advice).contains("0123456789abcdef"));

        std::fs::write(
            home.path().join("config.toml"),
            "cli_auth_credentials_store = \"ephemeral\"\n",
        )
        .expect("a config");
        assert_eq!(codex_facts(&ctx, None).stored_elsewhere, (0, 0));
        assert!(
            codex_checks(&with_codex())
                .iter()
                .all(|c| c.code != "codex_credential_manager")
        );
    }

    // Windows' alone: Codex does not start there where `CODEX_HOME` names no folder.
    #[test]
    fn a_codex_home_that_names_no_folder_is_said_as_it_is() {
        for (enrolled, level) in [(1, Level::Fail), (0, Level::Warn)] {
            let facts = CodexFacts {
                present: false,
                auth_access: None,
                login: Err(CodexLoginTrouble::NoHome(PathBuf::from(r"C:\nowhere"))),
                enrolled,
                ..with_codex()
            };
            let checks = codex_checks(&facts);
            let found = check(&checks, "codex_login");
            assert_eq!(
                (found.level, found.detail.as_str()),
                (
                    level,
                    r"CODEX_HOME is C:\nowhere, which is no folder, so Codex does not start"
                )
            );
            assert_eq!(found.advice, "Make that folder, or unset CODEX_HOME.");
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

    // Windows' facts are read from builds of their own, which its version lines name.
    #[test]
    fn a_build_is_set_beside_the_one_read_for_this_system() {
        use crate::assumptions::{Platform, verified_against};
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            let claude = verified_against(ProviderId::Claude, Platform::of(os));
            let mut f = facts();
            f.os = os;
            f.claude_version = None;
            assert_eq!(
                check(&evaluate(&f), "claude_version").detail,
                format!("not found here; Pitboard's facts were read from {claude}"),
                "{os:?}"
            );
            f.claude_version = Some(claude.into());
            assert_eq!(
                check(&evaluate(&f), "claude_version").detail,
                format!("{claude}, which is what Pitboard's facts were read from"),
                "{os:?}"
            );

            let codex = verified_against(ProviderId::Codex, Platform::of(os));
            let installed = CodexFacts {
                version: Some(codex.into()),
                ..with_codex()
            };
            let checks = judge_codex(os, None, &installed, &[], NOW, &time::moment);
            assert_eq!(
                check(&checks, "codex_version").detail,
                format!("{codex}, which is what Pitboard's facts were read from"),
                "{os:?}"
            );
        }
    }

    /// Each way Codex is installed, laid out in a scratch directory, and a shim that names
    /// nothing. Only the two directories above the program are looked at, names first, so a
    /// standalone install is named without opening any file inside it.
    #[test]
    fn a_version_is_read_out_of_each_way_codex_is_installed() {
        use crate::host::fs::testing;

        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
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
        testing::link(&standalone, &link).expect("a link");
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

    // `codex.exe` has no version resource in 0.162.1.
    #[test]
    fn a_version_is_read_where_codex_is_installed_on_windows() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let place = |at: &str| {
            let path = root.join(at);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
            path
        };
        let layout = |version: &str, variant: &str| {
            format!(
                r#"{{"layoutVersion": 1, "version": "{version}", "target": "x86_64-pc-windows-msvc", "variant": "{variant}", "entrypoint": "bin/codex.exe"}}"#
            )
        };

        let vendored = place("codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe");
        let package = root.join("codex-win32-x64/vendor/x86_64-pc-windows-msvc/codex-package.json");
        std::fs::write(&package, layout("0.162.1", "codex")).unwrap();
        assert_eq!(codex_version(&vendored).as_deref(), Some("0.162.1"));
        std::fs::write(&package, layout("0.162.1", "something-else")).unwrap();
        assert_eq!(codex_version(&vendored), None, "only Codex's own package");

        place("npm/node_modules/@openai/codex/bin/codex.js");
        std::fs::write(
            root.join("npm/node_modules/@openai/codex/package.json"),
            r#"{"name": "@openai/codex", "version": "0.161.0"}"#,
        )
        .unwrap();
        let shim = root.join("npm/codex.cmd");
        std::fs::write(
            &shim,
            "@ECHO off\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n)\r\n\
             \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n",
        )
        .unwrap();
        assert_eq!(codex_version(&shim).as_deref(), Some("0.161.0"));
    }

    #[test]
    fn only_macos_is_asked_for_claude_desktops_build() {
        let memory = crate::host::memory::MemoryHost::new();
        memory.app_at(std::path::Path::new("/Applications/Claude.app"), "2.7032.0");
        for (os, build) in [
            (Os::MacOs, Some("2.7032.0")),
            (Os::Linux, None),
            (Os::Windows, None),
        ] {
            let env = crate::context::Environment::on(
                os,
                [("HOME", "/nowhere"), ("USERPROFILE", "/nowhere")],
            );
            let ctx = Context::for_command_line(&env).with_memory_stores(memory.clone());
            assert_eq!(desktop_build(&ctx).as_deref(), build, "{os:?}");
        }
    }

    #[test]
    fn a_codex_login_anybody_can_read_or_nobody_can_parse_is_said() {
        let mut codex = with_codex();
        codex.auth_access = Some(mode(0o644));
        let checks = judge_codex(Os::Linux, None, &codex, &[], NOW, &time::moment);
        let file = check(&checks, "codex_auth_file");
        assert_eq!(file.level, Level::Warn);
        assert!(file.detail.contains("mode 644"), "{}", file.detail);
        assert!(file.advice.contains("chmod 600"), "{}", file.advice);

        codex.auth_file = PathBuf::from(r"C:\Users\dana\.codex\auth.json");
        codex.auth_access = Some(open_to_users());
        let checks = judge_codex(Os::Windows, Some(SID), &codex, &[], NOW, &time::moment);
        let file = check(&checks, "codex_auth_file");
        assert_eq!(file.level, Level::Warn);
        let [reset, grant] =
            crate::host::acl::icacls(false, r"C:\Users\dana\.codex\auth.json", SID);
        assert_eq!(
            file.advice,
            format!(
                "It holds a usable login in plain text, and Codex gives it no access list of \
                 its own, so it takes its folder's: `{reset}` and `{grant}`."
            )
        );

        codex.auth_access = Some(mode(0o600));
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

    // Said, never warned about: running it is the point of having it.
    #[test]
    fn running_codex_processes_are_reported_as_a_fact() {
        use crate::holder::classify;
        use crate::host::Process;
        let at = |pid: u32, path: &str| Process {
            pid,
            path: PathBuf::from(path),
            package: None,
        };
        let holders = crate::provider::codex::holders::HOLDERS;
        let mut codex = with_codex();
        codex.running = Some(classify(
            &[at(4321, "codex"), at(99, "codex")],
            holders,
            "codex",
        ));
        let checks = codex_checks(&codex);
        let running = check(&checks, "codex_running");
        assert_eq!(running.level, Level::Ok);
        assert!(
            running
                .detail
                .starts_with("2 `codex` sessions (pid 4321, 99)"),
            "{}",
            running.detail
        );
        assert!(
            running.detail.contains("started again"),
            "{}",
            running.detail
        );
        assert_eq!(
            running.advice,
            "Quit them and start again to take a switch."
        );

        let many: Vec<Process> = (1..=23).map(|pid| at(pid, "codex")).collect();
        codex.running = Some(classify(&many, holders, "codex"));
        let detail = check(&codex_checks(&codex), "codex_running").detail.clone();
        assert!(detail.starts_with("23 `codex` sessions"), "{detail}");
        assert!(detail.contains("1, 2, 3 and 20 more"), "{detail}");

        codex.running = Some(classify(
            &[at(
                7,
                "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/\
                 Contents/MacOS/codex",
            )],
            holders,
            "codex",
        ));
        let checks = codex_checks(&codex);
        let app = check(&checks, "codex_running");
        assert!(
            app.detail.starts_with("the ChatGPT app (pid 7)"),
            "{}",
            app.detail
        );
        assert_eq!(
            app.advice,
            "Quit ChatGPT with Command-Q and open it again to take a switch."
        );

        codex.running = None;
        assert!(
            check(&codex_checks(&codex), "codex_running")
                .detail
                .contains("could not tell")
        );
    }

    /// doctor asks what runs Codex the way a switch does, through the same host, so the two
    /// never disagree about what is still on the account a switch left. They used to count
    /// with two different scans, and only the switch's could be stood in for. Nor do they
    /// disagree where the process list cannot be read: doctor says it could not tell, and a
    /// switch that nobody can say.
    #[test]
    fn doctor_and_a_switch_see_the_same_codex_running() {
        use crate::host::memory::MemoryHost;
        let host = MemoryHost::new();
        host.runs_at(
            "codex",
            &[
                "codex",
                "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/\
                 MacOS/codex",
            ],
        );
        let ctx = Context::new(std::env::temp_dir()).with_memory_stores(Arc::clone(&host));
        let seen = running_codex(&ctx).expect("readable");
        assert_eq!(
            seen.iter().map(|h| h.holder.kind).collect::<Vec<_>>(),
            ["chatgpt_app", "session"]
        );
        assert_eq!(
            switch::StillHolding::These(seen),
            switch::still_holding(&ctx, ProviderId::Codex)
        );

        host.runs_at("codex", &[]);
        assert_eq!(running_codex(&ctx), Some(Vec::new()));
        assert_eq!(
            switch::still_holding(&ctx, ProviderId::Codex),
            switch::StillHolding::Nothing
        );

        host.without_a_process_list();
        assert_eq!(running_codex(&ctx), None);
        assert_eq!(
            switch::still_holding(&ctx, ProviderId::Codex),
            switch::StillHolding::Unknown
        );
    }

    #[test]
    fn every_codex_failure_and_warning_tells_the_user_something() {
        for backend in ["file", "keyring", "auto", "ephemeral", "secrets", "unknown"] {
            let codex = CodexFacts {
                backend,
                enrolled: 1,
                auth_access: Some(mode(0o666)),
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
        use crate::host::fs::testing;

        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let home = root.join("codex");
        // A `codex` of this test's own. Looked up on `PATH`, it would be this machine's, and
        // the standalone installer keeps that inside the real `~/.codex`.
        let codex = format!("codex{}", std::env::consts::EXE_SUFFIX);
        let program = root.join("bin").join(&codex);
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_codex_home(home.to_string_lossy().into())
            .with_codex_program(program.clone());

        let absent = codex_facts(&ctx, None);
        assert!(!absent.present);
        assert_eq!(absent.backend, "file");
        assert_eq!(absent.auth_access, None);
        match crate::host::OS {
            Os::MacOs | Os::Linux => assert!(matches!(absent.login, Ok(None))),
            Os::Windows => assert!(
                matches!(&absent.login, Err(CodexLoginTrouble::NoHome(path)) if *path == home),
                "Codex does not start there"
            ),
        }
        assert_eq!(absent.program, None, "the one named, and it is not there");
        assert_eq!(absent.version, None);

        let installed = root
            .join("releases/0.154.0-aarch64-apple-darwin/bin")
            .join(&codex);
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::write(&installed, "").unwrap();
        // A program is what can be run, as the installer leaves it.
        testing::make_runnable(&installed).expect("runnable");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        testing::link(&installed, &program).expect("a link");
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
        testing::open_to_others(&auth).expect("opened to others");
        let found = codex_facts(&ctx, None);
        assert!(found.present);
        assert!(found.auth_access.expect("there").shared);
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
            "a login mixing two accounts is one Pitboard cannot use"
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
            "a store Pitboard does not handle is not read"
        );

        // A keychain store with the encrypted-file feature is named as what it is, never as
        // plain `keyring`, which the configuration does not say.
        std::fs::write(
            home.join("config.toml"),
            "cli_auth_credentials_store = \"auto\"\n[features]\nsecret_auth_storage = true\n",
        )
        .expect("a config");
        let secrets = codex_facts(&ctx, None);
        assert_eq!(secrets.backend, "secrets");
        let config = crate::provider::codex::paths::config_file(&ctx);
        assert_eq!(
            secrets.backend_setting,
            format!(
                "`cli_auth_credentials_store = \"auto\"` in {config}, with \
                 `secret_auth_storage` in {config}",
                config = config.display()
            )
        );
        assert_eq!(
            secrets.backend_remedy,
            format!(
                "To use the file store, set `cli_auth_credentials_store = \"file\"` in \
                 {}, then sign in again with `codex login`",
                config.display()
            )
        );
    }

    #[test]
    fn a_version_is_read_out_of_the_path_codex_is_installed_at() {
        assert!(looks_like_a_version("0.154.0"));
        assert!(!looks_like_a_version("releases"));
        assert!(!looks_like_a_version("0.154"));
        assert!(!looks_like_a_version("v0.154.0"));
    }

    // Claude Code 2.1.301 runs it in Git Bash where it finds one, else in PowerShell.
    #[test]
    fn windows_is_told_which_shell_runs_the_status_line_and_whether_it_finds_pitboard() {
        use std::cell::Cell;
        let mut facts = facts();
        facts.os = Os::Windows;
        assert!(evaluate(&facts).iter().all(|c| c.code != "status_line"));
        let bash = PathBuf::from(r"C:\Program Files\Git\bin\bash.exe");
        let pitboard = PathBuf::from(r"C:\Users\x\.cargo\bin\pitboard.exe");
        let named = PathBuf::from(r"D:\tools\git.exe");
        let runs_pitboard = Cell::new(true);
        let five = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        let powershell = std::cell::RefCell::new(Some(claude::PowerShell::Windows(five.clone())));
        let mut said = |found: Option<&PathBuf>, passed_over: Option<&PathBuf>, on_path: bool| {
            facts.status_line = Some(StatusLineFact {
                git_bash: claude::GitBash {
                    found: found.cloned(),
                    passed_over: passed_over.cloned(),
                },
                powershell: powershell.borrow().clone(),
                pitboard: on_path.then(|| pitboard.clone()),
                runs_pitboard: runs_pitboard.get(),
            });
            let checks = evaluate(&facts);
            let line = check(&checks, "status_line");
            (line.level, line.detail.clone(), line.advice.clone())
        };
        assert_eq!(
            said(Some(&bash), None, true),
            (
                Level::Ok,
                format!(
                    "Claude Code runs the status line in Git Bash, {}; `pitboard` there is {}",
                    bash.display(),
                    pitboard.display()
                ),
                String::new()
            )
        );
        assert_eq!(
            said(None, Some(&named), true),
            (
                Level::Ok,
                format!(
                    "Claude Code runs the status line in Windows PowerShell, {}, finding no Git \
                     Bash; `pitboard` there is {}; it passes over {}, which \
                     CLAUDE_CODE_GIT_BASH_PATH names",
                    five.display(),
                    pitboard.display(),
                    named.display()
                ),
                String::new()
            )
        );
        let seven = PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe");
        *powershell.borrow_mut() = Some(claude::PowerShell::Seven(seven.clone()));
        assert_eq!(
            said(None, None, true).1,
            format!(
                "Claude Code runs the status line in PowerShell 7, {}, finding no Git Bash; \
                 `pitboard` there is {}",
                seven.display(),
                pitboard.display()
            )
        );
        *powershell.borrow_mut() = None;
        assert_eq!(
            said(None, None, true).1,
            format!(
                "Claude Code finds neither Git Bash nor PowerShell to run the status line in; \
                 `pitboard` there is {}",
                pitboard.display()
            )
        );
        *powershell.borrow_mut() = Some(claude::PowerShell::Windows(five.clone()));
        assert_eq!(
            said(Some(&bash), None, false),
            (
                Level::Warn,
                format!(
                    "Claude Code runs the status line in Git Bash, {}; no `pitboard` is on its \
                     PATH",
                    bash.display()
                ),
                "Add the folder pitboard.exe is in to your PATH, then start Claude Code again."
                    .to_string()
            )
        );
        runs_pitboard.set(false);
        assert_eq!(
            said(Some(&bash), None, false).0,
            Level::Ok,
            "nothing it runs needs one"
        );
        facts.claude_present = false;
        facts.codex.present = true;
        assert!(
            evaluate(&facts).iter().all(|c| c.code != "status_line"),
            "a machine with no Claude Code is not told"
        );
    }

    #[test]
    fn only_a_bare_pitboard_needs_one_on_the_path() {
        for command in [
            "pitboard statusline",
            "  PITBOARD.EXE statusline --offline",
            "\"pitboard\" statusline",
            "pitboard",
        ] {
            assert!(runs_bare_pitboard(command), "{command}");
        }
        for command in [
            r"C:\Tools\pitboard.exe statusline",
            "~/bin/status.sh",
            "npx ccstatusline",
            "",
        ] {
            assert!(!runs_bare_pitboard(command), "{command}");
        }
    }

    // L5 and H9 from Codex 0.163.0's and Claude Code 2.1.301's builds, read as bytes.
    #[test]
    fn windows_is_told_what_its_settings_change_for_pitboard_and_the_tools() {
        let said = |facts: &Facts| -> Vec<(&'static str, Level, String, String)> {
            evaluate(facts)
                .into_iter()
                .filter(|c| ["wsl", "pitboard_copies", "system_proxy"].contains(&c.code))
                .map(|c| (c.code, c.level, c.detail, c.advice))
                .collect()
        };
        let copy = |folder: &str| PathBuf::from(format!(r"C:\{folder}\pitboard.exe"));
        let mut facts = facts();
        facts.os = Os::Windows;
        facts.windows_settings = Some(crate::host::WindowsSettings::default());
        facts.pitboards = vec![copy("a")];
        assert!(said(&facts).is_empty());

        facts.windows_settings = Some(crate::host::WindowsSettings {
            wsl: vec!["Ubuntu".into(), "Debian".into()],
            proxy: Some("proxy.example.com:8080".into()),
            admin_approval: Some(true),
        });
        facts.pitboards = vec![copy("a"), copy("b"), copy("c")];
        facts.codex.home = PathBuf::from(r"C:\Users\x\.codex");
        assert_eq!(
            said(&facts),
            [
                (
                    "wsl",
                    Level::Ok,
                    "WSL holds Ubuntu and Debian".to_string(),
                    "Claude Code and Codex installed there are installs of their own, which \
                     Pitboard on Windows does not switch. Claude Code there keeps its own login. \
                     Where Codex there uses C:\\Users\\x\\.codex, Codex's home here, a switch here \
                     changes its login too. WSL 2 runs them in a virtual machine, so Windows' list \
                     of processes does not show them running."
                        .to_string()
                ),
                (
                    "pitboard_copies",
                    Level::Ok,
                    r"C:\a\pitboard.exe on PATH, then C:\b\pitboard.exe and C:\c\pitboard.exe"
                        .to_string(),
                    "A shell runs the first, so updating another changes nothing there. \
                     Uninstall the copies you do not use."
                        .to_string()
                ),
                (
                    "system_proxy",
                    Level::Ok,
                    "Windows' proxy settings name proxy.example.com:8080, and HTTPS_PROXY is not \
                     set"
                    .to_string(),
                    "Codex sends its requests through it. Claude Code and Pitboard follow only \
                     the proxy variables, so theirs go out directly: set HTTPS_PROXY where this \
                     network needs the proxy."
                        .to_string()
                ),
            ]
        );

        let advice = |facts: &Facts| check(&evaluate(facts), "system_proxy").advice.clone();
        let named_by = |variable: &'static str| {
            by(ProxyFact {
                variable,
                ..through(None, &[])
            })
        };
        facts.network = named_by("HTTP_PROXY");
        assert_eq!(
            advice(&facts),
            "Codex sends its requests through it. Claude Code's go through the one HTTP_PROXY \
             names, and Pitboard's through the one HTTP_PROXY names: set HTTPS_PROXY where this \
             network needs the proxy."
        );
        facts.network = named_by("ALL_PROXY");
        assert_eq!(
            advice(&facts),
            "Codex sends its requests through it. Claude Code's go out directly, and Pitboard's \
             through the one ALL_PROXY names: set HTTPS_PROXY where this network needs the proxy."
        );
        facts.network = direct();
        let with_server = |facts: &mut Facts, server: &str| {
            if let Some(settings) = facts.windows_settings.as_mut() {
                settings.proxy = Some(server.into());
            }
        };
        with_server(
            &mut facts,
            "http=web.example.com:80;https=secure.example.com:443",
        );
        assert_eq!(
            advice(&facts),
            "Codex reads the setting as one address, which a proxy for each protocol is not, so \
             its requests go out directly. Claude Code and Pitboard follow only the proxy \
             variables, so theirs go out directly: set HTTPS_PROXY where this network needs the \
             proxy."
        );
        with_server(&mut facts, "https=secure.example.com:443");
        assert_eq!(
            advice(&facts),
            "Codex reads the setting as one address, so it sends its requests to a proxy named \
             https=secure.example.com:443, and they fail. Claude Code and Pitboard follow only \
             the proxy variables, so theirs go out directly: set HTTPS_PROXY where this network \
             needs the proxy."
        );
        with_server(&mut facts, "proxy.example.com:8080");

        let codes = |facts: &Facts| {
            said(facts)
                .into_iter()
                .map(|(code, ..)| code)
                .collect::<Vec<_>>()
        };
        facts.network = by(through(None, &[]));
        assert_eq!(
            codes(&facts),
            ["wsl", "pitboard_copies"],
            "a variable names one"
        );
        facts.network = NetworkFact {
            proxy: None,
            passed_over: vec!["HTTPS_PROXY"],
            set: vec!["HTTPS_PROXY"],
        };
        assert_eq!(
            codes(&facts),
            ["wsl", "pitboard_copies"],
            "a variable is set"
        );
        facts.windows_settings = None;
        assert_eq!(codes(&facts), ["pitboard_copies"]);
    }

    #[test]
    fn windows_settings_are_read_from_the_host_on_windows_alone() {
        let host = crate::host::memory::MemoryHost::new();
        let settings = crate::host::WindowsSettings {
            wsl: vec!["Ubuntu".into()],
            proxy: Some("proxy.example.com:8080".into()),
            admin_approval: Some(false),
        };
        host.sets_windows(settings.clone());
        let ctx = Context::for_unit_test().with_memory_stores(host);
        assert_eq!(
            gather(&ctx).windows_settings,
            (crate::host::OS == Os::Windows).then_some(settings)
        );
    }

    // RF-F3: a bare name runs the first a shell finds.
    #[test]
    fn every_pitboard_a_windows_shell_finds_is_one_copy_first_first() {
        let root = tempfile::tempdir().expect("a scratch folder");
        let (a, b) = (root.path().join("a"), root.path().join("b"));
        for folder in [&a, &b] {
            std::fs::create_dir_all(folder).expect("a folder");
            let program = folder.join("pitboard.exe");
            std::fs::write(&program, b"MZ").expect("a program");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
                    .expect("runnable");
            }
        }
        let empty = root.path().join("empty");
        std::fs::create_dir_all(&empty).expect("a folder");
        // As winget's Links entry leads to its package's copy.
        let link = root.path().join("link");
        std::fs::create_dir_all(&link).expect("a folder");
        #[cfg(unix)]
        std::os::unix::fs::symlink(a.join("pitboard.exe"), link.join("pitboard.exe"))
            .expect("a link");
        let path = std::env::join_paths([&a, &empty, &link, &b, &a]).expect("a PATH");
        let ctx = Context::for_unit_test().with_search_path(path.to_string_lossy().into_owned());
        assert_eq!(
            pitboards(&ctx),
            [a.join("pitboard.exe"), b.join("pitboard.exe")]
        );
    }

    #[test]
    fn windows_proxy_setting_is_redacted_from_a_report() {
        let ctx = Context::new(PathBuf::from("/home/x"));
        let mut f = facts();
        f.os = Os::Windows;
        for (server, hidden) in [
            ("proxy.example.com:8080", &["proxy.example.com:8080"][..]),
            (
                "http=web.example.com:80;https=secure.example.com:443",
                &["web.example.com:80", "secure.example.com:443"][..],
            ),
            (
                "http=alice:p=ss@proxy.example.com:8080",
                &["proxy.example.com:8080"][..],
            ),
        ] {
            f.windows_settings = Some(crate::host::WindowsSettings {
                proxy: Some(server.into()),
                ..crate::host::WindowsSettings::default()
            });
            let detail = check(&evaluate(&f), "system_proxy").detail.clone();
            let shown = redaction_for(&ctx, &f).over(&detail);
            for part in hidden {
                assert!(detail.contains(part), "{detail}");
                assert!(!shown.contains(part), "{shown}");
            }
            assert!(shown.contains("<proxy "), "{shown}");
        }
    }

    // Codex reads a user name and a password out of `ProxyServer` as out of a variable.
    #[test]
    fn a_password_in_windows_proxy_setting_is_never_shown() {
        let ctx = Context::new(PathBuf::from("/home/x"));
        let mut f = facts();
        f.os = Os::Windows;
        for (server, shown) in [
            (
                "http=alice:s3cret@proxy.example.com:8080",
                "http=***:***@proxy.example.com:8080",
            ),
            (
                "alice:p=ss@proxy.example.com:8080",
                "***:***@proxy.example.com:8080",
            ),
            (
                "http://alice@proxy.example.com:8080;https=bob:s3cret@secure.example.com:443",
                "http://***@proxy.example.com:8080;https=***:***@secure.example.com:443",
            ),
        ] {
            f.windows_settings = Some(crate::host::WindowsSettings {
                proxy: Some(server.into()),
                ..crate::host::WindowsSettings::default()
            });
            let checks = evaluate(&f);
            let line = check(&checks, "system_proxy");
            assert_eq!(
                line.detail,
                format!("Windows' proxy settings name {shown}, and HTTPS_PROXY is not set")
            );
            for said in [&line.detail, &line.advice] {
                let redacted = redaction_for(&ctx, &f).over(said);
                for secret in ["alice", "bob", "s3cret", "p=ss"] {
                    assert!(!said.contains(secret), "{said}");
                    assert!(!redacted.contains(secret), "{redacted}");
                }
            }
        }
    }

    // RF-A1 read UAC on on both images, so only Microsoft's word says what off does.
    #[test]
    fn windows_says_user_account_control_is_off_where_it_reads_so() {
        let mut f = facts();
        f.os = Os::Windows;
        let advice = |f: &Facts| {
            evaluate(f)
                .into_iter()
                .find(|c| c.code == "elevated")
                .map(|c| c.advice)
        };
        f.elevation = crate::host::Elevation::Elevated {
            why: crate::host::token::IN_EVERY_PROGRAM,
        };
        let before = "Pitboard changes nothing this way: it reads, and renews and writes nothing. \
                      That is so with User Account Control off and in the built-in Administrator \
                      account: run it from a standard account, or turn User Account Control on.";
        for read in [None, Some(true)] {
            f.windows_settings = Some(crate::host::WindowsSettings {
                admin_approval: read,
                ..crate::host::WindowsSettings::default()
            });
            assert_eq!(advice(&f).as_deref(), Some(before), "{read:?}");
        }
        f.windows_settings = Some(crate::host::WindowsSettings {
            admin_approval: Some(false),
            ..crate::host::WindowsSettings::default()
        });
        assert_eq!(
            advice(&f).as_deref(),
            Some(
                "Pitboard changes nothing this way: it reads, and renews and writes nothing. \
                 User Account Control is off on this PC, so every program an administrator \
                 starts is elevated: turn it on, or run Pitboard from a standard account."
            )
        );
        f.elevation = crate::host::Elevation::Elevated {
            why: crate::host::token::AS_ADMINISTRATOR,
        };
        assert_eq!(
            advice(&f).as_deref(),
            Some(
                "Pitboard changes nothing this way: it reads, and renews and writes nothing. Run \
                 it from a terminal that is not elevated (not Run as administrator)."
            )
        );
        f.elevation = crate::host::Elevation::Normal;
        assert_eq!(advice(&f), None);
    }

    // An index brought from a Mac.
    #[test]
    fn a_claude_desktop_account_in_an_index_off_macos_is_a_note() {
        let mut state = State::default();
        state.accounts.push(desktop_account("work", None));
        state
            .accounts
            .push(desktop_account("home", Some(folder_park("home-1", None))));
        let mut f = facts();
        f.desktop = Some(DesktopFacts {
            root: Err("Pitboard switches Claude Desktop accounts on macOS only".into()),
            ..desktop()
        });
        f.state = Ok(state.clone());
        for os in [Os::Windows, Os::Linux] {
            f.os = os;
            let desktops: Vec<(&str, Level, String, String)> = evaluate(&f)
                .into_iter()
                .filter(|c| section(c.code) == Some(ProviderId::Desktop))
                .map(|c| (c.code, c.level, c.detail, c.advice))
                .collect();
            assert_eq!(
                desktops,
                [(
                    "desktop_data_folder",
                    Level::Ok,
                    "Pitboard switches Claude Desktop accounts on macOS only".to_string(),
                    "This index holds `desktop/work` and `desktop/home`. Nothing here changes \
                     them."
                        .to_string()
                )],
                "{os:?}"
            );
        }

        f.desktop = Some(desktop());
        assert_eq!(
            check(&evaluate(&f), "desktop_data_folder").detail,
            "/Users/dana/Library/Application Support/Claude",
            "a scratch data folder on Linux is read as on macOS"
        );

        state.accounts.clear();
        assert_eq!(
            judge_desktop_off_macos(&state).advice,
            "This index holds folders set aside for Claude Desktop. Nothing here changes them."
        );
    }

    #[test]
    fn linux_inside_wsl_is_told_what_pitboard_switches_there() {
        let mut facts = facts();
        facts.os = Os::Linux;
        assert!(evaluate(&facts).iter().all(|c| c.code != "wsl"));
        let said = |facts: &Facts| {
            let checks = evaluate(facts);
            let wsl = check(&checks, "wsl");
            (wsl.level, wsl.detail.clone(), wsl.advice.clone())
        };
        facts.wsl = Some(WslFact {
            distribution: Some("Ubuntu".into()),
            codex_home_on_windows: None,
        });
        assert_eq!(
            said(&facts),
            (
                Level::Ok,
                "inside WSL, in Ubuntu".into(),
                "Pitboard here switches what is installed in this Linux. Claude Code and Codex \
                 installed on Windows keep their own logins, which Pitboard for Windows switches."
                    .into()
            )
        );
        let shared = "Pitboard here switches what is installed in this Linux. Claude Code \
                      installed on Windows keeps its own login, which Pitboard for Windows \
                      switches.";
        facts.wsl = Some(WslFact {
            distribution: None,
            codex_home_on_windows: Some(PathBuf::from("/mnt/c/Users/x/.codex")),
        });
        assert_eq!(
            said(&facts),
            (
                Level::Ok,
                "inside WSL".into(),
                format!(
                    "{shared} Codex's home, /mnt/c/Users/x/.codex, is C:\\Users\\x\\.codex on \
                     Windows, where Codex here starts no shared background server. Where Codex \
                     on Windows uses C:\\Users\\x\\.codex, a switch here changes its login too."
                )
            )
        );
        facts.wsl = Some(WslFact {
            distribution: None,
            codex_home_on_windows: Some(PathBuf::from("/media/c/codex")),
        });
        assert_eq!(
            said(&facts).2,
            format!(
                "{shared} Codex's home, /media/c/codex, is on a Windows drive, where Codex here \
                 starts no shared background server. Where Codex on Windows uses that folder, a \
                 switch here changes its login too."
            )
        );
    }

    #[test]
    fn a_windows_drive_wsl_mounts_is_named_as_windows_names_it() {
        for (path, windows) in [
            ("/mnt/c", Some(r"C:\")),
            ("/mnt/c/Users/x/.codex", Some(r"C:\Users\x\.codex")),
            ("/mnt/d/", Some(r"D:\")),
            ("/mnt/cd/x", None),
            ("/mnt/1/x", None),
            ("/home/x/.codex", None),
        ] {
            assert_eq!(
                on_windows(std::path::Path::new(path)).as_deref(),
                windows,
                "{path}"
            );
        }
    }

    // `codex_wsl`: `WSL_DISTRO_NAME` set, even empty, or `/proc/version` naming Microsoft.
    #[test]
    fn wsl_is_read_as_codex_reads_it_and_on_linux_alone() {
        use crate::context::Environment;
        let home = tempfile::tempdir().expect("a scratch home");
        let codex = home.path().join("codex");
        let read =
            |os: Os, distribution: Option<&str>, host: &Arc<crate::host::memory::MemoryHost>| {
                let mut pairs = vec![
                    ("HOME", home.path().to_string_lossy().into_owned()),
                    ("USERPROFILE", home.path().to_string_lossy().into_owned()),
                    ("CODEX_HOME", codex.to_string_lossy().into_owned()),
                ];
                pairs.extend(distribution.map(|name| ("WSL_DISTRO_NAME", name.to_owned())));
                let ctx = Context::read(&Environment::on(os, pairs), "cli")
                    .with_memory_stores(host.clone());
                wsl_fact(&ctx).map(|wsl| (wsl.distribution, wsl.codex_home_on_windows))
            };
        let host = crate::host::memory::MemoryHost::new();
        assert_eq!(read(Os::Linux, None, &host), None);
        assert_eq!(
            read(Os::Linux, Some("Ubuntu"), &host),
            Some((Some("Ubuntu".into()), None))
        );
        assert_eq!(read(Os::Linux, Some(""), &host), Some((None, None)));
        for os in [Os::MacOs, Os::Windows] {
            assert_eq!(read(os, Some("Ubuntu"), &host), None, "{os:?}");
        }
        host.runs_in_wsl();
        host.mounts_a_windows_drive(home.path());
        assert_eq!(
            read(Os::Linux, None, &host),
            Some((None, Some(codex.clone())))
        );
        assert_eq!(read(Os::Windows, None, &host), None);
    }

    #[test]
    fn no_system_is_told_a_warm_up_is_not_available() {
        let mut facts = facts();
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            facts.os = os;
            assert!(
                evaluate(&facts).iter().all(|c| c.code != "warm_up"),
                "{os:?}"
            );
        }
    }

    // Add account said Claude Code was not installed, and This PC said to run it.
    #[test]
    fn a_machine_without_claude_code_is_told_to_install_it() {
        let mut facts = facts();
        facts.claude_program = None;
        facts.config = Err(Error::ClaudeConfigMissing {
            path: PathBuf::from("/home/x/.claude.json"),
            spelling: words::Spelling::Bare,
        });
        facts.config_account = None;
        let checks = evaluate(&facts);
        let said = |code: &str| {
            let check = checks.iter().find(|c| c.code == code).expect(code);
            (check.detail.clone(), check.advice.clone())
        };
        assert_eq!(
            said("config_file"),
            (
                "Claude Code is not installed where Pitboard looks, and /home/x/.claude.json does \
                 not exist"
                    .into(),
                "Install Claude Code, then run `claude` once.".into()
            )
        );
        assert_eq!(
            said("identity").1,
            "It writes this after its first successful call. Install Claude Code, then run \
             `claude` once."
        );
    }

    // Windows PowerShell 5.1 refuses the `claude.ps1` beside npm's `claude.cmd`.
    #[test]
    fn a_machine_where_claude_code_never_ran_is_told_to_run_it() {
        for (os, found, spelling) in [
            (Os::MacOs, "/opt/homebrew/bin/claude", words::Spelling::Bare),
            (
                Os::Windows,
                "C:/Users/x/.local/bin/claude.exe",
                words::Spelling::Bare,
            ),
            (
                Os::Windows,
                "C:/Users/x/AppData/Roaming/npm/claude.cmd",
                words::Spelling::Cmd,
            ),
        ] {
            let program = spelling.of(ProviderId::Claude);
            let mut facts = facts();
            facts.os = os;
            facts.claude_program = Some(PathBuf::from(found));
            let missing = Error::ClaudeConfigMissing {
                path: PathBuf::from("/home/x/.claude.json"),
                spelling,
            };
            let detail = missing.to_string();
            facts.config = Err(missing);
            facts.config_account = None;
            let checks = evaluate(&facts);
            let said = |code: &str| {
                let check = checks.iter().find(|c| c.code == code).expect(code);
                (check.detail.clone(), check.advice.clone())
            };
            assert_eq!(
                said("config_file"),
                (detail, format!("Run `{program}` once.")),
                "{found}"
            );
            assert_eq!(
                said("identity").1,
                format!("It writes this after its first successful call. Run `{program}` once.")
            );
        }
    }

    // The session's fresh PC read "1 broken: do not switch accounts until fixed." for this
    // alone, with nothing enrolled and Claude Code installed but never run.
    #[test]
    fn claude_code_that_never_ran_breaks_nothing() {
        let mut facts = facts();
        facts.config = Err(Error::ClaudeConfigMissing {
            path: PathBuf::from("/home/x/.claude.json"),
            spelling: words::Spelling::Bare,
        });
        facts.config_account = None;
        facts.in_use = None;
        facts.credential = Ok(None);
        for installed in [true, false] {
            facts.claude_program = installed.then(|| PathBuf::from("/opt/homebrew/bin/claude"));
            let checks = evaluate(&facts);
            assert_eq!(
                check(&checks, "config_file").level,
                Level::Warn,
                "{installed}"
            );
            assert!(
                checks.iter().all(|c| c.level != Level::Fail),
                "{installed}: {:?}",
                checks
                    .iter()
                    .filter(|c| c.level == Level::Fail)
                    .map(|c| c.code)
                    .collect::<Vec<_>>()
            );
        }
    }

    /// A machine that uses only Codex is not told Claude Code is broken, nor to run a
    /// program it does not use. Everything about Pitboard itself is still checked.
    #[test]
    fn a_machine_with_only_codex_is_not_judged_on_claude_code() {
        let mut windows = facts();
        windows.os = Os::Windows;
        windows.fallback_login = Some(FallbackLogin {
            path: windows.credential_file.clone(),
            fingerprint: Ok(None),
        });
        windows.status_line = Some(StatusLineFact {
            git_bash: claude::GitBash {
                found: None,
                passed_over: None,
            },
            powershell: None,
            pitboard: None,
            runs_pitboard: false,
        });
        let made: Vec<&str> = evaluate(&windows).iter().map(|c| c.code).collect();
        // A name no check makes hides nothing, and the loop under it would pass over it.
        for own in CLAUDE_CODES_OWN {
            assert!(made.contains(own), "{own} is no check's code");
        }

        for mut facts in [facts(), windows] {
            facts.claude_present = false;
            facts.config = Err(Error::ClaudeConfigMissing {
                path: PathBuf::from("/nowhere/.claude.json"),
                spelling: words::Spelling::Bare,
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
                "Pitboard's own still is"
            );
            assert!(checks.iter().any(|c| c.code.starts_with("codex_")));
        }
    }

    #[test]
    fn windows_is_told_how_it_runs_and_which_build_it_needs() {
        let mut facts = facts();
        facts.os = Os::Windows;
        let refusing = |facts: &Facts| -> Vec<(&'static str, String, String)> {
            evaluate(facts)
                .into_iter()
                .filter(|c| ["elevated", "system_too_old"].contains(&c.code))
                .map(|c| (c.code, c.detail, c.advice))
                .collect()
        };
        assert!(refusing(&facts).is_empty());

        facts.elevation = crate::host::Elevation::Elevated {
            why: crate::host::token::AS_ADMINISTRATOR,
        };
        assert_eq!(
            refusing(&facts),
            [(
                "elevated",
                "Pitboard runs as administrator".to_string(),
                "Pitboard changes nothing this way: it reads, and renews and writes nothing. Run \
                 it from a terminal that is not elevated (not Run as administrator)."
                    .to_string()
            )]
        );

        facts.elevation = crate::host::Elevation::Elevated {
            why: crate::host::token::IN_EVERY_PROGRAM,
        };
        facts.floor = crate::host::Floor::Below { build: 22631 };
        assert_eq!(
            refusing(&facts),
            [
                (
                    "system_too_old",
                    "Pitboard runs on Windows build 22631, older than Windows 11 24H2 (build \
                     26100)"
                        .to_string(),
                    "Pitboard changes nothing here: it reads, and renews and writes nothing. It \
                     changes things on Windows 11 24H2 and later, and on Windows Server 2025: \
                     update Windows to use it here."
                        .to_string()
                ),
                (
                    "elevated",
                    "Pitboard runs elevated, as every program this Windows account starts does"
                        .to_string(),
                    "Pitboard changes nothing this way: it reads, and renews and writes nothing. \
                     That is so with User Account Control off and in the built-in Administrator \
                     account: run it from a standard account, or turn User Account Control on."
                        .to_string()
                ),
            ]
        );
    }

    #[test]
    fn a_home_that_is_not_a_full_path_still_hears_why_nothing_changes() {
        use crate::context::Environment;
        let env: Environment = [("HOME", "")].into_iter().collect();
        let host = crate::host::memory::MemoryHost::new();
        host.runs_on(crate::host::Floor::Below { build: 22631 });
        host.runs_with(crate::host::Elevation::Unknown);
        let ctx = Context::for_command_line(&env).with_memory_stores(host);
        let codes: Vec<&str> = run(&ctx, &time::moment)
            .checks
            .iter()
            .map(|c| c.code)
            .collect();
        assert_eq!(codes, ["system_too_old", "elevated", "homes"]);
    }

    // RF-B1 measured the profile, local app data and the home agreeing on a local profile.
    #[test]
    fn doctor_says_where_windows_keeps_an_accounts_folders_apart_from_the_home() {
        let home = PathBuf::from("C:").join("Users").join("x");
        let measured = || WindowsHomes {
            home: home.clone(),
            default_pitboard_home: Os::Windows.default_pitboard_home(&home),
            local_app_data: Some(home.join("AppData").join("Local")),
            profile: Some(home.clone()),
            codex_home_set: false,
            roams: false,
        };
        let codes = |windows: Option<WindowsHomes>| {
            let mut facts = facts();
            facts.windows = windows;
            judge_windows_homes(&facts)
                .into_iter()
                .chain(judge_codex_installer_home(&facts))
                .map(|check| (check.code, check.level))
                .collect::<Vec<_>>()
        };
        assert!(codes(None).is_empty());
        assert!(codes(Some(measured())).is_empty());
        let in_any_case = PathBuf::from("c:").join("users").join("X");
        let other_case = WindowsHomes {
            local_app_data: Some(in_any_case.join("appdata").join("local")),
            profile: Some(in_any_case),
            ..measured()
        };
        assert!(codes(Some(other_case)).is_empty());
        let moved = WindowsHomes {
            local_app_data: Some(PathBuf::from("D:").join("Local")),
            ..measured()
        };
        assert_eq!(codes(Some(moved)), [("local_app_data", Level::Warn)]);
        let elsewhere = PathBuf::from("C:").join("Users").join("y");
        let parted = || WindowsHomes {
            profile: Some(elsewhere.clone()),
            ..measured()
        };
        assert_eq!(
            codes(Some(parted())),
            [("codex_installer_home", Level::Warn)]
        );
        let named = WindowsHomes {
            codex_home_set: true,
            ..parted()
        };
        assert!(codes(Some(named)).is_empty(), "CODEX_HOME names one folder");
        let roams = WindowsHomes {
            roams: true,
            ..measured()
        };
        assert_eq!(codes(Some(roams)), [("roaming_profile", Level::Warn)]);
    }

    // The command line heads a tool's checks where they start, so each tool's come together.
    #[test]
    fn where_codex_keeps_its_home_on_windows_is_one_of_codexs_checks() {
        let home = PathBuf::from("C:").join("Users").join("x");
        let mut facts = facts();
        facts.os = Os::Windows;
        facts.codex = with_codex();
        facts.windows = Some(WindowsHomes {
            home: home.clone(),
            default_pitboard_home: Os::Windows.default_pitboard_home(&home),
            local_app_data: Some(home.join("AppData").join("Local")),
            profile: Some(PathBuf::from("C:").join("Users").join("y")),
            codex_home_set: false,
            roams: true,
        });
        let checks = evaluate(&facts);
        let codes: Vec<&str> = checks.iter().map(|c| c.code).collect();
        let installer = codes
            .iter()
            .position(|code| *code == "codex_installer_home")
            .expect("said");
        assert!(
            codes[..installer]
                .iter()
                .all(|code| section(code).is_none()),
            "{codes:?}"
        );
        assert!(
            codes[installer..]
                .iter()
                .all(|code| section(code) == Some(ProviderId::Codex)),
            "{codes:?}"
        );
        facts.codex = no_codex();
        let codes: Vec<&str> = evaluate(&facts).iter().map(|c| c.code).collect();
        assert!(codes.contains(&"roaming_profile"), "{codes:?}");
        assert!(
            codes.iter().all(|code| section(code).is_none()),
            "a machine with no Codex has no Codex heading: {codes:?}"
        );
    }

    /// With neither tool present, a new machine is told what to do first, as it always was.
    #[test]
    fn a_machine_with_neither_tool_still_hears_about_claude_code() {
        let mut facts = facts();
        facts.claude_present = false;
        let checks = evaluate(&facts);
        assert!(checks.iter().any(|c| c.code == "config_file"));
    }

    /// Where a home the environment names is not a full path, doctor says so and checks
    /// nothing else, since every other check reads under one of the homes. It read Claude
    /// Code's and Pitboard's files under whatever folder it was run from.
    #[test]
    fn doctor_checks_nothing_under_a_home_that_is_not_a_full_path() {
        use crate::context::Environment;
        let variable = crate::host::OS.home_variable();
        let full = std::env::temp_dir().to_string_lossy().into_owned();
        let empty = format!("{variable} is empty");
        for (pairs, said) in [
            (&[(variable, "")][..], empty.as_str()),
            (
                &[(variable, full.as_str()), ("CODEX_HOME", "codex")],
                "CODEX_HOME is `codex`, which is not a full path",
            ),
        ] {
            let env: Environment = pairs.iter().copied().collect();
            let ctx = Context::for_command_line(&env)
                .with_memory_stores(crate::host::memory::MemoryHost::new());
            let diagnosis = run(&ctx, &time::moment);
            let checks: Vec<(&str, Level, &str)> = diagnosis
                .checks
                .iter()
                .map(|c| (c.code, c.level, c.detail.as_str()))
                .collect();
            assert_eq!(checks, [("homes", Level::Fail, said)], "{pairs:?}");
            assert!(
                diagnosis.checks[0]
                    .advice
                    .starts_with("Set it to a full path")
            );
            assert_eq!(diagnosis.environment, json!({}));
        }
    }

    // What Claude Code 2.1.302 left on the session's PC (2026-10-18) as it moved its login
    // into Credential Manager, which a fresh session read as "Not logged in".
    #[test]
    fn pieces_nobody_can_join_are_said_to_be_unreadable_to_claude_code_too() {
        for (found, spelling) in [
            ("C:/Users/x/.local/bin/claude.exe", words::Spelling::Bare),
            (
                "C:/Users/x/AppData/Roaming/npm/claude.cmd",
                words::Spelling::Cmd,
            ),
        ] {
            let program = spelling.of(ProviderId::Claude);
            let mut facts = facts();
            facts.os = Os::Windows;
            facts.claude_program = Some(PathBuf::from(found));
            facts.backend = Ok(store::Backend::CredentialManager);
            facts.credential = Err(store::Error::Broken {
                why: "`Claude Code-credentials/claude-code-user` lacks piece #2 of the 3 its \
                      manifest names"
                    .into(),
                spelling,
            });
            let check = judge_credential(&facts);
            assert_eq!(check.level, Level::Fail);
            assert_eq!(
                check.detail,
                "`Claude Code-credentials/claude-code-user` lacks piece #2 of the 3 its manifest \
                 names, which Claude Code cannot read either"
            );
            assert!(
                check
                    .advice
                    .starts_with(&format!("Run `{program} auth logout`")),
                "{}",
                check.advice
            );
        }
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

    // A switch refuses it as a shape it does not know, so it is no signed-out document.
    #[test]
    fn a_credential_that_is_not_an_object_is_a_failure() {
        use crate::provider::ProviderError;
        let document = serde_json::json!(["claudeAiOauth"]);
        assert!(matches!(
            crate::provider::claude_code().slice(&document),
            Err(ProviderError::ShapeUnexpected { .. })
        ));
        let mut facts = facts();
        facts.credential = Ok(Some(document));
        assert_eq!(judge_credential(&facts).level, Level::Fail);
    }
}
