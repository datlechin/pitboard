//! Claude Code behind the provider boundary.
//!
//! Every method here delegates to the code that was already doing the job, so this adds a
//! seam and changes no behaviour. What it proves is that the boundary is one Claude Code
//! can actually sit behind, which is worth knowing before a second tool is written against
//! it rather than after.

use super::{configfile, document, live, paths as claude};
use crate::api::{self, ApiError};
use crate::context::Context;
use crate::provider::{
    Adoption, Credential, Expiry, Identity, Isolation, LiveStore, ParkSemantics, Provider,
    ProviderError, ProviderId,
};
use crate::switch;
use crate::usage;
use serde_json::Value;
use std::path::PathBuf;

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

    fn live(&self, ctx: &Context) -> Result<LiveStore, ProviderError> {
        Ok(LiveStore {
            chain: live::chain(ctx),
            service: claude::live_service(ctx),
        })
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
        let oauth = document::oauth_in(&credential.raw);
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
            document::renewed(&credential.raw, &fresh, at_millis),
        ))
    }

    /// The directory lock Claude Code's own protocol takes around every write to its
    /// credential. A write that did not take it could land between Claude Code's read and
    /// its write of a refreshed token, and one of the two logins would be lost.
    fn write_lock(&self, ctx: &Context) -> Option<PathBuf> {
        Some(PathBuf::from(claude::storage_dir(ctx)).join(".storage-write"))
    }

    /// The identity cached in Claude Code's config, which it refreshes about once a day.
    fn recorded_identity(&self, ctx: &Context) -> Option<Identity> {
        let config = claude::load_config(ctx).ok()?;
        let found = claude::identity(&config)?;
        Some(Identity {
            account_id: found.account_uuid,
            email: found.email,
            group: Some(found.organization_uuid).filter(|o| !o.is_empty()),
        })
    }

    /// Record the new identity in Claude Code's config. Runs after the login is in place,
    /// so the config never names an account before its login is live. Claude Code does not
    /// correct a stale config on its own; it refetches its profile only once a day.
    fn after_switch(
        &self,
        ctx: &Context,
        incoming: &crate::state::Account,
        outgoing: &Identity,
    ) -> Result<(), crate::error::Error> {
        let path = claude::config_file(ctx);
        let outgoing_group = outgoing.group.clone().unwrap_or_default();
        configfile::backup(ctx, &path)?;
        configfile::update(ctx, &path, |config| {
            configfile::splice_identity(
                config,
                incoming.claude().map_or(&Value::Null, |c| c.oauth_account),
                &[outgoing.account_id.as_str(), outgoing_group.as_str()],
            )
        })
        .map(|_| ())
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
        document::slice(live).map_err(|detail| ProviderError::ShapeUnexpected {
            provider: ProviderId::Claude,
            detail,
        })
    }

    fn splice(&self, live: &Value, incoming: &Value) -> Result<Value, ProviderError> {
        document::splice(live, incoming).map_err(|detail| ProviderError::ShapeUnexpected {
            provider: ProviderId::Claude,
            detail,
        })
    }

    fn fingerprint(&self, slice: &Value) -> String {
        document::fingerprint_of(slice)
    }

    fn expiry(&self, slice: &Value) -> Expiry {
        let oauth = document::oauth_in(slice);
        // Claude Code records both in epoch milliseconds.
        let at = |key: &str| oauth.get(key).and_then(Value::as_i64).map(|ms| ms / 1000);
        Expiry {
            access_expires_at: at("expiresAt"),
            refresh_expires_at: at("refreshTokenExpiresAt"),
        }
    }
}

fn access_token(login: &Value) -> Option<&str> {
    document::oauth_in(login)
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
