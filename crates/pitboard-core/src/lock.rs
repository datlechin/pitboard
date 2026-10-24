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
//! `pitboard stow` while it keeps the login left in a file, and Pitboard while it renews a
//! login of Claude Code's itself, as a refresh does.
//!
//! The writers are a session and [`crate::daemon`], the supervisor that outlives sessions
//! and refreshes on a timer of its own. Both come through here.

use crate::permit::Permit;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime};

/// How a lock is kept and waited for, as Claude Code passes both for each of its own: how
/// long it goes untouched before anybody may take it as abandoned, how often its holder
/// touches it meanwhile, and how many times more one somebody holds is asked for, the first
/// after `backoff` and each after twice the wait before it, up to `most`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    stale: Duration,
    heartbeat: Duration,
    retries: u32,
    backoff: Duration,
    most: Duration,
}

/// Claude Code's write lock, `.storage-write`, around every write of its login (the
/// register's `write_lock`): proper-lockfile touches it at half its staleness.
pub(crate) const WRITE: Timing = Timing {
    stale: Duration::from_millis(15_000),
    heartbeat: Duration::from_millis(7_500),
    retries: 10,
    backoff: Duration::from_millis(100),
    most: Duration::from_millis(1_000),
};

/// Claude Code's refresh lock, `.oauth_refresh.lock` and the legacy one beside it, around a
/// refresh of its login (the register's `refresh_lock`). Claude Code takes one over before
/// it is stale only from a holder its owner record proves gone, and Pitboard writes none, so
/// it waits on Pitboard's. Asked for as the write lock is.
pub(crate) const REFRESH: Timing = Timing {
    stale: Duration::from_millis(60_000),
    heartbeat: Duration::from_millis(5_000),
    ..WRITE
};

impl Timing {
    /// The same lock, asked for `retries` times more, `every` apart.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn asked(self, retries: u32, every: Duration) -> Timing {
        Timing {
            retries,
            backoff: every,
            most: every,
            ..self
        }
    }
}

/// Claude Code's two locks, kept and waited for as a context takes them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Locks {
    pub(crate) write: Timing,
    pub(crate) refresh: Timing,
}

impl Locks {
    /// As Claude Code keeps and waits for each, which every real context does.
    pub(crate) const CLAUDE_CODE: Locks = Locks {
        write: WRITE,
        refresh: REFRESH,
    };
}

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

pub(crate) struct Guard {
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
    pub(crate) fn compromised(&self) -> bool {
        self.compromised.load(Ordering::Acquire)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let (flag, wake) = &*self.stop;
        *flag.lock().unwrap_or_else(PoisonError::into_inner) = true;
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

fn age(path: &Path, now: SystemTime) -> Option<Duration> {
    now.duration_since(mtime(path).ok()?).ok()
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

/// Take the lock guarding `target`, kept and waited for as `timing` says: about seven and a
/// half seconds for one of Claude Code's.
pub(crate) fn acquire(permit: Permit, target: &Path, timing: Timing) -> Result<Guard, LockError> {
    let path = PathBuf::from(format!("{}.lock", target.display()));
    if let Some(parent) = path.parent() {
        crate::host::fs::create_dir_all(permit, parent).map_err(LockError::Io)?;
    }

    let mut backoff = timing.backoff;
    for attempt in 0..=timing.retries {
        match crate::host::fs::create_dir(permit, &path) {
            Ok(()) => return start(permit, path, timing).map_err(LockError::Io),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if abandoned(&path, timing, SystemTime::now()) {
                    let _ = crate::host::fs::remove_dir(permit, &path);
                    continue;
                }
            }
            Err(e) => return Err(LockError::Io(e)),
        }
        if attempt < timing.retries {
            thread::sleep(backoff);
            backoff = (backoff * 2).min(timing.most);
        }
    }
    Err(LockError::Busy)
}

// Untouched past its staleness at `now`, which is when Claude Code takes it too.
fn abandoned(path: &Path, timing: Timing, now: SystemTime) -> bool {
    age(path, now).is_some_and(|a| a > timing.stale)
}

fn start(permit: Permit, path: PathBuf, timing: Timing) -> io::Result<Guard> {
    // What the filesystem actually stored for the directory we just made. Every later beat
    // compares against this, and replaces it with what it stores next.
    let held = mtime(&path)?;
    let stop: Stop = Arc::new((Mutex::new(false), Condvar::new()));
    let compromised = Arc::new(AtomicBool::new(false));
    let beat = {
        let (path, stop, compromised) = (path.clone(), Arc::clone(&stop), Arc::clone(&compromised));
        thread::spawn(move || {
            let (flag, wake) = &*stop;
            let mut stopped = Some(flag.lock().unwrap_or_else(PoisonError::into_inner));
            let kept = keep(permit, &path, timing, held, |every| {
                // Checks the flag before waiting and after every wakeup, spurious or not.
                let (next, _) = wake
                    .wait_timeout_while(stopped.take()?, every, |stop| !*stop)
                    .unwrap_or_else(PoisonError::into_inner);
                let released = *next;
                stopped = Some(next);
                (!released).then(SystemTime::now)
            });
            if !kept {
                compromised.store(true, Ordering::Release);
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

// `due` waits out each interval and gives the time to touch at, or None once released: the
// heartbeat's clock, which a test runs through stale windows at once. False once the lock
// stopped being ours.
fn keep(
    permit: Permit,
    path: &Path,
    timing: Timing,
    mut held: SystemTime,
    mut due: impl FnMut(Duration) -> Option<SystemTime>,
) -> bool {
    while let Some(now) = due(timing.heartbeat) {
        // Look before touching. A directory that is gone, or whose mtime is not the one this
        // guard last stored, is not this guard's lock any more: something aged it out and took
        // it, and is writing under it right now. Touching it then would renew somebody else's
        // lock.
        if mtime(path).ok() != Some(held) {
            return false;
        }
        match touch(permit, path, now) {
            Ok(stored) => held = stored,
            Err(_) => return false,
        }
    }
    true
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    /// As long as a test waits for another thread, however slow the machine running it.
    const PATIENCE: Duration = Duration::from_secs(20);

    /// Claude Code's write lock, touched every millisecond, and asked for twice more a
    /// millisecond apart.
    const QUICK: Timing = Timing {
        heartbeat: Duration::from_millis(1),
        retries: 2,
        backoff: Duration::from_millis(1),
        most: Duration::from_millis(1),
        ..WRITE
    };

    /// Waits for `guard`'s heartbeat to end, which it does by itself only once the lock has
    /// stopped being the guard's.
    fn heartbeat_ends(guard: &mut Guard) {
        let beat = guard.beat.take().expect("a heartbeat");
        let (ended, ends) = mpsc::channel();
        thread::spawn(move || {
            let _ = beat.join();
            let _ = ended.send(());
        });
        ends.recv_timeout(PATIENCE).expect("the heartbeat ended");
    }

    /// Waits until `done`, looking every millisecond.
    fn until(what: &str, done: impl Fn() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(started.elapsed() < PATIENCE, "never {what}");
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Backdate a lock so only a live heartbeat could rescue it.
    fn age_past_staleness(lock: &Path) {
        touch(
            Permit::for_a_test(),
            lock,
            SystemTime::now() - Duration::from_secs(3600),
        )
        .unwrap();
    }

    fn scratch() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        (dir, target)
    }

    #[test]
    fn acquires_and_releases() {
        let (_dir, t) = scratch();
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
    fn refuses_while_another_holder_is_alive() {
        let (_dir, t) = scratch();
        let _held = acquire(Permit::for_a_test(), &t, QUICK).expect("first acquire");
        // The holder heartbeats, so this must exhaust its retries rather than steal it.
        assert!(matches!(
            acquire(Permit::for_a_test(), &t, QUICK),
            Err(LockError::Busy)
        ));
    }

    #[test]
    fn reclaims_a_lock_left_behind_by_a_dead_process() {
        let (_dir, t) = scratch();
        let lock = PathBuf::from(format!("{}.lock", t.display()));
        std::fs::create_dir_all(&lock).unwrap();
        age_past_staleness(&lock);
        let _g =
            acquire(Permit::for_a_test(), &t, WRITE).expect("a stale lock must be reclaimable");
    }

    /// Claude Code keeps its refresh lock for 60 seconds untouched and touches it every 5, so
    /// one 20 seconds old is a refresh still under way, which a write lock that old is not.
    #[test]
    fn a_refresh_lock_is_abandoned_only_once_claude_code_would_take_it() {
        let (_dir, t) = scratch();
        let lock = t.with_extension("lock");
        std::fs::create_dir_all(&lock).unwrap();
        touch(
            Permit::for_a_test(),
            &lock,
            SystemTime::now() - Duration::from_secs(20),
        )
        .unwrap();

        assert!(abandoned(&lock, WRITE, SystemTime::now()));
        assert!(!abandoned(&lock, REFRESH, SystemTime::now()));
    }

    /// Releasing wakes the heartbeat rather than waiting out its interval, here an hour.
    #[test]
    fn releasing_is_immediate() {
        let (_dir, t) = scratch();
        let hourly = Timing {
            heartbeat: Duration::from_secs(3600),
            ..WRITE
        };
        let held = acquire(Permit::for_a_test(), &t, hourly).unwrap();
        let (released, done) = mpsc::channel();
        thread::spawn(move || {
            drop(held);
            let _ = released.send(());
        });
        done.recv_timeout(PATIENCE)
            .expect("released without waiting out the heartbeat");
        assert!(!PathBuf::from(format!("{}.lock", t.display())).exists());
    }

    /// The heartbeat touches the lock again and again, and finds it its own each time.
    #[test]
    fn the_heartbeat_keeps_a_held_lock_young() {
        let (_dir, t) = scratch();
        let g = acquire(Permit::for_a_test(), &t, QUICK).unwrap();
        let lock = PathBuf::from(format!("{}.lock", t.display()));

        for _ in 0..2 {
            let touched = mtime(&lock).unwrap();
            until("touched by the heartbeat", || {
                mtime(&lock).unwrap() != touched
            });
        }

        assert!(age(&lock, SystemTime::now()).unwrap() < QUICK.stale);
        assert!(!g.compromised(), "nobody else touched it");
    }

    // Each of Claude Code's two timings, kept past three of its stale windows on a clock the test
    // drives: touched with at least half the window to spare, never old enough for Claude Code or
    // acquire to take, and never taken for another's. What RF-C5 and RF-C5r measured against
    // proper-lockfile on Windows. On the wall clock, twenty times shorter, a runner that ran a
    // beat more than 375 ms late failed it (runs 38898431888 and 39015107289).
    #[test]
    fn each_heartbeat_outlives_its_stale_window() {
        for timing in [WRITE, REFRESH] {
            let (_dir, t) = scratch();
            let lock = PathBuf::from(format!("{}.lock", t.display()));
            std::fs::create_dir(&lock).unwrap();
            let taken = mtime(&lock).unwrap();
            let mut now = taken;
            let kept = keep(Permit::for_a_test(), &lock, timing, taken, |every| {
                assert!(every <= timing.stale / 2, "{timing:?}");
                now += every;
                assert!(!abandoned(&lock, timing, now), "{timing:?}");
                (now.duration_since(taken).unwrap() <= timing.stale * 3).then_some(now)
            });
            assert!(kept, "{timing:?}");
        }
    }

    /// The half of proper-lockfile's protocol this omitted, and the reason it matters. A
    /// machine that sleeps mid-switch lets the lock age past its staleness window; Claude
    /// Code reclaims it and starts writing the credential underneath a switch that believes
    /// it still holds it. An mtime that is not the one this guard last stored is the only
    /// evidence of that there is.
    #[test]
    fn a_lock_somebody_else_touched_is_never_ours_again() {
        let (_dir, t) = scratch();
        let mut g = acquire(Permit::for_a_test(), &t, QUICK).unwrap();
        let lock = PathBuf::from(format!("{}.lock", t.display()));

        {
            // The heartbeat looks and touches only while it holds this, so the mtime set here
            // cannot fall between its look and its touch.
            let _between_beats = g.stop.0.lock().unwrap();
            // What reclaiming it looks like from here: the directory's mtime is somebody else's.
            age_past_staleness(&lock);
        }

        heartbeat_ends(&mut g);
        assert!(
            g.compromised(),
            "a lock whose mtime this guard did not set is not this guard's"
        );

        drop(g);
        assert!(
            lock.exists(),
            "and releasing it must not take away the lock that now belongs to somebody else"
        );
    }

    /// A lock directory that vanished is gone whoever removed it, and renewing it would
    /// mean making one nobody is coordinating through.
    #[test]
    fn a_lock_that_vanished_is_not_quietly_remade() {
        let (_dir, t) = scratch();
        let mut g = acquire(Permit::for_a_test(), &t, QUICK).unwrap();
        let lock = PathBuf::from(format!("{}.lock", t.display()));
        std::fs::remove_dir(&lock).unwrap();

        heartbeat_ends(&mut g);
        assert!(g.compromised());
        assert!(!lock.exists(), "the heartbeat did not put it back");
    }

    /// APFS stores a value 18 to 60 nanoseconds from the one it is given, so a guard that
    /// remembered what it asked for would call every lock compromised and abandon every
    /// switch. The value read back is the only one worth keeping.
    #[test]
    fn what_the_filesystem_stored_is_what_gets_remembered() {
        let (_dir, t) = scratch();
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
