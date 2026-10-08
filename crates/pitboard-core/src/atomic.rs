//! The one durable write, for every file that must survive an interrupted run: a private
//! temporary, its contents synced, renamed over the file, and the rename made durable as the
//! system allows. The directory must already exist: who may reach it is the caller's to
//! choose. It takes the [`Permit`] only the one gate every change passes makes.

use crate::host::fs;
use crate::service::Permit;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub enum Perms {
    /// Private to its owner, whatever the umask and whatever is already there.
    Secret,
    /// Exactly the access the file at the path has now, tighter or looser than private.
    /// `Secret` when nothing is there yet, or when the path is a symlink.
    MatchExisting,
}

pub fn write(permit: Permit, path: &Path, contents: &[u8], perms: Perms) -> io::Result<()> {
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
        // Created private and given the existing access afterwards: too closed for a moment
        // is safe, too open is not.
        let mut file = create_fresh(permit, &temp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        if let Perms::MatchExisting = perms {
            fs::copy_access(permit, path, &temp)?;
        }
        fs::rename(permit, &temp, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(permit, &temp);
        return result;
    }
    fs::sync_dir(dir);
    Ok(())
}

/// Numbers every temporary this process creates, so writers on different threads never
/// share one.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// The pid in a temporary name `write` creates, `.<file>.<pid>.<n>.pitboard`, or in the
/// `.<file>.<pid>.pitboard` of earlier versions; `None` for any other name.
fn temp_owner(file_name: &str) -> Option<u32> {
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
fn sweep(permit: Permit, dir: &Path) {
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
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pitboard-atomic-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    use crate::host::fs::testing;

    fn private(path: &Path) -> bool {
        !fs::access(path).expect("it can be looked at").shared
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_secret_is_private_however_the_umask_is_set() {
        let dir = scratch("secret");
        let path = dir.join("state.json");
        write(Permit::for_a_test(), &path, b"{}", Perms::Secret).unwrap();
        assert!(private(&path));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn replacing_leaves_no_temp_file_behind() {
        let dir = scratch("replace");
        let path = dir.join("f.json");
        write(Permit::for_a_test(), &path, b"one", Perms::Secret).unwrap();
        write(Permit::for_a_test(), &path, b"two", Perms::Secret).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn match_existing_keeps_a_file_as_open_as_it_already_was() {
        let dir = scratch("match");
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
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn match_existing_keeps_a_file_as_closed_as_it_already_was() {
        let dir = scratch("tighter");
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
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn match_existing_defaults_to_private_when_nothing_is_there() {
        let dir = scratch("fresh");
        let path = dir.join("new.json");
        write(Permit::for_a_test(), &path, b"{}", Perms::MatchExisting).unwrap();
        assert!(private(&path));
        std::fs::remove_dir_all(&dir).unwrap();
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
    #[cfg_attr(windows, ignore = "W18: seeing which processes run on Windows")]
    fn temporaries_left_by_runs_that_are_gone_are_removed() {
        let dir = scratch("orphans");
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
        let running = dir.join(".state.json.1.0.pitboard");
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
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_name_left_by_an_earlier_process_with_the_same_pid_is_reclaimed() {
        let dir = scratch("reused-pid");
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
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A library shares one process between callers: a status refresh writing usage.json
    /// must never disturb a switch writing state.json beside it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn writers_in_one_process_never_disturb_each_other() {
        let dir = scratch("threads");
        let writers: Vec<_> = (0..8)
            .map(|n| {
                let dir = dir.clone();
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
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(left.len(), 2, "no temporary may be left behind: {left:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_symlinked_destination_does_not_lend_its_permissions() {
        let dir = scratch("symlink");
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
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
