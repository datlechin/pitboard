//! Files only their owner can reach, the POSIX way: a mode, set when the file is created so
//! it is never open for a moment, whatever the umask. The face creates each such file itself
//! and hands it back open, rather than setting an option on a file its caller then opens, so
//! how a file is made private is said here and nowhere else, and a face can refuse to make
//! one.
//!
//! Every way Pitboard changes the disk outside [`crate::atomic::write`] is here too, and
//! each takes the [`Permit`] only the one gate every change passes makes: creating a file
//! or a directory, giving one an access or a time, and moving, copying or removing one. So
//! nothing removes or makes a file without having asked whether this process may.

use crate::host::{Access, Private};
use crate::service::Permit;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::SystemTime;

/// Create `path` and any missing parent so only the owner can reach them: 0700. A directory
/// that already exists keeps its mode: it may be one the person named, and `pitboard
/// doctor` reports it if others can read it.
pub(crate) fn create_private_dir(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

/// A new file at `path`, open for writing, that only its owner can read or write: 0600.
/// Fails where anything is there already, a link included, so nothing planted there is
/// written through.
pub(crate) fn create_private(_: Permit, path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// The file at `path`, open to add to its end, made where it is not there so only its owner
/// can read or write it: 0600. A file already there keeps its own mode.
pub(crate) fn open_private_append(_: Permit, path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

/// The file at `path`, open for writing and left as it is, made where it is not there so
/// only its owner can read or write it: 0600. What a lock is taken on: nothing is written to
/// it, and a file already there keeps its own mode.
pub(crate) fn open_private_lock(_: Permit, path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(path)
}

/// Give `temp` exactly the access the file at `existing` has, tighter or looser than
/// private, so replacing another program's file neither opens nor closes it. Nothing
/// changes where nothing is there, and where `existing` is a link: a link's own mode says
/// nothing about its target, and the rename that follows replaces the link rather than
/// following it, as Claude Code's own writes do.
pub(crate) fn copy_access(_: Permit, existing: &Path, temp: &Path) -> io::Result<()> {
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
    let _ = flush_dir(dir);
}

pub(crate) fn flush_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

fn c_path(path: &Path) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
}

fn not_replacing(renamed: libc::c_int) -> io::Result<()> {
    if renamed == 0 {
        Ok(())
    } else {
        Err(refusal(io::Error::last_os_error()))
    }
}

fn refusal(refused: io::Error) -> io::Error {
    match refused.raw_os_error() {
        Some(libc::EEXIST) => io::Error::new(io::ErrorKind::AlreadyExists, refused),
        // a volume that cannot refuse to replace is refused, never renamed over
        Some(libc::ENOTSUP | libc::EINVAL | libc::ENOSYS) => {
            io::Error::new(io::ErrorKind::Unsupported, refused)
        }
        _ => refused,
    }
}

// The kernel's own never-replace rename: what is at `to`, a dangling link included, stays.
pub(crate) fn move_new(_: Permit, from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (c_path(from)?, c_path(to)?);
    not_replacing(crate::host::os::rename_exclusive(&from, &to))
}

fn nearest(path: &Path) -> io::Result<std::fs::Metadata> {
    let mut at = path;
    loop {
        match std::fs::metadata(at) {
            Ok(found) => return Ok(found),
            Err(e) if e.kind() == io::ErrorKind::NotFound => match at.parent() {
                Some(parent) => at = parent,
                None => return Err(e),
            },
            Err(e) => return Err(e),
        }
    }
}

// Each asked of its nearest ancestor that exists.
pub(crate) fn same_volume(a: &Path, b: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    Ok(nearest(a)?.dev() == nearest(b)?.dev())
}

// Short names and sync roots are Windows'.
pub(crate) fn short_path(_path: &Path) -> Option<std::path::PathBuf> {
    None
}

pub(crate) fn in_a_sync_root(_path: &Path) -> bool {
    false
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn file_id(path: &Path) -> Option<(u64, u128)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .map(|it| (it.dev(), u128::from(it.ino())))
        .ok()
}

/// Set the modification time of the directory at `path` to `at`.
pub(crate) fn touch_dir(_: Permit, path: &Path, at: SystemTime) -> io::Result<()> {
    File::open(path)?.set_modified(at)
}

/// Make the directory `path`, and any missing parent, with the access the umask gives: for a
/// directory another program keeps, which it made its own way.
pub(crate) fn create_dir_all(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Make the one directory `path`, which fails where something is there already: how a lock
/// made of a directory is taken.
pub(crate) fn create_dir(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::create_dir(path)
}

/// Copy the file at `from` to `to`.
pub(crate) fn copy(_: Permit, from: &Path, to: &Path) -> io::Result<u64> {
    std::fs::copy(from, to)
}

/// Move the file at `from` to `to`, over whatever is there.
pub(crate) fn rename(_: Permit, from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}

/// Remove the file at `path`, or the link there, never what a link leads to.
pub(crate) fn remove_file(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

/// Remove the empty directory at `path`.
pub(crate) fn remove_dir(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::remove_dir(path)
}

/// Remove the directory at `path` and everything in it.
pub(crate) fn remove_dir_all(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::remove_dir_all(path)
}

pub(crate) fn private_dir(path: &Path) -> io::Result<Private> {
    use std::os::unix::fs::MetadataExt;
    let found = match std::fs::symlink_metadata(path) {
        Ok(found) => found,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Private::Missing),
        Err(e) => return Err(e),
    };
    // SAFETY: geteuid only reports this process's effective user id.
    let me = unsafe { libc::geteuid() };
    Ok(if found.file_type().is_symlink() {
        Private::Link
    } else if !found.is_dir() {
        Private::NotADirectory
    } else if found.uid() != me {
        Private::NotOwn
    } else if found.mode() & 0o077 != 0 {
        Private::Shared
    } else {
        Private::Yes
    })
}

// An advisory lock on a read-only descriptor, never waited for: nothing is written there.
pub(crate) fn lock_dir(path: &Path) -> io::Result<File> {
    let folder = File::open(path)?;
    match folder.try_lock() {
        Ok(()) => Ok(folder),
        Err(std::fs::TryLockError::WouldBlock) => Err(io::ErrorKind::WouldBlock.into()),
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Who besides its owner can reach `path`. `None` where it cannot be looked at.
pub(crate) fn access(path: &Path) -> Option<Access> {
    let mode = std::fs::metadata(path).ok()?.permissions().mode() & 0o777;
    Some(Access {
        // Group and other, read, write or search. Anything there is somebody who is not
        // the owner.
        shared: mode & 0o077 != 0,
        described: format!("mode {mode:o}"),
    })
}

/// What a test or a fixture does to the disk to set up a machine the way a person, an
/// installer or another program might have left it, and what it reads back of a file's
/// access. Nothing outside a host's face sets a file's mode, makes a file runnable or makes a
/// link but through here, so a system whose files have no mode, or that makes a link to a
/// directory otherwise than one to a file, says how in its own face's `testing` and nowhere
/// else. Other crates reach it as `pitboard_core::testing::fs`.
///
/// Each says what went wrong rather than panicking, since a fixture is made in an app as well
/// as in a test, and an app that cannot make one says so.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(feature = "test-support"), allow(unreachable_pub))]
pub mod testing {
    use std::io;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn set(path: &Path, mode: u32) -> io::Result<()> {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }

    /// A file anybody may read, as a copy or a careless umask leaves one: 644.
    pub fn open_to_others(path: &Path) -> io::Result<()> {
        set(path, 0o644)
    }

    /// A file or a directory only its owner may reach, as Pitboard makes its own: 600 for a
    /// file and 700 for a directory.
    pub fn make_private(path: &Path) -> io::Result<()> {
        set(path, private_mode(path)?)
    }

    /// Whether `path` is private as Pitboard makes its own files and directories: exactly
    /// what [`make_private`] gives it, so its owner may still read and change it.
    pub fn is_private(path: &Path) -> io::Result<bool> {
        let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
        Ok(mode == private_mode(path)?)
    }

    fn private_mode(path: &Path) -> io::Result<u32> {
        Ok(if std::fs::metadata(path)?.is_dir() {
            0o700
        } else {
            0o600
        })
    }

    /// A file its owner may read and nobody may change: 400.
    pub fn read_only_for_owner(path: &Path) -> io::Result<()> {
        set(path, 0o400)
    }

    /// A file nobody may read or change, its owner included: 000. The system's administrator
    /// still may, so a test that needs it unread checks that it is.
    pub fn deny_reading(path: &Path) -> io::Result<()> {
        set(path, 0o000)
    }

    /// A file anybody may run: 755.
    pub fn make_runnable(path: &Path) -> io::Result<()> {
        set(path, 0o755)
    }

    /// A file nobody may run, which anybody may still read: 644, as a copy that lost a
    /// program's modes leaves one.
    pub fn deny_running(path: &Path) -> io::Result<()> {
        set(path, 0o644)
    }

    /// A directory nothing can be added to or removed from: 555.
    pub fn deny_changes(dir: &Path) -> io::Result<()> {
        set(dir, 0o555)
    }

    /// A directory its owner may change again: 755.
    pub fn allow_changes(dir: &Path) -> io::Result<()> {
        set(dir, 0o755)
    }

    /// A link at `at` that leads to `target`, a file or nothing at all, as an installer or a
    /// person makes one. A relative `target` is read from the link's own directory. A link to
    /// a directory is [`link_dir`]'s.
    pub fn link(target: &Path, at: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(target, at)
    }

    /// A link at `at` that leads to the directory `target`. Apart from [`link`], since not
    /// every system makes a link to a directory the way it makes one to a file. On macOS and
    /// Linux both are the same symbolic link.
    pub fn link_dir(target: &Path, at: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(target, at)
    }

    // What a rename keeps and a copy does not: how a test tells a move from a copy.
    pub fn inode(path: &Path) -> io::Result<u64> {
        use std::os::unix::fs::MetadataExt;
        Ok(std::fs::symlink_metadata(path)?.ino())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_folder_is_private_only_where_it_is_this_users_own_directory_closed_to_others() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().join("dir");
        assert_eq!(private_dir(&dir).unwrap(), Private::Missing);
        create_private_dir(Permit::for_a_test(), &dir).unwrap();
        assert_eq!(private_dir(&dir).unwrap(), Private::Yes);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o750)).unwrap();
        assert_eq!(private_dir(&dir).unwrap(), Private::Shared);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let link = dir.with_extension("link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        assert_eq!(private_dir(&link).unwrap(), Private::Link);
        std::fs::write(dir.join("file"), b"").unwrap();
        assert_eq!(
            private_dir(&dir.join("file")).unwrap(),
            Private::NotADirectory
        );
        use std::os::unix::fs::MetadataExt;
        let mine = std::fs::metadata(&dir).unwrap().uid();
        if std::fs::metadata("/").unwrap().uid() != mine {
            assert_eq!(private_dir(Path::new("/")).unwrap(), Private::NotOwn);
        }
        std::fs::remove_file(&link).unwrap();
    }

    #[test]
    fn a_rename_never_lands_on_something_already_there() {
        use std::os::unix::fs::MetadataExt;
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().join("dir");
        let permit = Permit::for_a_test();
        create_private_dir(permit, &dir).unwrap();
        let (from, to) = (dir.join("from"), dir.join("to"));
        std::fs::write(&from, b"incoming").unwrap();
        std::fs::write(&to, b"already there").unwrap();
        let refused = move_new(permit, &from, &to).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::AlreadyExists, "{refused}");
        assert_eq!(std::fs::read(&to).unwrap(), b"already there");
        assert_eq!(std::fs::read(&from).unwrap(), b"incoming");

        std::fs::remove_file(&to).unwrap();
        testing::link(&dir.join("nowhere"), &to).unwrap();
        let refused = move_new(permit, &from, &to).unwrap_err();
        assert_eq!(
            refused.kind(),
            io::ErrorKind::AlreadyExists,
            "a dangling link"
        );
        assert!(std::fs::symlink_metadata(&to).unwrap().is_symlink());
        assert!(!dir.join("nowhere").exists());
        std::fs::remove_file(&to).unwrap();

        let (dir_from, empty, full) = (dir.join("dir-from"), dir.join("empty"), dir.join("full"));
        std::fs::create_dir(&dir_from).unwrap();
        std::fs::create_dir(&empty).unwrap();
        std::fs::create_dir(&full).unwrap();
        std::fs::write(full.join("kept"), b"kept").unwrap();
        for there in [&empty, &full] {
            let refused = move_new(permit, &dir_from, there).unwrap_err();
            assert_eq!(refused.kind(), io::ErrorKind::AlreadyExists, "{refused}");
        }
        assert!(dir_from.is_dir());
        assert_eq!(std::fs::read(full.join("kept")).unwrap(), b"kept");

        let inode = std::fs::metadata(&from).unwrap().ino();
        move_new(permit, &from, &to).unwrap();
        assert_eq!(
            std::fs::metadata(&to).unwrap().ino(),
            inode,
            "a move keeps the inode"
        );
        assert!(!from.exists());
    }

    #[test]
    fn a_volume_that_cannot_refuse_is_refused_not_renamed_over() {
        for errno in [libc::ENOTSUP, libc::EINVAL, libc::ENOSYS] {
            assert_eq!(
                refusal(io::Error::from_raw_os_error(errno)).kind(),
                io::ErrorKind::Unsupported,
                "{errno}"
            );
        }
        assert_eq!(
            refusal(io::Error::from_raw_os_error(libc::EEXIST)).kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(not_replacing(0).is_ok());
    }

    #[test]
    fn one_scratch_folder_is_one_volume_and_a_missing_path_asks_its_parent() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().join("dir");
        create_private_dir(Permit::for_a_test(), &dir).unwrap();
        assert!(same_volume(&dir, &dir.join("not/made/yet")).unwrap());
        // devfs on macOS, devtmpfs on Linux: never the volume a scratch folder is on
        assert!(!same_volume(&dir, Path::new("/dev")).unwrap());
        assert!(flush_dir(&dir).is_ok());
        assert!(flush_dir(&dir.join("missing")).is_err());
    }

    #[test]
    fn a_locked_folder_refuses_a_second_lock() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().join("dir");
        create_private_dir(Permit::for_a_test(), &dir).unwrap();
        let before: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        let held = lock_dir(&dir).unwrap();
        assert_eq!(
            lock_dir(&dir).map(drop).map_err(|e| e.kind()),
            Err(io::ErrorKind::WouldBlock)
        );
        held.unlock().unwrap();
        let again = lock_dir(&dir).unwrap();
        drop(again);
        assert!(before.is_empty());
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "nothing written"
        );
    }

    #[test]
    fn created_directories_are_private_and_existing_ones_keep_their_mode() {
        let folder = tempfile::tempdir().expect("a scratch folder");
        let scratch = folder.path().join("dir");
        let permit = Permit::for_a_test();
        create_private_dir(permit, &scratch.join("nested")).unwrap();
        assert_eq!(
            mode(&scratch),
            0o700,
            "a created parent must be private too"
        );
        assert_eq!(mode(&scratch.join("nested")), 0o700);

        std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o755)).unwrap();
        create_private_dir(permit, &scratch).unwrap();
        assert_eq!(
            mode(&scratch),
            0o755,
            "a directory the user named is theirs to set"
        );
    }

    /// Each way the face makes a file makes it private. One to add to or to lock that is
    /// there already is opened as it is, mode and contents, and a new one is refused where
    /// anything is there, a link included.
    #[test]
    fn a_file_created_private_is_private_however_the_umask_is_set() {
        use std::io::Write;
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().join("dir");
        let permit = Permit::for_a_test();
        create_private_dir(permit, &dir).unwrap();
        let (fresh, history, lock) = (
            dir.join(".state.json.1.0.pitboard"),
            dir.join("history.jsonl"),
            dir.join("state.lock"),
        );
        create_private(permit, &fresh).unwrap();
        open_private_append(permit, &history).unwrap();
        open_private_lock(permit, &lock).unwrap();
        for made in [&fresh, &history, &lock] {
            assert_eq!(
                access(made),
                Some(Access {
                    shared: false,
                    described: "mode 600".into()
                }),
                "{}",
                made.display()
            );
        }

        for there in [&history, &lock] {
            std::fs::write(there, "one\n").unwrap();
            testing::open_to_others(there).unwrap();
            assert!(access(there).unwrap().shared);
        }
        writeln!(open_private_append(permit, &history).unwrap(), "two").unwrap();
        open_private_lock(permit, &lock).unwrap();
        assert_eq!(std::fs::read_to_string(&history).unwrap(), "one\ntwo\n");
        assert_eq!(std::fs::read_to_string(&lock).unwrap(), "one\n");
        for there in [&history, &lock] {
            assert_eq!(
                mode(there),
                0o644,
                "a file already there keeps its own mode"
            );
        }

        assert!(create_private(permit, &fresh).is_err(), "a file is there");
        let planted = dir.join(".usage.json.1.0.pitboard");
        testing::link(&dir.join("elsewhere"), &planted).unwrap();
        assert!(create_private(permit, &planted).is_err(), "a link is there");
        assert!(
            !dir.join("elsewhere").exists(),
            "and nothing is written through it"
        );
    }

    /// What a test or a fixture sets up through `testing` is what each says, and a link leads
    /// where it was asked to.
    #[test]
    fn each_way_a_test_sets_a_file_up_does_what_it_says() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().join("dir");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("file");
        std::fs::write(&file, "").unwrap();
        let gives = |set: fn(&Path) -> io::Result<()>, expected: u32| {
            set(&file).unwrap();
            assert_eq!(mode(&file), expected);
        };
        gives(testing::open_to_others, 0o644);
        gives(testing::make_private, 0o600);
        gives(testing::read_only_for_owner, 0o400);
        gives(testing::deny_reading, 0o000);
        gives(testing::make_runnable, 0o755);
        gives(testing::deny_running, 0o644);
        assert!(!testing::is_private(&file).unwrap());
        testing::make_private(&file).unwrap();
        assert!(testing::is_private(&file).unwrap());

        let folder = dir.join("folder");
        std::fs::create_dir(&folder).unwrap();
        testing::make_private(&folder).unwrap();
        assert_eq!(mode(&folder), 0o700, "a directory's own private mode");
        assert!(testing::is_private(&folder).unwrap());
        testing::deny_changes(&folder).unwrap();
        assert_eq!(mode(&folder), 0o555);
        testing::allow_changes(&folder).unwrap();
        assert_eq!(mode(&folder), 0o755);
        assert!(!testing::is_private(&folder).unwrap());

        testing::link(Path::new("file"), &dir.join("to-file")).unwrap();
        testing::link_dir(&folder, &dir.join("to-folder")).unwrap();
        assert_eq!(
            std::fs::read_link(dir.join("to-file")).unwrap(),
            Path::new("file")
        );
        assert!(
            dir.join("to-file").is_file(),
            "read from the link's own directory"
        );
        assert!(dir.join("to-folder").is_dir());
    }
}
