//! macOS: the login keychain, files, `ps`, launchd and managed preferences.

mod bundle;
mod keychain;
mod launchd;
mod preferences;
mod ps;
mod sqlite;

pub(super) use super::unix::awake;
pub(crate) use super::unix::{fs, proc, user};
#[cfg(any(test, feature = "test-support"))]
pub(crate) use sqlite::write_jar;

use super::administered::{self, Administered};
use super::unix::service;
use super::{Elevation, Floor, Host, Jar, JarError, Listed, LoginPath, Os, Scheduler};
use crate::context::{Context, Environment};
use crate::store::{PlainFile, RawStore};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) const OS: Os = Os::MacOs;

// No build for tests reads an app installed on the machine running it.
const READS_APPS: bool = !cfg!(any(test, feature = "test-support"));

/// What `posix_spawn` is told when Pitboard starts the person's login shell: a session of its
/// own, and none of this process's descriptors but the three it is handed, whatever in the
/// app opened the rest. `POSIX_SPAWN_SETSID` is 0x0400 in `<sys/spawn.h>` of the macOS 27
/// SDK, and the libc crate does not name it.
pub(super) const SPAWN_FLAGS: libc::c_short =
    0x0400 | libc::POSIX_SPAWN_CLOEXEC_DEFAULT as libc::c_short;

/// macOS has a login shell to ask.
pub(super) fn login_path(env: &Environment) -> LoginPath {
    super::unix::shell::login_path(env, SPAWN_FLAGS)
}

pub(super) fn rename_exclusive(from: &std::ffi::CStr, to: &std::ffi::CStr) -> libc::c_int {
    // SAFETY: both pointers are to NUL-terminated strings that outlive the call, which
    // reads them and writes no memory of this process.
    unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) }
}

/// The keychain account Pitboard stores its own items under.
///
/// It is Claude Code's derivation, and it stays Claude Code's derivation, because every
/// park already on every machine is filed under whatever this returned the day it was
/// written. Changing it would not move those items; it would make them unfindable, which
/// is the same as deleting every parked login on upgrade.
fn vault_account(ctx: &Context) -> String {
    crate::provider::claude::slot::account_name(ctx)
}

#[derive(Debug)]
struct MacOs {
    scheduler: launchd::Launchd,
}

impl Host for MacOs {
    fn foreign_secrets(&self, ctx: &Context, account: &str) -> Option<Box<dyn RawStore>> {
        Some(Box::new(keychain::Keychain::foreign(
            ctx,
            account.to_string(),
        )))
    }

    fn file(&self, path: PathBuf) -> Box<dyn RawStore> {
        Box::new(PlainFile::at(path))
    }

    fn vault(&self, ctx: &Context) -> Box<dyn RawStore> {
        Box::new(keychain::Keychain::vault(ctx))
    }

    fn vault_is_shared(&self) -> bool {
        true
    }

    fn listing(&self) -> Result<Vec<Listed>, String> {
        ps::listing()
    }

    fn cookies(&self, jar: &Path, hosts: &[&str], names: &[&str]) -> Result<Jar, JarError> {
        sqlite::cookies(jar, hosts, names)
    }

    fn app_version(&self, bundle: &Path) -> Option<String> {
        READS_APPS.then(|| bundle::version(bundle)).flatten()
    }

    fn app_id(&self, bundle: &Path) -> Option<String> {
        READS_APPS.then(|| bundle::identifier(bundle)).flatten()
    }

    fn scheduler(&self) -> Option<&dyn Scheduler> {
        Some(&self.scheduler)
    }

    fn elevation(&self, ctx: &Context) -> Elevation {
        user::elevation(ctx.sudo())
    }

    fn floor(&self) -> Floor {
        Floor::Met
    }

    fn managed_preference(&self, domain: &str, key: &str) -> Administered {
        if administered::READ_BY_REAL_HOSTS {
            preferences::forced(domain, key)
        } else {
            Administered::Unset
        }
    }

    fn managed_plist(&self, path: &Path) -> Administered {
        if administered::READ_BY_REAL_HOSTS {
            preferences::as_json(path)
        } else {
            Administered::Unset
        }
    }
}

pub(super) fn host() -> Arc<dyn Host> {
    Arc::new(MacOs {
        scheduler: launchd::Launchd::new(service::system()),
    })
}

/// macOS already says what path a program was started by.
pub(super) fn current_program() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}

/// launchd as a machine in memory has it: real files in the test's own home, and a service
/// manager that asks nobody.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn pretend_scheduler(
    refuse_start: Arc<std::sync::atomic::AtomicBool>,
) -> Option<Box<dyn Scheduler>> {
    Some(Box::new(launchd::Launchd::new(Arc::new(
        service::Pretend { refuse_start },
    ))))
}
