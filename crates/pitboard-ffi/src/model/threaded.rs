//! The model as an app has it: its mailbox, its actor, its lanes and its listener, over the
//! real core on a machine of the test's own. What these prove is what no test of
//! `State::apply` can: that snapshots reach the listener one at a time and in order, that a
//! listener may call the model back, stopping it too, and that stopping or dropping the
//! model ends every thread it started, a drop waiting on none of them.

use super::state::Cadence;
use super::testing::platform::{PATIENCE, Posted, StandInApps, Told, eventually};
use super::testing::records::{CHATGPT, CHATGPT_CODEX, percent, read_in, warning};
use super::testing::world::StandIn;
use super::testing::world::World;
use super::{
    AppControl, Intent, LocalTime, ModelListener, PitboardModel, Platform, PlatformError, Sheet,
    Snapshot,
};
use crate::AppCore;
use crate::present::testing::Utc;
use crate::present::{AccountItem, PanelNotice, words};
use pitboard_core::host::OS;
use pitboard_core::state::Key;
use pitboard_core::testing::{Asked, live_service};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

/// Looks every few milliseconds, reads by itself only when started, once, asks whether to
/// switch by itself only after a read or a change, and gives an app a moment to quit.
const QUICK: Cadence = Cadence {
    look_every: Duration::from_millis(10),
    read_every: Duration::from_secs(3_600),
    decide_every: Duration::from_secs(3_600),
    stale_after: Duration::from_secs(60),
    quit_within: Duration::from_millis(200),
    quit_checked_every: Duration::from_millis(5),
    tick_every: Duration::from_secs(3_600),
    arming: Duration::from_millis(10),
};

fn rows(snapshot: &Snapshot) -> impl Iterator<Item = &AccountItem> {
    snapshot
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
}

fn labels(snapshot: &Snapshot) -> Vec<String> {
    rows(snapshot)
        .filter_map(|row| row.qualified.as_deref())
        .map(|qualified| Key::parse(qualified).label)
        .collect()
}

// The account a switch is under way for, as its row says.
fn switching(snapshot: &Snapshot) -> Option<&str> {
    rows(snapshot)
        .find(|row| row.switching())
        .and_then(|row| row.qualified.as_deref())
}

fn notice<'a>(snapshot: &'a Snapshot, id: &str) -> Option<&'a PanelNotice> {
    snapshot.notices.iter().find(|notice| notice.id == id)
}

// Whether a notice says a read's `code` warning.
fn warns(snapshot: &Snapshot, code: &str) -> bool {
    let id = format!("warning/{code}/");
    snapshot
        .notices
        .iter()
        .any(|notice| notice.id.starts_with(&id))
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
        glance: None,
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
/// Before it is started it shows nothing, and no timer wakes it, as cadence.rs's
/// `nothing_runs_by_itself_until_started` shows in the model's own time.
#[test]
fn a_started_model_reads_the_accounts_and_tells_its_listener_in_order() {
    let world = World::new("reads");
    world.enrolled("work", "here", 42.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    let first = model.snapshot();
    assert_eq!((first.revision, first.sections.is_empty()), (0, true));

    model.send(Intent::Start);
    let snapshots = told.until("the account read", shows("work"));
    assert!(in_order(&snapshots), "{snapshots:#?}");
    let last = snapshots.last().expect("a snapshot");
    assert_eq!(percent(last, 0), Some(42.0));
    eventually("told the newest snapshot", || {
        last_revision(&told) == Some(model.snapshot().revision)
    });
    model.shutdown();
}

// The Windows app made Pitboard's directory again after `pitboard uninstall` took it away
// (session of 2026-10-18), so the command line refuses while a model runs, and goes on once
// it has stopped.
#[test]
fn uninstall_refuses_while_the_app_runs() {
    let world = World::new("uninstall-open");
    world.enrolled("work", "here", 42.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the account read", shows("work"));
    let terminal = world.elsewhere();
    let refused = terminal.uninstall().expect_err("the app is open");
    assert_eq!(refused.error.code(), "pitboard_open");
    model.shutdown();
    drop(model);
    eventually("the app let go", || terminal.uninstall().is_ok());
}

// The app removes Pitboard holding app.lock, and nothing asked after writes the directory again.
#[test]
fn after_uninstall_nothing_writes_pitboards_directory_again() {
    let world = World::new("uninstall-from-app");
    world.enrolled("work", "here", 42.0);
    world.parked("spare", "there", 10.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the accounts read", |snapshots| {
        snapshots
            .last()
            .is_some_and(|last| !last.reading && labels(last) == ["work", "spare"])
    });

    model.send(Intent::Uninstall);
    told.until("Pitboard removed", |snapshots| {
        snapshots
            .last()
            .is_some_and(|last| last.removal == Some(crate::RemovalShown::Done))
    });
    assert!(!world.pitboard_dir().exists());
    for intent in [
        Intent::Refresh { asked: true },
        Intent::Woke,
        Intent::Glanced,
        Intent::SwitchTo {
            qualified: "claude/spare".into(),
        },
    ] {
        model.send(intent);
    }
    model.shutdown();
    assert!(!world.pitboard_dir().exists());
}

// Through the lane of what the model keeps, into a folder it makes private where it is not there.
#[test]
fn the_glance_is_written_to_the_file_the_app_names() {
    let world = World::new("glance");
    world.enrolled("work", "here", 42.0);
    let glance = world
        .pitboard_dir()
        .with_file_name("group")
        .join("glance.json");
    let told = Arc::new(Told::default());
    let model = PitboardModel::over(
        world.core(),
        Arc::clone(&told) as Arc<dyn ModelListener>,
        Platform {
            glance: Some(glance.clone()),
            ..platform(StandInApps::new(&[], true))
        },
        QUICK,
    );
    model.send(Intent::Start);
    told.until("the account read", shows("work"));
    eventually("the glance written", || {
        std::fs::read_to_string(&glance).is_ok_and(|text| text.contains(r#""name":"work""#))
    });
    assert!(model.snapshot().glance_revision > 0);
    model.shutdown();
}

/// A change another front end makes, here a rename typed in a terminal, reaches the
/// listener within a look or two, read from what is already known: nobody is asked.
#[test]
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
    // Nothing holds the listener but this test any more, so nothing can tell it.
    assert_eq!(told.count(), count, "told nothing after shutdown");
    assert_eq!(labels(&model.snapshot()), ["work"]);
    model.shutdown();
}

/// A listener may stop the model from inside `changed`: `shutdown` waits for the actor alone,
/// which never waits on the listener, and not for the thread it is called on. The call it is
/// made from is the last the listener is told of, and every thread ends.
#[test]
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
    let read = snapshots.iter().position(read_in);
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
fn signed_in(snapshot: &Snapshot, provider: &str) -> Option<String> {
    rows(snapshot)
        .filter(|row| row.in_use)
        .filter_map(|row| row.qualified.as_deref())
        .map(Key::parse)
        .find(|key| key.provider.code() == provider)
        .map(|key| key.label)
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
                && switching(last).is_none()
                && signed_in(last, provider).as_deref() == Some(label)
        })
    }
}

/// The real core's Codex switch, through the model's lanes, with two `codex` sessions
/// running: the warning that they keep the account left and must not sign out reaches the
/// snapshot as what Codex's last switch said, and outlasts the read after the switch, which
/// says nothing of it. The switch is said to be under way until that read has landed.
#[test]
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
        snapshots.iter().any(|s| switching(s) == Some("codex/work")),
        "said to be under way"
    );
    let last = snapshots.last().expect("a snapshot");
    assert_eq!(last.failure_alert, None);
    let said = notice(last, "switch/codex")
        .unwrap_or_else(|| panic!("the switch's notice: {:#?}", last.notices));
    assert_eq!(said.until, None, "a running codex never follows");
    let warned = said
        .lines
        .iter()
        .find(|line| line.contains("2 codex sessions"))
        .expect("the sessions still running are warned about");
    assert!(warned.contains("personal’s sign-in"), "{warned:?}");
    assert!(
        !warns(last, "sessions_still_running"),
        "the read after it does not say it again: {:#?}",
        last.notices
    );
    model.shutdown();
}

/// A switch waits for the app holding the login to be quit, through the lanes over the real
/// core, whose holder detection reads ChatGPT's own `codex` in the process list. The switch
/// waits for the person; told to go ahead, ChatGPT is asked to quit, the switch is made once it
/// has, with nothing left running the old login to warn about, and ChatGPT is opened again.
#[test]
fn quitting_the_app_holding_the_login_switches_and_opens_it_again() {
    let world = two_codex_accounts("codex-quit");
    // Seen before: at a first launch the question comes up over the welcome.
    std::fs::write(
        world.pitboard_dir().join("app.json"),
        r#"{"has_been_seen":true}"#,
    )
    .expect("the app's preferences");
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
    assert_eq!(question.question.title, words::quit_to_switch("ChatGPT"));
    assert_eq!(switching(asked), Some("codex/work"));
    assert_eq!(asked.window_request.place, Some(super::Place::Accounts));
    assert!(
        apps.asked().is_empty(),
        "nothing is quit before the person says so"
    );
    assert_eq!(signed_in(asked, "codex").as_deref(), Some("personal"));

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
    assert_eq!(last.failure_alert, None);
    let said = notice(last, "switch/codex")
        .unwrap_or_else(|| panic!("the switch's notice: {:#?}", last.notices));
    assert_eq!(
        said.lines,
        [words::restart_notice("codex", "personal")],
        "switched once ChatGPT had quit, with nothing left running to warn about"
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
    assert_eq!(last.failure_alert, None);
    let said = notice(last, "switch/codex")
        .unwrap_or_else(|| panic!("the switch's notice: {:#?}", last.notices));
    assert_eq!(said.severity, crate::present::Severity::Warning);
    let [restart, unknown] = &said.lines[..] else {
        panic!("the restart line and what is unknown: {:#?}", said.lines);
    };
    assert_eq!(restart, &words::restart_notice("codex", "personal"));
    assert!(unknown.contains("personal’s sign-in"), "{unknown:?}");
    assert!(
        !warns(last, "sessions_unknown"),
        "said in the switch's notice"
    );
    assert!(
        last.notices
            .iter()
            .all(|n| n.title != words::warning_heading(&warning("sessions_unknown", ""))),
        "said once: {:#?}",
        last.notices
    );
    model.shutdown();
}

/// Reads answer while a switch waits, as the Swift service's queue of reads answered while
/// its other queues did: here the switch waits on the app's own code saying whether ChatGPT
/// runs, and a read somebody asks for meanwhile lands, with the switch still under way.
#[test]
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
        world.api.calls() > before && read_in(&model.snapshot())
    });
    assert_eq!(
        model.snapshot().quit_question,
        None,
        "the switch is still waiting"
    );

    drop(release);
    told.until("the quit question", |told| {
        told.last().is_some_and(|last| last.quit_question.is_some())
    });
    model.send(Intent::KeepAppOpen);
    told.until("the question gone", |told| {
        told.last()
            .is_some_and(|last| last.quit_question.is_none() && switching(last).is_none())
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
            .filter(|asked| **asked == Asked::Identity("access-there".into()))
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
        switching(&meanwhile),
        Some("claude/spare"),
        "the switch is still waiting"
    );
    assert_eq!(meanwhile.failure_alert, None);
    assert_eq!(signed_in(&meanwhile, "claude").as_deref(), Some("work"));

    drop(writing);
    let snapshots = told.until("the switch", in_use("claude", "spare"));
    let last = snapshots.last().expect("a snapshot");
    assert_eq!(last.failure_alert, None);
    assert!(
        notice(last, "switch/claude").is_some(),
        "{:#?}",
        last.notices
    );
    model.shutdown();
}

// Signing in, on the real core with a stand-in for `claude`, which is a shell script: so
// these run where one does.

/// What a test of signing in starts from: Claude Code with `work` in use, a stand-in in
/// place of `claude`, and a model over the core read once.
struct SigningInWorld {
    model: Arc<PitboardModel>,
    told: Arc<Told>,
    core: Arc<AppCore>,
    claude: StandIn,
    // Last, as fields are dropped in order: it waits for every core to have gone.
    world: World,
}

fn signing_in_world(name: &str) -> SigningInWorld {
    signing_in_world_with(name, World::claude_stand_in)
}

/// The same, with the stand-in `stand_in` makes.
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
fn signing(wanted: impl Fn(&super::RunningSignIn) -> bool) -> impl Fn(&[Snapshot]) -> bool {
    move |told| {
        told.last()
            .and_then(|last| last.signing_in.as_ref())
            .is_some_and(&wanted)
    }
}

/// The last snapshot told has no sign-in under way, and a read that has landed and is over
/// with `label` among the accounts and `then` true of it.
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

// An add of a Claude Code account, which the core names from its email.
fn sign_in() -> Intent {
    Intent::SignIn {
        provider: "claude".into(),
        again: None,
    }
}

// The last snapshot told has the account just added up to be named, under `label`.
fn naming(label: &str) -> impl Fn(&[Snapshot]) -> bool + '_ {
    move |told| {
        told.last().is_some_and(
            |last| matches!(&last.sheet, Some(Sheet::Name { label: named, .. }) if named == label),
        )
    }
}

fn paste(code: &str) -> Intent {
    Intent::PasteCode { code: code.into() }
}

// The owner's decision, on the real core with a stand-in for `claude` that reads lines as
// Claude Code does (`sign_in_takes_another_code`).
#[test]
fn a_code_claude_code_refused_is_typed_again_into_the_same_sign_in() {
    let at = signing_in_world("paste-again");
    at.model.send(Intent::PresentSheet {
        sheet: Sheet::Add { provider: None },
    });
    at.model.send(sign_in());
    at.told.until("a code asked for", signing(|s| s.wants_code));
    at.world.signed_in_privately("away", "access-away", 20.0);

    at.model.send(paste("half-a-code"));
    let told = at.told.until("the code refused", |told| {
        told.last()
            .and_then(|last| last.signing_in_text.as_ref())
            .is_some_and(|text| text.refused.is_some())
    });
    let refused = told
        .last()
        .and_then(|last| last.signing_in.clone())
        .expect("the sign-in");
    assert!(refused.wants_code, "asked for again");

    at.model.send(paste("the-code#the-state"));
    let told = at
        .told
        .until("away enrolled", signed_in_as("example", |_| true));
    let last = told.last().expect("a snapshot");
    assert_eq!(at.claude.typed(), ["half-a-code", "the-code#the-state"]);
    assert!(
        naming("example")(std::slice::from_ref(last)),
        "the sheet it was started from gives way to the name: {:?}",
        last.sheet
    );
    assert_eq!((&last.failure_alert, &last.sheet_failure), (&None, &None));
    let travel = rows(last)
        .find(|row| row.qualified.as_deref() == Some("claude/example"))
        .expect("away, under the name its email suggests");
    assert!(
        !travel.in_use
            && matches!(&travel.action, Some(action) if matches!(action.intent, Intent::SwitchTo { .. })),
        "{travel:?}"
    );
    assert_eq!(
        signed_in(last, "claude").as_deref(),
        Some("work"),
        "beside the one in use"
    );
    at.model.shutdown();
}

/// On the real core with a stand-in for `claude`, signing in again to the account in use puts
/// its new login in use, keeps it the account in use, and says so beside what Claude Code's
/// last switch said.
#[test]
fn signing_in_again_to_the_account_in_use_puts_its_new_login_in_use() {
    let at = signing_in_world("sign-in-again");
    at.model.send(Intent::PresentSheet {
        sheet: Sheet::SignInAgain {
            provider: "claude".into(),
            label: "work".into(),
        },
    });
    at.model.send(Intent::SignIn {
        provider: "claude".into(),
        again: Some("work".into()),
    });
    at.told.until("a code asked for", signing(|s| s.wants_code));
    at.world
        .signed_in_privately("here", "access-here-again", 10.0);
    at.model.send(paste("the-code#the-state"));

    let told = at.told.until(
        "what the sign-in said",
        signed_in_as("work", |last| notice(last, "switch/claude").is_some()),
    );
    let last = told.last().expect("a snapshot");
    let said = notice(last, "switch/claude").expect("the sign-in's notice");
    assert_eq!(said.lines, [words::signed_in_now("work", true, None)]);
    assert_eq!(signed_in(last, "claude").as_deref(), Some("work"));
    assert_eq!(last.sheet, None);
    assert!(
        at.world.claude_code_uses("access-here-again"),
        "the new login is the one in use"
    );
    at.model.shutdown();
}

/// On the real core with a stand-in for `claude` waiting for a code, cancelling stops the tool,
/// whose process is gone, nothing is typed to it or enrolled, and the sign-in's thread ends.
#[test]
fn cancelling_a_sign_in_stops_its_tool() {
    let at = signing_in_world("cancel");
    at.model.send(sign_in());
    at.told.until("a code asked for", signing(|s| s.wants_code));
    assert!(at.claude.is_running());

    at.model.send(Intent::CancelSignIn);
    at.told.until("the sign-in over", |told| {
        told.last().is_some_and(|last| last.signing_in.is_none())
    });
    eventually("the tool stopped", || !at.claude.is_running());
    let last = at.model.snapshot();
    assert_eq!(labels(&last), ["work"], "nothing enrolled");
    assert_eq!((last.failure_alert, last.sheet_failure), (None, None));
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
fn a_sign_in_asked_for_at_once_after_a_cancel_starts_once_that_one_has_stopped() {
    let at = signing_in_world("cancel-then-sign-in");
    at.model.send(sign_in());
    let told = at.told.until("a code asked for", signing(|s| s.wants_code));
    let first = told
        .last()
        .and_then(|last| last.signing_in.as_ref())
        .expect("the first sign-in")
        .id;
    at.model.send(Intent::CancelSignIn);
    at.model.send(sign_in());
    let told = at
        .told
        .until("a code asked for again, or a refusal", |told| {
            told.last().is_some_and(|last| {
                last.failure_alert.is_some()
                    || last
                        .signing_in
                        .as_ref()
                        .is_some_and(|signing| signing.id != first && signing.wants_code)
            })
        });
    let last = told.last().expect("a snapshot");
    assert_eq!(last.failure_alert, None, "the second sign-in refused");
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
/// closed, and showed it as starting all that while. On Windows the cancel ends every
/// `claude.exe` in the sign-in's job, the one holding that output too.
#[test]
fn a_sign_in_after_a_cancel_waits_for_no_output_the_tool_stopped_left_open() {
    let at = signing_in_world_with(
        "cancel-output-held",
        World::claude_stand_in_holding_its_output,
    );
    at.model.send(sign_in());
    let told = at.told.until("a code asked for", signing(|s| s.wants_code));
    let first = told
        .last()
        .and_then(|last| last.signing_in.as_ref())
        .expect("the first sign-in")
        .id;
    assert!(at.claude.holds_output(), "its output held open");

    at.model.send(Intent::CancelSignIn);
    at.model.send(sign_in());
    let told = at
        .told
        .until("a code asked for again, or a refusal", |told| {
            told.last().is_some_and(|last| {
                last.failure_alert.is_some()
                    || last
                        .signing_in
                        .as_ref()
                        .is_some_and(|signing| signing.id != first && signing.wants_code)
            })
        });
    let last = told.last().expect("a snapshot");
    assert_eq!(last.failure_alert, None, "the second sign-in refused");
    assert!(at.claude.is_running(), "the second sign-in's tool");
    assert_eq!(
        at.claude.holds_output(),
        !cfg!(windows),
        "the first's output still open as the second asks for a code, but on Windows"
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
fn shutting_down_stops_a_sign_in_under_way() {
    let at = signing_in_world("shutdown-signing-in");
    at.model.send(sign_in());
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

// On the real core, where Claude Code's program is nowhere it looks: the sheet says so under
// the tool before Sign In, and a sign-in asked for anyway leaves it said there.
#[test]
fn a_tool_the_core_cannot_find_is_said_missing_in_its_sheet() {
    let world = World::new("no-claude");
    world.enrolled("work", "here", 10.0);
    world.without_claude_code();
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the account read", shows("work"));
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { provider: None },
    });
    let missing = |told: &[Snapshot]| {
        told.last()
            .and_then(|last| last.sheet_text.as_ref())
            .and_then(|text| text.tools.iter().find(|tool| tool.code == "claude"))
            .is_some_and(|tool| tool.missing.is_some())
    };
    told.until("Claude Code said missing", missing);
    model.send(sign_in());
    let told = told.until("the sign-in over", |told| {
        told.last()
            .is_some_and(|last| last.signing_in.is_none() && read_in(last))
            && missing(told)
    });
    let last = told.last().expect("a snapshot");
    assert_eq!(last.sheet_failure, None, "said under the tool");
    assert_eq!(last.sheet, Some(Sheet::Add { provider: None }));
    assert_eq!(last.failure_alert, None);
    model.shutdown();
}

// The report this rebuild began with: Codex enrolled from a terminal, then only the ChatGPT
// app, so no Codex CLI. The tip's Add Account says the CLI is missing before Sign In.
#[test]
fn adding_codex_without_its_cli_says_so_before_sign_in() {
    let world = World::new("chatgpt-app-only");
    world.enrolled("work", "here", 10.0);
    world.codex_enrolled("main", "me");
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the accounts read", |told| {
        told.last()
            .is_some_and(|last| !last.reading && labels(last).iter().any(|l| l == "main"))
    });
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add {
            provider: Some("codex".into()),
        },
    });
    let told = told.until("the sheet", |told| {
        told.last()
            .and_then(|last| last.sheet_text.as_ref())
            .is_some_and(|text| text.tools.iter().any(|tool| tool.missing.is_some()))
    });
    let text = told
        .last()
        .and_then(|last| last.sheet_text.clone())
        .expect("the sheet");
    assert_eq!(text.tool, "codex");
    let codex = text
        .tools
        .iter()
        .find(|tool| tool.code == "codex")
        .expect("codex offered");
    assert_eq!(
        codex.missing,
        words::missing(OS, "codex"),
        "its CLI is not here"
    );
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

    fn time_of_day(&self, minutes: u16) -> Result<String, PlatformError> {
        Ok(format!("{:02}:{:02}", minutes / 60, minutes % 60))
    }

    fn date(&self, epoch: i64) -> Result<String, PlatformError> {
        self.clock(epoch, false)
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
            glance: None,
        },
        cadence,
    );
    let _ = told.model.set(Arc::downgrade(&model));
    model
}

/// A read that has landed and been said, with when it was.
fn read_and_said(told: &[Snapshot]) -> bool {
    told.last().is_some_and(read_in)
}

/// The person's clock is asked as a snapshot is made, with no lock held: a clock that asks
/// the model for its snapshot as it answers, which an app's code may, is answered, and the
/// snapshot it is part of is told. Asked under the lock `snapshot` takes, it would have
/// waited on itself, and nothing more been told.
#[test]
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
    assert_eq!((last.failure_alert, last.sheet_failure), (None, None));
    model.shutdown();
}

/// A login left in a file behind the keychain is put away from its sheet through the real
/// core: the read says the file is there, the sheet says whose the login is, and once it is
/// put away the file is gone and the read after it says nothing more of it.
#[test]
fn putting_away_a_login_left_in_a_file_reaches_the_core() {
    let world = World::new("stow");
    world.enrolled("work", "here", 10.0);
    world.parked("spare", "there", 20.0);
    world.left_in_a_file("there", 20.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    let warns = |told: &[Snapshot]| {
        told.last().is_some_and(|last| {
            last.notices
                .iter()
                .any(|notice| notice.id.starts_with("warning/fallback_login/"))
        })
    };
    told.until("the file said", warns);

    model.send(Intent::PresentSheet { sheet: Sheet::Stow });
    told.until("the sheet's look", |told| {
        told.last()
            .and_then(|last| last.stow_text.as_ref())
            .is_some_and(|text| text.can_confirm)
    });
    let text = model.snapshot().stow_text.expect("the sheet's words");
    assert!(text.message.contains("spare"), "{:?}", text.message);

    model.send(Intent::Stow);
    told.until("put away, and read after", |told| {
        told.last().is_some_and(|last| {
            !last.reading
                && last.sheet.is_none()
                && last.notices.iter().any(|notice| notice.id == "stowed")
                && !warns(std::slice::from_ref(last))
        })
    });
    assert_eq!(world.left_file().peek(&live_service(world.ctx())), None);
    model.shutdown();
}

/// Adding the login signed in now enrols it with the core under the name its email suggests,
/// which the sheet that follows starts on, and a new name renames it.
#[test]
fn adding_the_login_signed_in_now_enrols_it_and_names_it_after() {
    let world = World::new("name");
    world.signed_in("here", 10.0);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the login read", |told| {
        told.last()
            .is_some_and(|last| !last.reading && !last.sections.is_empty())
    });
    model.send(Intent::Enrol {
        provider: "claude".into(),
    });
    // The read after it lists the account: the poll leaves the account index alone
    // meanwhile, the enrolment being the app's own.
    let enrolled = told.until("the login enrolled", |told| {
        naming("example")(told) && told.last().is_some_and(|last| labels(last) == ["example"])
    });
    let last = enrolled.last().expect("a snapshot");
    assert_eq!(last.sheet_failure, None);
    assert_eq!(last.failure_alert, None);
    let sheet = last.sheet.clone().expect("the name");
    let to = crate::present::check_name(sheet, "claude".into(), "work".into())
        .intent
        .expect("a rename");
    model.send(to);
    told.until("renamed", |told| {
        told.last()
            .is_some_and(|last| labels(last) == ["work"] && last.sheet.is_none())
    });
    model.shutdown();
}

// A crash is the index as it stands, read by another core; a quit is the next launch's read.
#[test]
fn quitting_before_the_name_is_saved_keeps_the_account() {
    let at = signing_in_world("quit-before-naming");
    at.model.send(Intent::PresentSheet {
        sheet: Sheet::Add { provider: None },
    });
    at.model.send(sign_in());
    at.told.until("a code asked for", signing(|s| s.wants_code));
    at.world.signed_in_privately("away", "access-away", 20.0);
    at.model.send(paste("the-code#the-state"));
    at.told.until("the name asked for", naming("example"));

    let on_disk = at.world.core().status_offline().expect("the index read");
    assert!(
        on_disk
            .accounts
            .iter()
            .any(|account| account.label.as_deref() == Some("example")),
        "{:?}",
        on_disk.accounts
    );
    at.model.shutdown();
    let told = Arc::new(Told::default());
    let next = model(&at.core, &told);
    next.send(Intent::Start);
    told.until("the account kept", signed_in_as("example", |_| true));
    next.shutdown();
}

/// The last snapshot told is of a read that has landed after everything asked of the model,
/// and is over, with `want` the accounts' names.
fn read_after<'a>(want: &'a [&'a str]) -> impl Fn(&[Snapshot]) -> bool + 'a {
    move |told| {
        told.last()
            .is_some_and(|last| read_in(last) && last.signing_in.is_none() && labels(last) == want)
    }
}

// The look reads the index as `change` wrote it, and lands before the read after the change,
// once what is told says it has `answered`.
fn with_a_look_held(
    model: &PitboardModel,
    told: &Told,
    change: impl FnOnce(),
    answered: impl Fn(&[Snapshot]) -> bool,
) {
    let (held, release) = model.next_look.hold();
    held.recv_timeout(PATIENCE)
        .expect("a look held on the lane of reads");
    change();
    told.until("the change answered", answered);
    drop(release);
}

// Its sheet closes, or its sign-in ends, as it answers.
fn closed_or_over(told: &[Snapshot]) -> bool {
    told.last()
        .is_some_and(|last| last.sheet.is_none() && last.signing_in.is_none())
}

#[test]
fn a_rename_is_read_after_it_whatever_the_poll_finds_meanwhile() {
    let world = World::new("rename-looked-at");
    world.enrolled("work", "here", 10.0);
    world.index_written_earlier(60);
    let told = Arc::new(Told::default());
    let renaming = model(&world.core(), &told);
    renaming.send(Intent::Start);
    told.until("the account read", read_after(&["work"]));
    renaming.send(Intent::PresentSheet {
        sheet: Sheet::Rename {
            provider: "claude".into(),
            label: "work".into(),
        },
    });
    told.until("the sheet up", |told| {
        told.last().is_some_and(|last| last.sheet.is_some())
    });
    let rename = || {
        renaming.send(Intent::Rename {
            provider: "claude".into(),
            label: "work".into(),
            to: "office".into(),
        });
    };
    with_a_look_held(&renaming, &told, rename, closed_or_over);
    told.until("the read after the rename", read_after(&["office"]));
    renaming.shutdown();
}

#[test]
fn adding_the_login_in_use_is_read_after_it_whatever_the_poll_finds_meanwhile() {
    let world = World::new("name-looked-at");
    // Nothing enrolled, so there is no account index until the enrolment writes one.
    world.signed_in("here", 10.0);
    let told = Arc::new(Told::default());
    let adding = model(&world.core(), &told);
    adding.send(Intent::Start);
    told.until("the login read", |told| {
        told.last()
            .is_some_and(|last| read_in(last) && !last.sections.is_empty())
    });
    let enrol = || {
        adding.send(Intent::Enrol {
            provider: "claude".into(),
        });
    };
    with_a_look_held(&adding, &told, enrol, naming("example"));
    told.until("the read after adding it", read_after(&["example"]));
    adding.shutdown();
}

#[test]
fn a_sign_in_is_read_after_it_enrols_whatever_the_poll_finds_meanwhile() {
    let mut world = World::new("sign-in-looked-at");
    world.enrolled("work", "here", 10.0);
    let _claude = world.claude_stand_in();
    world.index_written_earlier(60);
    let told = Arc::new(Told::default());
    let model = model(&world.core(), &told);
    model.send(Intent::Start);
    told.until("the account read", read_after(&["work"]));
    model.send(sign_in());
    told.until("a code asked for", signing(|s| s.wants_code));
    world.signed_in_privately("away", "access-away", 20.0);
    let paste_it = || model.send(paste("the-code#the-state"));
    with_a_look_held(&model, &told, paste_it, closed_or_over);
    told.until(
        "the read after the sign-in",
        read_after(&["work", "example"]),
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
            glance: None,
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
    assert_eq!(
        notice.title,
        words::limit_notification(OS, "5-hour", None, true)
    );
    assert_eq!(
        notice.body,
        format!(
            "{} {}",
            words::has_used_share("work", "100%"),
            words::other_used("spare", Some(90))
        )
    );
    assert_eq!(
        notice.button.map(|button| button.clicked).as_deref(),
        Some("switch/claude/spare")
    );
    assert_eq!(notice.subtitle.as_deref(), Some("Claude Code"));
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
/// switch is said in a notification that offers Switch Back, the activity log records it as
/// automatic and made by the app, and the account switched to stays in use.
#[test]
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
    assert_eq!(notice.title, words::switched_to(None, "spare"));
    assert_eq!(
        notice.body,
        words::used_then(
            "work",
            "97% of its 5-hour limit",
            &words::follows(&crate::Adoption::PollingWithin(33))
        )
    );
    assert_eq!(
        notice.button.map(|button| button.clicked),
        Some(r#"switch-back/["claude/work","claude/spare"]"#.into())
    );
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
        told.last().is_some_and(read_in)
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

/// The machine over the real core, as the settings and the window's other panes ask for it:
/// daily renewal turned on and off through the core's pretend scheduler, which writes its job
/// into the scratch home and asks no service manager; a renewal and the read after it;
/// doctor's checks; every change Pitboard made, the schedule's among them; and the
/// `pitboard` a terminal would run, looked for in the scratch home alone.
#[test]
fn the_machine_is_kept_over_the_real_core() {
    use super::Pane;
    use crate::{FoundCommandLine, Schedule};
    use pitboard_core::host::Os;
    let mut world = World::new("machine");
    world.in_the_default_home();
    world.enrolled("work", "here", 42.0);
    world.app_with_a_command_line();
    let core = world.core();
    let told = Arc::new(Told::default());
    let model = model(&core, &told);
    model.send(Intent::Start);
    told.until("the account read", shows("work"));

    model.send(Intent::ReadSchedule);
    model.send(Intent::SetSchedule { on: true });
    // Held back while it changes, and on once the scheduler is read back with the job.
    let on = told.last_where("daily renewal on", |last| {
        last.machine.schedule.on && last.machine.schedule.enabled
    });
    assert_eq!(on.machine.schedule.note, None, "this copy can schedule it");
    let Some(Schedule::Installed { path: job, .. }) = core.schedule() else {
        panic!("the job installed");
    };
    let kept = match OS {
        Os::MacOs | Os::Linux => std::path::PathBuf::from(&job),
        // The task as the pretend Task Scheduler keeps it.
        Os::Windows => world
            .dir("Tasks")
            .join("Pitboard")
            .join(job.strip_prefix(r"\Pitboard\").expect("Pitboard's task")),
    };
    assert!(
        kept.starts_with(world.dir("")),
        "written in the scratch home: {job}"
    );
    assert!(kept.is_file());

    model.send(Intent::PaneShown {
        pane: Pane::Activity,
    });
    let logged = told.last_where("the log", |last| !last.machine.activity.lines.is_empty());
    let lines: Vec<(String, &str, String, String)> = logged
        .machine
        .activity
        .lines
        .iter()
        .map(|line| {
            (
                line.change.clone(),
                line.account.as_str(),
                line.result.clone(),
                line.asked_by.clone(),
            )
        })
        .collect();
    let said = |verb, account, outcome, caller| {
        (
            words::change_verb(verb),
            account,
            words::change_outcome(outcome),
            words::change_caller(caller),
        )
    };
    assert_eq!(
        lines.first(),
        Some(&said("schedule", "install", "ok", "app")),
        "{lines:?}"
    );
    assert!(
        lines.contains(&said("enroll", "work", "ok", "cli")),
        "{lines:?}"
    );

    model.send(Intent::PaneShown { pane: Pane::Checks });
    let checked = told.last_where("the checks", |last| {
        !last.machine.checks.lines().is_empty() && !last.machine.checks.checking
    });
    assert!(checked.machine.checks.summary.is_some());
    assert!(checked.machine.checks.checked.is_some());

    model.send(Intent::SetSchedule { on: false });
    told.last_where("daily renewal off", |last| {
        !last.machine.schedule.on && last.machine.schedule.enabled
    });
    assert_eq!(core.schedule(), Some(Schedule::Absent));
    assert!(!kept.exists(), "taken away");

    model.send(Intent::LookForCommandLine);
    let found = told.last_where("the command line looked for", |last| {
        last.machine.command_line.found.is_some()
    });
    let command_line = &found.machine.command_line;
    assert_eq!(command_line.found, Some(FoundCommandLine::Nowhere));
    assert_eq!(
        command_line.in_terminal.as_deref(),
        Some(words::NOT_INSTALLED)
    );
    assert!(command_line.offers_link);
    model.shutdown();
}

/// An account 0.8.0 enrolled keeps its window and the sign-in in it: the window's store is
/// derived from the account's id, which an account enrolled then keeps.
#[test]
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
    let stores: Vec<String> = crate::account_windows::window_accounts(&accounts)
        .into_iter()
        .map(|window| window.store)
        .collect();
    assert_eq!(
        stores,
        [crate::account_windows::stores::derive(
            pitboard_sites::CLAUDE.store_name,
            "here"
        )]
    );
}
/// The account windows' records, kept by the lanes in a directory of the app's own, here the
/// scratch home's: what the app's earlier store held is taken once, in its own upper case, and
/// written private to its owner; a store no enrolled account derives is asked of the app and
/// taken off the record once deleted; and an account forgotten in a terminal has its window
/// closed and its store asked for, the record keeping it until the app has deleted it.
#[test]
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
        crate::account_windows::stores::derive(pitboard_sites::CLAUDE.store_name, &enrolled.id)
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
                            key,
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
    told.last_where("the store nobody has asked for", |last| {
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
    let shown = told.last_where("both windows open", |last| {
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
    told.last_where("the download under way", |last| {
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
    let forgotten = told.last_where("the forgotten account's window closing", |last| {
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
fn the_windows_records_are_not_written_where_pitboard_may_change_nothing() {
    use super::{EarlierWindowRecords, WindowsLaunch, WindowsPlace};
    use std::collections::HashMap;

    let world = World::new("windows-elevated");
    world.enrolled("work", "here", 10.0);
    let work = crate::account_windows::stores::derive(
        pitboard_sites::CLAUDE.store_name,
        &world.elsewhere().account("work").expect("enrolled").id,
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
                            key,
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
    told.last_where("the store nobody has asked for", |last| {
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
