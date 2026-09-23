//! Claude Code behind the provider boundary.
//!
//! Every method here delegates to the code that was already doing the job, so this adds a
//! seam and changes no behaviour. What it proves is that the boundary is one Claude Code
//! can actually sit behind, which is worth knowing before a second tool is written against
//! it rather than after.

use super::{live, paths as claude, slot};
use crate::api::{self, ApiError};
use crate::context::Context;
use crate::park;
use crate::provider::{
    Adoption, Credential, Expiry, Identity, Isolation, ParkSemantics, Provider, ProviderError,
    ProviderId,
};
use crate::switch;
use crate::usage;
use serde_json::Value;

/// Claude Code. A zero-sized value: everything it needs comes from the context.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Claude;

/// Claude Code serves its credential from a 30 second cache, so a session already running
/// picks a switch up on its own. Measured against a running session; the three seconds of
/// margin are for the round trip that follows the cache expiring.
const ADOPTION_SECONDS: u32 = switch::ADOPTION_CEILING_SECONDS;

impl Provider for Claude {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn read_live(&self, ctx: &Context) -> Result<Option<Credential>, ProviderError> {
        let service = claude::live_service(ctx);
        crate::store::read(&live::chain(ctx), &service)
            .map(|found| found.map(|raw| Credential::new(ProviderId::Claude, raw)))
            .map_err(store_error)
    }

    /// Asked of Anthropic, never read from Claude Code's config, which can lag the
    /// credential by a day. That round trip is the reason a switch needs the network at
    /// all, and it is deliberate: filing a login under the wrong account is worse than
    /// refusing to file it.
    fn identify(&self, ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError> {
        let token =
            access_token(&credential.raw).ok_or_else(|| ProviderError::ShapeUnexpected {
                provider: ProviderId::Claude,
                detail: "it has no claudeAiOauth.accessToken".into(),
            })?;
        api::owner(ctx, token).map(from_owner).map_err(from_api)
    }

    fn usage(
        &self,
        ctx: &Context,
        credential: &Credential,
    ) -> Result<usage::Snapshot, ProviderError> {
        let token =
            access_token(&credential.raw).ok_or_else(|| ProviderError::ShapeUnexpected {
                provider: ProviderId::Claude,
                detail: "it has no claudeAiOauth.accessToken".into(),
            })?;
        api::usage(ctx, token).map_err(from_api)
    }

    fn renew(&self, ctx: &Context, credential: &Credential) -> Result<Credential, ProviderError> {
        let oauth = park::oauth_in(&credential.raw);
        let refresh = oauth["refreshToken"].as_str().unwrap_or_default();
        let mut scopes: Vec<String> = oauth["scopes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        if scopes.is_empty() {
            scopes = switch::renew::DEFAULT_SCOPES.map(str::to_owned).to_vec();
        }
        let client_id = oauth["clientId"].as_str();
        let fresh = api::renew(ctx, refresh, &scopes, client_id).map_err(from_api)?;
        // Anchored to the answer's own clock where it sent one, so a machine whose clock is
        // wrong does not write an expiry that is wrong with it.
        let at_millis = fresh.at.map_or_else(|| ctx.now_millis(), |at| at * 1000);
        Ok(Credential::new(
            ProviderId::Claude,
            park::renewed(&credential.raw, &fresh, at_millis),
        ))
    }

    /// Writes where the credential already lives and reads it back.
    ///
    /// Rolling a failed write back, and naming the two accounts when it cannot, belongs to
    /// the switch: it is pitboard's own bookkeeping and does not vary by tool.
    fn install_live(&self, ctx: &Context, credential: &Credential) -> Result<(), ProviderError> {
        let service = claude::live_service(ctx);
        let body = serde_json::to_string(&credential.raw)
            .expect("a credential document stays serialisable");
        crate::store::write_raw(&live::chain(ctx), &service, &body).map_err(store_error)
    }

    fn adoption(&self) -> Adoption {
        Adoption::PollingWithin(ADOPTION_SECONDS)
    }

    /// A copy may sit in the vault while the same login is still signed in: nothing of
    /// Claude Code's revokes for presenting either, and the live document holds the
    /// machine's other keys, which have to stay where they are.
    fn park_semantics(&self) -> ParkSemantics {
        ParkSemantics::CopyWhileLive
    }

    /// `CLAUDE_CONFIG_DIR` selects the keychain item by hashing the directory, so a scratch
    /// directory's item is one nothing else reads. There is no second backend that escapes
    /// it.
    fn private_signin_isolation(&self, _ctx: &Context) -> Isolation {
        Isolation::Isolated
    }

    fn slice(&self, live: &Value) -> Result<Value, ProviderError> {
        switch::slice_of(live).map_err(|e| ProviderError::ShapeUnexpected {
            provider: ProviderId::Claude,
            detail: e.to_string(),
        })
    }

    fn splice(&self, live: &Value, incoming: &Value) -> Result<Value, ProviderError> {
        let written =
            switch::splice(live, incoming).map_err(|e| ProviderError::ShapeUnexpected {
                provider: ProviderId::Claude,
                detail: e.to_string(),
            })?;
        serde_json::from_str(&written).map_err(|e| ProviderError::Malformed(e.to_string()))
    }

    fn fingerprint(&self, slice: &Value) -> String {
        park::fingerprint_of(slice)
    }

    fn expiry(&self, slice: &Value) -> Expiry {
        let oauth = park::oauth_in(slice);
        // Claude Code records both in epoch milliseconds.
        let at = |key: &str| oauth.get(key).and_then(Value::as_i64).map(|ms| ms / 1000);
        Expiry {
            access_expires_at: at("expiresAt"),
            refresh_expires_at: at("refreshTokenExpiresAt"),
        }
    }
}

fn access_token(document: &Value) -> Option<&str> {
    park::oauth_in(document)
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
}

fn from_owner(owner: api::Owner) -> Identity {
    Identity {
        account_id: owner.account_uuid,
        email: owner.email,
        // An account outside an organisation reports an empty string, which is not the same
        // as a tool that has no organisations at all.
        group: Some(owner.organization_uuid).filter(|o| !o.is_empty()),
    }
}

fn from_api(error: ApiError) -> ProviderError {
    match error {
        ApiError::Unauthorized => ProviderError::Unauthorized,
        ApiError::RateLimited { retry_after } => ProviderError::RateLimited { retry_after },
        ApiError::Network(detail) => ProviderError::Network {
            service: ProviderId::Claude.service(),
            detail,
        },
        ApiError::Unexpected { status } => ProviderError::Unexpected {
            service: ProviderId::Claude.service(),
            status,
        },
        ApiError::Malformed(detail) => ProviderError::Malformed(detail),
        ApiError::InvalidGrant => ProviderError::InvalidGrant,
    }
}

fn store_error(error: crate::store::Error) -> ProviderError {
    match error {
        crate::store::Error::Malformed(detail) => ProviderError::Malformed(detail),
        other => ProviderError::Network {
            service: "this machine's credential store",
            detail: other.to_string(),
        },
    }
}

/// Named so a reader meeting `slot` in this file knows where it went.
#[allow(unused_imports)]
use slot as _;
