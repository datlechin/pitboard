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
//! What is not here: parking and restoring themselves. Every other step of a switch is
//! either pitboard's own bookkeeping, which does not vary, or one tool's private mechanics,
//! which nothing outside that tool's own module ever calls. A `park` method on this trait
//! would have to either hide the difference between splicing a shared document and moving a
//! whole file behind a flag, or have two bodies so different that the trait bought nothing.
//! Also absent: Claude Code's config-file identity cache, its supervisor daemon, its status
//! line hook. A method most implementations no-op is a sign the method does not belong on a
//! shared trait.

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
}

impl ProviderId {
    /// Every provider pitboard knows, in the order a listing shows them.
    ///
    /// Named rather than written out at each call site, because resolving a bare label has
    /// to look at all of them and a provider missing from one such list would simply never
    /// be found, with nothing failing to say so.
    pub const ALL: &'static [ProviderId] = &[ProviderId::Claude];

    /// The one spelling used in a label prefix, in the state file, in a park's name and in
    /// the audit log. Written once so those four cannot drift, and chosen from the command
    /// a person types rather than the company behind it, because the command is the thing
    /// that is stable.
    pub fn code(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
        }
    }

    pub fn parse(code: &str) -> Option<ProviderId> {
        match code {
            "claude" => Some(ProviderId::Claude),
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
    #[error("the service asked for less traffic")]
    RateLimited { retry_after: Option<i64> },
    #[error("could not reach the service: {0}")]
    Network(String),
    #[error("the service answered {status}")]
    Unexpected { status: u16 },
    #[error("the answer was not understood: {0}")]
    Malformed(String),
    /// Refused for good: revoked, or already spent somewhere else.
    #[error("this login is no longer accepted")]
    InvalidGrant,
    /// The credential is not the shape this provider stores.
    #[error("the stored login is not the shape {provider} keeps: {detail}")]
    ShapeUnexpected {
        provider: ProviderId,
        detail: String,
    },
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

/// One coding tool's login, as the rest of pitboard needs to touch it.
///
/// Implementations live in `provider::<name>`. Nothing here knows about pitboard's state
/// file, its lock, its journal or its audit log: those are pitboard's own bookkeeping and
/// do not vary by tool.
#[allow(dead_code, reason = "the first implementation lands in a later commit")]
pub(crate) trait Provider: Send + Sync + std::fmt::Debug {
    fn id(&self) -> ProviderId;

    /// The credential this tool would authenticate with right now.
    ///
    /// `Ok(None)` means nothing is signed in, which is an answer. A store that could not be
    /// read is an error and must never collapse into `None`: reading one as the other tells
    /// somebody their login is gone when it is merely unreadable.
    fn read_live(&self, ctx: &Context) -> Result<Option<Credential>, ProviderError>;

    /// Whose credential this is.
    fn identify(&self, ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError>;

    /// What this credential has left, normalised into pitboard's own shape.
    ///
    /// Takes the whole credential and the context, not an access token, because what a
    /// usage call needs is not the same everywhere: Codex sends an account id header it
    /// reads out of the credential, and Gemini needs a project id from a file the
    /// credential never mentions.
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

    /// Make this credential the live one, and prove it landed.
    ///
    /// What "make live" means underneath is the implementation's business: splicing one
    /// account's keys into a document the machine shares for Claude Code, replacing a whole
    /// file for Codex and Gemini. What the caller is promised is that a successful return
    /// means the store was read back and holds what was written.
    fn install_live(&self, ctx: &Context, credential: &Credential) -> Result<(), ProviderError>;

    /// When a running session follows a switch. A fact about the tool, not a setting.
    fn adoption(&self) -> Adoption;

    /// Whether a park may coexist with the same account still live.
    fn park_semantics(&self) -> ParkSemantics;

    /// Whether a private sign-in on this machine, right now, is really private.
    ///
    /// Takes the context because the answer is not a constant: it depends on which backend
    /// the tool is configured to use here.
    fn private_signin_isolation(&self, ctx: &Context) -> Isolation;
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
                ProviderId::Claude => {}
            }
        }
        assert_eq!(ProviderId::ALL.len(), 1, "add the new provider to ALL");
    }

    /// A code is also what serde writes, so the two spellings must not drift.
    #[test]
    fn the_code_is_what_serde_writes() {
        for &id in ProviderId::ALL {
            let written = serde_json::to_value(id).expect("a provider id serialises");
            assert_eq!(written, serde_json::json!(id.code()), "{id}");
        }
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
}
