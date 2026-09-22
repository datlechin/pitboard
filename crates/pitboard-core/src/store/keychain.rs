//! The macOS backend. Every call execs `/usr/bin/security`, the only application the
//! item's access list trusts.
//!
//! Measured on throwaway items: a foreign in-process read adds the caller to that list, and
//! a foreign in-process write replaces the item's partition list, after which every read by
//! `/usr/bin/security` takes 1-3 seconds instead of 0.02, a cost Claude Code then pays on
//! every credential re-read. A write through `security -U` changes only the item's mtime.

use super::{Backend, Error, RawStore};
use crate::context::Context;
use crate::{process, slot};
use std::process::{Command, Output};
use std::time::Duration;

use super::SECURITY;

/// Claude Code's own ceiling on an interactive `security` command line. Past it Claude
/// Code passes the credential as an argument instead, where `ps` can read it; pitboard
/// refuses rather than do that, so this is a real ceiling here and not a transport choice.
pub(super) const MAX_COMMAND_BYTES: usize = 4032;

/// `security` exits with the low byte of the `OSStatus`: `errSecItemNotFound`.
const ITEM_NOT_FOUND: i32 = 44;
/// `errSecInteractionNotAllowed`: the keychain is locked and may not prompt.
const INTERACTION_NOT_ALLOWED: i32 = 36;

/// Claude Code reads an empty answer from its own slot as "absent". Nothing else is: a
/// locked keychain says nothing about what it holds, and reading it as empty would fall
/// through to a stale plaintext file and park or overwrite the wrong login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    ClaudeCode,
    Pitboard,
}

pub(super) struct Keychain {
    owner: Owner,
    /// The keychain account every item is stored under, as Claude Code names it.
    account: String,
    /// Whether a login past the stdin limit may go on the argument line instead.
    argv_fallback: bool,
}

impl Keychain {
    /// The slots Claude Code reads.
    pub(super) fn live(ctx: &Context) -> Keychain {
        Keychain {
            owner: Owner::ClaudeCode,
            account: slot::account_name(ctx),
            argv_fallback: ctx.argv_fallback(),
        }
    }

    /// Where pitboard parks credentials of its own.
    pub(super) fn vault(ctx: &Context) -> Keychain {
        Keychain {
            owner: Owner::Pitboard,
            account: slot::account_name(ctx),
            argv_fallback: ctx.argv_fallback(),
        }
    }
}

enum Presence {
    Present(String),
    Absent,
    Failed(String),
}

fn classify(owner: Owner, code: Option<i32>, stdout: String, stderr: String) -> Presence {
    match code {
        Some(0) if !stdout.trim().is_empty() => Presence::Present(stdout.trim_end().to_string()),
        Some(0) if owner == Owner::ClaudeCode => Presence::Absent,
        Some(ITEM_NOT_FOUND) => Presence::Absent,
        Some(INTERACTION_NOT_ALLOWED) => Presence::Failed(
            "the keychain is locked and cannot ask to be unlocked from here; unlock it with \
             `security unlock-keychain`, or run pitboard from a desktop session"
                .into(),
        ),
        other => Presence::Failed(format!(
            "security exited {}: {}",
            other.map_or_else(|| "on a signal".into(), |c| c.to_string()),
            stderr.trim()
        )),
    }
}

/// `security` answers in milliseconds, unless the keychain is locked and it waits on an
/// unlock prompt. This leaves a person time to answer one and stops a wait no one sees.
const SECURITY_TIMEOUT: Duration = Duration::from_secs(60);

fn security(args: &[&str], input: &str) -> std::io::Result<Output> {
    let mut command = Command::new(SECURITY);
    command.args(args);
    process::output_within(command, input.as_bytes(), SECURITY_TIMEOUT)
}

fn run(args: &[&str], owner: Owner) -> Presence {
    match security(args, "") {
        Err(e) => Presence::Failed(format!("{SECURITY} did not answer: {e}")),
        Ok(out) => classify(
            owner,
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ),
    }
}

/// One line of `dump-keychain` output, as the service name it names.
///
/// The shape is `    "svce"<blob>="pitboard-park-<uuid>-<millis>"`. A name pitboard made
/// contains no quote and no backslash, so a plain read to the closing quote is exact for
/// every name this is asked about, and anything stranger simply does not match.
fn service_of(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("\"svce\"<blob>=\"")?;
    let name = rest.strip_suffix('"')?;
    (!name.contains('\\') && !name.contains('"')).then(|| name.to_string())
}

/// The command `write` will send, so its length can be checked before anything changes.
fn command_for(account: &str, service: &str, secret: &str) -> String {
    format!(
        "add-generic-password -U -a \"{account}\" -s \"{service}\" -X {}\n",
        hex::encode(secret.as_bytes())
    )
}

impl Keychain {
    /// What the command that writes this would cost against what `security -i` reads.
    fn price(&self, service: &str, contents: &str) -> super::Cost {
        super::Cost {
            needs: command_for(&self.account, service, contents).len(),
            limit: MAX_COMMAND_BYTES,
            second_route: self.argv_fallback,
        }
    }

    fn find(&self, service: &str, with_data: bool) -> Presence {
        let account = self.account.as_str();
        let mut args = vec!["find-generic-password", "-a", account, "-s", service];
        if with_data {
            args.push("-w");
        }
        run(&args, self.owner)
    }
}

impl RawStore for Keychain {
    fn kind(&self) -> Backend {
        Backend::Keychain
    }

    fn contains(&self, service: &str) -> Result<bool, Error> {
        match self.find(service, false) {
            Presence::Present(_) => Ok(true),
            Presence::Absent => Ok(false),
            Presence::Failed(m) => Err(Error::Unreadable(m)),
        }
    }

    fn read(&self, service: &str) -> Result<Option<String>, Error> {
        match self.find(service, true) {
            Presence::Present(s) => Ok(Some(s)),
            Presence::Absent => Ok(None),
            Presence::Failed(m) => Err(Error::Unreadable(m)),
        }
    }

    /// Update in place, secret on stdin.
    ///
    /// `-X` takes hex so the value survives any byte; `-i` keeps it out of argv, where `ps`
    /// would expose it. Both names are quoted because `security -i` splits on whitespace
    /// and every real service name contains a space.
    fn write(&self, service: &str, contents: &str) -> Result<(), Error> {
        let account = self.account.as_str();
        if account.contains('"') || service.contains('"') {
            return Err(Error::Write(
                "account or service name contains a quote".into(),
            ));
        }
        let price = self.price(service, contents);
        if price.refused() {
            return Err(Error::Write(format!(
                "this credential is {} bytes, past the {MAX_COMMAND_BYTES}-byte command limit",
                price.needs
            )));
        }

        // Measured on macOS 26: `security -i` reads at most 4097 bytes of command line and
        // treats the rest as another command. Past that the only route `security` offers is
        // the argument line, which is what Claude Code falls back to for the same login.
        let hex = hex::encode(contents.as_bytes());
        let out = if price.over() {
            let args = [
                "add-generic-password",
                "-U",
                "-a",
                account,
                "-s",
                service,
                "-X",
                &hex,
            ];
            security(&args, "")
                .map_err(|e| Error::Write(format!("{SECURITY} did not answer: {e}")))?
        } else {
            security(&["-i"], &command_for(account, service, contents))
                .map_err(|e| Error::Write(format!("{SECURITY} did not answer: {e}")))?
        };
        if out.status.code() != Some(0) {
            return Err(Error::Write(format!(
                "security exited {}: {}",
                out.status
                    .code()
                    .map_or_else(|| "on a signal".into(), |c| c.to_string()),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }

        match self.read(service)? {
            Some(back) if back == contents => Ok(()),
            Some(_) => Err(Error::NotDurable(format!(
                "{service} holds different bytes"
            ))),
            None => Err(Error::NotDurable(format!("{service} reads back empty"))),
        }
    }

    fn delete(&self, service: &str) -> Result<(), Error> {
        let account = self.account.as_str();
        match run(
            &["delete-generic-password", "-a", account, "-s", service],
            self.owner,
        ) {
            Presence::Present(_) | Presence::Absent => Ok(()),
            Presence::Failed(m) => Err(Error::Write(m)),
        }
    }

    /// Measured on macOS 26 against a keychain holding 362 items: `dump-keychain` without
    /// `-d` exits 0 in 0.06 seconds, never prompts, and emits attributes only, no secret of
    /// any item. Reads of pitboard's own items afterwards take the usual 0.016 seconds, so
    /// listing does not carry the access-list side effect an in-process read does.
    ///
    /// Only pitboard's own names are returned, and only from the vault: the live chain has
    /// nothing to enumerate and Claude Code's items are none of pitboard's business.
    fn list(&self) -> Result<Option<Vec<String>>, Error> {
        if self.owner != Owner::Pitboard {
            return Ok(None);
        }
        let out = security(&["dump-keychain"], "")
            .map_err(|e| Error::Unreadable(format!("{SECURITY} did not answer: {e}")))?;
        if out.status.code() != Some(0) {
            return Err(Error::Unreadable(format!(
                "security dump-keychain exited {}",
                out.status
                    .code()
                    .map_or_else(|| "on a signal".into(), |c| c.to_string())
            )));
        }
        let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(service_of)
            .filter(|name| crate::park::is_park_name(name))
            .collect();
        names.sort();
        names.dedup();
        Ok(Some(names))
    }

    fn cost(&self, service: &str, contents: &str) -> Option<super::Cost> {
        Some(self.price(service, contents))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_codes_absent_codes_are_absent_and_the_rest_abort() {
        for code in [0, ITEM_NOT_FOUND] {
            assert!(matches!(
                classify(Owner::ClaudeCode, Some(code), String::new(), String::new()),
                Presence::Absent
            ));
        }
        for code in [1, INTERACTION_NOT_ALLOWED, 37, 50, 128] {
            assert!(matches!(
                classify(Owner::ClaudeCode, Some(code), String::new(), String::new()),
                Presence::Failed(_)
            ));
        }
    }

    #[test]
    fn our_own_items_only_accept_item_not_found_as_absent() {
        assert!(matches!(
            classify(
                Owner::Pitboard,
                Some(ITEM_NOT_FOUND),
                String::new(),
                String::new()
            ),
            Presence::Absent
        ));
        for code in [0, INTERACTION_NOT_ALLOWED, 37, 50] {
            assert!(matches!(
                classify(Owner::Pitboard, Some(code), String::new(), String::new()),
                Presence::Failed(_)
            ));
        }
    }

    #[test]
    fn a_signal_is_a_failure_not_an_absence() {
        assert!(matches!(
            classify(Owner::ClaudeCode, None, String::new(), String::new()),
            Presence::Failed(_)
        ));
    }

    #[test]
    fn the_command_quotes_both_names_and_hexes_the_secret() {
        let c = command_for("me", "Claude Code-credentials", "{\"a\":1}");
        assert!(
            c.starts_with("add-generic-password -U -a \"me\" -s \"Claude Code-credentials\" -X ")
        );
        assert!(c.ends_with("7b2261223a317d\n"));
        assert!(
            !c.contains("{\"a\":1}"),
            "the secret must not appear as plain text"
        );
    }

    /// Measured on macOS 26: `security -i` reads 4097 bytes of a command and treats the
    /// rest as another command, so a hex-encoded secret of about two kilobytes is the most
    /// that route carries. Past it the argument line is the only way, which is what Claude
    /// Code uses for the same login.
    /// One line of `dump-keychain`, as the name it carries. Measured against real output.
    #[test]
    fn a_dumped_line_gives_up_the_service_it_names() {
        assert_eq!(
            service_of(r#"    "svce"<blob>="pitboard-park-acc-1790000000000""#).as_deref(),
            Some("pitboard-park-acc-1790000000000")
        );
        assert_eq!(
            service_of(r#"    "svce"<blob>="Claude Code-credentials""#).as_deref(),
            Some("Claude Code-credentials"),
            "parsing is not filtering; the caller decides what it wants"
        );
        // Everything else in a dump, and anything that would need unescaping.
        assert_eq!(service_of(r#"    "acct"<blob>="ngoquocdat""#), None);
        assert_eq!(
            service_of("keychain: \"/Users/x/Library/Keychains/login\""),
            None
        );
        assert_eq!(service_of(r#"    "svce"<blob>=0x00"#), None);
        assert_eq!(service_of(r#"    "svce"<blob>="has\\backslash""#), None);
        assert_eq!(service_of(""), None);
    }

    #[test]
    fn the_stdin_route_stops_at_about_two_kilobytes() {
        let ctx = Context::from_env();
        let live = Keychain::live(&ctx);
        assert!(live.price("svc", &"x".repeat(2100)).over());
        assert!(!live.price("svc", &"x".repeat(1900)).over());
    }

    /// The refusal is available to whoever wants it, and is not the default: there is no
    /// third way to write a login this size.
    #[test]
    fn only_a_refusing_context_refuses_a_login_this_size() {
        let ctx = Context::from_env();
        let big = "x".repeat(2100);
        let allowed = Keychain::live(&ctx).price("svc", &big);
        assert!(allowed.on_the_second_route());
        assert!(!allowed.refused());

        let refusing = Keychain::live(&ctx.with_argv_fallback(false)).price("svc", &big);
        assert!(refusing.refused());
        assert!(!refusing.on_the_second_route());
    }
}
