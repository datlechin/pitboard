//! Making a diagnosis safe to paste into a bug report.
//!
//! pitboard's bug template asks for `pitboard doctor --json` and tells people it prints
//! "labels, codes, paths and times: no tokens, no email addresses, no account identifiers".
//! It printed the signed-in email address and the organization uuid in the identity check,
//! the login name in the slot check, a value derived from the refresh token in the
//! credential check, and home and config paths carrying the username. The promise was
//! false, and the contract snapshot could not catch it because it redacted the whole checks
//! array.
//!
//! So `--json` is now what the template says it is, and the human-readable report is not
//! touched: a person looking at their own machine should see their own email, and the thing
//! they paste somewhere else should not carry it.
//!
//! Identifiers become salted digests rather than disappearing, so two mentions of one
//! account still line up inside a report without naming it. The salt is this machine and
//! this moment, so two reports from one machine do not correlate. That is the intent and it
//! is worth saying out loud rather than leaving to be discovered.

use serde_json::Value;

/// Everything a report must not carry out of a machine, and what to put in its place.
#[derive(Debug, Clone)]
pub struct Sheet {
    salt: String,
    /// Longest first, so a substring never hides the string that contains it.
    secrets: Vec<(String, &'static str)>,
    home: String,
}

impl Sheet {
    /// What to hide, and what to hide it under.
    ///
    /// `salt` makes the digests this report's own. Pass something that changes between
    /// reports and does not identify the machine on its own.
    pub fn new(salt: impl Into<String>, home: impl Into<String>) -> Sheet {
        Sheet {
            salt: salt.into(),
            secrets: Vec::new(),
            home: home.into(),
        }
    }

    /// Hide every occurrence of `secret`, under a digest prefixed by `kind`.
    pub fn hide(mut self, secret: impl Into<String>, kind: &'static str) -> Sheet {
        let secret = secret.into();
        // Too short to be worth hiding is also too short to hide safely: a two-character
        // login name would rewrite half the report.
        if secret.len() >= 4 {
            self.secrets.push((secret, kind));
            self.secrets
                .sort_by_key(|(secret, _)| std::cmp::Reverse(secret.len()));
        }
        self
    }

    fn digest(&self, kind: &str, secret: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.salt.as_bytes());
        hasher.update(b"\0");
        hasher.update(secret.as_bytes());
        format!("<{kind} {}>", hex::encode(&hasher.finalize()[..4]))
    }

    /// One string, with everything hidden that should be.
    ///
    /// The home comes first, because a login name is usually inside it: digesting the name
    /// before shortening the path would leave a path nobody can read and a digest where a
    /// `~` belongs.
    pub fn over(&self, text: &str) -> String {
        let mut out = text.to_string();
        // A path is worth keeping, and whose it is is not.
        if self.home.len() >= 4 {
            out = out.replace(&self.home, "~");
        }
        for (secret, kind) in &self.secrets {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), &self.digest(kind, secret));
            }
        }
        out
    }

    /// The same, over every string anywhere in a JSON value.
    pub fn over_json(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.over(text)),
            Value::Array(items) => Value::Array(items.iter().map(|v| self.over_json(v)).collect()),
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.over_json(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> Sheet {
        Sheet::new("this report", "/Users/someone")
            .hide("me@example.com", "email")
            .hide("7c6b5a49-3827-4165-a4b3-c2d1e0f9a8b7", "account")
            .hide("someone", "user")
    }

    #[test]
    fn an_identifier_becomes_the_same_digest_everywhere_in_one_report() {
        let s = sheet();
        let once = s.over("me@example.com");
        assert_ne!(once, "me@example.com");
        assert!(once.starts_with("<email "));
        assert_eq!(
            s.over("signed in as me@example.com, parked for me@example.com"),
            format!("signed in as {once}, parked for {once}"),
            "two mentions of one account still line up"
        );
    }

    #[test]
    fn two_reports_from_one_machine_do_not_line_up_with_each_other() {
        let first = Sheet::new("one moment", "/Users/someone").hide("me@example.com", "email");
        let later = Sheet::new("another", "/Users/someone").hide("me@example.com", "email");
        assert_ne!(first.over("me@example.com"), later.over("me@example.com"));
    }

    #[test]
    fn a_path_is_kept_and_the_name_in_it_is_not() {
        assert_eq!(
            sheet().over("/Users/someone/.claude.json"),
            "~/.claude.json",
            "where a file is matters; whose it is does not"
        );
    }

    /// A login name that is a substring of an email address must not rewrite half of it.
    #[test]
    fn the_longest_secret_is_hidden_first() {
        let s = Sheet::new("salt", "/nowhere")
            .hide("someone", "user")
            .hide("someone@example.com", "email");
        let hidden = s.over("someone@example.com");
        assert!(hidden.starts_with("<email "), "got {hidden}");
        assert!(!hidden.contains("@example.com"));
    }

    #[test]
    fn nothing_worth_keeping_is_lost() {
        let s = sheet();
        assert_eq!(s.over("the keychain is locked"), "the keychain is locked");
        assert_eq!(s.over("credential_store"), "credential_store");
    }

    #[test]
    fn it_reaches_every_string_in_a_report() {
        let hidden = sheet().over_json(&serde_json::json!({
            "environment": {"home": "/Users/someone/.pitboard"},
            "checks": [
                {"code": "identity", "detail": "me@example.com", "level": "ok"},
                {"code": "accounts", "detail": ["me@example.com", 3]},
            ],
        }));
        let printed = hidden.to_string();
        assert!(!printed.contains("me@example.com"), "{printed}");
        assert!(!printed.contains("someone"), "{printed}");
        assert!(
            printed.contains("identity"),
            "codes are the point of the report"
        );
        assert!(printed.contains("~/.pitboard"));
    }

    #[test]
    fn something_too_short_to_hide_safely_is_not_hidden() {
        let s = Sheet::new("salt", "/nowhere").hide("ab", "user");
        assert_eq!(
            s.over("a table of absolutes"),
            "a table of absolutes",
            "a two-character name would rewrite half the report"
        );
    }
}
