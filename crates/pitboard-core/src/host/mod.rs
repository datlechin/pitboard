//! The machine Pitboard runs on, behind one seam.
//!
//! Everything that differs between operating systems is answered here, so the rest of the
//! crate asks a question and never which system it is on. The seam has two faces.
//!
//! [`Host`] is the part a test replaces: the stores secrets and logins live in, the process
//! list and the scheduler, reached through the [`Context`] every call carries. A test puts
//! [`memory::MemoryHost`] there and can make any of them fail.
//!
//! [`fs`], [`proc`] and [`user`] are plain functions for what the machine does the same way
//! whoever asks, which the tests run for real: creating a file only its owner can reach,
//! asking whether a process is still alive, naming the person signed in. [`login_path`] is
//! one too: the `PATH` the person's login shell builds, which an app the system started
//! does not have.
//!
//! The system is chosen once, in this file, and nowhere else. A fact that differs by system
//! is a `match` on [`OS`], and [`Os`] lists every system Pitboard runs on, so a system added
//! there does not compile until each such fact has been said for it. This replaced branches
//! that read "macOS, or else Linux", which compiled anywhere and did the Linux thing.

use crate::context::{Context, Environment};
use crate::error::Result;
use crate::provider::desktop::safe_storage::SafeStorage;
use crate::provider::desktop::types::CookieTable;
use crate::store::RawStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod bundle;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(test, feature = "test-support"))]
pub mod memory;
pub(crate) mod program;
#[cfg(unix)]
mod unix;

#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
use macos as os;

pub(crate) use bundle::Bundle;
pub(crate) use os::{fs, proc, user};

/// The operating systems Pitboard runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
}

/// The system this build runs on.
pub const OS: Os = os::OS;

/// The only program trusted to read Claude Code's keychain item. Named here so the backend
/// that runs it and the doctor check that looks for it cannot drift apart.
pub const SECURITY: &str = "/usr/bin/security";

impl Os {
    /// The program Pitboard reaches the system's own store of secrets through, where it goes
    /// through one rather than calling the system directly.
    pub fn secrets_tool(self) -> Option<&'static str> {
        match self {
            Os::MacOs => Some(SECURITY),
            Os::Linux => None,
        }
    }

    /// The command a person types on this system to make `paths` private to themselves.
    pub fn make_private_command(self, kind: Kind, paths: &[&str]) -> String {
        match self {
            Os::MacOs | Os::Linux => {
                let mode = match kind {
                    Kind::Directory => "700",
                    Kind::File => "600",
                    Kind::Any => "go-rwx",
                };
                format!("chmod {mode} {}", paths.join(" "))
            }
        }
    }

    /// The folders of a home, relative to it, that the system asks the person about before
    /// an app may look inside them. An app passes over a directory of `PATH` in one: looking
    /// would put that question to the person for something Pitboard never needed, and a
    /// program started from there would have it asked on its behalf.
    ///
    /// macOS asks for the Desktop, Documents and Downloads folders, iCloud Drive and the
    /// folders of other cloud storage, under Privacy & Security's Files & Folders. Linux
    /// asks nothing.
    pub fn guarded_folders(self) -> &'static [&'static str] {
        match self {
            Os::MacOs => &[
                "Desktop",
                "Documents",
                "Downloads",
                "Library/Mobile Documents",
                "Library/CloudStorage",
            ],
            Os::Linux => &[],
        }
    }

    /// Where the system's package managers put the programs they install, for an app that
    /// has no shell's `PATH` to find one on, after each tool's own installer's place.
    ///
    /// On a Mac, Homebrew's `bin` on Apple silicon and on Intel. npm's global `bin` is one of
    /// them where Homebrew installed Node, and `/usr/local/bin` where nodejs.org's installer
    /// did, so a global npm install of a tool is found there too: Claude Code 2.1.289 lists
    /// both among npm's places. Nothing on Linux looks, since no app runs there and the
    /// command line has a shell's `PATH`, so none is said for it.
    pub fn package_bins(self) -> &'static [&'static str] {
        match self {
            Os::MacOs => &["/opt/homebrew/bin", "/usr/local/bin"],
            Os::Linux => &[],
        }
    }

    /// Where Claude Desktop keeps its data folder, relative to the home: Chromium's place
    /// for an app called Claude. Claude Desktop does not run on Linux, so there it has none.
    pub fn claude_desktop_data(self) -> Option<&'static str> {
        match self {
            Os::MacOs => Some("Library/Application Support/Claude"),
            Os::Linux => None,
        }
    }

    /// Where the Claude app is installed unless somebody says otherwise. Claude Desktop does
    /// not run on Linux, so there it has none.
    pub fn claude_desktop_app(self) -> Option<&'static str> {
        match self {
            Os::MacOs => Some("/Applications/Claude.app"),
            Os::Linux => None,
        }
    }

    /// The program the Claude app at `app` starts, which is what a process list shows for
    /// it: a Mac app's `Contents/MacOS/Claude`, named for the app and not for its bundle, so
    /// a bundle renamed in the Finder still has it there. No Claude app runs on Linux.
    pub fn claude_desktop_program(self, app: &Path) -> Option<PathBuf> {
        match self {
            Os::MacOs => Some(app.join("Contents/MacOS/Claude")),
            Os::Linux => None,
        }
    }

    /// The command line an app at `app` comes with, which is what its renewal schedule runs:
    /// the app itself is not one. A Mac app carries it at `Contents/Helpers/pitboard`, where
    /// `build-app.sh` puts it, and anything that is not an app bundle, such as a test or a
    /// build directory, has none. No app runs on Linux.
    pub fn app_command_line(self, app: &Path) -> Option<PathBuf> {
        match self {
            Os::MacOs => (app.extension() == Some("app".as_ref()))
                .then(|| app.join("Contents/Helpers/pitboard")),
            Os::Linux => None,
        }
    }
}

/// What the person's login shell said its `PATH` is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoginPath {
    /// It said this.
    Said(String),
    /// It could not be asked, or did not say. Asking again would get the same.
    Unknown,
    /// It had not answered in time, and was stopped. Startup files are slowest while the
    /// machine is busy, which is when an app that opens at login first asks, so this is not
    /// the last word: a later ask may find what this one could not.
    Late,
}

/// The `PATH` the person's own terminal has, which an app the system started does not: asked
/// of their login shell where the system has one, which can take seconds, so never on an
/// app's main thread. A system without a login shell answers with the environment's own.
pub(crate) fn login_path(env: &Environment) -> LoginPath {
    os::login_path(env)
}

/// Who besides a file's owner can reach it, as the system says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    /// Whether anybody but the owner can read or change it.
    pub shared: bool,
    /// What the system says about it, for a person to read: `mode 600` where access is a
    /// mode.
    pub described: String,
}

/// What a person makes private with the command [`Os::make_private_command`] gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Directory,
    File,
    /// Directories and files alike.
    Any,
}

/// One process this user is running, and where its program runs from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    /// The program's path where the system says one, or its bare name where it does not.
    /// macOS gives what the program was started as, which is its full path when whatever
    /// started it named one; Linux gives the file it runs, where this user may read that.
    pub path: PathBuf,
}

/// The machine Pitboard is standing on, as one value rather than a set of `cfg` branches
/// spread through the crate. A host answers where another program's secrets may be, where
/// Pitboard's own parked logins go, what this user is running and how daily renewal is
/// scheduled. It takes the context on every call because a context is built by a builder
/// and can still change after it exists.
///
/// It was called `Platform` until a second provider was on the way. The name said "which
/// operating system", the body reached into Claude Code's own slot hashing, and once
/// "provider" became a word this codebase uses, a reader meeting `Platform` could not tell
/// which of the two axes it meant. What stays behind this name is the machine, and only the
/// machine: which item or file a tool keeps its login in is the tool's to say.
pub(crate) trait Host: Send + Sync + std::fmt::Debug {
    /// Secrets another program keeps in the system's own store of them, under `account`:
    /// the login keychain on macOS. `None` where the system has no such store, and a tool
    /// keeps its login in a file instead.
    ///
    /// Which items and which account is the other program's business, so both are handed
    /// in. Deriving them here is how Claude Code's slot hashing came to live inside what
    /// claimed to be an operating-system abstraction.
    fn foreign_secrets(&self, ctx: &Context, account: &str) -> Option<Box<dyn RawStore>>;

    /// The single file at `path`, as a store.
    fn file(&self, path: PathBuf) -> Box<dyn RawStore>;

    /// Where Pitboard's own parked logins go: the system's store of secrets where there is
    /// one, a private directory of files where there is not. This one really is a fact
    /// about the machine.
    fn vault(&self, ctx: &Context) -> Box<dyn RawStore>;

    /// Whether every `PITBOARD_HOME` on this machine parks its logins in the one vault. A
    /// keychain belongs to the whole login session, so a park in it that one home cannot
    /// account for may be another home's; a vault of files lives inside its home, and
    /// nothing in it can be anybody else's.
    fn vault_is_shared(&self) -> bool;

    /// The processes this user is running `program` in, with where each runs from. `None`
    /// where the process list could not be read, which is not the same as none running.
    ///
    /// What is compared is the program's own name, so a script or a shell that merely
    /// mentions the program is not counted. Only this user's: another user's sessions use
    /// another user's login, which a switch here never touches.
    fn processes(&self, program: &str) -> Option<Vec<Process>>;

    /// Every process this user is running from inside `bundle`, wherever in it the program
    /// is, except those at a path in `excluded` (relative to the bundle) that the bundle's
    /// own processes did not start. `None` where the process list could not be read, which
    /// is not the same as none running.
    fn processes_within(&self, bundle: Bundle<'_>, excluded: &[&str]) -> Option<Vec<Process>>;

    /// Whether process `pid` may be running. One this user may not signal is counted, since
    /// it is there.
    fn pid_alive(&self, pid: u32) -> bool;

    /// The program process `pid` runs: its full path where the system says one. `None`
    /// where that cannot be told, as for a process that has gone.
    fn program_of(&self, pid: u32) -> Option<PathBuf>;

    /// The device the file at `path` is on, so two places can be told to share a volume.
    fn device_of(&self, path: &Path) -> std::io::Result<u64>;

    /// What Chromium's cookie database at `path` holds for claude.ai, read without a lock.
    /// A jar that is not there is `NotFound`; one mid-write is `ResourceBusy`.
    fn cookie_table(&self, path: &Path) -> std::io::Result<CookieTable>;

    /// The version an app bundle at `app` says it is. `None` where there is no app there or
    /// it cannot be read.
    fn bundle_version(&self, app: &Path) -> Option<String>;

    /// The system's own scheduler, which runs daily renewal. `None` where there is none
    /// Pitboard knows how to ask.
    fn scheduler(&self) -> Option<&dyn Scheduler>;
}

/// The system's own scheduler, which starts `pitboard renew` once a day: launchd on macOS,
/// a systemd user timer on Linux. Never a homemade daemon.
///
/// What Pitboard decides about the schedule, which program it runs, how often, and whether
/// it is this home's to change, is [`crate::schedule`]'s. This is only how the system is
/// asked, and what it says back.
pub(crate) trait Scheduler: Send + Sync + std::fmt::Debug {
    /// Where the schedule is kept, where a person would look for it.
    fn location(&self, ctx: &Context) -> PathBuf;

    /// Whether a schedule is there.
    fn installed(&self, ctx: &Context) -> bool;

    /// The Pitboard the schedule runs, read back from what [`Scheduler::put`] wrote. `None`
    /// where nothing is installed, and where what is there does not name one the way `put`
    /// writes it.
    fn program(&self, ctx: &Context) -> Option<PathBuf>;

    /// Schedule `program renew`, and ask the system to start it.
    ///
    /// Where the system will not start it, what was there goes back as it was, and is
    /// started again. The status, doctor and the app all read what is there, so a schedule
    /// left written that nothing runs would say renewal is on while it is not, which is the
    /// failure nobody would notice until the parked logins had run out.
    fn put(&self, ctx: &Context, program: &Path) -> Result<()>;

    /// Stop the schedule and take it away. `false` when nothing was there.
    fn remove(&self, ctx: &Context) -> Result<bool>;

    /// Whether this process is a run the schedule itself started. `said` is the job a test
    /// says this process runs as; `None` leaves it to what the system says.
    fn started_this_process(&self, said: Option<&str>) -> bool;
}

/// The host this build is standing on, which is what every real context uses.
pub(crate) fn current() -> Arc<dyn Host> {
    os::host()
}

/// Claude Desktop's key, which every real context reads through: the keychain on macOS. A
/// system Claude Desktop does not run on has no key to read.
pub(crate) fn safe_storage() -> Arc<dyn SafeStorage> {
    os::safe_storage()
}

/// The path this program was started by, where that lasts longer than the file it runs.
pub(crate) fn current_program() -> std::io::Result<PathBuf> {
    os::current_program()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude Desktop runs on a Mac only: there it has a place and a program, and Linux
    /// names neither, even for a bundle somebody named.
    #[test]
    fn claude_desktop_is_said_for_each_system() {
        assert_eq!(
            Os::MacOs.claude_desktop_app(),
            Some("/Applications/Claude.app")
        );
        assert_eq!(
            Os::MacOs.claude_desktop_program(Path::new("/scratch/Test Claude.app")),
            Some(PathBuf::from(
                "/scratch/Test Claude.app/Contents/MacOS/Claude"
            ))
        );
        assert_eq!(Os::Linux.claude_desktop_app(), None);
        assert_eq!(
            Os::Linux.claude_desktop_program(Path::new("/scratch/Claude.app")),
            None
        );
    }

    /// A machine without a store of secrets must say so rather than hand back something
    /// that behaves like one. A tool's own module builds its chain out of this answer, so a
    /// host that always offered one would build a chain that cannot work.
    #[test]
    fn a_store_of_secrets_is_offered_only_where_there_is_one() {
        let ctx = Context::for_unit_test();
        let offered = ctx.host().foreign_secrets(&ctx, "someone");
        assert_eq!(
            offered.map(|k| k.kind()),
            match OS {
                Os::MacOs => Some(crate::store::Backend::Keychain),
                Os::Linux => None,
            }
        );
        assert_eq!(
            ctx.host().file(PathBuf::from("/nowhere/at/all")).kind(),
            crate::store::Backend::File
        );
    }

    /// Every system Pitboard runs on has a scheduler of its own that Pitboard writes for.
    #[test]
    fn this_system_has_a_scheduler() {
        assert!(current().scheduler().is_some());
    }
}
