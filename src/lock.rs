//! The lock Claude Code takes around every credential write.
//!
//! Claude Code guards `<storage dir>/.storage-write` with proper-lockfile, whose lock is
//! a directory created by `mkdir`, kept alive by touching its mtime, and released by
//! `rmdir`. A lock older than `STALE` is treated as abandoned. Taking the same lock the
//! same way is the only thing that makes our writes mutually exclusive with Claude Code's.
//!
//! A process killed with SIGKILL leaves the directory behind; the staleness rule reclaims
//! it. Our critical section is a few milliseconds, so that window is not worth a signal
//! handler.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime};

const STALE: Duration = Duration::from_millis(15_000);
const HEARTBEAT: Duration = Duration::from_millis(7_500);
const RETRIES: u32 = 10;
const MIN_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_millis(1_000);

#[derive(Debug)]
pub enum LockError {
    Busy(PathBuf),
    Io(io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Busy(p) => write!(
                f,
                "another process is writing credentials ({} is held)",
                p.display()
            ),
            LockError::Io(e) => write!(f, "cannot take the credential write lock: {e}"),
        }
    }
}

pub struct Guard {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    beat: Option<thread::JoinHandle<()>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.beat.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_dir(&self.path);
    }
}

fn age(path: &Path) -> Option<Duration> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(mtime).ok()
}

fn touch(path: &Path) -> io::Result<()> {
    let now = libc::timeval {
        tv_sec: unsafe { libc::time(std::ptr::null_mut()) },
        tv_usec: 0,
    };
    let times = [now, now];
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL"))?;
    if unsafe { libc::utimes(c.as_ptr(), times.as_ptr()) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Take the lock guarding `target`, blocking for up to about six seconds.
pub fn acquire(target: &Path) -> Result<Guard, LockError> {
    let path = PathBuf::from(format!("{}.lock", target.display()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(LockError::Io)?;
    }

    let mut backoff = MIN_BACKOFF;
    for attempt in 0..=RETRIES {
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(start(path)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if age(&path).is_some_and(|a| a > STALE) {
                    let _ = std::fs::remove_dir(&path);
                    continue;
                }
            }
            Err(e) => return Err(LockError::Io(e)),
        }
        if attempt < RETRIES {
            thread::sleep(backoff);
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }
    Err(LockError::Busy(path))
}

fn start(path: PathBuf) -> Guard {
    let stop = Arc::new(AtomicBool::new(false));
    let beat = {
        let (path, stop) = (path.clone(), Arc::clone(&stop));
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                // Wake often so release is not delayed by a full heartbeat interval.
                for _ in 0..75 {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(HEARTBEAT / 75);
                }
                if touch(&path).is_err() {
                    return;
                }
            }
        })
    };
    Guard {
        path,
        stop,
        beat: Some(beat),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("pitboard-lock-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p.join("target")
    }

    #[test]
    fn acquires_and_releases() {
        let t = scratch("basic");
        let lock = t.with_extension("").parent().unwrap().join("target.lock");
        {
            let _g = acquire(&t).expect("should acquire");
            assert!(lock.is_dir(), "the lock directory should exist while held");
        }
        assert!(
            !lock.exists(),
            "dropping the guard must remove the directory"
        );
    }

    #[test]
    fn refuses_while_another_holder_is_alive() {
        let t = scratch("busy");
        let _held = acquire(&t).expect("first acquire");
        // The holder heartbeats, so this must exhaust its retries rather than steal it.
        assert!(matches!(acquire(&t), Err(LockError::Busy(_))));
    }

    #[test]
    fn reclaims_a_lock_left_behind_by_a_dead_process() {
        let t = scratch("stale");
        let lock = PathBuf::from(format!("{}.lock", t.display()));
        std::fs::create_dir_all(&lock).unwrap();
        let old = libc::timeval {
            tv_sec: unsafe { libc::time(std::ptr::null_mut()) } - 3600,
            tv_usec: 0,
        };
        let times = [old, old];
        let c = std::ffi::CString::new(lock.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::utimes(c.as_ptr(), times.as_ptr()) }, 0);
        let _g = acquire(&t).expect("a stale lock must be reclaimable");
    }

    #[test]
    fn heartbeat_keeps_the_lock_fresh() {
        let t = scratch("beat");
        let _g = acquire(&t).unwrap();
        let lock = PathBuf::from(format!("{}.lock", t.display()));
        thread::sleep(HEARTBEAT + Duration::from_millis(300));
        assert!(
            age(&lock).unwrap() < STALE,
            "the lock should have been touched"
        );
    }
}
