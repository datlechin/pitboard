//! What pitboard asks of Anthropic: who a login belongs to and how much it has left, both
//! read with an access token, and fresh tokens for a parked login.
//!
//! Renewing is only ever done for a parked login, which pitboard alone holds. The login
//! signed in is Claude Code's to renew: two holders renewing one refresh chain would break
//! it for both.

use crate::usage::{self, Snapshot};
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

const BASE: &str = "https://api.anthropic.com";
const AUTH_BASE: &str = "https://platform.claude.com";

/// Claude Code's own OAuth client, which every login it stores was issued to.
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// Where requests go instead, for tests. Nothing else may redirect them, because an address
/// that answers "this token belongs to account X" decides which account a credential is filed
/// under. Only loopback is accepted, so a token or an answer never leaves this machine.
fn test_base() -> Option<String> {
    std::env::var("PITBOARD_API_BASE")
        .ok()
        .filter(|url| is_loopback(url))
}

fn base() -> String {
    test_base().unwrap_or_else(|| BASE.to_string())
}

fn auth_base() -> String {
    test_base().unwrap_or_else(|| AUTH_BASE.to_string())
}

/// Judged on the parsed host, never on a prefix: `http://127.0.0.1:@elsewhere/` begins like
/// loopback and is not. A name is refused too, since the hosts file decides where it points.
fn is_loopback(url: &str) -> bool {
    let Ok(uri) = url.parse::<ureq::http::Uri>() else {
        return false;
    };
    uri.scheme_str() == Some("http")
        && uri.host().is_some_and(|host| {
            host.trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        })
}
const TIMEOUT: Duration = Duration::from_secs(5);

/// Who a token belongs to, as the server sees it. Unlike a refresh-token fingerprint, this
/// does not change when Claude Code rotates the token, and unlike Claude Code's config, it
/// cannot lag behind the credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub account_uuid: String,
    pub email: String,
    pub organization_uuid: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The access token has expired or been revoked.
    #[error("the session has expired")]
    Unauthorized,
    #[error("Anthropic is rate limiting this request")]
    RateLimited,
    #[error("could not reach Anthropic: {0}")]
    Network(String),
    #[error("Anthropic answered {status}")]
    Unexpected { status: u16 },
    #[error("Anthropic's answer was not understood: {0}")]
    Malformed(String),
    /// The refresh token was refused for good: revoked, or already used elsewhere.
    #[error("Anthropic no longer accepts this login")]
    InvalidGrant,
}

/// Fresh tokens for a parked login, as the token endpoint returns them.
#[derive(Debug, PartialEq)]
pub struct Renewed {
    pub access_token: String,
    /// `None` when the server keeps the refresh token it was given.
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    pub refresh_token_expires_in: Option<i64>,
    pub scopes: Option<Vec<String>>,
}

fn agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        // rustls's documented way to choose a crypto provider. Returns Err only when one is
        // already installed, which is exactly the state wanted.
        let _ = rustls::crypto::ring::default_provider().install_default();
        Agent::config_builder()
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::Rustls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .user_agent(concat!("pitboard/", env!("CARGO_PKG_VERSION")))
            .build()
            .new_agent()
    })
}

fn get(path: &str, access_token: &str) -> Result<Value, ApiError> {
    let mut response = agent()
        .get(format!("{}{path}", base()))
        .header("Authorization", format!("Bearer {access_token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .call()
        .map_err(|e| ApiError::Network(e.to_string()))?;
    match response.status().as_u16() {
        200 => {
            let body = response
                .body_mut()
                .read_to_string()
                .map_err(|e| ApiError::Network(e.to_string()))?;
            serde_json::from_str(&body).map_err(|e| ApiError::Malformed(e.to_string()))
        }
        401 | 403 => Err(ApiError::Unauthorized),
        429 => Err(ApiError::RateLimited),
        status => Err(ApiError::Unexpected { status }),
    }
}

/// The request Claude Code makes to renew its own login, for a parked one. The scopes asked
/// for are the ones the login already has, so the answer can never be `invalid_scope`.
pub fn renew(refresh_token: &str, scopes: &[String]) -> Result<Renewed, ApiError> {
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLIENT_ID,
        "scope": scopes.join(" "),
    });
    let mut response = agent()
        .post(format!("{}/v1/oauth/token", auth_base()))
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|e| ApiError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| ApiError::Network(e.to_string()))?;
    match status {
        200 => {
            let body: Value =
                serde_json::from_str(&text).map_err(|e| ApiError::Malformed(e.to_string()))?;
            parse_renewed(&body)
        }
        429 => Err(ApiError::RateLimited),
        400..=499 if text.contains("invalid_grant") => Err(ApiError::InvalidGrant),
        status => Err(ApiError::Unexpected { status }),
    }
}

fn parse_renewed(body: &Value) -> Result<Renewed, ApiError> {
    let text = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(Renewed {
        access_token: text("access_token")
            .ok_or_else(|| ApiError::Malformed("no access_token".into()))?,
        refresh_token: text("refresh_token"),
        expires_in: body
            .get("expires_in")
            .and_then(Value::as_i64)
            .ok_or_else(|| ApiError::Malformed("no expires_in".into()))?,
        refresh_token_expires_in: body.get("refresh_token_expires_in").and_then(Value::as_i64),
        scopes: text("scope").map(|s| s.split_whitespace().map(str::to_owned).collect()),
    })
}

pub fn owner(access_token: &str) -> Result<Owner, ApiError> {
    let body = get("/api/oauth/profile", access_token)?;
    parse_owner(&body)
}

pub fn usage(access_token: &str) -> Result<Snapshot, ApiError> {
    let body = get("/api/oauth/usage", access_token)?;
    Ok(usage::from_usage_object(&body, crate::time::now()))
}

fn parse_owner(body: &Value) -> Result<Owner, ApiError> {
    let text = |path: &[&str]| -> Result<String, ApiError> {
        path.iter()
            .try_fold(body, |v, key| v.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| ApiError::Malformed(format!("no {}", path.join("."))))
    };
    Ok(Owner {
        account_uuid: text(&["account", "uuid"])?,
        email: text(&["account", "email"])?,
        organization_uuid: text(&["organization", "uuid"])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_renewal_answer_is_read_as_claude_code_reads_it() {
        let full = json!({
            "access_token": "a2", "refresh_token": "r2", "expires_in": 28_800,
            "refresh_token_expires_in": 2_592_000, "scope": "user:inference user:profile",
            "token_type": "Bearer"
        });
        assert_eq!(
            parse_renewed(&full).unwrap(),
            Renewed {
                access_token: "a2".into(),
                refresh_token: Some("r2".into()),
                expires_in: 28_800,
                refresh_token_expires_in: Some(2_592_000),
                scopes: Some(vec!["user:inference".into(), "user:profile".into()]),
            }
        );
        let kept = parse_renewed(&json!({"access_token": "a2", "expires_in": 60})).unwrap();
        assert_eq!(
            kept.refresh_token, None,
            "no refresh_token means the old one stays"
        );
        assert!(parse_renewed(&json!({"expires_in": 60})).is_err());
    }

    /// Field names copied from a live response on 2026-09-21.
    #[test]
    fn reads_the_owner_from_the_shape_the_profile_endpoint_returns() {
        let body = json!({
            "account": {
                "uuid": "acc", "email": "a@b.c", "display_name": "A", "full_name": "A",
                "created_at": "2025-10-15T02:36:35Z", "has_claude_max": true, "has_claude_pro": false
            },
            "organization": {
                "uuid": "org", "name": "Org", "organization_type": "claude_max",
                "rate_limit_tier": "default_claude_max_20x", "billing_type": "stripe_subscription"
            },
            "application": {}, "enabled_plugins": []
        });
        assert_eq!(
            parse_owner(&body).unwrap(),
            Owner {
                account_uuid: "acc".into(),
                email: "a@b.c".into(),
                organization_uuid: "org".into(),
            }
        );
    }

    #[test]
    fn only_a_loopback_address_can_redirect_requests() {
        for allowed in ["http://127.0.0.1:8080", "http://[::1]:9"] {
            assert!(is_loopback(allowed), "{allowed}");
        }
        for refused in [
            "https://evil.example.com",
            "http://127.0.0.1.evil.example.com:80",
            "http://localhost.evil.example.com:80",
            "http://127.0.0.1:@evil.example.com/",
            "http://127.0.0.1:8080@evil.example.com/",
            "http://localhost:1",
            "https://127.0.0.1:443",
            "http://10.0.0.1:80",
            "",
        ] {
            assert!(
                !is_loopback(refused),
                "{refused} must not be able to answer identity"
            );
        }
    }

    #[test]
    fn a_profile_missing_the_account_is_malformed_rather_than_guessed() {
        assert!(matches!(
            parse_owner(&json!({"organization": {"uuid": "org"}})),
            Err(ApiError::Malformed(_))
        ));
    }
}
