//! Which of this user's processes run from inside an app bundle, by one rule every system's
//! process list is read by.

use super::Process;
use std::path::{Path, PathBuf};

/// One line of a process list that also says each process's parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Listed {
    pub(super) pid: u32,
    pub(super) ppid: u32,
    pub(super) path: PathBuf,
}

/// Which app bundle [`super::Host::processes_within`] asks about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bundle<'a> {
    /// Every bundle of this name, wherever it is: the outermost one a program is in.
    Named(&'a str),
    /// The bundle at exactly this path, and no other, whatever it is called.
    At(&'a Path),
}

impl Bundle<'_> {
    /// The bundle `path` runs from, if it runs from one this asks about. By name, that is
    /// the outermost bundle of that name, or of the copies in `renamed`.
    fn root_of<'p>(&self, path: &'p Path, renamed: &[&Path]) -> Option<&'p Path> {
        match *self {
            Bundle::Named(name) => path
                .ancestors()
                .skip(1)
                .filter(|dir| {
                    dir.file_name().is_some_and(|dir| dir == name) || renamed.contains(dir)
                })
                .last(),
            Bundle::At(root) => path.ancestors().skip(1).find(|dir| *dir == root),
        }
    }

    /// The program the bundle's app runs, which a Mac app is named for: `Claude` for
    /// `Claude.app`. Renaming the bundle in the Finder leaves it as it was.
    pub fn program(&self) -> Option<&str> {
        let name = match *self {
            Bundle::Named(name) => name,
            Bundle::At(root) => root.file_name()?.to_str()?,
        };
        name.strip_suffix(".app")
    }

    /// The bundles of `listed`, by name, that are a copy of this one under another name:
    /// the outermost bundle around a process running this one's program, as its main
    /// program in `Contents/MacOS` or as one of its helpers in `Contents/Frameworks`.
    fn renamed<'p>(&self, listed: &'p [Listed]) -> Vec<&'p Path> {
        let (Bundle::Named(_), Some(program)) = (self, self.program()) else {
            return Vec::new();
        };
        let helper = format!("{program} Helper");
        let mut found: Vec<&Path> = listed
            .iter()
            .filter_map(|process| {
                let app = process
                    .path
                    .ancestors()
                    .skip(1)
                    .filter(|dir| dir.extension().is_some_and(|ext| ext == "app"))
                    .last()?;
                let rest = process.path.strip_prefix(app).ok()?;
                let parts: Vec<&str> = rest
                    .components()
                    .map(|c| c.as_os_str().to_str())
                    .collect::<Option<_>>()?;
                let ours = match parts.as_slice() {
                    ["Contents", "MacOS", main] => *main == program,
                    ["Contents", "Frameworks", inner, ..] => {
                        inner.starts_with(&helper) && inner.ends_with(".app")
                    }
                    _ => false,
                };
                ours.then_some(app)
            })
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }
}

/// The lines of a `ps -o pid=,ppid=,comm=` listing. A path can have spaces in it, so
/// everything after the parent is the path; a line that is not three fields is skipped.
// Linux reads `/proc` rather than `ps`, so there only the tests read a listing.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) fn listed(listing: &str) -> Vec<Listed> {
    listing
        .lines()
        .filter_map(|line| {
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            let (ppid, path) = rest.trim_start().split_once(char::is_whitespace)?;
            Some(Listed {
                pid: pid.parse().ok()?,
                ppid: ppid.parse().ok()?,
                path: PathBuf::from(path.trim()),
            })
        })
        .collect()
}

/// The processes of `listed` inside `bundle`, in pid order: every one wherever in the
/// bundle its program is, except those at a path in `excluded` (relative to the bundle)
/// that the bundle's own processes did not start.
///
/// An excluded program counts when the app itself started it: Claude's
/// `chrome-native-host` is started by Chrome for the browser extension and holds nothing of
/// the app's data folder, but one the app started is the app's.
pub(super) fn within(listed: &[Listed], bundle: Bundle<'_>, excluded: &[&str]) -> Vec<Process> {
    // Where in its bundle each process runs from, for those inside one. By name, the
    // outermost bundle of that name is the app's: a helper is a bundle of its own inside it.
    let renamed = bundle.renamed(listed);
    let inside: Vec<(&Listed, &Path)> = listed
        .iter()
        .filter_map(|process| {
            let root = bundle.root_of(&process.path, &renamed)?;
            Some((process, process.path.strip_prefix(root).ok()?))
        })
        .collect();
    let is_excluded = |rest: &Path| excluded.iter().any(|path| rest.starts_with(path));
    let own: std::collections::HashSet<u32> = inside
        .iter()
        .filter(|(_, rest)| !is_excluded(rest))
        .map(|(process, _)| process.pid)
        .collect();
    let mut found: Vec<Process> = inside
        .into_iter()
        .filter(|(process, rest)| !is_excluded(rest) || own.contains(&process.ppid))
        .map(|(process, _)| Process {
            pid: process.pid,
            path: process.path.clone(),
        })
        .collect();
    found.sort_unstable_by_key(|p| p.pid);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample of `ps -x -o pid=,ppid=,comm=` from a Mac running Claude, with Chrome's
    /// native host for the browser extension both as Chrome starts it and as Claude does.
    const CLAUDE_LISTING: &str = "\
  501     1 /Applications/Claude.app/Contents/MacOS/Claude
  502   501 /Applications/Claude.app/Contents/Frameworks/Claude Helper (Renderer).app/Contents/MacOS/Claude Helper (Renderer)
  503   501 /Applications/Claude.app/Contents/Frameworks/Electron Framework.framework/Helpers/chrome_crashpad_handler
  590     1 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
  600   590 /Applications/Claude.app/Contents/Helpers/chrome-native-host
  601   501 /Applications/Claude.app/Contents/Helpers/chrome-native-host
  700     1 /Applications/Claude Other.app/Contents/MacOS/Claude
  701     1 /Users/a/Downloads/Claude.app
  702     1 /usr/local/bin/claude
  703     1 Claude
  704   700 /Applications/Claude Other.app/Contents/Frameworks/Electron Framework.framework/Helpers/chrome_crashpad_handler
  705     1 /Applications/Claude 2.app/Contents/Frameworks/Claude Helper (GPU).app/Contents/MacOS/Claude Helper (GPU)
  706     1 /Applications/Notes.app/Contents/MacOS/Notes
  707     1 /Applications/Other.app/Contents/MacOS/Claude Helper
garbage
";

    #[test]
    fn a_helper_counts_and_chrome_native_host_does_not() {
        let found = within(
            &listed(CLAUDE_LISTING),
            Bundle::Named("Claude.app"),
            &["Contents/Helpers/chrome-native-host"],
        );
        // The app, its helpers wherever in the bundle they are, and a native host the app
        // itself started; never one Chrome started, which holds nothing of Claude's folder.
        // A copy renamed in the Finder is found by the program it runs, which keeps its
        // name: its main program, or a helper left after it quit, and with either everything
        // else inside it.
        assert_eq!(
            found.iter().map(|p| p.pid).collect::<Vec<_>>(),
            [501, 502, 503, 601, 700, 704, 705]
        );
        assert_eq!(
            found[1].path,
            PathBuf::from(
                "/Applications/Claude.app/Contents/Frameworks/Claude Helper (Renderer).app/\
                 Contents/MacOS/Claude Helper (Renderer)"
            )
        );
        // With nothing excluded, every process inside the bundle counts.
        assert_eq!(
            within(&listed(CLAUDE_LISTING), Bundle::Named("Claude.app"), &[]).len(),
            8
        );
        assert!(within(&listed(CLAUDE_LISTING), Bundle::Named("ChatGPT.app"), &[]).is_empty());
    }

    /// A bundle a test or an app put somewhere of its own is asked about by that path: the
    /// person's own Claude, and any other copy, run from elsewhere and do not count. Within
    /// it the same rule holds for Chrome's native host.
    #[test]
    fn a_bundle_at_a_path_counts_only_what_runs_from_there() {
        const LISTING: &str = "\
  501     1 /Applications/Claude.app/Contents/MacOS/Claude
  502   501 /Applications/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper
  590     1 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
  800     1 /scratch/t/Claude.app/Contents/MacOS/Claude
  801   800 /scratch/t/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper
  810   590 /scratch/t/Claude.app/Contents/Helpers/chrome-native-host
  811   800 /scratch/t/Claude.app/Contents/Helpers/chrome-native-host
  820     1 /scratch/t/Claude.app.old/Contents/MacOS/Claude
  821     1 /scratch/Claude.app/Contents/MacOS/Claude
";
        let excluded = &["Contents/Helpers/chrome-native-host"];
        let pids = |bundle| {
            within(&listed(LISTING), bundle, excluded)
                .iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            pids(Bundle::At(Path::new("/scratch/t/Claude.app"))),
            [800, 801, 811]
        );
        assert!(pids(Bundle::At(Path::new("/nowhere/Claude.app"))).is_empty());
        // By name, every bundle called that counts, wherever it is.
        assert_eq!(
            pids(Bundle::Named("Claude.app")),
            [501, 502, 800, 801, 811, 821]
        );
    }

    /// Read from the machine running the tests: every test binary runs from Cargo's `deps`.
    #[test]
    fn this_users_processes_within_a_folder_can_be_read() {
        let host = super::super::current();
        assert_eq!(
            host.processes_within(Bundle::Named("definitely-not-a-bundle.app"), &[]),
            Some(Vec::new())
        );
        let found = host
            .processes_within(Bundle::Named("deps"), &[])
            .expect("readable");
        assert!(
            found.iter().any(|p| p.pid == std::process::id()),
            "{found:?}"
        );
    }

    /// Read from the machine running the tests: this test runs its own binary, and a
    /// process that has gone runs nothing.
    #[test]
    fn the_program_a_process_runs_can_be_read() {
        let host = super::super::current();
        let me = host.program_of(std::process::id()).expect("readable");
        let exe = std::env::current_exe().unwrap();
        assert_eq!(me.file_name(), exe.file_name(), "{me:?}");

        let mut gone = std::process::Command::new("true").spawn().unwrap();
        let dead = gone.id();
        gone.wait().unwrap();
        assert_eq!(host.program_of(dead), None);
        assert!(!host.pid_alive(dead));
        assert!(host.pid_alive(std::process::id()));
    }
}
