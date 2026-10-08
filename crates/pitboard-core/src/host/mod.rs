//! The machine Pitboard runs on, behind one seam.
//!
//! Everything that differs between operating systems is answered here, so the rest of the
//! crate asks a question and never which system it is on. The seam has two faces.
//!
//! [`Host`] is the part a test replaces: the stores secrets and logins live in, the process
//! list, the scheduler and whether this process runs as the person, reached through the
//! [`Context`] every call carries. A test puts [`memory::MemoryHost`] there and can make any
//! of them fail, or say this process runs as root.
//!
//! [`fs`], [`proc`] and [`user`] are plain functions for what the machine does the same way
//! whoever asks, which the tests run for real: creating a file only its owner can reach, and
//! every other change to the disk, each taking the [`Permit`] the one gate every change
//! passes makes; asking whether a process is still alive; naming the person signed in.
//! [`login_path`] is one too: the `PATH` the person's login shell builds, which an app the
//! system started does not have.
//!
//! The system is chosen once, in this file, and nowhere else. A fact that differs by system
//! is a `match` on [`OS`], and [`Os`] lists every system Pitboard runs on, so a system added
//! there does not compile until each such fact has been said for it. This replaced branches
//! that read "macOS, or else Linux", which compiled anywhere and did the Linux thing.

use crate::context::{Context, Environment};
use crate::error::Result;
use crate::service::Permit;
use crate::store::RawStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) mod administered;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(test, feature = "test-support"))]
pub mod memory;
pub(crate) mod program;
pub(crate) mod token;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
use macos as os;
#[cfg(windows)]
use windows as os;

pub(crate) use administered::Administered;
pub(crate) use os::{fs, proc, user};

/// The operating systems Pitboard runs on. Windows only once it is released
/// ([`crate::release`]): until then a Windows build of a release refuses everything, and its
/// face, `host/windows`, refuses whatever is not built yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
    Windows,
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
            // Credential Manager is called directly, through Windows' own API.
            Os::Linux | Os::Windows => None,
        }
    }

    /// The command a person types on this system to make `paths` private to themselves.
    /// `None` where Pitboard cannot say one yet: on Windows, where a file is private through
    /// its access control list, W15 says the command along with who may read a file, and
    /// until then nothing asks, since the Windows face cannot tell who may.
    pub fn make_private_command(self, kind: Kind, paths: &[&str]) -> Option<String> {
        match self {
            Os::MacOs | Os::Linux => {
                let mode = match kind {
                    Kind::Directory => "700",
                    Kind::File => "600",
                    Kind::Any => "go-rwx",
                };
                Some(format!("chmod {mode} {}", paths.join(" ")))
            }
            Os::Windows => None,
        }
    }

    /// The folders of a home, relative to it, that the system asks the person about before
    /// an app may look inside them. An app passes over a directory of `PATH` in one: looking
    /// would put that question to the person for something Pitboard never needed, and a
    /// program started from there would have it asked on its behalf.
    ///
    /// macOS asks for the Desktop, Documents and Downloads folders, iCloud Drive and the
    /// folders of other cloud storage, under Privacy & Security's Files & Folders. Linux
    /// asks nothing. Which folders Windows asks about, if any, is W17's to say, with where an
    /// app finds a program there; until then none is passed over.
    pub fn guarded_folders(self) -> &'static [&'static str] {
        match self {
            Os::MacOs => &[
                "Desktop",
                "Documents",
                "Downloads",
                "Library/Mobile Documents",
                "Library/CloudStorage",
            ],
            Os::Linux | Os::Windows => &[],
        }
    }

    /// Where the system's package managers put the programs they install, for an app that
    /// has no shell's `PATH` to find one on, after each tool's own installer's place.
    ///
    /// On a Mac, Homebrew's `bin` on Apple silicon and on Intel. npm's global `bin` is one of
    /// them where Homebrew installed Node, and `/usr/local/bin` where nodejs.org's installer
    /// did, so a global npm install of a tool is found there too: Claude Code 2.1.289 lists
    /// both among npm's places. Nothing on Linux looks, since no app runs there and the
    /// command line has a shell's `PATH`, so none is said for it. Where winget, Scoop and npm
    /// put programs on Windows is W17's to read; until then none is looked in.
    pub fn package_bins(self) -> &'static [&'static str] {
        match self {
            Os::MacOs => &["/opt/homebrew/bin", "/usr/local/bin"],
            Os::Linux | Os::Windows => &[],
        }
    }

    /// Whether `a` and `b` are one path, as Pitboard compares the places a program runs
    /// from: component by component, exactly on macOS and Linux, and in any case on Windows,
    /// whose file systems take `Codex.exe` and `codex.exe` for one file. Their process lists
    /// give a program's path as it was started or as the file system has it, and the places a
    /// tool's holders name are named by their makers in one case. Every comparison of where
    /// a program runs from goes through this, so a system whose paths ignore case says so
    /// once.
    pub(crate) fn same_path(self, a: &Path, b: &Path) -> bool {
        match self {
            Os::MacOs | Os::Linux => a == b,
            Os::Windows => same_path_in_any_case(a, b),
        }
    }

    /// The command line an app at `app` comes with, which is what its renewal schedule runs:
    /// the app itself is not one. A Mac app carries it at `Contents/Helpers/pitboard`, where
    /// `build-app.sh` puts it, and anything that is not an app bundle, such as a test or a
    /// build directory, has none. No app runs on Linux. Where the Windows app carries its
    /// own is the app's to say, which it does not yet.
    pub fn app_command_line(self, app: &Path) -> Option<PathBuf> {
        match self {
            Os::MacOs => (app.extension() == Some("app".as_ref()))
                .then(|| app.join("Contents/Helpers/pitboard")),
            Os::Linux | Os::Windows => None,
        }
    }
}

/// Whether `a` and `b` are one path as Windows compares them: component by component, each
/// name in any case ([`same_name_in_any_case`]). Said once, for every comparison of paths on
/// Windows, the core's and an app's.
pub fn same_path_in_any_case(a: &Path, b: &Path) -> bool {
    let (mut a, mut b) = (a.components(), b.components());
    loop {
        match (a.next(), b.next()) {
            (None, None) => return true,
            (Some(one), Some(other))
                if same_name_in_any_case(one.as_os_str(), other.as_os_str()) => {}
            _ => return false,
        }
    }
}

/// Whether two names are one as Windows compares a file's name: each character in its upper
/// case, one for one, and no other change, so `é` written as one character and as `e` with an
/// accent are two names. A character whose upper case is more than one, such as `ß`, and one
/// outside the Basic Multilingual Plane, which Windows compares a UTF-16 unit at a time, are
/// compared as they are.
fn same_name_in_any_case(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    fn upper(c: char) -> char {
        if u32::from(c) > 0xFFFF {
            return c;
        }
        let mut upper = c.to_uppercase();
        match (upper.next(), upper.next()) {
            (Some(one), None) => one,
            _ => c,
        }
    }
    let (a, b) = (a.to_string_lossy(), b.to_string_lossy());
    a.chars().map(upper).eq(b.chars().map(upper))
}

/// What the person's login shell said its `PATH` is.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    windows,
    allow(
        dead_code,
        reason = "only a login shell says or is late, and Windows has none; W17 says which \
                  `PATH` an app finds programs on there"
    )
)]
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
/// app's main thread. Windows has no login shell, and says nothing until W17 reads which
/// `PATH` an app finds programs on there.
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

/// Whether this process runs as the person themselves, as the system says. Pitboard changes
/// nothing unless it is [`Elevation::Normal`]: what a run with more rights than the person's
/// own writes is not theirs, in their own home and their own keychain, and their next
/// ordinary run may not be able to read it, replace it or take it away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevation {
    /// As the person themselves.
    Normal,
    /// With rights that are not the person's own. `why` says how, in words that follow
    /// "Pitboard runs", such as `as root`, `with sudo` or `as administrator`.
    Elevated { why: &'static str },
    /// The system could not say, which Pitboard takes as a reason to change nothing.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Floor {
    Met,
    Below { build: u32 },
    Unknown,
}

// Windows 11 24H2's build, which Windows Server 2025 shares.
pub const WINDOWS_FLOOR: u32 = 26100;

#[cfg(any(windows, test))]
pub(crate) fn windows_floor(version: Option<(u32, u32, u32)>) -> Floor {
    match version {
        None => Floor::Unknown,
        Some((major, minor, build)) if (major, minor, build) >= (10, 0, WINDOWS_FLOOR) => {
            Floor::Met
        }
        Some((_, _, build)) => Floor::Below { build },
    }
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

    /// The system's own scheduler, which runs daily renewal. `None` where there is none
    /// Pitboard knows how to ask.
    fn scheduler(&self) -> Option<&dyn Scheduler>;

    /// Whether this process runs as the person themselves, which is what the one gate every
    /// change passes asks ([`crate::service::Permit`]). Takes the context because part of
    /// the answer can be in the environment this process was started with, such as the
    /// `SUDO_UID` sudo sets.
    fn elevation(&self, ctx: &Context) -> Elevation;

    fn floor(&self) -> Floor;

    /// The file at `path`, outside every home, that only an administrator writes, such as
    /// Codex's `/etc/codex/requirements.toml`. Which file is the tool's to say.
    ///
    /// Unset in a build for tests, on the real hosts ([`administered::READ_BY_REAL_HOSTS`]).
    fn administered_file(&self, path: &Path) -> Administered {
        if administered::READ_BY_REAL_HOSTS {
            administered::read_text(path)
        } else {
            Administered::Unset
        }
    }

    /// The text an administrator's configuration profile forces `key` of the program whose
    /// preferences are `domain` to: a managed preference, on macOS. Unset where nothing
    /// forces it, whoever set it otherwise, and on a system with no managed preferences.
    ///
    /// Unset in a build for tests, on the real hosts ([`administered::READ_BY_REAL_HOSTS`]).
    fn managed_preference(&self, domain: &str, key: &str) -> Administered;
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

    /// The variables the schedule's job is given, by name, as [`Scheduler::put`] wrote them,
    /// read back. Empty where nothing is installed, and where what is there gives none the way
    /// `put` writes them. The system can give a job more of its own, which this does not say.
    fn environment(&self, ctx: &Context) -> Vec<(String, String)>;

    /// Schedule `program renew`, given `environment`, each variable by name, besides what the
    /// system gives every job, and ask the system to start it. What is written can hold a
    /// proxy's password, so only the person can read it, whatever was there before.
    ///
    /// Where the system will not start it, what was there goes back as it was, and is
    /// started again. The status, doctor and the app all read what is there, so a schedule
    /// left written that nothing runs would say renewal is on while it is not, which is the
    /// failure nobody would notice until the parked logins had run out.
    fn put(
        &self,
        ctx: &Context,
        permit: Permit,
        program: &Path,
        environment: &[(String, String)],
    ) -> Result<()>;

    /// Stop the schedule and take it away. `false` when nothing was there.
    fn remove(&self, ctx: &Context, permit: Permit) -> Result<bool>;

    /// Whether this process is a run the schedule itself started. `said` is the job a test
    /// says this process runs as; `None` leaves it to what the system says.
    fn started_this_process(&self, said: Option<&str>) -> bool;
}

/// Where Pitboard keeps its own files for a person whose home is `home`, when
/// `PITBOARD_HOME` names nowhere else: `.pitboard` in that home on macOS and Linux.
///
/// Worked out from the home it is handed, never from this account's own, so a context made
/// for a home of a test's own, or of an app's choosing, keeps Pitboard's files there. The
/// daily renewal schedule belongs to this home and no other ([`crate::schedule`]).
///
/// On Windows it is the person's local application data, `%LOCALAPPDATA%\Pitboard`, which
/// W14 finds from the home it is handed. Until then there is none: the empty path is refused
/// as a home that is not a full path, so nothing is read or written anywhere Pitboard did not
/// find, and `PITBOARD_HOME` has to name the folder.
pub(crate) fn default_pitboard_home(home: &Path) -> PathBuf {
    match OS {
        Os::MacOs | Os::Linux => home.join(".pitboard"),
        Os::Windows => PathBuf::new(),
    }
}

/// The host this build is standing on, which is what every real context uses.
pub(crate) fn current() -> Arc<dyn Host> {
    os::host()
}

/// The path this program was started by, where that lasts longer than the file it runs.
pub(crate) fn current_program() -> std::io::Result<PathBuf> {
    os::current_program()
}

#[cfg(test)]
mod tests {
    use super::*;

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
                // Credential Manager, which W23 offers.
                Os::Windows => None,
            }
        );
        assert_eq!(
            ctx.host().file(PathBuf::from("/nowhere/at/all")).kind(),
            crate::store::Backend::File
        );
    }

    /// A path is the same one only in the same case on macOS and Linux, as Pitboard has always
    /// compared where a program runs from there. A doubled or trailing separator still
    /// leaves it the same path.
    #[test]
    fn paths_are_compared_exactly_on_macos_and_linux() {
        for os in [Os::MacOs, Os::Linux] {
            let same = |a: &str, b: &str| os.same_path(Path::new(a), Path::new(b));
            assert!(same("ChatGPT.app", "ChatGPT.app"), "{os:?}");
            assert!(
                same("/Applications/ChatGPT.app/", "/Applications//ChatGPT.app"),
                "{os:?}"
            );
            assert!(!same("chatgpt.app", "ChatGPT.app"), "{os:?}");
            assert!(!same("ChatGPT.app", "ChatGPT"), "{os:?}");
        }
    }

    /// On Windows a path is the same one in any case, and only as Windows changes a name's
    /// case: a name written decomposed is another name, and so is one whose upper case is
    /// longer than itself.
    #[test]
    fn paths_are_compared_in_any_case_on_windows() {
        let same = |a: &str, b: &str| Os::Windows.same_path(Path::new(a), Path::new(b));
        assert!(same("Codex.exe", "codex.EXE"));
        assert!(same("/Programs/OpenAI/Codex/", "/programs//openai/CODEX"));
        assert!(same(r"C:\Users\Dana\Codex.exe", r"c:\users\dana\codex.exe"));
        assert!(same("Éclair", "éCLAIR"));
        assert!(!same("e\u{301}clair", "\u{e9}clair"), "not normalised");
        assert!(!same("Straße", "STRASSE"), "ß is upper case as it is");
        assert!(!same("Codex.exe", "Codex"));
        assert!(!same("/a/b", "/a/b/c"));
    }

    /// Pitboard writes for a scheduler of each system's own: launchd and a systemd user timer.
    /// Task Scheduler is written for by W25, and until then Windows offers none.
    #[test]
    fn a_scheduler_is_offered_where_pitboard_writes_for_one() {
        assert_eq!(
            current().scheduler().is_some(),
            match OS {
                Os::MacOs | Os::Linux => true,
                Os::Windows => false,
            }
        );
    }

    /// This system's host reads sudo from the environment the context was read from, which
    /// is where sudo says it, and root from the process itself. Set but empty, `SUDO_UID`
    /// says nothing, as every variable Pitboard reads that way.
    #[test]
    fn the_host_reads_sudo_from_the_context_and_root_from_the_process() {
        let under = |value: &str| {
            let env: Environment = [("HOME", "/Users/x"), ("SUDO_UID", value)]
                .into_iter()
                .collect();
            let ctx = Context::for_command_line(&env);
            ctx.host().elevation(&ctx)
        };
        assert_eq!(
            under("501"),
            match OS {
                Os::MacOs | Os::Linux => Elevation::Elevated { why: "with sudo" },
                Os::Windows => user::elevation(false),
            }
        );
        let ctx = Context::for_unit_test();
        assert_eq!(
            ctx.host().elevation(&ctx),
            user::elevation(false),
            "withheld from a unit test, so only root is read"
        );
        assert_eq!(under(""), user::elevation(false));
    }

    // 22631 is Windows 11 23H2, and 19045 Windows 10 22H2.
    #[test]
    fn the_floor_is_windows_11_24h2_with_server_2025_counted() {
        for met in [(10, 0, 26100), (10, 0, 26200), (10, 0, 27000), (11, 0, 100)] {
            assert_eq!(windows_floor(Some(met)), Floor::Met, "{met:?}");
        }
        for (version, build) in [
            ((10, 0, 26099), 26099),
            ((10, 0, 22631), 22631),
            ((10, 0, 19045), 19045),
            ((6, 3, 9600), 9600),
        ] {
            assert_eq!(
                windows_floor(Some(version)),
                Floor::Below { build },
                "{version:?}"
            );
        }
        assert_eq!(windows_floor(None), Floor::Unknown);
        assert_eq!(WINDOWS_FLOOR, 26100);
    }

    #[test]
    fn the_floor_is_met_on_the_systems_the_tests_run_on() {
        assert_eq!(current().floor(), Floor::Met);
    }
}
