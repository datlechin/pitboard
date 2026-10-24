//! An Anthropic that answers what it was told to, and remembers what it was asked.
//!
//! The integration tests run a loopback server, which proves the parsing and the wiring.
//! What a server cannot produce on demand is the half that decides behaviour: a request
//! that never answers, a 429, a refresh token Anthropic has stopped accepting. Those are
//! the answers the engine has to be right about, and they were untestable.
//!
//! It also counts. How often Pitboard asks is a design question in its own right, and a
//! test that can say "asked once, for two front ends" is how that stays true.

use crate::context::Context;
use crate::permit::Permit;
use crate::provider::Owner;
use crate::provider::ProviderError;
use crate::provider::claude::api::{Api, Renewed, Validated};
use crate::provider::codex::api::{Authority, Fresh, Login, OpenAi};
use crate::usage::Snapshot;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    /// The service answered this status with this body.
    Answered(u16, &'static str),
    Server(u16),
    /// The refresh token was refused for good, which is the answer that ends a park.
    InvalidGrant,
    /// The account is on hold, which ends nothing.
    OnHold,
}

/// What a scripted endpoint does when it is asked.
#[derive(Debug, Clone)]
pub enum Answer<T> {
    Give(T),
    Fail(Trouble),
}

impl<T: Clone> Answer<T> {
    fn take(&self, service: &'static str) -> Result<T, ProviderError> {
        match self {
            Answer::Give(value) => Ok(value.clone()),
            Answer::Fail(trouble) => Err(trouble.of(service)),
        }
    }
}

/// One question that was asked, for a test that cares how often.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Which login an access token is.
    Identity(String),
    /// Whose profile an access token reads.
    Owner(String),
    Usage(String),
    Renew(String),
}

#[derive(Debug, Default)]
struct Script {
    owners: HashMap<String, Answer<Owner>>,
    /// Tokens `validate` answers with no organisation for.
    unorganised: std::collections::HashSet<String>,
    /// What the profile answers in place of what `owners` says, for a token whose identity
    /// is told and whose profile is not.
    profiles: HashMap<String, Answer<Owner>>,
    usage: HashMap<String, Answer<Snapshot>>,
    renewals: HashMap<String, Answer<Renewed>>,
    codex_renewals: HashMap<String, Answer<Fresh>>,
    codex_renewed_as: Vec<(String, String)>,
    asked: Vec<Asked>,
    /// What happens elsewhere while a renewal of a refresh token is answered, once.
    answering: HashMap<String, Meanwhile>,
}

struct Meanwhile(Box<dyn FnOnce() + Send>);

impl std::fmt::Debug for Meanwhile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Meanwhile")
    }
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

    /// `validate` names this access token's account and no organisation.
    pub fn validated_without_organisation(&self, access_token: &str) -> &ScriptedApi {
        self.script().unorganised.insert(access_token.into());
        self
    }

    /// Asking for this access token's profile goes wrong, and nothing else.
    pub fn profile_trouble(&self, access_token: &str, trouble: Trouble) -> &ScriptedApi {
        self.script()
            .profiles
            .insert(access_token.into(), Answer::Fail(trouble));
        self
    }

    /// Asking about this access token goes wrong, every question.
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

    // Each Codex renewal's address and client, in the order asked.
    pub fn codex_renewed_as(&self) -> Vec<(String, String)> {
        self.script().codex_renewed_as.clone()
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

    /// Runs `meanwhile` while the next renewal of this refresh token is answered: once it has
    /// been sent, before the answer is back.
    pub fn while_renewing(
        &self,
        refresh_token: &str,
        meanwhile: impl FnOnce() + Send + 'static,
    ) -> &ScriptedApi {
        self.script()
            .answering
            .insert(refresh_token.into(), Meanwhile(Box::new(meanwhile)));
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
    ) -> Result<T, ProviderError> {
        let mut script = self.script();
        script.asked.push(asked);
        match pick(&script) {
            Some(answer) => answer.take(crate::provider::ProviderId::Claude.service()),
            // An unknown token is exactly what Anthropic refuses.
            None => Err(ProviderError::Unauthorized),
        }
    }
}

impl Api for ScriptedApi {
    fn validate(&self, _ctx: &Context, access_token: &str) -> Result<Validated, ProviderError> {
        let unorganised = self.script().unorganised.contains(access_token);
        self.answer(Asked::Identity(access_token.into()), |s| {
            s.owners.get(access_token)
        })
        .map(|owner| Validated {
            account_uuid: owner.account_uuid,
            organization_uuid: (!unorganised).then_some(owner.organization_uuid),
        })
    }

    fn owner(&self, _ctx: &Context, access_token: &str) -> Result<Owner, ProviderError> {
        self.answer(Asked::Owner(access_token.into()), |s| {
            s.profiles
                .get(access_token)
                .or_else(|| s.owners.get(access_token))
        })
    }

    fn usage(&self, _ctx: &Context, access_token: &str) -> Result<Snapshot, ProviderError> {
        self.answer(Asked::Usage(access_token.into()), |s| {
            s.usage.get(access_token)
        })
    }

    fn renew(
        &self,
        _ctx: &Context,
        _: Permit,
        refresh_token: &str,
        _scopes: &[String],
        _client_id: Option<&str>,
    ) -> Result<Renewed, ProviderError> {
        let answer = self.answer(Asked::Renew(refresh_token.into()), |s| {
            s.renewals.get(refresh_token)
        });
        let meanwhile = self.script().answering.remove(refresh_token);
        if let Some(Meanwhile(meanwhile)) = meanwhile {
            meanwhile();
        }
        answer
    }
}

impl Trouble {
    /// This failure as `service`'s side of the boundary reports it.
    fn of(self, service: &'static str) -> ProviderError {
        let answered = |refusal| ProviderError::answered(service, refusal);
        match self {
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
            Trouble::Answered(status, body) => answered(crate::api::Refusal::read(status, body)),
            Trouble::Server(status) => answered(crate::api::Refusal::read(status, "")),
            Trouble::InvalidGrant => ProviderError::InvalidGrant { service },
            Trouble::OnHold => ProviderError::OnHold {
                service,
                link: None,
            },
        }
    }
}

/// Usage is scripted by access token whichever service it is asked of, because a token
/// names one login and a test gives each login its own.
impl OpenAi for ScriptedApi {
    fn usage(
        &self,
        _ctx: &Context,
        access_token: &str,
        _login: Login<'_>,
        _now: i64,
    ) -> Result<Snapshot, ProviderError> {
        let mut script = self.script();
        script.asked.push(Asked::Usage(access_token.into()));
        match script.usage.get(access_token) {
            Some(Answer::Give(snapshot)) => Ok(snapshot.clone()),
            Some(Answer::Fail(trouble)) => {
                Err(trouble.of(crate::provider::ProviderId::Codex.service()))
            }
            None => Err(ProviderError::Unauthorized),
        }
    }

    fn renew(
        &self,
        _ctx: &Context,
        _: Permit,
        refresh_token: &str,
        authority: &Authority,
    ) -> Result<Fresh, ProviderError> {
        let service = crate::provider::ProviderId::Codex.service();
        let mut script = self.script();
        script.asked.push(Asked::Renew(refresh_token.into()));
        script
            .codex_renewed_as
            .push((authority.token_url(), authority.client_id.clone()));
        match script.codex_renewals.get(refresh_token) {
            Some(Answer::Give(fresh)) => Ok(fresh.clone()),
            Some(Answer::Fail(trouble)) => Err(trouble.of(service)),
            None => Err(ProviderError::InvalidGrant { service }),
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
            Err(ProviderError::Unauthorized)
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
            Err(ProviderError::RateLimited { .. })
        ));
        assert!(matches!(
            Api::usage(&*api, &ctx, "live"),
            Err(ProviderError::RateLimited { .. })
        ));
        assert!(matches!(
            Api::renew(&*api, &ctx, Permit::for_a_test(), "stale", &[], None),
            Err(ProviderError::InvalidGrant { .. })
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
