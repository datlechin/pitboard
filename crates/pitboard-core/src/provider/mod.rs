//! The boundary between pitboard's own machinery and one particular coding tool's login.
//!
//! pitboard was written against Claude Code, and for a long time that was the whole of it:
//! the keychain slot hashing, the five keys a logout deletes, the write lock and its
//! constants, the config file whose identity cache has to be spliced after a switch. None
//! of that is a fact about parking a login. It is a fact about Claude Code.
//!
//! This module names the small set of things that genuinely differ between one tool and
//! the next, so the rest of the crate can stop knowing which tool it is serving.
//!
//! # What is here and what deliberately is not
//!
//! Five operations: read the live credential, learn whose it is, measure what it has left,
//! renew it, install it. Every one of them is something the engine has to call without
//! caring how it is done underneath, and every one was checked against all three tools'
//! measured shapes before it was written down rather than derived from Claude Code alone.
//! [`Provider::usage`] takes the whole credential and the context rather than a bare access
//! token for exactly that reason: Gemini's quota call needs a project id out of a second
//! file that has nothing to do with the token, and a signature that looked sufficient after
//! Claude and Codex would have been wrong.
//!
//! Three facts, as values rather than code paths: [`Adoption`], [`ParkSemantics`],
//! [`Isolation`]. These are things the engine and the front ends must branch on, and a
//! value lets them branch on the fact instead of on the provider's name. Nothing anywhere
//! should read `if provider == Claude`.
//!
//! Four pure functions over a login document: which part of it belongs to the account,
//! how to put another account's part in, a non-secret handle on its refresh token, and when
//! it stops working. These are here because pitboard's own bookkeeping needs them and they
//! are genuinely different per tool: Claude Code's login sits in a document the machine
//! shares with unrelated keys, while Codex and Gemini keep one account per file. They were
//! not in the first sketch of this trait, which is how it came to be a boundary nothing
//! could actually park through.
//!
//! What is not here: a `park` method. Parking is pitboard's own bookkeeping, built out of
//! the pieces above, and a method for it would have to hide the difference between splicing
//! a shared document and replacing a whole file behind a flag. Also absent: Claude Code's
//! config-file identity cache, its supervisor daemon, its status line hook. A method most
//! implementations no-op is a sign it does not belong on a shared trait.

pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod jwt;

use crate::context::Context;
use crate::usage;
use serde_json::Value;

/// Which tool's login this is.
///
/// `non_exhaustive` from the first day it exists, while there is still only one variant, so
/// every caller outside this crate is made to write a fallback arm before there is a second
/// variant to catch them out.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProviderId {
    Claude,
    Codex,
}

impl ProviderId {
    /// Every provider pitboard knows, in the order a listing shows them.
    ///
    /// Named rather than written out at each call site, because resolving a bare label has
    /// to look at all of them and a provider missing from one such list would simply never
    /// be found, with nothing failing to say so.
    pub const ALL: &'static [ProviderId] = &[ProviderId::Claude, ProviderId::Codex];

    /// The one spelling used in a label prefix, in the state file, in a park's name and in
    /// the audit log. Written once so those four cannot drift, and chosen from the command
    /// a person types rather than the company behind it, because the command is the thing
    /// that is stable.
    pub fn code(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
            ProviderId::Codex => "codex",
        }
    }

    /// The service behind the tool, as a person would name it.
    ///
    /// Not the same as the tool: `claude` talks to Anthropic, `codex` to OpenAI. Messages
    /// about a failed request name this, because "could not reach OpenAI" is something
    /// somebody can act on and "could not reach the service" is not.
    pub fn service(self) -> &'static str {
        match self {
            ProviderId::Claude => "Anthropic",
            ProviderId::Codex => "OpenAI",
        }
    }

    /// The tool, as its own documentation names it.
    pub fn name(self) -> &'static str {
        match self {
            ProviderId::Claude => "Claude Code",
            ProviderId::Codex => "Codex",
        }
    }

    /// The command a person types to run the tool.
    pub fn program(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
            ProviderId::Codex => "codex",
        }
    }

    /// The environment variable that moves where the tool keeps its login.
    pub fn home_variable(self) -> &'static str {
        match self {
            ProviderId::Claude => "CLAUDE_CONFIG_DIR",
            ProviderId::Codex => "CODEX_HOME",
        }
    }

    /// What a person runs to sign in with the tool's own command. Claude Code signs in from
    /// inside the program it starts; Codex has a subcommand for it.
    pub fn login_command(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
            ProviderId::Codex => "codex login",
        }
    }

    pub fn parse(code: &str) -> Option<ProviderId> {
        match code {
            "claude" => Some(ProviderId::Claude),
            "codex" => Some(ProviderId::Codex),
            _ => None,
        }
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// One tool's login document, carried without being understood.
///
/// The provider it came from travels with it, so a value crossing this boundary can always
/// say which shape it is, and a credential can never be handed to the wrong engine by
/// accident. What is inside is that engine's business and nothing else's.
#[derive(Debug, Clone, PartialEq)]
pub struct Credential {
    pub provider: ProviderId,
    pub raw: Value,
}

impl Credential {
    pub fn new(provider: ProviderId, raw: Value) -> Credential {
        Credential { provider, raw }
    }
}

/// Who a credential belongs to, as the tool's own service understands it.
///
/// How this is learned is deliberately not part of the answer. Claude Code's is a network
/// call to Anthropic on every switch, because its local config can lag the credential by a
/// day. Codex's is a local decode of the ID token it already holds. Gemini's is a local
/// decode when the token carries one and a network call when it does not. The engine wants
/// the answer, not the method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Stable for the life of the account. A UUID for Claude, a UUID for Codex's
    /// `chatgpt_account_id`, Google's `sub` for Gemini.
    pub account_id: String,
    pub email: String,
    /// Claude's organisation, Codex's ChatGPT workspace, Gemini's project. `None` where the
    /// tool has no such concept or did not say, which is not the same as an empty one.
    pub group: Option<String>,
}

/// Why an answer about a login could not be had.
///
/// Every variant says whether asking again later could answer differently, because that is
/// the difference between a switch that should wait and one that should stop.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("the session has expired")]
    Unauthorized,
    /// `retry_after` is what the service said to wait, in seconds, where it said anything.
    #[error("{service} is rate limiting this request")]
    RateLimited {
        service: &'static str,
        retry_after: Option<i64>,
    },
    #[error("could not reach {service}: {detail}")]
    Network {
        service: &'static str,
        detail: String,
    },
    #[error("{service} answered {status}")]
    Unexpected { service: &'static str, status: u16 },
    #[error("{service}'s answer was not understood: {detail}")]
    Malformed {
        service: &'static str,
        detail: String,
    },
    /// Refused for good: revoked, or already spent somewhere else.
    #[error("{service} no longer accepts this login")]
    InvalidGrant { service: &'static str },
    /// The credential is not the shape this provider stores.
    #[error("the stored login is not the shape {provider} keeps: {detail}")]
    ShapeUnexpected {
        provider: ProviderId,
        detail: String,
    },
    /// The tool is configured to keep its login somewhere pitboard does not handle.
    #[error("{reason}")]
    Unsupported {
        provider: ProviderId,
        reason: String,
    },
    /// The document holds no account's login at all, which is what signing out leaves in a
    /// document the machine also keeps other things in. Not a malformed login: none.
    #[error("nothing is signed in")]
    NoLogin { provider: ProviderId },
}

/// When a session that is already running picks a switch up.
///
/// Measured, not assumed, and it differs enough between tools that a single number would be
/// a lie for two of the three. Claude Code serves its credential from a 30 second cache, so
/// a session follows on its own. Codex caches for the life of the process, watches no file
/// and refuses a reload whose account id has changed; Gemini caches its client for the
/// process with no expiry. Neither ever notices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adoption {
    /// A session already running follows within this many seconds, with no action.
    PollingWithin(u32),
    /// Nothing follows until the program is started again. Never rendered as a countdown.
    RestartRequired { program: &'static str },
}

/// Whether a parked copy may exist while the same account is still live.
///
/// For Claude Code it may: the live document holds the machine's other keys too, and
/// nothing revokes for presenting either copy. For Codex it must not. `codex login` and
/// `codex logout` both revoke the stored refresh token at OpenAI before clearing it, so a
/// copy left live while its twin sits in the vault is a token the person's own next login
/// can kill in both places at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParkSemantics {
    /// A park may be a copy; the live credential can keep working.
    CopyWhileLive,
    /// There must never be two usable copies of one account's credential at rest on this
    /// machine, not even between two steps of a switch.
    MoveOnly,
}

/// Whether signing in to a second account in a private directory really leaves the live
/// login alone.
///
/// The trick pitboard uses for enrolment is to point the tool's own sign-in at a scratch
/// directory through its home variable, let it write there, and read back what it wrote.
/// That works for `CLAUDE_CONFIG_DIR` and for `CODEX_HOME`. It does not work for Gemini
/// when its optional keychain backend is in use: that backend's service and account names
/// are global constants which `GEMINI_CLI_HOME` does not namespace, so the "private"
/// sign-in would write over the live login instead of beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Isolation {
    /// A home override fully isolates a sign-in from the live credential.
    Isolated,
    /// It does not, and here is what to tell the person.
    NotIsolated { reason: String },
}

/// When a login stops working, in epoch seconds. `None` where the tool does not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Expiry {
    /// Until then its usage can be asked without renewing it first.
    pub access_expires_at: Option<i64>,
    /// Until then it can be restored at all.
    pub refresh_expires_at: Option<i64>,
}

/// Where one tool keeps its live login: the backends it reads, in the order it reads them,
/// and the name the login is filed under in each.
///
/// Every tool pitboard knows keeps its login this way: Claude Code in a keychain item with
/// a file behind it, Codex in a file or a keychain item depending on its configuration,
/// Gemini in a file. Handing the switch the store itself, rather than a pair of read and
/// write methods, is what lets one switch ask the questions a store answers the same way
/// for every tool: what is there byte for byte, what a write would cost, whether it held.
pub(crate) struct LiveStore {
    pub(crate) chain: crate::store::Live,
    pub(crate) service: String,
}

/// A store that could not be read, as the provider boundary reports it.
pub(crate) fn store_error(error: crate::store::Error) -> ProviderError {
    const STORE: &str = "this machine's credential store";
    match error {
        crate::store::Error::Malformed(detail) => ProviderError::Malformed {
            service: STORE,
            detail,
        },
        other => ProviderError::Network {
            service: STORE,
            detail: other.to_string(),
        },
    }
}

/// One coding tool's login, as the rest of pitboard needs to touch it.
///
/// Implementations live in `provider::<name>`. Nothing here knows about pitboard's state
/// file, its lock, its journal or its audit log: those are pitboard's own bookkeeping and
/// do not vary by tool.
pub(crate) trait Provider: Send + Sync + std::fmt::Debug {
    fn id(&self) -> ProviderId;

    /// Where this tool's live login is kept on this machine, right now.
    ///
    /// Resolved on every call and never cached: which backend holds the login depends on
    /// the tool's own configuration and home variables, and either can change between two
    /// commands. An error means this tool keeps nothing at rest here that pitboard could
    /// park, which is not the same as nothing being signed in.
    fn live(&self, ctx: &Context) -> Result<LiveStore, ProviderError>;

    /// The credential this tool would authenticate with right now.
    ///
    /// `Ok(None)` means nothing is signed in, which is an answer. A store that could not be
    /// read is an error and must never collapse into `None`: reading one as the other tells
    /// somebody their login is gone when it is merely unreadable.
    fn read_live(&self, ctx: &Context) -> Result<Option<Credential>, ProviderError> {
        let live = self.live(ctx)?;
        crate::store::read(&live.chain, &live.service)
            .map(|found| found.map(|raw| Credential::new(self.id(), raw)))
            .map_err(store_error)
    }

    /// Whose credential this is.
    fn identify(&self, ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError>;

    /// Whose credential this is, confirmed by the service still accepting it.
    ///
    /// The same answer as [`Provider::identify`] where that already asks the service, which
    /// it does for Claude Code. A tool whose login names its own account can say whose it
    /// is without anybody's agreement, and that is not enough before installing it: a login
    /// the service has stopped accepting would be switched to, read back, found present,
    /// and fail the next time the person ran the tool, with nothing parked to go back to.
    fn verify(&self, ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError> {
        self.identify(ctx, credential)
    }

    /// What this credential has left, normalised into pitboard's own shape.
    ///
    /// Takes the whole credential and the context, not an access token, because what a
    /// usage call needs is not the same everywhere: Codex sends an account id header it
    /// reads out of the credential, and Gemini needs a project id the credential never
    /// mentions.
    fn usage(
        &self,
        ctx: &Context,
        credential: &Credential,
    ) -> Result<usage::Snapshot, ProviderError>;

    /// Fresh tokens for a parked login.
    ///
    /// Only ever called on a park. Renewing what is signed in is the tool's own job, and
    /// racing it there is how a refresh chain gets spent twice.
    fn renew(&self, ctx: &Context, credential: &Credential) -> Result<Credential, ProviderError>;

    /// A name for where this tool's live login is on this machine right now.
    ///
    /// One state file serves every place a tool can keep its login, and a home variable
    /// changes which one is live, so a record of which account was switched to in one
    /// says nothing about another. Claude Code's is the keychain item its directory hashes
    /// to; Codex's is the file its home puts the login in.
    fn slot(&self, ctx: &Context) -> String;

    /// The lock this tool takes around its own writes to the live login, which pitboard
    /// must hold too while it writes there. `None` for a tool that takes none, where there
    /// is nothing to hold and nothing it could wait for.
    fn write_lock(&self, ctx: &Context) -> Option<std::path::PathBuf>;

    /// Who this tool itself says is signed in, read from its own files without asking
    /// anybody.
    ///
    /// A cache for a tool that keeps one apart from its login, and can lag it; the login's
    /// own claims for a tool whose login names its account. Good enough to decide which of
    /// two messages to show and whether an account may be forgotten, never good enough to
    /// file a login under.
    fn recorded_identity(&self, ctx: &Context) -> Option<Identity>;

    /// Correct whatever this tool caches about who is signed in, now that `incoming`'s
    /// login is live in place of `outgoing`'s.
    ///
    /// Runs after the login has moved and cannot undo it, so a failure here is reported
    /// and never rolled back: the tool would otherwise name an account whose login is no
    /// longer there.
    fn after_switch(
        &self,
        ctx: &Context,
        incoming: &crate::state::Account,
        outgoing: &Identity,
    ) -> Result<(), crate::error::Error>;

    /// The tool's own program, where it is installed. A private sign-in runs it, so its
    /// absence is worth saying before anybody opens a browser.
    fn program(&self, ctx: &Context) -> Option<std::path::PathBuf>;

    /// The tool's own sign-in, pointed at `dir` so the live login is never touched.
    ///
    /// `dir` exists and is private when this is called, and it is where the tool writes the
    /// new login: every tool pitboard handles lets a home variable move its whole store,
    /// which is the only reason a second account can be signed in without signing the first
    /// one out. Whether that really isolates the live login is
    /// [`Provider::private_signin_isolation`]'s question, asked first.
    fn sign_in(&self, ctx: &Context, dir: &std::path::Path) -> std::process::Command;

    /// The login a sign-in left in `dir`, as the tool stored it.
    fn read_signin(
        &self,
        ctx: &Context,
        dir: &std::path::Path,
    ) -> Result<Option<String>, crate::store::Error>;

    /// Take away whatever a sign-in into `dir` left outside it. The directory itself is the
    /// caller's to remove.
    fn discard_signin(&self, ctx: &Context, dir: &std::path::Path);

    /// Names of whatever on this machine makes the tool sign in with something other than
    /// the login pitboard moves: an environment variable or a setting holding a key of its
    /// own. Read from files as well as this process's environment, so the app, which has no
    /// shell environment at all, gets the same answer as the command line.
    fn overridden_by(&self, ctx: &Context) -> Vec<String>;

    /// When a running session follows a switch. A fact about the tool, not a setting.
    fn adoption(&self) -> Adoption;

    /// Whether a park may coexist with the same account still live.
    fn park_semantics(&self) -> ParkSemantics;

    /// Whether a private sign-in on this machine, right now, is really private.
    ///
    /// Takes the context because the answer is not a constant: it depends on which backend
    /// the tool is configured to use here.
    fn private_signin_isolation(&self, ctx: &Context) -> Isolation;

    /// The part of a live document that belongs to the account signed in.
    ///
    /// For Claude Code that is a slice: its credential document also holds MCP tokens and
    /// other keys that belong to the machine, and parking those would take them away from
    /// whoever switches in. For a tool that keeps one account per file it is the whole
    /// document.
    fn slice(&self, live: &Value) -> Result<Value, ProviderError>;

    /// `live` with `incoming` in place of whatever account was there, and nothing of the
    /// outgoing account left behind.
    fn splice(&self, live: &Value, incoming: &Value) -> Result<Value, ProviderError>;

    /// A short, non-secret handle on the refresh token inside a slice.
    ///
    /// Two slices with the same handle hold the same refresh chain. It is what lets an
    /// interrupted switch work out which side landed without asking anybody.
    fn fingerprint(&self, slice: &Value) -> String;

    /// When a slice stops being askable and stops being restorable.
    fn expiry(&self, slice: &Value) -> Expiry;
}

/// Where a program somebody named is: the path itself when it has a directory in it, or
/// the first match on `search`, a list in `PATH`'s form, the way a shell would look.
pub(crate) fn find_program(
    named: &std::path::Path,
    search: &std::ffi::OsStr,
) -> Option<std::path::PathBuf> {
    if named.components().count() > 1 {
        return std::fs::metadata(named)
            .is_ok()
            .then(|| named.to_path_buf());
    }
    std::env::split_paths(search)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(named))
        .find(|candidate| std::fs::metadata(candidate).is_ok())
}

/// Where `tool`'s own program is, looked for the way the context says to look.
pub(crate) fn program_of(ctx: &Context, tool: ProviderId) -> Option<std::path::PathBuf> {
    find_program(ctx.program_for(tool), &ctx.search_path())
}

/// A command that runs `tool`'s own program, by the path it was found at, with that
/// program's own directory first on its `PATH`.
///
/// An npm install is a script that starts `#!/usr/bin/env node`, and npm puts it beside the
/// `node` that installed it, under whatever prefix or version manager that was. So the
/// program's own directory is where its interpreter is, even for an app whose `PATH` has
/// neither. The directory as found, never the script it links to: npm links
/// `<prefix>/bin/codex` to a file deep inside `lib/node_modules`, where no `node` is.
///
/// A program that was not found is left to the search path, where starting it fails the
/// way a missing program does.
pub(crate) fn command(ctx: &Context, tool: ProviderId) -> std::process::Command {
    let search = ctx.search_path();
    let Some(program) = program_of(ctx, tool) else {
        let mut command = std::process::Command::new(ctx.program_for(tool));
        command.env("PATH", search);
        return command;
    };
    let mut path = std::ffi::OsString::new();
    if let Some(dir) = program.parent().filter(|d| !d.as_os_str().is_empty()) {
        path.push(dir);
        if !search.is_empty() {
            path.push(":");
        }
    }
    path.push(&search);
    let mut command = std::process::Command::new(program);
    command.env("PATH", path);
    command
}

/// The implementation for one tool.
///
/// An exhaustive match rather than a lookup, so a tool added to [`ProviderId`] and not to
/// here stops compiling instead of being silently absent.
pub(crate) fn of(provider: ProviderId) -> &'static dyn Provider {
    match provider {
        ProviderId::Claude => &claude::engine::Claude,
        ProviderId::Codex => &codex::engine::Codex,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The code is written into a label prefix, the state file, a park's name and the audit
    /// log. If it ever stopped round-tripping, a state file would load with an account
    /// nothing could name.
    #[test]
    fn every_provider_code_parses_back_to_itself() {
        for &id in ProviderId::ALL {
            assert_eq!(ProviderId::parse(id.code()), Some(id), "{id}");
            assert!(
                id.code().chars().all(|c| c.is_ascii_lowercase()),
                "{id} is not a plain lowercase code"
            );
        }
        assert_eq!(ProviderId::parse("nothing"), None);
    }

    /// `ALL` is what resolving a bare label walks. A provider missing from it would never
    /// be found and nothing would say so.
    #[test]
    fn every_provider_is_in_all() {
        // Exhaustive by construction: adding a variant without adding it here stops
        // compiling, which is the point.
        for &id in ProviderId::ALL {
            match id {
                ProviderId::Claude | ProviderId::Codex => {}
            }
        }
        assert_eq!(ProviderId::ALL.len(), 2, "add the new provider to ALL");
    }

    /// A code is also what serde writes, so the two spellings must not drift.
    #[test]
    fn the_code_is_what_serde_writes() {
        for &id in ProviderId::ALL {
            let written = serde_json::to_value(id).expect("a provider id serialises");
            assert_eq!(written, serde_json::json!(id.code()), "{id}");
        }
    }

    /// A registry entry pointing at the wrong implementation would be silent: every
    /// account of that tool would be handled by another tool's rules.
    #[test]
    fn every_implementation_agrees_about_which_tool_it_is() {
        for &id in ProviderId::ALL {
            assert_eq!(of(id).id(), id, "{id} is registered against another tool");
        }
    }

    /// The three facts, asserted against what was measured, so a change to one is a change
    /// to a test rather than a surprise on somebody's machine.
    #[test]
    fn claude_code_follows_a_switch_on_its_own_and_tolerates_a_copy() {
        let claude = of(ProviderId::Claude);
        assert_eq!(
            claude.adoption(),
            Adoption::PollingWithin(33),
            "measured: a session serves its credential from a 30 second cache"
        );
        assert_eq!(
            claude.park_semantics(),
            ParkSemantics::CopyWhileLive,
            "nothing of Claude Code's revokes for presenting either copy, and the live              document holds the machine's other keys"
        );
        assert_eq!(
            claude.private_signin_isolation(&Context::from_env()),
            Isolation::Isolated,
            "CLAUDE_CONFIG_DIR picks the keychain item by hashing the directory, and there              is no second backend that escapes it"
        );
    }

    /// A restart-required provider has no number of seconds to show, and a caller that
    /// treated one as zero would render "follows in 0 seconds", which is the opposite of
    /// what is true.
    #[test]
    fn a_restart_is_not_a_countdown_of_zero() {
        let restart = Adoption::RestartRequired { program: "codex" };
        assert_ne!(restart, Adoption::PollingWithin(0));
        assert!(matches!(Adoption::PollingWithin(33), Adoption::PollingWithin(s) if s == 33));
    }

    /// A scratch directory standing in for an npm prefix's `bin`, with an empty file for
    /// each tool's program. Nothing here is ever run.
    struct Prefix(std::path::PathBuf);

    impl Prefix {
        fn new(name: &str) -> Prefix {
            let root = std::env::temp_dir().join(format!(
                "pitboard-search-path-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            let bin = root.join("npm/bin");
            std::fs::create_dir_all(&bin).expect("a scratch prefix");
            for &tool in ProviderId::ALL {
                std::fs::write(bin.join(tool.program()), "").expect("a program");
            }
            Prefix(root)
        }

        fn bin(&self) -> std::path::PathBuf {
            self.0.join("npm/bin")
        }
    }

    impl Drop for Prefix {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn env_of<'a>(command: &'a std::process::Command, name: &str) -> Option<&'a std::ffi::OsStr> {
        command
            .get_envs()
            .find(|(key, _)| *key == name)
            .and_then(|(_, value)| value)
    }

    /// A program is looked for where the caller says, not on this process's own `PATH`: an
    /// app opened from Finder has only the system's directories there.
    #[test]
    fn a_program_is_looked_for_on_the_search_path_it_is_given() {
        let prefix = Prefix::new("find");
        let search = format!("/nowhere/at/all::{}", prefix.bin().display());
        let found = find_program(std::path::Path::new("codex"), search.as_ref());
        assert_eq!(found, Some(prefix.bin().join("codex")));
        assert_eq!(
            find_program(std::path::Path::new("ls"), search.as_ref()),
            None,
            "ls is on this process's PATH and not on the one given"
        );
        assert_eq!(
            find_program(std::path::Path::new("codex"), "".as_ref()),
            None,
            "an empty search path finds nothing, and is not the current directory"
        );
    }

    /// Every tool's sign-in runs the program found, by its full path, with the program's
    /// own directory first on `PATH`. An npm install's script names `node` through `env`,
    /// and `node` is beside it, in a directory an app opened from Finder does not have.
    #[test]
    fn a_sign_in_runs_the_program_found_with_its_own_directory_first_on_path() {
        let prefix = Prefix::new("sign-in");
        let search = "/nowhere/before";
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_search_path(format!("{search}:{}", prefix.bin().display()));
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        for &tool in ProviderId::ALL {
            let command = of(tool).sign_in(&ctx, dir);
            let program = prefix.bin().join(tool.program());
            assert_eq!(command.get_program(), program.as_os_str(), "{tool}");
            assert_eq!(
                env_of(&command, "PATH"),
                Some(
                    format!(
                        "{}:{search}:{}",
                        prefix.bin().display(),
                        prefix.bin().display()
                    )
                    .as_ref()
                ),
                "{tool}"
            );
            assert_eq!(of(tool).program(&ctx), Some(program), "{tool}");
        }
    }

    /// A program named outright is run from its own directory too, whatever the search
    /// path holds, which is how an app that found it somewhere else starts it.
    #[test]
    fn a_program_named_outright_is_run_with_its_own_directory_on_path() {
        let prefix = Prefix::new("named");
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_claude_program(prefix.bin().join("claude"))
            .with_codex_program(prefix.bin().join("codex"))
            .with_search_path("/usr/bin:/bin".into());
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        for &tool in ProviderId::ALL {
            let command = of(tool).sign_in(&ctx, dir);
            assert_eq!(
                command.get_program(),
                prefix.bin().join(tool.program()).as_os_str(),
                "{tool}"
            );
            assert_eq!(
                env_of(&command, "PATH"),
                Some(format!("{}:/usr/bin:/bin", prefix.bin().display()).as_ref()),
                "{tool}"
            );
        }
    }

    /// A program found nowhere is left to the search path, so starting it fails the way a
    /// missing program does rather than running whatever this process's `PATH` has.
    #[test]
    fn a_program_found_nowhere_is_left_to_the_search_path() {
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_search_path("/nowhere/at/all".into());
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        for &tool in ProviderId::ALL {
            let command = of(tool).sign_in(&ctx, dir);
            assert_eq!(command.get_program(), tool.program(), "{tool}");
            assert_eq!(
                env_of(&command, "PATH"),
                Some("/nowhere/at/all".as_ref()),
                "{tool}"
            );
            assert_eq!(of(tool).program(&ctx), None, "{tool}");
        }
    }

    /// Without a search path of its own, a context looks where this process would, which
    /// is what the command line has always done.
    #[test]
    fn the_search_path_is_this_processs_own_path_unless_given() {
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"));
        assert_eq!(
            ctx.search_path(),
            std::env::var_os("PATH").unwrap_or_default()
        );
        assert_eq!(
            ctx.with_search_path("/opt/tools/bin".into()).search_path(),
            "/opt/tools/bin"
        );
    }
}
