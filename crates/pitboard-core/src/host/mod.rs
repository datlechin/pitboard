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
//! system started does not have. So is [`watch`]: a watch on one folder, through the system's
//! own notice of a change, where the host says one hears every write ([`Host::watches`]).
//!
//! The system is chosen once, in this file, and nowhere else. A fact that differs by system
//! is a `match` on [`OS`], and [`Os`] lists every system Pitboard runs on, so a system added
//! there does not compile until each such fact has been said for it. This replaced branches
//! that read "macOS, or else Linux", which compiled anywhere and did the Linux thing.

use crate::context::{Context, Environment};
use crate::error::Result;
use crate::permit::Permit;
use crate::schedule::Job;
use crate::state::VaultForm;
use crate::store::RawStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "only Windows reads an access list; every system's tests read these rules"
    )
)]
pub(crate) mod acl;
pub(crate) mod administered;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod memory;
#[cfg(any(windows, test))]
pub(crate) mod owner;
pub(crate) mod program;
#[cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "only Windows replaces a file this way; every system's tests read these rules"
    )
)]
pub(crate) mod replace;
#[cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "only Windows schedules a task; every system's tests read these rules"
    )
)]
pub(crate) mod task;
pub(crate) mod token;
#[cfg(unix)]
mod unix;
pub(crate) mod watch;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
use macos as os;
#[cfg(windows)]
use windows as os;

pub(crate) use administered::{Administered, Hive};
pub(crate) use os::{fs, proc, user};
#[cfg(all(windows, any(test, feature = "test-support")))]
pub(crate) use windows::credman as windows_credman;
#[cfg(all(windows, test))]
pub(crate) use windows::seal::Dpapi;

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

/// A system's store of secrets that holds a login ahead of a tool's file, with a ceiling on
/// what one item takes: the login keychain on macOS, Credential Manager on Windows. On Linux
/// Claude Code keeps its login in `.credentials.json` alone (`live_chain_order`), so no file
/// there is behind a store, and no store's ceiling refuses a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretStore {
    Keychain,
    CredentialManager,
}

/// Where a home keeps its parked logins of Claude Code and Codex, as a person would look for
/// them: on macOS the keychain, on Windows sealed files, on Linux the form the home recorded.
/// Kept in a record by its [`Vault::code`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vault {
    /// Items of the login keychain, which every home on the Mac shares.
    Keychain,
    /// Items of the Secret Service's default collection, which every home of the session shares.
    SecretService,
    /// Files in the home's `vault`, which only this account reads.
    Files,
    /// Files in the home's `vault`, sealed to this Windows account.
    SealedFiles,
}

impl Vault {
    /// What a program branches on: `keychain`, `secret_service`, `files` or `sealed_files`.
    pub fn code(self) -> &'static str {
        match self {
            Vault::Keychain => "keychain",
            Vault::SecretService => "secret_service",
            Vault::Files => "files",
            Vault::SealedFiles => "sealed_files",
        }
    }
}

/// The only program trusted to read Claude Code's keychain item. Named here so the backend
/// that runs it and the doctor check that looks for it cannot drift apart.
pub(crate) const SECURITY: &str = "/usr/bin/security";

// The Linux app's program, which its packages put beside `pitboard`.
const LINUX_APP: &str = "pitboard-app";

impl Os {
    /// The program Pitboard reaches the system's own store of secrets through, where it goes
    /// through one rather than calling the system directly.
    pub(crate) fn secrets_tool(self) -> Option<&'static str> {
        match self {
            Os::MacOs => Some(SECURITY),
            // Credential Manager is called directly, through Windows' own API.
            Os::Linux | Os::Windows => None,
        }
    }

    // What a person types to make `paths` private to themselves: on Windows two `icacls` a
    // path, naming them by `sid`, and none without it or for files and folders alike.
    pub(crate) fn make_private_commands(
        self,
        kind: Kind,
        paths: &[&str],
        sid: Option<&str>,
    ) -> Vec<String> {
        match self {
            Os::MacOs | Os::Linux => {
                let mode = match kind {
                    Kind::Directory => "700",
                    Kind::File => "600",
                    Kind::Any => "go-rwx",
                };
                vec![format!("chmod {mode} {}", paths.join(" "))]
            }
            Os::Windows => match (kind, sid) {
                (Kind::Directory | Kind::File, Some(sid)) => paths
                    .iter()
                    .flat_map(|path| acl::icacls(kind == Kind::Directory, path, sid))
                    .collect(),
                _ => Vec::new(),
            },
        }
    }

    pub(crate) fn remove_file_command(self, path: &Path) -> String {
        match self {
            Os::MacOs | Os::Linux => {
                format!("rm '{}'", path.display().to_string().replace('\'', r"'\''"))
            }
            Os::Windows => {
                let mut quoted = String::new();
                for c in path.display().to_string().chars() {
                    // PowerShell takes each of these as a single quote, so each is doubled.
                    if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
                        quoted.push(c);
                    }
                    quoted.push(c);
                }
                format!("Remove-Item -LiteralPath '{quoted}'")
            }
        }
    }

    // Looking in one asks the person whether Pitboard may. Linux and Windows guard none.
    pub(crate) fn guarded_folders(self) -> &'static [&'static str] {
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

    // Homebrew's on a Mac, also npm's global `bin` there (`install_places`), and on Linux its
    // default prefix's (docs/facts/linux.md).
    pub(crate) fn package_bins(self, env: &Environment) -> Vec<PathBuf> {
        match self {
            Os::MacOs => vec!["/opt/homebrew/bin".into(), "/usr/local/bin".into()],
            Os::Linux => vec!["/home/linuxbrew/.linuxbrew/bin".into()],
            Os::Windows => [
                ("APPDATA", &["npm"][..]),
                ("LOCALAPPDATA", &["Microsoft", "WinGet", "Links"]),
                ("USERPROFILE", &["scoop", "shims"]),
            ]
            .into_iter()
            .filter_map(|(variable, below)| {
                let mut place = env.named_folder(variable)?;
                place.extend(below);
                Some(place)
            })
            .collect(),
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

    pub(crate) fn variable_key(self, name: &std::ffi::OsStr) -> std::ffi::OsString {
        match self {
            Os::MacOs | Os::Linux => name.to_os_string(),
            Os::Windows => name
                .to_string_lossy()
                .chars()
                .map(upper_one_for_one)
                .collect::<String>()
                .into(),
        }
    }

    pub(crate) fn user_variable(self) -> &'static str {
        match self {
            Os::MacOs | Os::Linux => "USER",
            Os::Windows => "USERNAME",
        }
    }

    // Bun 1.4.3's `os.homedir` on Windows is libuv's `uv_os_homedir`: `USERPROFILE`, else
    // `GetUserProfileDirectoryW`. Git Bash sets `HOME`, which no tool takes for the home there.
    pub fn home_variable(self) -> &'static str {
        match self {
            Os::MacOs | Os::Linux => "HOME",
            Os::Windows => "USERPROFILE",
        }
    }

    // From the home it is handed, so a scheduled run and one by hand agree.
    pub(crate) fn default_pitboard_home(self, home: &Path) -> PathBuf {
        match self {
            Os::MacOs | Os::Linux => home.join(".pitboard"),
            Os::Windows => home.join("AppData").join("Local").join("Pitboard"),
        }
    }

    // Never beside the Windows app's `Pitboard.exe`, since NTFS takes the two names for one.
    pub(crate) fn app_command_line(self, app: &Path) -> Option<PathBuf> {
        match self {
            Os::MacOs => (app.extension() == Some("app".as_ref()))
                .then(|| app.join("Contents/Helpers/pitboard")),
            Os::Linux => (app.file_name() == Some(LINUX_APP.as_ref()))
                .then(|| app.with_file_name("pitboard")),
            Os::Windows => app
                .file_name()
                .filter(|name| name.eq_ignore_ascii_case("Pitboard.exe"))
                .and_then(|_| app.parent())
                .map(|install| install.join("bin").join("pitboard.exe")),
        }
    }

    // The program an app itself starts, where no command line is kept, so a schedule that runs
    // it renews nothing. Windows keeps macOS's bundle rule, which no path there meets.
    pub(crate) fn an_apps_own_program(self, program: &Path) -> bool {
        match self {
            Os::MacOs | Os::Windows => {
                let mut dirs = program
                    .ancestors()
                    .skip(1)
                    .map(|dir| dir.file_name().and_then(|n| n.to_str()));
                dirs.next() == Some(Some("MacOS")) && dirs.next() == Some(Some("Contents"))
            }
            Os::Linux => program.file_name() == Some(LINUX_APP.as_ref()),
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

// One text per Windows path, read lexically on every system: two spellings of one folder key
// alike, and a short name or a link keys apart, as a symlink does on macOS.
pub fn path_key_in_any_case(path: &str) -> String {
    let path = path.replace('/', r"\");
    let path = match path.get(..8) {
        Some(verbatim) if verbatim.eq_ignore_ascii_case(r"\\?\UNC\") => {
            format!(r"\\{}", &path[8..])
        }
        _ => path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned(),
    };
    let upper = |name: &str| name.chars().map(upper_one_for_one).collect::<String>();
    let drive = path
        .as_bytes()
        .get(..2)
        .filter(|head| head[0].is_ascii_alphabetic() && head[1] == b':')
        .map(|head| char::from(head[0].to_ascii_uppercase()));
    let (root, rest) = if let Some(unc) = path.strip_prefix(r"\\") {
        let mut parts = unc.splitn(3, '\\');
        let (server, share) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        (
            format!(r"\\{}\{}\", upper(server), upper(share)),
            parts.next().unwrap_or(""),
        )
    } else if let Some(drive) = drive {
        match path[2..].strip_prefix('\\') {
            Some(rest) => (format!(r"{drive}:\"), rest),
            None => (format!("{drive}:"), &path[2..]),
        }
    } else if let Some(rest) = path.strip_prefix('\\') {
        (r"\".to_owned(), rest)
    } else {
        (String::new(), path.as_str())
    };
    let rooted = root.ends_with('\\');
    let mut names: Vec<&str> = Vec::new();
    for name in rest.split('\\') {
        match name {
            "" | "." => {}
            ".." if names.last().is_some_and(|last| *last != "..") => {
                names.pop();
            }
            ".." if rooted => {}
            name => names.push(name),
        }
    }
    let names: Vec<String> = names.into_iter().map(upper).collect();
    root + &names.join(r"\")
}

// As libuv's realpath and Codex's `AbsolutePathBuf` spell what `GetFinalPathNameByHandleW`
// gives, read lexically on every system.
pub(crate) fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path;
    };
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if drive_absolute(rest) => PathBuf::from(rest),
        _ => path,
    }
}

fn drive_absolute(path: &str) -> bool {
    matches!(path.as_bytes(), [drive, b':', b'\\' | b'/', ..] if drive.is_ascii_alphabetic())
}

// A full path as Rust's `Path::is_absolute` reads one on Windows, on any system: from a drive's
// root, `\\server\share`, or a `\\?\` or `\\.\` path, `/` taken for `\`.
pub(crate) fn windows_absolute(path: &str) -> bool {
    let separator = |c: char| c == '\\' || c == '/';
    let Some(rest) = path
        .strip_prefix(separator)
        .and_then(|rest| rest.strip_prefix(separator))
    else {
        return drive_absolute(path);
    };
    // With a `/` in it, `\\?\` is no verbatim path but a server named `?`.
    if path.starts_with(r"\\?\")
        || rest
            .strip_prefix('.')
            .is_some_and(|device| device.starts_with(separator))
    {
        return true;
    }
    let mut names = rest.split(separator);
    names.next().is_some_and(|server| !server.is_empty())
        && names.next().is_some_and(|share| !share.is_empty())
}

// Claude Desktop's on Windows, which no test reads (AGENTS.md). Its `Claude-3p` there is named
// by analogy with macOS's, unmeasured: a guard may refuse more than is there.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn claude_desktop_on_windows(roaming: &[PathBuf], local: &[PathBuf]) -> Vec<PathBuf> {
    let roaming = roaming
        .iter()
        .flat_map(|dir| ["Claude", "Claude-3p"].map(|name| dir.join(name)));
    roaming
        .chain(local.iter().map(|dir| dir.join("AnthropicClaude")))
        .collect()
}

// Where the Windows app's account windows keep their WebView2 sign-ins, each build its own.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn pitboard_app_on_windows(local: &[PathBuf]) -> Vec<PathBuf> {
    local
        .iter()
        .flat_map(|dir| ["Pitboard App", "Pitboard App Debug"].map(|name| dir.join(name)))
        .collect()
}

// Where the Linux app keeps its own data and cache, each build its own: the account windows'
// sign-ins among them, as on Windows.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn pitboard_app_on_linux(data: &[PathBuf], cache: &[PathBuf]) -> Vec<PathBuf> {
    data.iter()
        .chain(cache)
        .flat_map(|dir| {
            ["com.usepitboard.Pitboard", "com.usepitboard.Pitboard.Devel"].map(|id| dir.join(id))
        })
        .collect()
}

// Where this account keeps a real login, or would, and where `started_in` says, for a build for
// tests to refuse. This account's own are asked of the system, never of the environment: its
// XDG folders are the defaults in the home the system names.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn real_login_places(
    started_in: Option<&Context>,
) -> std::result::Result<Vec<PathBuf>, String> {
    let home = user::accounts_own_home().ok_or("this system names no home for this account")?;
    let pitboard = user::accounts_own_pitboard_home()
        .ok_or("this system names no Pitboard folder for this account")?;
    let own = Context::new(home).with_pitboard_home(pitboard);
    // A test's child is started with scratch app data folders inside its own config folder.
    let (roaming, local) = user::app_data_folders(started_in.is_some());
    Ok(std::iter::once(&own)
        .chain(started_in)
        .flat_map(|ctx| {
            let mut places = vec![
                crate::provider::claude::paths::config_dir(ctx),
                crate::provider::claude::paths::config_file(ctx),
                crate::provider::codex::paths::home(ctx),
                ctx.pitboard_home.clone(),
                ctx.default_pitboard_home(),
            ];
            match ctx.os {
                Os::MacOs => places.extend([
                    ctx.home.join("Library/Application Support/Claude"),
                    ctx.home.join("Library/Application Support/Claude-3p"),
                    PathBuf::from("/Applications/Claude.app"),
                ]),
                Os::Linux => places.extend(pitboard_app_on_linux(
                    ctx.data_home.as_slice(),
                    ctx.cache_home.as_slice(),
                )),
                // Asked of the system below, with the ones this process was started with.
                Os::Windows => {}
            }
            places
        })
        .chain(claude_desktop_on_windows(&roaming, &local))
        .chain(pitboard_app_on_windows(&local))
        .filter(|place| place.is_absolute())
        .collect())
}

#[cfg(all(windows, any(test, feature = "test-support")))]
pub(crate) use windows::lasting_environment;

// What the account's lasting environment names, which a test's own may have inherited: nothing
// off Windows.
#[cfg(all(not(windows), any(test, feature = "test-support")))]
pub(crate) fn lasting_environment(_name: &str) -> Vec<String> {
    Vec::new()
}

// Is, holds or lies in, for a build for tests to refuse, the same on every system: through
// every link, one to nothing yet included, and in any case, as macOS's own volumes compare.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn lies_in(path: &Path, place: &Path) -> bool {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let (real, real_place) = (resolved(&path, LINKS), resolved(place, LINKS));
    near(&path, place)
        || near(&real, &real_place)
        || meets(&real, &real_place)
        || meets(&real_place, &real)
}

// So a loop of links ends, as the system's own lookup does.
#[cfg(any(test, feature = "test-support"))]
const LINKS: u8 = 32;

#[cfg(any(test, feature = "test-support"))]
fn near(a: &Path, b: &Path) -> bool {
    begins(a, b) || begins(b, a)
}

// `canonicalize` spells `C:\x` as `\\?\C:\x`.
#[cfg(any(test, feature = "test-support"))]
fn begins(path: &Path, with: &Path) -> bool {
    let with = without_verbatim_prefix(with.to_path_buf());
    without_verbatim_prefix(path.to_path_buf())
        .ancestors()
        .any(|above| same_path_in_any_case(above, &with))
}

// A folder above `a` that is `b`'s deepest existing one, whatever either is named:
// `canonicalize` keeps /System/Volumes/Data/Users, which is /Users.
#[cfg(any(test, feature = "test-support"))]
fn meets(a: &Path, b: &Path) -> bool {
    let Some((deepest, below_b)) = b
        .ancestors()
        .find_map(|above| Some((fs::file_id(above)?, b.strip_prefix(above).ok()?)))
    else {
        return false;
    };
    a.ancestors().any(|above| {
        fs::file_id(above) == Some(deepest)
            && a.strip_prefix(above)
                .is_ok_and(|below_a| near(below_a, below_b))
    })
}

// The deepest part that exists, resolved, or a link there followed though it leads nowhere
// yet; the rest after it as written, less each `..`, as a folder made there would take it.
#[cfg(any(test, feature = "test-support"))]
fn resolved(path: &Path, links: u8) -> PathBuf {
    for found in path.ancestors() {
        let Ok(rest) = path.strip_prefix(found) else {
            continue;
        };
        if let Ok(real) = std::fs::canonicalize(found) {
            return beyond(real, rest);
        }
        let link = std::fs::symlink_metadata(found).is_ok_and(|it| it.file_type().is_symlink());
        if let (true, Some(left), Ok(to)) = (link, links.checked_sub(1), std::fs::read_link(found))
        {
            let to = match found.parent() {
                Some(parent) => parent.join(to),
                None => to,
            };
            return resolved(&beyond(to, rest), left);
        }
    }
    path.to_path_buf()
}

#[cfg(any(test, feature = "test-support"))]
fn beyond(mut at: PathBuf, rest: &Path) -> PathBuf {
    for part in rest.components() {
        match part {
            std::path::Component::ParentDir => {
                at.pop();
            }
            std::path::Component::CurDir => {}
            part => at.push(part),
        }
    }
    at
}

fn same_name_in_any_case(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    let (a, b) = (a.to_string_lossy(), b.to_string_lossy());
    a.chars()
        .map(upper_one_for_one)
        .eq(b.chars().map(upper_one_for_one))
}

// As Windows upper-cases a name: one UTF-16 unit for one, so ß and anything past the BMP stay.
pub(crate) fn upper_one_for_one(c: char) -> char {
    if u32::from(c) > 0xFFFF {
        return c;
    }
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(one), None) => one,
        _ => c,
    }
}

/// What the person's login shell said its `PATH` is.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    windows,
    allow(
        dead_code,
        reason = "only a login shell is late, and Windows has none: its `Path` is read from \
                  the registry"
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

// A login shell can take seconds, so never on an app's main thread.
pub(crate) fn login_path(env: &Environment) -> LoginPath {
    os::login_path(env)
}

/// Who besides a file's owner can reach it, as the system says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Access {
    /// Whether anybody but the owner can read or change it.
    pub(crate) shared: bool,
    // For a person to read: `mode 600`, or on Windows who may reach it, never by SID.
    pub(crate) described: String,
}

// What a folder Pitboard keeps logins in is, as `fs::private_dir` finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Private {
    Yes,
    Shared,
    NotOwn,
    Link,
    NotADirectory,
    Missing,
}

// What a person makes private with the commands `Os::make_private_commands` gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
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

// What Windows' settings say that changes what Pitboard or a tool does there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WindowsSettings {
    // The distributions WSL holds for this account, by `DistributionName` under HKCU's `Lxss`.
    pub(crate) wsl: Vec<String>,
    // `ProxyServer` where `ProxyEnable` is not 0, as hyper-util 0.1.20 reads them for Codex.
    pub(crate) proxy: Option<String>,
    // `EnableLUA`, which runs administrators in Admin Approval Mode where it is not 0.
    pub(crate) admin_approval: Option<bool>,
}

// Windows 11 24H2's build, which Windows Server 2025 shares.
pub(crate) const WINDOWS_FLOOR: u32 = 26100;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Process {
    pub(crate) pid: u32,
    // macOS gives what it was started as, which a shell gives as the bare name.
    pub(crate) path: PathBuf,
    // The family of the Windows package it runs from.
    pub(crate) package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub pid: u32,
    pub parent: u32,
    pub path: PathBuf,
    pub package: Option<String>,
    // False where Windows would not say whose it is or where it runs from: `path` is its name.
    pub placed: bool,
}

// None where one of them cannot be placed, since it may be any kind of holder.
pub(crate) fn named(os: Os, all: Vec<Listed>, program: &str) -> Option<Vec<Process>> {
    let files: Vec<String> = match os {
        Os::MacOs | Os::Linux => vec![program.to_owned()],
        Os::Windows => {
            let mut files = vec![format!("{program}.exe")];
            // winget's Codex is the nested portable codex-<triple>.exe behind Links\codex.exe;
            // Toolhelp names the image the link resolves to (codex_windows_install_places).
            if program == "codex" {
                files.push("codex-x86_64-pc-windows-msvc.exe".to_owned());
                files.push("codex-aarch64-pc-windows-msvc.exe".to_owned());
            }
            files
        }
    };
    let runs = |name: &std::ffi::OsStr| match os {
        Os::MacOs | Os::Linux => files.iter().any(|file| name == file.as_str()),
        Os::Windows => files
            .iter()
            .any(|file| same_name_in_any_case(name, std::ffi::OsStr::new(file))),
    };
    let found: Vec<Listed> = all
        .into_iter()
        .filter(|p| p.path.file_name().is_some_and(runs))
        .collect();
    if found.iter().any(|p| !p.placed) {
        return None;
    }
    Some(
        found
            .into_iter()
            .map(|p| Process {
                pid: p.pid,
                path: p.path,
                package: p.package,
            })
            .collect(),
    )
}

pub struct Jar {
    pub version: i64,
    pub compatible: i64,
    pub cookies: Vec<Cookie>,
}

pub struct Cookie {
    pub host: String,
    pub name: String,
    pub path: String,
    pub stored: Vec<u8>,
    pub expires_utc: i64,
}

// Lengths only: a cookie's value is a login.
impl std::fmt::Debug for Cookie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cookie")
            .field("host", &self.host)
            .field("name", &self.name)
            .field("path", &self.path)
            .field("stored", &format_args!("{} bytes", self.stored.len()))
            .field("expires_utc", &self.expires_utc)
            .finish()
    }
}

#[derive(Debug)]
pub enum JarError {
    Absent,
    // a write is under way, or the jar is held
    Unsettled(String),
    // read, and not a shape Pitboard knows
    Unknown(String),
    Unreadable(String),
    Unsupported,
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) use os::write_jar;

// What a run is handed to read. Windows takes `Nothing` alone.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Input<'a> {
    Nothing,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Stdin(&'a [u8]),
    #[cfg_attr(windows, allow(dead_code))]
    Descriptor3(&'a [u8]),
}

// What is kept of what a warm-up's tool prints.
const TOOL_PRINTS: u64 = 1 << 20;

// How long what a run printed is waited for once it ended: something that left its group may
// still hold a pipe.
const DRAINED: std::time::Duration = std::time::Duration::from_secs(5);

// Up to `kept` bytes of what a pipe carries, read on a thread of its own so a chatty program
// cannot stall.
fn drained(
    pipe: Option<impl std::io::Read + Send + 'static>,
    kept: u64,
) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (said, printed) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        if let Some(pipe) = pipe {
            let _ = pipe.take(kept).read_to_end(&mut bytes);
        }
        let _ = said.send(bytes);
    });
    printed
}

// What a run that ended with `status` printed, both pipes by one deadline.
fn output(
    status: std::process::ExitStatus,
    stdout: std::sync::mpsc::Receiver<Vec<u8>>,
    stderr: std::sync::mpsc::Receiver<Vec<u8>>,
) -> std::io::Result<std::process::Output> {
    let by = std::time::Instant::now() + DRAINED;
    let (stdout, stderr) = printed(&stdout, &stderr, by)?;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

// A pipe still open at `by` fails the run, rather than answer with part of what it said.
fn printed(
    stdout: &std::sync::mpsc::Receiver<Vec<u8>>,
    stderr: &std::sync::mpsc::Receiver<Vec<u8>>,
    by: std::time::Instant,
) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let read = |pipe: &std::sync::mpsc::Receiver<Vec<u8>>| {
        pipe.recv_timeout(by.saturating_duration_since(std::time::Instant::now()))
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "its output was still open {}s after it ended",
                        DRAINED.as_secs()
                    ),
                )
            })
    };
    Ok((read(stdout)?, read(stderr)?))
}

// What `ceiling` stopped, as a run says it.
fn timed_out(ceiling: std::time::Duration) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!("no answer within {}s", ceiling.as_secs()),
    )
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
    /// the login keychain on macOS, Credential Manager's generic items on Windows. `None`
    /// where the system has no such store, and a tool keeps its login in a file instead.
    ///
    /// Which items and which account is the other program's business, so both are handed
    /// in. Deriving them here is how Claude Code's slot hashing came to live inside what
    /// claimed to be an operating-system abstraction.
    fn foreign_secrets(&self, ctx: &Context, account: &str) -> Option<Box<dyn RawStore>>;

    /// The single file at `path`, as a store.
    fn file(&self, path: PathBuf) -> Box<dyn RawStore>;

    /// The forms a home may keep its parked logins in, where this system offers a choice,
    /// which each home records in `state.json`: Linux's Secret Service and files. None where
    /// the system keeps them one way, as macOS does in the keychain and Windows in sealed files.
    fn vault_forms(&self) -> &'static [VaultForm] {
        &[]
    }

    /// Whether a vault of `form` answers here, for a form this system offers: files always do,
    /// and the Secret Service where something on this session's bus has a default collection,
    /// locked or not. Nothing is unlocked to tell.
    fn answers(&self, _ctx: &Context, _form: VaultForm) -> bool {
        false
    }

    /// Where Pitboard's own parked logins go: the system's store of secrets where there is
    /// one, a private directory of files where there is not, and where the system offers
    /// both, the one `form` names. This one really is a fact about the machine.
    fn vault(&self, ctx: &Context, form: Option<VaultForm>) -> Box<dyn RawStore>;

    /// Whether every `PITBOARD_HOME` on this machine that keeps its parks in `form` parks
    /// them in the one vault. A keychain or a Secret Service collection belongs to the whole
    /// login session, so a park in it that one home cannot account for may be another home's;
    /// a vault of files lives inside its home, and nothing in it can be anybody else's.
    fn vault_is_shared(&self, form: Option<VaultForm>) -> bool;

    // Every process this user runs, in any session; Err says why the list could not be read.
    fn listing(&self) -> std::result::Result<Vec<Listed>, String>;

    // By the program's own name, so a script that merely mentions it is not counted. None is
    // not the same as none running. Another user's processes use that user's login.
    fn processes(&self, program: &str) -> Option<Vec<Process>> {
        named(OS, self.listing().ok()?, program)
    }

    // a Chromium jar's rows for `hosts`, and those named one of `names` on any host, as stored
    fn cookies(
        &self,
        jar: &Path,
        hosts: &[&str],
        names: &[&str],
    ) -> std::result::Result<Jar, JarError>;

    // None on a real host in a build for tests; a test names a version through `app_at`.
    fn app_version(&self, bundle: &Path) -> Option<String>;
    fn app_id(&self, bundle: &Path) -> Option<String>;

    /// The system's own scheduler, which runs Pitboard's jobs ([`crate::schedule::Job`]).
    /// `None` where there is none Pitboard knows how to ask.
    fn scheduler(&self) -> Option<&dyn Scheduler>;

    /// Whether this process runs as the person themselves, which is what the one gate every
    /// change passes asks ([`crate::permit::Permit`]). Takes the context because part of
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

    // As `administered_file`, read as `administered::read_up_to` reads it.
    fn administered_file_up_to(&self, path: &Path, limit: u64) -> Administered {
        if administered::READ_BY_REAL_HOSTS {
            administered::read_up_to(path, limit)
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

    // A configuration profile's plist as JSON: on macOS alone, and never in a build for tests.
    fn managed_plist(&self, _path: &Path) -> Administered {
        Administered::Unset
    }

    // None in a build for tests, on the real hosts.
    fn administered_folder(&self, dir: &Path) -> Vec<PathBuf> {
        if administered::READ_BY_REAL_HOSTS {
            administered::listed(dir)
        } else {
            Vec::new()
        }
    }

    // A Windows policy value; unset elsewhere, and in a build for tests, on the real hosts.
    fn policy(&self, _hive: Hive, _key: &str, _value: &str) -> Administered {
        Administered::Unset
    }

    // Windows' `FOLDERID_ProgramData`, where it can say; none elsewhere.
    fn program_data(&self) -> Option<PathBuf> {
        None
    }

    // The letters of Windows' fixed drives, at whose root a path with no drive leads; none
    // elsewhere.
    fn fixed_drives(&self) -> Vec<char> {
        Vec::new()
    }

    // The names of Credential Manager's items that begin `prefix`, never what they hold; none
    // elsewhere, and none in a build for tests on the real hosts.
    fn credential_names(&self, _prefix: &str) -> std::result::Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    // As `HKCU\Environment` holds it, which a program started from now on is given; unset
    // elsewhere, and in a build for tests on the real hosts.
    fn user_environment(&self, _name: &str) -> Administered {
        Administered::Unset
    }

    // As `HKLM`'s `Session Manager\Environment` holds it, which a program started from now on is
    // given where `HKCU\Environment` does not name it; unset elsewhere, and in a build for tests
    // on the real hosts.
    fn machine_environment(&self, _name: &str) -> Administered {
        Administered::Unset
    }

    // Whether Credential Manager holds a generic item named `target`, never what it holds; Err
    // says why it cannot tell. None is held where there is no Credential Manager.
    fn credential_held(
        &self,
        _ctx: &Context,
        _target: &str,
    ) -> std::result::Result<bool, crate::store::Error> {
        Ok(false)
    }

    // None elsewhere, and none in a build for tests on the real hosts.
    fn windows_settings(&self) -> WindowsSettings {
        WindowsSettings::default()
    }

    // Whether `/proc/version` names Microsoft, as Codex tells WSL (`codex_wsl`); false elsewhere,
    // and in a build for tests on the real hosts.
    fn kernel_is_wsl(&self) -> bool {
        false
    }

    // Whether `path`, or the nearest folder above it that opens, is on 9p, as WSL 2 mounts
    // Windows' drives; false off Linux.
    fn on_a_windows_drive(&self, _path: &Path) -> bool {
        false
    }

    /// Whether a watch of the folder `dir` ([`watch`]) hears every write there. Where it does
    /// not, a front end looks at Pitboard's files every few seconds instead.
    fn watches(&self, _dir: &Path) -> bool {
        true
    }
}

/// The system's own scheduler, which starts Pitboard's jobs ([`crate::schedule::Job`]):
/// `pitboard renew` once a day, and `pitboard watch` from sign-in on. launchd on macOS, a
/// systemd user unit on Linux, Task Scheduler on Windows. Never a homemade daemon.
///
/// What Pitboard decides about a job, which program it runs, with what, and whether it is
/// this home's to change, is [`crate::schedule`]'s. This is only how the system is asked, and
/// what it says back.
pub(crate) trait Scheduler: Send + Sync + std::fmt::Debug {
    /// Where the job is kept, where a person would look for it.
    fn location(&self, ctx: &Context, job: Job) -> crate::schedule::Location;

    /// Whether the job is there.
    fn installed(&self, ctx: &Context, job: Job) -> bool;

    /// The Pitboard the job runs, read back from what [`Scheduler::put`] wrote. `None` where
    /// nothing is installed, and where what is there does not name one the way `put` writes
    /// it.
    fn program(&self, ctx: &Context, job: Job) -> Option<PathBuf>;

    /// The options the job runs Pitboard with after its marker ([`Job::marked`]), such as
    /// `--at 90`, as [`Scheduler::put`] wrote them, read back. Empty where nothing is
    /// installed, and where what is there does not run Pitboard the way `put` writes it.
    fn options(&self, ctx: &Context, job: Job) -> Vec<String>;

    /// The variables the job is given, by name, as [`Scheduler::put`] wrote them, read back.
    /// Empty where nothing is installed, and where what is there gives none the way `put`
    /// writes them. The system can give a job more of its own, which this does not say.
    fn environment(&self, ctx: &Context, job: Job) -> Vec<(String, String)>;

    /// Hand the system `job`, which runs `program` with the job's marker and `options`, given
    /// `environment`, each variable by name, besides what the system gives every job, and ask
    /// the system to start it. What is written can hold a proxy's password, so only the person
    /// can read it, whatever was there before.
    ///
    /// Where the system will not start it, what was there goes back as it was, and is
    /// started again. The status, doctor and the app all read what is there, so a job left
    /// written that nothing runs would say it is on while it is not, which is the failure
    /// nobody would notice until the parked logins had run out or an account had.
    fn put(
        &self,
        ctx: &Context,
        permit: Permit,
        job: Job,
        program: &Path,
        options: &[String],
        environment: &[(String, String)],
    ) -> Result<()>;

    /// Stop the job and take it away. `false` when nothing was there.
    fn remove(&self, ctx: &Context, permit: Permit, job: Job) -> Result<bool>;

    /// Whether this process is a run `job` started. `said` is the job a test says this
    /// process runs as; `None` leaves it to what the system says.
    fn started_this_process(&self, job: Job, said: Option<&str>) -> bool;

    // What a run it started reads for itself of what `put` was given: none where the system
    // hands the job its variables.
    fn read_by_its_runs(&self, _ctx: &Context, _job: Job) -> Vec<(String, String)> {
        Vec::new()
    }

    // What the system says of the job beyond what Pitboard wrote: Task Scheduler's alone.
    fn task(&self, _ctx: &Context, _job: Job) -> Option<task::Task> {
        None
    }

    // Whether the job is there as an earlier Pitboard wrote it, which installing again mends:
    // systemd's alone.
    fn outdated(&self, _ctx: &Context, _job: Job) -> bool {
        false
    }
}

// Time awake since a fixed moment (boot on Windows, the first call elsewhere); a sleep adds none.
pub fn awake() -> std::time::Duration {
    os::awake()
}

#[cfg(any(windows, test))]
pub(crate) fn from_interrupt_time(units: u64) -> std::time::Duration {
    std::time::Duration::from_secs(units / 10_000_000)
        + std::time::Duration::from_nanos(units % 10_000_000 * 100)
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

    #[test]
    fn a_vault_is_kept_by_its_code() {
        for vault in [
            Vault::Keychain,
            Vault::SecretService,
            Vault::Files,
            Vault::SealedFiles,
        ] {
            let kept = serde_json::to_value(vault).expect("kept");
            assert_eq!(kept, vault.code(), "{vault:?}");
            assert_eq!(serde_json::from_value::<Vault>(kept).expect("read"), vault);
        }
    }

    // As a pipe a program started meanwhile took with it, or one that left the run's group.
    #[test]
    fn a_pipe_still_open_once_a_run_ended_fails_the_run() {
        let closed = || {
            let (reader, writer) = std::io::pipe().expect("a pipe");
            drop(writer);
            drained(Some(reader), u64::MAX)
        };
        let soon = std::time::Instant::now() + DRAINED;
        assert!(printed(&closed(), &closed(), soon).is_ok());
        let (reader, _held) = std::io::pipe().expect("a pipe");
        let refused = printed(
            &drained(Some(reader), u64::MAX),
            &closed(),
            std::time::Instant::now(),
        )
        .expect_err("still open");
        assert_eq!(refused.kind(), std::io::ErrorKind::TimedOut);
    }

    // This crate's clippy.toml replaces the root's, so it repeats each of its rules.
    #[test]
    fn the_crates_lints_keep_every_rule_of_the_workspaces_and_refuse_stds_creates() {
        let read = |path: &str| {
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).expect(path)
        };
        let rules = |text: &str| -> Vec<String> {
            text.lines()
                .filter(|line| line.trim_start().starts_with("{ path"))
                .map(str::to_owned)
                .collect()
        };
        let own = rules(&read("clippy.toml"));
        for rule in rules(&read("../../clippy.toml")) {
            assert!(own.contains(&rule), "{rule}");
        }
        for create in [
            "std::fs::write",
            "std::fs::OpenOptions::create",
            "std::fs::create_dir_all",
        ] {
            let named = format!("{{ path = \"{create}\",");
            assert!(own.iter().any(|rule| rule.contains(&named)), "{create}");
        }
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
                Os::Windows => Some(crate::store::Backend::CredentialManager),
            }
        );
        assert_eq!(
            ctx.host().file(PathBuf::from("/nowhere/at/all")).kind(),
            crate::store::Backend::File
        );
    }

    fn listed(pid: u32, path: &str) -> Listed {
        Listed {
            pid,
            parent: 1,
            path: PathBuf::from(path),
            package: None,
            placed: true,
        }
    }

    // 0.162.1's Windows package holds codex.exe beside programs whose names start like it.
    #[test]
    fn a_program_is_named_by_its_file_as_each_system_names_it() {
        let all = || {
            vec![
                listed(
                    1,
                    "C:/Users/a/.codex/packages/standalone/current/bin/Codex.EXE",
                ),
                listed(2, "C:/n/vendor/codex-resources/codex-command-runner.exe"),
                listed(3, "C:/n/vendor/bin/codex-code-mode-host.exe"),
                listed(4, "C:/n/vendor/codex-path/rg.exe"),
                listed(5, "/usr/local/bin/codex"),
                listed(6, "C:/n/codex"),
                // winget's nested portable, which Toolhelp names behind Links\codex.exe.
                listed(
                    7,
                    "C:/Users/a/AppData/Local/Microsoft/WinGet/Packages/OpenAI.Codex_x/codex-x86_64-pc-windows-msvc.exe",
                ),
                listed(
                    8,
                    "C:/Users/a/AppData/Local/Microsoft/WinGet/Packages/OpenAI.Codex_x/codex-aarch64-pc-windows-msvc.exe",
                ),
            ]
        };
        let pids = |os: Os| {
            named(os, all(), "codex").map(|found| found.iter().map(|p| p.pid).collect::<Vec<_>>())
        };
        assert_eq!(pids(Os::Windows), Some(vec![1, 7, 8]));
        assert_eq!(pids(Os::MacOs), Some(vec![5, 6]));
        assert_eq!(pids(Os::Linux), Some(vec![5, 6]));
    }

    // Such as the same person's elevated codex: it may be any kind of holder.
    #[test]
    fn a_process_that_cannot_be_placed_makes_the_answer_unknown() {
        let unplaced = |name: &str| Listed {
            placed: false,
            ..listed(7, name)
        };
        let placed = listed(1, "C:/x/codex.exe");
        assert_eq!(
            named(
                Os::Windows,
                vec![placed.clone(), unplaced("codex.exe")],
                "codex"
            ),
            None
        );
        assert_eq!(
            named(Os::Windows, vec![placed, unplaced("svchost.exe")], "codex")
                .map(|found| found.len()),
            Some(1),
            "unless it is not the program"
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

    #[test]
    fn a_verbatim_path_is_spelled_as_libuv_and_codex_spell_it() {
        let plain = |path: &str| without_verbatim_prefix(PathBuf::from(path));
        assert_eq!(
            plain(r"\\?\C:\Users\X\.codex"),
            PathBuf::from(r"C:\Users\X\.codex")
        );
        assert_eq!(
            plain(r"\\?\UNC\server\share\x"),
            PathBuf::from(r"\\server\share\x")
        );
        for kept in [
            r"C:\Users\X",
            r"\\?\Volume{0}\x",
            r"\\server\share",
            "/Users/x",
        ] {
            assert_eq!(plain(kept), PathBuf::from(kept), "{kept}");
        }
    }

    // Read by the text alone, on every system, as Foundation standardises a file URL on macOS.
    #[test]
    fn a_windows_path_has_one_key_however_it_is_spelled() {
        let key = path_key_in_any_case;
        for spelling in [
            r"C:\Users\X\AppData\Local\Pitboard",
            r"c:\users\x\appdata\local\pitboard\",
            r"\\?\C:\Users\X\AppData\Local\Pitboard",
            "C:/Users/X/AppData/Local/Pitboard",
            r"C:\Users\X\.\AppData\Local\..\Local\Pitboard",
            r"C:\Users\\X\AppData\Local\Pitboard",
        ] {
            assert_eq!(
                key(spelling),
                r"C:\USERS\X\APPDATA\LOCAL\PITBOARD",
                "{spelling}"
            );
        }
        assert_eq!(key(r"\\server\share\Pitboard"), r"\\SERVER\SHARE\PITBOARD");
        assert_eq!(
            key(r"\\?\UNC\server\share\Pitboard"),
            r"\\SERVER\SHARE\PITBOARD"
        );
        assert_eq!(key(r"C:\..\Pitboard"), r"C:\PITBOARD");
        assert_eq!(key(r"C:\"), r"C:\");
        assert_eq!(key(r"C:\Users\Straße"), r"C:\USERS\STRAßE");
        assert_ne!(key("C:\\Users\\e\u{301}"), key("C:\\Users\\\u{e9}"));
        assert_ne!(
            key(r"C:\Users\XAVIER~1\AppData\Local\Pitboard"),
            key(r"C:\Users\Xavier Dana\AppData\Local\Pitboard"),
            "no short name is expanded"
        );
    }

    // One answer on every system: macOS's own volumes ignore case, and Windows drops `..` as
    // written, which a folder made where none is yet would do too.
    #[test]
    fn a_path_lies_in_a_place_in_any_case_and_past_a_folder_not_made_yet() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path().join("root");
        let place = root.join("Library/Application Support");
        let lies = |path: &str| lies_in(&root.join(path), &place);
        assert!(lies("Library/Application Support/x"));
        assert!(lies("library/APPLICATION SUPPORT/x"), "in any case");
        assert!(lies("LIBRARY"), "it holds it");
        assert!(lies("never-made/../Library/Application Support/x"));
        assert!(!lies("Library/Application Supportx"));
        assert!(!lies("never-made/Library/Application Support"));
        assert!(!lies("Library/../elsewhere/Application Support"));
        assert!(!root.exists());
    }

    #[test]
    fn claude_desktops_windows_folders_are_refused_in_any_spelling() {
        let (roaming, local) = (
            PathBuf::from("/p/AppData/Roaming"),
            PathBuf::from("/p/AppData/Local"),
        );
        let places =
            claude_desktop_on_windows(std::slice::from_ref(&roaming), std::slice::from_ref(&local));
        let refused = |path: PathBuf| places.iter().any(|place| lies_in(&path, place));
        for path in [
            roaming.join("Claude"),
            roaming.join("claude").join("Cookies"),
            roaming.join("CLAUDE-3P"),
            local.join("AnthropicClaude").join("app-1.0"),
            roaming.clone(),
        ] {
            assert!(refused(path.clone()), "{}", path.display());
        }
        for path in [
            roaming.join("Claudette"),
            local.join("Claude"),
            local.join("Pitboard"),
        ] {
            assert!(!refused(path.clone()), "{}", path.display());
        }
    }

    // Compared by component, so `Pitboard` and `Pitboard App` are never inside one another.
    #[test]
    fn the_windows_apps_folders_are_refused_in_any_spelling() {
        let local = PathBuf::from("/p/AppData/Local");
        let places = pitboard_app_on_windows(std::slice::from_ref(&local));
        let refused = |path: PathBuf| places.iter().any(|place| lies_in(&path, place));
        for path in [
            local.join("Pitboard App").join("WebView2"),
            local.join("pitboard app debug").join("x"),
            local.join("PITBOARD APP"),
            local.clone(),
        ] {
            assert!(refused(path.clone()), "{}", path.display());
        }
        for path in [
            local.join("Pitboard Apps"),
            local.join("Pitboard"),
            local.join("Pitboard").join("App"),
            local.join("Pitboard App Debugger"),
        ] {
            assert!(!refused(path.clone()), "{}", path.display());
        }
    }

    // Compared by component, so the release's folder and the debug build's never hold each other.
    #[test]
    fn the_linux_apps_folders_are_refused_in_any_spelling() {
        let (data, cache) = (PathBuf::from("/h/.local/share"), PathBuf::from("/h/.cache"));
        let places =
            pitboard_app_on_linux(std::slice::from_ref(&data), std::slice::from_ref(&cache));
        let refused = |path: PathBuf| places.iter().any(|place| lies_in(&path, place));
        for path in [
            data.join("com.usepitboard.Pitboard").join("window.json"),
            data.join("com.usepitboard.pitboard.devel").join("x"),
            cache.join("COM.USEPITBOARD.PITBOARD"),
            cache.join("com.usepitboard.Pitboard.Devel"),
            data.clone(),
            cache.clone(),
        ] {
            assert!(refused(path.clone()), "{}", path.display());
        }
        for path in [
            data.join("com.usepitboard.Pitboards"),
            data.join("com.usepitboard.Pitboard.Develop"),
            data.join("com.usepitboard"),
            cache.join("pitboard"),
            PathBuf::from("/h/.config/com.usepitboard.Pitboard"),
        ] {
            assert!(!refused(path.clone()), "{}", path.display());
        }
    }

    #[test]
    #[cfg(windows)]
    fn a_verbatim_path_lies_where_its_plain_spelling_does() {
        let lies = |path: &str, place: &str| lies_in(Path::new(path), Path::new(place));
        assert!(lies(
            r"\\?\C:\Users\Dana\Library\x",
            r"c:\users\dana\library"
        ));
        assert!(lies(
            r"C:\Users\Dana\Library\x",
            r"\\?\c:\users\dana\library"
        ));
        assert!(!lies(
            r"\\?\D:\Users\Dana\Library\x",
            r"C:\Users\Dana\Library"
        ));
    }

    // Homebrew's default prefix on Linux, where its `claude-code` and `codex` casks install too.
    #[test]
    fn an_app_on_linux_looks_in_homebrews_bin() {
        let env = Environment::on(Os::Linux, [("HOME", "/home/x")]);
        let brew = PathBuf::from("/home/linuxbrew/.linuxbrew/bin");
        assert_eq!(Os::Linux.package_bins(&env), std::slice::from_ref(&brew));
        for tool in crate::provider::ProviderId::ALL {
            if let Some(cli) = tool.cli() {
                assert!((cli.install_places)(&env).contains(&brew), "{tool}");
            }
        }
        assert!(crate::app::command_line_places(&env, None).contains(&brew));
    }

    // Linux's packages put `pitboard-app` beside `pitboard` in one `bin`.
    #[test]
    fn each_systems_app_names_the_command_line_it_comes_with() {
        let names = |os: Os, app: &str| os.app_command_line(Path::new(app));
        let helper = |app: &str| Some(Path::new(app).join("Contents/Helpers/pitboard"));
        for (os, app, inside) in [
            (
                Os::Linux,
                "/usr/bin/pitboard-app",
                Some(PathBuf::from("/usr/bin/pitboard")),
            ),
            (
                Os::Linux,
                "/home/x/.local/bin/pitboard-app",
                Some(PathBuf::from("/home/x/.local/bin/pitboard")),
            ),
            (Os::Linux, "/usr/bin/pitboard", None),
            (Os::Linux, "/usr/bin/Pitboard-App", None),
            (Os::Linux, "/usr/bin/pitboard-app.desktop", None),
            (Os::Linux, "/Applications/Pitboard.app", None),
            (
                Os::MacOs,
                "/Applications/Pitboard.app",
                helper("/Applications/Pitboard.app"),
            ),
            (Os::MacOs, "/usr/bin/pitboard-app", None),
            (
                Os::Windows,
                "C:/Program Files/Pitboard/Pitboard.exe",
                Some(PathBuf::from("C:/Program Files/Pitboard/bin/pitboard.exe")),
            ),
            (Os::Windows, "C:/Program Files/Pitboard/pitboard-app", None),
        ] {
            assert_eq!(names(os, app), inside, "{os:?} {app}");
        }
    }

    // launchd, a systemd user timer where systemd started Linux, and Task Scheduler.
    #[test]
    fn a_scheduler_is_offered_where_pitboard_writes_for_one() {
        assert_eq!(
            current().scheduler().is_some(),
            OS != Os::Linux || Path::new("/run/systemd/system").is_dir()
        );
    }

    /// This system's host reads sudo from the environment the context was read from, which
    /// is where sudo says it, and root from the process itself, with the home's owner on
    /// Linux. Set but empty, `SUDO_UID` says nothing, as every variable Pitboard reads that
    /// way.
    #[test]
    fn the_host_reads_sudo_from_the_context_and_root_from_the_process() {
        let ctx_under = |value: &str| {
            let env: Environment = [("HOME", "/Users/x"), ("SUDO_UID", value)]
                .into_iter()
                .collect();
            Context::for_command_line(&env)
        };
        let under = |value: &str| {
            let ctx = ctx_under(value);
            ctx.host().elevation(&ctx)
        };
        #[cfg(target_os = "linux")]
        let unsudoed = |ctx: &Context| user::linux_elevation(false, &crate::home::written(ctx));
        #[cfg(not(target_os = "linux"))]
        let unsudoed = |_: &Context| user::elevation(false);
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
            unsudoed(&ctx),
            "withheld from a unit test, so only root is read"
        );
        assert_eq!(under(""), unsudoed(&ctx_under("")));
    }

    // Instant reads the same clock on macOS and Linux, and a clock that counts sleep on Windows.
    #[test]
    fn time_awake_moves_with_this_process_while_it_is_awake() {
        let (awake_at, instant_at) = (awake(), std::time::Instant::now());
        std::thread::sleep(std::time::Duration::from_millis(50));
        let (passed, measured) = (awake() - awake_at, instant_at.elapsed());
        // Wide enough for a busy runner, narrow enough to catch a unit read ten times off.
        assert!(
            passed > measured / 2 && passed < measured * 2,
            "{passed:?} against {measured:?}"
        );
    }

    #[test]
    fn interrupt_time_is_in_hundreds_of_nanoseconds() {
        use std::time::Duration;
        assert_eq!(from_interrupt_time(10_000_000), Duration::from_secs(1));
        assert_eq!(from_interrupt_time(1), Duration::from_nanos(100));
        assert_eq!(
            from_interrupt_time(36_000_000_015),
            Duration::new(3600, 1_500)
        );
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
