//! The window made in the test's own process over a fixture's model, as the app makes it: the
//! model's snapshots taken on the main loop into the store the window follows, and what the
//! window sends handed to the model. What it sends and opens, and whether it quit, is kept for
//! the test, and opening reaches no browser. Start at sign-in is a fixture's stand-in, and the
//! app's own folders are the test's. For a test of what a view shows and sends, which drives
//! the widgets as GTK's own tests do, by their signals.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::glib;
use pitboard_linux::data_folder::DataFolder;
use pitboard_linux::listener;
use pitboard_linux::local_time::DesktopClock;
use pitboard_linux::platform::autostart::StartAtSignIn;
use pitboard_linux::platform::stand_in;
use pitboard_linux::shown::Shown;
use pitboard_linux::window::{Host, Window};
use pitboard_model::{
    AccountItem, Front, Intent, LocalTime, Notice, Notifier, PitboardModel, PlatformError, Snapshot,
};

/// How long anything a view test waits for may take.
pub(crate) const PATIENCE: Duration = Duration::from_secs(30);

struct Unposted;

impl Notifier for Unposted {
    fn post(&self, _: Notice) -> Result<(), PlatformError> {
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Asked {
    pub(crate) sent: RefCell<Vec<Intent>>,
    pub(crate) opened: RefCell<Vec<String>>,
    pub(crate) quit: Cell<bool>,
}

struct Hands {
    model: OnceCell<Arc<PitboardModel>>,
    asked: Rc<Asked>,
    data: DataFolder,
    start_at_sign_in: stand_in::Autostart,
    own_folders: Vec<PathBuf>,
    local_time: Arc<dyn LocalTime>,
}

impl Host for Hands {
    fn send(&self, intent: Intent) {
        self.asked.sent.borrow_mut().push(intent.clone());
        if let Some(model) = self.model.get() {
            model.send(intent);
        }
    }
    fn open(&self, link: &str, _: &gtk::Window) {
        self.asked.opened.borrow_mut().push(link.to_owned());
    }
    fn keeps_running(&self) -> bool {
        true
    }
    fn quit(&self) {
        self.asked.quit.set(true);
    }
    fn data(&self) -> &DataFolder {
        &self.data
    }
    fn token(&self) -> Option<String> {
        None
    }
    fn local_time(&self) -> Arc<dyn LocalTime> {
        Arc::clone(&self.local_time)
    }
    fn start_at_sign_in(&self) -> Option<&dyn StartAtSignIn> {
        Some(&self.start_at_sign_in)
    }
    fn own_folders(&self) -> Vec<PathBuf> {
        self.own_folders.clone()
    }
}

/// The window over the fixture `world`, started, once its first read is shown.
pub(crate) struct Viewed {
    pub(crate) window: Window,
    pub(crate) shown: Shown,
    pub(crate) asked: Rc<Asked>,
    /// The folder the fixture's world is made in, whose `linux` folder holds what the stand-in
    /// for start at sign-in wrote, and whose `data` and `cache` are the app's own folders.
    pub(crate) folder: PathBuf,
    model: Arc<PitboardModel>,
    _listening: glib::JoinHandle<()>,
    _scratch: Option<tempfile::TempDir>,
}

impl Viewed {
    /// In a folder of the test's own, telling the time by the desktop's clock, once it lists
    /// the accounts it read.
    pub(crate) fn of(world: &str) -> Viewed {
        Viewed::once(world, |shown| !shown.sections.is_empty())
    }

    /// As `of`, once a read shows what `read` holds of.
    pub(crate) fn once(world: &str, read: impl Fn(&Snapshot) -> bool) -> Viewed {
        let scratch = tempfile::tempdir().unwrap();
        let clock = DesktopClock::following(None, None);
        let mut viewed = Viewed::reading(world, scratch.path(), clock.local_time(), read);
        viewed._scratch = Some(scratch);
        viewed
    }

    // The fixture made in `folder`, telling the time by `local_time`.
    fn reading(
        world: &str,
        folder: &Path,
        local_time: Arc<dyn LocalTime>,
        read: impl Fn(&Snapshot) -> bool,
    ) -> Viewed {
        adw::init().unwrap();
        pitboard_linux::resources::register();
        gtk::Settings::default()
            .unwrap()
            .set_gtk_enable_animations(false);
        let store: Rc<OnceCell<Shown>> = Rc::default();
        let taking = store.clone();
        let (told, listening) = listener::listening(move |snapshot| {
            if let Some(shown) = taking.get() {
                shown.take(snapshot);
            }
        });
        let model = PitboardModel::fixture_in(
            world.into(),
            Front::Gnome,
            folder.to_string_lossy().into_owned(),
            told,
            None,
            Arc::new(Unposted),
            Arc::clone(&local_time),
        )
        .unwrap();
        let shown = Shown::new(model.snapshot());
        let _ = store.set(shown.clone());
        let asked = Rc::new(Asked::default());
        let hands = Rc::new(Hands {
            model: OnceCell::from(Arc::clone(&model)),
            asked: asked.clone(),
            data: DataFolder::at(&folder.join("data")),
            start_at_sign_in: stand_in::Autostart::new(folder),
            own_folders: vec![folder.join("data"), folder.join("cache")],
            local_time,
        });
        let window = Window::new(None, &shown, hands);
        window.present();
        let viewed = Viewed {
            window,
            shown,
            asked,
            folder: folder.to_path_buf(),
            model,
            _listening: listening,
            _scratch: None,
        };
        viewed.until("the window shown", |_| viewed.window.is_mapped());
        viewed.model.send(Intent::Start);
        viewed.until("the accounts read", |shown| !shown.reading && read(shown));
        viewed.asked.sent.borrow_mut().clear();
        viewed
    }

    /// Runs the main loop until `done` holds of what is shown, within `PATIENCE`.
    pub(crate) fn until(&self, what: &str, done: impl Fn(&Snapshot) -> bool) {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + PATIENCE;
        while !done(&self.shown.snapshot()) {
            assert!(
                Instant::now() < deadline,
                "{what}: not within {PATIENCE:?}: {:#?}",
                self.shown.snapshot().sections
            );
            if !context.iteration(false) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        while context.iteration(false) {}
    }

    /// Every account shown, in its order.
    pub(crate) fn items(&self) -> Vec<AccountItem> {
        (self.shown.snapshot().sections.iter())
            .flat_map(|section| section.accounts.clone())
            .collect()
    }

    pub(crate) fn sent(&self) -> Vec<Intent> {
        self.asked.sent.borrow().clone()
    }

    /// What it sent since the last time this was asked.
    pub(crate) fn sent_since(&self) -> Vec<Intent> {
        self.asked.sent.take()
    }

    /// Sends `intent` to the model as something other than the window would, which the window
    /// is not said to have sent.
    pub(crate) fn tell(&self, intent: Intent) {
        self.model.send(intent);
    }

    /// What the stand-in for start at sign-in wrote, a JSON value a line.
    pub(crate) fn started_at_sign_in(&self) -> Vec<serde_json::Value> {
        let written = self.folder.join("linux/autostart.jsonl");
        (std::fs::read_to_string(written).unwrap_or_default().lines())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// The dialog the window shows on top, once it and its default button are on the screen:
    /// GTK activates a button, as Return does the default, only once it is realized, and a
    /// dialog's buttons come a frame after the dialog.
    pub(crate) fn dialog(&self) -> adw::Dialog {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + PATIENCE;
        loop {
            while context.iteration(false) {}
            if let Some(dialog) = self.window.visible_dialog()
                && dialog.is_mapped()
                && dialog
                    .default_widget()
                    .is_none_or(|button| button.is_mapped())
            {
                return dialog;
            }
            assert!(Instant::now() < deadline, "no dialog within {PATIENCE:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Viewed {
    fn drop(&mut self) {
        self.window.destroy();
        self.model.shutdown();
    }
}

struct Told(std::sync::Mutex<std::sync::mpsc::Sender<Snapshot>>);

impl pitboard_model::Listener for Told {
    fn changed(&self, snapshot: Snapshot) -> Result<(), PlatformError> {
        let _ = self.0.lock().unwrap().send(snapshot);
        Ok(())
    }
}

/// What the fixture `world`'s model shows once it has read its accounts and been sent
/// `intents`, once `done` holds, as the program launched into it shows it: for a test over
/// AT-SPI to know the model's words.
pub(crate) fn shown_in(
    world: &str,
    intents: &[Intent],
    done: impl Fn(&Snapshot) -> bool,
) -> Snapshot {
    let scratch = tempfile::tempdir().unwrap();
    let (told, heard) = std::sync::mpsc::channel();
    let model = PitboardModel::fixture_in(
        world.into(),
        Front::Gnome,
        scratch.path().to_string_lossy().into_owned(),
        Arc::new(Told(std::sync::Mutex::new(told))),
        None,
        Arc::new(Unposted),
        DesktopClock::following(None, None).local_time(),
    )
    .unwrap();
    model.send(Intent::Start);
    let deadline = Instant::now() + PATIENCE;
    let mut sent = false;
    let shown = loop {
        let snapshot = heard
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|_| panic!("{world}: not shown within {PATIENCE:?}"));
        let read = !snapshot.reading && !snapshot.sections.is_empty();
        if read && !sent {
            sent = true;
            intents.iter().for_each(|intent| model.send(intent.clone()));
        }
        if read && sent && done(&snapshot) {
            break snapshot;
        }
    };
    model.shutdown();
    shown
}

/// The accounts the fixture `world`'s model shows once it has read them.
pub(crate) fn items_of(world: &str) -> Vec<AccountItem> {
    (shown_in(world, &[], |_| true).sections.into_iter())
        .flat_map(|section| section.accounts)
        .collect()
}

/// Every widget of type `T` under `root`, depth first, in the order drawn.
pub(crate) fn every<T: IsA<gtk::Widget>>(root: &impl IsA<gtk::Widget>) -> Vec<T> {
    let mut found = Vec::new();
    let mut next = vec![root.as_ref().clone()];
    while let Some(widget) = next.pop() {
        if let Ok(widget) = widget.clone().downcast::<T>() {
            found.push(widget);
        }
        let mut child = widget.last_child();
        while let Some(widget) = child {
            child = widget.prev_sibling();
            next.push(widget);
        }
    }
    found
}

/// The button under `root` labelled `label`, which a click on presses.
pub(crate) fn button(root: &impl IsA<gtk::Widget>, label: &str) -> gtk::Button {
    (every::<gtk::Button>(root).into_iter())
        .find(|button| button.label().as_deref() == Some(label))
        .unwrap_or_else(|| {
            let labels: Vec<_> = (every::<gtk::Button>(root).iter())
                .map(|button| button.label())
                .collect();
            panic!("no button {label:?}: {labels:?}")
        })
}

/// The row under `root` of type `T` whose title is `title`.
pub(crate) fn row<T: IsA<adw::PreferencesRow> + IsA<gtk::Widget>>(
    root: &impl IsA<gtk::Widget>,
    title: &str,
) -> T {
    (every::<T>(root).into_iter())
        .find(|row| row.title() == title)
        .unwrap_or_else(|| {
            let titles: Vec<_> = (every::<T>(root).iter()).map(|row| row.title()).collect();
            panic!("no row {title:?}: {titles:?}")
        })
}

/// What Escape does where `from` has the focus: the first shortcut for it from `from` up to its
/// window, activated as GTK activates one for the key, after the compositor's answers to what the
/// window asked, as a key comes. GTK keeps a freed entry's input method context when the entry
/// goes before them (GNOME/gtk#8458).
pub(crate) fn escape(from: &impl IsA<gtk::Widget>) {
    from.as_ref().display().sync();
    while glib::MainContext::default().iteration(false) {}
    let mut at = Some(from.as_ref().clone());
    while let Some(widget) = at {
        let controllers = widget.observe_controllers();
        for controller in (0..controllers.n_items()).filter_map(|i| controllers.item(i)) {
            let Ok(shortcuts) = controller.downcast::<gtk::ShortcutController>() else {
                continue;
            };
            for shortcut in (0..shortcuts.n_items()).filter_map(|i| shortcuts.item(i)) {
                let shortcut = shortcut
                    .downcast::<gtk::Shortcut>()
                    .expect("shortcuts alone");
                let escape = (shortcut.trigger())
                    .and_downcast::<gtk::KeyvalTrigger>()
                    .is_some_and(|trigger| {
                        trigger.keyval() == gtk::gdk::Key::Escape && trigger.modifiers().is_empty()
                    });
                if escape && let Some(action) = shortcut.action() {
                    action.activate(gtk::ShortcutActionFlags::EXCLUSIVE, &widget, None);
                    return;
                }
            }
        }
        at = widget.parent();
    }
    panic!("nothing takes Escape");
}
