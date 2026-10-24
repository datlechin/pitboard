//! What Pitboard asks OpenAI about a Codex login.
//!
//! Read from codex-cli 0.154.0. Two requests: one to exchange a refresh token, one to ask
//! how much of a plan is left. Neither is a model request and neither costs quota.

use crate::api::{Refusal, agent, retry_after, server_time, test_base};
use crate::context::Context;
use crate::permit::Permit;
use crate::provider::{ProviderError, ProviderId};
use crate::usage::{Snapshot, Source, Window};
use serde_json::Value;

/// The client Codex renews as. Its own, not a first-party one: a login issued to this
/// client can only be renewed as it.
pub(crate) const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

const ISSUER: &str = "https://auth.openai.com";
pub(crate) const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub(crate) const USAGE_BASE: &str = "https://chatgpt.com/backend-api";

// Where a login is renewed and as which client (`codex_renewal`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Authority {
    issuer: String,
    pub(crate) client_id: String,
}

impl Authority {
    // What `tokens.oauth_provenance` records, else Codex's own. Only over https, since the
    // refresh token goes there.
    pub(crate) fn of(tokens: &Value) -> Result<Authority, String> {
        let Some(recorded) = tokens.get("oauth_provenance").filter(|v| !v.is_null()) else {
            return Ok(Authority {
                issuer: ISSUER.into(),
                client_id: CLIENT_ID.into(),
            });
        };
        match (recorded["issuer"].as_str(), recorded["client_id"].as_str()) {
            (Some(issuer), Some(client_id))
                if issuer.starts_with("https://") && !client_id.is_empty() =>
            {
                Ok(Authority {
                    issuer: issuer.into(),
                    client_id: client_id.into(),
                })
            }
            _ => Err("its tokens.oauth_provenance names no https issuer and client".into()),
        }
    }

    pub(crate) fn token_url(&self) -> String {
        format!("{}/oauth/token", self.issuer.trim_end_matches('/'))
    }
}

fn token_url(ctx: &Context, authority: &Authority) -> String {
    test_base(ctx).map_or_else(
        || authority.token_url(),
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

/// Fresh tokens for a refresh chain.
///
/// The body is JSON rather than form-encoded, which is what Codex sends and is not what its
/// own authorisation-code exchange sends. Getting that wrong is a 400 with nothing useful
/// in it.
#[cfg_attr(not(feature = "test-support"), allow(unreachable_pub))]
#[derive(Debug, Clone)]
pub struct Fresh {
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    /// The server's own clock, from the `Date` header, in epoch seconds.
    pub at: Option<i64>,
}

/// What Pitboard asks OpenAI, as a seam, for the same reason Anthropic's is one: a loopback
/// server proves the parsing, and cannot produce on demand the timeouts, 429s and refused
/// refresh tokens the engine has to be right about.
pub(crate) trait OpenAi: Send + Sync + std::fmt::Debug {
    fn usage(
        &self,
        ctx: &Context,
        access_token: &str,
        login: Login<'_>,
        now: i64,
    ) -> Result<Snapshot, ProviderError>;

    // Spends `refresh_token` as it answers, so it takes the Permit only the change gate makes.
    fn renew(
        &self,
        ctx: &Context,
        permit: Permit,
        refresh_token: &str,
        authority: &Authority,
    ) -> Result<Fresh, ProviderError>;
}

/// OpenAI, over the network. What every real context uses.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Network;

impl OpenAi for Network {
    fn usage(
        &self,
        ctx: &Context,
        access_token: &str,
        login: Login<'_>,
        now: i64,
    ) -> Result<Snapshot, ProviderError> {
        ask_usage(ctx, access_token, login, now)
    }

    fn renew(
        &self,
        ctx: &Context,
        permit: Permit,
        refresh_token: &str,
        authority: &Authority,
    ) -> Result<Fresh, ProviderError> {
        ask_renew(ctx, permit, refresh_token, authority)
    }
}

/// Ask OpenAI through this context.
pub(crate) fn renew(
    ctx: &Context,
    permit: Permit,
    refresh_token: &str,
    authority: &Authority,
) -> Result<Fresh, ProviderError> {
    ctx.openai().renew(ctx, permit, refresh_token, authority)
}

// The login a usage answer is asked for: its `tokens.account_id`, which the request names, and
// its ID token's person.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Login<'a> {
    pub(crate) account_id: &'a str,
    pub(crate) user_id: Option<&'a str>,
}

pub(crate) fn usage(
    ctx: &Context,
    access_token: &str,
    login: Login<'_>,
    now: i64,
) -> Result<Snapshot, ProviderError> {
    ctx.openai().usage(ctx, access_token, login, now)
}

fn ask_renew(
    ctx: &Context,
    _: Permit,
    refresh_token: &str,
    authority: &Authority,
) -> Result<Fresh, ProviderError> {
    let body = serde_json::json!({
        "client_id": authority.client_id,
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
    });
    let mut response = agent(ctx)
        .post(token_url(ctx, authority))
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
        other => Err(ProviderError::answered(
            ProviderId::Codex.service(),
            Refusal::answered(other, response.headers(), &text),
        )),
    }
}

// Read as Codex reads it (`codex_refusal_is_invalid_grant`): a 401, `invalid_grant` with a
// 400, or one of the older codes. A refusal of the client is never one: it says nothing of
// the login, and the park may be its only copy.
fn refused_for_good(status: u16, body: &str) -> bool {
    let body = serde_json::from_str::<Value>(body).ok();
    let code = body.as_ref().and_then(|body| {
        body.get("error")
            .and_then(Value::as_str)
            .or_else(|| body.pointer("/error/code").and_then(Value::as_str))
            .or_else(|| body.get("code").and_then(Value::as_str))
    });
    if matches!(code, Some("invalid_client" | "unauthorized_client")) {
        return false;
    }
    if status == 401 {
        return true;
    }
    match code {
        Some("refresh_token_expired" | "refresh_token_reused" | "refresh_token_invalidated") => {
            true
        }
        Some("invalid_grant") => status == 400,
        _ => false,
    }
}

// A GET that spends no quota, which needs `ChatGPT-Account-ID` (`codex_usage_endpoint`).
fn ask_usage(
    ctx: &Context,
    access_token: &str,
    login: Login<'_>,
    now: i64,
) -> Result<Snapshot, ProviderError> {
    let mut response = agent(ctx)
        .get(usage_url(ctx))
        .header("Authorization", format!("Bearer {access_token}"))
        .header("ChatGPT-Account-ID", login.account_id)
        .call()
        .map_err(|e| network(e.to_string()))?;
    match response.status().as_u16() {
        200 => {
            let text = response
                .body_mut()
                .read_to_string()
                .map_err(|e| network(e.to_string()))?;
            let body: Value = serde_json::from_str(&text).map_err(|e| malformed(e.to_string()))?;
            answered_for(&body, login.account_id, login.user_id)?;
            Ok(snapshot(&body, now))
        }
        401 | 403 => Err(ProviderError::Unauthorized),
        429 => Err(ProviderError::RateLimited {
            service: ProviderId::Codex.service(),
            retry_after: retry_after(response.headers()),
        }),
        _ => Err(ProviderError::answered(
            ProviderId::Codex.service(),
            Refusal::of(&mut response),
        )),
    }
}

// An answer that names another account or person than the login's gives no numbers of it
// (`codex_model_limits`). One that names neither is taken as the login's, as an older one is.
fn answered_for(
    body: &Value,
    account_id: &str,
    user_id: Option<&str>,
) -> Result<(), ProviderError> {
    let said = |key: &str| body.get(key).and_then(Value::as_str);
    let another = said("account_id").is_some_and(|named| named != account_id)
        || said("user_id")
            .zip(user_id)
            .is_some_and(|(named, asked)| named != asked);
    if another {
        return Err(malformed(
            "it is for another account than the login asked about".into(),
        ));
    }
    Ok(())
}

// The shape measured from a live answer (`codex_usage_endpoint`), and a model's own limits
// scoped to its `limit_name` (`codex_model_limits`). A window that will not normalise is
// dropped, and as either can be null, no answer is taken to list every limit.
fn snapshot(body: &Value, now: i64) -> Snapshot {
    let windows_of = |limits: &Value, scope: Option<&str>| -> Vec<Window> {
        ["primary_window", "secondary_window"]
            .into_iter()
            .filter_map(|which| window(limits.get(which)?, which, scope))
            .collect()
    };
    let models = body["additional_rate_limits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|limit| {
            let name = limit["limit_name"]
                .as_str()
                .filter(|name| !name.is_empty())?;
            Some(windows_of(&limit["rate_limit"], Some(name)))
        })
        .flatten();
    let windows = windows_of(&body["rate_limit"], None)
        .into_iter()
        .chain(models)
        .collect();
    Snapshot {
        windows,
        observed_at: Some(now),
        answered_at: Some(now),
        lists_every_limit: false,
        source: Source::Live,
    }
}

fn window(value: &Value, which: &str, scope: Option<&str>) -> Option<Window> {
    let percent = value.get("used_percent").and_then(Value::as_f64)?;
    if !percent.is_finite() || percent < 0.0 {
        return None;
    }
    let seconds = value.get("limit_window_seconds").and_then(Value::as_i64);
    Some(Window {
        kind: kind(seconds, which),
        scope: scope.map(str::to_owned),
        percent,
        resets_at: value.get("reset_at").and_then(moment),
        // OpenAI says which window a request is being judged against only by filling one
        // in, so every window it sends is one that counts.
        is_active: true,
        severity: None,
        length_seconds: seconds.filter(|s| *s > 0),
    })
}

/// The same vocabulary the rest of Pitboard already uses where the lengths match, so a
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
        .map(jiff::Timestamp::as_second)
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
        assert!(
            !refused_for_good(401, r#"{"error":"invalid_client"}"#),
            "a refusal of the client says nothing of the login"
        );
        assert!(!refused_for_good(400, r#"{"error":"unauthorized_client"}"#));
        assert!(!refused_for_good(400, "not json"));
        assert!(!refused_for_good(403, r#"{"error":"invalid_grant"}"#));
    }

    // Codex 0.164.0's refresh_token_endpoint and the client id beside it.
    #[test]
    fn a_login_is_renewed_where_its_provenance_says_it_was_issued() {
        let codex = Authority::of(&serde_json::json!({"refresh_token": "r"})).expect("Codex's");
        assert_eq!(codex.token_url(), TOKEN_URL);
        assert_eq!(codex.client_id, CLIENT_ID);
        let recorded = Authority::of(&serde_json::json!({"oauth_provenance": {
            "issuer": "https://auth.example.com/", "client_id": "app_other"
        }}))
        .expect("recorded");
        assert_eq!(recorded.client_id, "app_other");
        assert_eq!(
            token_url(
                &Context::new(std::path::PathBuf::from("/nowhere")),
                &recorded
            ),
            "https://auth.example.com/oauth/token"
        );
        for refused in [
            serde_json::json!({"issuer": "http://auth.example.com", "client_id": "app_other"}),
            serde_json::json!({"issuer": "https://auth.example.com"}),
            serde_json::json!("https://auth.example.com"),
        ] {
            assert!(
                Authority::of(&serde_json::json!({"oauth_provenance": refused})).is_err(),
                "{refused}"
            );
        }
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
        let snapshot = snapshot(&measured(), 1_790_000_000);
        assert_eq!(
            snapshot.windows.len(),
            1,
            "one window is filled in, one is null"
        );
        let window = &snapshot.windows[0];
        assert_eq!(window.kind, "seven_day");
        assert!((window.percent - 45.0).abs() < f64::EPSILON);
        assert_eq!(window.resets_at, Some(1_790_628_078));
        assert_eq!(snapshot.source, Source::Live);
        assert_eq!(snapshot.answered_at, Some(1_790_000_000));
        assert!(!snapshot.lists_every_limit);
    }

    #[test]
    fn both_windows_are_read_when_both_are_filled_in() {
        let mut body = measured();
        body["rate_limit"]["secondary_window"] = serde_json::json!({
            "used_percent": 12.5,
            "limit_window_seconds": 18_000,
            "reset_at": 1_790_100_000i64
        });
        let snapshot = snapshot(&body, 0);
        assert_eq!(
            snapshot
                .windows
                .iter()
                .map(|w| w.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["seven_day", "five_hour"]
        );
    }

    // `additional_rate_limits` at 0.164.0: a model's own `rate_limit`, which Codex shows under
    // its `limit_name`.
    #[test]
    fn a_models_own_limits_are_read_under_its_name() {
        let mut body = measured();
        body["additional_rate_limits"] = serde_json::json!([
            {
                "limit_name": "codex_other",
                "metered_feature": "codex_other",
                "rate_limit": {
                    "allowed": true,
                    "limit_reached": false,
                    "primary_window": {
                        "used_percent": 12, "limit_window_seconds": 18_000,
                        "reset_at": 1_790_100_000i64
                    },
                    "secondary_window": {
                        "used_percent": 30.5, "limit_window_seconds": 604_800,
                        "reset_at": 1_790_600_000i64
                    }
                }
            },
            {"limit_name": "nothing_measured", "metered_feature": "x", "rate_limit": null}
        ]);
        let read: Vec<(String, Option<String>, f64)> = snapshot(&body, 0)
            .windows
            .into_iter()
            .map(|w| (w.kind, w.scope, w.percent))
            .collect();
        assert_eq!(
            read,
            [
                ("seven_day".to_string(), None, 45.0),
                (
                    "five_hour".to_string(),
                    Some("codex_other".to_string()),
                    12.0
                ),
                (
                    "seven_day".to_string(),
                    Some("codex_other".to_string()),
                    30.5
                ),
            ]
        );
    }

    // Codex 0.164.0's app server shows what is bound to an account only where the answer's
    // `account_id` and `user_id` are the login's.
    #[test]
    fn an_answer_for_another_account_gives_no_numbers() {
        let body = measured();
        assert!(answered_for(&body, "acc-1", Some("user-x")).is_ok());
        assert!(answered_for(&body, "acc-1", None).is_ok());
        assert!(answered_for(&body, "acc-2", Some("user-x")).is_err());
        assert!(answered_for(&body, "acc-1", Some("user-y")).is_err());
        assert!(
            answered_for(
                &serde_json::json!({"rate_limit": null}),
                "acc-2",
                Some("user-y")
            )
            .is_ok(),
            "an answer that names nobody is the login's, as an older one is"
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
        assert!(snapshot(&body, 0).windows.is_empty());
    }

    /// An answer with no rate limit block at all reads as no windows rather than panicking.
    #[test]
    fn an_answer_with_nothing_in_it_is_not_a_crash() {
        for body in [
            serde_json::json!({}),
            serde_json::json!({"rate_limit": null}),
        ] {
            assert!(snapshot(&body, 0).windows.is_empty(), "{body}");
        }
    }
}
