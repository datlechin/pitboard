//! Reading the claims out of a JSON Web Token, without checking who signed it.
//!
//! Two of the three tools put a signed token in the login they store, and its claims name
//! the account. Reading them is how pitboard can label a parked login with no network call
//! at all, which is better than it manages for Claude Code.
//!
//! The signature is deliberately not checked, and none of this may be used to decide
//! whether a token is genuine. What it reads are facts out of a token the person's own tool
//! put on their own disk, which is the same trust as reading any other file there.
//! Verifying would mean fetching and pinning somebody else's signing keys in order to learn
//! an email address that pitboard is about to send the same token to a server to confirm.
//!
//! The base64url decode is written out rather than taken as a dependency. It is twenty
//! lines, it is the only thing a crate would have been for, and a login document is exactly
//! the input not worth widening the dependency tree over.

use serde_json::Value;

/// The payload of `token`, or `None` when it is not three base64url parts with a JSON
/// object in the middle.
pub(crate) fn claims(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let (_header, payload, _signature) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let raw = base64url(payload)?;
    serde_json::from_slice(&raw).ok().filter(Value::is_object)
}

/// When `token` expires, from its `exp` claim, in epoch seconds.
pub(crate) fn expires_at(token: &str) -> Option<i64> {
    claims(token)?.get("exp")?.as_i64()
}

/// A claim of `token` that is a string, by path: `claims(&["https://x/auth", "plan"])`.
pub(crate) fn claim<'a>(claims: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut at = claims;
    for step in path {
        at = at.get(step)?;
    }
    at.as_str().filter(|found| !found.is_empty())
}

/// base64url without padding, as RFC 7515 requires of every JWT part.
fn base64url(text: &str) -> Option<Vec<u8>> {
    fn sextet(byte: u8) -> Option<u32> {
        Some(match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'-' => 62,
            b'_' => 63,
            // Standard base64's own two characters are refused rather than accepted as a
            // kindness: a JWT part containing one is not a JWT part, and reading it anyway
            // would decode something nobody wrote.
            _ => return None,
        })
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut held: u32 = 0;
    let mut bits = 0;
    for byte in text.bytes() {
        held = (held << 6) | sextet(byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((held >> bits) & 0xff).expect("masked to one byte"));
        }
    }
    // Whatever is left is the padding the encoding dropped, and must be zero. Anything else
    // is a truncated part rather than an unpadded one.
    ((held & ((1u32 << bits) - 1)) == 0).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let mut held = 0u32;
            for (at, byte) in chunk.iter().enumerate() {
                held |= u32::from(*byte) << (16 - 8 * at);
            }
            let sextets = chunk.len().saturating_mul(8).div_ceil(6);
            for at in 0..sextets {
                let index = (held >> (18 - 6 * at)) & 0x3f;
                out.push(char::from(ALPHABET[index as usize]));
            }
        }
        out
    }

    fn token(payload: &Value) -> String {
        format!(
            "{}.{}.{}",
            part(br#"{"alg":"RS256"}"#),
            part(payload.to_string().as_bytes()),
            part(b"not a real signature")
        )
    }

    #[test]
    fn the_claims_come_back_whole() {
        let made = serde_json::json!({"email": "a@b.c", "exp": 1_789_935_600});
        assert_eq!(claims(&token(&made)).unwrap(), made);
        assert_eq!(expires_at(&token(&made)), Some(1_789_935_600));
    }

    /// The claims that matter are nested under a namespaced key, which is how both OpenAI
    /// and Google put their own facts in a standard token.
    #[test]
    fn a_nested_claim_is_reachable_by_path() {
        let made = serde_json::json!({
            "email": "a@b.c",
            "https://api.openai.com/auth": {"chatgpt_plan_type": "pro", "empty": ""}
        });
        let found = claims(&token(&made)).unwrap();
        assert_eq!(claim(&found, &["email"]), Some("a@b.c"));
        assert_eq!(
            claim(
                &found,
                &["https://api.openai.com/auth", "chatgpt_plan_type"]
            ),
            Some("pro")
        );
        assert_eq!(claim(&found, &["nothing", "here"]), None);
        assert_eq!(
            claim(&found, &["https://api.openai.com/auth", "empty"]),
            None,
            "an empty claim is not an answer"
        );
    }

    /// A login on somebody's disk holds whatever their tool last wrote there, so anything
    /// that is not a token has to come back as nothing rather than as a panic.
    #[test]
    fn anything_that_is_not_a_token_is_not_read() {
        let bad_parts = token(&serde_json::json!("a string, not an object"));
        for bad in [
            "",
            "one.part",
            "four.parts.are.wrong",
            "aaa.not+base64.ccc",
            "aaa.not/base64.ccc",
            bad_parts.as_str(),
        ] {
            assert!(claims(bad).is_none(), "{bad} was read as claims");
            assert_eq!(expires_at(bad), None, "{bad}");
        }
    }

    /// Against the real thing, on a machine with a signed-in Codex.
    ///
    /// Ignored by default because it needs one, and skipped rather than failed where there
    /// is none: a test that depends on somebody being signed in to something is not a test
    /// most people running `cargo test` should see fail. Run it with
    /// `cargo test -p pitboard-core -- --ignored jwt` on a machine that has one.
    ///
    /// It prints and asserts nothing secret: claim names and whether the token parsed.
    #[test]
    #[ignore = "needs a signed-in Codex on this machine"]
    fn a_real_codex_id_token_reads() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let path = std::path::Path::new(&home).join(".codex/auth.json");
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return;
        };
        let document: Value = serde_json::from_str(&raw).expect("auth.json is JSON");
        let token = document["tokens"]["id_token"]
            .as_str()
            .expect("a signed-in Codex has an id token");
        let found = claims(token).expect("the id token reads");
        let mut names: Vec<&str> = found
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        names.sort_unstable();
        assert!(names.contains(&"email"), "claims: {names:?}");
        assert!(
            names.contains(&"https://api.openai.com/auth"),
            "claims: {names:?}"
        );
        assert!(
            claim(
                &found,
                &["https://api.openai.com/auth", "chatgpt_account_id"]
            )
            .is_some(),
            "the account id claim is where pitboard reads it"
        );
        assert!(
            expires_at(token).is_some(),
            "an id token says when it expires"
        );
    }

    /// Every byte pattern round trips, including the two lengths that need padding.
    #[test]
    fn the_decode_is_the_inverse_of_the_encode() {
        for len in 0..40usize {
            let bytes: Vec<u8> = (0..len)
                .map(|i| u8::try_from(i * 7 % 256).unwrap())
                .collect();
            assert_eq!(base64url(&part(&bytes)), Some(bytes.clone()), "len {len}");
        }
    }
}
