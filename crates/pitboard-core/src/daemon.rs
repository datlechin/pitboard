//! Claude Code's supervisor daemon, read only. It outlives the session that started it and
//! refreshes the login on a schedule of its own, so it is a second writer of the credential
//! that has nothing to do with anyone typing.
//!
//! Measured in 2.1.278. The daemon writes `<config dir>/daemon.lock` when it starts and
//! leaves it behind when it dies, so the file says which daemon ran, not that one is
//! running; the pid decides that. It logs `auth: scheduling proactive refresh in <n>s` with
//! n around eight hours, and exits on its own once no session has used it for a while.
//!
//! Its refresh is a credential write like any other: it takes the same `.storage-write`
//! lock [`crate::lock`] takes, and every write there invalidates the read cache and reads
//! the credential again inside the lock before changing it. So a daemon that was started
//! before a switch cannot write the outgoing account back over the incoming one. What it
//! can do is rotate the refresh token of whatever login is in the slot at the time, which
//! is why a switch reads the slot back rather than trusting that its own write stood.

use crate::atomic;
use crate::claude;
use crate::context::Context;
use serde_json::Value;
use std::path::PathBuf;

/// What `daemon.lock` says, and whether that daemon is still there.
#[derive(Debug, Clone, PartialEq)]
pub struct Daemon {
    pub pid: u32,
    /// The Claude Code that is running it, as the daemon recorded itself.
    pub version: Option<String>,
    /// Milliseconds since the epoch, as the daemon recorded them.
    pub started_at: Option<i64>,
    /// `transient` for one an ordinary session spawned.
    pub origin: Option<String>,
    /// The build the daemon launched, which is the installed Claude Code at that moment.
    pub launch_target: Option<PathBuf>,
    /// The pid answers to a signal. A pid the system has since reused would read as running,
    /// which over-reports a daemon rather than missing one, and is the safe direction for a
    /// fact that only ever warns.
    pub running: bool,
}

/// The daemon of this slot, where one has ever run. `None` means no `daemon.lock`, which is
/// an ordinary state: a machine whose Claude Code has not started a daemon yet.
pub fn read(ctx: &Context) -> Option<Daemon> {
    parse(
        &std::fs::read_to_string(lock_path(ctx)).ok()?,
        atomic::may_be_running,
    )
}

pub fn lock_path(ctx: &Context) -> PathBuf {
    claude::config_dir(ctx).join("daemon.lock")
}

fn parse(raw: &str, alive: fn(u32) -> bool) -> Option<Daemon> {
    let json: Value = serde_json::from_str(raw).ok()?;
    let pid: u32 = json.get("pid").and_then(Value::as_u64)?.try_into().ok()?;
    Some(Daemon {
        pid,
        version: text(&json, "version"),
        started_at: json.get("startedAt").and_then(Value::as_i64),
        origin: text(&json, "origin"),
        launch_target: text(&json, "launchTarget").map(PathBuf::from),
        running: alive(pid),
    })
}

fn text(json: &Value, key: &str) -> Option<String> {
    json.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = r#"{
        "pid": 46789,
        "version": "2.1.278",
        "jsonPath": "/home/someone/.claude/daemon.json",
        "startedAt": 1790079766317,
        "origin": "transient",
        "procStart": "Tue Sep 22 12:22:46 2026",
        "launchTarget": "/home/someone/.local/share/claude/versions/2.1.278",
        "processWrapper": ""
    }"#;

    fn dead(_: u32) -> bool {
        false
    }
    fn live(_: u32) -> bool {
        true
    }

    #[test]
    fn reads_what_the_daemon_wrote() {
        let d = parse(LOCK, live).expect("a lock file with a pid parses");
        assert_eq!(d.pid, 46789);
        assert_eq!(d.version.as_deref(), Some("2.1.278"));
        assert_eq!(d.started_at, Some(1790079766317));
        assert_eq!(d.origin.as_deref(), Some("transient"));
        assert_eq!(
            d.launch_target,
            Some(PathBuf::from(
                "/home/someone/.local/share/claude/versions/2.1.278"
            ))
        );
        assert!(d.running);
    }

    #[test]
    fn a_lock_left_behind_by_a_dead_daemon_is_not_running() {
        assert!(!parse(LOCK, dead).expect("parses").running);
    }

    #[test]
    fn empty_strings_are_absences() {
        let d = parse(r#"{"pid": 1, "version": "", "processWrapper": ""}"#, dead)
            .expect("a pid is all that is required");
        assert_eq!(d.version, None);
        assert_eq!(d.origin, None);
        assert_eq!(d.launch_target, None);
        assert_eq!(d.started_at, None);
    }

    #[test]
    fn without_a_pid_there_is_nothing_to_report() {
        assert_eq!(parse(r#"{"version": "2.1.278"}"#, live), None);
        assert_eq!(parse(r#"{"pid": -1}"#, live), None);
        assert_eq!(parse("not json", live), None);
    }
}
