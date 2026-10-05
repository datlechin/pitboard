//! Files only their owner can reach, the POSIX way: a mode, set when the file is created so
//! it is never open for a moment, whatever the umask.

use crate::host::Access;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::SystemTime;

/// Create `path` and any missing parent so only the owner can reach them: 0700. A directory
/// that already exists keeps its mode: it may be one the person named, and `pitboard
/// doctor` reports it if others can read it.
pub(crate) fn create_private_dir(path: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

/// `options`, made to create a file only its owner can read and write: 0600. A file that is
/// already there keeps its own mode.
pub(crate) fn private(options: &mut OpenOptions) -> &mut OpenOptions {
    options.mode(0o600)
}

/// Give `temp` exactly the access the file at `existing` has, tighter or looser than
/// private, so replacing another program's file neither opens nor closes it. Nothing
/// changes where nothing is there, and where `existing` is a link: a link's own mode says
/// nothing about its target, and the rename that follows replaces the link rather than
/// following it, as Claude Code's own writes do.
pub(crate) fn copy_access(existing: &Path, temp: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(existing) {
        Ok(found) if !found.file_type().is_symlink() => {
            let mode = found.permissions().mode() & 0o777;
            std::fs::set_permissions(temp, std::fs::Permissions::from_mode(mode))
        }
        _ => Ok(()),
    }
}

/// Make a rename in `dir` durable. ext4 and xfs can lose a rename across a crash even when
/// the file's contents were synced. Best effort: the contents are durable already.
pub(crate) fn sync_dir(dir: &Path) {
    let _ = File::open(dir).and_then(|d| d.sync_all());
}

/// Set the modification time of the directory at `path` to `at`.
pub(crate) fn touch_dir(path: &Path, at: SystemTime) -> io::Result<()> {
    File::open(path)?.set_modified(at)
}

/// Who besides its owner can reach `path`. `None` where it cannot be looked at.
pub fn access(path: &Path) -> Option<Access> {
    let mode = std::fs::metadata(path).ok()?.permissions().mode() & 0o777;
    Some(Access {
        // Group and other, read, write or search. Anything there is somebody who is not
        // the owner.
        shared: mode & 0o077 != 0,
        described: format!("mode {mode:o}"),
    })
}

/// The device the file at `path` is on, as its metadata says: two places on one device can
/// be renamed between, and two on different ones cannot.
pub(crate) fn device(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).map(|found| found.dev())
}

fn c_path(path: &Path) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
}

/// A rename the kernel itself refuses when `to` exists, so nothing that appears between a
/// caller's check that `to` is free and the rename is replaced. Where the volume cannot do
/// that, the caller's check is what stands, as it is on a system with no such call.
#[cfg(target_vendor = "apple")]
pub(crate) fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    let (c_from, c_to) = (c_path(from)?, c_path(to)?);
    // SAFETY: both pointers are to NUL-terminated strings that outlive the call, which
    // reads them and writes no memory of this process.
    let renamed = unsafe { libc::renamex_np(c_from.as_ptr(), c_to.as_ptr(), libc::RENAME_EXCL) };
    if renamed == 0 {
        return Ok(());
    }
    match io::Error::last_os_error() {
        e if matches!(e.raw_os_error(), Some(libc::EINVAL | libc::ENOTSUP)) => {
            std::fs::rename(from, to)
        }
        e => Err(e),
    }
}

#[cfg(all(target_os = "linux", any(target_env = "gnu", target_env = "musl")))]
pub(crate) fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    let (c_from, c_to) = (c_path(from)?, c_path(to)?);
    // SAFETY: both pointers are to NUL-terminated strings that outlive the call, which
    // reads them and writes no memory of this process; `AT_FDCWD` names no descriptor.
    let renamed = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            c_from.as_ptr(),
            libc::AT_FDCWD,
            c_to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if renamed == 0 {
        return Ok(());
    }
    match io::Error::last_os_error() {
        e if matches!(
            e.raw_os_error(),
            Some(libc::EINVAL | libc::ENOSYS | libc::EOPNOTSUPP)
        ) =>
        {
            std::fs::rename(from, to)
        }
        e => Err(e),
    }
}

#[cfg(not(any(
    target_vendor = "apple",
    all(target_os = "linux", any(target_env = "gnu", target_env = "musl"))
)))]
pub(crate) fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    let _ = c_path;
    std::fs::rename(from, to)
}

/// What a test does to a file's access to set up a machine, the way a person or another
/// program might have left it.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    fn set(path: &Path, mode: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .unwrap_or_else(|e| panic!("{} could not be changed: {e}", path.display()));
    }

    /// A file anybody may read, as a copy or a careless umask leaves one.
    pub(crate) fn open_to_others(path: &Path) {
        set(path, 0o644);
    }

    /// A file only its owner may read or write.
    pub(crate) fn make_private(path: &Path) {
        set(path, 0o600);
    }

    /// A file its owner may read and nobody may change.
    pub(crate) fn read_only_for_owner(path: &Path) {
        set(path, 0o400);
    }

    /// A file anybody may run.
    pub(crate) fn make_runnable(path: &Path) {
        set(path, 0o755);
    }

    /// A directory nothing can be added to or removed from.
    pub(crate) fn deny_changes(dir: &Path) {
        set(dir, 0o555);
    }

    /// A directory its owner may change again.
    pub(crate) fn allow_changes(dir: &Path) {
        set(dir, 0o755);
    }

    /// A link at `link` that leads to `target`.
    pub(crate) fn link(target: &Path, link: &Path) {
        std::os::unix::fs::symlink(target, link)
            .unwrap_or_else(|e| panic!("{} could not be linked: {e}", link.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-host-fs-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn created_directories_are_private_and_existing_ones_keep_their_mode() {
        let scratch = scratch("dirs");
        create_private_dir(&scratch.join("nested")).unwrap();
        assert_eq!(
            mode(&scratch),
            0o700,
            "a created parent must be private too"
        );
        assert_eq!(mode(&scratch.join("nested")), 0o700);

        std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o755)).unwrap();
        create_private_dir(&scratch).unwrap();
        assert_eq!(
            mode(&scratch),
            0o755,
            "a directory the user named is theirs to set"
        );
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    fn a_file_created_private_is_private_however_the_umask_is_set() {
        let dir = scratch("file");
        create_private_dir(&dir).unwrap();
        let path = dir.join("state.lock");
        private(OpenOptions::new().write(true).create(true))
            .open(&path)
            .unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(
            access(&path),
            Some(Access {
                shared: false,
                described: "mode 600".into()
            })
        );
        testing::open_to_others(&path);
        assert!(access(&path).unwrap().shared);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
