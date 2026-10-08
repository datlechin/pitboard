//! What the harness does differently on each system, said once per fact as a `match` on
//! [`OS`], so a system added to [`Os`] does not compile here until each fact has been said
//! for it. The rest of the harness asks one of these, never which system it runs on, and sets
//! a file's access or makes a link only through the core's own `pitboard_core::testing::fs`.

use pitboard_core::host::{OS, Os};
use pitboard_core::testing::fs as files;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Where a login is kept, as the harness plants one, reads it back and takes it away. Told
/// apart only by a `match`, so a way of keeping one added here does not compile until every
/// part of the harness that plants, reads or takes away a login says what it does there.
#[derive(Debug, Clone, Copy)]
pub enum Kept {
    /// An item of the login keychain, under the name it is given and [`super::account`]. It
    /// outlives the test's own directory, so the harness takes it away by name.
    InKeychain,
    /// A file in the test's own directory, private to its owner, which goes with that
    /// directory.
    InFile,
    // Sealed to the Windows account: only Pitboard's own vault can write or open it.
    Sealed,
}

/// Where Claude Code keeps the login it uses: an item of the login keychain on macOS, and
/// `.credentials.json` in its config directory on Linux. Assumed the same on Windows, pending W22.
pub fn claude_code_login() -> Kept {
    match OS {
        Os::MacOs => Kept::InKeychain,
        Os::Linux | Os::Windows => Kept::InFile,
    }
}

/// Where Pitboard parks a login: an item of the login keychain on macOS, and a file in the
/// `vault` of its own directory on Linux, sealed to the account on Windows.
pub fn parked_login() -> Kept {
    match OS {
        Os::MacOs => Kept::InKeychain,
        Os::Linux => Kept::InFile,
        Os::Windows => Kept::Sealed,
    }
}

pub fn same_variable(a: &str, b: &str) -> bool {
    match OS {
        Os::MacOs | Os::Linux => a == b,
        // Windows takes a variable's name in any case: `all_proxy` and `ALL_PROXY` are one.
        Os::Windows => a.eq_ignore_ascii_case(b),
    }
}

// Pitboard reads none of the Windows ones before W14; listed so they pass on from then.
pub fn passed_on() -> &'static [&'static str] {
    match OS {
        Os::MacOs | Os::Linux => &["HOME", "USER"],
        Os::Windows => &["SystemRoot", "TEMP", "TMP", "USERNAME"],
    }
}

// `HOME` too: Pitboard reads it as the home on Windows until W14 reads `USERPROFILE`.
pub fn account_folders(root: &Path) -> Vec<(&'static str, PathBuf)> {
    match OS {
        Os::MacOs | Os::Linux => Vec::new(),
        Os::Windows => {
            let profile = root.join("profile");
            vec![
                ("USERPROFILE", profile.clone()),
                ("HOME", profile.clone()),
                ("APPDATA", profile.join("AppData").join("Roaming")),
                ("LOCALAPPDATA", profile.join("AppData").join("Local")),
            ]
        }
    }
}

pub fn make_account_folders(root: &Path) {
    for (_, folder) in account_folders(root) {
        std::fs::create_dir_all(&folder)
            .unwrap_or_else(|e| panic!("{} could not be made: {e}", folder.display()));
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "the tests' own PATH, behind the scratch programs a command is given first"
)]
pub fn search_path(bin: &Path) -> OsString {
    let after: Vec<PathBuf> = match OS {
        Os::MacOs | Os::Linux => std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default(),
        Os::Windows => system_folders(),
    };
    std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(after))
        .unwrap_or_else(|e| panic!("no PATH can hold {}: {e}", bin.display()))
}

#[allow(
    clippy::disallowed_methods,
    reason = "where Windows is, which every Windows program is given as SystemRoot"
)]
pub fn system_folders() -> Vec<PathBuf> {
    match OS {
        Os::MacOs | Os::Linux => vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
        Os::Windows => std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .map(|windows| vec![windows.join("System32"), windows])
            .unwrap_or_default(),
    }
}

// `env` exits 127; what `cmd.exe` exits with when npm's shim finds no interpreter is W19's.
pub fn found_no_interpreter(status: std::process::ExitStatus) -> bool {
    match OS {
        Os::MacOs | Os::Linux => status.code() == Some(127),
        Os::Windows => !status.success(),
    }
}

pub fn program(name: &str) -> String {
    match OS {
        Os::MacOs | Os::Linux => name.to_owned(),
        Os::Windows => format!("{name}.exe"),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Npm {
    LinksTheScript,
    WritesAShim,
}

pub fn npm() -> Npm {
    match OS {
        Os::MacOs | Os::Linux => Npm::LinksTheScript,
        Os::Windows => Npm::WritesAShim,
    }
}

/// `contents` at `path`, private to its owner, as Claude Code, Codex and Pitboard each leave
/// a login in a file: a stand-in that leaves the umask to decide writes one anybody can read,
/// which `doctor` is right to fail on and which none of them does.
pub fn write_private(path: &Path, contents: &str) {
    std::fs::write(path, contents)
        .unwrap_or_else(|e| panic!("{} could not be written: {e}", path.display()));
    files::make_private(path)
        .unwrap_or_else(|e| panic!("{} could not be made private: {e}", path.display()));
}

/// The directory `path`, made with any parent it is missing, and itself private to its owner
/// as Pitboard makes its vault.
pub fn create_private_dir(path: &Path) {
    std::fs::create_dir_all(path)
        .unwrap_or_else(|e| panic!("{} could not be made: {e}", path.display()));
    files::make_private(path)
        .unwrap_or_else(|e| panic!("{} could not be made private: {e}", path.display()));
}
