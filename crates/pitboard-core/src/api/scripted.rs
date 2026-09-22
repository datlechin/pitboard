//! An Anthropic that answers what it was told to, and remembers what it was asked.
//!
//! The integration tests run a loopback server, which proves the parsing and the wiring.
//! What a server cannot produce on demand is the half that decides behaviour: a request
//! that never answers, a 429, a refresh token Anthropic has stopped accepting. Those are
//! the answers the engine has to be right about, and they were untestable.
//!
//! It also counts. How often pitboard asks is a design question in its own right, and a
//! test that can say "asked once, for two front ends" is how that stays true.

use super::{Api, ApiError, Owner, Renewed};
use crate::context::Context;
use crate::usage::Snapshot;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A failure the network can produce and a loopback server cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trouble {
    /// The access token has expired or been revoked.
    Unauthorized,
    RateLimited,
    /// No route to Anthropic at all: a plane, a captive portal, a bad morning.
    Offline,
    Server(u16),
    /// The refresh token was refused for good, which is the answer that ends a park.
    InvalidGrant,
}

impl From<Trouble> for ApiError {
    fn from(t: Trouble) -> ApiError {
        match t {
            Trouble::Unauthorized => ApiError::Unauthorized,
            Trouble::RateLimited => ApiError::RateLimited,
            Trouble::Offline => ApiError::Network("no route to host".into()),
            Trouble::Server(status) => ApiError::Unexpected { status },
            Trouble::InvalidGrant => ApiError::InvalidGrant,
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
}

#[derive(Debug, Default)]
struct Script {
    owners: HashMap<String, Answer<Owner>>,
    usage: HashMap<String, Answer<Snapshot>>,
    renewals: HashMap<String, Answer<Renewed>>,
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
    pub fn renew_trouble(&self, refresh_token: &str, trouble: Trouble) -> &ScriptedApi {
        self.script()
            .renewals
            .insert(refresh_token.into(), Answer::Fail(trouble));
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
            Err(ApiError::RateLimited)
        ));
        assert!(matches!(
            api.usage(&ctx, "live"),
            Err(ApiError::RateLimited)
        ));
        assert!(matches!(
            api.renew(&ctx, "stale", &[], None),
            Err(ApiError::InvalidGrant)
        ));
    }

    #[test]
    fn it_remembers_what_it_was_asked() {
        let api = ScriptedApi::new();
        api.owned_by("live", owner("acc"));
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"));

        let _ = api.owner(&ctx, "live");
        let _ = api.usage(&ctx, "live");
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
