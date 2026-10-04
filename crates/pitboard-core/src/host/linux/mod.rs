//! Linux: files, a vault of files, `/proc` and systemd.
//!
//! Claude Code has no keyring backend on Linux, so its login is a plaintext file it sets to
//! 0600, and Pitboard's parked logins are 0600 files in a 0700 directory beside it.

mod procfs;
mod systemd;

pub(crate) use super::unix::{fs, proc, user};

use super::unix::service;
use super::{Bundle, Host, LoginPath, Os, Process, Scheduler};
use crate::context::{Context, Environment};
use crate::provider::desktop::safe_storage::{ItemStamp, KeyRead, KeyReadError, SafeStorage};
use crate::provider::desktop::types::CookieTable;
use crate::store::vault::FileVault;
use crate::store::{PlainFile, RawStore};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

pub(super) const OS: Os = Os::Linux;

/// What `posix_spawn` is told when Pitboard starts the person's login shell: a session of its
/// own. Linux has no flag that keeps the rest of this process's descriptors from it; every
/// one Rust opens is closed on exec already, and no app on Linux asks yet.
pub(super) const SPAWN_FLAGS: libc::c_short = libc::POSIX_SPAWN_SETSID;

/// Linux has a login shell to ask, as macOS does.
pub(super) fn login_path(env: &Environment) -> LoginPath {
    super::unix::shell::login_path(env, SPAWN_FLAGS)
}

#[derive(Debug)]
struct Linux {
    scheduler: systemd::Systemd,
}

impl Host for Linux {
    fn foreign_secrets(&self, _ctx: &Context, _account: &str) -> Option<Box<dyn RawStore>> {
        None
    }

    fn file(&self, path: PathBuf) -> Box<dyn RawStore> {
        Box::new(PlainFile::at(path))
    }

    fn vault(&self, ctx: &Context) -> Box<dyn RawStore> {
        Box::new(FileVault::new(ctx))
    }

    fn vault_is_shared(&self) -> bool {
        false
    }

    fn processes(&self, program: &str) -> Option<Vec<Process>> {
        procfs::processes(program)
    }

    fn processes_within(&self, bundle: Bundle<'_>, excluded: &[&str]) -> Option<Vec<Process>> {
        procfs::processes_within(bundle, excluded)
    }

    fn pid_alive(&self, pid: u32) -> bool {
        proc::may_be_running(pid)
    }

    fn program_of(&self, pid: u32) -> Option<PathBuf> {
        procfs::program_of(pid)
    }

    fn device_of(&self, path: &Path) -> std::io::Result<u64> {
        fs::device(path)
    }

    /// Claude Desktop does not run on Linux, and there is no jar to read.
    fn cookie_table(&self, _path: &Path) -> std::io::Result<CookieTable> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Claude Desktop's cookies are read on macOS only",
        ))
    }

    /// No app runs on Linux.
    fn bundle_version(&self, _app: &Path) -> Option<String> {
        None
    }

    fn scheduler(&self) -> Option<&dyn Scheduler> {
        Some(&self.scheduler)
    }
}

pub(super) fn host() -> Arc<dyn Host> {
    Arc::new(Linux {
        scheduler: systemd::Systemd::new(service::system()),
    })
}

/// Claude Desktop does not run on Linux, so there is no key to read.
#[derive(Debug)]
struct NoSafeStorage;

impl SafeStorage for NoSafeStorage {
    fn stamp(&self, _ctx: &Context) -> Result<ItemStamp, KeyReadError> {
        Err(KeyReadError::Missing)
    }

    fn password(&self, _ctx: &Context, _how: KeyRead) -> Result<Zeroizing<Vec<u8>>, KeyReadError> {
        Err(KeyReadError::Missing)
    }
}

pub(super) fn safe_storage() -> Arc<dyn SafeStorage> {
    Arc::new(NoSafeStorage)
}

/// The path this Pitboard was started by, where that leads to the program running.
///
/// Linux says where the running program is with every link resolved, and the link is what
/// lasts: Homebrew starts Pitboard through one in its `bin` that leads into a directory
/// named after the version, which the next upgrade deletes.
#[allow(
    clippy::disallowed_methods,
    reason = "the PATH this process was found on, as its own arguments are read"
)]
pub(super) fn current_program() -> std::io::Result<PathBuf> {
    Ok(started_as(
        std::env::args_os().next().as_deref(),
        &std::env::var_os("PATH").unwrap_or_default(),
        std::env::current_exe()?,
    ))
}

/// `argv0` found on `search` the way the shell found it, where that leads to the `running`
/// program; otherwise `running`. A path that leads to some other file did not start this
/// one.
fn started_as(
    argv0: Option<&std::ffi::OsStr>,
    search: &std::ffi::OsStr,
    running: PathBuf,
) -> PathBuf {
    let resolved = std::fs::canonicalize(&running).ok();
    argv0
        .and_then(|named| super::program::find(std::path::Path::new(named), search))
        .filter(|found| resolved.is_some() && std::fs::canonicalize(found).ok() == resolved)
        .unwrap_or(running)
}

/// systemd as a machine in memory has it: real unit files in the test's own home, and a
/// service manager that asks nobody.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn pretend_scheduler(
    refuse_start: Arc<std::sync::atomic::AtomicBool>,
) -> Box<dyn Scheduler> {
    Box::new(systemd::Systemd::new(Arc::new(service::Pretend {
        refuse_start,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Homebrew starts Pitboard through a link in its `bin` that leads into a directory
    /// named after the version, and the next upgrade deletes that directory. Linux says
    /// where the running program is with every link resolved, so the schedule is given the
    /// path Pitboard was started by instead, wherever that leads to this same program.
    #[test]
    fn the_command_line_is_known_by_the_path_it_was_started_by() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-started-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let runnable = |path: &std::path::Path| {
            std::fs::create_dir_all(path.parent().unwrap()).expect("its directory");
            std::fs::write(path, "").expect("a program");
            fs::testing::make_runnable(path);
        };
        let running = home.join("Caskroom/pitboard/0.4.0/pitboard");
        runnable(&running);
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).expect("a bin");
        let link = bin.join("pitboard");
        std::os::unix::fs::symlink(&running, &link).expect("a link");
        let another = home.join("elsewhere/pitboard");
        runnable(&another);

        let started = |argv0: Option<&std::path::Path>, search: &std::path::Path| {
            started_as(
                argv0.map(std::path::Path::as_os_str),
                search.as_os_str(),
                running.clone(),
            )
        };
        let name = std::path::Path::new("pitboard");
        let nowhere = std::path::Path::new("");
        assert_eq!(started(Some(name), &bin), link, "a name found on PATH");
        assert_eq!(started(Some(&link), nowhere), link, "a path");
        assert_eq!(
            started(Some(name), another.parent().expect("its directory")),
            running,
            "another Pitboard on PATH is not the one that is running"
        );
        assert_eq!(
            started(Some(name), nowhere),
            running,
            "a name found nowhere"
        );
        assert_eq!(started(None, &bin), running, "no name at all");
        let _ = std::fs::remove_dir_all(&home);
    }
}
