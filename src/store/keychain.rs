//! All keychain access execs `/usr/bin/security`.
//!
//! The item's Decrypt ACL trusts only `/usr/bin/security`. A foreign in-process read
//! through the Security framework permanently appends the caller to that ACL and its
//! code hash to the partition list, and a poisoned partition list costs `/usr/bin/security`
//! 1-3 seconds per read instead of 0.02 — which Claude Code then pays on every credential
//! re-read, silently. Writing through `security -U` leaves cdat, the ACL and the partition
//! list byte-identical; only mtime moves.

use super::{Owner, Presence};
use crate::{hex, slot};
use std::io::Write;
use std::process::{Command, Stdio};

const SECURITY: &str = "/usr/bin/security";

/// Claude Code's own ceiling on an interactive `security` command line.
const MAX_COMMAND_BYTES: usize = 4032;

fn classify(owner: Owner, code: Option<i32>, stdout: String, stderr: String) -> Presence {
    match code {
        Some(0) if !stdout.trim().is_empty() => Presence::Present(stdout.trim_end().to_string()),
        Some(0) if owner == Owner::ClaudeCode => Presence::Absent,
        Some(44) => Presence::Absent,
        Some(36) if owner == Owner::ClaudeCode => Presence::Absent,
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

pub fn attributes(service: &str) -> Presence {
    let account = slot::account_name();
    run(
        &["find-generic-password", "-a", &account, "-s", service],
        Owner::ClaudeCode,
    )
}

pub fn read(service: &str, owner: Owner) -> Presence {
    let account = slot::account_name();
    run(
        &["find-generic-password", "-a", &account, "-s", service, "-w"],
        owner,
    )
}

/// The command `write` will send, so its length can be checked before anything is changed.
fn command_for(account: &str, service: &str, secret: &str) -> String {
    format!(
        "add-generic-password -U -a \"{account}\" -s \"{service}\" -X {}\n",
        hex::encode(secret.as_bytes())
    )
}

pub fn projected_command_bytes(service: &str, secret: &str) -> usize {
    command_for(&slot::account_name(), service, secret).len()
}

pub fn too_large(service: &str, secret: &str) -> bool {
    projected_command_bytes(service, secret) > MAX_COMMAND_BYTES
}

/// Update in place, secret on stdin.
///
/// `-X` takes hex so the value survives any byte; `-i` keeps it out of argv, where `ps`
/// would expose it. Both `-a` and `-s` are quoted because `security -i` splits on
/// whitespace and every real service name contains a space.
pub fn write(service: &str, secret: &str) -> Result<(), String> {
    if too_large(service, secret) {
        return Err(format!(
            "credential is {} bytes, past the {MAX_COMMAND_BYTES}-byte command limit",
            projected_command_bytes(service, secret)
        ));
    }
    let account = slot::account_name();
    if account.contains('"') || service.contains('"') {
        return Err("account or service name contains a quote".into());
    }
    let mut child = Command::new(SECURITY)
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot exec {SECURITY}: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no stdin on security")?
        .write_all(command_for(&account, service, secret).as_bytes())
        .map_err(|e| format!("cannot write to security: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("security did not finish: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(()),
        other => Err(format!(
            "security exited {}: {}",
            other.map_or_else(|| "on a signal".into(), |c| c.to_string()),
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

pub fn delete(service: &str) -> Result<(), String> {
    let account = slot::account_name();
    match run(
        &["delete-generic-password", "-a", &account, "-s", service],
        Owner::Pitboard,
    ) {
        Presence::Present(_) | Presence::Absent => Ok(()),
        Presence::Failed(m) => Err(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_codes_absent_codes_are_absent_and_the_rest_abort() {
        for code in [0, 44, 36] {
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
    fn our_own_items_only_accept_44_as_absent() {
        assert!(matches!(
            classify(Owner::Pitboard, Some(44), String::new(), String::new()),
            Presence::Absent
        ));
        for code in [0, 36, 37, 50] {
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
        let big = "x".repeat(2100);
        assert!(too_large("svc", &big));
        assert!(!too_large("svc", &"x".repeat(1900)));
    }
}
