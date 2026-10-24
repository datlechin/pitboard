// A file replaced whole from a private temporary, in a folder the caller made with its own access.

use crate::host::fs;
use crate::permit::Permit;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) enum Perms {
    /// Private to its owner, whatever the umask and whatever is already there.
    Secret,
    /// Exactly the access the file at the path has now, tighter or looser than private.
    /// `Secret` when nothing is there yet, or when the path is a symlink.
    MatchExisting,
}

pub(crate) fn write(permit: Permit, path: &Path, contents: &[u8], perms: Perms) -> io::Result<()> {
    write_at(permit, path, contents, perms, "atomic.temp_written")
}

// For the status line, whose budget a flush can outlast: a power cut may undo the write.
pub(crate) fn write_unsynced(permit: Permit, path: &Path, contents: &[u8]) -> io::Result<()> {
    replace(
        permit,
        path,
        contents,
        Perms::Secret,
        "atomic.temp_written",
        false,
    )
}

// `write`, with a fault point named `at` between the temporary and its rename.
pub(crate) fn write_at(
    permit: Permit,
    path: &Path,
    contents: &[u8],
    perms: Perms,
    at: &'static str,
) -> io::Result<()> {
    replace(permit, path, contents, perms, at, true)
}

fn replace(
    permit: Permit,
    path: &Path,
    contents: &[u8],
    perms: Perms,
    at: &'static str,
    synced: bool,
) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    sweep(permit, dir);
    let temp = dir.join(format!(
        ".{}.{}.{}.pitboard",
        name.to_string_lossy(),
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| -> io::Result<()> {
        // Private first, given the existing access after: too closed for a moment is safe.
        let mut file = create_fresh(permit, &temp)?;
        file.write_all(contents)?;
        if synced {
            if crate::fault::fails("atomic.synced") {
                return Err(io::Error::other("a test failed this flush"));
            }
            file.sync_all()?;
        }
        // Closed before Windows opens it again to set its list and to rename it.
        drop(file);
        if let Perms::MatchExisting = perms {
            fs::copy_access(permit, path, &temp)?;
        }
        crate::fault::point(at);
        fs::rename(permit, &temp, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(permit, &temp);
        return result;
    }
    if synced {
        fs::sync_dir(dir);
    }
    Ok(())
}

// Any reader can find a file missing while NTFS replaces it (ARCHITECTURE.md).
pub(crate) fn read<T>(path: &Path, read: impl Fn(&Path) -> io::Result<T>) -> io::Result<T> {
    let mut again = 0;
    loop {
        match read(path) {
            Err(missing)
                if cfg!(windows) && missing.kind() == io::ErrorKind::NotFound && again < 5 =>
            {
                again += 1;
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            read => return read,
        }
    }
}

/// Numbers every temporary this process creates, so writers on different threads never
/// share one.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// The pid in a temporary name `write` creates, `.<file>.<pid>.<n>.pitboard`, or in the
/// `.<file>.<pid>.pitboard` of earlier versions; `None` for any other name.
pub(crate) fn temp_owner(file_name: &str) -> Option<u32> {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let (rest, last) = file_name
        .strip_prefix('.')?
        .strip_suffix(".pitboard")?
        .rsplit_once('.')?;
    let pid = match rest.rsplit_once('.') {
        Some((target, pid)) if digits(pid) && digits(last) && !target.is_empty() => pid,
        _ if digits(last) && !rest.is_empty() => last,
        _ => return None,
    };
    pid.parse().ok()
}

/// A new private file at `temp`. This process never reuses a name, so a file already there
/// was left by an earlier process that had the same pid, as happens in containers.
fn create_fresh(permit: Permit, temp: &Path) -> io::Result<File> {
    let create = || fs::create_private(permit, temp);
    match create() {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            remove_stale(permit, temp)?;
            create()
        }
        other => other,
    }
}

/// Only a regular file: `symlink_metadata` does not follow a link, so a planted link is
/// never removed in place of a file.
fn remove_stale(permit: Permit, path: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(path)?.file_type().is_file() {
        fs::remove_file(permit, path)
    } else {
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a non-file is in the way",
        ))
    }
}

/// Remove temporaries a killed run left in `dir`: each can hold a whole credential. Only a
/// dead process's are touched; this process's may be mid-write on another thread.
pub(crate) fn sweep(permit: Permit, dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let me = std::process::id();
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(temp_owner) else {
            continue;
        };
        if pid != me && !crate::host::proc::may_be_running(pid) {
            let _ = remove_stale(permit, &entry.path());
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    use crate::host::fs::testing;

    fn private(path: &Path) -> bool {
        !fs::access(path).expect("it can be looked at").shared
    }

    #[test]
    fn a_secret_is_private_however_the_umask_is_set() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let path = dir.join("state.json");
        write(Permit::for_a_test(), &path, b"{}", Perms::Secret).unwrap();
        assert!(private(&path));
    }

    #[test]
    fn a_read_looks_again_where_windows_shows_a_file_being_replaced_missing() {
        let misses = std::cell::Cell::new(2);
        let found = read(Path::new("state.json"), |_| match misses.get() {
            0 => Ok("whole"),
            left => {
                misses.set(left - 1);
                Err(io::Error::from(io::ErrorKind::NotFound))
            }
        });
        assert_eq!(found.is_ok(), cfg!(windows), "{found:?}");
        let gone = read(Path::new("state.json"), |_| {
            Err::<(), _>(io::Error::from(io::ErrorKind::NotFound))
        });
        assert_eq!(gone.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn replacing_leaves_no_temp_file_behind() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let path = dir.join("f.json");
        write(Permit::for_a_test(), &path, b"one", Perms::Secret).unwrap();
        write(Permit::for_a_test(), &path, b"two", Perms::Secret).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1);
    }

    // A temporary left behind can hold a whole login.
    #[test]
    fn a_write_whose_flush_fails_leaves_the_file_and_no_temporary() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let path = dir.join("state.json");
        write(Permit::for_a_test(), &path, b"one", Perms::Secret).unwrap();
        let failed = crate::fault::failing("atomic.synced", || {
            write(Permit::for_a_test(), &path, b"two", Perms::Secret)
        });
        assert!(failed.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"one");
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1);
    }

    #[test]
    fn match_existing_keeps_a_file_as_open_as_it_already_was() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let path = dir.join("claude.json");
        std::fs::write(&path, b"old").unwrap();
        testing::open_to_others(&path).expect("opened to others");
        let before = fs::access(&path);
        write(Permit::for_a_test(), &path, b"new", Perms::MatchExisting).unwrap();
        assert_eq!(
            fs::access(&path),
            before,
            "tightening another tool's file would surprise it"
        );
    }

    #[test]
    fn match_existing_keeps_a_file_as_closed_as_it_already_was() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let path = dir.join("claude.json");
        std::fs::write(&path, b"old").unwrap();
        testing::read_only_for_owner(&path).expect("read only");
        let before = fs::access(&path);
        write(Permit::for_a_test(), &path, b"new", Perms::MatchExisting).unwrap();
        assert_eq!(
            fs::access(&path),
            before,
            "loosening another tool's file is as wrong as tightening it"
        );
    }

    #[test]
    fn match_existing_defaults_to_private_when_nothing_is_there() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let path = dir.join("new.json");
        write(Permit::for_a_test(), &path, b"{}", Perms::MatchExisting).unwrap();
        assert!(private(&path));
    }

    #[test]
    fn only_the_exact_temporary_shape_is_recognised() {
        assert_eq!(temp_owner(".state.json.4242.17.pitboard"), Some(4242));
        assert_eq!(temp_owner("..claude.json.7.0.pitboard"), Some(7));
        assert_eq!(
            temp_owner(".state.json.4242.pitboard"),
            Some(4242),
            "0.1's shape"
        );
        assert_eq!(
            temp_owner("..claude.json.7.pitboard"),
            Some(7),
            "0.1's shape"
        );
        for other in [
            ".pitboard",
            "..pitboard",
            ".state.json.pitboard",
            ".state.json.12x.pitboard",
            ".4242.pitboard",
            "state.json.4242.pitboard",
            ".state.json.4242.pitboard.bak",
        ] {
            assert_eq!(temp_owner(other), None, "{other}");
        }
    }

    #[test]
    fn temporaries_left_by_runs_that_are_gone_are_removed() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let exited = {
            // This test's own program, which lists its tests and exits.
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--list")
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let pid = child.id();
            child.wait().unwrap();
            pid
        };
        let orphan = dir.join(format!(".state.json.{exited}.3.pitboard"));
        let old_orphan = dir.join(format!(".state.json.{exited}.pitboard"));
        let own = dir.join(format!(".usage.json.{}.9.pitboard", std::process::id()));
        // The system's own, always running: System's is 4 on Windows.
        let system = if cfg!(windows) { 4 } else { 1 };
        let running = dir.join(format!(".state.json.{system}.0.pitboard"));
        let unrelated = dir.join(".state.json.bak");
        for leftover in [&orphan, &old_orphan, &own, &running, &unrelated] {
            std::fs::write(leftover, b"token").unwrap();
        }

        write(
            Permit::for_a_test(),
            &dir.join("state.json"),
            b"{}",
            Perms::Secret,
        )
        .unwrap();

        assert!(!orphan.exists(), "a dead run's copy must not linger");
        assert!(!old_orphan.exists(), "nor one in 0.1's shape");
        assert!(
            own.exists(),
            "this process may be writing it on another thread"
        );
        assert!(
            running.exists(),
            "a pid that is running may still be writing"
        );
        assert!(
            unrelated.exists(),
            "only Pitboard's own temporaries are touched"
        );
    }

    #[test]
    fn a_name_left_by_an_earlier_process_with_the_same_pid_is_reclaimed() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let temp = dir.join(".state.json.1.0.pitboard");
        std::fs::write(&temp, b"stale token").unwrap();
        let mut file = create_fresh(Permit::for_a_test(), &temp).unwrap();
        file.write_all(b"new").unwrap();
        assert_eq!(std::fs::read(&temp).unwrap(), b"new");

        let link = dir.join(".usage.json.1.0.pitboard");
        testing::link(&dir.join("elsewhere"), &link).expect("a planted link");
        assert!(
            create_fresh(Permit::for_a_test(), &link).is_err(),
            "a planted link is never removed"
        );
    }

    /// A library shares one process between callers: a status refresh writing usage.json
    /// must never disturb a switch writing state.json beside it.
    #[test]
    fn writers_in_one_process_never_disturb_each_other() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let writers: Vec<_> = (0..8)
            .map(|n| {
                let dir = dir.to_path_buf();
                std::thread::spawn(move || {
                    for round in 0..50 {
                        let file = if (n + round) % 2 == 0 {
                            "state.json"
                        } else {
                            "usage.json"
                        };
                        write(
                            Permit::for_a_test(),
                            &dir.join(file),
                            format!("{n}-{round}").as_bytes(),
                            Perms::Secret,
                        )
                        .unwrap_or_else(|e| panic!("writer {n}, round {round}: {e}"));
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let left: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(left.len(), 2, "no temporary may be left behind: {left:?}");
    }

    #[test]
    fn a_symlinked_destination_does_not_lend_its_permissions() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let target = dir.join("elsewhere");
        std::fs::write(&target, b"x").unwrap();
        testing::open_to_others(&target).expect("opened to others");
        let path = dir.join("link.json");
        testing::link(&target, &path).expect("a link");

        write(Permit::for_a_test(), &path, b"{}", Perms::MatchExisting).unwrap();
        assert!(
            private(&path),
            "a link's own access says nothing about its target, and must never be borrowed"
        );
        assert!(
            !std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the rename replaces the link rather than writing through it"
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"x",
            "the link target must be untouched"
        );
    }
}
