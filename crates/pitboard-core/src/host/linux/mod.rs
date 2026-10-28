//! Linux: files, a vault of files, `/proc` and systemd.
//!
//! Claude Code has no keyring backend on Linux, so its login is a plaintext file it sets to
//! 0600, and Pitboard's parked logins are 0600 files in a 0700 directory beside it.

mod procfs;
mod systemd;

pub(super) use super::unix::awake;
pub(super) use super::unix::watch;
pub(crate) use super::unix::{fs, proc, user};

use super::administered::Administered;
use super::unix::service;
use super::{Elevation, Floor, Host, Jar, JarError, Listed, LoginPath, Os, Process, Scheduler};
use crate::context::{Context, Environment};
use crate::store::vault::FileVault;
use crate::store::{PlainFile, RawStore};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) const OS: Os = Os::Linux;

/// What `posix_spawn` is told when Pitboard starts the person's login shell: a session of its
/// own. Linux has no flag that keeps the rest of this process's descriptors from it; every
/// one Rust opens is closed on exec already, and no app on Linux asks yet.
pub(super) const SPAWN_FLAGS: libc::c_short = libc::POSIX_SPAWN_SETSID;

/// Linux has a login shell to ask, as macOS does.
pub(super) fn login_path(env: &Environment) -> LoginPath {
    super::unix::shell::login_path(env, SPAWN_FLAGS)
}

pub(super) fn rename_exclusive(from: &std::ffi::CStr, to: &std::ffi::CStr) -> libc::c_int {
    // SAFETY: both pointers are to NUL-terminated strings that outlive the call, which
    // reads them and writes no memory of this process; `AT_FDCWD` names no descriptor.
    let renamed = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if renamed == 0 { 0 } else { -1 }
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

    // Nothing asks: Claude Desktop's data folder is refused here first.
    fn listing(&self) -> Result<Vec<Listed>, String> {
        Err("this system's process list is not read for Claude Desktop".into())
    }

    // By `comm`, the name a script runs as, where `/proc/<pid>/exe` is its interpreter.
    fn processes(&self, program: &str) -> Option<Vec<Process>> {
        procfs::processes(program)
    }

    fn cookies(&self, _jar: &Path, _hosts: &[&str], _names: &[&str]) -> Result<Jar, JarError> {
        Err(JarError::Unsupported)
    }

    fn app_version(&self, _bundle: &Path) -> Option<String> {
        None
    }

    fn app_id(&self, _bundle: &Path) -> Option<String> {
        None
    }

    // A container started without systemd has no user manager to ask.
    fn scheduler(&self) -> Option<&dyn Scheduler> {
        systemd::started_by_systemd(Path::new("/")).then_some(&self.scheduler as &dyn Scheduler)
    }

    fn elevation(&self, ctx: &Context) -> Elevation {
        user::linux_elevation(ctx.sudo(), &crate::home::written(ctx))
    }

    fn floor(&self) -> Floor {
        Floor::Met
    }

    /// Linux has no managed preferences: an administrator sets things in files.
    fn managed_preference(&self, _domain: &str, _key: &str) -> Administered {
        Administered::Unset
    }

    fn kernel_is_wsl(&self) -> bool {
        super::administered::READ_BY_REAL_HOSTS && kernel_names_microsoft()
    }

    fn on_a_windows_drive(&self, path: &Path) -> bool {
        on_9p(path)
    }

    fn watches(&self, dir: &Path) -> bool {
        super::unix::file_system(dir).is_none_or(|stats| !written_elsewhere(magic(&stats)))
    }
}

fn kernel_names_microsoft() -> bool {
    std::fs::read_to_string("/proc/version")
        .is_ok_and(|version| version.to_lowercase().contains("microsoft"))
}

fn on_9p(path: &Path) -> bool {
    super::unix::file_system(path).is_some_and(|stats| magic(&stats) == V9FS_MAGIC)
}

// `<linux/magic.h>`'s numbers are 32 bits, which `f_type` holds in a type each C library
// chooses.
fn magic(stats: &libc::statfs) -> u32 {
    stats.f_type as u32
}

// From `<linux/magic.h>`, as the next three are, which the libc crate leaves out.
const V9FS_MAGIC: u32 = 0x0102_1997;
const CIFS_SUPER_MAGIC: u32 = 0xFF53_4D42;
const SMB2_SUPER_MAGIC: u32 = 0xFE53_4D42;
const AFS_FS_MAGIC: u32 = 0x6B41_4653;
const CEPH_SUPER_MAGIC: u32 = 0x00C3_6400;

// inotify(7): it "does not catch remote events that occur on network filesystems". So a write
// another machine makes in a folder on one goes unheard: on NFS, SMB, either AFS, Ceph, Coda
// or OCFS2's shared disk, on 9p, where WSL 2 mounts Windows' drives, and on FUSE, whose
// daemon may serve another machine's files, as sshfs does. There Pitboard's directory is
// looked at instead.
fn written_elsewhere(magic: u32) -> bool {
    [
        libc::NFS_SUPER_MAGIC as u32,
        libc::SMB_SUPER_MAGIC as u32,
        CIFS_SUPER_MAGIC,
        SMB2_SUPER_MAGIC,
        libc::AFS_SUPER_MAGIC as u32,
        AFS_FS_MAGIC,
        CEPH_SUPER_MAGIC,
        libc::CODA_SUPER_MAGIC as u32,
        libc::OCFS2_SUPER_MAGIC as u32,
        V9FS_MAGIC,
        libc::FUSE_SUPER_MAGIC as u32,
    ]
    .contains(&magic)
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn write_jar(_path: &Path, _cookies: &[super::Cookie]) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

pub(super) fn host() -> Arc<dyn Host> {
    Arc::new(Linux {
        scheduler: systemd::Systemd::new(service::system()),
    })
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
        .and_then(|named| {
            super::program::find(Path::new(named), search, &super::program::Lookup::here())
        })
        .filter(|found| resolved.is_some() && std::fs::canonicalize(found).ok() == resolved)
        .unwrap_or(running)
}

/// systemd as a machine in memory has it: real unit files in the test's own home, and a
/// service manager that asks nobody.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn pretend_scheduler(
    refuse_start: Arc<std::sync::atomic::AtomicBool>,
) -> Option<Box<dyn Scheduler>> {
    Some(Box::new(systemd::Systemd::new(Arc::new(
        service::Pretend { refuse_start },
    ))))
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    // Read only. A scratch folder is in this Linux, whether or not it runs in WSL.
    #[test]
    fn the_kernel_and_a_folders_file_system_are_read_as_codex_reads_them() {
        assert!(std::fs::read_to_string("/proc/version").is_ok());
        let _ = kernel_names_microsoft();
        let scratch = tempfile::tempdir().expect("a scratch folder");
        assert!(!on_9p(scratch.path()));
        assert!(!on_9p(&scratch.path().join("not").join("made")));
        assert!(
            host().watches(scratch.path()),
            "watched where it is not on 9p"
        );
    }

    /// inotify hears only what this machine's own programs do through its file system, so a
    /// folder another machine writes is looked at instead. The numbers are `<linux/magic.h>`'s
    /// at Linux 6.17.
    #[test]
    fn a_file_system_another_machine_writes_is_not_watched() {
        for (name, magic) in [
            ("NFS_SUPER_MAGIC", 0x6969),
            ("SMB_SUPER_MAGIC", 0x517B),
            ("CIFS_SUPER_MAGIC", 0xFF53_4D42),
            ("SMB2_SUPER_MAGIC", 0xFE53_4D42),
            ("AFS_SUPER_MAGIC", 0x5346_414F),
            ("AFS_FS_MAGIC", 0x6B41_4653),
            ("CEPH_SUPER_MAGIC", 0x00C3_6400),
            ("CODA_SUPER_MAGIC", 0x7375_7245),
            ("OCFS2_SUPER_MAGIC", 0x7461_636F),
            ("V9FS_MAGIC", 0x0102_1997),
            ("FUSE_SUPER_MAGIC", 0x6573_5546),
        ] {
            assert!(written_elsewhere(magic), "{name}");
        }
        for (name, magic) in [
            ("EXT4_SUPER_MAGIC", 0xEF53),
            ("BTRFS_SUPER_MAGIC", 0x9123_683E),
            ("XFS_SUPER_MAGIC", 0x5846_5342),
            ("TMPFS_MAGIC", 0x0102_1994),
            ("OVERLAYFS_SUPER_MAGIC", 0x794C_7630),
            ("BCACHEFS_SUPER_MAGIC", 0xCA45_1A4E),
            ("F2FS_SUPER_MAGIC", 0xF2F5_2010),
        ] {
            assert!(!written_elsewhere(magic), "{name}");
        }
    }

    /// Homebrew starts Pitboard through a link in its `bin` that leads into a directory
    /// named after the version, and the next upgrade deletes that directory. Linux says
    /// where the running program is with every link resolved, so the schedule is given the
    /// path Pitboard was started by instead, wherever that leads to this same program.
    #[test]
    fn the_command_line_is_known_by_the_path_it_was_started_by() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let runnable = |path: &Path| {
            std::fs::create_dir_all(path.parent().unwrap()).expect("its directory");
            std::fs::write(path, "").expect("a program");
            fs::testing::make_runnable(path).expect("runnable");
        };
        let running = home.join("Caskroom/pitboard/0.4.0/pitboard");
        runnable(&running);
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).expect("a bin");
        let link = bin.join("pitboard");
        fs::testing::link(&running, &link).expect("a link");
        let another = home.join("elsewhere/pitboard");
        runnable(&another);

        let started = |argv0: Option<&Path>, search: &Path| {
            started_as(
                argv0.map(Path::as_os_str),
                search.as_os_str(),
                running.clone(),
            )
        };
        let name = Path::new("pitboard");
        let nowhere = Path::new("");
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
    }
}
