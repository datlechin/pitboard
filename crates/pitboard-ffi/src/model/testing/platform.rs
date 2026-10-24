use crate::model::{AppControl, Notifications, PlatformError, RunOutNotice};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

/// What else an app quitting does, given the app's id.
type OnQuit = Box<dyn Fn(&str) + Send>;

/// Other apps, as a test says they are, and nothing on the machine running the tests: this
/// machine may have ChatGPT open. Records every app asked to quit and every app opened.
pub(crate) struct StandInApps {
    running: Mutex<BTreeSet<String>>,
    /// Whether an app asked to quit does. One busy with work, or whose person said no, does
    /// not.
    quits: AtomicBool,
    asked: Mutex<Vec<String>>,
    running_calls: AtomicUsize,
    fail_running: AtomicBool,
    fail_quit: AtomicBool,
    fail_open: AtomicBool,
    /// Holds each call of `running` until the test lets it go, telling the test as it
    /// arrives, where a test says so.
    hold: Mutex<Option<(Sender<()>, Receiver<()>)>>,
    /// What else an app quitting does, such as its processes leaving the process list.
    on_quit: Mutex<Option<OnQuit>>,
}

impl StandInApps {
    pub(crate) fn new(running: &[&str], quits: bool) -> Arc<StandInApps> {
        Arc::new(StandInApps {
            running: Mutex::new(running.iter().map(|&id| id.to_owned()).collect()),
            quits: AtomicBool::new(quits),
            asked: Mutex::new(Vec::new()),
            running_calls: AtomicUsize::new(0),
            fail_running: AtomicBool::new(false),
            fail_quit: AtomicBool::new(false),
            fail_open: AtomicBool::new(false),
            hold: Mutex::new(None),
            on_quit: Mutex::new(None),
        })
    }

    /// From now on an app that quits does `then` too, with its id.
    pub(crate) fn on_quit(&self, then: impl Fn(&str) + Send + 'static) {
        *self.on_quit.lock().expect("a test's own lock") = Some(Box::new(then));
    }

    /// Where a test's app is, which is nowhere on the machine running it.
    pub(crate) fn copy_of(app: &str) -> String {
        format!("/stand-in/{app}.app")
    }

    /// Every app asked to quit and every app opened, in order: "quit com.openai.codex".
    pub(crate) fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("a test's own lock").clone()
    }

    pub(crate) fn set_running(&self, apps: &[&str]) {
        *self.running.lock().expect("a test's own lock") =
            apps.iter().map(|&id| id.to_owned()).collect();
    }

    pub(crate) fn is_running(&self, app: &str) -> bool {
        self.running
            .lock()
            .expect("a test's own lock")
            .contains(app)
    }

    /// How many times it was asked whether an app runs.
    pub(crate) fn running_calls(&self) -> usize {
        self.running_calls.load(Ordering::SeqCst)
    }

    /// From now on asking whether an app runs throws, as an app's own code can.
    pub(crate) fn fail_running(&self, fails: bool) {
        self.fail_running.store(fails, Ordering::SeqCst);
    }

    pub(crate) fn fail_quit(&self, fails: bool) {
        self.fail_quit.store(fails, Ordering::SeqCst);
    }

    pub(crate) fn fail_open(&self, fails: bool) {
        self.fail_open.store(fails, Ordering::SeqCst);
    }

    /// Holds every call of `running` from now on until the returned sender is dropped,
    /// telling the returned receiver as each arrives.
    pub(crate) fn hold_running(&self) -> (Receiver<()>, Sender<()>) {
        let (arrived, arrivals) = channel();
        let (release, held) = channel();
        *self.hold.lock().expect("a test's own lock") = Some((arrived, held));
        (arrivals, release)
    }

    fn refused(what: &str) -> PlatformError {
        PlatformError::Failed {
            reason: format!("the stand-in was told to fail {what}"),
        }
    }
}

impl AppControl for StandInApps {
    fn running(&self, app: String) -> Result<Option<String>, PlatformError> {
        self.running_calls.fetch_add(1, Ordering::SeqCst);
        if let Some((arrived, held)) = self.hold.lock().expect("a test's own lock").as_ref() {
            let _ = arrived.send(());
            let _ = held.recv();
        }
        if self.fail_running.load(Ordering::SeqCst) {
            return Err(StandInApps::refused("to say what runs"));
        }
        Ok(self.is_running(&app).then(|| StandInApps::copy_of(&app)))
    }

    fn request_quit(&self, app: String) -> Result<(), PlatformError> {
        if self.fail_quit.load(Ordering::SeqCst) {
            return Err(StandInApps::refused("to ask an app to quit"));
        }
        self.asked
            .lock()
            .expect("a test's own lock")
            .push(format!("quit {app}"));
        if self.quits.load(Ordering::SeqCst) {
            self.running.lock().expect("a test's own lock").remove(&app);
            if let Some(then) = self.on_quit.lock().expect("a test's own lock").as_ref() {
                then(&app);
            }
        }
        Ok(())
    }

    fn reopen(&self, location: String, front: bool) -> Result<(), PlatformError> {
        if self.fail_open.load(Ordering::SeqCst) {
            return Err(StandInApps::refused("to open an app"));
        }
        let app = location
            .strip_prefix("/stand-in/")
            .and_then(|rest| rest.strip_suffix(".app"))
            .unwrap_or(&location)
            .to_owned();
        let opened = if front { "open-front" } else { "open" };
        self.asked
            .lock()
            .expect("a test's own lock")
            .push(format!("{opened} {app}"));
        self.running.lock().expect("a test's own lock").insert(app);
        Ok(())
    }
}

/// The system's notifications, as a test has them: every notification posted is kept, and
/// none reaches the machine running the tests.
#[derive(Default)]
pub(crate) struct Posted {
    posted: Mutex<Vec<RunOutNotice>>,
    // why the system refuses each one, where it does
    refused: Option<String>,
}

impl Posted {
    pub(crate) fn refusing(reason: &str) -> Posted {
        Posted {
            refused: Some(reason.into()),
            ..Posted::default()
        }
    }

    pub(crate) fn posted(&self) -> Vec<RunOutNotice> {
        self.posted.lock().expect("a test's own lock").clone()
    }
}

impl Notifications for Posted {
    fn post(&self, notice: RunOutNotice) -> Result<(), PlatformError> {
        self.posted.lock().expect("a test's own lock").push(notice);
        match &self.refused {
            Some(reason) => Err(PlatformError::Failed {
                reason: reason.clone(),
            }),
            None => Ok(()),
        }
    }
}
