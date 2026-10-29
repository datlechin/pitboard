//! The window made in the test's own process over a fixture's model, as the app makes it: the
//! model's snapshots taken on the main loop into the store the window follows, and what the
//! window sends handed to the model. What it sends and opens is kept for the test, and opening
//! reaches no browser. For a test of what a view shows and sends, which drives the widgets as
//! GTK's own tests do, by their signals.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::glib;
use pitboard_linux::data_folder::DataFolder;
use pitboard_linux::listener;
use pitboard_linux::local_time::DesktopClock;
use pitboard_linux::shown::Shown;
use pitboard_linux::window::{Host, Window};
use pitboard_model::{
    AccountItem, Front, Intent, Notice, Notifier, PitboardModel, PlatformError, Snapshot,
};

use super::harness::PATIENCE;

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
}

struct Hands {
    model: OnceCell<Arc<PitboardModel>>,
    asked: Rc<Asked>,
    data: DataFolder,
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
    fn quit(&self) {}
    fn data(&self) -> &DataFolder {
        &self.data
    }
}

/// The window over the fixture `world`, started, once its first read is shown.
pub(crate) struct Viewed {
    pub(crate) window: Window,
    pub(crate) shown: Shown,
    pub(crate) asked: Rc<Asked>,
    model: Arc<PitboardModel>,
    _listening: glib::JoinHandle<()>,
    _scratch: tempfile::TempDir,
}

impl Viewed {
    pub(crate) fn of(world: &str) -> Viewed {
        adw::init().unwrap();
        pitboard_linux::resources::register();
        gtk::Settings::default()
            .unwrap()
            .set_gtk_enable_animations(false);
        let scratch = tempfile::tempdir().unwrap();
        let store: Rc<OnceCell<Shown>> = Rc::default();
        let taking = store.clone();
        let (told, listening) = listener::listening(move |snapshot| {
            if let Some(shown) = taking.get() {
                shown.take(snapshot);
            }
        });
        let clock = DesktopClock::following(None);
        let model = PitboardModel::fixture_in(
            world.into(),
            Front::Gnome,
            scratch.path().to_string_lossy().into_owned(),
            told,
            None,
            Arc::new(Unposted),
            clock.local_time(),
        )
        .unwrap();
        let shown = Shown::new(model.snapshot());
        let _ = store.set(shown.clone());
        let asked = Rc::new(Asked::default());
        let hands = Rc::new(Hands {
            model: OnceCell::from(Arc::clone(&model)),
            asked: asked.clone(),
            data: DataFolder::at(&scratch.path().join("data")),
        });
        let window = Window::new(None, &shown, hands);
        window.present();
        let viewed = Viewed {
            window,
            shown,
            asked,
            model,
            _listening: listening,
            _scratch: scratch,
        };
        viewed.until("the window shown", |_| viewed.window.is_mapped());
        viewed.model.send(Intent::Start);
        viewed.until("the accounts read", |shown| {
            !shown.reading && !shown.sections.is_empty()
        });
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

/// The accounts the fixture `world`'s model shows once it has read them, as the program
/// launched into it shows them: for a test over AT-SPI to know the model's words.
pub(crate) fn items_of(world: &str) -> Vec<AccountItem> {
    let scratch = tempfile::tempdir().unwrap();
    let (told, heard) = std::sync::mpsc::channel();
    let model = PitboardModel::fixture_in(
        world.into(),
        Front::Gnome,
        scratch.path().to_string_lossy().into_owned(),
        Arc::new(Told(std::sync::Mutex::new(told))),
        None,
        Arc::new(Unposted),
        DesktopClock::following(None).local_time(),
    )
    .unwrap();
    model.send(Intent::Start);
    let deadline = Instant::now() + PATIENCE;
    let read = loop {
        let snapshot = heard
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|_| panic!("{world}: not read within {PATIENCE:?}"));
        if !snapshot.reading && !snapshot.sections.is_empty() {
            break snapshot;
        }
    };
    model.shutdown();
    (read.sections.into_iter())
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
