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
use crate::host::{OS, Process};
use crate::provider::ProviderId;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    // A directory of exactly this name, such as an app bundle.
    Within(&'static str),
    // An editor extension's folder is named for its version.
    WithinPrefixed(&'static str),
    // A Windows package's family, wherever the package is installed.
    Package(&'static str),
    Anywhere,
}

/// What makes a kind of holder take a switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remedy {
    /// Quit it and start it again.
    Restart,
    /// Quit the app the way its system quits an app, and open it again. Closing its windows
    /// is not enough. The menu bar app can do both for the person; the command line only
    /// says so.
    ReopenApp {
        /// The app, as its own system names it.
        app: AppId,
        /// The app as a person knows it: "ChatGPT".
        name: &'static str,
    },
    // Run the program with `arguments`, from a terminal that is not elevated where it says so.
    Run {
        arguments: &'static str,
        not_elevated: bool,
    },
    /// Do this, somewhere Pitboard cannot reach.
    Do(&'static str),
}

// Which system's id is part of the id: a program in a Mac app's bundle runs only on a Mac, so a
// test whose machine runs one has that Mac app on whatever system the test runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppId {
    MacBundle(&'static str),
    // An Application User Model ID, `<package family>!<app>`.
    WindowsAumid(&'static str),
}

impl AppId {
    pub fn as_str(self) -> &'static str {
        match self {
            AppId::MacBundle(id) | AppId::WindowsAumid(id) => id,
        }
    }

    // A Mac app stays open after its last window closes.
    fn reopen(self, name: &str) -> String {
        match self {
            AppId::MacBundle(_) => format!("quit {name} with Command-Q and open it again"),
            AppId::WindowsAumid(_) => {
                format!("quit {name} from its notification-area icon and open it again")
            }
        }
    }
}

/// The processes of one kind of holder that are running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holding {
    pub holder: Holder,
    /// Their process ids, in the order the process list gave them.
    pub pids: Vec<u32>,
    // How a terminal here is told to run the program, as `words::spelled` spells it.
    pub program: &'static str,
}

impl Location {
    // Never by the program's own name. A prefix is compared as that many first characters.
    fn holds(self, process: &Process) -> bool {
        let mut directories = process.path.ancestors().skip(1).filter_map(Path::file_name);
        match self {
            Location::Anywhere => true,
            Location::Package(family) => process
                .package
                .as_deref()
                .is_some_and(|its| its.eq_ignore_ascii_case(family)),
            Location::Within(name) => {
                directories.any(|dir| OS.same_path(Path::new(dir), Path::new(name)))
            }
            Location::WithinPrefixed(prefix) => directories.any(|dir| {
                let head: String = dir
                    .to_string_lossy()
                    .chars()
                    .take(prefix.chars().count())
                    .collect();
                OS.same_path(Path::new(&head), Path::new(prefix))
            }),
        }
    }
}

// Each to the first holder whose location holds it. A kind with nothing running is left out.
pub(crate) fn classify(
    processes: &[Process],
    holders: &[Holder],
    program: &'static str,
) -> Vec<Holding> {
    let mut holding: Vec<Holding> = holders
        .iter()
        .map(|&holder| Holding {
            holder,
            pids: Vec::new(),
            program,
        })
        .collect();
    for process in processes {
        if let Some(found) = holding
            .iter_mut()
            .find(|h| h.holder.location.holds(process))
        {
            found.pids.push(process.pid);
        }
    }
    holding.retain(|h| !h.pids.is_empty());
    holding
}

/// What is running `program` on this machine, by kind. `None` where the process list could
/// not be read, which is not the same as nothing running.
pub(crate) fn find(
    ctx: &Context,
    tool: ProviderId,
    program: &str,
    holders: &[Holder],
) -> Option<Vec<Holding>> {
    let processes = ctx.host().processes(program)?;
    // Spelled only for something running, since on Windows that searches `PATH`.
    let spelled = if processes.is_empty() {
        tool.program()
    } else {
        crate::words::spelled(ctx, tool).of(tool)
    };
    Some(classify(&processes, holders, spelled))
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
            Remedy::ReopenApp { app, name } => app.reopen(name),
            Remedy::Run {
                arguments,
                not_elevated,
            } => {
                let run = format!("run `{} {arguments}`", self.program);
                if not_elevated {
                    format!("from a terminal that is not elevated, {run}")
                } else {
                    run
                }
            }
            Remedy::Do(instruction) => instruction.to_string(),
        }
    }
}

/// Everything running, as one noun phrase: "2 `codex` sessions and the ChatGPT app".
pub(crate) fn described(holding: &[Holding]) -> String {
    crate::words::listed(holding.iter().map(Holding::phrase).collect())
}

/// The same, with the pids of each: "2 `codex` sessions (pid 41, 42)".
pub(crate) fn described_with_pids(holding: &[Holding]) -> String {
    crate::words::listed(
        holding
            .iter()
            .map(|h| format!("{} (pid {})", h.phrase(), some_of(&h.pids)))
            .collect(),
    )
}

/// Whether what is running reads as more than one thing, for "is" or "are".
pub(crate) fn plural(holding: &[Holding]) -> bool {
    holding.iter().map(Holding::things).sum::<usize>() > 1
}

/// One sentence saying what makes everything running take a switch, with `purpose`:
/// "Quit them and start again to use the new account." Several kinds are each said in turn,
/// apart, since one clause can have an "and" of its own: "To use the new account: quit the
/// `codex` sessions and start them again; quit ChatGPT with Command-Q and open it again."
pub(crate) fn remedies(holding: &[Holding], purpose: &str) -> String {
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
            app: AppId::MacBundle("com.example.app"),
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
            package: None,
        }
    }

    fn kinds(holding: &[Holding]) -> Vec<(&str, Vec<u32>)> {
        holding
            .iter()
            .map(|h| (h.holder.kind, h.pids.clone()))
            .collect()
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
            "tool",
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

    #[test]
    fn only_the_directories_a_program_is_in_say_where_it_runs() {
        let holding = classify(&[at(1, "/usr/bin/App.app")], HOLDERS, "tool");
        assert_eq!(kinds(&holding), [("session", vec![1])]);
        assert!(!Location::WithinPrefixed("vendor.tool-").holds(&at(1, "vendor.tool-9")));
    }

    #[test]
    fn a_place_is_named_as_this_system_compares_paths() {
        let holding = classify(
            &[
                at(1, "/Applications/app.app/x/tool"),
                at(2, "/e/Vendor.Tool-1/bin/tool"),
            ],
            HOLDERS,
            "tool",
        );
        match OS {
            crate::host::Os::MacOs | crate::host::Os::Linux => {
                assert_eq!(kinds(&holding), [("session", vec![1, 2])]);
            }
            crate::host::Os::Windows => {
                assert_eq!(kinds(&holding), [("app", vec![1]), ("extension", vec![2])]);
            }
        }
        assert!(Location::WithinPrefixed("vendor.tool-").holds(&at(1, "/e/vendor.tool-1/t")));
        assert!(
            !Location::WithinPrefixed("vendor.tool-").holds(&at(1, "/e/vendor.tool/t")),
            "a name shorter than the prefix does not start with it"
        );
    }

    /// A Mac app is named by its bundle id and quit with Command-Q, since closing its last
    /// window leaves it running: the id is its own system's, whichever system finds it.
    #[test]
    fn an_app_is_named_and_quit_its_own_system_s_way() {
        let mac = AppId::MacBundle("com.example.app");
        assert_eq!(mac.as_str(), "com.example.app");
        assert_eq!(
            mac.reopen("App"),
            "quit App with Command-Q and open it again"
        );
        let app = classify(&[at(1, "/Applications/App.app/x/tool")], HOLDERS, "tool");
        assert_eq!(
            app.first().map(|h| h.holder.remedy),
            Some(Remedy::ReopenApp {
                app: mac,
                name: "App"
            })
        );
    }

    // By its family, so a package installed elsewhere is still the app, and a path is not.
    #[test]
    fn a_windows_app_is_its_package_and_is_quit_from_its_notification_area_icon() {
        const STORE: Holder = Holder {
            kind: "app",
            noun: Noun::One("the App"),
            location: Location::Package("Vendor.App_0000000000000"),
            remedy: Remedy::ReopenApp {
                app: AppId::WindowsAumid("Vendor.App_0000000000000!App"),
                name: "App",
            },
        };
        let packaged = |pid, family: &str| Process {
            package: Some(family.into()),
            ..at(pid, "D:/Apps/Vendor.App/tool.exe")
        };
        let holding = classify(
            &[
                packaged(1, "vendor.app_0000000000000"),
                packaged(2, "Vendor.Other_0000000000000"),
                at(
                    3,
                    "C:/Program Files/WindowsApps/Vendor.App_1.0.0.0_x64__0000000000000/tool.exe",
                ),
            ],
            &[STORE, SESSION],
            "tool",
        );
        assert_eq!(kinds(&holding), [("app", vec![1]), ("session", vec![2, 3])]);
        assert_eq!(
            AppId::WindowsAumid("Vendor.App_0000000000000!App").as_str(),
            "Vendor.App_0000000000000!App"
        );
        assert_eq!(
            remedies(&holding[..1], "to use the new account"),
            "Quit App from its notification-area icon and open it again to use the new account."
        );
    }

    #[test]
    fn nothing_running_is_nothing_held() {
        assert!(classify(&[], HOLDERS, "tool").is_empty());
        assert!(
            classify(&[at(1, "tool")], &[APP], "tool").is_empty(),
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
            "tool",
        );
        assert_eq!(described(&holding), "the App");
        assert!(!plural(&holding));
        assert_eq!(described_with_pids(&holding), "the App (pid 7, 8)");
    }

    /// Alone, a kind's advice is what it always was. Beside others, each names what it is
    /// about, and the purpose leads.
    #[test]
    fn what_to_do_is_said_once_for_each_kind() {
        let sessions = classify(&[at(1, "tool"), at(2, "tool")], HOLDERS, "tool");
        assert_eq!(described(&sessions), "2 `tool` sessions");
        assert!(plural(&sessions));
        assert_eq!(
            remedies(&sessions, "to use the new account"),
            "Quit them and start again to use the new account."
        );

        let one = classify(&[at(1, "tool")], HOLDERS, "tool");
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
            "tool",
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
    fn a_command_is_said_as_one_with_the_program_as_spelled() {
        let said = |not_elevated, program| {
            let daemon = Holder {
                kind: "daemon",
                noun: Noun::One("the daemon"),
                location: Location::Within("daemon"),
                remedy: Remedy::Run {
                    arguments: "daemon restart",
                    not_elevated,
                },
            };
            remedies(
                &classify(&[at(1, "/x/daemon/tool")], &[daemon], program),
                "to take a switch",
            )
        };
        assert_eq!(
            said(false, "tool"),
            "Run `tool daemon restart` to take a switch."
        );
        assert_eq!(
            said(true, "tool.cmd"),
            "From a terminal that is not elevated, run `tool.cmd daemon restart` to take a \
             switch."
        );
    }

    #[test]
    fn some_pids_stand_for_many() {
        assert_eq!(some_of(&[4321, 99]), "4321, 99");
        let many: Vec<u32> = (1..=23).collect();
        assert_eq!(some_of(&many), "1, 2, 3 and 20 more");
    }
}
