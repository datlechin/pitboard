//! What live usage asks claude.ai, with the session Claude Desktop is signed in with.
//!
//! The one request is the usage of one organisation, as the app's own settings page reads
//! it, and its answer has the shape Anthropic's OAuth usage endpoint has. Nothing claude.ai
//! sets in return is kept: a `Set-Cookie` is never read, so the jar stays Claude's.

use crate::api::ApiError;
use crate::context::Context;
use crate::usage::{self, Snapshot};

const BASE: &str = "https://claude.ai";

/// What stops a request when Cloudflare says, by `cf-mitigated`, that it did.
pub(crate) const BOT_CHECK: &str = "claude.ai's bot check";

/// claude.ai, as a seam: the network in every real context, a script in the tests.
pub(crate) trait ClaudeWeb: Send + Sync + std::fmt::Debug {
    fn usage(&self, ctx: &Context, org: &str, session_key: &str) -> Result<Snapshot, ApiError>;
}

/// claude.ai over the network.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClaudeAi;

/// Where claude.ai is reached instead, for tests: loopback only, so a session never leaves
/// the machine for an address somebody put in the environment.
fn base(ctx: &Context) -> String {
    ctx.web_base
        .clone()
        .filter(|url| crate::api::is_loopback(url))
        .unwrap_or_else(|| BASE.to_string())
}

impl ClaudeWeb for ClaudeAi {
    fn usage(&self, ctx: &Context, org: &str, session_key: &str) -> Result<Snapshot, ApiError> {
        // An organisation id is a uuid; anything else would change the path asked for.
        if org.is_empty() || !org.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err(ApiError::Malformed("not an organisation id".into()));
        }
        // The header holds the session in the clear, so it is wiped like the session is.
        let cookie = zeroize::Zeroizing::new(format!("sessionKey={session_key}"));
        let mut response = crate::api::agent()
            .get(format!("{}/api/organizations/{org}/usage", base(ctx)))
            .header("Cookie", cookie.as_str())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let bot_check = response.headers().contains_key("cf-mitigated");
        let wait = crate::api::retry_after(response.headers());
        match judge(status, bot_check, wait) {
            Some(refused) => Err(refused),
            None => {
                let body = response
                    .body_mut()
                    .read_to_string()
                    .map_err(|e| ApiError::Network(e.to_string()))?;
                let value: serde_json::Value =
                    serde_json::from_str(&body).map_err(|e| ApiError::Malformed(e.to_string()))?;
                Ok(usage::from_usage_object(&value, ctx.now()))
            }
        }
    }
}

/// What an answer's status says, before its body is read. `None` is an answer to read.
///
/// A 403 is a refused session, unless Cloudflare says it stopped the request itself:
/// that is claude.ai's bot check, which says nothing about the session and is reported by
/// name, as an answer to try again later rather than as a sign-out.
fn judge(status: u16, bot_check: bool, retry_after: Option<i64>) -> Option<ApiError> {
    match status {
        200 => None,
        401 => Some(ApiError::Unauthorized),
        403 if bot_check => Some(ApiError::Blocked {
            status: 403,
            by: BOT_CHECK,
        }),
        403 => Some(ApiError::Unauthorized),
        429 => Some(ApiError::RateLimited { retry_after }),
        status => Some(ApiError::Unexpected { status }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_is_read_as_claude_ai_means_it() {
        assert!(judge(200, false, None).is_none());
        assert!(matches!(
            judge(401, false, None),
            Some(ApiError::Unauthorized)
        ));
        assert!(matches!(
            judge(403, false, None),
            Some(ApiError::Unauthorized)
        ));
        assert!(matches!(
            judge(429, false, Some(30)),
            Some(ApiError::RateLimited {
                retry_after: Some(30)
            })
        ));
        assert!(matches!(
            judge(500, false, None),
            Some(ApiError::Unexpected { status: 500 })
        ));
    }

    /// claude.ai's bot check is not a sign-out, and is never reported as one: it is named,
    /// so whoever reads it knows what stopped the request.
    #[test]
    fn the_bot_check_is_not_an_expired_session() {
        let refused = judge(403, true, None).expect("refused");
        assert!(
            matches!(refused, ApiError::Blocked { status: 403, by } if by == BOT_CHECK),
            "{refused:?}"
        );
        assert!(
            refused.to_string().contains("claude.ai's bot check"),
            "{refused}"
        );
        assert_ne!(
            crate::error::Cause::of(&refused),
            crate::error::Cause::TokenExpired
        );
    }

    /// Only loopback may stand in for claude.ai, so a session never goes anywhere else.
    #[test]
    fn only_loopback_stands_in_for_claude_ai() {
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"));
        assert_eq!(base(&ctx), "https://claude.ai");
        let mut local = ctx.clone();
        local.web_base = Some("http://127.0.0.1:4000".into());
        assert_eq!(base(&local), "http://127.0.0.1:4000");
        let mut away = ctx;
        away.web_base = Some("https://claude.example.com".into());
        assert_eq!(base(&away), "https://claude.ai");
    }

    /// An organisation id that would change the path is not sent anywhere.
    #[test]
    fn an_odd_organisation_is_never_asked_about() {
        let mut ctx = Context::new(std::path::PathBuf::from("/nowhere"));
        // Unroutable on purpose: the request must fail before it is made.
        ctx.web_base = Some("http://127.0.0.1:9".into());
        for org in ["", "../account", "a/b", "org?x=1"] {
            assert!(matches!(
                ClaudeAi.usage(&ctx, org, "not-a-session"),
                Err(ApiError::Malformed(_))
            ));
        }
    }
}
