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
//! Three facts, as values rather than code paths: [`Adoption`], `ParkSemantics`,
//! `Isolation`. These are things the engine and the front ends must branch on, and a
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
pub(crate) mod desktop;
pub(crate) mod folder;
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
    Desktop,
}

// What a tool that is a program a person runs has, which an app's login has not.
#[derive(Debug, Clone, Copy)]
pub struct Cli {
    // Pitboard's own variable naming the tool's program outright
    pub program_variable: &'static str,
    // where the tool's own installers put its program, as an app started with this
    // environment finds them
    pub install_places: fn(&crate::context::Environment) -> Vec<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignsIn {
    ByItsProgram,
    InItsApp,
}

impl ProviderId {
    /// Every provider Pitboard knows, in the order a listing shows them.
    ///
    /// Named rather than written out at each call site, because resolving a bare label has
    /// to look at all of them and a provider missing from one such list would simply never
    /// be found, with nothing failing to say so.
    pub const ALL: &'static [ProviderId] =
        &[ProviderId::Claude, ProviderId::Codex, ProviderId::Desktop];

    /// The one spelling used in a label prefix, in the state file, in a park's name and in
    /// the audit log. Written once so those four cannot drift, and chosen from the command
    /// a person types rather than the company behind it, because the command is the thing
    /// that is stable.
    pub fn code(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
            ProviderId::Codex => "codex",
            ProviderId::Desktop => "desktop",
        }
    }

    /// The service behind the tool, as a person would name it.
    ///
    /// Not the same as the tool: `claude` talks to Anthropic, `codex` to OpenAI. Messages
    /// about a failed request name this, because "could not reach OpenAI" is something
    /// somebody can act on and "could not reach the service" is not.
    pub const fn service(self) -> &'static str {
        match self {
            ProviderId::Claude | ProviderId::Desktop => "Anthropic",
            ProviderId::Codex => "OpenAI",
        }
    }

    /// The tool, as its own documentation names it.
    pub fn name(self) -> &'static str {
        match self {
            ProviderId::Claude => "Claude Code",
            ProviderId::Codex => "Codex",
            ProviderId::Desktop => "Claude Desktop",
        }
    }

    // The program a person runs, or opens and quits.
    pub fn program(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
            ProviderId::Codex => "codex",
            ProviderId::Desktop => "Claude",
        }
    }

    pub fn cli(self) -> Option<Cli> {
        match self {
            ProviderId::Claude => Some(Cli {
                program_variable: "PITBOARD_CLAUDE",
                install_places: claude::paths::install_places,
            }),
            ProviderId::Codex => Some(Cli {
                program_variable: "PITBOARD_CODEX",
                install_places: codex::paths::install_places,
            }),
            ProviderId::Desktop => None,
        }
    }

    pub fn signs_in(self) -> SignsIn {
        match of(self) {
            LoginKind::Document(_) => SignsIn::ByItsProgram,
            LoginKind::Folder(_) => SignsIn::InItsApp,
        }
    }

    // An app's login: no store of it is read and no service asked.
    pub fn reads_usage(self) -> bool {
        match of(self) {
            LoginKind::Document(_) => true,
            LoginKind::Folder(_) => false,
        }
    }

    pub fn supported_on(self, os: crate::host::Os) -> bool {
        use crate::host::Os;
        match (self, os) {
            (ProviderId::Claude | ProviderId::Codex, _) => true,
            (ProviderId::Desktop, Os::MacOs) => true,
            (ProviderId::Desktop, Os::Linux | Os::Windows) => false,
        }
    }

    pub fn parse(code: &str) -> Option<ProviderId> {
        match code {
            "claude" => Some(ProviderId::Claude),
            "codex" => Some(ProviderId::Codex),
            "desktop" => Some(ProviderId::Desktop),
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
pub(crate) struct Credential {
    pub(crate) provider: ProviderId,
    pub(crate) raw: Value,
    /// Whose the service said it is as it issued it, where it said.
    pub(crate) named: Option<Owner>,
}

impl Credential {
    pub(crate) fn new(provider: ProviderId, raw: Value) -> Credential {
        Credential {
            provider,
            raw,
            named: None,
        }
    }
}

/// Which login a credential is, as its service names it: the account, in its organisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) account_uuid: String,
    pub(crate) organization_uuid: String,
}

/// Whose a login is. Unlike a refresh-token fingerprint, it does not change when a tool
/// rotates the token.
///
/// How it is learned is not part of the answer: Claude Code's from Anthropic, since its
/// config can lag the credential by a day, and Codex's from the ID token it holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Owner {
    /// Anthropic's account uuid, or Codex's ChatGPT account joined with the person in it.
    pub account_uuid: String,
    pub email: String,
    /// Claude's organisation or Codex's ChatGPT account. Empty for an account outside an
    /// organisation, as Anthropic reports one.
    pub organization_uuid: String,
}

impl Owner {
    pub(crate) fn named(identity: Identity, email: String) -> Owner {
        Owner {
            account_uuid: identity.account_uuid,
            email,
            organization_uuid: identity.organization_uuid,
        }
    }

    pub(crate) fn identity(&self) -> Identity {
        Identity {
            account_uuid: self.account_uuid.clone(),
            organization_uuid: self.organization_uuid.clone(),
        }
    }

    /// Whether `other` is this login: the same account in the same organisation.
    pub(crate) fn same_login(&self, other: &Owner) -> bool {
        self.identity() == other.identity()
    }
}

/// Why an answer about a login could not be had.
///
/// Every variant says whether asking again later could answer differently, because that is
/// the difference between a switch that should wait and one that should stop.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ProviderError {
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
    /// Asked again, the service will answer the same.
    #[error("{service} answered {refusal}")]
    Refused {
        service: &'static str,
        refusal: Box<crate::api::Refusal>,
    },
    /// The service failed to answer, and may answer later.
    #[error("{service} answered {refusal}")]
    Unavailable {
        service: &'static str,
        refusal: Box<crate::api::Refusal>,
    },
    /// The service named the login's account and would not give its email, for `error`.
    #[error("{error}")]
    Unnamed { error: Box<ProviderError> },
    /// The account is on hold with its service: the login is kept, and works once that ends.
    #[error("{service} has put this account on hold{}", link.as_ref().map(|link| format!(" ({link})")).unwrap_or_default())]
    OnHold {
        service: &'static str,
        link: Option<String>,
    },
    #[error("{service}'s answer was not understood: {detail}")]
    Malformed {
        service: &'static str,
        detail: String,
    },
    /// Refused for good: revoked, or already spent somewhere else.
    #[error("{service} no longer accepts this login")]
    InvalidGrant { service: &'static str },
    // Refused as another client than the login's own, which says nothing of the login.
    #[error("{detail}")]
    OtherClient { detail: String },
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
    #[error("{}", crate::error::codex_home_missing(path))]
    CodexHomeMissing { path: std::path::PathBuf },
}

impl ProviderError {
    /// `service`'s answer in place of the one asked for: refused where asking again would be
    /// answered the same, unavailable otherwise.
    pub(crate) fn answered(service: &'static str, refusal: crate::api::Refusal) -> ProviderError {
        let refusal = Box::new(refusal);
        if refusal.is_final() {
            ProviderError::Refused { service, refusal }
        } else {
            ProviderError::Unavailable { service, refusal }
        }
    }
}

/// When a session that is already running picks a switch up.
///
/// Measured or read, not assumed, and it differs enough between tools that a single number
/// would be a lie for two of the three. Claude Code serves its credential from a 30 second
/// cache, so a session follows on its own, unless a file sits behind the keychain, which it
/// watches in place of the store. Codex caches for the life of the process, watches no file
/// and refuses a reload whose account id has changed; Gemini caches its client for the
/// process with no expiry. Neither ever notices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adoption {
    /// A session already running follows within this many seconds, with no action.
    PollingWithin(u32),
    /// A session already running keeps the login it holds until that login is next renewed,
    /// or until it is started again, because `file` sits behind the store the switch wrote.
    /// Never rendered as a countdown: when a session's login is renewed is the session's.
    AtRenewal {
        file: std::path::PathBuf,
    },
    /// Nothing follows until the program is started again. Never rendered as a countdown.
    ///
    /// `holders` names every kind of process that runs `program`, most particular first and
    /// ending with one that is anywhere, and what makes each take the switch: a terminal
    /// session and an app that runs the program for itself are started again differently.
    RestartRequired {
        program: &'static str,
        holders: &'static [crate::holder::Holder],
    },
    // The app was closed for the change, and holds the login from when it is next opened.
    NextOpen {
        app: &'static str,
    },
}

/// A file of a tool's behind the store that holds the login in use, which no switch reaches.
/// What it holds can matter less than that it is there: a session can watch it by a look,
/// whatever is in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Behind {
    pub path: std::path::PathBuf,
    pub held: Held,
}

/// What a file behind a tool's store holds, as far as Pitboard can read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// A login of the tool's, which a session that cannot read that store signs in with.
    Login,
    /// No login of the tool's.
    NoLogin,
    /// It is there, and cannot be read, as a file that is not text or that this user may
    /// not read.
    Unreadable,
}

/// Whether a parked copy may exist while the same account is still live.
///
/// For Claude Code it may: the live document holds the machine's other keys too, and
/// nothing revokes for presenting either copy. For Codex it must not. `codex login` and
/// `codex logout` both revoke the stored refresh token at OpenAI before clearing it, so a
/// copy left live while its twin sits in the vault is a token the person's own next login
/// can kill in both places at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParkSemantics {
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
pub(crate) enum Isolation {
    /// A home override fully isolates a sign-in from the live credential.
    Isolated,
    /// It does not, and here is what to tell the person.
    NotIsolated { reason: String },
}

/// When a login stops working, in epoch seconds. `None` where the tool does not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Expiry {
    /// Until then its usage can be asked without renewing it first.
    pub(crate) access_expires_at: Option<i64>,
    /// Until then it can be restored at all.
    pub(crate) refresh_expires_at: Option<i64>,
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

    /// Which login this credential is.
    fn identity(&self, ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError>;

    /// The email of the login `identity` names, asked only for an account Pitboard does not
    /// know already.
    fn name(
        &self,
        ctx: &Context,
        credential: &Credential,
        identity: &Identity,
    ) -> Result<String, ProviderError>;

    /// Whose credential this is: its identity, and the name that goes with it.
    fn identify(&self, ctx: &Context, credential: &Credential) -> Result<Owner, ProviderError> {
        let identity = self.identity(ctx, credential)?;
        let email = self.name(ctx, credential, &identity)?;
        Ok(Owner::named(identity, email))
    }

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
        self.identity(ctx, credential)
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

    /// What the locks this tool takes before it renews its login guard: its own, then one
    /// beside it that it goes without where that cannot be made. Pitboard holds them too while
    /// it counts on the refresh token stored. `None` for a tool that takes none.
    fn refresh_lock(&self, ctx: &Context) -> Option<(std::path::PathBuf, std::path::PathBuf)>;

    /// The account a tool that keeps a record of its own apart from its login names there,
    /// read from its files without asking anybody. `None` for a tool whose login names its
    /// own account, and where the record names nobody.
    ///
    /// Claude Code's is its config, which a Claude Code process started or signed in on
    /// another login can rewrite with its own account (the register's
    /// `config_identity_is_the_last_writers`), so it need not be the login stored's. So it is
    /// a sign that something signed in, the words for a message, and what a switch to the
    /// account in use writes over: never whose a login is, which row is in use, whose a
    /// session's numbers are, or whether an account may be forgotten.
    fn own_record(&self, ctx: &Context) -> Option<Owner>;

    /// Correct whatever this tool caches about who is signed in, now that `incoming`'s
    /// login is live and the tool still names `outgoing`: the account switched away from, or
    /// the one its own record names in place of the account already in use.
    ///
    /// Runs after the login has moved and cannot undo it, so a failure here is reported
    /// and never rolled back: the tool would otherwise name an account whose login is no
    /// longer there.
    fn after_switch(
        &self,
        ctx: &Context,
        permit: Permit,
        incoming: &crate::state::Account,
        outgoing: &Owner,
    ) -> Result<(), crate::error::Error>;

    /// The tool's own program, where it is installed. A private sign-in runs it, so its
    /// absence is worth saying before anybody opens a browser.
    fn program(&self, ctx: &Context) -> Option<std::path::PathBuf>;

    // Pointed at `dir`, private and there already, so the live login is never touched; only
    // with the Permit, since `codex login` revokes whatever login its home holds.
    fn sign_in(
        &self,
        ctx: &Context,
        permit: Permit,
        dir: &std::path::Path,
    ) -> std::io::Result<std::process::Command>;

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

    // Why this tool, as this machine configures it, would sign `document`, `owner`'s login, out
    // or leave it unused once in place, as a sentence with no full stop. A move-only park is the
    // only copy.
    fn refuses(&self, ctx: &Context, document: &Value, owner: &Owner) -> Option<String>;

    /// The file behind the store in use, wherever a look says it is there, whatever it
    /// holds. `None` where there is none, and where whether there is one cannot be told: the
    /// store in use cannot be read, which reading the login in it says as well, or the file
    /// cannot even be looked at, which the tool's own look cannot either.
    fn behind(&self, ctx: &Context) -> Option<Behind>;

    /// When a running session follows a switch made with `behind` behind the store the
    /// switch writes. A fact about the tool, not a setting.
    fn adoption(&self, behind: Option<&Behind>) -> Adoption;

    // The processes this tool recorded by id as ones it started in the background, which its
    // holders can name wherever they run from.
    fn recorded(&self, _ctx: &Context) -> Vec<u32> {
        Vec::new()
    }

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

    /// The plan a login or a park of it says, as a label: "Max 20x", "Pro".
    fn plan(&self, document: &Value) -> Option<String>;

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
    match of(provider) {
        LoginKind::Document(tool) => {
            let read = tool.read_sign_in(said);
            SignInView {
                wants_code: read.wants_code && !pasted,
                ..read
            }
        }
        // An app's sign-in prints no address and takes no code.
        LoginKind::Folder(_) => SignInView::default(),
    }
}

/// Whether `provider`'s sign-in refused the code typed back last, by what it said after it,
/// `since`, and reads another in its place.
pub fn refused_code(provider: ProviderId, since: &str) -> bool {
    match of(provider) {
        LoginKind::Document(tool) => tool.refused_code(since),
        LoginKind::Folder(_) => false,
    }
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

// Looked for each time, so a program installed since Pitboard started is found.
pub(crate) fn program_of(ctx: &Context, tool: ProviderId) -> Option<std::path::PathBuf> {
    program_among(ctx, tool, crate::host::proc::can_run)
}

pub(crate) fn program_among(
    ctx: &Context,
    tool: ProviderId,
    runnable: impl FnMut(&std::path::Path) -> bool,
) -> Option<std::path::PathBuf> {
    let installers = ctx
        .installer_places
        .get(&tool)
        .into_iter()
        .flatten()
        .cloned();
    let search = ctx.search_path();
    let dirs = std::env::split_paths(&search).chain(installers);
    crate::host::program::find_in(ctx.program_for(tool)?, dirs, &ctx.lookup(), runnable)
}

// What a sign-in of each tool could start here now: its program, or its app's data folder.
pub(crate) fn installed_among(
    ctx: &Context,
    mut runnable: impl FnMut(&std::path::Path) -> bool,
) -> Vec<ProviderId> {
    ProviderId::ALL
        .iter()
        .copied()
        .filter(|&tool| match of(tool) {
            LoginKind::Document(_) => program_among(ctx, tool, &mut runnable).is_some(),
            LoginKind::Folder(app) => app.root(ctx).is_ok_and(|root| root.is_dir()),
        })
        .collect()
}

// The program's own folder goes in front of a search path that lacks it, so an npm script
// finds the `node` beside it. A bare name on Windows would start one beside Pitboard (RF-F3).
pub(crate) fn command(
    ctx: &Context,
    _: Permit,
    tool: ProviderId,
) -> std::io::Result<std::process::Command> {
    let search = ctx.search_path();
    let Some(program) = program_of(ctx, tool) else {
        if ctx.os == crate::host::Os::Windows {
            return Err(std::io::ErrorKind::NotFound.into());
        }
        let named = ctx
            .program_for(tool)
            .unwrap_or(std::path::Path::new(tool.program()));
        let mut command = std::process::Command::new(named);
        command.env("PATH", search);
        return Ok(command);
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
            .unwrap_or(search),
        None => search,
    };
    let (started, set) = behind_npms_shim(tool, &program).unwrap_or((program, Vec::new()));
    let mut command = std::process::Command::new(started);
    command.env("PATH", path);
    for (name, value) in set {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    Ok(command)
}

// A variable a launcher sets, or with `None` takes away.
pub(crate) type Launched = Vec<(&'static str, Option<std::ffi::OsString>)>;

// Started by its `cmd.exe`, a tool asks "Terminate batch job (Y/N)?" on a Ctrl+C.
fn behind_npms_shim(
    tool: ProviderId,
    shim: &std::path::Path,
) -> Option<(std::path::PathBuf, Launched)> {
    let target = crate::host::program::npm_shim_target(shim)?;
    let a_program = target
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
    if a_program {
        return crate::host::proc::can_run(&target).then(|| (target, Vec::new()));
    }
    match tool {
        ProviderId::Codex => codex::paths::npm_native(&target),
        ProviderId::Claude | ProviderId::Desktop => None,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LoginKind {
    Document(&'static dyn Provider),
    Folder(&'static dyn folder::Folder),
}

/// An exhaustive match rather than a lookup, so a tool added to [`ProviderId`] and not to
/// here stops compiling instead of being silently absent.
pub(crate) fn of(tool: ProviderId) -> LoginKind {
    match tool {
        ProviderId::Claude => LoginKind::Document(&claude::engine::Claude),
        ProviderId::Codex => LoginKind::Document(&codex::engine::Codex),
        ProviderId::Desktop => LoginKind::Folder(&desktop::Desktop),
    }
}

pub(crate) fn claude_code() -> &'static dyn Provider {
    &claude::engine::Claude
}

pub(crate) fn codex() -> &'static dyn Provider {
    &codex::engine::Codex
}

// None: the tool keeps nothing Pitboard acts on here
pub(crate) fn slot(ctx: &Context, tool: ProviderId) -> Option<String> {
    match of(tool) {
        LoginKind::Document(found) => Some(found.slot(ctx)),
        LoginKind::Folder(app) => app
            .root(ctx)
            .ok()
            .map(|root| root.to_string_lossy().into_owned()),
    }
}

#[cfg(test)]
pub(crate) fn document(tool: ProviderId) -> &'static dyn Provider {
    match of(tool) {
        LoginKind::Document(found) => found,
        LoginKind::Folder(_) => panic!("{tool} keeps no login document"),
    }
}

#[cfg(test)]
pub(crate) fn documents() -> impl Iterator<Item = &'static dyn Provider> {
    ProviderId::ALL.iter().filter_map(|&tool| match of(tool) {
        LoginKind::Document(found) => Some(found),
        LoginKind::Folder(_) => None,
    })
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
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
                ProviderId::Claude | ProviderId::Codex | ProviderId::Desktop => {}
            }
        }
        assert_eq!(ProviderId::ALL.len(), 3, "add the new provider to ALL");
    }

    #[test]
    fn desktop_parses_as_a_tool_and_keeps_no_command_line() {
        assert_eq!(ProviderId::parse("desktop"), Some(ProviderId::Desktop));
        assert_eq!(ProviderId::Desktop.name(), "Claude Desktop");
        assert_eq!(ProviderId::Desktop.program(), "Claude");
        assert!(ProviderId::Desktop.cli().is_none());
        assert_eq!(ProviderId::Desktop.signs_in(), SignsIn::InItsApp);
        assert!(!ProviderId::Desktop.reads_usage());
        assert!(matches!(of(ProviderId::Desktop), LoginKind::Folder(_)));
        for tool in [ProviderId::Claude, ProviderId::Codex] {
            assert!(tool.cli().is_some(), "{tool}");
            assert_eq!(tool.signs_in(), SignsIn::ByItsProgram, "{tool}");
            assert!(tool.reads_usage(), "{tool}");
        }
        assert_eq!(
            documents().map(|tool| tool.id()).collect::<Vec<_>>(),
            [ProviderId::Claude, ProviderId::Codex]
        );
        assert_eq!(
            sign_in_view(ProviderId::Desktop, "https://claude.ai/login", false),
            SignInView::default()
        );
        assert!(!refused_code(ProviderId::Desktop, "Invalid code"));
    }

    #[test]
    fn the_tool_is_absent_off_macos() {
        use crate::host::Os;
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            assert_eq!(
                ProviderId::Desktop.supported_on(os),
                os == Os::MacOs,
                "{os:?}"
            );
            assert!(ProviderId::Claude.supported_on(os), "{os:?}");
            assert!(ProviderId::Codex.supported_on(os), "{os:?}");
        }
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
            let said = match of(id) {
                LoginKind::Document(tool) => tool.id(),
                LoginKind::Folder(app) => app.id(),
            };
            assert_eq!(said, id, "{id} is registered against another tool");
        }
    }

    /// The three facts, asserted against what was measured, so a change to one is a change
    /// to a test rather than a surprise on somebody's machine.
    #[test]
    fn claude_code_follows_a_switch_on_its_own_and_tolerates_a_copy() {
        let claude = document(ProviderId::Claude);
        assert_eq!(
            claude.adoption(None),
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
        let restart = document(ProviderId::Codex).adoption(None);
        assert_ne!(restart, Adoption::PollingWithin(0));
        assert!(matches!(Adoption::PollingWithin(33), Adoption::PollingWithin(s) if s == 33));
    }

    // Nothing here is ever run.
    struct Prefix(std::path::PathBuf);

    fn file_of(program: &str) -> String {
        format!("{program}{}", std::env::consts::EXE_SUFFIX)
    }

    fn joined(dirs: &[&std::path::Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs).expect("a search path")
    }

    impl Prefix {
        fn new(name: &str) -> Prefix {
            let root = std::env::temp_dir().join(format!(
                "pitboard-search-path-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            let bin = root.join("npm").join("bin");
            std::fs::create_dir_all(&bin).expect("a scratch prefix");
            for tool in documents().map(|tool| tool.id()) {
                let program = bin.join(file_of(tool.program()));
                std::fs::write(&program, "").expect("a program");
                crate::host::fs::testing::make_runnable(&program).expect("runnable");
            }
            Prefix(root)
        }

        fn bin(&self) -> std::path::PathBuf {
            self.0.join("npm").join("bin")
        }

        fn program(&self, tool: ProviderId) -> std::path::PathBuf {
            self.bin().join(file_of(tool.program()))
        }

        // Where no folder is.
        fn nowhere(&self) -> std::path::PathBuf {
            self.0.join("nowhere")
        }

        // A folder of each program's name, and a file of it nobody may run.
        fn decoys(&self) -> (std::path::PathBuf, std::path::PathBuf) {
            let (dirs, files) = (self.0.join("dirs"), self.0.join("files"));
            for tool in documents().map(|tool| tool.id()) {
                std::fs::create_dir_all(dirs.join(file_of(tool.program()))).expect("a directory");
                std::fs::create_dir_all(&files).expect("a directory");
                let file = files.join(file_of(tool.program()));
                std::fs::write(&file, "").expect("a file");
                crate::host::fs::testing::deny_running(&file).expect("nobody may run it");
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

    // An app opened from Finder has only the system's folders on its own `PATH`.
    #[test]
    fn a_program_is_looked_for_on_the_search_path_it_is_given() {
        let prefix = Prefix::new("find");
        let search = joined(&[&prefix.nowhere(), &prefix.bin()]);
        let lookup = crate::host::program::Lookup::here();
        let found = crate::host::program::find(std::path::Path::new("codex"), &search, &lookup);
        assert_eq!(found, Some(prefix.program(ProviderId::Codex)));
        let everywhere = match crate::host::OS {
            crate::host::Os::MacOs | crate::host::Os::Linux => "ls",
            crate::host::Os::Windows => "cmd",
        };
        assert_eq!(
            crate::host::program::find(std::path::Path::new(everywhere), &search, &lookup),
            None,
            "{everywhere} is on this process's PATH and not on the one given"
        );
    }

    #[test]
    fn what_cannot_be_run_is_passed_over() {
        let prefix = Prefix::new("decoys");
        let (dirs, files) = prefix.decoys();
        let search = joined(&[&dirs, &files, &prefix.bin()]);
        let lookup = crate::host::program::Lookup::here();
        assert_eq!(
            crate::host::program::find(std::path::Path::new("codex"), &search, &lookup),
            Some(prefix.program(ProviderId::Codex))
        );
        for decoy in [dirs.join(file_of("codex")), files.join(file_of("codex"))] {
            assert_eq!(
                crate::host::program::find(&decoy, "".as_ref(), &lookup),
                None,
                "{}",
                decoy.display()
            );
        }
    }

    // Its folder first would change which `node` an npm script finds from the terminal's.
    #[test]
    fn a_sign_in_runs_the_program_found_with_the_search_path_as_it_is() {
        let prefix = Prefix::new("sign-in");
        let search = joined(&[&prefix.nowhere(), &prefix.bin()]);
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_search_path(search.to_string_lossy().into_owned());
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        for tool in documents().map(|tool| tool.id()) {
            let command = document(tool)
                .sign_in(&ctx, Permit::for_a_test(), dir)
                .expect("found");
            let program = prefix.program(tool);
            assert_eq!(command.get_program(), program.as_os_str(), "{tool}");
            assert_eq!(env_of(&command, "PATH"), Some(search.as_os_str()), "{tool}");
            assert_eq!(document(tool).program(&ctx), Some(program), "{tool}");
        }
    }

    // An npm script names `node` through `env`, and `node` is beside it.
    #[test]
    fn a_program_named_outright_is_run_with_its_own_directory_on_path() {
        let prefix = Prefix::new("named");
        let (system, other) = (prefix.0.join("system"), prefix.0.join("other"));
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_claude_program(prefix.program(ProviderId::Claude))
            .with_codex_program(prefix.program(ProviderId::Codex))
            .with_search_path(joined(&[&system, &other]).to_string_lossy().into_owned());
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        for tool in documents().map(|tool| tool.id()) {
            let command = document(tool)
                .sign_in(&ctx, Permit::for_a_test(), dir)
                .expect("found");
            assert_eq!(
                command.get_program(),
                prefix.program(tool).as_os_str(),
                "{tool}"
            );
            assert_eq!(
                env_of(&command, "PATH"),
                Some(joined(&[&prefix.bin(), &system, &other]).as_os_str()),
                "{tool}"
            );
        }
        let mut bin = prefix.bin().into_os_string();
        bin.push(std::path::MAIN_SEPARATOR_STR);
        let on_it = joined(&[&system, std::path::Path::new(&bin)]);
        let ctx = ctx.with_search_path(on_it.to_string_lossy().into_owned());
        for tool in documents().map(|tool| tool.id()) {
            let command = document(tool)
                .sign_in(&ctx, Permit::for_a_test(), dir)
                .expect("found");
            assert_eq!(env_of(&command, "PATH"), Some(on_it.as_os_str()), "{tool}");
        }
    }

    // std's `Command` looks for a bare name beside Pitboard first on Windows (RF-F3).
    #[test]
    fn a_program_found_nowhere_is_started_by_its_bare_name_only_off_windows() {
        let prefix = Prefix::new("nowhere");
        let nowhere = prefix.nowhere();
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        for os in [
            crate::host::Os::MacOs,
            crate::host::Os::Linux,
            crate::host::Os::Windows,
        ] {
            let env = crate::context::Environment::on(
                os,
                [
                    ("HOME", prefix.0.as_os_str()),
                    ("USERPROFILE", prefix.0.as_os_str()),
                    ("PATH", nowhere.as_os_str()),
                ],
            );
            let ctx = Context::for_command_line(&env);
            for tool in documents().map(|tool| tool.id()) {
                let command = document(tool).sign_in(&ctx, Permit::for_a_test(), dir);
                assert_eq!(document(tool).program(&ctx), None, "{tool}");
                match os {
                    crate::host::Os::MacOs | crate::host::Os::Linux => {
                        let command = command.expect("left to the search path");
                        assert_eq!(command.get_program(), tool.program(), "{tool}");
                        assert_eq!(
                            env_of(&command, "PATH"),
                            Some(nowhere.as_os_str()),
                            "{tool}"
                        );
                    }
                    crate::host::Os::Windows => assert_eq!(
                        command.map(drop).map_err(|e| e.kind()),
                        Err(std::io::ErrorKind::NotFound),
                        "{tool}"
                    ),
                }
            }
        }
    }

    // As npm's cmd-shim 9.0.2 writes one for a program, and for a script `node` runs.
    fn npm_shim(at: &std::path::Path, target: &str, interpreted: bool) {
        let run = if interpreted {
            format!(
                "\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  \
                 SET \"_prog=node\"\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title \
                 %COMSPEC% & set PATHEXT=%PATHEXT:;.JS;=;% & \"%_prog%\"  \"%dp0%\\{target}\" %*\r\n"
            )
        } else {
            format!("\"%dp0%\\{target}\"   %*\r\n")
        };
        std::fs::write(
            at,
            format!(
                "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\n\
                 SETLOCAL\r\nCALL :find_dp0\r\n{run}"
            ),
        )
        .expect("a shim");
        crate::host::fs::testing::make_runnable(at).expect("runnable");
    }

    fn a_program_at(at: &std::path::Path) -> std::path::PathBuf {
        std::fs::create_dir_all(at.parent().expect("its folder")).expect("a folder");
        std::fs::write(at, "").expect("a program");
        crate::host::fs::testing::make_runnable(at).expect("runnable");
        at.to_path_buf()
    }

    fn under(root: &std::path::Path, parts: &[&str]) -> std::path::PathBuf {
        parts
            .iter()
            .fold(root.to_path_buf(), |at, part| at.join(part))
    }

    // npm's own `codex.cmd` and `claude.cmd`, as `npm install -g` leaves them on Windows.
    #[test]
    fn behind_npms_shim_a_sign_in_starts_the_program_the_shim_would() {
        let prefix = Prefix::new("npm-shim");
        let npm = prefix.0.join("npm");
        let claude = a_program_at(&under(
            &npm,
            &[
                "node_modules",
                "@anthropic-ai",
                "claude-code",
                "bin",
                "claude.exe",
            ],
        ));
        npm_shim(
            &npm.join("claude.cmd"),
            r"node_modules\@anthropic-ai\claude-code\bin\claude.exe",
            false,
        );
        let package = under(&npm, &["node_modules", "@openai", "codex"]);
        a_program_at(&package.join("bin").join("codex.js"));
        let platform = under(&package, &["node_modules", "@openai", "codex-win32-x64"]);
        let codex = a_program_at(&under(
            &platform,
            &["vendor", "x86_64-pc-windows-msvc", "bin", "codex.exe"],
        ));
        std::fs::write(platform.join("package.json"), "{}").expect("its package.json");
        npm_shim(
            &npm.join("codex.cmd"),
            r"node_modules\@openai\codex\bin\codex.js",
            true,
        );
        let search = joined(&[&prefix.nowhere()]);
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_claude_program(npm.join("claude.cmd"))
            .with_codex_program(npm.join("codex.cmd"))
            .with_search_path(search.to_string_lossy().into_owned());
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        let on_path = joined(&[&npm, &prefix.nowhere()]);
        for (tool, native) in [(ProviderId::Claude, &claude), (ProviderId::Codex, &codex)] {
            let command = document(tool)
                .sign_in(&ctx, Permit::for_a_test(), dir)
                .expect("found");
            assert_eq!(command.get_program(), native.as_os_str(), "{tool}");
            assert_eq!(
                env_of(&command, "PATH"),
                Some(on_path.as_os_str()),
                "{tool}"
            );
        }
        let command = codex_sign_in(&ctx, dir);
        let root = crate::host::without_verbatim_prefix(
            std::fs::canonicalize(&package).expect("the package's folder"),
        );
        assert_eq!(
            env_of(&command, "CODEX_MANAGED_PACKAGE_ROOT"),
            Some(root.as_os_str())
        );
        assert_eq!(
            env_of(&command, "CODEX_MANAGED_BY_NPM"),
            Some(std::ffi::OsStr::new("1"))
        );
        for other in [
            "CODEX_MANAGED_BY_BUN",
            "CODEX_MANAGED_BY_PNPM",
            "CODEX_MANAGED_BY_VITE_PLUS",
        ] {
            assert!(
                command
                    .get_envs()
                    .any(|(key, value)| key == other && value.is_none()),
                "{other} removed"
            );
        }
    }

    // Where the platform's package sits beside Codex's, or is missing, or the shim is not npm's.
    #[test]
    fn behind_any_other_cmd_a_sign_in_starts_the_cmd() {
        let prefix = Prefix::new("npm-hoisted");
        let npm = prefix.0.join("npm");
        let package = under(&npm, &["node_modules", "@openai", "codex"]);
        a_program_at(&package.join("bin").join("codex.js"));
        npm_shim(
            &npm.join("codex.cmd"),
            r"node_modules\@openai\codex\bin\codex.js",
            true,
        );
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_codex_program(npm.join("codex.cmd"))
            .with_search_path(prefix.nowhere().to_string_lossy().into_owned());
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        assert_eq!(
            codex_sign_in(&ctx, dir).get_program(),
            npm.join("codex.cmd").as_os_str(),
            "no codex.exe for it to start"
        );

        let hoisted = under(&npm, &["node_modules", "@openai", "codex-win32-arm64"]);
        let codex = a_program_at(&under(
            &hoisted,
            &["vendor", "aarch64-pc-windows-msvc", "bin", "codex.exe"],
        ));
        std::fs::write(hoisted.join("package.json"), "{}").expect("its package.json");
        assert_eq!(codex_sign_in(&ctx, dir).get_program(), codex.as_os_str());

        let own = a_program_at(&npm.join("other").join("codex.cmd"));
        std::fs::write(&own, "@echo off\r\n\"%~dp0codex.exe\" %*\r\n").expect("a script");
        a_program_at(&npm.join("other").join("codex.exe"));
        let ctx = ctx.with_codex_program(own.clone());
        assert_eq!(codex_sign_in(&ctx, dir).get_program(), own.as_os_str());
    }

    fn codex_sign_in(ctx: &Context, dir: &std::path::Path) -> std::process::Command {
        document(ProviderId::Codex)
            .sign_in(ctx, Permit::for_a_test(), dir)
            .expect("found")
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
