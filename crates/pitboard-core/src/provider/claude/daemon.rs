//! Claude Code's supervisor daemon, read only (`supervisor_daemon`, `windows_daemon_identity`).

// `daemon.lock` outlives the daemon, so its pid, and its creation time on Windows, say whether
// one runs. A daemon renews whatever login is in the slot, so a switch reads the slot back.

use super::paths as claude;
use crate::context::Context;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Daemon {
    pub(crate) pid: u32,
    pub(crate) version: Option<String>,
    // Off Windows a reused pid reads as running: over-reporting is the safe side of a warning.
    pub(crate) running: bool,
}

// None is no `daemon.lock`: a Claude Code that has not started a daemon yet.
pub(crate) fn read(ctx: &Context) -> Option<Daemon> {
    parse(
        &std::fs::read_to_string(lock_path(ctx)).ok()?,
        crate::host::proc::may_be_running,
        crate::host::proc::started,
    )
}

pub(crate) fn lock_path(ctx: &Context) -> PathBuf {
    claude::config_dir(ctx).join("daemon.lock")
}

fn parse(raw: &str, alive: fn(u32) -> bool, started: fn(u32) -> Option<u64>) -> Option<Daemon> {
    let json: Value = serde_json::from_str(raw).ok()?;
    let pid: u32 = json.get("pid").and_then(Value::as_u64)?.try_into().ok()?;
    // Compared as text, as Claude Code compares it (windows_daemon_identity).
    let same = match (text(&json, "procStartFt"), started(pid)) {
        (Some(recorded), Some(now)) => recorded == now.to_string(),
        _ => true,
    };
    Some(Daemon {
        pid,
        version: text(&json, "version"),
        running: alive(pid) && same,
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
    fn untimed(_: u32) -> Option<u64> {
        None
    }

    // The fields 2.1.300's win32 builds write with bun:ffi loaded, the values made up.
    const WINDOWS_LOCK: &str = r#"{
        "pid": 7312,
        "version": "2.1.300",
        "jsonPath": "C:\\Users\\someone\\.claude\\daemon.json",
        "logPath": "C:\\Users\\someone\\.claude\\daemon.log",
        "startedAt": 1792054345123,
        "origin": "transient",
        "procStartFt": "134047912451234567",
        "processWrapper": ""
    }"#;

    #[test]
    fn reads_what_the_daemon_wrote() {
        let d = parse(LOCK, live, untimed).expect("a lock file with a pid parses");
        assert_eq!(d.pid, 46789);
        assert_eq!(d.version.as_deref(), Some("2.1.278"));
        assert!(d.running);
    }

    #[test]
    fn a_lock_left_behind_by_a_dead_daemon_is_not_running() {
        assert!(!parse(LOCK, dead, untimed).expect("parses").running);
    }

    // So a pid Windows has given another process since is not the daemon.
    #[test]
    fn on_windows_the_pid_is_the_daemon_only_while_it_started_when_the_lock_says() {
        fn then(_: u32) -> Option<u64> {
            Some(134_047_912_451_234_567)
        }
        fn later(_: u32) -> Option<u64> {
            Some(134_047_912_461_234_567)
        }
        assert!(parse(WINDOWS_LOCK, live, then).expect("parses").running);
        assert!(!parse(WINDOWS_LOCK, live, later).expect("parses").running);
        assert!(
            parse(WINDOWS_LOCK, live, untimed).expect("parses").running,
            "a time that does not read leaves the pid's answer"
        );
        assert!(!parse(WINDOWS_LOCK, dead, then).expect("parses").running);
        assert!(
            parse(LOCK, live, later).expect("parses").running,
            "a lock with no `procStartFt` is not compared"
        );
    }

    #[test]
    fn empty_strings_are_absences() {
        let d = parse(
            r#"{"pid": 1, "version": "", "processWrapper": ""}"#,
            dead,
            untimed,
        )
        .expect("a pid is all that is required");
        assert_eq!(d.version, None);
    }

    #[test]
    fn without_a_pid_there_is_nothing_to_report() {
        assert_eq!(parse(r#"{"version": "2.1.278"}"#, live, untimed), None);
        assert_eq!(parse(r#"{"pid": -1}"#, live, untimed), None);
        assert_eq!(parse("not json", live, untimed), None);
    }

    // A real one on Windows, where this test's own creation time is what a lock would hold.
    #[test]
    #[cfg(windows)]
    fn this_process_is_the_daemon_its_lock_names() {
        let me = std::process::id();
        let started = crate::host::proc::started(me).expect("this process's creation time");
        let lock = format!(r#"{{"pid": {me}, "procStartFt": "{started}"}}"#);
        let read = |lock: &str| {
            parse(
                lock,
                crate::host::proc::may_be_running,
                crate::host::proc::started,
            )
            .expect("parses")
            .running
        };
        assert!(read(&lock));
        assert!(!read(&format!(
            r#"{{"pid": {me}, "procStartFt": "{}"}}"#,
            started + 1
        )));
    }
}
