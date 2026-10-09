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
use crate::host::{Access, Kind, Os};
use crate::provider::ProviderId;
use crate::provider::claude::daemon;
use crate::provider::claude::live as claude_live;
use crate::provider::claude::paths as claude;
use crate::provider::claude::slot;
use crate::provider::codex::paths as codex;
use crate::service::Stored;
use crate::state::{Park, State};
use crate::{home, park, store, switch, time, words};
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

/// Everything read from the machine, so judging it touches nothing. Each check that is
/// added reads something more, so it cannot be built outside this crate.
#[non_exhaustive]
pub struct Facts {
    /// The system these were read on, which is what decides some of the judgements.
    pub os: Os,
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
    pub home_access: Option<Access>,
    /// Anything on this disk holding a login that somebody other than the owner can read:
    /// (path, its access). Empty on a machine with a keychain and a tidy plaintext
    /// fallback, and the whole security story on a machine without one.
    pub readable_by_others: Vec<(String, Access)>,
    pub machine_id_known: bool,
    /// `CLAUDE_CODE_HOVER_REST`, which switches on the successor credential backend.
    pub hover_rest_env: bool,
    /// Claude Code's supervisor daemon, where one has ever run for this slot.
    pub daemon: Option<daemon::Daemon>,
    /// Names Pitboard wrote down before creating a park and has not resolved yet.
    pub pending_parks: Vec<String>,
    /// Which Claude Code is installed here, read off disk.
    pub claude_version: Option<String>,
    /// Every reason a session here would authenticate as something other than the stored
    /// login, read from settings files as well as from this process's environment.
    pub auth_overrides: Vec<crate::settings::Override>,
    /// What of those settings Pitboard does not read here, where it does not read them all:
    /// Claude Code's managed settings on Windows, until W17.
    pub auth_unread: Option<&'static str>,
    /// Accounts Pitboard is not asking Anthropic about yet, and for how long: (uuid, seconds).
    pub asking_held: Vec<(String, i64)>,
    pub state: Result<State, Error>,
    /// Whose login Claude Code has stored, as Anthropic last said, where the account list
    /// can be read.
    pub in_use: Option<crate::in_use::Known>,
    /// Each enrolled account's parked login, read back from the vault.
    pub parks: Vec<ParkFact>,
    pub interrupted: bool,
    /// The refusal the next change would make over the interrupted switch, where that
    /// change could not finish it and that can be told without asking anybody.
    pub stuck: Option<String>,
    /// What is read about Codex here, for the section that is about it.
    pub codex: CodexFacts,
    /// Whether Claude Code is on this machine at all: installed, run once, signed in, or
    /// holding enrolled accounts. A machine that uses only Codex is not told Claude Code is
    /// broken.
    pub claude_present: bool,
    /// The daily renewal schedule, where this home has one installed.
    pub schedule: Option<ScheduleFact>,
    /// How Pitboard's requests leave this machine, as its environment says.
    pub network: NetworkFact,
    /// Whether this is the app's doctor, whose environment is the one the system started the
    /// app with, not a shell's: the schedule's runs are started the same way, and given a
    /// shell's proxy only where it was installed from one.
    pub in_the_app: bool,
    /// Whether this process runs as the person themselves, as the host says, which is what
    /// the gate every change passes asks.
    pub elevation: crate::host::Elevation,
    /// Claude Code's fallback file, where it is behind the keychain holding the login in use.
    pub fallback_login: Option<FallbackLogin>,
    pub floor: crate::host::Floor,
    pub now: i64,
}

/// How Pitboard's requests leave this machine: directly, or through the HTTP proxy its
/// environment names, read as ureq 3.4.2 reads one from a process's environment. A SOCKS
/// proxy, which Pitboard does not use yet, fails each request it would have carried.
pub struct NetworkFact {
    /// The proxy the environment names, where it names one Pitboard can read.
    pub proxy: Option<ProxyFact>,
    /// The variables before it set to something that is not a proxy's address, which
    /// Pitboard passes over, in the order it reads them.
    pub passed_over: Vec<&'static str>,
}

/// The proxy the environment names.
pub struct ProxyFact {
    /// The variable that names it.
    pub variable: &'static str,
    /// Its scheme, host and port, with `***` for a user name or a password it holds.
    pub address: String,
    /// Its host and port, `proxy.example.com:3128`, as `address` shows them: the part of it
    /// that can name the company whose network it is.
    pub server: String,
    /// Whether its host is loopback, this machine itself, which names nothing beyond it.
    pub on_loopback: bool,
    /// Whether it is a SOCKS proxy, which Pitboard does not use yet: a request to a host it
    /// does not exempt fails before anything is sent, and one to a host it exempts goes out
    /// directly.
    pub socks: bool,
    /// `NO_PROXY` or `no_proxy`, whichever is read, where either is set.
    pub exempting: Option<&'static str>,
    /// The hosts Pitboard calls that it exempts, which Pitboard reaches directly.
    pub exempt: Vec<String>,
}

/// The daily renewal schedule as it is installed, read from the file Pitboard wrote and
/// never by asking the scheduler.
pub struct ScheduleFact {
    /// The file the platform's scheduler reads.
    pub path: PathBuf,
    /// The Pitboard it runs, where the file names one the way Pitboard writes it.
    pub program: Option<PathBuf>,
    /// Whether that Pitboard is still there to be run.
    pub program_found: bool,
    /// How its runs send requests, as the proxy variables its file gives them say: those of
    /// the Pitboard that installed it, read as this run's are ([`Facts::network`]).
    pub network: NetworkFact,
    /// Whether its runs send requests another way than this run does: through another proxy
    /// or none, with another user name or password, or with other hosts exempt.
    pub network_differs: bool,
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
    /// How many Codex accounts Pitboard has enrolled.
    pub enrolled: usize,
    /// Where Codex is configured to keep its login, in Codex's own words:
    /// `cli_auth_credentials_store`'s `file`, `keyring`, `auto` or `ephemeral`, or `secrets`
    /// for a keychain store with `[features] secret_auth_storage`, which Codex has no one
    /// word for. `unknown` where a layer Codex reads it from is there and cannot be read as
    /// Codex reads it, which stops Codex from starting.
    pub backend: &'static str,
    /// The setting that chose it, in words that follow it in brackets: `Codex's default`,
    /// `` `cli_auth_credentials_store = "keyring"` in /etc/codex/config.toml ``, or `pinned to
    /// `keyring` by /etc/codex/requirements.toml`. For `unknown`, what cannot be read.
    pub backend_setting: String,
    /// What would choose the file store instead, as a sentence with no full stop: the line to
    /// set and where, or who can. For `unknown`, what Codex does until what cannot be read is
    /// put right, as sentences with no last full stop. Empty for the file.
    pub backend_remedy: String,
    /// Where the default store keeps it.
    pub auth_file: PathBuf,
    /// Who can reach that file, where there is such a file.
    pub auth_access: Option<Access>,
    /// Whose login the file holds, or why that could not be told. `Ok(None)` for no file,
    /// and for a store Pitboard does not read.
    pub login: Result<Option<CodexLogin>, CodexLoginTrouble>,
    /// Where the `codex` Pitboard would run is, where it is anywhere.
    pub program: Option<PathBuf>,
    /// Which Codex that is, read off the path it is installed at. `None` both where there
    /// is no `codex` and where its path does not say; [`CodexFacts::program`] tells which.
    pub version: Option<String>,
    /// Every `codex` running as this user, by where it runs from. `None` where that could
    /// not be asked.
    pub running: Option<Vec<crate::holder::Holding>>,
}

/// Whose a Codex login is, as its own ID token says. Nothing in here is a secret: the
/// fingerprint is a handle on the refresh token, never the token.
pub struct CodexLogin {
    pub email: String,
    pub account_id: String,
    /// Empty where the login holds no refresh token.
    pub fingerprint: String,
}

/// Why Codex's login names no account Pitboard can handle.
///
/// Two answers rather than one, because they call for different things. A login signed in
/// some way Pitboard does not switch, such as with an API key, is somebody's choice and
/// nothing is wrong with it; a login that cannot be read, or that mixes two accounts, is.
#[derive(Debug)]
pub enum CodexLoginTrouble {
    /// Signed in, and not with an account Pitboard parks or switches. Says why, in Codex's
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
    /// Whether its login is the one its tool has stored, as far as files say.
    pub active: bool,
    /// When this account last came to be in use, where that is recorded.
    pub last_used_at: Option<i64>,
    pub park: Option<Park>,
    /// Why it cannot be read back, if it cannot.
    pub unreadable: Option<Unreadable>,
}

/// Why a parked login could not be read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    /// It is behind a keychain that is locked here, which says nothing about the login.
    Locked,
    /// Missing, damaged, or refused by the store, in words.
    Broken(String),
}

/// Claude Code's plaintext fallback file, there while the keychain holds the login in use.
pub struct FallbackLogin {
    pub path: PathBuf,
    /// A handle on the refresh token of the login it holds, never the token: `None` where
    /// it holds no login, empty where that login has no refresh token, and why where what
    /// it holds cannot be read.
    pub fingerprint: Result<Option<String>, store::Error>,
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
    let in_use: std::collections::BTreeMap<ProviderId, Option<crate::api::Owner>> = ProviderId::ALL
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
            label: a.label.clone(),
            name: state.typed(&a.key()),
            last_used_at: a.last_used_at,
            active: in_use
                .get(&a.provider())
                .and_then(Option::as_ref)
                .is_some_and(|owner| a.owned_by(owner)),
            park: a.parked.clone(),
            unreadable: a.parked.as_ref().and_then(|p| {
                park::load(ctx, &a.key(), p).err().map(|e| match e {
                    Error::Store(store::Error::Locked) => Unreadable::Locked,
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

pub fn gather(ctx: &Context) -> Facts {
    let config = claude::load_config(ctx);
    let state = crate::state::load(ctx);
    let service = claude::live_service(ctx);
    let home = home::dir(ctx);
    let identity = config.as_ref().ok().and_then(claude::identity);
    Facts {
        os: crate::host::OS,
        security_tool: crate::host::OS
            .secrets_tool()
            .filter(|p| std::fs::metadata(p).is_ok())
            .map(str::to_string),
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
        home_access: crate::host::fs::access(&home),
        readable_by_others: loose_logins(ctx),
        home,
        machine_id_known: crate::state::machine_id() != "unknown",
        hover_rest_env: ctx.hover_rest,
        daemon: daemon::read(ctx),
        pending_parks: crate::pending::outstanding(ctx, state.as_ref().ok()),
        claude_version: claude::installed_version(ctx),
        auth_overrides: crate::settings::overrides(ctx),
        auth_unread: crate::settings::unread(),
        asking_held: crate::budget::holds(ctx),
        in_use: state
            .as_ref()
            .ok()
            .map(|s| crate::in_use::known(ctx, s, ProviderId::Claude)),
        parks: state
            .as_ref()
            .map(|s| park_facts(ctx, s))
            .unwrap_or_default(),
        codex: codex_facts(ctx, state.as_ref().ok()),
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
        network: network_fact(&ctx.proxy),
        in_the_app: ctx.caller == "app",
        elevation: ctx.host().elevation(ctx),
        fallback_login: fallback_login(ctx),
        floor: ctx.host().floor(),
        now: ctx.now(),
    }
}

/// Claude Code's fallback file, where it is behind the keychain, and the login it holds.
/// Where whether it is there cannot be told, nothing is said of it: a keychain that cannot
/// be read is the `credential_store` check's to say, and a file that cannot even be looked
/// at is one Claude Code's own look takes for not there.
fn fallback_login(ctx: &Context) -> Option<FallbackLogin> {
    let held = claude_live::behind(ctx)?;
    Some(FallbackLogin {
        path: claude_live::credential_file(ctx),
        fingerprint: held.map(|held| {
            claude_live::login_in(&held)
                .map(|document| crate::provider::claude::document::fingerprint_of(&document))
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
                exempting: proxies.exempting(),
                exempt: proxies.exempt_hosts(),
            }),
        passed_over: proxies.passed_over().to_vec(),
    }
}

/// The schedule, where this home has one.
fn schedule_fact(ctx: &Context) -> Option<ScheduleFact> {
    if !crate::schedule::serves(ctx) {
        return None;
    }
    let crate::schedule::Installed::Yes { path, .. } = crate::schedule::status(ctx) else {
        return None;
    };
    let program = crate::schedule::installed_program(ctx);
    let proxies = crate::schedule::installed_proxies(ctx).unwrap_or_default();
    Some(ScheduleFact {
        program_found: program.as_deref().is_some_and(std::path::Path::is_file),
        program,
        path,
        network: network_fact(&proxies),
        network_differs: !proxies.same_way(&ctx.proxy),
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

/// Every file on this machine that holds a usable login and is not private to its owner.
///
/// Claude Code has no keyring backend outside macOS and Windows, so on Linux its own login
/// is a plaintext file it chmods to 0600, and Pitboard's parked logins are plaintext files
/// beside it. That is not Pitboard weakening anything, but it does mean the only thing
/// between a parked OAuth token and everyone else with an account on the machine is who
/// the file lets in, and that is something a backup restore, a `cp`, an rsync or a careless
/// umask quietly changes. So it is looked at rather than assumed.
fn loose_logins(ctx: &Context) -> Vec<(String, Access)> {
    let mut loose = Vec::new();
    let mut look = |path: std::path::PathBuf| {
        if let Some(access) = crate::host::fs::access(&path)
            && access.shared
        {
            loose.push((path.display().to_string(), access));
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
    let store = codex::store(ctx);
    let backend = store.backend;
    let auth_file = codex::auth_file(ctx);
    let home = codex::home(ctx);
    // The program Pitboard would run, as the context names it and where the context looks:
    // an app started from Finder has no shell `PATH` and passes the login shell's, and a
    // test names a program of its own.
    let program = crate::provider::program_of(ctx, ProviderId::Codex);
    CodexFacts {
        present: home.is_dir(),
        enrolled: state.map_or(0, |s| {
            s.accounts
                .iter()
                .filter(|a| a.provider() == ProviderId::Codex)
                .count()
        }),
        backend: backend_name(backend),
        backend_setting: match backend {
            codex::Backend::Unknown => store.why_unknown().unwrap_or_default(),
            _ => store.setting(),
        },
        backend_remedy: match backend {
            codex::Backend::File => String::new(),
            codex::Backend::Unknown => store.while_unknown().unwrap_or_default(),
            _ => store.to_use_the_file(),
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
        // Not Codex's word: nobody can tell, Codex included.
        codex::Backend::Unknown => "unknown",
    }
}

/// What the file store's line adds: the one layer Pitboard leaves unread, and what it does.
const PROJECTS_NOT_READ: &str = "Pitboard does not read a project's own \
    `.codex/config.toml`, which applies only inside that project: in a trusted project that \
    sets `cli_auth_credentials_store`, Codex keeps its login somewhere else.";

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

/// Every `codex` running as this user, by kind, asked the way a switch asks so the two
/// cannot disagree.
fn running_codex(ctx: &Context) -> Option<Vec<crate::holder::Holding>> {
    match crate::provider::of(ProviderId::Codex).adoption(None) {
        crate::provider::Adoption::RestartRequired { program, holders } => {
            crate::holder::find(ctx, program, holders)
        }
        crate::provider::Adoption::PollingWithin(_)
        | crate::provider::Adoption::AtRenewal { .. } => Some(Vec::new()),
    }
}

/// The check that fails where this process runs as root or under sudo, or elevated on
/// Windows, or nobody could tell whether it does, since then Pitboard changes nothing: `None`
/// where it runs as the person themselves.
fn elevated(os: Os, elevation: crate::host::Elevation) -> Option<Check> {
    let said = match elevation {
        crate::host::Elevation::Normal => return None,
        crate::host::Elevation::Elevated { why } => crate::words::elevated(os, Some(why)),
        crate::host::Elevation::Unknown => crate::words::elevated(os, None),
    };
    Some(fail(
        "elevated",
        "runs as",
        said.because,
        format!(
            "Pitboard changes nothing this way: it reads, and renews and writes nothing. {}",
            said.way_out
        ),
    ))
}

fn too_old(floor: crate::host::Floor) -> Option<Check> {
    let said = match floor {
        crate::host::Floor::Met => return None,
        crate::host::Floor::Below { build } => crate::words::too_old(Some(build)),
        crate::host::Floor::Unknown => crate::words::too_old(None),
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

/// The end of a sentence of advice that says what to run, from `: ` to its full stop, where
/// Pitboard can say a command, and the full stop alone where it cannot
/// ([`Os::make_private_command`]).
fn running(command: Option<String>) -> String {
    command.map_or_else(|| ".".into(), |command| format!(": `{command}`."))
}

pub fn evaluate(facts: &Facts) -> Vec<Check> {
    let mut checks = Vec::new();
    // Claude Code's own checks, where there is a Claude Code, or where there is no other
    // tool either: a new machine is told what to do first, as it always was. A machine
    // that uses only Codex is not told to run a program it does not use.
    let codex_here = facts.codex.present || facts.codex.enrolled > 0;
    let claude_here = facts.claude_present || !codex_here;

    // First, in the order the gate asks them: they are why every change here is refused.
    checks.extend(too_old(facts.floor));
    checks.extend(elevated(facts.os, facts.elevation));

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
    checks.extend(judge_in_use(facts));

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
            match facts.os {
                Os::MacOs => warn(
                    "credential_store",
                    "credential store",
                    detail,
                    "Claude Code fell back to a file, which means a keychain write failed at some point.",
                ),
                Os::Linux => ok("credential_store", "credential store", detail),
                // Claude Code has no keychain on Windows: the file is where it keeps the
                // login unless it uses Credential Manager, which W22 tells apart. Until then
                // its chain there is a store Pitboard does not read, never the file, so this
                // is not reached; and the file alone says nothing of which login is in use
                // while Credential Manager may hold another.
                Os::Windows => warn(
                    "credential_store",
                    "credential store",
                    detail,
                    "Claude Code may keep its login in Credential Manager instead, which \
                     Pitboard does not read yet.",
                ),
            }
        }
        Ok(store::Backend::Unknown) => fail(
            "credential_store",
            "credential store",
            "a store Pitboard does not read yet",
            "Treat this as unknown, never as empty.",
        ),
        // The one wrong diagnosis in this file. If Claude Code's config names somebody as
        // signed in, "nothing is signed in" is not an observation, it is Pitboard looking
        // in the wrong place, and it is the failure that would follow Claude Code moving
        // where it keeps a login.
        Ok(store::Backend::Absent) if facts.identity.is_some() => fail(
            "credential_store",
            "credential store",
            format!(
                "Claude Code's config says {} is signed in, and no store Pitboard reads \
                 holds that login",
                facts
                    .identity
                    .as_ref()
                    .map(|i| i.email.as_str())
                    .unwrap_or("somebody")
            ),
            "Pitboard will not write a login where nobody reads it. Check for a Pitboard \
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
        Some(access) => {
            warn(
                "home",
                "Pitboard home",
                format!("{} is {}", facts.home.display(), access.described),
                format!(
                    "Park names contain account identifiers, so only you should read it{}",
                    running(facts.os.make_private_command(
                        Kind::Directory,
                        &[&facts.home.display().to_string()]
                    ))
                ),
            )
        }
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
            .map(|(path, access)| format!("{path} is {}", access.described))
            .collect();
        fail(
            "private_on_disk",
            "logins on disk",
            names.join("; "),
            format!(
                "These hold usable OAuth tokens in plain text, which is how Claude Code \
                 stores them where there is no keychain. Anyone else on this machine can \
                 read them{}",
                running(
                    facts.os.make_private_command(
                        Kind::Any,
                        &facts
                            .readable_by_others
                            .iter()
                            .map(|(path, _)| path.as_str())
                            .collect::<Vec<_>>()
                    )
                ),
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
                _ => "Pitboard will not switch until its account list can be read.",
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
        checks.push(warn(
            "discarded",
            "old parked logins",
            format!("{} waiting to be deleted", state.discarded.len()),
            "Pitboard deletes them on its next change; if they stay, check the keychain is unlocked.",
        ));
    }

    if !facts.machine_id_known {
        checks.push(warn(
            "machine_id",
            "machine id",
            "this machine has no stable identifier",
            "Pitboard cannot tell this machine from another that also lacks one, so it cannot \
             refuse state copied between them. Never copy ~/.pitboard between machines.",
        ));
    }

    checks.push(judge_storage_v5(facts));
    checks.push(judge_daemon(facts));
    checks.push(judge_pending(facts));
    checks.extend(judge_schedule(facts));
    checks.extend(judge_schedule_proxy(facts));
    checks.push(judge_claude_version(facts));
    checks.push(judge_auth(facts));
    checks.push(judge_asking(facts));
    checks.push(judge_network(&facts.network));
    checks.extend(
        claude_parks
            .iter()
            .filter_map(|p| judge_dormant(p, facts.now)),
    );
    // Only where there is a Codex to say something about. A machine that has never run it
    // reads exactly as it did before Pitboard knew Codex existed.
    if codex_here || !codex_parks.is_empty() {
        checks.extend(judge_codex(facts.os, &facts.codex, &codex_parks, facts.now));
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
fn judge_in_use(facts: &Facts) -> Option<Check> {
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
    let when = time::moment(last.known_at, facts.now);
    let then = match &last.owner {
        Some(_) => format!("Anthropic named {in_use} {when}"),
        None => format!("Pitboard found no login stored {when}"),
    };
    match known.doubt {
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
        Ok(Some(document)) if document.get("claudeAiOauth").is_some() => Some(Some(
            crate::provider::of(ProviderId::Claude).fingerprint(document),
        )),
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
        // Read in 2.1.294 (the register's `locked_keychain_keeps_last_login`): a session
        // keeps serving the login it last read while the keychain is locked, and one that
        // read none reads the keychain as empty and falls through to the file.
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
        Err(e) => fail(
            "credential",
            "credential",
            e.to_string(),
            "Do not write to the store while this is failing.",
        ),
    }
}

/// The fallback file behind the keychain, with a login in it, none, or what cannot be read.
///
/// Read in 2.1.294: a sign-in where the keychain is locked writes its login to the file and
/// leaves the keychain's in place (`locked_sign_in_writes_fallback`), and a keychain write
/// deletes the file only where the keychain held nothing before
/// (`fallback_outlives_keychain_writes`). So the file stays through every switch, and a
/// session that cannot read the keychain signs in with a login in it whatever Pitboard
/// switched to. A session already running watches the file by a look at it, whatever it
/// holds and whether or not it can be read, and while it is there takes a switch only at
/// its login's next renewal (`fallback_file_pins_session_login`).
fn judge_fallback_login(facts: &Facts) -> Option<Check> {
    let left = facts.fallback_login.as_ref()?;
    let path = left.path.display();
    let running = format!(
        "While it is there, Claude Code sessions already running at a switch {}.",
        words::kept_until_renewed()
    );
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
                "Claude Code signs in with this wherever it cannot read the keychain, such as \
                 in a session started over SSH, and no switch reaches it. A sign-in made where \
                 the keychain could not be read leaves one, as `/login` over SSH does. \
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
                 cannot read the keychain signs in with, cannot be told, and `pitboard stow` \
                 cannot put it away. {running} Deleting it lets them follow a switch, and takes \
                 whatever it holds with it: `rm {path}`."
            ),
        ),
    };
    Some(warn("fallback_login", "fallback login", detail, advice))
}

/// A parked login this close to expiring is worth renewing now.
pub const RENEW_WITHIN: i64 = 3 * 86_400;

/// Whether a parked login is due to be renewed by when its refresh token expires, at
/// `refresh_expires_at`: within [`RENEW_WITHIN`], and not yet. One that has expired cannot
/// be renewed, and its account needs a sign-in. A renewal run, doctor's parked login check,
/// the standing `pitboard status` gives an account and the column form of a parked login's
/// life all ask this, so they agree on when a login is due. A run also renews a login whose
/// access token has lapsed, which `switch::renew` decides.
pub fn renewal_due(refresh_expires_at: i64, now: i64) -> bool {
    (1..RENEW_WITHIN).contains(&(refresh_expires_at - now))
}

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
    match &fact.unreadable {
        Some(Unreadable::Locked) => {
            return warn(
                code,
                name,
                "not read: the keychain is locked",
                "Unlock it, then run `pitboard doctor` again.",
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
        let when = time::moment(at, now);
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
    let fails = requests_fail(proxy);
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
        detail.push_str(&format!("; {clause}"));
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

/// Whether each request through `proxy` to a host Pitboard calls that the variable exempting
/// hosts does not name fails: whether it is a SOCKS proxy, which Pitboard does not use yet,
/// and any host Pitboard calls is not exempt from it.
fn requests_fail(proxy: &ProxyFact) -> bool {
    proxy.socks && proxy.exempt.len() < crate::proxy::called().len()
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

/// Whether moving the stored login would change anything a session sees.
///
/// Claude Code resolves this from layered settings, so a managed policy or a line in a
/// person's own `settings.json` can make every switch Pitboard performs a no-op. Read from
/// files rather than from this process's environment, because an app launched from Finder
/// has no environment to read and is the surface most likely to be used on a machine that
/// needs the answer.
///
/// Where Pitboard does not read every layer, it says which it did not read beside that
/// answer, since a layer it did not read could still set something else. Where a file sits
/// behind the keychain, a session already running takes the stored login only when it next
/// reads it, at its login's renewal, which the `fallback_login` check says.
fn judge_auth(facts: &Facts) -> Check {
    if facts.auth_overrides.is_empty() {
        let read = if facts.fallback_login.is_some() {
            "the stored login, which is what Pitboard moves, once a session reads it again"
        } else {
            "the stored login, which is what Pitboard moves"
        };
        return ok(
            "auth_source",
            "what a session authenticates with",
            match facts.auth_unread {
                None => read.to_string(),
                Some(unread) => format!("{read}, as far as it reads: {unread}"),
            },
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
    let verified = crate::provider::claude::assumptions::VERIFIED_AGAINST;
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
    Some(match &schedule.program {
        Some(program) if !schedule.program_found => fail(
            "schedule",
            "renewal schedule",
            format!("runs {}, which is not there any more", program.display()),
            again,
        ),
        Some(program) if crate::schedule::an_apps_own_program(program) => fail(
            "schedule",
            "renewal schedule",
            format!(
                "runs {}, which is the app itself and not a command line",
                program.display()
            ),
            again,
        ),
        Some(program) => ok(
            "schedule",
            "renewal schedule",
            format!("daily  ·  runs {}", program.display()),
        ),
        None => warn(
            "schedule",
            "renewal schedule",
            format!(
                "{} does not say which Pitboard it runs",
                schedule.path.display()
            ),
            again,
        ),
    })
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
        Some(proxy) if requests_fail(proxy) => format!(
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

/// How to write the schedule again. There is an app only on macOS until the Windows app
/// ships.
fn again(os: Os) -> &'static str {
    if has_app(os) {
        "Turn daily renewal off and on again: in the app's Settings, or with \
         `pitboard schedule uninstall` and then `pitboard schedule install`."
    } else {
        "Turn daily renewal off and on again with `pitboard schedule uninstall` and then \
         `pitboard schedule install`."
    }
}

/// Whether Pitboard has an app for `os`, with a Settings window of its own. The Windows app
/// is still to be written, and says so here once it has its Settings.
fn has_app(os: Os) -> bool {
    match os {
        Os::MacOs => true,
        Os::Linux | Os::Windows => false,
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
fn judge_codex(os: Os, facts: &CodexFacts, parks: &[&ParkFact], now: i64) -> Vec<Check> {
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
        // Not a choice: Codex 0.160.0 does not start, or Pitboard does not read where the
        // store is chosen on this system yet.
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
                    "Pitboard reads only Codex's file store, auth.json, and will not touch the \
                     keychain item Codex created for itself, because every read of it would \
                     ask you for permission, so no enrolled Codex account can be switched to. \
                     {}.",
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
                "Sign in to one of your enrolled accounts with `codex login`. A switch \
                 needs an account signed in to park, so `pitboard use` cannot put one \
                 back while nothing is.",
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
                    "It holds a usable login in plain text, and Codex sets 0600 only when it \
                     creates the file, never on a later write{}",
                    running(os.make_private_command(
                        Kind::File,
                        &[&facts.auth_file.display().to_string()]
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
                "Pitboard parks and switches Codex's ChatGPT sign-ins, so `pitboard use \
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
                "Pitboard will not park or switch a Codex login it cannot read as one \
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
fn judge_codex_version(facts: &CodexFacts) -> Check {
    let verified = crate::provider::codex::assumptions::VERIFIED_AGAINST;
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
    if let Err(crate::error::Error::HomeNotAbsolute { variable, path }) =
        crate::home::check_absolute(ctx)
    {
        return unplaced(ctx, variable, &path);
    }
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
                // Unknown for a store Pitboard does not read, rather than a guess.
                "login_present": (facts.codex.backend == "file")
                    .then_some(facts.codex.auth_access.is_some()),
                "version": facts.codex.version,
            },
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
        checks: too_old(ctx.host().floor())
            .into_iter()
            .chain(elevated(crate::host::OS, ctx.host().elevation(ctx)))
            .chain(std::iter::once(homes))
            .collect(),
        environment: json!({}),
        redaction: crate::redact::Sheet::new(
            format!("{}:{}", crate::state::machine_id(), ctx.now_millis()),
            ctx.home().to_string_lossy(),
        ),
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
                .hide(held.refresh_fingerprint.clone(), "login")
                .hide(held.service.clone(), "park");
        }
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
mod tests {
    use super::*;
    use crate::service::Permit;
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
        }
    }

    fn facts() -> Facts {
        Facts {
            os: crate::host::OS,
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
            home_access: Some(mode(0o700)),
            readable_by_others: Vec::new(),
            machine_id_known: true,
            hover_rest_env: false,
            daemon: None,
            pending_parks: Vec::new(),
            claude_version: Some(crate::provider::claude::assumptions::VERIFIED_AGAINST.into()),
            auth_overrides: Vec::new(),
            auth_unread: None,
            asking_held: Vec::new(),
            state: Ok(State::default()),
            in_use: Some(in_use_as_named()),
            parks: Vec::new(),
            interrupted: false,
            stuck: None,
            codex: no_codex(),
            claude_present: true,
            schedule: None,
            network: direct(),
            in_the_app: false,
            elevation: crate::host::Elevation::Normal,
            fallback_login: None,
            floor: crate::host::Floor::Met,
            now: NOW,
        }
    }

    /// What [`facts`] holds of whose login Claude Code has stored: the login in its
    /// keychain, `acc`'s in `org`, as Anthropic named it a minute ago, with the config naming
    /// it too.
    fn in_use_as_named() -> crate::in_use::Known {
        let owner = crate::api::Owner {
            account_uuid: "acc".into(),
            email: "a@b.c".into(),
            organization_uuid: "org".into(),
        };
        let stored = json!({"claudeAiOauth": {"refreshToken": "r"}});
        crate::in_use::Known {
            last: Some(crate::in_use::InUse {
                owner: Some(owner.clone()),
                login: crate::provider::of(ProviderId::Claude).fingerprint(&stored),
                known_at: NOW - 60,
                named: Some(crate::state::new_id(ProviderId::Claude, &owner)),
            }),
            doubt: None,
            own_record: Some(owner),
        }
    }

    /// A machine that has never run Codex.
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
        }
    }

    /// Where Pitboard does not read every layer of Claude Code's settings, as it does not
    /// read the managed ones on Windows yet, the check says which beside its answer, and
    /// still passes: nothing it read says anything else.
    #[test]
    fn the_auth_check_says_what_it_did_not_read() {
        let mut facts = facts();
        assert_eq!(
            judge_auth(&facts).detail,
            "the stored login, which is what Pitboard moves"
        );
        facts.auth_unread = Some("Claude Code's managed settings are not read on Windows yet");
        let check = judge_auth(&facts);
        assert_eq!(check.level, Level::Ok);
        assert_eq!(
            check.detail,
            "the stored login, which is what Pitboard moves, as far as it reads: Claude Code's \
             managed settings are not read on Windows yet"
        );
    }

    /// Advice to make something private ends with the command that does, where Pitboard can
    /// say one, and with the sentence alone where it cannot, as on Windows until it reads who
    /// may read a file.
    #[test]
    fn advice_names_a_command_only_where_there_is_one() {
        assert_eq!(
            running(Os::MacOs.make_private_command(Kind::File, &["/a"])),
            ": `chmod 600 /a`."
        );
        assert_eq!(
            running(Os::Linux.make_private_command(Kind::Directory, &["/a", "/b"])),
            ": `chmod 700 /a /b`."
        );
        assert_eq!(Os::Windows.make_private_command(Kind::Any, &["/a"]), None);
        assert_eq!(running(None), ".");
    }

    /// Codex's section for a machine with no Codex accounts parked.
    fn codex_checks(codex: &CodexFacts) -> Vec<Check> {
        judge_codex(crate::host::OS, codex, &[], NOW)
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
    #[cfg_attr(windows, ignore = "W15: files made private to the person on Windows")]
    fn a_world_readable_park_is_found_on_the_disk() {
        use crate::host::fs::testing;

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
        assert_eq!(Some(&found[0].1), crate::host::fs::access(&park).as_ref());
        assert!(found[0].0.ends_with("pitboard-park-x.json"), "{found:?}");

        testing::make_private(&park).expect("made private");
        assert!(loose_logins(&ctx).is_empty(), "a private park is not loose");
    }

    /// On a machine with no keychain every parked login is a plaintext OAuth token in a
    /// file, and the only thing between it and everyone else with an account here is a
    /// mode bit. A backup restore, a `cp -r`, an rsync or a careless umask changes one
    /// quietly, and nothing else in Pitboard would ever mention it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W15: who else can read a login on Windows, and the command that makes one private"
    )]
    fn a_login_anyone_on_this_machine_can_read_is_a_failure() {
        let mut f = facts();
        assert_eq!(check(&evaluate(&f), "private_on_disk").level, Level::Ok);

        f.readable_by_others = vec![
            (
                "/home/a/.pitboard/vault/pitboard-park-x.json".into(),
                mode(0o644),
            ),
            ("/home/a/.claude/.credentials.json".into(), mode(0o640)),
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
             with it: `rm /home/x/.claude/.credentials.json`."
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

    /// What the checks above are given, read off the stores: the file while the keychain
    /// holds the login in use, with the login in it, none, or why it cannot be read, and
    /// nothing where the file is the store in use.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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

    /// Claude Code's file is the whole answer only where it has no other store: on Linux. On
    /// macOS it is a fallback after a keychain write failed, and on Windows Credential
    /// Manager may hold the login in use instead, which Pitboard does not read yet.
    #[test]
    fn a_file_store_is_the_whole_answer_only_on_linux() {
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
            (
                Os::Windows,
                Level::Warn,
                "Claude Code may keep its login in Credential Manager instead, which Pitboard \
                 does not read yet.",
            ),
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
    fn a_schedule_is_judged_by_whether_the_pitboard_it_runs_is_still_there() {
        let mut f = facts();
        assert!(
            evaluate(&f).iter().all(|c| c.code != "schedule"),
            "nothing is said where there is no schedule"
        );

        f.schedule = Some(ScheduleFact {
            path: PathBuf::from("/home/x/Library/LaunchAgents/com.datlechin.pitboard.renew.plist"),
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
            path: PathBuf::from("/home/x/Library/LaunchAgents/com.datlechin.pitboard.renew.plist"),
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

    /// The way back works from the command line everywhere, and names the app only where
    /// there is one.
    #[test]
    fn a_broken_schedule_is_written_again_by_whatever_this_machine_has() {
        let mac = again(Os::MacOs);
        assert!(
            mac.contains("the app's Settings") && mac.contains("pitboard schedule install"),
            "{mac}"
        );
        let linux = again(Os::Linux);
        assert!(linux.contains("pitboard schedule install"), "{linux}");
        assert!(!linux.contains("app"), "there is no app here: {linux}");
    }

    /// What the check above is given, read off a real disk: a schedule written the way
    /// `pitboard schedule install` writes it, whose Pitboard is then taken away.
    #[test]
    #[cfg_attr(windows, ignore = "W25: Task Scheduler")]
    fn a_schedule_whose_pitboard_is_gone_is_found_on_the_disk() {
        let root = std::env::temp_dir().join(format!(
            "pitboard-doctor-schedule-{}-{:?}",
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
        std::fs::create_dir_all(&root).expect("a scratch home");

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

    /// A schedule that runs a Pitboard still there, installed with `network`.
    fn scheduled(network: NetworkFact, network_differs: bool) -> ScheduleFact {
        ScheduleFact {
            path: PathBuf::from("/home/x/.config/systemd/user/pitboard-renew.timer"),
            program: Some(PathBuf::from("/usr/local/bin/pitboard")),
            program_found: true,
            network,
            network_differs,
        }
    }

    /// A network whose requests go through `proxy`.
    fn by(proxy: ProxyFact) -> NetworkFact {
        NetworkFact {
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
    #[cfg_attr(windows, ignore = "W25: Task Scheduler")]
    fn the_schedules_proxy_is_read_from_what_install_wrote() {
        let root = std::env::temp_dir().join(format!(
            "pitboard-doctor-schedule-proxy-{}-{:?}",
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
        let program = root.join("bin/pitboard");
        std::fs::create_dir_all(root.join("bin")).expect("a bin");
        std::fs::write(&program, "").expect("a Pitboard");
        let machine = crate::host::memory::MemoryHost::new();
        let home = root.to_string_lossy().into_owned();
        let run = |proxy: Option<&str>| {
            let env: crate::context::Environment = [("HOME", home.as_str())]
                .into_iter()
                .chain(proxy.map(|address| ("HTTPS_PROXY", address)))
                .collect();
            Context::for_command_line(&env)
                .with_memory_stores(std::sync::Arc::clone(&machine))
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
        f.readable_by_others = vec![("/home/a/.claude/.credentials.json".into(), mode(0o644))];
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
            exempting,
            exempt: exempt.iter().map(|host| (*host).to_string()).collect(),
        }
    }

    /// The network check, for a network that is `proxy`, passing over `passed_over`.
    fn network(proxy: Option<ProxyFact>, passed_over: &[&'static str]) -> Check {
        let mut f = facts();
        f.network = NetworkFact {
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
        unusable.unreadable = Some(Unreadable::Broken("missing from the vault".into()));
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
            discarded: vec!["pitboard-park-x-1".into()],
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
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn an_account_is_active_by_its_own_tools_record() {
        use crate::host::memory::MemoryHost;

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
        let live = crate::provider::of(ProviderId::Codex)
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

    /// The account in use is the one Anthropic last named for the login stored, known by
    /// that login's fingerprint, and doctor says when it was named. Claude Code's config
    /// naming another, once a read has found the login is still the one named, is said, and
    /// moves nothing.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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

    /// A machine that has never run Codex reads exactly as it did before Pitboard knew
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
        // The way back is signing in: a switch refuses while nothing is signed in, so
        // advice to switch would send somebody to a command that fails.
        f.codex = CodexFacts {
            enrolled: 1,
            ..no_codex()
        };
        let checks = evaluate(&f);
        let file = check(&checks, "codex_auth_file");
        assert_eq!(file.level, Level::Warn);
        assert!(file.advice.contains("`codex login`"), "{}", file.advice);
        assert!(
            !file.advice.contains("pitboard use codex/"),
            "{}",
            file.advice
        );
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
        for store in ["keyring", "auto", "secrets"] {
            let found = judged(store, 1);
            let advice = &check(&found, "codex_backend").advice;
            assert!(advice.contains("will not touch"), "{advice}");
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

    /// What `/etc/codex` and the person's own config say is gathered as Codex reads it, and a
    /// store a requirement pins is said to be one no line of the person's own changes.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_store_an_administrator_pinned_is_gathered_and_said() {
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        host.administers(
            "/etc/codex/requirements.toml",
            "cli_auth_credentials_store = \"keyring\"\n",
        );
        let codex = CodexFacts {
            enrolled: 1,
            ..codex_facts(&ctx, None)
        };
        assert_eq!(codex.backend, "keyring");
        assert_eq!(
            codex.backend_setting,
            "pinned to `keyring` by /etc/codex/requirements.toml"
        );
        let checks = codex_checks(&codex);
        let found = check(&checks, "codex_backend");
        assert_eq!(found.level, Level::Fail);
        assert_eq!(
            found.detail,
            "keyring (pinned to `keyring` by /etc/codex/requirements.toml)"
        );
        assert!(
            found.advice.ends_with(
                "No line in your own config.toml can change it: \
                 /etc/codex/requirements.toml pins it, and only an administrator can change \
                 that."
            ),
            "{}",
            found.advice
        );

        host.administers("/etc/codex/requirements.toml", "x = \n");
        let unknown = codex_facts(&ctx, None);
        assert_eq!(unknown.backend, "unknown");
        assert_eq!(
            unknown.backend_setting,
            "/etc/codex/requirements.toml is not TOML Codex can read, at line 1"
        );
        assert_eq!(
            unknown.backend_remedy,
            "Codex 0.160.0 does not start until that is put right"
        );
        assert!(matches!(unknown.login, Ok(None)), "nothing is read");
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
    #[cfg_attr(windows, ignore = "W15: files made private to the person on Windows")]
    fn a_version_is_read_out_of_each_way_codex_is_installed() {
        use crate::host::fs::testing;

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

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W15: who else can read a login on Windows, and the command that makes one private"
    )]
    fn a_codex_login_anybody_can_read_or_nobody_can_parse_is_said() {
        let mut codex = with_codex();
        codex.auth_access = Some(mode(0o644));
        let checks = codex_checks(&codex);
        let file = check(&checks, "codex_auth_file");
        assert_eq!(file.level, Level::Warn);
        assert!(file.detail.contains("mode 644"), "{}", file.detail);
        assert!(file.advice.contains("chmod 600"), "{}", file.advice);

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

    /// A running codex holds the account it started with for as long as it runs, which is
    /// the one thing a switch cannot reach. Said, never warned about: running it is the
    /// point of having it. What makes each kind take a switch is said beside it.
    #[test]
    fn running_codex_processes_are_reported_as_a_fact() {
        use crate::holder::classify;
        use crate::host::Process;
        let at = |pid: u32, path: &str| Process {
            pid,
            path: PathBuf::from(path),
        };
        let holders = crate::provider::codex::holders::HOLDERS;
        let mut codex = with_codex();
        codex.running = Some(classify(&[at(4321, "codex"), at(99, "codex")], holders));
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
        codex.running = Some(classify(&many, holders));
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
        let ctx =
            Context::new(std::env::temp_dir()).with_memory_stores(std::sync::Arc::clone(&host));
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
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn codex_facts_are_read_off_the_disk() {
        use crate::host::fs::testing;

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
        assert_eq!(absent.auth_access, None);
        assert!(matches!(absent.login, Ok(None)));
        assert_eq!(absent.program, None, "the one named, and it is not there");
        assert_eq!(absent.version, None);

        let installed = root.join("releases/0.154.0-aarch64-apple-darwin/bin/codex");
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
        let config = home.join("config.toml");
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

    /// A machine that uses only Codex is not told Claude Code is broken, nor to run a
    /// program it does not use. Everything about Pitboard itself is still checked.
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
            "Pitboard's own still is"
        );
        assert!(checks.iter().any(|c| c.code.starts_with("codex_")));
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
        let codes: Vec<&str> = run(&ctx).checks.iter().map(|c| c.code).collect();
        assert_eq!(codes, ["system_too_old", "elevated", "homes"]);
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
    #[cfg_attr(windows, ignore = "W14: Pitboard's own folder on Windows")]
    fn doctor_checks_nothing_under_a_home_that_is_not_a_full_path() {
        use crate::context::Environment;
        for (pairs, said) in [
            (&[("HOME", "")][..], "HOME is empty"),
            (
                &[("HOME", "/Users/x"), ("CODEX_HOME", "codex")],
                "CODEX_HOME is `codex`, which is not a full path",
            ),
        ] {
            let env: Environment = pairs.iter().copied().collect();
            let ctx = Context::for_command_line(&env)
                .with_memory_stores(crate::host::memory::MemoryHost::new());
            let diagnosis = run(&ctx);
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
