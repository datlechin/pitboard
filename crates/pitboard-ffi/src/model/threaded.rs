//! The model as an app has it: its mailbox, its actor, its lanes and its listener, over the
//! real core on a machine of the test's own. What these prove is what no test of
//! `State::apply` can: that snapshots reach the listener one at a time and in order, that a
//! listener may call the model back, stopping it too, and that stopping or dropping the
//! model ends every thread it started, a drop waiting on none of them.

use super::state::Cadence;
#[cfg(unix)]
use super::testing::StandIn;
use super::testing::{CHATGPT, CHATGPT_CODEX, Posted, StandInApps, World};
use super::{
    AppControl, Intent, LocalTime, ModelListener, PitboardModel, Platform, PlatformError, Sheet,
    Snapshot,
};
use crate::AppCore;
use crate::present::testing::Utc;
use pitboard_core::testing::Asked;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

/// As long as a test waits for another thread, however slow the machine running it.
const PATIENCE: Duration = Duration::from_secs(20);

/// Looks every few milliseconds, reads by itself only when started, once, and gives an app
/// a moment to quit.
const QUICK: Cadence = Cadence {
    look_every: Duration::from_millis(10),
    read_every: Duration::from_secs(3_600),
    stale_after: Duration::from_secs(60),
    quit_within: Duration::from_millis(200),
    quit_checked_every: Duration::from_millis(5),
    tick_every: Duration::from_secs(3_600),
    arming: Duration::from_millis(10),
};

/// What a listener does with the model from inside `changed`.
type Inside = Box<dyn FnOnce(&PitboardModel) + Send>;

/// Keeps every snapshot it is told of, and can call the model back from inside `changed`.
#[derive(Default)]
struct Told {
    snapshots: Mutex<Vec<Snapshot>>,
    arrived: Condvar,
    /// The model, for a listener that calls it back.
    model: OnceLock<Weak<PitboardModel>>,
    /// Done once, from inside the first `changed` told of a read that has landed, where a
    /// test says so.
    once_read: Mutex<Option<Inside>>,
    /// The revision `snapshot` gave from inside `changed`, each time, beside the one told.
    asked_inside: Mutex<Vec<(u64, u64)>>,
    /// Told the revision of each call as it starts, before it is held, where a test says so.
    entered: Mutex<Option<Sender<u64>>>,
    /// Holds each call until the test lets it go, where a test says so.
    hold: Mutex<Option<Receiver<()>>>,
}

impl ModelListener for Told {
    fn changed(&self, snapshot: Snapshot) -> Result<(), PlatformError> {
        if let Some(model) = self.model.get().and_then(Weak::upgrade) {
            let inside = model.snapshot().revision;
            self.asked_inside
                .lock()
                .expect("a test's own lock")
                .push((snapshot.revision, inside));
            if !snapshot.reading
                && snapshot.status.is_some()
                && let Some(once) = self.once_read.lock().expect("a test's own lock").take()
            {
                once(&model);
            }
        }
        if let Some(entered) = self.entered.lock().expect("a test's own lock").as_ref() {
            let _ = entered.send(snapshot.revision);
        }
        if let Some(hold) = self.hold.lock().expect("a test's own lock").as_ref() {
            let _ = hold.recv();
        }
        self.snapshots
            .lock()
            .expect("a test's own lock")
            .push(snapshot);
        self.arrived.notify_all();
        Ok(())
    }
}

impl Told {
    /// Waits until the snapshots told so far satisfy `done`, and says what they were.
    fn until(&self, what: &str, done: impl Fn(&[Snapshot]) -> bool) -> Vec<Snapshot> {
        let started = Instant::now();
        let mut snapshots = self.snapshots.lock().expect("a test's own lock");
        while !done(&snapshots) {
            let left = PATIENCE
                .checked_sub(started.elapsed())
                .unwrap_or_else(|| panic!("never told {what}: {snapshots:#?}"));
            snapshots = self
                .arrived
                .wait_timeout(snapshots, left)
                .expect("a test's own lock")
                .0;
        }
        snapshots.clone()
    }

    fn count(&self) -> usize {
        self.snapshots.lock().expect("a test's own lock").len()
    }
}

/// Waits until `done` says so, checking every few milliseconds.
fn eventually(what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < PATIENCE, "never {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn labels(snapshot: &Snapshot) -> Vec<String> {
    snapshot
        .status
        .iter()
        .flat_map(|status| &status.accounts)
        .filter_map(|account| account.label.clone())
        .collect()
}

/// A read that has landed and is over, naming `label`.
fn shows(label: &str) -> impl Fn(&[Snapshot]) -> bool + '_ {
    move |told| {
        told.last()
            .is_some_and(|last| !last.reading && labels(last) == [label])
    }
}

fn in_order(told: &[Snapshot]) -> bool {
    told.windows(2)
        .all(|pair| pair[0].revision < pair[1].revision)
}

/// The system as a test's model has it: `apps`, and a clock read in UTC.
fn platform(apps: Arc<dyn AppControl>) -> Platform {
    Platform {
        apps,
        notifications: Arc::new(Posted::default()),
        local_time: Arc::new(Utc),
        earlier: None,
        windows: None,
    }
}

/// A model over `core` telling `told`, which may call it back, with no other app running.
fn model(core: &Arc<AppCore>, told: &Arc<Told>) -> Arc<PitboardModel> {
    model_with(core, told, &StandInApps::new(&[], true))
}

/// A model over `core` and `apps` telling `told`, which may call it back.
fn model_with(
    core: &Arc<AppCore>,
    told: &Arc<Told>,
    apps: &Arc<StandInApps>,
) -> Arc<PitboardModel> {
    let model = PitboardModel::over(
        Arc::clone(core),
        Arc::clone(told) as Arc<dyn ModelListener>,
        platform(Arc::clone(apps) as Arc<dyn AppControl>),
        QUICK,
    );
    let _ = told.model.set(Arc::downgrade(&model));
    model
}

fn last_revision(told: &Told) -> Option<u64> {
    told.snapshots
        .lock()
        .expect("a test's own lock")
        .last()
        .map(|last| last.revision)
}

/// The model reads the accounts once started, through its lanes over the real core, and
/// tells its listener each snapshot in order, the last of them the one `snapshot` gives.
/// Before it is started it shows nothing and asks nothing.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_started_model_reads_the_accounts_and_tells_its_listener_in_order() {
    let world = World::new("reads");
    world.enrolled("work", "here", 42.0);
    let asked = world.api.calls();
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    let first = model.snapshot();
    assert_eq!((first.revision, first.status.as_ref()), (0, None));
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(told.count(), 0, "nothing runs before Start");
    assert_eq!(world.api.calls(), asked);

    model.send(Intent::Start);
    let snapshots = told.until("the account read", shows("work"));
    assert!(in_order(&snapshots), "{snapshots:#?}");
    let last = snapshots.last().expect("a snapshot");
    assert_eq!(last.installed.as_ref().map(Vec::len), Some(1));
    let percent = last.status.as_ref().expect("accounts").accounts[0]
        .usage
        .as_ref()
        .expect("numbers")
        .windows[0]
        .percent;
    assert_eq!(percent, 42.0);
    eventually("told the newest snapshot", || {
        last_revision(&told) == Some(model.snapshot().revision)
    });
    model.shutdown();
}

/// A change another front end makes, here a rename typed in a terminal, reaches the
/// listener within a look or two, read from what is already known: nobody is asked.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_change_another_front_end_makes_is_told_without_asking_anyone() {
    let world = World::new("elsewhere");
    world.enrolled("work", "here", 10.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the account read", shows("work"));

    world
        .elsewhere()
        .rename("work", "office")
        .expect("renamed in a terminal");
    world.index_written_later(5);
    let asked = world.api.calls();
    told.until("the rename", shows("office"));
    assert_eq!(world.api.calls(), asked, "nobody was asked");
    model.shutdown();
}

/// A listener may ask for the snapshot and send an intent from inside `changed` without
/// the model waiting on itself: `snapshot` is a lock the model never holds while it calls
/// out, and `send` only posts. The snapshot it is given there is never older than the one
/// it is being told of.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_listener_may_call_the_model_while_it_is_told() {
    let world = World::new("inside");
    world.enrolled("work", "here", 10.0);
    let told = Arc::new(Told::default());
    // Sent once the start's read has landed, when nothing else asks Anthropic: a read
    // somebody asked for asks it whatever it was asked moments ago, so only that read can
    // make more calls than there were then.
    let sent_at = Arc::new(OnceLock::new());
    let api = Arc::clone(&world.api);
    let at = Arc::clone(&sent_at);
    *told.once_read.lock().expect("a test's own lock") = Some(Box::new(move |model| {
        let _ = at.set(api.calls());
        model.send(Intent::Refresh { asked: true });
    }));
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    eventually("the read sent from inside the listener", || {
        sent_at
            .get()
            .is_some_and(|&sent| world.api.calls() > sent && !model.snapshot().reading)
    });
    told.until("the read it asked for", shows("work"));
    let inside = told.asked_inside.lock().expect("a test's own lock").clone();
    assert!(!inside.is_empty());
    assert!(
        inside.iter().all(|(being_told, given)| given >= being_told),
        "{inside:?}"
    );
    model.shutdown();
}

/// Intents sent from many threads at once, each starting a read, make many snapshots, and
/// they reach the listener in order and one at a time, ending with the newest.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn snapshots_made_while_many_threads_send_arrive_in_order() {
    let world = World::new("many");
    world.enrolled("work", "here", 10.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    let senders: Vec<_> = (0..4)
        .map(|_| {
            let model = Arc::clone(&model);
            std::thread::spawn(move || {
                for _ in 0..25 {
                    model.send(Intent::Refresh { asked: false });
                }
            })
        })
        .collect();
    for sender in senders {
        sender.join().expect("a thread that sends");
    }
    let snapshots = told.until("the last read over", |told| {
        told.last()
            .is_some_and(|last| !last.reading && last.revision == model.snapshot().revision)
    });
    assert!(in_order(&snapshots), "{snapshots:#?}");
    model.shutdown();
}

/// `shutdown` stops everything the model started: the listener is told nothing more, a
/// change made elsewhere is not looked for, and the actor, the lanes and the notifier end,
/// letting go of the listener and the core. The model still answers `snapshot`, and `send`
/// comes to nothing.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn shutdown_stops_the_timers_the_lanes_and_the_listener() {
    let world = World::new("shutdown");
    world.enrolled("work", "here", 10.0);
    let core = world.core();
    let told = Arc::new(Told::default());
    let model = model(&core, &told);
    model.send(Intent::Start);
    told.until("the account read", shows("work"));

    model.shutdown();
    let count = told.count();
    eventually("the notifier let go of the listener", || {
        Arc::strong_count(&told) == 1
    });
    eventually("the lanes let go of the core", || {
        Arc::strong_count(&core) == 1
    });

    world
        .elsewhere()
        .rename("work", "office")
        .expect("renamed in a terminal");
    world.index_written_later(5);
    model.send(Intent::Refresh { asked: true });
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(told.count(), count, "told nothing after shutdown");
    assert_eq!(labels(&model.snapshot()), ["work"]);
    model.shutdown();
}

/// A listener may stop the model from inside `changed`: `shutdown` waits for the actor alone,
/// which never waits on the listener, and not for the thread it is called on. The call it is
/// made from is the last the listener is told of, and every thread ends.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_listener_may_shut_the_model_down_while_it_is_told() {
    let world = World::new("shut-inside");
    world.enrolled("work", "here", 10.0);
    let core = world.core();
    let told = Arc::new(Told::default());
    let (shut, was_shut) = channel();
    *told.once_read.lock().expect("a test's own lock") = Some(Box::new(move |model| {
        model.shutdown();
        let _ = shut.send(());
    }));
    let model = model(&core, &told);
    model.send(Intent::Start);
    was_shut
        .recv_timeout(PATIENCE)
        .expect("shut down from inside the listener");

    eventually("the notifier let go of the listener", || {
        Arc::strong_count(&told) == 1
    });
    eventually("the lanes let go of the core", || {
        Arc::strong_count(&core) == 1
    });
    let snapshots = told.snapshots.lock().expect("a test's own lock").clone();
    let read = snapshots
        .iter()
        .position(|snapshot| !snapshot.reading && snapshot.status.is_some());
    assert_eq!(
        read,
        Some(snapshots.len() - 1),
        "told nothing after the call that shut it down: {snapshots:#?}"
    );
    model.shutdown();
}

/// Dropping the model waits for nothing, since .NET's finalizer thread can be the one dropping
/// it: not for its listener, held up here inside a call, nor for its actor, held up here
/// making a snapshot. A drop that joined either thread would wait until the test let it go.
/// Once let go, the threads end, letting go of the listener and the core.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn dropping_the_model_waits_for_nothing() {
    let world = World::new("drop");
    world.enrolled("work", "here", 10.0);
    let core = world.core();
    let told = Arc::new(Told::default());
    let (release, held): (Sender<()>, Receiver<()>) = channel();
    *told.hold.lock().expect("a test's own lock") = Some(held);
    let (entered, inside) = channel();
    *told.entered.lock().expect("a test's own lock") = Some(entered);
    // Not given the model, so the test's is the last of it.
    let model = PitboardModel::over(
        Arc::clone(&core),
        Arc::clone(&told) as Arc<dyn ModelListener>,
        platform(StandInApps::new(&[], true) as Arc<dyn AppControl>),
        QUICK,
    );
    model.send(Intent::Start);
    inside
        .recv_timeout(PATIENCE)
        .expect("the listener held up inside a call");

    // The actor takes this lock after every message it takes, and before the stop the drop
    // sends it takes the intent sent while the test holds it.
    let shown = Arc::clone(&model.shown);
    let making = super::lock(&shown);
    model.send(Intent::Refresh { asked: false });

    let (dropped, done) = channel();
    std::thread::spawn(move || {
        drop(model);
        let _ = dropped.send(());
    });
    done.recv_timeout(Duration::from_secs(5))
        .expect("dropped without waiting for the actor or the listener");
    assert_eq!(told.count(), 0, "the listener is still held up");

    drop(making);
    drop(release);
    eventually("the notifier let go of the listener", || {
        Arc::strong_count(&told) == 1
    });
    eventually("the lanes let go of the core", || {
        Arc::strong_count(&core) == 1
    });
}

/// The account `provider`'s tool has signed in, by its label.
fn signed_in<'a>(snapshot: &'a Snapshot, provider: &str) -> Option<&'a str> {
    snapshot
        .status
        .iter()
        .flat_map(|status| &status.accounts)
        .find(|account| account.provider == provider && account.signed_in)
        .and_then(|account| account.label.as_deref())
}

/// Codex with `personal` signed in and `work` parked, as a terminal leaves two accounts.
fn two_codex_accounts(name: &str) -> World {
    let world = World::new(name);
    world.codex_enrolled("personal", "p");
    world.codex_parked("work", "w");
    world
}

/// A read that has landed and is over, with `label` signed in to Codex and no switch under
/// way.
fn codex_shows(label: &str) -> impl Fn(&[Snapshot]) -> bool + '_ {
    in_use("codex", label)
}

/// A read that has landed and is over, with `label` signed in to `provider`'s tool and no
/// switch under way.
fn in_use<'a>(provider: &'a str, label: &'a str) -> impl Fn(&[Snapshot]) -> bool + 'a {
    move |told| {
        told.last().is_some_and(|last| {
            !last.reading
                && last.switch_under_way.is_none()
                && signed_in(last, provider) == Some(label)
        })
    }
}

/// The real core's Codex switch, through the model's lanes, with two `codex` sessions
/// running: the warning that they keep the account left and must not sign out reaches the
/// snapshot as what Codex's last switch said, and outlasts the read after the switch, which
/// says nothing of it. The switch is said to be under way until that read has landed.
#[test]
#[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
fn a_codex_switchs_warning_reaches_the_snapshot() {
    let world = two_codex_accounts("codex-switch");
    world.host.runs("codex", 2);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the accounts read", codex_shows("personal"));

    model.send(Intent::SwitchTo {
        qualified: "codex/work".into(),
    });
    let snapshots = told.until("the switch and the read after it", codex_shows("work"));
    assert!(in_order(&snapshots), "{snapshots:#?}");
    assert!(
        snapshots
            .iter()
            .any(|s| s.switch_under_way.as_deref() == Some("codex/work")),
        "said to be under way"
    );
    let last = snapshots.last().expect("a snapshot");
    assert_eq!(last.failure, None);
    let [said] = &last.last_switches[..] else {
        panic!("one tool switched: {:#?}", last.last_switches);
    };
    assert_eq!(
        (said.provider.as_str(), said.to.as_str()),
        ("codex", "codex/work")
    );
    assert_eq!(said.follows_at, None, "a running codex never follows");
    assert_eq!(
        said.restart
            .as_ref()
            .map(|r| (r.program.as_str(), r.from.as_str())),
        Some(("codex", "personal"))
    );
    let warned = said
        .warnings
        .iter()
        .find(|w| w.code == "sessions_still_running")
        .expect("the sessions still running are warned about");
    assert!(warned.message.contains("2 `codex` sessions"), "{warned:?}");
    assert!(warned.message.contains("`codex/personal`"), "{warned:?}");
    assert!(
        last.warnings
            .iter()
            .all(|w| w.code != "sessions_still_running"),
        "the read after it does not say it again: {:?}",
        last.warnings
    );
    model.shutdown();
}

/// AppModelTests.swift's aSwitchWaitsForTheAppHoldingTheLoginToBeQuit and
/// quittingTheAppSwitchesAndOpensItAgain, through the lanes over the real core, whose
/// holder detection reads ChatGPT's own `codex` in the process list. The switch waits for
/// the person; told to go ahead, ChatGPT is asked to quit, the switch is made once it has,
/// with nothing left running the old login to warn about, and ChatGPT is opened again.
#[test]
#[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
fn quitting_the_app_holding_the_login_switches_and_opens_it_again() {
    let world = two_codex_accounts("codex-quit");
    world.host.runs_at("codex", &[CHATGPT_CODEX]);
    let apps = StandInApps::new(&[CHATGPT], true);
    let host = Arc::clone(&world.host);
    apps.on_quit(move |_| host.runs_at("codex", &[]));
    let told = Arc::new(Told::default());
    let model = model_with(&world.core(), &told, &apps);
    model.send(Intent::Start);
    told.until("the accounts read", codex_shows("personal"));

    model.send(Intent::SwitchTo {
        qualified: "codex/work".into(),
    });
    let asked = told.until("the quit question", |told| {
        told.last().is_some_and(|last| last.quit_question.is_some())
    });
    let asked = asked.last().expect("a snapshot");
    let question = asked.quit_question.as_ref().expect("a question");
    assert_eq!(
        (question.app_id.as_str(), question.name.as_str()),
        (CHATGPT, "ChatGPT")
    );
    assert_eq!(asked.switch_under_way.as_deref(), Some("codex/work"));
    assert_eq!(asked.window_request.pane, Some(super::Pane::Accounts));
    assert!(
        apps.asked().is_empty(),
        "nothing is quit before the person says so"
    );
    assert_eq!(signed_in(asked, "codex"), Some("personal"));

    model.send(Intent::QuitAndSwitch {
        qualified: "codex/work".into(),
    });
    let snapshots = told.until("the switch", codex_shows("work"));
    let last = snapshots.last().expect("a snapshot");
    // Opened on the lane of processes, beside the read on the lane of reads, so either can
    // be done first.
    eventually("ChatGPT opened again", || apps.asked().len() == 2);
    assert_eq!(
        apps.asked(),
        [format!("quit {CHATGPT}"), format!("open {CHATGPT}")]
    );
    assert_eq!(last.quit_question, None);
    assert_eq!(last.failure, None);
    assert!(
        last.last_switches
            .iter()
            .flat_map(|said| &said.warnings)
            .all(|w| w.code != "sessions_still_running"),
        "switched once ChatGPT had quit: {:#?}",
        last.last_switches
    );
    model.shutdown();
}

/// The real core's Codex switch, through the lanes, on a machine whose process list cannot
/// be read. Nobody can say whether ChatGPT's own `codex` runs, so no quit question is
/// asked, though ChatGPT is open, and the switch is made. What Codex's last switch said
/// carries the core's `sessions_unknown`, and the switch's notice says it, after the
/// restart line it keeps where the core counted nothing, rather than in a notice of its own
/// under the code's heading.
#[test]
#[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
fn a_switch_on_a_process_list_nobody_could_read_says_so_and_asks_nothing() {
    let world = two_codex_accounts("codex-listless");
    world.host.without_a_process_list();
    let apps = StandInApps::new(&[CHATGPT], true);
    let told = Arc::new(Told::default());
    let model = model_with(&world.core(), &told, &apps);
    model.send(Intent::Start);
    told.until("the accounts read", codex_shows("personal"));

    model.send(Intent::SwitchTo {
        qualified: "codex/work".into(),
    });
    let snapshots = told.until("the switch and the read after it", codex_shows("work"));
    assert!(
        snapshots.iter().all(|s| s.quit_question.is_none()),
        "nothing found to quit: {snapshots:#?}"
    );
    assert_eq!(apps.running_calls(), 0, "nor asked whether ChatGPT runs");
    assert!(apps.asked().is_empty(), "ChatGPT neither quit nor opened");
    let last = snapshots.last().expect("a snapshot");
    assert_eq!(last.failure, None);
    let [said] = &last.last_switches[..] else {
        panic!("one tool switched: {:#?}", last.last_switches);
    };
    let unknown = said
        .warnings
        .iter()
        .find(|w| w.code == "sessions_unknown")
        .unwrap_or_else(|| panic!("warned: {:?}", said.warnings));
    assert!(unknown.message.contains("`codex/personal`"), "{unknown:?}");
    assert!(
        said.warnings
            .iter()
            .all(|w| w.code != "sessions_still_running"),
        "nothing counted: {:?}",
        said.warnings
    );
    let notice = last
        .notices
        .iter()
        .find(|n| n.id == "switch/codex")
        .unwrap_or_else(|| panic!("the switch's notice: {:#?}", last.notices));
    assert_eq!(notice.severity, crate::present::Severity::Warning);
    assert_eq!(
        notice.lines,
        [
            "Any codex session started before this switch keeps using personal until it is \
             quit and started again."
                .to_owned(),
            unknown.message.clone(),
        ]
    );
    assert!(
        last.notices
            .iter()
            .all(|n| n.title != "Couldn’t tell which sessions are open"),
        "said once: {:#?}",
        last.notices
    );
    model.shutdown();
}

/// Reads answer while a switch waits, as the Swift service's queue of reads answered while
/// its other queues did: here the switch waits on the app's own code saying whether ChatGPT
/// runs, and a read somebody asks for meanwhile lands, with the switch still under way.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_read_answers_while_a_switch_waits() {
    let world = World::new("read-while-switching");
    world.enrolled("work", "here", 10.0);
    world.host.runs_at("codex", &[CHATGPT_CODEX]);
    let apps = StandInApps::new(&[CHATGPT], true);
    let told = Arc::new(Told::default());
    let model = model_with(&world.core(), &told, &apps);
    model.send(Intent::Start);
    told.until("the account read", shows("work"));

    let (arrivals, release) = apps.hold_running();
    model.send(Intent::SwitchTo {
        qualified: "codex/spare".into(),
    });
    arrivals
        .recv_timeout(PATIENCE)
        .expect("the switch asks whether ChatGPT runs");
    let before = world.api.calls();
    model.send(Intent::Refresh { asked: true });
    eventually("the read asked for meanwhile", || {
        let now = model.snapshot();
        world.api.calls() > before && !now.reading && now.updated_at.is_some()
    });
    assert_eq!(
        model.snapshot().switch_under_way.as_deref(),
        Some("codex/spare"),
        "the switch is still waiting"
    );

    drop(release);
    told.until("the quit question", |told| {
        told.last().is_some_and(|last| last.quit_question.is_some())
    });
    model.send(Intent::KeepAppOpen);
    told.until("the question gone", |told| {
        told.last()
            .is_some_and(|last| last.quit_question.is_none() && last.switch_under_way.is_none())
    });
    assert!(apps.asked().is_empty(), "kept open, nothing quit");
    model.shutdown();
}

/// A read answers while the core's switch itself waits, which is what the lane of changes is
/// for: here the switch has taken Pitboard's own lock and waits on Claude Code's, which a
/// Claude Code session writing its login holds, and a read somebody asks for meanwhile lands,
/// with the switch still under way and nothing switched. Once the session lets go, the
/// switch is made.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_read_answers_while_the_core_switches() {
    let world = World::new("read-while-the-core-switches");
    world.enrolled("work", "here", 10.0);
    world.parked("spare", "there", 20.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the accounts read", in_use("claude", "work"));

    // The switch asks Anthropic whose the login going in is just before it takes Claude
    // Code's lock, as enrolling it asked before.
    let whose = || {
        world
            .api
            .asked()
            .iter()
            .filter(|asked| **asked == Asked::Owner("access-there".into()))
            .count()
    };
    let asked_before = whose();
    let writing = world.claude_code_writing();
    model.send(Intent::SwitchTo {
        qualified: "claude/spare".into(),
    });
    eventually("the switch at Claude Code's lock", || {
        whose() > asked_before
    });
    let before = world.api.calls();
    model.send(Intent::Refresh { asked: true });
    eventually("the read asked for meanwhile", || {
        world.api.calls() > before && !model.snapshot().reading
    });
    let meanwhile = model.snapshot();
    assert_eq!(
        meanwhile.switch_under_way.as_deref(),
        Some("claude/spare"),
        "the switch is still waiting"
    );
    assert_eq!(meanwhile.failure, None);
    assert_eq!(signed_in(&meanwhile, "claude"), Some("work"));

    drop(writing);
    let snapshots = told.until("the switch", in_use("claude", "spare"));
    let last = snapshots.last().expect("a snapshot");
    assert_eq!(last.failure, None);
    assert_eq!(
        last.last_switches
            .iter()
            .map(|said| (said.provider.as_str(), said.to.as_str()))
            .collect::<Vec<_>>(),
        [("claude", "spare")]
    );
    model.shutdown();
}

// Signing in, on the real core with a stand-in for `claude`, which is a shell script: so
// these run where one does.

/// What a test of signing in starts from: Claude Code with `work` in use, a stand-in in
/// place of `claude`, and a model over the core read once.
#[cfg(unix)]
struct SigningInWorld {
    world: World,
    claude: StandIn,
    core: Arc<AppCore>,
    told: Arc<Told>,
    model: Arc<PitboardModel>,
}

#[cfg(unix)]
fn signing_in_world(name: &str) -> SigningInWorld {
    signing_in_world_with(name, World::claude_stand_in)
}

/// The same, with the stand-in `stand_in` makes.
#[cfg(unix)]
fn signing_in_world_with(name: &str, stand_in: fn(&mut World) -> StandIn) -> SigningInWorld {
    let mut world = World::new(name);
    world.enrolled("work", "here", 10.0);
    let claude = stand_in(&mut world);
    let core = world.core();
    let told = Arc::new(Told::default());
    let model = model(&core, &told);
    model.send(Intent::Start);
    told.until("the account read", shows("work"));
    SigningInWorld {
        world,
        claude,
        core,
        told,
        model,
    }
}

/// The last snapshot told has a sign-in under way that is `wanted`.
#[cfg(unix)]
fn signing(wanted: impl Fn(&super::RunningSignIn) -> bool) -> impl Fn(&[Snapshot]) -> bool {
    move |told| {
        told.last()
            .and_then(|last| last.signing_in.as_ref())
            .is_some_and(&wanted)
    }
}

/// The last snapshot told has no sign-in under way, and a read that has landed and is over
/// with `label` among the accounts and `then` true of it.
#[cfg(unix)]
fn signed_in_as<'a>(
    label: &'a str,
    then: impl Fn(&Snapshot) -> bool + 'a,
) -> impl Fn(&[Snapshot]) -> bool + 'a {
    move |told| {
        told.last().is_some_and(|last| {
            last.signing_in.is_none()
                && !last.reading
                && labels(last).iter().any(|shown| shown == label)
                && then(last)
        })
    }
}

fn sign_in(name: &str) -> Intent {
    Intent::SignIn {
        provider: "claude".into(),
        name: name.into(),
    }
}

#[cfg(unix)]
fn paste(code: &str) -> Intent {
    Intent::PasteCode { code: code.into() }
}

/// The owner's decision, on the real core with a stand-in for `claude` that reads lines as
/// 2.1.289 reads them: after Claude Code refuses a code typed back, the field is offered
/// again, and the next code goes to the same sign-in, whose tool takes it and signs in. What
/// it signed in to is parked beside the account in use, and the sheet it was started from
/// closes.
#[test]
#[cfg(unix)]
fn a_code_claude_code_refused_is_typed_again_into_the_same_sign_in() {
    let at = signing_in_world("paste-again");
    at.model.send(Intent::PresentSheet {
        sheet: Sheet::Add { provider: None },
    });
    at.model.send(sign_in("travel"));
    at.told.until("a code asked for", signing(|s| s.wants_code));
    at.world.signed_in_privately("away", "access-away", 20.0);

    at.model.send(paste("half-a-code"));
    let told = at
        .told
        .until("the code refused", signing(|s| s.code_refused));
    let refused = told
        .last()
        .and_then(|last| last.signing_in.clone())
        .expect("the sign-in");
    assert!(refused.wants_code, "asked for again");
    assert!(refused.said.contains("Invalid code."), "{:?}", refused.said);

    at.model.send(paste("the-code#the-state"));
    let told = at
        .told
        .until("travel enrolled", signed_in_as("travel", |_| true));
    let last = told.last().expect("a snapshot");
    assert_eq!(at.claude.typed(), ["half-a-code", "the-code#the-state"]);
    assert_eq!(last.sheet, None, "the sheet it was started from closed");
    assert_eq!((&last.failure, &last.sheet_failure), (&None, &None));
    let travel = last
        .status
        .iter()
        .flat_map(|status| &status.accounts)
        .find(|account| account.label.as_deref() == Some("travel"))
        .expect("travel");
    assert!(!travel.signed_in && travel.switchable, "{travel:?}");
    assert_eq!(
        signed_in(last, "claude"),
        Some("work"),
        "beside the one in use"
    );
    at.model.shutdown();
}

/// RoutingTests.swift's signingInAgainToTheAccountInUseSaysItHasANewLogin, on the real core
/// with a stand-in for `claude`: signing in again to the account in use puts its new login in
/// use, keeps it the account in use, and says so beside what Claude Code's last switch said.
#[test]
#[cfg(unix)]
fn signing_in_again_to_the_account_in_use_puts_its_new_login_in_use() {
    let at = signing_in_world("sign-in-again");
    at.model.send(Intent::PresentSheet {
        sheet: Sheet::SignInAgain {
            provider: "claude".into(),
            label: "work".into(),
        },
    });
    at.model.send(sign_in("work"));
    at.told.until("a code asked for", signing(|s| s.wants_code));
    at.world
        .signed_in_privately("here", "access-here-again", 10.0);
    at.model.send(paste("the-code#the-state"));

    let told = at.told.until(
        "what the sign-in said",
        signed_in_as("work", |last| !last.last_switches.is_empty()),
    );
    let last = told.last().expect("a snapshot");
    let [said] = &last.last_switches[..] else {
        panic!("one tool's: {:#?}", last.last_switches);
    };
    assert_eq!(
        (said.provider.as_str(), said.to.as_str()),
        ("claude", "work")
    );
    assert_eq!(
        said.said.as_deref(),
        Some("Signed in to work again. Its new login is the one in use now.")
    );
    assert_eq!(signed_in(last, "claude"), Some("work"));
    assert_eq!(last.sheet, None);
    assert!(
        at.world.claude_code_uses("access-here-again"),
        "the new login is the one in use"
    );
    at.model.shutdown();
}

/// AppModelTests.swift's aCancelledSignInStopsTheToolAndReportsNothing, on the real core with
/// a stand-in for `claude` waiting for a code: cancelling stops the tool, whose process is
/// gone, nothing is typed to it or enrolled, and the sign-in's thread ends.
#[test]
#[cfg(unix)]
fn cancelling_a_sign_in_stops_its_tool() {
    let at = signing_in_world("cancel");
    at.model.send(sign_in("travel"));
    at.told.until("a code asked for", signing(|s| s.wants_code));
    assert!(at.claude.is_running());

    at.model.send(Intent::CancelSignIn);
    at.told.until("the sign-in over", |told| {
        told.last().is_some_and(|last| last.signing_in.is_none())
    });
    eventually("the tool stopped", || !at.claude.is_running());
    let last = at.model.snapshot();
    assert_eq!(labels(&last), ["work"], "nothing enrolled");
    assert_eq!((last.failure, last.sheet_failure), (None, None));
    assert!(at.claude.typed().is_empty());

    at.model.shutdown();
    eventually("every thread let go of the core", || {
        Arc::strong_count(&at.core) == 1
    });
}

/// Cancel and then Sign In at once, on the real core with a stand-in for `claude`, as a person
/// can press one after the other: the new sign-in starts once the one cancelled before it has
/// stopped, its tool exited and the core's one sign-in at a time let go, and asks for a code.
/// It was refused as a sign-in already waiting, since the stop runs on the lane of sign-in
/// calls while the new sign-in started at once on a thread of its own.
#[test]
#[cfg(unix)]
fn a_sign_in_asked_for_at_once_after_a_cancel_starts_once_that_one_has_stopped() {
    let at = signing_in_world("cancel-then-sign-in");
    at.model.send(sign_in("travel"));
    let told = at.told.until("a code asked for", signing(|s| s.wants_code));
    let first = told
        .last()
        .and_then(|last| last.signing_in.as_ref())
        .expect("the first sign-in")
        .id;
    at.model.send(Intent::CancelSignIn);
    at.model.send(sign_in("travel"));
    let told = at
        .told
        .until("a code asked for again, or a refusal", |told| {
            told.last().is_some_and(|last| {
                last.failure.is_some()
                    || last
                        .signing_in
                        .as_ref()
                        .is_some_and(|signing| signing.id != first && signing.wants_code)
            })
        });
    let last = told.last().expect("a snapshot");
    assert_eq!(last.failure, None, "the second sign-in refused");
    assert!(at.claude.is_running(), "the second sign-in's tool");

    at.model.send(Intent::CancelSignIn);
    at.model.shutdown();
    assert!(!at.claude.is_running());
}

/// Cancel and then Sign In at once, where the tool cancelled leaves a program it started
/// running with its output open, as Codex's npm launcher leaves `codex login` once it is
/// killed. The new sign-in starts once the stop has stopped the tool, waited for it and let go
/// of the core's one sign-in at a time, and asks for a code while that output is still open.
/// It waited for the thread of the one cancelled, which ends only once its tool's output has
/// closed, and showed it as starting all that while.
#[test]
#[cfg(unix)]
fn a_sign_in_after_a_cancel_waits_for_no_output_the_tool_stopped_left_open() {
    let at = signing_in_world_with(
        "cancel-output-held",
        World::claude_stand_in_holding_its_output,
    );
    at.model.send(sign_in("travel"));
    let told = at.told.until("a code asked for", signing(|s| s.wants_code));
    let first = told
        .last()
        .and_then(|last| last.signing_in.as_ref())
        .expect("the first sign-in")
        .id;
    assert!(at.claude.holds_output(), "its output held open");

    at.model.send(Intent::CancelSignIn);
    at.model.send(sign_in("travel"));
    let told = at
        .told
        .until("a code asked for again, or a refusal", |told| {
            told.last().is_some_and(|last| {
                last.failure.is_some()
                    || last
                        .signing_in
                        .as_ref()
                        .is_some_and(|signing| signing.id != first && signing.wants_code)
            })
        });
    let last = told.last().expect("a snapshot");
    assert_eq!(last.failure, None, "the second sign-in refused");
    assert!(at.claude.is_running(), "the second sign-in's tool");
    assert!(
        at.claude.holds_output(),
        "the first's output still open as the second asks for a code"
    );

    at.model.send(Intent::CancelSignIn);
    at.model.shutdown();
    assert!(!at.claude.is_running());
    at.claude.let_go_of_output();
    eventually("every thread let go of the core", || {
        Arc::strong_count(&at.core) == 1
    });
}

/// Shutting the model down stops a sign-in under way, and has stopped it by the time it
/// returns: nothing is left to say it is over, so its tool is stopped and its thread ends,
/// letting go of the core. Without the stop the thread waited on the tool, and the tool on a
/// browser, for ever. An app shuts the model down as it quits, and its process ends once that
/// returns, which ends no tool it started: a stop still under way then never comes.
#[test]
#[cfg(unix)]
fn shutting_down_stops_a_sign_in_under_way() {
    let at = signing_in_world("shutdown-signing-in");
    at.model.send(sign_in("travel"));
    at.told.until("a code asked for", signing(|s| s.wants_code));
    let private = at.world.pitboard_dir().join("signin");
    assert!(at.claude.is_running() && private.exists());
    at.model.shutdown();
    // The directory the tool signed in to goes as the sign-in is stopped, once its tool has
    // been stopped and waited for; looking for it takes no time to speak of, where asking
    // whether the tool runs starts a shell, long enough for a stop made elsewhere to land.
    assert!(!private.exists(), "stopped before shutdown returned");
    assert!(!at.claude.is_running());
    eventually("every thread let go of the core", || {
        Arc::strong_count(&at.core) == 1
    });
}

/// AppModelTests.swift's aSignInThatCannotStartIsReported on the real core, where Claude
/// Code's program is nowhere it looks: the sheet it was started from says the core's own
/// sentence, under its code, and nothing is half-shown.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_sign_in_the_core_cannot_start_says_so_in_its_sheet() {
    let world = World::new("no-claude");
    world.enrolled("work", "here", 10.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the account read", shows("work"));
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { provider: None },
    });
    model.send(sign_in("travel"));
    let told = told.until("the failure", |told| {
        told.last().is_some_and(|last| last.sheet_failure.is_some())
    });
    let last = told.last().expect("a snapshot");
    let failure = last.sheet_failure.as_ref().expect("said in the sheet");
    assert_eq!(failure.title, "Couldn’t sign in to travel");
    assert_eq!(failure.code.as_deref(), Some("claude_program_missing"));
    assert_eq!(last.signing_in, None);
    assert_eq!(last.sheet, Some(Sheet::Add { provider: None }));
    assert_eq!(last.failure, None);
    model.shutdown();
}

/// A person's clock that says something new each time it is asked, and asks the model for its
/// snapshot as it answers, where it has been given the model.
#[derive(Default)]
struct Counting {
    asked: AtomicUsize,
    /// How many times it asked the model back from inside a call.
    inside: AtomicUsize,
    model: OnceLock<Weak<PitboardModel>>,
}

impl LocalTime for Counting {
    fn clock(&self, _epoch: i64, _with_weekday: bool) -> Result<String, PlatformError> {
        if let Some(model) = self.model.get().and_then(Weak::upgrade) {
            let _ = model.snapshot();
            self.inside.fetch_add(1, Ordering::SeqCst);
        }
        Ok(format!(
            "{:02}:00",
            self.asked.fetch_add(1, Ordering::SeqCst) % 24
        ))
    }

    fn same_day(&self, _first: i64, _second: i64) -> Result<bool, PlatformError> {
        Ok(true)
    }

    fn date_and_time(&self, epoch: i64) -> Result<String, PlatformError> {
        self.clock(epoch, true)
    }
}

/// A model over `core` telling `told`, its clock times said by `clock`, doing what it does by
/// itself every `cadence`.
fn model_telling_time(
    core: &Arc<AppCore>,
    told: &Arc<Told>,
    clock: &Arc<Counting>,
    cadence: Cadence,
) -> Arc<PitboardModel> {
    let model = PitboardModel::over(
        Arc::clone(core),
        Arc::clone(told) as Arc<dyn ModelListener>,
        Platform {
            apps: StandInApps::new(&[], true),
            notifications: Arc::new(Posted::default()),
            local_time: Arc::clone(clock) as Arc<dyn LocalTime>,
            earlier: None,
            windows: None,
        },
        cadence,
    );
    let _ = told.model.set(Arc::downgrade(&model));
    model
}

/// A read that has landed and been said, with when it was.
fn read_and_said(told: &[Snapshot]) -> bool {
    told.last().is_some_and(|last| {
        !last.reading && last.status.is_some() && last.updated_menu.starts_with("Updated")
    })
}

/// The person's clock is asked as a snapshot is made, with no lock held: a clock that asks
/// the model for its snapshot as it answers, which an app's code may, is answered, and the
/// snapshot it is part of is told. Asked under the lock `snapshot` takes, it would have
/// waited on itself, and nothing more been told.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_persons_clock_is_asked_with_no_lock_held() {
    let world = World::new("clock-inside");
    world.enrolled("work", "here", 10.0);
    let told = Arc::new(Told::default());
    let clock = Arc::new(Counting::default());
    let model = model_telling_time(&world.core(), &told, &clock, QUICK);
    let _ = clock.model.set(Arc::downgrade(&model));
    model.send(Intent::Start);
    told.until("the read said with its time", read_and_said);
    assert!(clock.inside.load(Ordering::SeqCst) > 0);
    model.shutdown();
}

/// Once started, what is shown is made again every so often for the time alone, and told
/// where it says something new, with nothing sent and nothing else due: the minute tick, run
/// here every few milliseconds.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_minute_tick_makes_what_is_shown_again() {
    let world = World::new("tick");
    world.enrolled("work", "here", 10.0);
    let told = Arc::new(Told::default());
    let clock = Arc::new(Counting::default());
    let ticking = Cadence {
        look_every: Duration::from_secs(3_600),
        read_every: Duration::from_secs(3_600),
        tick_every: Duration::from_millis(20),
        ..QUICK
    };
    let model = model_telling_time(&world.core(), &told, &clock, ticking);
    model.send(Intent::Start);
    let after_read = told
        .until("the read said with its time", read_and_said)
        .len();
    let snapshots = told.until("the time said again with nothing sent", |snapshots| {
        snapshots.len() >= after_read + 3
    });
    assert!(in_order(&snapshots), "{snapshots:#?}");
    model.shutdown();
}

/// Renaming and forgetting from the app reach the core, and the accounts are read after
/// each: the rename names the account anew, and forgetting takes it away.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn renaming_and_forgetting_reach_the_core() {
    let world = World::new("rename-forget");
    world.enrolled("work", "here", 10.0);
    world.parked("spare", "there", 20.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    let has = |want: &'static [&'static str]| {
        move |told: &[Snapshot]| {
            told.last()
                .is_some_and(|last| !last.reading && labels(last) == want)
        }
    };
    told.until("both accounts read", has(&["work", "spare"]));

    model.send(Intent::Rename {
        provider: "claude".into(),
        label: "spare".into(),
        to: "home".into(),
    });
    told.until("the rename", has(&["work", "home"]));
    model.send(Intent::Forget {
        qualified: "claude/home".into(),
    });
    told.until("the account forgotten", has(&["work"]));
    let last = model.snapshot();
    assert_eq!((last.failure, last.sheet_failure), (None, None));
    model.shutdown();
}

/// Naming the login signed in now from its sheet enrols it with the core, and the sheet
/// closes once it has.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn naming_the_login_signed_in_now_enrols_it() {
    let world = World::new("name");
    world.signed_in("here", 10.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the login read", |told| {
        told.last().is_some_and(|last| {
            !last.reading && matches!(last.footing, crate::Footing::Unnamed { .. })
        })
    });
    model.send(Intent::PresentSheet {
        sheet: Sheet::Name {
            provider: "claude".into(),
            email: "here@example.com".into(),
        },
    });
    model.send(Intent::Enrol {
        provider: "claude".into(),
        name: " work ".into(),
    });
    // The sheet closes as the enrolment answers, and the read after it lists the account:
    // the poll leaves the account index alone meanwhile, the enrolment being the app's own.
    let told = told.until("the login enrolled", |told| {
        told.last()
            .is_some_and(|last| labels(last) == ["work"] && last.sheet.is_none())
    });
    let last = told.last().expect("a snapshot");
    assert_eq!(last.sheet_failure, None);
    assert_eq!(last.failure, None);
    model.shutdown();
}

/// The last snapshot told is of a read that has landed after everything asked of the model,
/// and is over, with `want` the accounts' names.
fn read_after<'a>(want: &'a [&'a str]) -> impl Fn(&[Snapshot]) -> bool + 'a {
    move |told| {
        told.last().is_some_and(|last| {
            !last.reading
                && last.updated_at.is_some()
                && last.signing_in.is_none()
                && labels(last) == want
        })
    }
}

/// Makes `change` while a read that started before it is held on the lane of reads, with a
/// look for changes waiting behind it, and lets them go once the change has answered, which
/// leaves nothing read meanwhile. The look then finds the account index as the change wrote
/// it, and lands after the change's answer and before the read the change asked for, which
/// waits behind it. The read is held by the lock usage readings are written under, which it
/// waits for with a newer reading of `who` to record.
///
/// Taken for a change made elsewhere, that look dropped the read after the change as one
/// that started before a change: what the change did was shown only from what is already
/// known, with no time it was read, until something asked for another read, the timer's
/// minutes later where nobody opens the menu or shows the accounts pane. The fixture's
/// sign-in of a Codex account met it 2 times in 140 runs of the whole suite, where its look
/// read the index between the enrolment's write and its answer; held, it meets it each time.
fn with_a_look_behind_a_read(
    world: &World,
    model: &PitboardModel,
    told: &Told,
    who: &str,
    change: impl FnOnce(),
) {
    let recording = world.recording();
    world.measures(who, 30.0);
    let asked = world.api.calls();
    model.send(Intent::Refresh { asked: true });
    eventually("the read held", || world.api.calls() > asked);
    // A look every 10 ms, so one is asked for meanwhile, and waits behind the read.
    std::thread::sleep(Duration::from_millis(200));
    change();
    told.until("the change answered", |told| {
        told.last().is_some_and(|last| last.updated_at.is_none())
    });
    drop(recording);
}

/// A rename the app makes on the real core is read after it, though a look for changes lands
/// after the rename has answered, having found the account index as the rename wrote it. The
/// index's time is set back a minute before the model starts, so that the rename's write
/// moves it whatever second it is made in.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_rename_is_read_after_it_whatever_the_poll_finds_meanwhile() {
    let world = World::new("rename-looked-at");
    world.enrolled("work", "here", 10.0);
    world.index_written_earlier(60);
    let told = Arc::new(Told::default());
    let renaming = model(&world.core(), &told);
    renaming.send(Intent::Start);
    told.until("the account read", read_after(&["work"]));
    with_a_look_behind_a_read(&world, &renaming, &told, "here", || {
        renaming.send(Intent::Rename {
            provider: "claude".into(),
            label: "work".into(),
            to: "office".into(),
        });
    });
    told.until("the read after the rename", read_after(&["office"]));
    renaming.shutdown();
}

/// The same for naming the login signed in now, where no account is enrolled yet, so that
/// there is no account index until the name writes one.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn naming_the_login_in_use_is_read_after_it_whatever_the_poll_finds_meanwhile() {
    let world = World::new("name-looked-at");
    world.signed_in("here", 10.0);
    let told = Arc::new(Told::default());
    let naming = model(&world.core(), &told);
    naming.send(Intent::Start);
    told.until("the login read", |told| {
        told.last().is_some_and(|last| {
            !last.reading
                && last.updated_at.is_some()
                && matches!(last.footing, crate::Footing::Unnamed { .. })
        })
    });
    with_a_look_behind_a_read(&world, &naming, &told, "here", || {
        naming.send(Intent::Enrol {
            provider: "claude".into(),
            name: "work".into(),
        });
    });
    told.until("the read after naming it", read_after(&["work"]));
    naming.shutdown();
}

/// The same for a sign-in, on the real core with a stand-in for `claude`: what it enrols is
/// read after it.
#[test]
#[cfg(unix)]
fn a_sign_in_is_read_after_it_enrols_whatever_the_poll_finds_meanwhile() {
    let mut world = World::new("sign-in-looked-at");
    world.enrolled("work", "here", 10.0);
    let _claude = world.claude_stand_in();
    world.index_written_earlier(60);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the account read", read_after(&["work"]));
    model.send(sign_in("travel"));
    told.until("a code asked for", signing(|s| s.wants_code));
    world.signed_in_privately("away", "access-away", 20.0);
    with_a_look_behind_a_read(&world, &model, &told, "here", || {
        model.send(paste("the-code#the-state"));
    });
    told.until(
        "the read after the sign-in",
        read_after(&["work", "travel"]),
    );
    model.shutdown();
}

/// A model over `core` telling `told`, posting through `posted`, with no other app running.
fn model_posting(
    core: &Arc<AppCore>,
    told: &Arc<Told>,
    posted: &Arc<Posted>,
) -> Arc<PitboardModel> {
    let model = PitboardModel::over(
        Arc::clone(core),
        Arc::clone(told) as Arc<dyn ModelListener>,
        Platform {
            apps: StandInApps::new(&[], true),
            notifications: Arc::clone(posted) as Arc<dyn super::Notifications>,
            local_time: Arc::new(Utc),
            earlier: None,
            windows: None,
        },
        QUICK,
    );
    let _ = told.model.set(Arc::downgrade(&model));
    model
}

/// Advice about `work` running out is shown.
fn advised(told: &[Snapshot]) -> bool {
    told.last().is_some_and(|last| {
        last.notices
            .iter()
            .any(|notice| notice.id == "advice/claude/work/session/")
    })
}

/// On the real core, an account in use that has run out, with another of its tool to switch
/// to, is notified through the app's system once, and the record of it is kept in Pitboard's
/// directory, so the app opened again over the same machine notifies nothing more, though its
/// window still says it: the owner's decision. The Swift kept the record in memory and told
/// it again after every relaunch.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_run_out_is_notified_once_across_a_relaunch() {
    let world = World::new("run-out");
    world.enrolled("work", "here", 100.0);
    world.parked("spare", "there", 10.0);
    let core = world.core();

    let told = Arc::new(Told::default());
    let posted = Arc::new(Posted::default());
    let model = model_posting(&core, &told, &posted);
    model.send(Intent::Start);
    told.until("the advice", advised);
    eventually("the notification posted", || posted.posted().len() == 1);
    let notice = posted.posted().remove(0);
    assert_eq!(notice.title, "work has no 5-hour limit left");
    assert_eq!(notice.body, "spare has 90% of its own left.");
    assert_eq!(notice.switch_to.as_deref(), Some("claude/spare"));
    assert_eq!(notice.subtitle, None);
    let kept = world.pitboard_dir().join("told.json");
    eventually("the record of it kept", || {
        std::fs::read_to_string(&kept).is_ok_and(|text| text.contains("claude/work/session/"))
    });
    model.shutdown();

    let again = Arc::new(Told::default());
    let unposted = Arc::new(Posted::default());
    let reopened = model_posting(&core, &again, &unposted);
    reopened.send(Intent::Start);
    again.until("the advice, said again in the window", advised);
    // Another read, held as it records a newer reading of the spare account until the model's
    // snapshot says it is under way, and over once a snapshot after that says nothing is
    // being read. The listener is told only the newest snapshot waiting, so one saying a read
    // is under way can go untold: counting them, this waited for ever in 4 of 960 copies run
    // 24 at a time on Linux. Nor does Anthropic having been asked say the snapshot is of the
    // read: the actor hands a read to its lane before it publishes, so the snapshot can still
    // be the one from before it, and say nothing is being read.
    let recording = world.recording();
    world.measures("there", 20.0);
    let asked = world.api.calls();
    reopened.send(Intent::Refresh { asked: true });
    eventually("another read under way", || {
        world.api.calls() > asked && reopened.snapshot().reading
    });
    drop(recording);
    eventually("another read over", || !reopened.snapshot().reading);
    assert!(unposted.posted().is_empty(), "notified before the relaunch");
    reopened.shutdown();
}

/// On the real core, with switching by itself turned on in the app's preferences, an account
/// in use at its share is switched from to another with room through the model's lanes: the
/// switch is said in a notification with nothing to switch back to, the activity log records
/// it as automatic and made by the app, and the account switched to stays in use.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn an_account_at_its_share_is_switched_from_by_itself_on_the_real_core() {
    let world = World::new("auto-switch");
    world.enrolled("work", "here", 97.0);
    world.parked("spare", "there", 10.0);
    world.in_use_an_hour_ago();
    std::fs::write(
        world.pitboard_dir().join("app.json"),
        r#"{"has_been_seen":true,"auto_switch":true}"#,
    )
    .expect("the app's preferences");
    let core = world.core();

    let told = Arc::new(Told::default());
    let posted = Arc::new(Posted::default());
    let model = model_posting(&core, &told, &posted);
    model.send(Intent::Start);
    told.until("spare in use", in_use("claude", "spare"));
    eventually("the switch said", || !posted.posted().is_empty());
    let notice = posted.posted().remove(0);
    assert_eq!(notice.title, "Switched Claude Code to spare");
    assert_eq!(
        notice.body,
        "work had used 97% of its 5-hour limit. Sessions already running follow within 33 \
         seconds."
    );
    assert_eq!(notice.switch_to, None);
    let changes = pitboard_core::audit::read(world.ctx(), 20);
    let automatic: Vec<_> = changes
        .iter()
        .filter(|change| change.verb == "auto-switch")
        .map(|change| {
            (
                change.caller.as_str(),
                change.subject.as_str(),
                change.outcome.as_str(),
            )
        })
        .collect();
    assert_eq!(automatic, [("app", "spare", "ok")]);
    model.shutdown();
}

/// An `app.json` that is there and cannot be read, here a directory where the file goes, is
/// left as it is on the real core, and is not taken for a first launch: the window is not
/// asked for. Taken as no file, as it was while any failure to read it read as none, the
/// model opened the window and kept the defaults in its place.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn preferences_that_cannot_be_read_are_not_a_first_launch() {
    let world = World::new("prefs-unreadable");
    world.enrolled("work", "here", 100.0);
    world.parked("spare", "there", 10.0);
    let file = world.pitboard_dir().join("app.json");
    std::fs::create_dir_all(&file).expect("a directory where the file goes");
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    // Advice waits for what the model keeps, so once it is shown the preferences are in.
    let shown = told.until("the advice", advised);
    assert_eq!(
        shown.last().expect("a snapshot").window_request.serial,
        0,
        "not opened as though for the first time"
    );
    model.shutdown();
    assert!(file.is_dir(), "left as it was");
}

/// The app's preferences are kept in Pitboard's directory on the real core, as the core keeps
/// its own files, and so they follow it: a "Not Now" said over one machine's directory says
/// nothing in another's, which takes what the app's earlier store held, once. The Swift kept
/// them in UserDefaults, one store for every Pitboard directory: the owner's decision moved
/// them.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_apps_preferences_follow_its_pitboard_directory() {
    let one = World::new("prefs-one");
    one.enrolled("work", "here", 10.0);
    let told = Arc::new(Told::default());
    let first_model = model(&one.core(), &told);
    first_model.send(Intent::Start);
    let nudged = |told: &[Snapshot]| {
        told.last()
            .is_some_and(|last| !last.reading && last.setup.is_some())
    };
    let first = told.until("the nudge", nudged);
    let last = first.last().expect("a snapshot");
    assert_eq!(
        last.window_request.serial, 1,
        "the first launch opens the window"
    );
    first_model.send(Intent::DeclineSecondAccount {
        provider: "claude".into(),
    });
    told.until("the nudge declined", |told| {
        told.last().is_some_and(|last| last.setup.is_none())
    });
    let kept = one.pitboard_dir().join("app.json");
    eventually("the preferences kept", || {
        std::fs::read_to_string(&kept).is_ok_and(|text| {
            text == r#"{"second_account_declined":["claude"],"has_been_seen":true}"#
        })
    });
    assert!(
        pitboard_core::testing::fs::is_private(&kept).expect("kept"),
        "private, as the core's own files are"
    );
    first_model.shutdown();

    // Opened again over the same directory, it reads them back: no nudge, and no window.
    let reopened = Arc::new(Told::default());
    let again = model(&one.core(), &reopened);
    again.send(Intent::Start);
    let told = reopened.until("the account read again", |told| {
        told.last()
            .is_some_and(|last| !last.reading && last.status.is_some())
    });
    let last = told.last().expect("a snapshot");
    assert_eq!(last.setup, None, "declined, as kept");
    assert_eq!(last.window_request.serial, 0, "seen, as kept");
    again.shutdown();

    let other = World::new("prefs-other");
    other.enrolled("work", "here", 10.0);
    let elsewhere = Arc::new(Told::default());
    let again = model(&other.core(), &elsewhere);
    again.send(Intent::Start);
    elsewhere.until("the nudge, in another directory", nudged);
    again.shutdown();
}

/// The last snapshot told, once `done` holds of it.
fn last_where(told: &Told, what: &str, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
    told.until(what, |told| told.last().is_some_and(&done))
        .pop()
        .expect("a snapshot")
}

/// The machine over the real core, as the settings and the window's other panes ask for it:
/// daily renewal turned on and off through the core's pretend scheduler, which writes its job
/// into the scratch home and asks no service manager; a renewal and the read after it;
/// doctor's checks; every change Pitboard made, the schedule's among them; and the
/// `pitboard` a terminal would run, looked for in the scratch home alone.
#[test]
#[cfg(unix)]
fn the_machine_is_kept_over_the_real_core() {
    use super::Pane;
    use crate::{FoundCommandLine, Schedule};
    let mut world = World::new("machine");
    world.enrolled("work", "here", 42.0);
    world.app_with_a_command_line();
    let core = world.core();
    let told = Arc::new(Told::default());
    let model = model(&core, &told);
    model.send(Intent::Start);
    told.until("the account read", shows("work"));

    model.send(Intent::ReadSchedule);
    let read = last_where(&told, "the schedule read", |last| {
        last.machine.schedule.schedule == Some(Schedule::Absent)
    });
    let schedule = &read.machine.schedule;
    assert!(schedule.enabled && !schedule.on, "{schedule:?}");
    assert_eq!(schedule.note, None, "this copy can schedule it");

    model.send(Intent::SetSchedule { on: true });
    let on = last_where(&told, "daily renewal on", |last| {
        let schedule = &last.machine.schedule;
        schedule.on && !schedule.changing && schedule.scheduled_in.is_some()
    });
    let job = on.machine.schedule.scheduled_in.clone().expect("where");
    assert!(
        job.starts_with(&*world.dir("").to_string_lossy()),
        "written in the scratch home: {job}"
    );
    assert!(std::path::Path::new(&job).is_file());
    assert_eq!(on.machine.schedule.runs.as_deref(), Some("Every day"));

    // Waited for by what the renewal says: the listener is told only the newest snapshot
    // waiting, so one saying it is under way can go untold.
    model.send(Intent::RenewNow);
    let renewed = last_where(&told, "the renewal and the read after it", |last| {
        !last.machine.renewal.renewing
            && !last.reading
            && last.machine.renewal.note == "No parked login was due."
    });
    assert_eq!(renewed.machine.renewal.note, "No parked login was due.");

    model.send(Intent::PaneShown {
        pane: Pane::Activity,
    });
    let logged = last_where(&told, "the log", |last| {
        !last.machine.activity.lines.is_empty()
    });
    let lines: Vec<(&str, &str, &str, &str)> = logged
        .machine
        .activity
        .lines
        .iter()
        .map(|line| {
            (
                line.change.as_str(),
                line.account.as_str(),
                line.result.as_str(),
                line.asked_by.as_str(),
            )
        })
        .collect();
    assert_eq!(
        lines.first(),
        Some(&("Schedule", "install", "Done", "Pitboard app")),
        "{lines:?}"
    );
    assert!(
        lines.contains(&("Enrol", "work", "Done", "Command line")),
        "{lines:?}"
    );

    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    let checked = last_where(&told, "the checks", |last| {
        !last.machine.checks.lines.is_empty() && !last.machine.checks.checking
    });
    assert!(checked.machine.checks.summary.is_some());
    assert!(checked.machine.checks.checked.is_some());

    model.send(Intent::SetSchedule { on: false });
    last_where(&told, "daily renewal off", |last| {
        last.machine.schedule.schedule == Some(Schedule::Absent) && !last.machine.schedule.changing
    });
    assert!(!std::path::Path::new(&job).exists(), "taken away");

    model.send(Intent::LookForCommandLine);
    let found = last_where(&told, "the command line looked for", |last| {
        last.machine.command_line.found.is_some()
    });
    let command_line = &found.machine.command_line;
    assert_eq!(command_line.found, Some(FoundCommandLine::Nowhere));
    assert_eq!(command_line.in_terminal.as_deref(), Some("Not installed"));
    assert!(command_line.offers_link);
    model.shutdown();
}

/// An account 0.8.0 enrolled keeps its window and the sign-in in it: the window's store is
/// derived from the account's id, which an account enrolled then keeps.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn an_account_enrolled_by_0_8_0_keeps_its_window() {
    let world = World::new("windows-0-8-0");
    world.enrolled("work", "here", 10.0);
    let file = world.pitboard_dir().join("state.json");
    let mut written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("the account index"))
            .expect("JSON");
    written["schema"] = 4.into();
    written["accounts"][0]
        .as_object_mut()
        .expect("an account")
        .remove("id");
    std::fs::write(&file, written.to_string()).expect("written as 0.8.0 writes it");

    let accounts = world.core().status(false).expect("a read").accounts;
    let stores: Vec<String> = crate::account_windows::window_accounts(accounts)
        .into_iter()
        .map(|window| window.store)
        .collect();
    assert_eq!(
        stores,
        [crate::account_windows::store_id(
            (&pitboard_sites::CLAUDE).into(),
            "here".into()
        )]
    );
}
/// The account windows' records, kept by the lanes in a directory of the app's own, here the
/// scratch home's: what the app's earlier store held is taken once, in its own upper case, and
/// written private to its owner; a store no enrolled account derives is asked of the app and
/// taken off the record once deleted; and an account forgotten in a terminal has its window
/// closed and its store asked for, the record keeping it until the app has deleted it.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_windows_records_are_kept_in_the_apps_own_directory() {
    use super::{DownloadEnd, EarlierWindowRecords, WindowsLaunch, WindowsPlace};
    use std::collections::HashMap;

    let world = World::new("windows");
    world.enrolled("work", "here", 10.0);
    world.parked("spare", "there", 5.0);
    let directory = world.dir("Application Support").join("app");
    let file = directory.join("windows.json");
    let key = world.pitboard_dir().to_string_lossy().into_owned();
    let store = |label: &str| {
        let enrolled = world.elsewhere().account(label).expect("enrolled");
        crate::account_windows::store_id((&pitboard_sites::CLAUDE).into(), enrolled.id)
    };
    let (work, spare) = (store("work"), store("spare"));
    let gone = "00000000-0000-4000-8000-000000000001";
    let read = || std::fs::read_to_string(&file).unwrap_or_default();
    let told = Arc::new(Told::default());
    let model = PitboardModel::over(
        world.core(),
        Arc::clone(&told) as Arc<dyn ModelListener>,
        Platform {
            windows: Some(WindowsPlace {
                launch: WindowsLaunch {
                    directory: directory.to_string_lossy().into_owned(),
                    key: key.clone(),
                    link_scheme: "pitboard".into(),
                    earlier: Some(EarlierWindowRecords {
                        stores: HashMap::from([(
                            key.clone(),
                            vec![gone.to_uppercase(), work.to_uppercase()],
                        )]),
                        pages: HashMap::new(),
                    }),
                },
                web_scheme: "https".into(),
            }),
            ..platform(StandInApps::new(&[], true))
        },
        QUICK,
    );
    let _ = told.model.set(Arc::downgrade(&model));
    model.send(Intent::Start);
    last_where(&told, "the store nobody has asked for", |last| {
        last.account_windows
            .deleting
            .iter()
            .map(|asked| asked.store.as_str())
            .eq([gone])
    });
    let kept = read();
    assert!(kept.contains(gone) && kept.contains(&work), "{kept}");
    assert!(pitboard_core::testing::fs::is_private(&file).expect("there"));
    model.send(Intent::StoreDeleted {
        store: gone.to_uppercase(),
    });
    eventually("the deleted store taken off the record", || {
        !read().contains(gone)
    });

    for opened in [&work, &spare] {
        model.send(Intent::WindowOpened {
            store: opened.clone(),
        });
    }
    let shown = last_where(&told, "both windows open", |last| {
        last.account_windows.open.len() == 2
    });
    let opened = &shown.account_windows.open[0];
    assert_eq!(opened.note, None, "the earlier store had made it");
    assert_eq!(opened.load.url, "https://claude.ai/");
    assert!(read().contains(&spare), "recorded before it opened");
    model.send(Intent::DownloadStarted {
        id: "1".into(),
        store: spare.clone(),
        name: None,
    });
    last_where(&told, "the download under way", |last| {
        last.account_windows
            .downloads
            .iter()
            .any(|download| download.running)
    });
    model.send(Intent::DownloadEnded {
        id: "1".into(),
        end: DownloadEnd::Cancelled,
    });

    world
        .elsewhere()
        .forget("spare")
        .expect("forgotten in a terminal");
    world.index_written_later(5);
    let forgotten = last_where(&told, "the forgotten account's window closing", |last| {
        last.account_windows.closing == [spare.clone()]
            && last
                .account_windows
                .deleting
                .iter()
                .map(|asked| &asked.store)
                .eq([&spare])
    });
    assert!(
        !forgotten
            .account_windows
            .downloads
            .iter()
            .any(|download| download.running)
    );
    assert!(read().contains(&spare), "recorded until deleted");
    model.send(Intent::WindowClosed {
        store: spare.clone(),
    });
    model.send(Intent::StoreDeleted {
        store: spare.clone(),
    });
    eventually("the forgotten account's store taken off the record", || {
        !read().contains(&spare)
    });
    model.shutdown();
}

/// Where Pitboard runs as root or under sudo, the app writes no file of its own either:
/// the account windows' records are kept as they are, here not there at all, while the
/// windows' books are kept in memory as before. A record written as root would be root's.
///
/// The model still asks the platform to delete the website data of a store nobody has asked
/// for, which the test waits on only as the sign that the launch's bookkeeping ran. That is
/// not meant: an elevated app is to be refused whole, which the app phase of the Windows
/// plan does, and until then the platform's delete is not gated.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_windows_records_are_not_written_where_pitboard_may_change_nothing() {
    use super::{EarlierWindowRecords, WindowsLaunch, WindowsPlace};
    use std::collections::HashMap;

    let world = World::new("windows-elevated");
    world.enrolled("work", "here", 10.0);
    let work = crate::account_windows::store_id(
        (&pitboard_sites::CLAUDE).into(),
        world.elsewhere().account("work").expect("enrolled").id,
    );
    world
        .host
        .runs_with(pitboard_core::host::Elevation::Elevated { why: "as root" });
    let directory = world.dir("Application Support").join("app");
    let key = world.pitboard_dir().to_string_lossy().into_owned();
    let gone = "00000000-0000-4000-8000-000000000001";
    let told = Arc::new(Told::default());
    let model = PitboardModel::over(
        world.core(),
        Arc::clone(&told) as Arc<dyn ModelListener>,
        Platform {
            windows: Some(WindowsPlace {
                launch: WindowsLaunch {
                    directory: directory.to_string_lossy().into_owned(),
                    key: key.clone(),
                    link_scheme: "pitboard".into(),
                    earlier: Some(EarlierWindowRecords {
                        stores: HashMap::from([(
                            key.clone(),
                            vec![gone.to_uppercase(), work.to_uppercase()],
                        )]),
                        pages: HashMap::new(),
                    }),
                },
                web_scheme: "https".into(),
            }),
            ..platform(StandInApps::new(&[], true))
        },
        QUICK,
    );
    let _ = told.model.set(Arc::downgrade(&model));
    model.send(Intent::Start);
    last_where(&told, "the store nobody has asked for", |last| {
        last.account_windows
            .deleting
            .iter()
            .map(|asked| asked.store.as_str())
            .eq([gone])
    });
    model.shutdown();
    assert!(
        !directory.join("windows.json").exists(),
        "nothing is written as root"
    );
}
