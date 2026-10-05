//! This user's processes on macOS, as `ps` lists them.

use crate::host::Process;
use crate::host::bundle::{self, Bundle};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

pub(super) fn processes(program: &str) -> Option<Vec<Process>> {
    // `-x` with no other selection is every process this user owns, with or without a
    // terminal.
    let mut ps = Command::new("/bin/ps");
    ps.args(["-x", "-o", "pid=,comm="]);
    let out = super::helper::output_within(ps, b"", Duration::from_secs(5)).ok()?;
    if !out.status.success() {
        return None;
    }
    Some(named(&String::from_utf8_lossy(&out.stdout), program))
}

/// This user's processes inside `bundle`, by the rule [`bundle::within`] gives, from a
/// listing that also says each one's parent.
pub(super) fn processes_within(bundle: Bundle<'_>, excluded: &[&str]) -> Option<Vec<Process>> {
    let mut ps = Command::new("/bin/ps");
    ps.args(["-x", "-o", "pid=,ppid=,comm="]);
    let out = super::helper::output_within(ps, b"", Duration::from_secs(5)).ok()?;
    if !out.status.success() {
        return None;
    }
    Some(bundle::within(
        &bundle::listed(&String::from_utf8_lossy(&out.stdout)),
        bundle,
        excluded,
    ))
}

/// The program process `pid` runs, as `ps` names it: its full path when whatever started
/// it named one.
pub(super) fn program_of(pid: u32) -> Option<PathBuf> {
    let mut ps = Command::new("/bin/ps");
    ps.args(["-p", &pid.to_string(), "-o", "comm="]);
    let out = super::helper::output_within(ps, b"", Duration::from_secs(5)).ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !path.is_empty()).then(|| PathBuf::from(path))
}

/// The processes in a `ps -o pid=,comm=` listing whose program is called `program`. A path
/// can have spaces in it, so everything after the pid is the path.
fn named(listing: &str, program: &str) -> Vec<Process> {
    listing
        .lines()
        .filter_map(|line| {
            let (pid, path) = line.trim_start().split_once(char::is_whitespace)?;
            let path = PathBuf::from(path.trim());
            (path.file_name()? == program).then_some(Process {
                pid: pid.parse().ok()?,
                path,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_found_by_its_own_name_and_nothing_else() {
        let listing = "  412 /Users/a/.codex/packages/standalone/bin/codex\n\
                       7031 codex\n\
                       7032 /bin/zsh\n\
                         88 /usr/bin/codex-helper\n\
                       9001 /Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex\n\
                       9002 /Users/a/Codex Things/bin/codex\n\
                       node\n";
        let found = named(listing, "codex");
        assert_eq!(
            found.iter().map(|p| p.pid).collect::<Vec<_>>(),
            [412, 7031, 9001, 9002]
        );
        assert_eq!(
            found[3].path,
            PathBuf::from("/Users/a/Codex Things/bin/codex")
        );
        assert!(named(listing, "gemini").is_empty());
    }

    /// The list is readable, and this test's own process is in it, found by its own name,
    /// with where it runs from.
    #[test]
    fn this_users_processes_can_be_read() {
        assert_eq!(processes("definitely-not-a-program-name"), Some(Vec::new()));
        let me = std::env::current_exe().expect("this test's own binary");
        let name = me.file_name().unwrap().to_string_lossy().into_owned();
        let found = processes(&name).expect("readable");
        let mine = found
            .iter()
            .find(|p| p.pid == std::process::id())
            .unwrap_or_else(|| panic!("{name} is running: {found:?}"));
        assert_eq!(mine.path.file_name(), me.file_name());
    }

    /// Another user's processes use another user's login, which a switch here never
    /// touches. The first process is launchd, the system's.
    #[test]
    fn another_users_processes_are_not_listed() {
        let owner = |pid: u32| {
            let mut ps = Command::new("ps");
            ps.args(["-o", "uid=", "-p", &pid.to_string()]);
            super::super::helper::output_within(ps, b"", Duration::from_secs(5))
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                .unwrap_or_default()
        };
        if owner(1) == owner(std::process::id()) {
            // Run as the system's own user, where it is ours.
            return;
        }
        let found = processes("launchd").expect("readable");
        assert!(found.iter().all(|p| p.pid != 1), "launchd: {found:?}");
    }
}
