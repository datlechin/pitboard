//! The locks Claude Code takes around its login: proper-lockfile's, a directory created by
//! `mkdir`, kept alive by touching its mtime, released by `rmdir`, and treated as abandoned
//! once older than its staleness. Only the same lock taken the same way excludes Claude
//! Code. A process killed outright leaves the directory behind for staleness to reclaim.
//!
//! The write lock was measured in 2.1.278, against the storage layer in the installed build.
//! Its constants are the ones Claude Code passes ([`WRITE`]): stale 15000, ten retries,
//! 100ms to 1000ms of backoff, and the heartbeat at half the staleness. A write under it is
//! always a read-modify-write: the read cache is dropped, the credential is read again inside
//! the lock, and a read that fails abandons the write rather than guessing. That is why a
//! session or a daemon holding an older idea of who is signed in cannot write it back over
//! a switch.
//!
//! Three things the write lock does not give, and all matter more than the lock itself.
//!
//! Claude Code treats its own lock going missing as a warning and carries on writing, so
//! Pitboard cannot expect the other side to stop when a lock is broken. Whatever Pitboard
//! does about a compromised lock, it has to do alone.
//!
//! One write path skips the lock entirely. A write can be marked as already inside the
//! lock without the lock being taken, which is what `/logout` does after it has retried for
//! its own 7.5 seconds: it deletes the credential with nothing held. Every other write,
//! the OAuth refresh's save included, waits or fails. So holding this lock makes a switch
//! safe against Claude Code writing underneath it, but not against a logout that gave up
//! waiting, which is why a switch reads the slot back rather than trusting its own write.
//!
//! And a refresh spends the refresh token stored outside it. Read in 2.1.294, a refresh
//! takes a lock of its own ([`REFRESH`]), reads the login again under it, sends the refresh
//! token, and takes the write lock only to save the answer, where the login stored still
//! holds the token it sent. Only the refresh lock keeps a refresh from spending a token
//! Pitboard is about to count on. So a switch holds it while it parks the outgoing login,
//! and `pitboard stow` while it keeps the login left in a file.
//!
//! The writers are a session and [`crate::daemon`], the supervisor that outlives sessions
//! and refreshes on a timer of its own. Both come through here.

use crate::service::Permit;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

/// How a lock is kept: how long it goes untouched before anybody may take it as abandoned,
/// and how often its holder touches it meanwhile, as Claude Code passes both for each of its
/// own.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    stale: Duration,
    heartbeat: Duration,
}

/// Claude Code's write lock, `.storage-write`, around every write of its login (the
/// register's `write_lock`): proper-lockfile touches it at half its staleness.
pub const WRITE: Timing = Timing {
    stale: Duration::from_millis(15_000),
    heartbeat: Duration::from_millis(7_500),
};

/// Claude Code's refresh lock, `.oauth_refresh.lock` and the legacy one beside it, around a
/// refresh of its login (the register's `refresh_lock`). Claude Code takes one over before
/// it is stale only from a holder its owner record proves gone, and Pitboard writes none, so
/// it waits on Pitboard's.
pub const REFRESH: Timing = Timing {
    stale: Duration::from_millis(60_000),
    heartbeat: Duration::from_millis(5_000),
};

const RETRIES: u32 = 10;
const MIN_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_millis(1_000);

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LockError {
    #[error(
        "another process is writing or renewing credentials right now; try again in a few seconds"
    )]
    Busy,
    #[error("cannot take the credential lock: {0}")]
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
    /// What lets the heartbeat touch the lock and its release remove it.
    permit: Permit,
    stop: Stop,
    beat: Option<thread::JoinHandle<()>>,
    /// Set by the heartbeat when the lock directory stopped being the one this guard took.
    compromised: Arc<AtomicBool>,
}

impl Guard {
    /// Whether this lock stopped being ours while we held it. A caller that has written
    /// something must not report success on a compromised lock: for the span between the
    /// two, Claude Code believed it held the lock too.
    pub fn compromised(&self) -> bool {
        self.compromised.load(Ordering::Acquire)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let (flag, wake) = &*self.stop;
        *flag.lock().unwrap_or_else(|e| e.into_inner()) = true;
        wake.notify_all();
        if let Some(handle) = self.beat.take() {
            let _ = handle.join();
        }
        // A compromised lock belongs to whoever reclaimed it. Removing it here would take
        // their lock away and leave the next writer colliding with them too.
        if !self.compromised() {
            let _ = crate::host::fs::remove_dir(self.permit, &self.path);
        }
    }
}

fn age(path: &Path) -> Option<Duration> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(mtime).ok()
}

fn mtime(path: &Path) -> io::Result<SystemTime> {
    std::fs::metadata(path)?.modified()
}

/// proper-lockfile renews the lock by setting the directory's mtime, and remembers what the
/// filesystem stored rather than what it was asked to store.
///
/// Measured on macOS 26: APFS keeps nanoseconds and keeps them approximately, returning a
/// value 18 to 60 nanoseconds from the one it was given. A check that compared against the
/// value it asked for would find a mismatch every single time and abandon every switch, so
/// the value read back is the only one worth remembering. It also makes a filesystem that
/// truncates, which a network home may, a case that needs no special handling at all.
fn touch(permit: Permit, path: &Path, at: SystemTime) -> io::Result<SystemTime> {
    crate::host::fs::touch_dir(permit, path, at)?;
    mtime(path)
}

/// Take the lock guarding `target`, kept as `timing` says, waiting up to about seven and a
/// half seconds.
pub fn acquire(permit: Permit, target: &Path, timing: Timing) -> Result<Guard, LockError> {
    let path = PathBuf::from(format!("{}.lock", target.display()));
    if let Some(parent) = path.parent() {
        crate::host::fs::create_dir_all(permit, parent).map_err(LockError::Io)?;
    }

    let mut backoff = MIN_BACKOFF;
    for attempt in 0..=RETRIES {
        match crate::host::fs::create_dir(permit, &path) {
            Ok(()) => return start(permit, path, timing).map_err(LockError::Io),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if abandoned(&path, timing) {
                    let _ = crate::host::fs::remove_dir(permit, &path);
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

/// Whether the lock at `path` has gone untouched past `timing`'s staleness, which is when
/// Claude Code would take it too.
fn abandoned(path: &Path, timing: Timing) -> bool {
    age(path).is_some_and(|a| a > timing.stale)
}

fn start(permit: Permit, path: PathBuf, timing: Timing) -> io::Result<Guard> {
    // What the filesystem actually stored for the directory we just made. Every later beat
    // compares against this, and replaces it with what it stores next.
    let mut held = mtime(&path)?;
    let stop: Stop = Arc::new((Mutex::new(false), Condvar::new()));
    let compromised = Arc::new(AtomicBool::new(false));
    let beat = {
        let (path, stop, compromised) = (path.clone(), Arc::clone(&stop), Arc::clone(&compromised));
        thread::spawn(move || {
            let (flag, wake) = &*stop;
            let mut stopped = flag.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                // Checks the flag before waiting and after every wakeup, spurious or not.
                let (next, _) = wake
                    .wait_timeout_while(stopped, timing.heartbeat, |stop| !*stop)
                    .unwrap_or_else(|e| e.into_inner());
                stopped = next;
                if *stopped {
                    return;
                }
                // Look before touching. A directory that is gone, or whose mtime is not the
                // one this guard last stored, is not this guard's lock any more: something
                // aged it out and took it, and is writing under it right now. Touching it
                // then would renew somebody else's lock.
                match mtime(&path) {
                    Ok(found) if found == held => {}
                    _ => {
                        compromised.store(true, Ordering::Release);
                        return;
                    }
                }
                match touch(permit, &path, SystemTime::now()) {
                    Ok(stored) => held = stored,
                    Err(_) => {
                        compromised.store(true, Ordering::Release);
                        return;
                    }
                }
            }
        })
    };
    Ok(Guard {
        path,
        permit,
        stop,
        beat: Some(beat),
        compromised,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Backdate a lock so only a live heartbeat could rescue it.
    fn age_past_staleness(lock: &Path) {
        touch(
            Permit::for_a_test(),
            lock,
            SystemTime::now() - Duration::from_secs(3600),
        )
        .unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("pitboard-lock-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p.join("target")
    }

    #[test]
    #[cfg_attr(windows, ignore = "W16: Pitboard's locks on Windows")]
    fn acquires_and_releases() {
        let t = scratch("basic");
        let lock = t.with_extension("").parent().unwrap().join("target.lock");
        {
            let _g = acquire(Permit::for_a_test(), &t, WRITE).expect("should acquire");
            assert!(lock.is_dir(), "the lock directory should exist while held");
        }
        assert!(
            !lock.exists(),
            "dropping the guard must remove the directory"
        );
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn refuses_while_another_holder_is_alive() {
        let t = scratch("busy");
        let _held = acquire(Permit::for_a_test(), &t, WRITE).expect("first acquire");
        // The holder heartbeats, so this must exhaust its retries rather than steal it.
        assert!(matches!(
            acquire(Permit::for_a_test(), &t, WRITE),
            Err(LockError::Busy)
        ));
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn reclaims_a_lock_left_behind_by_a_dead_process() {
        let t = scratch("stale");
        let lock = PathBuf::from(format!("{}.lock", t.display()));
        std::fs::create_dir_all(&lock).unwrap();
        age_past_staleness(&lock);
        let _g =
            acquire(Permit::for_a_test(), &t, WRITE).expect("a stale lock must be reclaimable");
    }

    /// Claude Code keeps its refresh lock for 60 seconds untouched and touches it every 5, so
    /// one 20 seconds old is a refresh still under way, which a write lock that old is not.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_refresh_lock_is_abandoned_only_once_claude_code_would_take_it() {
        let lock = scratch("refresh").with_extension("lock");
        std::fs::create_dir_all(&lock).unwrap();
        touch(
            Permit::for_a_test(),
            &lock,
            SystemTime::now() - Duration::from_secs(20),
        )
        .unwrap();

        assert!(abandoned(&lock, WRITE));
        assert!(!abandoned(&lock, REFRESH));
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn releasing_is_immediate() {
        let t = scratch("release");
        let started = std::time::Instant::now();
        drop(acquire(Permit::for_a_test(), &t, WRITE).unwrap());
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "release took {:?}",
            started.elapsed()
        );
        assert!(!PathBuf::from(format!("{}.lock", t.display())).exists());
    }

    /// Aged past staleness first, so only a live heartbeat can bring it back.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn the_heartbeat_keeps_a_held_lock_young() {
        let t = scratch("beat");
        let g = acquire(Permit::for_a_test(), &t, WRITE).unwrap();
        let lock = PathBuf::from(format!("{}.lock", t.display()));
        let when_taken = mtime(&lock).unwrap();

        thread::sleep(WRITE.heartbeat + Duration::from_millis(400));

        assert!(
            mtime(&lock).unwrap() > when_taken,
            "the heartbeat never touched the lock"
        );
        assert!(age(&lock).unwrap() < WRITE.stale);
        assert!(!g.compromised(), "nobody else touched it");
    }

    /// The half of proper-lockfile's protocol this omitted, and the reason it matters. A
    /// machine that sleeps mid-switch lets the lock age past its staleness window; Claude
    /// Code reclaims it and starts writing the credential underneath a switch that believes
    /// it still holds it. An mtime that is not the one this guard last stored is the only
    /// evidence of that there is.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_lock_somebody_else_touched_is_never_ours_again() {
        let t = scratch("compromised");
        let g = acquire(Permit::for_a_test(), &t, WRITE).unwrap();
        let lock = PathBuf::from(format!("{}.lock", t.display()));

        // What reclaiming it looks like from here: the directory's mtime is somebody else's.
        age_past_staleness(&lock);

        thread::sleep(WRITE.heartbeat + Duration::from_millis(400));
        assert!(
            g.compromised(),
            "a lock whose mtime this guard did not set is not this guard's"
        );

        drop(g);
        assert!(
            lock.exists(),
            "and releasing it must not take away the lock that now belongs to somebody else"
        );
        let _ = std::fs::remove_dir(&lock);
    }

    /// A lock directory that vanished is gone whoever removed it, and renewing it would
    /// mean making one nobody is coordinating through.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_lock_that_vanished_is_not_quietly_remade() {
        let t = scratch("vanished");
        let g = acquire(Permit::for_a_test(), &t, WRITE).unwrap();
        let lock = PathBuf::from(format!("{}.lock", t.display()));
        std::fs::remove_dir(&lock).unwrap();

        thread::sleep(WRITE.heartbeat + Duration::from_millis(400));
        assert!(g.compromised());
        assert!(!lock.exists(), "the heartbeat did not put it back");
    }

    /// APFS stores a value 18 to 60 nanoseconds from the one it is given, so a guard that
    /// remembered what it asked for would call every lock compromised and abandon every
    /// switch. The value read back is the only one worth keeping.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn what_the_filesystem_stored_is_what_gets_remembered() {
        let t = scratch("granularity");
        let _g = acquire(Permit::for_a_test(), &t, WRITE).unwrap();
        let lock = PathBuf::from(format!("{}.lock", t.display()));

        let asked = SystemTime::now();
        let stored = touch(Permit::for_a_test(), &lock, asked).unwrap();
        assert_eq!(
            stored,
            mtime(&lock).unwrap(),
            "reading it back twice gives the same answer, whatever it is"
        );
    }
}
