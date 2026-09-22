//! The lock Claude Code takes around credential writes: proper-lockfile's, a directory
//! created by `mkdir`, kept alive by touching its mtime, released by `rmdir`, and treated as
//! abandoned once older than `STALE`. Only the same lock taken the same way excludes Claude
//! Code. A process killed outright leaves the directory behind for staleness to reclaim.
//!
//! Measured in 2.1.278, against the storage layer in the installed build. Every constant
//! below is the one Claude Code passes: stale 15000, ten retries, 100ms to 1000ms of
//! backoff, and the heartbeat at half the staleness. A write under this lock is always a
//! read-modify-write: the read cache is dropped, the credential is read again inside the
//! lock, and a read that fails abandons the write rather than guessing. That is why a
//! session or a daemon holding an older idea of who is signed in cannot write it back over
//! a switch.
//!
//! Two things this lock does not give, and both matter more than the lock itself.
//!
//! Claude Code treats its own lock going missing as a warning and carries on writing, so
//! pitboard cannot expect the other side to stop when a lock is broken. Whatever pitboard
//! does about a compromised lock, it has to do alone.
//!
//! And one write path skips the lock entirely. A write can be marked as already inside the
//! lock without the lock being taken, which is what `/logout` does after it has retried for
//! its own 7.5 seconds: it deletes the credential with nothing held. Every other write,
//! the OAuth refresh included, waits or fails. So holding this lock makes a switch safe
//! against Claude Code writing underneath it, but not against a logout that gave up
//! waiting, which is why a switch reads the slot back rather than trusting its own write.
//!
//! The writers are a session and [`crate::daemon`], the supervisor that outlives sessions
//! and refreshes on a timer of its own. Both come through here.

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
    Busy,
    #[error("cannot take the credential write lock: {0}")]
    Io(#[source] io::Error),
}

/// Signalled to stop the heartbeat, so release does not wait out the current interval.
type Stop = Arc<(Mutex<bool>, Condvar)>;

impl LockError {
    pub fn code(&self) -> &'static str {
        match self {
            LockError::Busy => "switch_in_progress",
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

/// proper-lockfile renews the lock by setting the directory's mtime.
fn touch(path: &Path, at: SystemTime) -> io::Result<()> {
    std::fs::File::open(path)?.set_modified(at)
}

/// Take the lock guarding `target`, waiting up to about seven and a half seconds.
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
    Err(LockError::Busy)
}

fn start(path: PathBuf) -> Guard {
    let stop: Stop = Arc::new((Mutex::new(false), Condvar::new()));
    let beat = {
        let (path, stop) = (path.clone(), Arc::clone(&stop));
        thread::spawn(move || {
            let (flag, wake) = &*stop;
            let mut stopped = flag.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                // Checks the flag before waiting and after every wakeup, spurious or not.
                let (next, _) = wake
                    .wait_timeout_while(stopped, HEARTBEAT, |stop| !*stop)
                    .unwrap_or_else(|e| e.into_inner());
                stopped = next;
                if *stopped || touch(&path, SystemTime::now()).is_err() {
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
        touch(lock, SystemTime::now() - Duration::from_secs(3600)).unwrap();
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
        assert!(matches!(acquire(&t), Err(LockError::Busy)));
    }

    #[test]
    fn reclaims_a_lock_left_behind_by_a_dead_process() {
        let t = scratch("stale");
        let lock = PathBuf::from(format!("{}.lock", t.display()));
        std::fs::create_dir_all(&lock).unwrap();
        age_past_staleness(&lock);
        let _g = acquire(&t).expect("a stale lock must be reclaimable");
    }

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

    /// Aged past staleness first, so only a live heartbeat can bring it back.
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
