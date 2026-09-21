//! The macOS backend. Every call execs `/usr/bin/security`, the only application the
//! item's access list trusts.
//!
//! Measured on throwaway items: a foreign in-process read adds the caller to that list, and
//! a foreign in-process write replaces the item's partition list, after which every read by
//! `/usr/bin/security` takes 1-3 seconds instead of 0.02 — a cost Claude Code then pays on
//! every credential re-read. A write through `security -U` changes only the item's mtime.

use super::{Backend, Error, RawStore};
use crate::{hex, slot};
use std::io::Write;
use std::process::{Command, Stdio};

const SECURITY: &str = "/usr/bin/security";

/// Claude Code's own ceiling on an interactive `security` command line.
const MAX_COMMAND_BYTES: usize = 4032;

/// `security` exits with the low byte of the `OSStatus`: `errSecItemNotFound`.
const ITEM_NOT_FOUND: i32 = 44;
/// `errSecInteractionNotAllowed`: the keychain is locked and may not prompt.
const INTERACTION_NOT_ALLOWED: i32 = 36;

/// Claude Code reads an empty answer and a locked keychain as "absent", and its slot is read
/// the same way so both agree on which backend is live. For pitboard's own items only
/// `ITEM_NOT_FOUND` means absent: reading could-not-tell as nothing-there loses a login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    ClaudeCode,
    Pitboard,
}

pub(super) struct Keychain {
    owner: Owner,
}

/// The slot Claude Code reads.
pub(super) const LIVE: Keychain = Keychain {
    owner: Owner::ClaudeCode,
};
/// Where pitboard parks credentials of its own.
pub(super) const VAULT: Keychain = Keychain {
    owner: Owner::Pitboard,
};

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
        Some(INTERACTION_NOT_ALLOWED) if owner == Owner::ClaudeCode => Presence::Absent,
        other => Presence::Failed(format!(
            "security exited {}: {}",
            other.map_or_else(|| "on a signal".into(), |c| c.to_string()),
            stderr.trim()
        )),
    }
}

fn run(args: &[&str], owner: Owner) -> Presence {
    match Command::new(SECURITY).args(args).output() {
        Err(e) => Presence::Failed(format!("cannot exec {SECURITY}: {e}")),
        Ok(out) => classify(
            owner,
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ),
    }
}

/// The command `write` will send, so its length can be checked before anything changes.
fn command_for(account: &str, service: &str, secret: &str) -> String {
    format!(
        "add-generic-password -U -a \"{account}\" -s \"{service}\" -X {}\n",
        hex::encode(secret.as_bytes())
    )
}

impl Keychain {
    fn find(&self, service: &str, with_data: bool) -> Presence {
        let account = slot::account_name();
        let mut args = vec!["find-generic-password", "-a", &account, "-s", service];
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
        if self.too_large(service, contents) {
            return Err(Error::Write(format!(
                "this credential is {} bytes, past the {MAX_COMMAND_BYTES}-byte command limit",
                command_for(&slot::account_name(), service, contents).len()
            )));
        }
        let account = slot::account_name();
        if account.contains('"') || service.contains('"') {
            return Err(Error::Write(
                "account or service name contains a quote".into(),
            ));
        }

        let mut child = Command::new(SECURITY)
            .arg("-i")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Write(format!("cannot exec {SECURITY}: {e}")))?;
        child
            .stdin
            .take()
            .ok_or_else(|| Error::Write("security has no stdin".into()))?
            .write_all(command_for(&account, service, contents).as_bytes())
            .map_err(|e| Error::Write(format!("cannot write to security: {e}")))?;
        let out = child
            .wait_with_output()
            .map_err(|e| Error::Write(format!("security did not finish: {e}")))?;
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
        let account = slot::account_name();
        match run(
            &["delete-generic-password", "-a", &account, "-s", service],
            self.owner,
        ) {
            Presence::Present(_) | Presence::Absent => Ok(()),
            Presence::Failed(m) => Err(Error::Write(m)),
        }
    }

    fn too_large(&self, service: &str, contents: &str) -> bool {
        command_for(&slot::account_name(), service, contents).len() > MAX_COMMAND_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_codes_absent_codes_are_absent_and_the_rest_abort() {
        for code in [0, ITEM_NOT_FOUND, INTERACTION_NOT_ALLOWED] {
            assert!(matches!(
                classify(Owner::ClaudeCode, Some(code), String::new(), String::new()),
                Presence::Absent
            ));
        }
        for code in [1, 37, 50, 128] {
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

    #[test]
    fn oversize_credentials_are_refused_before_anything_is_written() {
        assert!(LIVE.too_large("svc", &"x".repeat(2100)));
        assert!(!LIVE.too_large("svc", &"x".repeat(1900)));
    }
}
