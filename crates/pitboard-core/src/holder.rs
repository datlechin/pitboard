//! What keeps a tool's login in memory while it runs, and what makes each kind take a switch.
//!
//! A tool whose running sessions never notice a switch (`Adoption::RestartRequired`) names
//! the kinds of process that run its program. One program can run in very different
//! places: Codex's runs in a terminal, inside OpenAI's ChatGPT app, as a background app
//! server and inside an editor's Codex extension. Each takes a switch its own way, and
//! advice meant for one is wrong for another: quitting a terminal session does nothing for
//! an app that stays open in the menu bar after its windows close.
//!
//! The kinds are told apart by where the program runs from, which the process list says.
//! Nothing here names a tool; a tool's own list does, most particular kind first and ending
//! with one that is anywhere, so every process is one kind and none is guessed to be an app.

use crate::context::Context;
use crate::host::Process;
use std::path::Path;

/// One kind of process that holds a tool's login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holder {
    /// A stable name in snake case, for a program to tell kinds apart by.
    pub kind: &'static str,
    /// How a sentence names what is running.
    pub noun: Noun,
    /// Where its program runs from.
    pub location: Location,
    /// What makes it take a switch.
    pub remedy: Remedy,
}

/// How a sentence names one kind of holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Noun {
    /// Each process is one of them, counted: "1 `codex` session", "2 `codex` sessions".
    Counted {
        one: &'static str,
        many: &'static str,
    },
    /// One thing however many processes it runs: an app that starts two of the program is
    /// still one app.
    One(&'static str),
}

/// Where a kind of holder runs its program from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// Inside a directory of exactly this name, such as an app bundle.
    Within(&'static str),
    /// Inside a directory whose name starts with this, such as an editor extension's folder,
    /// which is named for its version.
    WithinPrefixed(&'static str),
    /// Anywhere at all: the kind every process no other kind claims is.
    Anywhere,
}

/// What makes a kind of holder take a switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remedy {
    /// Quit it and start it again.
    Restart,
    /// Quit the app the way Command-Q does, and open it again. Closing its windows is not
    /// enough. The menu bar app can do both for the person; the command line only says so.
    ReopenApp {
        bundle_id: &'static str,
        name: &'static str,
    },
    /// Run this command.
    Run(&'static str),
    /// Do this, somewhere Pitboard cannot reach.
    Do(&'static str),
}

/// The processes of one kind of holder that are running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holding {
    pub holder: Holder,
    /// Their process ids, in the order the process list gave them.
    pub pids: Vec<u32>,
}

impl Location {
    /// Whether a program at `path` runs from here. Only the directories it is in are
    /// looked at, never the program's own name.
    fn holds(self, path: &Path) -> bool {
        let mut directories = path.ancestors().skip(1).filter_map(Path::file_name);
        match self {
            Location::Anywhere => true,
            Location::Within(name) => directories.any(|dir| dir == name),
            Location::WithinPrefixed(prefix) => {
                directories.any(|dir| dir.to_string_lossy().starts_with(prefix))
            }
        }
    }
}

/// `processes` sorted into `holders`, each to the first whose location holds it, in the
/// holders' order. A kind with nothing running is left out, and so is a process no holder
/// claims, which a list ending in [`Location::Anywhere`] never leaves.
pub fn classify(processes: &[Process], holders: &[Holder]) -> Vec<Holding> {
    let mut holding: Vec<Holding> = holders
        .iter()
        .map(|&holder| Holding {
            holder,
            pids: Vec::new(),
        })
        .collect();
    for process in processes {
        if let Some(found) = holding
            .iter_mut()
            .find(|h| h.holder.location.holds(&process.path))
        {
            found.pids.push(process.pid);
        }
    }
    holding.retain(|h| !h.pids.is_empty());
    holding
}

/// What is running `program` on this machine, by kind. `None` where the process list could
/// not be read, which is not the same as nothing running.
pub(crate) fn find(ctx: &Context, program: &str, holders: &[Holder]) -> Option<Vec<Holding>> {
    ctx.host()
        .processes(program)
        .map(|processes| classify(&processes, holders))
}

/// What is running from inside a tree login's bundle, by kind. `None` where the process
/// list could not be read, which a caller about to move the login counts as running.
pub(crate) fn find_within(
    ctx: &Context,
    tree: &dyn crate::provider::TreeLogin,
) -> Option<Vec<Holding>> {
    ctx.host()
        .processes_within(tree.bundle(ctx), tree.excluded())
        .map(|processes| classify(&processes, tree.holders()))
}

/// The live process a tree login's lock in `root` names, if it has one and that process is
/// running the app's program. A lock Pitboard cannot read is an error, never "nobody". No
/// lock is `None`, which says nothing: Claude Desktop 2.19675.0 keeps no `SingletonLock`
/// (experiment E14), so for it the process list is the only sign the app is open.
///
/// An app that crashed leaves its lock behind, and the system can give its pid to anything
/// after, so a process running another program does not hold the lock. One whose program
/// cannot be told is counted, since it may.
pub(crate) fn lock_holder(
    ctx: &Context,
    tree: &dyn crate::provider::TreeLogin,
    root: &Path,
) -> std::io::Result<Option<u32>> {
    let Some(lock) = tree.singleton_lock() else {
        return Ok(None);
    };
    let Some(pid) = singleton_pid(&root.join(lock))?.filter(|&pid| ctx.host().pid_alive(pid))
    else {
        return Ok(None);
    };
    let bundle = tree.bundle(ctx);
    let reused = match (bundle.program(), ctx.host().program_of(pid)) {
        (Some(program), Some(path)) => path.file_name().is_none_or(|name| name != program),
        _ => false,
    };
    Ok((!reused).then_some(pid))
}

/// The process Chromium's `SingletonLock` at `path` names: a link whose target is
/// `<hostname>-<pid>`. `None` where there is no lock. Something there that names no
/// process is an error, never "nobody", since a lock Pitboard cannot read may still be
/// held.
fn singleton_pid(path: &Path) -> std::io::Result<Option<u32>> {
    let target = match std::fs::read_link(path) {
        Ok(target) => target,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    // A hostname can have dashes in it, and a pid cannot.
    target
        .to_str()
        .and_then(|target| target.rsplit_once('-'))
        .and_then(|(_, pid)| pid.parse::<u32>().ok())
        .filter(|&pid| pid != 0)
        .map(Some)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{} names no process", path.display()),
            )
        })
}

impl Holding {
    /// How many things are running: processes for a counted kind, one for the rest.
    fn things(&self) -> usize {
        match self.holder.noun {
            Noun::Counted { .. } => self.pids.len(),
            Noun::One(_) => 1,
        }
    }

    /// "2 `codex` sessions", "the ChatGPT app".
    pub fn phrase(&self) -> String {
        match self.holder.noun {
            Noun::Counted { one, many } => match self.pids.len() {
                1 => format!("1 {one}"),
                n => format!("{n} {many}"),
            },
            Noun::One(name) => name.to_string(),
        }
    }

    /// What to do, as a clause. Said alone it can say "it" or "them"; beside others it has
    /// to name what it is about.
    fn clause(&self, alone: bool) -> String {
        let them = if self.things() == 1 { "it" } else { "them" };
        match self.holder.remedy {
            Remedy::Restart if alone => format!("quit {them} and start again"),
            Remedy::Restart => {
                let what = match self.holder.noun {
                    Noun::Counted { one, many } => {
                        format!("the {}", if self.things() == 1 { one } else { many })
                    }
                    Noun::One(name) => name.to_string(),
                };
                format!("quit {what} and start {them} again")
            }
            Remedy::ReopenApp { name, .. } => {
                format!("quit {name} with Command-Q and open it again")
            }
            Remedy::Run(command) => format!("run `{command}`"),
            Remedy::Do(instruction) => instruction.to_string(),
        }
    }
}

/// Everything running, as one noun phrase: "2 `codex` sessions and the ChatGPT app".
pub fn described(holding: &[Holding]) -> String {
    listed(holding.iter().map(Holding::phrase).collect())
}

/// The same, with the pids of each: "2 `codex` sessions (pid 41, 42)".
pub fn described_with_pids(holding: &[Holding]) -> String {
    listed(
        holding
            .iter()
            .map(|h| format!("{} (pid {})", h.phrase(), some_of(&h.pids)))
            .collect(),
    )
}

/// Whether what is running reads as more than one thing, for "is" or "are".
pub fn plural(holding: &[Holding]) -> bool {
    holding.iter().map(Holding::things).sum::<usize>() > 1
}

/// One sentence saying what makes everything running take a switch, with `purpose`:
/// "Quit them and start again to use the new account." Several kinds are each said in turn,
/// apart, since one clause can have an "and" of its own: "To use the new account: quit the
/// `codex` sessions and start them again; quit ChatGPT with Command-Q and open it again."
pub fn remedies(holding: &[Holding], purpose: &str) -> String {
    match holding {
        [] => String::new(),
        [one] => format!("{} {purpose}.", capitalised(&one.clause(true))),
        many => format!(
            "{}: {}.",
            capitalised(purpose),
            many.iter()
                .map(|h| h.clause(false))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }
}

/// "a", "a and b", "a, b and c".
fn listed(mut items: Vec<String>) -> String {
    match items.len() {
        0 => String::new(),
        1 => items.remove(0),
        _ => {
            let last = items.pop().unwrap_or_default();
            format!("{} and {last}", items.join(", "))
        }
    }
}

/// `text` with its first letter a capital, to begin a sentence with.
pub(crate) fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A few pids and how many more, because somebody with twenty sessions open needs to know
/// there are twenty, not which twenty.
pub(crate) fn some_of(pids: &[u32]) -> String {
    const SHOWN: usize = 3;
    let named: Vec<String> = pids.iter().take(SHOWN).map(u32::to_string).collect();
    match pids.len().saturating_sub(SHOWN) {
        0 => named.join(", "),
        more => format!("{} and {more} more", named.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const APP: Holder = Holder {
        kind: "app",
        noun: Noun::One("the App"),
        location: Location::Within("App.app"),
        remedy: Remedy::ReopenApp {
            bundle_id: "com.example.app",
            name: "App",
        },
    };
    const EXTENSION: Holder = Holder {
        kind: "extension",
        noun: Noun::One("the extension"),
        location: Location::WithinPrefixed("vendor.tool-"),
        remedy: Remedy::Do("reload the editor's window"),
    };
    const SESSION: Holder = Holder {
        kind: "session",
        noun: Noun::Counted {
            one: "`tool` session",
            many: "`tool` sessions",
        },
        location: Location::Anywhere,
        remedy: Remedy::Restart,
    };
    const HOLDERS: &[Holder] = &[APP, EXTENSION, SESSION];

    fn at(pid: u32, path: &str) -> Process {
        Process {
            pid,
            path: PathBuf::from(path),
        }
    }

    fn kinds(holding: &[Holding]) -> Vec<(&str, Vec<u32>)> {
        holding
            .iter()
            .map(|h| (h.holder.kind, h.pids.clone()))
            .collect()
    }

    /// A test or an app that moved both Claude's bundle and its data folder is asked about
    /// its own bundle only, whatever it is called: the person's own Claude holds nothing of
    /// a folder elsewhere. Moving only one of them leaves the real folder or the real app in
    /// play, so every bundle called `Claude.app` still counts.
    #[test]
    fn a_dead_singleton_lock_does_not_count() {
        let host = crate::host::current();
        let dir = std::env::temp_dir().join(format!(
            "pitboard-singleton-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let lock = dir.join("SingletonLock");
        assert_eq!(singleton_pid(&lock).unwrap(), None, "no lock, nobody");

        let mut gone = std::process::Command::new("true").spawn().unwrap();
        let dead = gone.id();
        gone.wait().unwrap();
        std::os::unix::fs::symlink(format!("my-mac.local-{dead}"), &lock).unwrap();
        assert_eq!(singleton_pid(&lock).unwrap(), Some(dead));
        assert!(!host.pid_alive(dead));

        std::fs::remove_file(&lock).unwrap();
        std::os::unix::fs::symlink(format!("my-mac-2-{}", std::process::id()), &lock).unwrap();
        assert_eq!(singleton_pid(&lock).unwrap(), Some(std::process::id()));
        assert!(host.pid_alive(std::process::id()));

        // Something there that names no process is not read as nobody.
        for target in ["nonsense", "host-", "host-12x"] {
            std::fs::remove_file(&lock).unwrap();
            std::os::unix::fs::symlink(target, &lock).unwrap();
            assert!(singleton_pid(&lock).is_err(), "{target}");
        }
        std::fs::remove_file(&lock).unwrap();
        std::fs::write(&lock, b"").unwrap();
        assert!(singleton_pid(&lock).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_moved_bundle_and_folder_are_held_only_by_that_bundle() {
        use crate::host::memory::MemoryHost;
        use crate::provider::desktop::DESKTOP;
        let mem = MemoryHost::new();
        let real = mem.runs_within("/Applications/Claude.app/Contents/MacOS/Claude");
        let base = Context::new(PathBuf::from("/nowhere")).with_memory_stores(mem.clone());
        for ctx in [
            base.clone(),
            base.clone().with_desktop_app("/scratch/Claude.app".into()),
            base.clone()
                .with_desktop_dir("/scratch/claude-desktop".into()),
        ] {
            let found = find_within(&ctx, &DESKTOP).expect("readable");
            assert_eq!(kinds(&found), [("claude_desktop_app", vec![real])]);
        }

        let moved = base
            .with_desktop_dir("/scratch/claude-desktop".into())
            .with_desktop_app("/scratch/Test Claude.app".into());
        assert_eq!(find_within(&moved, &DESKTOP), Some(Vec::new()));
        let own = mem.runs_within("/scratch/Test Claude.app/Contents/MacOS/Claude");
        mem.runs_within("/scratch/Test Claude.app/Contents/Helpers/chrome-native-host");
        let found = find_within(&moved, &DESKTOP).expect("readable");
        assert_eq!(kinds(&found), [("claude_desktop_app", vec![own])]);
    }

    /// A tree login is held by whatever runs inside its bundle, and by a live process its
    /// lock names, and by nothing else.
    #[test]
    fn a_tree_login_is_held_by_its_bundle_and_its_lock() {
        use crate::host::memory::MemoryHost;
        use crate::provider::desktop::DESKTOP;
        let mem = MemoryHost::new();
        let ctx = Context::new(PathBuf::from("/nowhere")).with_memory_stores(mem.clone());
        assert_eq!(find_within(&ctx, &DESKTOP), Some(Vec::new()));

        let app = mem.runs_within("/Applications/Claude.app/Contents/MacOS/Claude");
        mem.runs_within("/Applications/Claude.app/Contents/Helpers/chrome-native-host");
        mem.runs_within("/usr/local/bin/claude");
        let found = find_within(&ctx, &DESKTOP).expect("readable");
        assert_eq!(kinds(&found), [("claude_desktop_app", vec![app])]);

        mem.process_list_fails();
        assert_eq!(find_within(&ctx, &DESKTOP), None);

        let dir = std::env::temp_dir().join(format!(
            "pitboard-lock-holder-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(lock_holder(&ctx, &DESKTOP, &dir).unwrap(), None);
        std::os::unix::fs::symlink(format!("mac-{app}"), dir.join("SingletonLock")).unwrap();
        assert_eq!(lock_holder(&ctx, &DESKTOP, &dir).unwrap(), Some(app));
        std::fs::remove_file(dir.join("SingletonLock")).unwrap();
        std::os::unix::fs::symlink("mac-4000000", dir.join("SingletonLock")).unwrap();
        assert_eq!(lock_holder(&ctx, &DESKTOP, &dir).unwrap(), None);

        // A Claude that crashed leaves its lock, and its pid can be given to anything after.
        // Only a process running Claude's own program holds the lock.
        mem.runs_at("zsh", &["/bin/zsh"]);
        std::fs::remove_file(dir.join("SingletonLock")).unwrap();
        std::os::unix::fs::symlink("mac-1", dir.join("SingletonLock")).unwrap();
        assert_eq!(
            lock_holder(&ctx, &DESKTOP, &dir).unwrap(),
            None,
            "pid reused"
        );
        // A Claude whose bundle is not where Pitboard looks still holds it.
        mem.runs_at("zsh", &[]);
        mem.runs_at(
            "Claude",
            &["/Volumes/Apps/Claude Beta.app/Contents/MacOS/Claude"],
        );
        assert_eq!(lock_holder(&ctx, &DESKTOP, &dir).unwrap(), Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_process_is_the_first_kind_whose_place_holds_it() {
        let holding = classify(
            &[
                at(1, "tool"),
                at(
                    2,
                    "/Applications/App.app/Contents/Helpers/Tool.app/Contents/MacOS/tool",
                ),
                at(3, "/home/a/.editor/extensions/vendor.tool-1.2.3/bin/tool"),
                at(4, "/usr/local/bin/tool"),
            ],
            HOLDERS,
        );
        assert_eq!(
            kinds(&holding),
            [
                ("app", vec![2]),
                ("extension", vec![3]),
                ("session", vec![1, 4])
            ]
        );
    }

    /// The program's own name is not where it runs from: a program that happens to be
    /// called like a place is still wherever it is.
    #[test]
    fn only_the_directories_a_program_is_in_say_where_it_runs() {
        let holding = classify(&[at(1, "/usr/bin/App.app")], HOLDERS);
        assert_eq!(kinds(&holding), [("session", vec![1])]);
        assert!(!Location::WithinPrefixed("vendor.tool-").holds(Path::new("vendor.tool-9")));
    }

    #[test]
    fn nothing_running_is_nothing_held() {
        assert!(classify(&[], HOLDERS).is_empty());
        assert!(
            classify(&[at(1, "tool")], &[APP]).is_empty(),
            "and a process no kind claims is left out"
        );
    }

    #[test]
    fn an_app_is_one_thing_however_many_processes_it_runs() {
        let holding = classify(
            &[
                at(7, "/Applications/App.app/x/tool"),
                at(8, "/Applications/App.app/y/tool"),
            ],
            HOLDERS,
        );
        assert_eq!(described(&holding), "the App");
        assert!(!plural(&holding));
        assert_eq!(described_with_pids(&holding), "the App (pid 7, 8)");
    }

    /// Alone, a kind's advice is what it always was. Beside others, each names what it is
    /// about, and the purpose leads.
    #[test]
    fn what_to_do_is_said_once_for_each_kind() {
        let sessions = classify(&[at(1, "tool"), at(2, "tool")], HOLDERS);
        assert_eq!(described(&sessions), "2 `tool` sessions");
        assert!(plural(&sessions));
        assert_eq!(
            remedies(&sessions, "to use the new account"),
            "Quit them and start again to use the new account."
        );

        let one = classify(&[at(1, "tool")], HOLDERS);
        assert_eq!(described(&one), "1 `tool` session");
        assert_eq!(
            remedies(&one, "to go on"),
            "Quit it and start again to go on."
        );

        let all = classify(
            &[
                at(1, "tool"),
                at(2, "/Applications/App.app/x/tool"),
                at(3, "/e/vendor.tool-1/bin/tool"),
            ],
            HOLDERS,
        );
        assert_eq!(
            described(&all),
            "the App, the extension and 1 `tool` session"
        );
        assert_eq!(
            remedies(&all, "to use the new account"),
            "To use the new account: quit App with Command-Q and open it again; reload the \
             editor's window; quit the `tool` session and start it again."
        );
    }

    #[test]
    fn a_command_is_said_as_one() {
        let holding = classify(
            &[at(1, "/x/daemon/tool")],
            &[Holder {
                kind: "daemon",
                noun: Noun::One("the daemon"),
                location: Location::Within("daemon"),
                remedy: Remedy::Run("tool daemon restart"),
            }],
        );
        assert_eq!(
            remedies(&holding, "to take a switch"),
            "Run `tool daemon restart` to take a switch."
        );
    }

    #[test]
    fn some_pids_stand_for_many() {
        assert_eq!(some_of(&[4321, 99]), "4321, 99");
        let many: Vec<u32> = (1..=23).collect();
        assert_eq!(some_of(&many), "1, 2, 3 and 20 more");
    }
}
