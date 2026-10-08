//! The boundary between Pitboard's own machinery and one particular coding tool's login.
//!
//! Pitboard was written against Claude Code, and for a long time that was the whole of it:
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
//! it stops working. These are here because Pitboard's own bookkeeping needs them and they
//! are genuinely different per tool: Claude Code's login sits in a document the machine
//! shares with unrelated keys, while Codex and Gemini keep one account per file. They were
//! not in the first sketch of this trait, which is how it came to be a boundary nothing
//! could actually park through.
//!
//! One pure function over what a tool's own sign-in prints, which an app shows while it
//! runs: the address to open and whether the tool waits for a code. Each tool prints its
//! own words, so each reads its own, and both apps get the same answer from
//! [`sign_in_view`].
//!
//! What is not here: a `park` method. Parking is Pitboard's own bookkeeping, built out of
//! the pieces above, and a method for it would have to hide the difference between splicing
//! a shared document and replacing a whole file behind a flag. Also absent: Claude Code's
//! config-file identity cache, its supervisor daemon, its status line hook. A method most
//! implementations no-op is a sign it does not belong on a shared trait.

pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod jwt;
pub mod names;
mod printed;

use crate::context::Context;
use crate::service::Permit;
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
    /// Every provider Pitboard knows, in the order a listing shows them.
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

    /// Pitboard's own environment variable naming the tool's program outright, in place of
    /// the one it would find: one installed where nothing looks, or a stand-in.
    pub fn program_variable(self) -> &'static str {
        match self {
            ProviderId::Claude => "PITBOARD_CLAUDE",
            ProviderId::Codex => "PITBOARD_CODEX",
        }
    }

    /// Where the tool's own installers put its program under `home`, for an app that has no
    /// shell's `PATH` to find it on. Each tool says its own, read from its own build.
    pub fn install_places(self, home: &std::path::Path) -> Vec<std::path::PathBuf> {
        match self {
            ProviderId::Claude => claude::paths::install_places(home),
            ProviderId::Codex => codex::paths::install_places(home),
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
    /// The tool is configured to keep its login somewhere Pitboard does not handle.
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
    ///
    /// `holders` names every kind of process that runs `program`, most particular first and
    /// ending with one that is anywhere, and what makes each take the switch: a terminal
    /// session and an app that runs the program for itself are started again differently.
    RestartRequired {
        program: &'static str,
        holders: &'static [crate::holder::Holder],
    },
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
/// The trick Pitboard uses for enrolment is to point the tool's own sign-in at a scratch
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
/// Every tool Pitboard knows keeps its login this way: Claude Code in a keychain item with
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

/// One coding tool's login, as the rest of Pitboard needs to touch it.
///
/// Implementations live in `provider::<name>`. Nothing here knows about Pitboard's state
/// file, its lock, its journal or its audit log: those are Pitboard's own bookkeeping and
/// do not vary by tool.
pub(crate) trait Provider: Send + Sync + std::fmt::Debug {
    fn id(&self) -> ProviderId;

    /// Where this tool's live login is kept on this machine, right now.
    ///
    /// Resolved on every call and never cached: which backend holds the login depends on
    /// the tool's own configuration and home variables, and either can change between two
    /// commands. An error means this tool keeps nothing at rest here that Pitboard could
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

    /// Whether [`Provider::identify`] answers from the credential alone, asking nobody.
    /// Where it does, whose a login is can be told by a read that sends no request, as
    /// `doctor` and `status --offline` are.
    fn identifies_by_itself(&self) -> bool;

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

    /// What this credential has left, normalised into Pitboard's own shape.
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
    ///
    /// The service spends the refresh token it is given as it answers, so this takes the
    /// [`Permit`] only the one gate every change passes makes: nothing rotates a token that
    /// could not then be written down.
    fn renew(
        &self,
        ctx: &Context,
        permit: Permit,
        credential: &Credential,
    ) -> Result<Credential, ProviderError>;

    /// A name for where this tool's live login is on this machine right now.
    ///
    /// One state file serves every place a tool can keep its login, and a home variable
    /// changes which one is live, so a record of which account was switched to in one
    /// says nothing about another. Claude Code's is the keychain item its directory hashes
    /// to; Codex's is the file its home puts the login in.
    fn slot(&self, ctx: &Context) -> String;

    /// The lock this tool takes around its own writes to the live login, which Pitboard
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
        permit: Permit,
        incoming: &crate::state::Account,
        outgoing: &Identity,
    ) -> Result<(), crate::error::Error>;

    /// The tool's own program, where it is installed. A private sign-in runs it, so its
    /// absence is worth saying before anybody opens a browser.
    fn program(&self, ctx: &Context) -> Option<std::path::PathBuf>;

    /// The tool's own sign-in, pointed at `dir` so the live login is never touched.
    ///
    /// `dir` exists and is private when this is called, and it is where the tool writes the
    /// new login: every tool Pitboard handles lets a home variable move its whole store,
    /// which is the only reason a second account can be signed in without signing the first
    /// one out. Whether that really isolates the live login is
    /// [`Provider::private_signin_isolation`]'s question, asked first.
    ///
    /// Starting it changes things a run that may change nothing must not: `codex login`
    /// revokes whatever login is in the home it is given before it signs in. So the command
    /// is made only with the [`Permit`] only the one gate every change passes makes.
    fn sign_in(
        &self,
        ctx: &Context,
        permit: Permit,
        dir: &std::path::Path,
    ) -> std::process::Command;

    /// The login a sign-in left in `dir`, as the tool stored it.
    fn read_signin(
        &self,
        ctx: &Context,
        dir: &std::path::Path,
    ) -> Result<Option<String>, crate::store::Error>;

    /// Take away whatever a sign-in into `dir` left outside it. The directory itself is the
    /// caller's to remove.
    fn discard_signin(&self, ctx: &Context, permit: Permit, dir: &std::path::Path);

    /// What the tool's own sign-in has printed so far, `said`, comes to: the address it gave
    /// for a browser that did not open by itself, and whether it waits for a code typed
    /// back. Everything said so far, because a tool's output arrives in pieces that need
    /// not end where a line does.
    ///
    /// Whether a code was typed back already is Pitboard's to know, not the tool's, and
    /// [`sign_in_view`] adds it.
    fn read_sign_in(&self, said: &str) -> SignInView;

    /// Whether what the tool's own sign-in said after a code was typed back, `since`,
    /// refuses that code and reads another in the same sign-in, so a person may type one
    /// again. Everything said since, for the same reason as [`Provider::read_sign_in`]. A
    /// tool that reads no code refuses none.
    fn refused_code(&self, since: &str) -> bool;

    /// Names of whatever on this machine makes the tool sign in with something other than
    /// the login Pitboard moves: an environment variable or a setting holding a key of its
    /// own. Read from files as well as this process's environment, so the app, which has no
    /// shell environment at all, gets the same answer as the command line.
    fn overridden_by(&self, ctx: &Context) -> Vec<String>;

    /// The file holding a login of the tool's behind the store in use: what a session that
    /// cannot read that store signs in with instead, and what no switch reaches. `None`
    /// where there is none, and where that cannot be told, which `doctor` says.
    fn fallback_login(&self, ctx: &Context) -> Option<std::path::PathBuf>;

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

/// What a tool's sign-in has printed so far comes to, for an app showing it while it runs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SignInView {
    /// The address the tool printed for a person to open when the browser did not open by
    /// itself, as printed, for the app to make a link of.
    pub url: Option<String>,
    /// Whether to offer a field for the code the browser shows, which the tool is waiting
    /// to have typed back.
    pub wants_code: bool,
}

/// What `provider`'s sign-in has printed so far, `said`, comes to. `pasted` is whether a
/// code has been typed back already, after which none is asked for.
pub fn sign_in_view(provider: ProviderId, said: &str, pasted: bool) -> SignInView {
    let read = of(provider).read_sign_in(said);
    SignInView {
        wants_code: read.wants_code && !pasted,
        ..read
    }
}

/// Whether `provider`'s sign-in refused the code typed back last, by what it said after it,
/// `since`, and reads another in its place.
pub fn refused_code(provider: ProviderId, since: &str) -> bool {
    of(provider).refused_code(since)
}

/// The first `https` address in what a tool printed, read as a terminal reads it: where a
/// hyperlink goes, or an address printed as text, up to where an address printed bare
/// cannot go on: white space, a quote or an angle bracket. No byte of an escape sequence is
/// ever part of one, and a hyperlink's text is read only after where it goes.
///
/// Only `https`: the address each tool prints for a person to open is one, and a loopback
/// address it prints, where the browser comes back to, is `http`. The registers record
/// both.
pub(crate) fn https_address(said: &str) -> Option<String> {
    printed::read(said).find_map(|piece| match piece {
        printed::Printed::Text(text) => bare_https_address(text),
        printed::Printed::Link(target) => target
            .strip_prefix(HTTPS)
            .is_some_and(|rest| !rest.is_empty() && !rest.contains(ends_an_address))
            .then(|| target.to_owned()),
    })
}

const HTTPS: &str = "https://";

/// The first `https` address in `text`, which has no escape sequence in it.
fn bare_https_address(text: &str) -> Option<String> {
    text.match_indices(HTTPS).find_map(|(at, _)| {
        let rest = &text[at + HTTPS.len()..];
        let length = rest.find(ends_an_address).unwrap_or(rest.len());
        (length > 0).then(|| text[at..at + HTTPS.len() + length].to_owned())
    })
}

/// White space is Unicode's, as `char::is_whitespace` has it, which is what the app's own
/// pattern ended an address at before this moved here. The pattern also ended one at ESC,
/// which never reaches here: `printed` takes every escape sequence out first.
fn ends_an_address(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>')
}

/// Where `tool`'s own program is, looked for the way the context says to look.
pub(crate) fn program_of(ctx: &Context, tool: ProviderId) -> Option<std::path::PathBuf> {
    crate::host::program::find(ctx.program_for(tool), &ctx.search_path())
}

/// A command that runs `tool`'s own program, by the path it was found at, with the search
/// path as its `PATH`, and the program's own directory in front of it when it is not on it
/// already.
///
/// An npm install is a script that starts `#!/usr/bin/env node`, and npm puts it beside the
/// `node` that installed it, under whatever prefix or version manager that was. So a
/// program found somewhere the search path does not reach, where an app finds one its
/// installer put there, finds its interpreter in its own directory, even for an app whose
/// `PATH` has neither. The directory as found, never the script it links to: npm links
/// `<prefix>/bin/codex` to a file deep inside `lib/node_modules`, where no `node` is. A
/// program found on the search path runs with that path as it is, so `env` finds the `node`
/// the person's own terminal would, and not an older one that happens to sit beside it.
///
/// A program that was not found is left to the search path, where starting it fails the
/// way a missing program does.
///
/// What it runs is a tool's sign-in, which changes what that tool keeps, so it is made only
/// with the [`Permit`] the one gate every change passes makes.
pub(crate) fn command(ctx: &Context, _: Permit, tool: ProviderId) -> std::process::Command {
    let search = ctx.search_path();
    let Some(program) = program_of(ctx, tool) else {
        let mut command = std::process::Command::new(ctx.program_for(tool));
        command.env("PATH", search);
        return command;
    };
    // An empty search path has no entries. Split, it would give one empty entry, and an
    // empty entry is the current directory.
    let entries: Vec<std::path::PathBuf> = if search.is_empty() {
        Vec::new()
    } else {
        std::env::split_paths(&search).collect()
    };
    let path = match program
        .parent()
        .filter(|dir| !entries.iter().any(|entry| entry == dir))
    {
        // Each entry was read out of a search path, so they join again as they were; only
        // the program's own directory could hold the separator, and then it is left off.
        Some(dir) => std::env::join_paths(std::iter::once(dir.to_path_buf()).chain(entries))
            .unwrap_or_else(|_| search.clone()),
        None => search.clone(),
    };
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
            claude.private_signin_isolation(&Context::for_unit_test()),
            Isolation::Isolated,
            "CLAUDE_CONFIG_DIR picks the keychain item by hashing the directory, and there              is no second backend that escapes it"
        );
    }

    /// A restart-required provider has no number of seconds to show, and a caller that
    /// treated one as zero would render "follows in 0 seconds", which is the opposite of
    /// what is true.
    #[test]
    fn a_restart_is_not_a_countdown_of_zero() {
        let restart = of(ProviderId::Codex).adoption();
        assert_ne!(restart, Adoption::PollingWithin(0));
        assert!(matches!(Adoption::PollingWithin(33), Adoption::PollingWithin(s) if s == 33));
    }

    /// A scratch directory standing in for an npm prefix's `bin`, with an empty file for
    /// each tool's program that anybody may run. Nothing here is ever run.
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
                let program = bin.join(tool.program());
                std::fs::write(&program, "").expect("a program");
                crate::host::fs::testing::make_runnable(&program).expect("runnable");
            }
            Prefix(root)
        }

        fn bin(&self) -> std::path::PathBuf {
            self.0.join("npm/bin")
        }

        /// A directory beside `bin` holding something named for each tool's program that
        /// cannot be run: a directory of that name, or a file with no execute bit.
        fn decoys(&self) -> (std::path::PathBuf, std::path::PathBuf) {
            let (dirs, files) = (self.0.join("dirs"), self.0.join("files"));
            for &tool in ProviderId::ALL {
                std::fs::create_dir_all(dirs.join(tool.program())).expect("a directory");
                std::fs::create_dir_all(&files).expect("a directory");
                std::fs::write(files.join(tool.program()), "").expect("a file");
            }
            (dirs, files)
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
        let found = crate::host::program::find(std::path::Path::new("codex"), search.as_ref());
        assert_eq!(found, Some(prefix.bin().join("codex")));
        assert_eq!(
            crate::host::program::find(std::path::Path::new("ls"), search.as_ref()),
            None,
            "ls is on this process's PATH and not on the one given"
        );
    }

    /// A directory named for the program, or a file of that name nobody may run, is passed
    /// over the way `execvp` passes over it, so what is found is what a sign-in can start.
    #[test]
    fn what_cannot_be_run_is_passed_over() {
        let prefix = Prefix::new("decoys");
        let (dirs, files) = prefix.decoys();
        let search = format!(
            "{}:{}:{}",
            dirs.display(),
            files.display(),
            prefix.bin().display()
        );
        assert_eq!(
            crate::host::program::find(std::path::Path::new("codex"), search.as_ref()),
            Some(prefix.bin().join("codex"))
        );
        for decoy in [dirs.join("codex"), files.join("codex")] {
            assert_eq!(
                crate::host::program::find(&decoy, "".as_ref()),
                None,
                "{}",
                decoy.display()
            );
        }
    }

    /// Every tool's sign-in runs the program found, by its full path. Found on the search
    /// path, it runs with that path as it is: its directory is on it already, and putting it
    /// first would only change which `node` an npm install's `env` finds from the one the
    /// person's own terminal finds.
    #[test]
    fn a_sign_in_runs_the_program_found_with_the_search_path_as_it_is() {
        let prefix = Prefix::new("sign-in");
        let search = format!("/nowhere/before:{}", prefix.bin().display());
        let ctx =
            Context::new(std::path::PathBuf::from("/nowhere")).with_search_path(search.clone());
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        for &tool in ProviderId::ALL {
            let command = of(tool).sign_in(&ctx, Permit::for_a_test(), dir);
            let program = prefix.bin().join(tool.program());
            assert_eq!(command.get_program(), program.as_os_str(), "{tool}");
            assert_eq!(env_of(&command, "PATH"), Some(search.as_ref()), "{tool}");
            assert_eq!(of(tool).program(&ctx), Some(program), "{tool}");
        }
    }

    /// A program named outright where the search path does not reach is run with its own
    /// directory first on `PATH`, which is how an app that found it where its installer
    /// puts it starts it: an npm install's script names `node` through `env`, and `node` is
    /// beside it. Named outright on the search path, it runs with that path as it is.
    #[test]
    fn a_program_named_outright_is_run_with_its_own_directory_on_path() {
        let prefix = Prefix::new("named");
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_claude_program(prefix.bin().join("claude"))
            .with_codex_program(prefix.bin().join("codex"))
            .with_search_path("/usr/bin:/bin".into());
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        for &tool in ProviderId::ALL {
            let command = of(tool).sign_in(&ctx, Permit::for_a_test(), dir);
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
        let on_it = format!("/usr/bin:{}/", prefix.bin().display());
        let ctx = ctx.with_search_path(on_it.clone());
        for &tool in ProviderId::ALL {
            let command = of(tool).sign_in(&ctx, Permit::for_a_test(), dir);
            assert_eq!(env_of(&command, "PATH"), Some(on_it.as_ref()), "{tool}");
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
            let command = of(tool).sign_in(&ctx, Permit::for_a_test(), dir);
            assert_eq!(command.get_program(), tool.program(), "{tool}");
            assert_eq!(
                env_of(&command, "PATH"),
                Some("/nowhere/at/all".as_ref()),
                "{tool}"
            );
            assert_eq!(of(tool).program(&ctx), None, "{tool}");
        }
    }

    /// The address to open is the first `https` one. Codex prints its loopback address
    /// first, and that is where the browser comes back to, not where a person goes. Codex
    /// reads nothing typed back, whatever it prints; Claude Code waits for a code once it
    /// asks for one.
    #[test]
    fn the_address_to_open_is_the_one_the_person_goes_to() {
        let mut said = String::from(
            "Starting local login server on http://localhost:1455.\n\
             If your browser did not open, navigate to this URL to authenticate:\n\n\
             https://auth.openai.com/oauth/authorize?response_type=code&state=x\u{1b}[0m\n",
        );
        assert_eq!(
            sign_in_view(ProviderId::Codex, &said, false).url.as_deref(),
            Some("https://auth.openai.com/oauth/authorize?response_type=code&state=x")
        );
        said.push_str("Paste code here if prompted > ");
        assert!(!sign_in_view(ProviderId::Codex, &said, false).wants_code);

        let said = "Paste code here if prompted > ";
        assert!(sign_in_view(ProviderId::Claude, said, false).wants_code);
    }

    /// A code field is offered only by a sign-in whose tool reads one, whatever the tool
    /// prints, and once a code has been typed back it is not asked for again.
    #[test]
    fn a_code_is_asked_for_only_where_the_tool_takes_one() {
        let said = "Paste code here if prompted > ";
        for (tool, takes) in [(ProviderId::Claude, true), (ProviderId::Codex, false)] {
            assert_eq!(sign_in_view(tool, said, false).wants_code, takes, "{tool}");
            assert!(
                !sign_in_view(tool, said, true).wants_code,
                "{tool}: asked once"
            );
        }
    }

    /// Before a tool has said anything there is nothing to open and nothing to type.
    #[test]
    fn a_sign_in_that_has_said_nothing_offers_nothing() {
        for &tool in ProviderId::ALL {
            assert_eq!(
                sign_in_view(tool, "", false),
                SignInView::default(),
                "{tool}"
            );
        }
    }

    /// An address printed bare ends where one cannot go on: white space, a quote, an angle
    /// bracket, or the escape that starts a terminal's colour. White space is Unicode's, as
    /// the app's own pattern had it: on 5 October 2026 that pattern ended an address at each
    /// of the first characters here, and went on past each of the others.
    #[test]
    fn an_address_ends_where_an_address_printed_bare_cannot_go_on() {
        let ends = [
            '\t', '\n', '\u{b}', '\u{c}', '\r', ' ', '"', '\'', '<', '>', '\u{1b}', '\u{85}',
            '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}', '\u{2028}', '\u{2029}', '\u{202f}',
            '\u{205f}', '\u{3000}',
        ];
        for end in ends {
            let said = format!("x https://a.b/c{end}d e");
            assert_eq!(
                https_address(&said).as_deref(),
                Some("https://a.b/c"),
                "{:?}",
                end
            );
        }
        for kept in ['\u{1c}', '\u{7f}', '\u{180e}', '\u{200b}', '\u{feff}'] {
            let said = format!("x https://a.b/c{kept}d e");
            assert_eq!(
                https_address(&said),
                Some(format!("https://a.b/c{kept}d")),
                "{:?}",
                kept
            );
        }
    }

    /// A scheme with nothing after it is no address, and the next one is looked for. An
    /// address that is not `https` is never the one to open.
    #[test]
    fn only_an_https_address_with_something_in_it_is_one_to_open() {
        assert_eq!(
            https_address("https:// then https://x.y").as_deref(),
            Some("https://x.y")
        );
        assert_eq!(https_address("http://localhost:1455/auth/callback"), None);
        assert_eq!(https_address("HTTPS://x.y"), None);
    }

    /// A hyperlink's address is where it goes, whatever its text says, and the escape
    /// sequences around it are no part of it. A hyperlink that goes somewhere other than an
    /// `https` address offers its text, as a terminal shows it.
    #[test]
    fn a_hyperlinks_address_is_where_it_goes() {
        let cases = [
            (
                "\u{1b}]8;;https://a.b/c\u{7}sign in\u{1b}]8;;\u{7}",
                "https://a.b/c",
            ),
            (
                "\u{1b}]8;id=1;https://a.b/c\u{1b}\\https://x.y\u{1b}]8;;\u{1b}\\",
                "https://a.b/c",
            ),
            (
                "\u{1b}]8;;http://localhost:1455\u{7}\u{1b}[94mhttps://x.y\u{1b}[39m\u{1b}]8;;\u{7}",
                "https://x.y",
            ),
            (
                "\u{1b}]8;;https://a.b/c d\u{7}https://x.y\u{1b}]8;;\u{7}",
                "https://x.y",
            ),
        ];
        for (said, address) in cases {
            assert_eq!(
                https_address(&format!("visit: {said}\n")).as_deref(),
                Some(address),
                "{said:?}"
            );
        }
    }

    /// What a terminal does not show is not an address to open, such as a window's title.
    #[test]
    fn an_address_a_terminal_does_not_show_is_none_to_open() {
        assert_eq!(https_address("\u{1b}]0;https://a.b/c\u{7}ready"), None);
    }

    /// Read part way through, as it arrives, a hyperlink offers nothing until where it goes
    /// has arrived whole, and then offers all of it.
    #[test]
    fn a_hyperlink_read_part_way_through_offers_all_of_its_address_or_none() {
        let said = "visit: \u{1b}]8;;https://a.b/c?d=e\u{7}https://a.b/c?d=e\u{1b}]8;;\u{7}\n";
        let whole = said.find('\u{7}').expect("the end of where it goes") + 1;
        for (at, _) in said.char_indices().chain([(said.len(), ' ')]) {
            let offered = https_address(&said[..at]);
            if at < whole {
                assert_eq!(offered, None, "{:?}", &said[..at]);
            } else {
                assert_eq!(offered.as_deref(), Some("https://a.b/c?d=e"), "{at}");
            }
        }
    }

    /// Without a search path of its own, a context looks where this process would, which
    /// is what the command line has always done.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "this process's own PATH is what the context is meant to read"
    )]
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
