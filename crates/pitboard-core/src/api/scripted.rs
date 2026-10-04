//! An Anthropic that answers what it was told to, and remembers what it was asked.
//!
//! The integration tests run a loopback server, which proves the parsing and the wiring.
//! What a server cannot produce on demand is the half that decides behaviour: a request
//! that never answers, a 429, a refresh token Anthropic has stopped accepting. Those are
//! the answers the engine has to be right about, and they were untestable.
//!
//! It also counts. How often Pitboard asks is a design question in its own right, and a
//! test that can say "asked once, for two front ends" is how that stays true.

use super::{Api, ApiError, Owner, Renewed};
use crate::context::Context;
use crate::provider::ProviderError;
use crate::provider::codex::api::{Fresh, OpenAi};
use crate::provider::desktop::safe_storage::{ItemStamp, KeyRead, KeyReadError, SafeStorage};
use crate::provider::desktop::web::ClaudeWeb;
use crate::usage::Snapshot;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// A failure the network can produce and a loopback server cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trouble {
    /// The access token has expired or been revoked.
    Unauthorized,
    RateLimited,
    /// Rate limited, with Anthropic saying how long to wait.
    RateLimitedFor(i64),
    /// No route to Anthropic at all: a plane, a captive portal, a bad morning.
    Offline,
    Server(u16),
    /// The refresh token was refused for good, which is the answer that ends a park.
    InvalidGrant,
    /// claude.ai's bot check stopped the request before claude.ai read it.
    BotCheck,
}

impl From<Trouble> for ApiError {
    fn from(t: Trouble) -> ApiError {
        match t {
            Trouble::Unauthorized => ApiError::Unauthorized,
            Trouble::RateLimited => ApiError::RateLimited { retry_after: None },
            Trouble::RateLimitedFor(seconds) => ApiError::RateLimited {
                retry_after: Some(seconds),
            },
            Trouble::Offline => ApiError::Network("no route to host".into()),
            Trouble::Server(status) => ApiError::Unexpected { status },
            Trouble::InvalidGrant => ApiError::InvalidGrant,
            Trouble::BotCheck => ApiError::Blocked {
                status: 403,
                by: crate::provider::desktop::web::BOT_CHECK,
            },
        }
    }
}

/// What a scripted endpoint does when it is asked.
#[derive(Debug, Clone)]
pub enum Answer<T> {
    Give(T),
    Fail(Trouble),
}

impl<T: Clone> Answer<T> {
    fn take(&self) -> Result<T, ApiError> {
        match self {
            Answer::Give(value) => Ok(value.clone()),
            Answer::Fail(trouble) => Err((*trouble).into()),
        }
    }
}

/// One question that was asked, for a test that cares how often.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Owner(String),
    Usage(String),
    Renew(String),
    /// claude.ai's usage of an organisation, with the session it was asked with.
    WebUsage {
        org: String,
        session: String,
    },
}

#[derive(Debug, Default)]
struct Script {
    owners: HashMap<String, Answer<Owner>>,
    usage: HashMap<String, Answer<Snapshot>>,
    renewals: HashMap<String, Answer<Renewed>>,
    codex_renewals: HashMap<String, Answer<Fresh>>,
    web_usage: HashMap<String, Answer<Snapshot>>,
    asked: Vec<Asked>,
}

/// Anthropic, scripted. Unknown tokens are unauthorized, which is what an unknown token is.
#[derive(Debug, Default)]
pub struct ScriptedApi(Mutex<Script>);

impl ScriptedApi {
    pub fn new() -> Arc<ScriptedApi> {
        Arc::new(ScriptedApi::default())
    }

    fn script(&self) -> std::sync::MutexGuard<'_, Script> {
        self.0.lock().expect("a poisoned script is a failed test")
    }

    /// Who this access token belongs to.
    pub fn owned_by(&self, access_token: &str, owner: Owner) -> &ScriptedApi {
        self.script()
            .owners
            .insert(access_token.into(), Answer::Give(owner));
        self
    }

    /// What this access token has left.
    pub fn using(&self, access_token: &str, snapshot: Snapshot) -> &ScriptedApi {
        self.script()
            .usage
            .insert(access_token.into(), Answer::Give(snapshot));
        self
    }

    /// What this refresh token exchanges for.
    pub fn renews(&self, refresh_token: &str, renewed: Renewed) -> &ScriptedApi {
        self.script()
            .renewals
            .insert(refresh_token.into(), Answer::Give(renewed));
        self
    }

    /// Asking about this access token goes wrong, both questions.
    pub fn token_trouble(&self, access_token: &str, trouble: Trouble) -> &ScriptedApi {
        let mut script = self.script();
        script
            .owners
            .insert(access_token.into(), Answer::Fail(trouble));
        script
            .usage
            .insert(access_token.into(), Answer::Fail(trouble));
        drop(script);
        self
    }

    /// Renewing this refresh token goes wrong.
    /// What OpenAI answers when Codex's refresh token is exchanged.
    pub fn codex_renews(&self, refresh_token: &str, fresh: Fresh) -> &ScriptedApi {
        self.script()
            .codex_renewals
            .insert(refresh_token.into(), Answer::Give(fresh));
        self
    }

    pub fn codex_renew_trouble(&self, refresh_token: &str, trouble: Trouble) -> &ScriptedApi {
        self.script()
            .codex_renewals
            .insert(refresh_token.into(), Answer::Fail(trouble));
        self
    }

    pub fn renew_trouble(&self, refresh_token: &str, trouble: Trouble) -> &ScriptedApi {
        self.script()
            .renewals
            .insert(refresh_token.into(), Answer::Fail(trouble));
        self
    }

    /// What claude.ai answers for a Claude Desktop session, whichever organisation.
    pub fn web_using(&self, session_key: &str, snapshot: Snapshot) -> &ScriptedApi {
        self.script()
            .web_usage
            .insert(session_key.into(), Answer::Give(snapshot));
        self
    }

    /// Asking claude.ai with this session goes wrong.
    pub fn web_trouble(&self, session_key: &str, trouble: Trouble) -> &ScriptedApi {
        self.script()
            .web_usage
            .insert(session_key.into(), Answer::Fail(trouble));
        self
    }

    /// Everything that was asked, in order.
    pub fn asked(&self) -> Vec<Asked> {
        self.script().asked.clone()
    }

    /// How many times anything was asked at all.
    pub fn calls(&self) -> usize {
        self.script().asked.len()
    }

    fn answer<T: Clone>(
        &self,
        asked: Asked,
        pick: impl FnOnce(&Script) -> Option<&Answer<T>>,
    ) -> Result<T, ApiError> {
        let mut script = self.script();
        script.asked.push(asked);
        match pick(&script) {
            Some(answer) => answer.take(),
            // An unknown token is exactly what Anthropic refuses.
            None => Err(ApiError::Unauthorized),
        }
    }
}

impl Api for ScriptedApi {
    fn owner(&self, _ctx: &Context, access_token: &str) -> Result<Owner, ApiError> {
        self.answer(Asked::Owner(access_token.into()), |s| {
            s.owners.get(access_token)
        })
    }

    fn usage(&self, _ctx: &Context, access_token: &str) -> Result<Snapshot, ApiError> {
        self.answer(Asked::Usage(access_token.into()), |s| {
            s.usage.get(access_token)
        })
    }

    fn renew(
        &self,
        _ctx: &Context,
        refresh_token: &str,
        _scopes: &[String],
        _client_id: Option<&str>,
    ) -> Result<Renewed, ApiError> {
        self.answer(Asked::Renew(refresh_token.into()), |s| {
            s.renewals.get(refresh_token)
        })
    }
}

/// claude.ai, scripted by session. An unknown session is unauthorized, as on Anthropic's.
impl ClaudeWeb for ScriptedApi {
    fn usage(&self, _ctx: &Context, org: &str, session_key: &str) -> Result<Snapshot, ApiError> {
        self.answer(
            Asked::WebUsage {
                org: org.into(),
                session: session_key.into(),
            },
            |s| s.web_usage.get(session_key),
        )
    }
}

/// A scripted failure as OpenAI's side of the boundary reports it.
fn from_trouble(trouble: Trouble) -> ProviderError {
    let service = crate::provider::ProviderId::Codex.service();
    match trouble {
        Trouble::Unauthorized => ProviderError::Unauthorized,
        Trouble::RateLimited => ProviderError::RateLimited {
            service,
            retry_after: None,
        },
        Trouble::RateLimitedFor(seconds) => ProviderError::RateLimited {
            service,
            retry_after: Some(seconds),
        },
        Trouble::Offline => ProviderError::Network {
            service,
            detail: "no route to host".into(),
        },
        Trouble::Server(status) => ProviderError::Unexpected { service, status },
        Trouble::InvalidGrant => ProviderError::InvalidGrant { service },
        // Only claude.ai has one, and OpenAI's side reads it as any other refusal.
        Trouble::BotCheck => ProviderError::Unexpected {
            service,
            status: 403,
        },
    }
}

/// Usage is scripted by access token whichever service it is asked of, because a token
/// names one login and a test gives each login its own.
impl OpenAi for ScriptedApi {
    fn usage(
        &self,
        _ctx: &Context,
        access_token: &str,
        _account_id: &str,
        _now: i64,
    ) -> Result<Snapshot, ProviderError> {
        let mut script = self.script();
        script.asked.push(Asked::Usage(access_token.into()));
        match script.usage.get(access_token) {
            Some(Answer::Give(snapshot)) => Ok(snapshot.clone()),
            Some(Answer::Fail(trouble)) => Err(from_trouble(*trouble)),
            None => Err(ProviderError::Unauthorized),
        }
    }

    fn renew(&self, _ctx: &Context, refresh_token: &str) -> Result<Fresh, ProviderError> {
        let service = crate::provider::ProviderId::Codex.service();
        let mut script = self.script();
        script.asked.push(Asked::Renew(refresh_token.into()));
        match script.codex_renewals.get(refresh_token) {
            Some(Answer::Give(fresh)) => Ok(fresh.clone()),
            Some(Answer::Fail(trouble)) => Err(from_trouble(*trouble)),
            None => Err(ProviderError::InvalidGrant { service }),
        }
    }
}

/// What macOS can answer about Claude Desktop's key, as `security` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyTrouble {
    /// Exit 36: macOS would have to ask, and cannot from here.
    NoGui,
    /// Exit 128: somebody said no, or typed a wrong password and chose Allow.
    Denied,
    /// Exit 51, which no answer was seen to give (experiment U-K2).
    AuthFailed,
    /// Nobody answered in time; the question may still be on screen.
    TimedOut,
    /// Exit 44: there is no such item.
    Missing,
    /// An exit `security` is not known to give, or none at all.
    Other,
}

impl From<KeyTrouble> for KeyReadError {
    fn from(t: KeyTrouble) -> KeyReadError {
        match t {
            KeyTrouble::NoGui => KeyReadError::NoGui,
            KeyTrouble::Denied => KeyReadError::Denied,
            KeyTrouble::AuthFailed => KeyReadError::AuthFailed,
            KeyTrouble::TimedOut => KeyReadError::TimedOut,
            KeyTrouble::Missing => KeyReadError::Missing,
            KeyTrouble::Other => KeyReadError::Other("security exited 1: scripted".into()),
        }
    }
}

#[derive(Debug)]
struct Key {
    /// `None` is the forbidding script: nothing may be read at all.
    password: Option<Result<Vec<u8>, KeyTrouble>>,
    stamp: Result<ItemStamp, KeyTrouble>,
    /// How long a password read takes to answer, as when macOS is slow to.
    delay: std::time::Duration,
    /// The `mdat` an approved read leaves behind, as if answering Always Allow changed it.
    allowed_at: Option<String>,
}

/// Claude Desktop's key, scripted, and counted. The real item is somebody's real key, so
/// no test reads it: this answers for it, and says how often it was asked.
#[derive(Debug)]
pub struct ScriptedSafeStorage {
    key: Mutex<Key>,
    stamps: AtomicUsize,
    passwords: AtomicUsize,
    approvals: AtomicUsize,
}

/// When the scripted item was made and last changed, as `security` lists them.
const MADE: &str = "20260727084307Z";

impl ScriptedSafeStorage {
    fn with(key: Key) -> Arc<ScriptedSafeStorage> {
        Arc::new(ScriptedSafeStorage {
            key: Mutex::new(key),
            stamps: AtomicUsize::new(0),
            passwords: AtomicUsize::new(0),
            approvals: AtomicUsize::new(0),
        })
    }

    /// An item holding `password`, made once and never changed since.
    pub fn holding(password: &str) -> Arc<ScriptedSafeStorage> {
        ScriptedSafeStorage::with(Key {
            password: Some(Ok(password.as_bytes().to_vec())),
            stamp: Ok(ItemStamp {
                cdat: MADE.into(),
                mdat: MADE.into(),
            }),
            delay: std::time::Duration::ZERO,
            allowed_at: None,
        })
    }

    /// An item nothing may ask for: asking to approve panics, and every read is counted,
    /// for a test that says nothing but turning live usage on ever reads the key.
    pub fn forbidding() -> Arc<ScriptedSafeStorage> {
        ScriptedSafeStorage::with(Key {
            password: None,
            stamp: Err(KeyTrouble::Missing),
            delay: std::time::Duration::ZERO,
            allowed_at: None,
        })
    }

    fn key(&self) -> std::sync::MutexGuard<'_, Key> {
        self.key.lock().expect("a poisoned script is a failed test")
    }

    /// Reading the password goes wrong from now on.
    pub fn refusing(&self, trouble: KeyTrouble) -> &ScriptedSafeStorage {
        self.key().password = Some(Err(trouble));
        self
    }

    /// The item now holds `password`, as when Claude made its key again.
    pub fn now_holding(&self, password: &str) -> &ScriptedSafeStorage {
        self.key().password = Some(Ok(password.as_bytes().to_vec()));
        self
    }

    /// The item was changed at `mdat`, as when Claude made its key again.
    pub fn changed_at(&self, mdat: &str) -> &ScriptedSafeStorage {
        let mut key = self.key();
        let cdat = key
            .stamp
            .as_ref()
            .map_or_else(|_| MADE.to_string(), |s| s.cdat.clone());
        key.stamp = Ok(ItemStamp {
            cdat,
            mdat: mdat.into(),
        });
        drop(key);
        self
    }

    /// Reading the password takes `delay` from now on, long enough for readers on other
    /// threads to arrive while one is still waiting.
    pub fn slow(&self, delay: std::time::Duration) -> &ScriptedSafeStorage {
        self.key().delay = delay;
        self
    }

    /// Answering a read that asks changes the item at `mdat`, as Always Allow may when it
    /// rewrites the item's access list.
    pub fn allowing_changes_it(&self, mdat: &str) -> &ScriptedSafeStorage {
        self.key().allowed_at = Some(mdat.into());
        self
    }

    /// Reading the item's attributes goes wrong from now on.
    pub fn unlisted(&self, trouble: KeyTrouble) -> &ScriptedSafeStorage {
        self.key().stamp = Err(trouble);
        self
    }

    /// How many times the item's attributes were read.
    pub fn stamp_reads(&self) -> usize {
        self.stamps.load(Ordering::SeqCst)
    }

    /// How many times its password was read, for any reason.
    pub fn password_reads(&self) -> usize {
        self.passwords.load(Ordering::SeqCst)
    }

    /// How many of those were to turn live usage on.
    pub fn approval_reads(&self) -> usize {
        self.approvals.load(Ordering::SeqCst)
    }
}

impl SafeStorage for ScriptedSafeStorage {
    fn stamp(&self, _ctx: &Context) -> Result<ItemStamp, KeyReadError> {
        self.stamps.fetch_add(1, Ordering::SeqCst);
        self.key().stamp.clone().map_err(KeyReadError::from)
    }

    fn password(&self, _ctx: &Context, how: KeyRead) -> Result<Zeroizing<Vec<u8>>, KeyReadError> {
        self.passwords.fetch_add(1, Ordering::SeqCst);
        if how == KeyRead::Approve {
            self.approvals.fetch_add(1, Ordering::SeqCst);
        }
        let delay = self.key().delay;
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        let mut key = self.key();
        if how == KeyRead::Approve
            && let Some(mdat) = key.allowed_at.take()
            && let Ok(stamp) = key.stamp.as_mut()
        {
            stamp.mdat = mdat;
        }
        match &key.password {
            None if how == KeyRead::Approve => {
                drop(key);
                panic!("only turning live usage on may ask to read Claude's key");
            }
            None => Err(KeyReadError::Missing),
            Some(Ok(password)) => Ok(Zeroizing::new(password.clone())),
            Some(Err(trouble)) => Err((*trouble).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(uuid: &str) -> Owner {
        Owner {
            account_uuid: uuid.into(),
            email: "me@example.com".into(),
            organization_uuid: "org".into(),
        }
    }

    #[test]
    fn it_answers_what_it_was_told_and_refuses_what_it_was_not() {
        let api = ScriptedApi::new();
        api.owned_by("live", owner("acc"));
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"));

        assert_eq!(
            api.owner(&ctx, "live").expect("scripted").account_uuid,
            "acc"
        );
        assert!(matches!(
            api.owner(&ctx, "someone else"),
            Err(ApiError::Unauthorized)
        ));
    }

    #[test]
    fn it_produces_the_failures_a_server_cannot() {
        let api = ScriptedApi::new();
        api.token_trouble("live", Trouble::RateLimited);
        api.renew_trouble("stale", Trouble::InvalidGrant);
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"));

        assert!(matches!(
            api.owner(&ctx, "live"),
            Err(ApiError::RateLimited { .. })
        ));
        assert!(matches!(
            Api::usage(&*api, &ctx, "live"),
            Err(ApiError::RateLimited { .. })
        ));
        assert!(matches!(
            Api::renew(&*api, &ctx, "stale", &[], None),
            Err(ApiError::InvalidGrant)
        ));
    }

    #[test]
    fn it_remembers_what_it_was_asked() {
        let api = ScriptedApi::new();
        api.owned_by("live", owner("acc"));
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"));

        let _ = api.owner(&ctx, "live");
        let _ = Api::usage(&*api, &ctx, "live");
        let _ = api.owner(&ctx, "live");

        assert_eq!(api.calls(), 3);
        assert_eq!(
            api.asked(),
            vec![
                Asked::Owner("live".into()),
                Asked::Usage("live".into()),
                Asked::Owner("live".into()),
            ]
        );
    }
}
