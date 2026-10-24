//! Making a diagnosis safe to paste into a bug report.
//!
//! Pitboard's bug template asks for `pitboard doctor --json` and tells people it prints
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

use crate::context::Context;
use crate::host::Os;
use serde_json::Value;

/// Everything a report must not carry out of a machine, and what to put in its place.
#[derive(Debug, Clone)]
pub struct Sheet {
    salt: String,
    /// Longest first, so a substring never hides the string that contains it.
    secrets: Vec<Hidden>,
    // Each spelling of a folder and what it is shown as, longest first.
    folders: Vec<Hidden>,
}

#[derive(Debug, Clone)]
struct Hidden {
    text: String,
    under: &'static str,
    any_case: bool,
}

impl Sheet {
    /// What to hide, and what to hide it under.
    ///
    /// `salt` makes the digests this report's own. Pass something that changes between
    /// reports and does not identify the machine on its own.
    pub(crate) fn new(salt: impl Into<String>, home: impl Into<String>) -> Sheet {
        let home = Hidden {
            text: home.into(),
            under: "~",
            any_case: false,
        };
        Sheet {
            salt: salt.into(),
            secrets: Vec::new(),
            folders: vec![home],
        }
    }

    // On Windows also the profile and `%LOCALAPPDATA%`, in every spelling and case, the user
    // and the SID.
    pub(crate) fn of(ctx: &Context, salt: impl Into<String>) -> Sheet {
        let sheet = Sheet::new(salt, ctx.home().to_string_lossy());
        match ctx.os {
            Os::MacOs | Os::Linux => sheet,
            Os::Windows => {
                let homes = ctx.profile.iter().chain([&ctx.home]);
                let sheet = homes.fold(sheet, |sheet, folder| {
                    sheet.folder(&windows_spellings(folder), "~")
                });
                let sheet = match &ctx.local_app_data {
                    Some(local) => sheet.folder(&windows_spellings(local), "%LOCALAPPDATA%"),
                    None => sheet,
                };
                sheet
                    .hide_in_any_case(ctx.user.clone().unwrap_or_default(), "user")
                    .hide_in_any_case(ctx.sid.clone().unwrap_or_default(), "sid")
            }
        }
    }

    // Shown as `under`, in any case, as Windows compares a path.
    pub(crate) fn folder(mut self, spellings: &[String], under: &'static str) -> Sheet {
        self.folders.extend(spellings.iter().map(|text| Hidden {
            text: text.clone(),
            under,
            any_case: true,
        }));
        self.folders
            .sort_by_key(|folder| std::cmp::Reverse(folder.text.chars().count()));
        self
    }

    /// Hide every occurrence of `secret`, under a digest prefixed by `kind`.
    pub(crate) fn hide(self, secret: impl Into<String>, kind: &'static str) -> Sheet {
        self.secret(secret.into(), kind, false)
    }

    // A Windows user's name, which Windows takes in any case.
    pub(crate) fn hide_in_any_case(self, secret: impl Into<String>, kind: &'static str) -> Sheet {
        self.secret(secret.into(), kind, true)
    }

    fn secret(mut self, secret: String, kind: &'static str, any_case: bool) -> Sheet {
        // Too short to be worth hiding is also too short to hide safely: a two-character
        // login name would rewrite half the report.
        if secret.len() >= 4 {
            self.secrets.push(Hidden {
                text: secret,
                under: kind,
                any_case,
            });
            self.secrets
                .sort_by_key(|secret| std::cmp::Reverse(secret.text.len()));
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
    pub(crate) fn over(&self, text: &str) -> String {
        let mut out = text.to_string();
        // A path is worth keeping, and whose it is is not.
        for folder in self.folders.iter().filter(|folder| folder.text.len() >= 4) {
            out = folder.replace(&out, folder.under);
        }
        for secret in &self.secrets {
            out = secret.replace(&out, &self.digest(secret.under, &secret.text));
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

impl Hidden {
    fn replace(&self, text: &str, with: &str) -> String {
        if !self.any_case {
            return text.replace(&self.text, with);
        }
        let upper = crate::host::upper_one_for_one;
        let wanted: Vec<char> = self.text.chars().map(upper).collect();
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        while at < chars.len() {
            let here = chars.get(at..at + wanted.len());
            if here.is_some_and(|here| here.iter().copied().map(upper).eq(wanted.iter().copied())) {
                out.push_str(with);
                at += wanted.len();
            } else {
                out.push(chars[at]);
                at += 1;
            }
        }
        out
    }
}

// Each way a folder can be written in a report: as given, by its long name and its short one
// (docs/facts/windows-runners.md), with `\\?\` and with forward slashes.
fn windows_spellings(folder: &std::path::Path) -> Vec<String> {
    let long = std::fs::canonicalize(folder)
        .ok()
        .map(crate::host::without_verbatim_prefix);
    let short = crate::host::fs::short_path(folder);
    let names: Vec<String> = [Some(folder.to_path_buf()), long, short]
        .into_iter()
        .flatten()
        .map(|named| named.to_string_lossy().into_owned())
        .collect();
    spellings(&names.iter().map(String::as_str).collect::<Vec<_>>())
}

fn spellings(names: &[&str]) -> Vec<String> {
    names
        .iter()
        .flat_map(|named| {
            [
                (*named).to_owned(),
                format!(r"\\?\{named}"),
                named.replace('\\', "/"),
            ]
        })
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
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

    // RF-B2, run 38693947329: a profile whose name is not ASCII has an ASCII short name.
    #[test]
    fn a_windows_folder_is_hidden_in_every_spelling_and_case() {
        let sheet = Sheet::new("salt", "")
            .folder(&spellings(&[r"C:\Users\Đạt Ng", r"C:\Users\DTNG~1"]), "~")
            .folder(&spellings(&[r"D:\Local"]), "%LOCALAPPDATA%")
            .hide_in_any_case("Đạt", "user");
        for (text, shown) in [
            (r"C:\Users\Đạt Ng\.claude", r"~\.claude"),
            (r"c:\users\đẠT NG\.claude.json", r"~\.claude.json"),
            (r"\\?\C:\Users\Đạt Ng\.codex", r"~\.codex"),
            ("C:/Users/Đạt Ng/.codex", "~/.codex"),
            (
                r"C:\Users\dtng~1\AppData\Local\Pitboard",
                r"~\AppData\Local\Pitboard",
            ),
            (r"\\?\d:\LOCAL\Pitboard", r"%LOCALAPPDATA%\Pitboard"),
        ] {
            assert_eq!(sheet.over(text), shown, "{text}");
        }
        let user = sheet.over("Đạt");
        assert!(user.starts_with("<user "), "{user}");
        assert_eq!(
            sheet.over("signed in as đẠT"),
            format!("signed in as {user}")
        );
    }

    #[test]
    fn a_report_from_windows_hides_its_profile_local_app_data_and_user() {
        use crate::context::Environment;
        let scratch = tempfile::Builder::new()
            .prefix("pitboard redact ")
            .tempdir()
            .expect("a scratch profile");
        let profile = scratch.path().to_path_buf();
        let local = profile.join("local");
        let named = |path: &std::path::Path| path.to_string_lossy().into_owned();
        let env = Environment::on(
            Os::Windows,
            [
                ("USERPROFILE", named(&profile)),
                ("LOCALAPPDATA", named(&local)),
                ("USERNAME", "Dana Test".into()),
            ],
        );
        let sheet = Sheet::of(&Context::read(&env, "cli"), "salt");
        let shown = |path: &std::path::Path| sheet.over(&named(&path.join("x")));
        assert_eq!(shown(&profile), named(&std::path::Path::new("~").join("x")));
        assert_eq!(
            shown(&local),
            named(&std::path::Path::new("%LOCALAPPDATA%").join("x"))
        );
        assert_eq!(sheet.over(&named(&profile).to_uppercase()), "~");
        if let Some(short) = crate::host::fs::short_path(&profile) {
            assert_eq!(sheet.over(&named(&short)), "~", "{}", short.display());
        }
        assert!(sheet.over("dana test").starts_with("<user "));
    }

    // The SID as `icacls` advice names it, and as Windows compares one.
    #[test]
    fn a_report_from_windows_hides_the_persons_sid_in_every_form() {
        use crate::context::Environment;
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let profile = scratch.path().join("profile");
        let env = Environment::on(
            Os::Windows,
            [("USERPROFILE", profile.to_string_lossy().into_owned())],
        );
        let mut ctx = Context::read(&env, "cli");
        if cfg!(windows) {
            assert!(ctx.sid.is_some(), "the token's own");
            assert_eq!(ctx.sid, crate::host::user::sid());
        }
        let sid = ctx
            .sid
            .get_or_insert_with(|| "S-1-5-21-1004336348-1177238915-682003330-1001".into())
            .clone();
        let sheet = Sheet::of(&ctx, "salt");
        let hidden = sheet.over(&sid);
        assert!(hidden.starts_with("<sid "), "{hidden}");
        for form in [
            format!(r#""*{sid}:F""#),
            format!(r#""*{sid}:(OI)(CI)F""#),
            sid.to_lowercase(),
        ] {
            let shown = sheet.over(&form);
            assert!(shown.contains(&hidden), "{form}: {shown}");
            assert!(!shown.to_uppercase().contains("S-1-5-21-"), "{shown}");
        }
        assert_eq!(
            sheet.over("*S-1-5-18:F"),
            "*S-1-5-18:F",
            "SYSTEM is nobody's"
        );
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
