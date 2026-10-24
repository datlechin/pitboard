//! This user's processes on macOS, as `ps` lists them.

use crate::host::{Input, Listed};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

pub(super) fn listing() -> Result<Vec<Listed>, String> {
    // `-x` with no other selection is every process this user owns, with or without a
    // terminal.
    let mut ps = Command::new("/bin/ps");
    ps.args(["-x", "-o", "pid=,ppid=,comm="]);
    let out = match super::proc::run_within(ps, Input::Nothing, Duration::from_secs(5)) {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            return Err("`ps` did not answer within 5 seconds".into());
        }
        Err(e) => return Err(format!("`ps` could not be started: {e}")),
    };
    if !out.status.success() {
        return Err(format!("`ps` exited with status {}", out.status));
    }
    Ok(listed(&String::from_utf8_lossy(&out.stdout)))
}

// A path can hold spaces, so everything after the parent is the path.
fn listed(listing: &str) -> Vec<Listed> {
    listing
        .lines()
        .filter_map(|line| {
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            let (parent, path) = rest.trim_start().split_once(char::is_whitespace)?;
            Some(Listed {
                pid: pid.parse().ok()?,
                parent: parent.parse().ok()?,
                path: PathBuf::from(path.trim()),
                package: None,
                placed: true,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The contributor's `ps` sample of 3 October 2026, Chrome's native host started both ways.
    const CLAUDE_LISTING: &str = "\
  501     1 /Applications/Claude.app/Contents/MacOS/Claude
  502   501 /Applications/Claude.app/Contents/Frameworks/Claude Helper (Renderer).app/Contents/MacOS/Claude Helper (Renderer)
  503   501 /Applications/Claude.app/Contents/Frameworks/Electron Framework.framework/Helpers/chrome_crashpad_handler
  590     1 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
  600   590 /Applications/Claude.app/Contents/Helpers/chrome-native-host
  601   501 /Applications/Claude.app/Contents/Helpers/chrome-native-host
  702     1 /usr/local/bin/claude
  703     1 Claude
garbage
  704
";

    #[test]
    fn a_listing_keeps_each_process_with_its_parent_and_its_whole_path() {
        let found = listed(CLAUDE_LISTING);
        assert_eq!(
            found.iter().map(|p| (p.pid, p.parent)).collect::<Vec<_>>(),
            [
                (501, 1),
                (502, 501),
                (503, 501),
                (590, 1),
                (600, 590),
                (601, 501),
                (702, 1),
                (703, 1)
            ]
        );
        assert_eq!(
            found[1].path,
            PathBuf::from(
                "/Applications/Claude.app/Contents/Frameworks/Claude Helper (Renderer).app/\
                 Contents/MacOS/Claude Helper (Renderer)"
            )
        );
        assert_eq!(found[7].path, PathBuf::from("Claude"));
    }

    #[test]
    fn a_program_is_found_by_its_own_name_and_nothing_else() {
        let listing = "  412     1 /Users/a/.codex/packages/standalone/bin/codex\n\
                       7031     1 codex\n\
                       7032     1 /bin/zsh\n\
                         88     1 /usr/bin/codex-helper\n\
                       9001     1 /Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex\n\
                       9002     1 /Users/a/Codex Things/bin/codex\n\
                       node\n";
        let named = |program: &str| {
            crate::host::named(crate::host::Os::MacOs, listed(listing), program)
                .expect("every one placed")
        };
        let found = named("codex");
        assert_eq!(
            found.iter().map(|p| p.pid).collect::<Vec<_>>(),
            [412, 7031, 9001, 9002]
        );
        assert_eq!(
            found[3].path,
            PathBuf::from("/Users/a/Codex Things/bin/codex")
        );
        assert!(named("gemini").is_empty());
    }

    /// The list is readable, and this test's own process is in it, found by its own name,
    /// with where it runs from.
    #[test]
    fn this_users_processes_can_be_read() {
        let host = super::super::host();
        assert_eq!(
            host.processes("definitely-not-a-program-name"),
            Some(Vec::new())
        );
        let me = std::env::current_exe().expect("this test's own binary");
        let name = me.file_name().unwrap().to_string_lossy().into_owned();
        let found = host.processes(&name).expect("readable");
        let mine = found
            .iter()
            .find(|p| p.pid == std::process::id())
            .unwrap_or_else(|| panic!("{name} is running: {found:?}"));
        assert_eq!(mine.path.file_name(), me.file_name());
        let listed = listing().expect("readable");
        assert!(
            listed
                .iter()
                .any(|p| p.pid == std::process::id() && p.parent != 0),
            "{listed:?}"
        );
    }

    /// Another user's processes use another user's login, which a switch here never
    /// touches. The first process is launchd, the system's.
    #[test]
    fn another_users_processes_are_not_listed() {
        let owner = |pid: u32| {
            let mut ps = Command::new("ps");
            ps.args(["-o", "uid=", "-p", &pid.to_string()]);
            super::super::proc::run_within(ps, Input::Nothing, Duration::from_secs(5))
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                .unwrap_or_default()
        };
        if owner(1) == owner(std::process::id()) {
            // Run as the system's own user, where it is ours.
            return;
        }
        let found = listing().expect("readable");
        assert!(found.iter().all(|p| p.pid != 1), "launchd: {found:?}");
    }
}
