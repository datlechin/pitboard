//! What every request Pitboard makes shares, to Anthropic or to OpenAI: the agent each
//! context's requests go out on, the loopback base a test redirects them to, and the headers
//! read off every answer.

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(feature = "test-support"), allow(unreachable_pub))]
pub(crate) mod scripted;

use crate::context::Context;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// Where requests go instead, for tests. Nothing else may redirect them, because an address
/// that answers "this token belongs to account X" decides which account a credential is filed
/// under. Only loopback is accepted, so a token or an answer never leaves this machine.
pub(crate) fn test_base(ctx: &Context) -> Option<String> {
    ctx.api_base.clone().filter(|url| is_loopback(url))
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

/// The agent a context's requests go out on, made the first time one of them needs it and
/// shared by every copy of the context, so they share its connections too.
#[derive(Clone, Default)]
pub(crate) struct Agents(Arc<OnceLock<Agent>>);

/// Whether it has been made, and nothing of how: ureq's own form shows the first letter of a
/// proxy's user name.
impl std::fmt::Debug for Agents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agents")
            .field("made", &self.0.get().is_some())
            .finish()
    }
}

/// The agent `ctx`'s requests go out on: through the HTTP proxy its environment names, and
/// through no other ([`crate::proxy`]). ureq finds one of its own in this process's
/// environment as the configuration is made; giving it the context's, or none, replaces that
/// before the agent exists, so what ureq found never reaches a request. Where the environment
/// names a SOCKS proxy, which Pitboard does not use yet, ureq is given none, and the agent
/// refuses each request that proxy would have carried before anything is sent
/// ([`crate::proxy::Refusal`]).
pub(crate) fn agent(ctx: &Context) -> &Agent {
    ctx.agent
        .0
        .get_or_init(|| made(ctx, ureq::unversioned::resolver::DefaultResolver::default()))
}

/// [`agent`]'s agent, looking hosts up with `resolver`, which is ureq's own but in a test,
/// and connecting through ureq's own default chain of connectors.
///
/// A SOCKS proxy's refusal is the agent's middleware, not a check made by each function that
/// sends a request: every request on the agent passes it, whichever code sends it, and
/// `clippy.toml` refuses every other way to make an agent, so no request can go out directly
/// past the proxy the person named. ureq follows a redirect inside the request the
/// middleware handed on, so with a SOCKS proxy named the agent follows none: one from a host
/// `NO_PROXY` names could take the request to a host it does not name, directly.
///
/// ureq 3.4.2 also lets a single request change the agent's configuration, with
/// `RequestBuilder::config`, `Agent::configure_request`, `WithAgent::configure`, or
/// `RequestExt::middleware_config` inside a middleware. A request given its own
/// `max_redirects` follows that redirect after all, and one given its own proxy goes another
/// way, so `clippy.toml` refuses each of those too: every request goes out with the
/// configuration made here.
#[allow(
    clippy::disallowed_methods,
    reason = "the one agent configuration Pitboard makes, which is given the context's proxy, \
              and the one agent, given the resolver a test can replace"
)]
pub(crate) fn made(ctx: &Context, resolver: impl ureq::unversioned::resolver::Resolver) -> Agent {
    // rustls's documented way to choose a crypto provider. Returns Err only when one is
    // already installed, which is exactly the state wanted.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut config = Agent::config_builder()
        .proxy(ctx.proxy.for_ureq())
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::Rustls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .user_agent(format!("pitboard/{}", crate::release::pitboard_version()));
    if let Some(refusal) = ctx.proxy.refusal() {
        config = config.middleware(refusal).max_redirects(0);
    }
    Agent::with_parts(
        config.build(),
        ureq::unversioned::transport::DefaultConnector::default(),
        resolver,
    )
}

/// How long a service asked us to wait, from `Retry-After`. Only the seconds form is read:
/// the date form is allowed by the standard and has not been seen from this endpoint, and
/// misreading one would be worse than not reading it.
pub(crate) fn retry_after(headers: &ureq::http::HeaderMap) -> Option<i64> {
    headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|seconds| *seconds > 0)
}

// The instant a response says it was sent, with no skew estimated (docs/facts/services.md).
pub(crate) fn server_time(headers: &ureq::http::HeaderMap) -> Option<i64> {
    let raw = headers.get("date")?.to_str().ok()?;
    // RFC 9110's preferred form, which is what every answer measured used:
    // `Mon, 22 Sep 2026 12:34:56 GMT`.
    jiff::civil::DateTime::strptime("%a, %d %b %Y %H:%M:%S GMT", raw)
        .ok()?
        .to_zoned(jiff::tz::TimeZone::UTC)
        .ok()
        .map(|z| z.timestamp().as_second())
}

/// What a service answered in place of what was asked, as much of it as says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) status: u16,
    /// `error.type`, or an OAuth answer's `error`.
    pub(crate) kind: Option<String>,
    /// `error.details.error_code`, OpenAI's `error.code`, or a top-level `code`.
    pub(crate) code: Option<String>,
    /// `error.message`, or an OAuth answer's `error_description`.
    pub(crate) message: Option<String>,
    /// An OAuth answer's `error_uri`, unchecked.
    pub(crate) link: Option<String>,
    pub(crate) request_id: Option<String>,
}

// Enough for any error envelope; a page of HTML from a proxy is cut off, not kept.
const REFUSAL_BODY_LIMIT: u64 = 64 * 1024;
const REASON_LIMIT: usize = 240;

impl Refusal {
    /// Read off a response whose status is not the one asked for.
    pub(crate) fn of(response: &mut ureq::http::Response<ureq::Body>) -> Refusal {
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(REFUSAL_BODY_LIMIT)
            .read_to_string()
            .unwrap_or_default();
        Refusal::answered(status, response.headers(), &body)
    }

    /// [`Refusal::of`], for a body already read.
    pub(crate) fn answered(status: u16, headers: &ureq::http::HeaderMap, body: &str) -> Refusal {
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(clean)
        };
        let mut refusal = Refusal::read(status, body);
        refusal.request_id = header("request-id")
            .or_else(|| header("x-request-id"))
            .or(refusal.request_id);
        refusal
    }

    /// `body` read as Anthropic's API error envelope, an OAuth error answer or OpenAI's.
    pub(crate) fn read(status: u16, body: &str) -> Refusal {
        let value: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
        let text = |pointer: &str| {
            value
                .pointer(pointer)
                .and_then(serde_json::Value::as_str)
                .map(clean)
                .filter(|text| !text.is_empty())
        };
        let oauth = value.get("error").is_some_and(serde_json::Value::is_string);
        Refusal {
            status,
            kind: if oauth {
                text("/error")
            } else {
                text("/error/type")
            },
            code: text("/error/details/error_code")
                .or_else(|| text("/error/code"))
                .or_else(|| text("/code")),
            message: text("/error/message").or_else(|| text("/error_description")),
            link: text("/error_uri"),
            request_id: text("/request_id"),
        }
    }

    // Asking again is answered the same way, as Claude Code decides (`refusals_are_final`).
    pub(crate) fn is_final(&self) -> bool {
        (400..500).contains(&self.status) && !matches!(self.status, 408 | 409 | 429)
    }

    /// The `error` or `error.type` an OAuth answer gives.
    pub(crate) fn oauth_error(&self) -> Option<&str> {
        self.kind.as_deref()
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.status)?;
        if let Some(reason) = self
            .message
            .as_ref()
            .or(self.code.as_ref())
            .or(self.kind.as_ref())
        {
            // Said inside a sentence of Pitboard's, so its own full stop goes.
            write!(f, ": {}", reason.strip_suffix('.').unwrap_or(reason))?;
        }
        if let Some(id) = &self.request_id {
            write!(f, "; request {id}")?;
        }
        Ok(())
    }
}

// Control and format characters out, one line, bounded, as Claude Code cleans a message it shows.
fn clean(text: &str) -> String {
    let words: Vec<&str> = text
        .split(|c: char| c.is_control() || c.is_whitespace() || is_format(c))
        .filter(|word| !word.is_empty())
        .collect();
    let line = words.join(" ");
    if line.chars().count() <= REASON_LIMIT {
        return line;
    }
    let cut: String = line.chars().take(REASON_LIMIT - 1).collect();
    format!("{}…", cut.trim_end())
}

fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header every answer measured carried, in RFC 9110's preferred form.
    #[test]
    fn the_instant_an_answer_says_it_was_sent_is_read_off_it() {
        let mut headers = ureq::http::HeaderMap::new();
        headers.insert(
            "date",
            "Tue, 22 Sep 2026 12:34:56 GMT".parse().expect("valid"),
        );
        assert_eq!(server_time(&headers), Some(1_790_080_496));

        headers.insert("date", "whenever".parse().expect("valid"));
        assert_eq!(
            server_time(&headers),
            None,
            "an answer Pitboard cannot read the time off is not a reason to guess one"
        );
        assert_eq!(server_time(&ureq::http::HeaderMap::new()), None);
    }

    // The envelope as measured on 2026-10-16: a 405 from /api/oauth/profile.
    #[test]
    fn an_anthropic_refusal_keeps_its_reason_and_request() {
        let refusal = Refusal::read(
            405,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"Method Not Allowed"},"request_id":"req_011Cg5hXyTc3FaNivgfRjtAU"}"#,
        );
        assert_eq!(refusal.kind.as_deref(), Some("invalid_request_error"));
        assert_eq!(refusal.message.as_deref(), Some("Method Not Allowed"));
        assert_eq!(
            refusal.to_string(),
            "405: Method Not Allowed; request req_011Cg5hXyTc3FaNivgfRjtAU"
        );
        assert!(refusal.is_final());
    }

    #[test]
    fn a_permission_error_keeps_its_code() {
        let refusal = Refusal::read(
            403,
            r#"{"type":"error","error":{"type":"permission_error","message":"Verify your account at https://claude.ai/verify","details":{"error_code":"verification_required"}}}"#,
        );
        assert_eq!(refusal.code.as_deref(), Some("verification_required"));
        assert_eq!(
            refusal.to_string(),
            "403: Verify your account at https://claude.ai/verify"
        );
    }

    #[test]
    fn an_oauth_refusal_is_read_as_one() {
        let refusal = Refusal::read(
            400,
            r#"{"error":"invalid_request","error_description":"No organization associated with this token"}"#,
        );
        assert_eq!(refusal.oauth_error(), Some("invalid_request"));
        assert_eq!(
            refusal.to_string(),
            "400: No organization associated with this token"
        );
        let on_hold = Refusal::read(
            400,
            r#"{"error":"invalid_grant","error_description":"account_on_hold","error_uri":"https://claude.ai/hold"}"#,
        );
        assert_eq!(on_hold.link.as_deref(), Some("https://claude.ai/hold"));
    }

    #[test]
    fn an_openai_refusal_keeps_its_code() {
        let refusal = Refusal::read(
            400,
            r#"{"error":{"message":"Could not validate your refresh token.","type":"invalid_request_error","code":"refresh_token_expired"}}"#,
        );
        assert_eq!(refusal.code.as_deref(), Some("refresh_token_expired"));
        assert_eq!(
            refusal.to_string(),
            "400: Could not validate your refresh token"
        );
    }

    #[test]
    fn an_answer_that_is_not_an_envelope_is_its_status() {
        assert_eq!(
            Refusal::read(502, "<html>Bad Gateway</html>").to_string(),
            "502"
        );
        assert!(!Refusal::read(502, "").is_final());
        for retried in [408, 409, 429, 500] {
            assert!(!Refusal::read(retried, "").is_final(), "{retried}");
        }
    }

    #[test]
    fn a_reason_is_one_bounded_line() {
        let long = "x".repeat(1000);
        let refusal = Refusal::read(
            400,
            &serde_json::json!({"error": {"type": "e", "message": format!("one\nline\u{202E}{long}")}})
                .to_string(),
        );
        let message = refusal.message.expect("a message");
        assert!(message.starts_with("one line "), "{message}");
        assert_eq!(message.chars().count(), REASON_LIMIT);
        assert!(message.ends_with('…'));
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
}
