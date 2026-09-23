//! What pitboard asks OpenAI about a Codex login.
//!
//! Read from codex-cli 0.154.0. Two requests: one to exchange a refresh token, one to ask
//! how much of a plan is left. Neither is a model request and neither costs quota.

use crate::api::{agent, retry_after, server_time, test_base};
use crate::context::Context;
use crate::provider::{ProviderError, ProviderId};
use crate::usage::{Snapshot, Source, Window};
use serde_json::Value;

/// The client Codex renews as. Its own, not a first-party one: a login issued to this
/// client can only be renewed as it.
pub(crate) const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const USAGE_BASE: &str = "https://chatgpt.com/backend-api";

fn token_url(ctx: &Context) -> String {
    test_base(ctx).map_or_else(
        || TOKEN_URL.to_string(),
        |base| format!("{base}/oauth/token"),
    )
}

fn usage_url(ctx: &Context) -> String {
    test_base(ctx).map_or_else(
        || format!("{USAGE_BASE}/wham/usage"),
        |base| format!("{base}/wham/usage"),
    )
}

fn network(detail: String) -> ProviderError {
    ProviderError::Network {
        service: ProviderId::Codex.service(),
        detail,
    }
}

fn malformed(detail: String) -> ProviderError {
    ProviderError::Malformed {
        service: ProviderId::Codex.service(),
        detail,
    }
}

fn unexpected(status: u16) -> ProviderError {
    ProviderError::Unexpected {
        service: ProviderId::Codex.service(),
        status,
    }
}

/// Fresh tokens for a refresh chain.
///
/// The body is JSON rather than form-encoded, which is what Codex sends and is not what its
/// own authorisation-code exchange sends. Getting that wrong is a 400 with nothing useful
/// in it.
#[derive(Debug, Clone)]
pub struct Fresh {
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    /// The server's own clock, from the `Date` header, in epoch seconds.
    pub at: Option<i64>,
}

/// What pitboard asks OpenAI, as a seam, for the same reason Anthropic's is one: a loopback
/// server proves the parsing, and cannot produce on demand the timeouts, 429s and refused
/// refresh tokens the engine has to be right about.
pub(crate) trait OpenAi: Send + Sync + std::fmt::Debug {
    fn usage(
        &self,
        ctx: &Context,
        access_token: &str,
        account_id: &str,
        now: i64,
    ) -> Result<Snapshot, ProviderError>;

    fn renew(&self, ctx: &Context, refresh_token: &str) -> Result<Fresh, ProviderError>;
}

/// OpenAI, over the network. What every real context uses.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Network;

impl OpenAi for Network {
    fn usage(
        &self,
        ctx: &Context,
        access_token: &str,
        account_id: &str,
        now: i64,
    ) -> Result<Snapshot, ProviderError> {
        ask_usage(ctx, access_token, account_id, now)
    }

    fn renew(&self, ctx: &Context, refresh_token: &str) -> Result<Fresh, ProviderError> {
        ask_renew(ctx, refresh_token)
    }
}

/// Ask OpenAI through this context.
pub(crate) fn renew(ctx: &Context, refresh_token: &str) -> Result<Fresh, ProviderError> {
    ctx.openai().renew(ctx, refresh_token)
}

pub(crate) fn usage(
    ctx: &Context,
    access_token: &str,
    account_id: &str,
    now: i64,
) -> Result<Snapshot, ProviderError> {
    ctx.openai().usage(ctx, access_token, account_id, now)
}

fn ask_renew(ctx: &Context, refresh_token: &str) -> Result<Fresh, ProviderError> {
    let body = serde_json::json!({
        "client_id": CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
    });
    let mut response = agent()
        .post(token_url(ctx))
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|e| network(e.to_string()))?;
    let status = response.status().as_u16();
    let at = server_time(response.headers());
    let wait = retry_after(response.headers());
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| network(e.to_string()))?;
    match status {
        200 => {
            let body: Value = serde_json::from_str(&text).map_err(|e| malformed(e.to_string()))?;
            let string = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
            let fresh = Fresh {
                id_token: string("id_token"),
                access_token: string("access_token"),
                refresh_token: string("refresh_token"),
                at,
            };
            if fresh.access_token.is_none() {
                return Err(malformed("the answer carried no access token".into()));
            }
            Ok(fresh)
        }
        429 => Err(ProviderError::RateLimited {
            service: ProviderId::Codex.service(),
            retry_after: wait,
        }),
        other if refused_for_good(other, &text) => Err(ProviderError::InvalidGrant {
            service: ProviderId::Codex.service(),
        }),
        other => Err(unexpected(other)),
    }
}

/// Whether a failed exchange means the refresh chain is finished, read the way Codex reads
/// it.
///
/// A 401 always does. A 400 does only when it says so: `invalid_grant`, which is how the
/// current server reports a token that was spent or revoked, or one of the older codes that
/// named which (`refresh_token_expired`, `refresh_token_reused`,
/// `refresh_token_invalidated`). Any other 400 is a request the server did not like, and
/// reading that as a dead login would drop a park that works. The code can be the `error`
/// string, `error.code`, or a top-level `code`.
fn refused_for_good(status: u16, body: &str) -> bool {
    if status == 401 {
        return true;
    }
    let Ok(body) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    let code = body
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| body.pointer("/error/code").and_then(Value::as_str))
        .or_else(|| body.get("code").and_then(Value::as_str));
    match code {
        Some("refresh_token_expired" | "refresh_token_reused" | "refresh_token_invalidated") => {
            true
        }
        Some("invalid_grant") => status == 400,
        _ => false,
    }
}

/// How much of the plan is left, for the account this access token belongs to.
///
/// A standalone GET: no model request, no quota spent. `ChatGPT-Account-ID` is required and
/// comes out of the login document, which is why usage takes the whole credential rather
/// than a bare token.
fn ask_usage(
    ctx: &Context,
    access_token: &str,
    account_id: &str,
    now: i64,
) -> Result<Snapshot, ProviderError> {
    let mut response = agent()
        .get(usage_url(ctx))
        .header("Authorization", format!("Bearer {access_token}"))
        .header("ChatGPT-Account-ID", account_id)
        .call()
        .map_err(|e| network(e.to_string()))?;
    match response.status().as_u16() {
        200 => {
            let text = response
                .body_mut()
                .read_to_string()
                .map_err(|e| network(e.to_string()))?;
            let body: Value = serde_json::from_str(&text).map_err(|e| malformed(e.to_string()))?;
            Ok(snapshot(&body, account_id, now))
        }
        401 | 403 => Err(ProviderError::Unauthorized),
        429 => Err(ProviderError::RateLimited {
            service: ProviderId::Codex.service(),
            retry_after: retry_after(response.headers()),
        }),
        other => Err(unexpected(other)),
    }
}

/// OpenAI's answer in pitboard's own shape.
///
/// Measured against the live endpoint rather than taken from a description of it. The
/// windows are under `rate_limit`, the length is `limit_window_seconds` and the reset is
/// `reset_at`; a reading of the source had all three somewhere else, and the parser built
/// from it returned no windows at all while the request itself succeeded.
///
/// A window that will not normalise is dropped rather than drawn, the same rule the Claude
/// Code side has always used: a number nobody can explain is worse than no number.
fn snapshot(body: &Value, account_id: &str, now: i64) -> Snapshot {
    let limits = &body["rate_limit"];
    let windows = ["primary_window", "secondary_window"]
        .into_iter()
        .filter_map(|which| window(limits.get(which)?, which))
        .collect();
    Snapshot {
        windows,
        observed_at: Some(now),
        account_uuid: Some(account_id.to_string()),
        source: Source::Live,
    }
}

fn window(value: &Value, which: &str) -> Option<Window> {
    let percent = value.get("used_percent").and_then(Value::as_f64)?;
    if !percent.is_finite() || percent < 0.0 {
        return None;
    }
    let seconds = value.get("limit_window_seconds").and_then(Value::as_i64);
    Some(Window {
        kind: kind(seconds, which),
        scope: None,
        percent,
        resets_at: value.get("reset_at").and_then(moment),
        // OpenAI says which window a request is being judged against only by filling one
        // in, so every window it sends is one that counts.
        is_active: true,
        severity: None,
        length_seconds: seconds.filter(|s| *s > 0),
    })
}

/// The same vocabulary the rest of pitboard already uses where the lengths match, so a
/// five-hour window reads as one whichever tool it came from. Anything else is named by its
/// own length rather than forced into a word that would be wrong.
fn kind(seconds: Option<i64>, which: &str) -> String {
    const HOUR: i64 = 3600;
    match seconds {
        Some(18_000) => "five_hour".into(),
        Some(604_800) => "seven_day".into(),
        Some(s) if s % (HOUR * 24) == 0 => format!("{}_day", s / (HOUR * 24)),
        Some(s) if s % HOUR == 0 => format!("{}_hour", s / HOUR),
        Some(s) if s % 60 == 0 => format!("{}_minute", s / 60),
        Some(s) => format!("{s}_second"),
        // `primary_window` and `secondary_window` say which one it is and nothing about how
        // long it runs, so that is what it is called rather than a guess.
        None => which.trim_end_matches("_window").to_string(),
    }
}

/// Epoch seconds, however the answer put them.
///
/// Measured as a number. The string form is read too because the field is a timestamp and
/// reading one shape while silently dropping the other would show a limit with no reset
/// time and no reason.
fn moment(value: &Value) -> Option<i64> {
    if let Some(seconds) = value.as_i64() {
        // A value in milliseconds would be around a thousand times too large; nothing
        // measured has sent one, and a date in the year 55000 is worth refusing.
        return (seconds > 0 && seconds < 100_000_000_000).then_some(seconds);
    }
    value
        .as_str()?
        .parse::<jiff::Timestamp>()
        .ok()
        .map(|t| t.as_second())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only a refusal that says the chain is finished drops a park. A 400 for a malformed
    /// request used to read as a dead login, which would throw away one that works.
    #[test]
    fn only_a_finished_chain_is_refused_for_good() {
        assert!(refused_for_good(401, ""));
        assert!(refused_for_good(400, r#"{"error":"invalid_grant"}"#));
        for code in [
            "refresh_token_expired",
            "refresh_token_reused",
            "refresh_token_invalidated",
        ] {
            assert!(refused_for_good(
                400,
                &format!(r#"{{"error":{{"code":"{code}"}}}}"#)
            ));
            assert!(refused_for_good(400, &format!(r#"{{"code":"{code}"}}"#)));
        }
        assert!(!refused_for_good(400, r#"{"error":"invalid_request"}"#));
        assert!(!refused_for_good(400, "not json"));
        assert!(!refused_for_good(403, r#"{"error":"invalid_grant"}"#));
    }

    #[test]
    fn the_windows_keep_the_vocabulary_the_rest_of_pitboard_uses() {
        assert_eq!(kind(Some(18_000), "primary_window"), "five_hour");
        assert_eq!(kind(Some(604_800), "secondary_window"), "seven_day");
        assert_eq!(kind(Some(86_400), "primary_window"), "1_day");
        assert_eq!(kind(Some(10_800), "primary_window"), "3_hour");
        assert_eq!(kind(Some(5_400), "primary_window"), "90_minute");
        assert_eq!(
            kind(None, "primary_window"),
            "primary",
            "a window with no length is named by which one it is, not by a guess"
        );
    }

    #[test]
    fn a_reset_time_reads_as_seconds_or_as_a_timestamp() {
        assert_eq!(
            moment(&serde_json::json!(1_789_935_600)),
            Some(1_789_935_600)
        );
        assert_eq!(
            moment(&serde_json::json!("2026-09-21T10:30:00Z")),
            Some(1_789_986_600)
        );
        for bad in [
            serde_json::json!(null),
            serde_json::json!(0),
            serde_json::json!(-5),
            serde_json::json!(1_789_935_600_000i64),
            serde_json::json!("not a time"),
        ] {
            assert_eq!(moment(&bad), None, "{bad}");
        }
    }

    /// The answer this endpoint really sends, copied from a live response, with the
    /// account's own identifiers replaced. The first parser was written from a description
    /// of this and had the windows, the length field and the reset field all in the wrong
    /// place; it returned no windows while the request succeeded, which is the quietest
    /// possible way to be wrong.
    fn measured() -> Value {
        serde_json::json!({
            "user_id": "user-x",
            "account_id": "acc-1",
            "email": "a@b.c",
            "plan_type": "pro",
            "rate_limit": {
                "allowed": true,
                "limit_reached": false,
                "primary_window": {
                    "used_percent": 45,
                    "limit_window_seconds": 604_800,
                    "reset_after_seconds": 492_401,
                    "reset_at": 1_790_628_078i64
                },
                "secondary_window": null
            },
            "credits": {"has_credits": false, "unlimited": false, "balance": "0"},
            "rate_limit_reached_type": null
        })
    }

    #[test]
    fn the_answer_this_endpoint_really_sends_becomes_a_window() {
        let snapshot = snapshot(&measured(), "acc-1", 1_790_000_000);
        assert_eq!(
            snapshot.windows.len(),
            1,
            "one window is filled in, one is null"
        );
        let window = &snapshot.windows[0];
        assert_eq!(window.kind, "seven_day");
        assert!((window.percent - 45.0).abs() < f64::EPSILON);
        assert_eq!(window.resets_at, Some(1_790_628_078));
        assert_eq!(snapshot.account_uuid.as_deref(), Some("acc-1"));
        assert_eq!(snapshot.source, Source::Live);
    }

    #[test]
    fn both_windows_are_read_when_both_are_filled_in() {
        let mut body = measured();
        body["rate_limit"]["secondary_window"] = serde_json::json!({
            "used_percent": 12.5,
            "limit_window_seconds": 18_000,
            "reset_at": 1_790_100_000i64
        });
        let snapshot = snapshot(&body, "acc-1", 0);
        assert_eq!(
            snapshot
                .windows
                .iter()
                .map(|w| w.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["seven_day", "five_hour"]
        );
    }

    /// A number nobody can explain is worse than no number, which is the rule the Claude
    /// Code side has always used.
    #[test]
    fn a_window_that_will_not_normalise_is_dropped_rather_than_drawn() {
        let body = serde_json::json!({
            "rate_limit": {
                "primary_window": {"used_percent": "quite a lot", "limit_window_seconds": 18_000},
                "secondary_window": {"used_percent": -1.0, "limit_window_seconds": 604_800},
            }
        });
        assert!(snapshot(&body, "acc-1", 0).windows.is_empty());
    }

    /// An answer with no rate limit block at all reads as no windows rather than panicking.
    #[test]
    fn an_answer_with_nothing_in_it_is_not_a_crash() {
        for body in [
            serde_json::json!({}),
            serde_json::json!({"rate_limit": null}),
        ] {
            assert!(snapshot(&body, "acc-1", 0).windows.is_empty(), "{body}");
        }
    }
}
