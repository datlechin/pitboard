//! When Pitboard's account index or its readings are written, for a front end that acts on a
//! change made elsewhere: the app, and `pitboard watch`.
//!
//! [`Written::wait`] returns as either file is written, heard through the host's watch on
//! Pitboard's directory. Where the host cannot watch it, where it is not there yet, or once a
//! watch has ended, it returns every [`LOOK_EVERY_SECONDS`] instead, for its caller to look at
//! when the two were last written, and watches again as soon as it can. A watch that began,
//! ended, or had events the system dropped, counts as a write: what was written before it or
//! meanwhile went unheard.

use super::cadence::LOOK_EVERY_SECONDS;
use crate::context::Context;
use crate::host::Host;
use crate::host::watch::{self, Heard, Watch};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

/// What a wait came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// The account index or the readings were written, or may have been where nothing heard
    /// it: a watch began, ended or missed something.
    Written,
    /// Nothing watches them, so its caller looks: at once as that begins, then every
    /// [`LOOK_EVERY_SECONDS`].
    Unwatched,
    /// The time it was given is up, and nothing was written.
    Elapsed,
    /// It was asked to stop, through [`Written::stop_handle`]. Every wait after says so too.
    Stopped,
}

/// Pitboard's directory, watched for writes to the account index and the readings.
#[derive(Debug)]
pub struct Written {
    dir: PathBuf,
    names: Vec<OsString>,
    host: Arc<dyn Host>,
    told: Receiver<Told>,
    tell: Sender<Told>,
    watching: Watching,
    /// The number of the last watch begun, so what a watch already ended still tells is
    /// told apart.
    begun: u64,
    /// A watch has begun since the last wait, so what was written before it went unheard.
    unheard: bool,
    stopped: Arc<AtomicBool>,
}

#[derive(Debug)]
enum Watching {
    // Dropped, it stops.
    On {
        _watch: Watch,
    },
    /// Not watched. `again` is when a watch is tried next, and `said` whether a wait has said
    /// so since one was last tried.
    Off {
        again: Instant,
        said: bool,
    },
}

#[derive(Debug)]
enum Told {
    Heard { watch: u64, heard: Heard },
    Stop,
}

/// Stops a wait from another thread: the one under way, and every one after.
#[derive(Debug, Clone)]
pub struct Stop {
    stopped: Arc<AtomicBool>,
    tell: Sender<Told>,
}

impl Stop {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.tell.send(Told::Stop);
    }
}

impl Written {
    /// Watching Pitboard's directory for `ctx`, where it can be watched now.
    pub(crate) fn new(ctx: &Context) -> Written {
        let (tell, told) = channel();
        let mut written = Written {
            dir: crate::home::dir(ctx),
            names: vec![
                crate::state::FILE.into(),
                crate::usage::readings::FILE.into(),
            ],
            host: Arc::clone(&ctx.host),
            told,
            tell,
            watching: Watching::Off {
                again: Instant::now(),
                said: false,
            },
            begun: 0,
            unheard: false,
            stopped: Arc::new(AtomicBool::new(false)),
        };
        written.watch();
        written
    }

    /// What stops a wait from another thread.
    pub fn stop_handle(&self) -> Stop {
        Stop {
            stopped: Arc::clone(&self.stopped),
            tell: self.tell.clone(),
        }
    }

    /// Waits until either file is written, `within` is up or it is stopped, and says which.
    /// Where nothing watches them it returns sooner, to be looked at.
    pub fn wait(&mut self, within: Duration) -> Waited {
        let until = Instant::now().checked_add(within);
        loop {
            if self.stopped.load(Ordering::SeqCst) {
                return Waited::Stopped;
            }
            if let Watching::Off { again, .. } = self.watching
                && Instant::now() >= again
            {
                self.watch();
            }
            if std::mem::take(&mut self.unheard) {
                return Waited::Written;
            }
            let next = match &mut self.watching {
                Watching::On { .. } => until,
                Watching::Off { again, said } => {
                    if !*said {
                        *said = true;
                        return Waited::Unwatched;
                    }
                    Some(until.map_or(*again, |until| until.min(*again)))
                }
            };
            let told = match next {
                None => self.told.recv().ok(),
                Some(at) => self
                    .told
                    .recv_timeout(at.saturating_duration_since(Instant::now()))
                    .ok(),
            };
            match told {
                Some(Told::Stop) => return Waited::Stopped,
                Some(Told::Heard { watch, heard }) if self.current(watch) => {
                    self.heard(heard);
                    self.drain();
                    return Waited::Written;
                }
                // Told by a watch that has ended since.
                Some(Told::Heard { .. }) => {}
                None if until.is_some_and(|until| Instant::now() >= until) => {
                    return Waited::Elapsed;
                }
                None => {}
            }
        }
    }

    /// Watches the directory, where that can be done now.
    fn watch(&mut self) {
        let number = self.begun + 1;
        let tell = self.tell.clone();
        let heard = Arc::new(move |heard| {
            let _ = tell.send(Told::Heard {
                watch: number,
                heard,
            });
        });
        match watch::start(self.host.as_ref(), &self.dir, &self.names, heard) {
            Ok(watch) => {
                self.begun = number;
                self.watching = Watching::On { _watch: watch };
                self.unheard = true;
            }
            Err(_) => {
                self.watching = Watching::Off {
                    again: Instant::now() + look_every(),
                    said: false,
                };
            }
        }
    }

    fn current(&self, watch: u64) -> bool {
        watch == self.begun && matches!(self.watching, Watching::On { .. })
    }

    /// What the watch heard: one that ended is let go of, and tried again at the next wait.
    fn heard(&mut self, heard: Heard) {
        if heard == Heard::Ended {
            self.watching = Watching::Off {
                again: Instant::now(),
                said: false,
            };
        }
    }

    /// Everything the watch has told meanwhile, which this wait has answered.
    fn drain(&mut self) {
        while let Ok(told) = self.told.try_recv() {
            match told {
                // The flag is set, which the next wait reads.
                Told::Stop => {}
                Told::Heard { watch, heard } if self.current(watch) => self.heard(heard),
                Told::Heard { .. } => {}
            }
        }
    }
}

fn look_every() -> Duration {
    Duration::from_secs(LOOK_EVERY_SECONDS.unsigned_abs())
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::atomic::{self, Perms};
    use crate::permit::Permit;
    use crate::service::Pitboard;
    use crate::switch::harness::machine;

    // Long enough for any system to tell a write, however busy.
    const PATIENCE: Duration = Duration::from_secs(20);

    // Long enough for a write the watch heard to be told, where nothing should be.
    const QUIET: Duration = Duration::from_millis(500);

    // FSEvents may tell the rig's own writes, made before the watch began, after it began.
    // Waits until nothing more is told, and says how the last wait ended.
    fn settled(written: &mut Written) -> Waited {
        loop {
            match written.wait(QUIET) {
                Waited::Written => {}
                other => return other,
            }
        }
    }

    fn replaced(dir: &std::path::Path, name: &str) {
        atomic::write(Permit::for_a_test(), &dir.join(name), b"{}", Perms::Secret)
            .expect("replaced as the core replaces its files");
    }

    /// An atomic replace of the account index, as every change writes it, wakes a wait as it
    /// lands, where a look every `LOOK_EVERY_SECONDS` would have found it up to two seconds on.
    /// So does one of the readings, as each status line writes them.
    #[test]
    fn a_write_to_the_index_is_seen_as_it_lands() {
        let rig = machine("written-index");
        let pitboard = Pitboard::new(rig.ctx.clone());
        let dir = pitboard.directory();
        let mut written = pitboard.written();
        assert_eq!(
            written.wait(Duration::ZERO),
            Waited::Written,
            "a watch began"
        );
        assert_eq!(settled(&mut written), Waited::Elapsed, "watched");

        for name in ["state.json", "usage.json"] {
            let began = Instant::now();
            replaced(&dir, name);
            assert_eq!(written.wait(PATIENCE), Waited::Written, "{name}");
            assert!(
                began.elapsed() < look_every(),
                "{name} heard {:?} after it was written",
                began.elapsed()
            );
        }
    }

    /// Pitboard's other files written, here the app's own, wake nothing, and neither does a
    /// read of the index.
    #[test]
    fn a_write_to_another_file_wakes_nothing() {
        let rig = machine("written-elsewhere");
        let pitboard = Pitboard::new(rig.ctx.clone());
        let dir = pitboard.directory();
        let mut written = pitboard.written();
        assert_eq!(
            written.wait(Duration::ZERO),
            Waited::Written,
            "a watch began"
        );
        assert_eq!(settled(&mut written), Waited::Elapsed, "watched");

        replaced(&dir, "told.json");
        replaced(&dir, "app.json");
        let index = std::fs::read_to_string(dir.join("state.json")).expect("the index");
        assert!(!index.is_empty());
        assert_eq!(written.wait(QUIET), Waited::Elapsed);

        replaced(&dir, "state.json");
        assert_eq!(written.wait(PATIENCE), Waited::Written, "still watched");
    }

    /// Before Pitboard's directory is there, as before the first account is added, nothing can
    /// watch it: the first wait says so at once, and each after it every two seconds, until the
    /// directory is made. Then it is watched, and the wait that begins the watch says a write
    /// may have come, since one may have landed before it.
    #[test]
    fn a_folder_made_after_the_watch_began_is_watched_once_it_is_there() {
        let rig = machine("written-later");
        let pitboard = Pitboard::new(rig.ctx.clone());
        let dir = pitboard.directory();
        std::fs::remove_dir_all(&dir).expect("no directory yet");
        let began = Instant::now();
        let mut written = pitboard.written();
        assert_eq!(written.wait(PATIENCE), Waited::Unwatched, "at once");
        assert_eq!(written.wait(PATIENCE), Waited::Unwatched);
        assert!(began.elapsed() >= look_every());

        std::fs::create_dir_all(&dir).expect("the directory made");
        assert_eq!(
            written.wait(PATIENCE),
            Waited::Written,
            "watched once there"
        );
        assert_eq!(settled(&mut written), Waited::Elapsed, "watched");
        replaced(&dir, "state.json");
        assert_eq!(written.wait(PATIENCE), Waited::Written);
    }

    /// `pitboard uninstall` removes a directory a front end watches, whole and by the time it
    /// returns. The watch ends with it, which is a write, and nothing is watched until it is
    /// there again.
    #[test]
    fn a_watched_pitboard_directory_is_removed_whole_by_uninstall() {
        let rig = machine("written-uninstall");
        let pitboard = Pitboard::new(rig.ctx.clone());
        let dir = pitboard.directory();
        let mut written = pitboard.written();
        assert_eq!(
            written.wait(Duration::ZERO),
            Waited::Written,
            "a watch began"
        );
        assert_eq!(settled(&mut written), Waited::Elapsed, "watched");

        let removed = pitboard.uninstall().expect("uninstalled");
        assert!(removed.value.home_removed);
        assert!(!dir.exists(), "gone as uninstall returns");
        assert_eq!(written.wait(PATIENCE), Waited::Written);
        loop {
            match written.wait(PATIENCE) {
                Waited::Unwatched => break,
                Waited::Written => continue,
                other => panic!("{other:?} with nothing to watch"),
            }
        }
    }

    /// Where the host says a watch would not hear every write, nothing is watched, and a wait
    /// returns every two seconds to be looked at.
    #[test]
    fn where_nothing_can_be_watched_a_wait_returns_to_be_looked_at() {
        let rig = machine("written-unwatched");
        rig.mem.without_a_watch();
        let pitboard = Pitboard::new(rig.ctx.clone());
        let began = Instant::now();
        let mut written = pitboard.written();
        assert_eq!(written.wait(PATIENCE), Waited::Unwatched, "at once");
        replaced(&pitboard.directory(), "state.json");
        assert_eq!(written.wait(PATIENCE), Waited::Unwatched);
        assert!(began.elapsed() >= look_every());
        assert_eq!(written.wait(Duration::ZERO), Waited::Elapsed);
    }

    /// A stop wakes a wait under way on another thread, watched or not, and every wait after
    /// it says it was stopped.
    #[test]
    fn a_stop_wakes_a_wait_under_way() {
        for watched in [true, false] {
            let rig = machine("written-stop");
            if !watched {
                rig.mem.without_a_watch();
            }
            let mut written = Pitboard::new(rig.ctx.clone()).written();
            let first = if watched {
                Waited::Written
            } else {
                Waited::Unwatched
            };
            assert_eq!(written.wait(PATIENCE), first);
            let stop = written.stop_handle();
            let waiting = std::thread::spawn(move || {
                let waited = written.wait(Duration::from_secs(3_600));
                (waited, written.wait(Duration::from_secs(3_600)))
            });
            stop.stop();
            assert_eq!(
                waiting.join().expect("the wait"),
                (Waited::Stopped, Waited::Stopped),
                "watched: {watched}"
            );
        }
    }
}
