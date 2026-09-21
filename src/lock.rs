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
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

const STALE: Duration = Duration::from_millis(15_000);
const HEARTBEAT: Duration = Duration::from_millis(7_500);
const RETRIES: u32 = 10;
const MIN_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_millis(1_000);

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("another process is writing credentials right now; try again in a few seconds")]
    Busy(PathBuf),
    #[error("cannot take the credential write lock: {0}")]
    Io(#[source] io::Error),
}

/// Set to true and signalled to stop the heartbeat; the condition variable is what makes
/// release immediate instead of waiting out the current interval.
type Stop = Arc<(Mutex<bool>, Condvar)>;

impl LockError {
    pub fn code(&self) -> &'static str {
        match self {
            LockError::Busy(_) => "switch_in_progress",
            LockError::Io(_) => "lock_unavailable",
        }
    }
}

pub struct Guard {
    path: PathBuf,
    stop: Stop,
    beat: Option<thread::JoinHandle<()>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let (flag, wake) = &*self.stop;
        *flag.lock().unwrap_or_else(|e| e.into_inner()) = true;
        wake.notify_all();
        if let Some(handle) = self.beat.take() {
            let _ = handle.join();
        }
        let _ = std::fs::remove_dir(&self.path);
    }
}

fn age(path: &Path) -> Option<Duration> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(mtime).ok()
}

fn touch(path: &Path) -> io::Result<()> {
    filetime::set_file_mtime(path, filetime::FileTime::now())
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
    let stop: Stop = Arc::new((Mutex::new(false), Condvar::new()));
    let beat = {
        let (path, stop) = (path.clone(), Arc::clone(&stop));
        thread::spawn(move || {
            let (flag, wake) = &*stop;
            let mut stopped = flag.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                // Checked before waiting: a guard dropped before this thread reaches the
                // wait would otherwise signal into an empty room and be missed entirely.
                if *stopped {
                    return;
                }
                let (next, timeout) = wake
                    .wait_timeout(stopped, HEARTBEAT)
                    .unwrap_or_else(|e| e.into_inner());
                stopped = next;
                if *stopped {
                    return;
                }
                // Waiting on a deadline rather than sleeping in slices keeps the interval
                // exact: seventy-five chained sleeps drifted the first beat to 7.9 seconds.
                if timeout.timed_out() && touch(&path).is_err() {
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

    /// Backdate a lock so only a live heartbeat could rescue it.
    fn age_past_staleness(lock: &Path) {
        let stale =
            filetime::FileTime::from_unix_time(filetime::FileTime::now().unix_seconds() - 3600, 0);
        filetime::set_file_mtime(lock, stale).unwrap();
    }

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
        age_past_staleness(&lock);
        let _g = acquire(&t).expect("a stale lock must be reclaimable");
    }

    /// Dropping must not wait out the current heartbeat interval. The previous design
    /// slept in slices and could hold the lock up to a slice longer than needed.
    #[test]
    fn releasing_is_immediate() {
        let t = scratch("release");
        let started = std::time::Instant::now();
        drop(acquire(&t).unwrap());
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "release took {:?}",
            started.elapsed()
        );
        assert!(!PathBuf::from(format!("{}.lock", t.display())).exists());
    }

    /// Ages the lock past staleness first. A test that merely checks the lock is fresh
    /// passes identically when no heartbeat is running at all.
    #[test]
    fn the_heartbeat_rescues_a_lock_that_has_aged_out() {
        let t = scratch("beat");
        let _g = acquire(&t).unwrap();
        let lock = PathBuf::from(format!("{}.lock", t.display()));

        age_past_staleness(&lock);
        assert!(
            age(&lock).unwrap() > STALE,
            "the lock should start out stale"
        );

        thread::sleep(HEARTBEAT + Duration::from_millis(400));
        assert!(
            age(&lock).unwrap() < STALE,
            "the heartbeat never touched the lock"
        );
    }
}
