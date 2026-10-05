//! This user's processes on Linux, as `/proc` says.
//!
//! Every Linux has `/proc`, where `/bin/ps` is not on every one: a slim container or NixOS
//! has none, and the warning this feeds would quietly vanish.

use crate::host::Process;
use crate::host::bundle::{self, Bundle, Listed};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

pub(super) fn processes(program: &str) -> Option<Vec<Process>> {
    let me = std::fs::metadata("/proc/self").ok()?.uid();
    let mut found: Vec<Process> = std::fs::read_dir("/proc")
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            if entry.metadata().ok()?.uid() != me {
                return None;
            }
            let name = std::fs::read_to_string(entry.path().join("comm")).ok()?;
            (name.trim() == program).then(|| Process {
                pid,
                path: std::fs::read_link(entry.path().join("exe"))
                    .unwrap_or_else(|_| PathBuf::from(program)),
            })
        })
        .collect();
    found.sort_unstable_by_key(|p| p.pid);
    Some(found)
}

/// This user's processes inside `bundle`, by the rule [`bundle::within`] gives.
pub(super) fn processes_within(bundle: Bundle<'_>, excluded: &[&str]) -> Option<Vec<Process>> {
    let me = std::fs::metadata("/proc/self").ok()?.uid();
    let listed: Vec<Listed> = std::fs::read_dir("/proc")
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            if entry.metadata().ok()?.uid() != me {
                return None;
            }
            // The parent is the second field after the name, which is in parentheses and
            // may itself hold spaces and parentheses.
            let stat = std::fs::read_to_string(entry.path().join("stat")).ok()?;
            let (_, rest) = stat.rsplit_once(')')?;
            let ppid = rest.split_whitespace().nth(1)?.parse().ok()?;
            let path = std::fs::read_link(entry.path().join("exe")).ok()?;
            Some(Listed { pid, ppid, path })
        })
        .collect();
    Some(bundle::within(&listed, bundle, excluded))
}

/// The file process `pid` runs, where this user may read that.
pub(super) fn program_of(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list is readable, and this test's own process is in it, found by its own name,
    /// with where it runs from.
    #[test]
    fn this_users_processes_can_be_read() {
        assert_eq!(processes("definitely-not-a-program-name"), Some(Vec::new()));
        let me = std::env::current_exe().expect("this test's own binary");
        // Linux keeps fifteen characters of a process's name, and test binaries are longer.
        let name: String = me
            .file_name()
            .unwrap()
            .to_string_lossy()
            .chars()
            .take(15)
            .collect();
        let found = processes(&name).expect("readable");
        let mine = found
            .iter()
            .find(|p| p.pid == std::process::id())
            .unwrap_or_else(|| panic!("{name} is running: {found:?}"));
        assert_eq!(mine.path.file_name(), me.file_name());
    }

    /// Another user's processes use another user's login, which a switch here never
    /// touches. The first process of every machine is the system's.
    #[test]
    fn another_users_processes_are_not_listed() {
        let uid = |pid: &str| std::fs::metadata(format!("/proc/{pid}")).map(|m| m.uid());
        if uid("1").ok() == uid("self").ok() {
            // Run as the system's own user, as in some containers, where it is ours.
            return;
        }
        let first = std::fs::read_to_string("/proc/1/comm")
            .expect("the first process")
            .trim()
            .to_string();
        let found = processes(&first).expect("readable");
        assert!(found.iter().all(|p| p.pid != 1), "{first}: {found:?}");
    }
}
