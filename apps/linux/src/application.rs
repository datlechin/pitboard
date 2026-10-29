//! The app: one GApplication, which only the primary instance runs. A second launch, a link, a
//! notification's click and the launcher's actions reach the primary over the bus, and the
//! primary alone makes the model, sends it `Intent::Start`, and stops it as the app shuts down.
//!
//! It runs while something shows it or waits on it, and ends by GApplication's own use count
//! once nothing does: it is held while a tray shows its status item, GtkApplication holds it for
//! its window, and it holds itself while a switch asked from outside is answered and, from its
//! start, until it has heard whether a tray is there. D-Bus activation has GLib's own wait for
//! the call that started it, and a launch at sign-in waits a while for a tray to appear. So
//! nothing of it runs unseen where no tray shows it: a window it had hidden for a tray closes as
//! the tray goes, and a launch at sign-in with no tray ends.
//!
//! What it asks of the desktop is the desktop's own services in a live launch, and a fixture's
//! stand-ins in a fixture, which write in the fixture's folder alone. The model's snapshots go
//! to the store the window, the presence and the widget feed follow; the window is made as the
//! model first asks for it, and answers each request itself. The model is told at the start
//! whether the app starts at sign-in, as the desktop has it.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use pitboard_model::{
    AppLaunch, Front, Intent, LocalTime, Part, PitboardModel, Snapshot, Surfaces,
};

use crate::config;
use crate::data_folder::DataFolder;
use crate::listener;
use crate::local_time::DesktopClock;
use crate::notices::Notices;
use crate::outside::Outside;
use crate::platform::autostart::{Autostart, StartAtSignIn};
use crate::platform::leftovers;
use crate::platform::links::{self, Opener};
use crate::platform::notifications::{self, Notifier};
use crate::platform::search::Asked;
use crate::platform::tray::{self, Heard, StatusItem};
use crate::platform::{krunner, logind, search};
use crate::presence::{self, Picked, Presence};
use crate::resources;
use crate::shown::Shown;
use crate::widget_feed::{self, WidgetFeed};
use crate::window::{Host, Window};

/// Starts the app with no window, as start at sign-in does: held where a tray shows it.
pub const SIGN_IN: &str = "sign-in";

// How long a launch at sign-in runs with nothing holding it, for a tray that starts after it in
// the session (docs/facts/not-measured.md): GLib's own wait for a D-Bus activation's call
// (`gio/gapplication.c` at 2.84.0, `g_application_run`).
const SIGN_IN_WAITS_MS: u32 = 10_000;

/// Asks the running app for what the launcher's action of the same name asks, starting it where
/// none runs: for a launcher that starts the program rather than asking over the bus.
pub const ADD_ACCOUNT: &str = crate::outside::ADD_ACCOUNT;
pub const REFRESH: &str = crate::outside::REFRESH;

/// How this process was launched, which `launch` decided before the app was made.
pub enum Launched {
    /// Over the homes `environment`, every variable the process was started with, names.
    Live {
        environment: HashMap<String, String>,
    },
    /// In the fixture `PITBOARD_FIXTURE` names, which the model makes once this is the primary.
    #[cfg(feature = "fixture")]
    Fixture { name: String },
}

/// What a live launch hands the model: the installed program, where Meson installed it, which
/// the core finds the command line beside, and none for a build Cargo made alone.
pub fn app_launch(environment: HashMap<String, String>) -> AppLaunch {
    AppLaunch {
        environment,
        app_location: config::installed_program(),
        front: Front::Gnome,
        // Until WebKitGTK 2.56 is on the floor distributions (brief, webwindows).
        account_windows: None,
        handover: None,
        widget_feed: None,
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Application {
        pub(super) launched: RefCell<Option<Launched>>,
        // a launch with `--sign-in`, whose own activation opens no window
        pub(super) held_at_sign_in: Cell<bool>,
        pub(super) running: OnceCell<Rc<Running>>,
        pub(super) window: RefCell<Option<Window>>,
        pub(super) served: RefCell<Option<Served>>,
        // the fixture's world could not be made, so the launch failed
        pub(super) failed: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "PitboardApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {
        fn constructed(&self) {
            self.parent_constructed();
            let app = self.obj();
            let words = pitboard_model::app_words(Front::Gnome).window;
            let options = [
                (SIGN_IN, "Start with no window, as start at sign-in does"),
                (ADD_ACCOUNT, words.add_account_help.as_str()),
                (REFRESH, words.refresh_help.as_str()),
            ];
            for (name, description) in options {
                app.add_main_option(
                    name,
                    glib::Char::from(0),
                    glib::OptionFlags::NONE,
                    glib::OptionArg::None,
                    description,
                    None,
                );
            }
        }
    }

    impl ApplicationImpl for Application {
        // Registered here, so a launch that asks for an action asks the primary for it: the
        // primary where none was, which goes on to run.
        fn handle_local_options(&self, options: &glib::VariantDict) -> ControlFlow<glib::ExitCode> {
            let asked: Vec<&str> = [ADD_ACCOUNT, REFRESH]
                .into_iter()
                .filter(|name| options.contains(name))
                .collect();
            let sign_in = options.contains(SIGN_IN);
            if asked.is_empty() && !sign_in {
                return ControlFlow::Continue(());
            }
            let app = self.obj();
            if let Err(e) = app.register(gio::Cancellable::NONE) {
                eprintln!("pitboard-app: {e}");
                return ControlFlow::Break(glib::ExitCode::FAILURE);
            }
            for name in asked {
                app.activate_action(name, None);
            }
            if app.is_remote() {
                return ControlFlow::Break(glib::ExitCode::SUCCESS);
            }
            if sign_in {
                app.set_inactivity_timeout(SIGN_IN_WAITS_MS);
            }
            self.held_at_sign_in.set(sign_in);
            ControlFlow::Continue(())
        }

        fn startup(&self) {
            self.parent_startup();
            let app = self.obj();
            resources::register();
            let launched = self.launched.take().expect("made with how it was launched");
            match Running::start(&app, launched) {
                Ok(running) => {
                    let _ = self.running.set(running);
                }
                Err(reason) => {
                    eprintln!("pitboard-app: {reason}");
                    self.failed.set(true);
                    app.quit();
                }
            }
        }

        // The window is made at once, which holds the app until the model asks for it.
        fn activate(&self) {
            self.parent_activate();
            if self.held_at_sign_in.take() {
                return;
            }
            if let Some(running) = self.running.get() {
                self.obj().window();
                running.send(Intent::Show { place: None });
            }
        }

        fn open(&self, files: &[gio::File], _hint: &str) {
            let Some(running) = self.running.get() else {
                return;
            };
            self.obj().window();
            for text in links::opened(files) {
                running.send(Intent::LinkArrived { text });
            }
            // Over the window, where the model asks which account opens it.
            running.send(Intent::Show { place: None });
        }

        fn dbus_register(
            &self,
            connection: &gio::DBusConnection,
            object_path: &str,
        ) -> Result<(), glib::Error> {
            self.parent_dbus_register(connection, object_path)?;
            let app = self.obj();
            let (found, asked) = (app.downgrade(), app.downgrade());
            let provider = search::Provider::serve(
                connection,
                object_path,
                move |terms| found_by(&found, terms),
                move |asked_for| {
                    if let Some(app) = asked.upgrade() {
                        app.searched(asked_for, None);
                    }
                },
            )?;
            let (found, asked) = (app.downgrade(), app.downgrade());
            let runner = krunner::Runner::serve(
                connection,
                object_path,
                move |terms| found_by(&found, terms),
                move |run: krunner::Run| {
                    if let Some(app) = asked.upgrade() {
                        app.searched(Asked::Activate(run.id), run.token);
                    }
                },
            )?;
            let feed = Rc::new(WidgetFeed::serve(connection, object_path)?);
            self.served.replace(Some(Served {
                _provider: provider,
                _runner: runner,
                feed,
            }));
            Ok(())
        }

        fn dbus_unregister(&self, connection: &gio::DBusConnection, object_path: &str) {
            self.served.take();
            self.parent_dbus_unregister(connection, object_path);
        }

        fn shutdown(&self) {
            if let Some(window) = self.window.take() {
                window.keep_size();
                window.destroy();
            }
            if let Some(running) = self.running.get() {
                running.stop();
            }
            self.parent_shutdown();
        }
    }

    impl GtkApplicationImpl for Application {
        // Closed, the window is made again the next time it is shown.
        fn window_removed(&self, window: &gtk::Window) {
            let ours = (self.window.borrow().as_ref())
                .is_some_and(|made| made.upcast_ref::<gtk::Window>() == window);
            if ours {
                self.window.take();
            }
            self.parent_window_removed(window);
        }
    }
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Application {
    pub fn new(launched: Launched) -> Application {
        glib::set_application_name(&pitboard_model::app_words(Front::Gnome).app_name);
        let app: Application = glib::Object::builder()
            .property("application-id", config::APP_ID)
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            // GApplication's own is the id's path, and the debug build's id has a part more than
            // the resource's.
            .property("resource-base-path", resources::BASE)
            .build();
        app.imp().launched.replace(Some(launched));
        app
    }

    /// Whether the launch failed: the fixture's world could not be made.
    pub fn failed(&self) -> bool {
        self.imp().failed.get()
    }

    fn running(&self) -> Option<Rc<Running>> {
        self.imp().running.get().cloned()
    }

    // As the tray goes, a window it kept hidden closes, as it would have closed with none.
    fn close_hidden_window(&self) {
        let hidden = (self.imp().window.borrow().clone()).filter(|window| !window.is_visible());
        if let Some(window) = hidden {
            window.keep_size();
            window.destroy();
        }
    }

    // The main window, made the first time it is asked for.
    fn window(&self) -> Option<Window> {
        let running = self.running()?;
        let made = self.imp().window.borrow().clone();
        Some(made.unwrap_or_else(|| {
            let window = Window::new(
                Some(self.upcast_ref()),
                &running.shown,
                running.clone() as Rc<dyn Host>,
            );
            self.imp().window.replace(Some(window.clone()));
            window
        }))
    }

    fn searched(&self, asked: Asked, token: Option<String>) {
        let Some(running) = self.running() else {
            return;
        };
        running.token.replace(token);
        match asked {
            Asked::Activate(id) => {
                if let Some(picked) = presence::result_picked(&id) {
                    running.pick(picked);
                }
            }
            Asked::Open => running.send(Intent::Show { place: None }),
        }
    }
}

// What the app serves on its connection beside GApplication's own, from its registration on.
struct Served {
    _provider: search::Provider,
    _runner: krunner::Runner,
    feed: Rc<WidgetFeed>,
}

fn found_by(app: &glib::WeakRef<Application>, terms: &[String]) -> Vec<search::Found> {
    (app.upgrade().and_then(|app| app.running()))
        .map(|running| running.presence.found(terms))
        .unwrap_or_default()
}

/// The primary instance's parts, from its start to its shutdown.
pub struct Running {
    app: glib::WeakRef<Application>,
    model: Arc<PitboardModel>,
    shown: Shown,
    outside: Rc<Outside>,
    presence: Presence,
    opener: Box<dyn Opener>,
    data: DataFolder,
    start_at_sign_in: Option<Box<dyn StartAtSignIn>>,
    own_folders: Vec<PathBuf>,
    // from the start until the tray's first word, but where GLib's service mode waits itself
    launching: RefCell<Option<gio::ApplicationHoldGuard>>,
    // while a tray shows the status item
    tray: RefCell<Option<gio::ApplicationHoldGuard>>,
    // the activation token what asked for the window came with, for the next it shows
    token: RefCell<Option<String>>,
    // the last window request, the first of which makes the window
    asked: Cell<u64>,
    local_time: Arc<dyn LocalTime>,
    _clock: DesktopClock,
    _listening: glib::JoinHandle<()>,
    _hearing: Hearing,
}

// What a launch asks the desktop through, and what it hears from it: its own services, or a
// fixture's stand-ins.
struct Services {
    notifier: Arc<dyn Notifier>,
    opener: Box<dyn Opener>,
    item: Box<dyn StatusItem>,
    data: DataFolder,
    // none where the build is installed nowhere an autostart entry could start it
    start_at_sign_in: Option<Box<dyn StartAtSignIn>>,
    own_folders: Vec<PathBuf>,
    // what it hears through, for as long as the app runs
    hearing: Hearing,
}

type Hearing = Rc<RefCell<Vec<Box<dyn std::any::Any>>>>;

impl Services {
    fn of_the_desktop(app: &Application, connection: &gio::DBusConnection) -> Services {
        let item = tray::Desktop::new(
            connection,
            config::APP_ID,
            &pitboard_model::app_words(Front::Gnome).app_name,
            told(app),
        );
        let hearing = Hearing::default();
        let (kept, woke) = (Rc::downgrade(&hearing), app.downgrade());
        gio::bus_get(gio::BusType::System, gio::Cancellable::NONE, move |bus| {
            // No system bus is said as no logind is: the model's own timers still run.
            let (Ok(bus), Some(kept)) = (bus, kept.upgrade()) else {
                return;
            };
            let waking = logind::hear(&bus, move |heard| {
                let running = woke.upgrade().and_then(|app| app.running());
                if let (logind::Heard::Woke, Some(running)) = (heard, running) {
                    running.send(Intent::Woke);
                }
            });
            kept.borrow_mut().push(Box::new(waking));
        });
        Services {
            notifier: Arc::new(notifications::Desktop::new(app)),
            opener: Box::new(links::Browser),
            item: Box::new(item),
            data: DataFolder::of_this_user(),
            start_at_sign_in: config::installed_program().map(|program| {
                Box::new(Autostart::of_this_user(&program)) as Box<dyn StartAtSignIn>
            }),
            own_folders: leftovers::folders_of_this_user().to_vec(),
            hearing,
        }
    }

    // In `pitboard-fixture` in this process's temporary directory, where the model makes the
    // fixture's world. Its tray is there while a watcher's name has an owner on the bus.
    #[cfg(feature = "fixture")]
    fn of_the_fixture(app: &Application, connection: &gio::DBusConnection) -> Services {
        use crate::platform::stand_in;
        let fixture = std::env::temp_dir().join("pitboard-fixture");
        let hearing = Hearing::default();
        let watcher = stand_in::Tray::hear(connection, told(app));
        hearing.borrow_mut().push(Box::new(watcher));
        let data = DataFolder::in_fixture(&fixture);
        Services {
            notifier: Arc::new(stand_in::Notifications::new(&fixture)),
            opener: Box::new(stand_in::Browser::new(&fixture)),
            item: Box::new(stand_in::Tray::new(&fixture)),
            start_at_sign_in: Some(Box::new(stand_in::Autostart::new(&fixture))),
            own_folders: vec![data.path().to_path_buf()],
            data,
            hearing,
        }
    }
}

// What the tray tells the app's running parts.
fn told(app: &Application) -> impl Fn(Heard) + 'static {
    let app = app.downgrade();
    move |heard| {
        if let Some(running) = app.upgrade().and_then(|app| app.running()) {
            running.heard(heard);
        }
    }
}

impl Running {
    fn start(app: &Application, launched: Launched) -> Result<Rc<Running>, String> {
        let clock = DesktopClock::following(gio::SettingsSchemaSource::default().as_ref());
        let taking = app.downgrade();
        let (listener, listening) = listener::listening(move |snapshot| {
            if let Some(running) = taking.upgrade().and_then(|app| app.running()) {
                running.take(snapshot);
            }
        });
        let connection = app.dbus_connection().expect("the primary is on the bus");
        let (model, services) = match launched {
            Launched::Live { environment } => {
                let services = Services::of_the_desktop(app, &connection);
                let model = PitboardModel::new(
                    app_launch(environment),
                    listener,
                    // The core has no Linux app id: what holds a login is said in words.
                    None,
                    Arc::new(Notices(Arc::clone(&services.notifier))),
                    clock.local_time(),
                );
                (model, services)
            }
            #[cfg(feature = "fixture")]
            Launched::Fixture { name } => {
                let model =
                    PitboardModel::fixture(name, Front::Gnome, listener, clock.local_time())
                        .map_err(|refused| refused.to_string())?;
                // Made once the model has made the world, which empties its folder first.
                (model, Services::of_the_fixture(app, &connection))
            }
        };
        let shown = Shown::new(model.snapshot());
        let asked = shown.snapshot().window_request.serial;
        let outside = Outside::new(
            Arc::clone(&model),
            Notices(services.notifier),
            app.upcast_ref::<gio::Application>(),
        );
        for action in outside.actions() {
            app.add_action(&action);
        }
        let running = Rc::new(Running {
            app: app.downgrade(),
            asked: Cell::new(asked),
            model,
            shown,
            outside,
            presence: Presence::new(services.item),
            opener: services.opener,
            data: services.data,
            start_at_sign_in: services.start_at_sign_in,
            own_folders: services.own_folders,
            launching: RefCell::new(
                (!app.flags().contains(gio::ApplicationFlags::IS_SERVICE)).then(|| app.hold()),
            ),
            tray: RefCell::default(),
            token: RefCell::default(),
            local_time: clock.local_time(),
            _clock: clock,
            _listening: listening,
            _hearing: services.hearing,
        });
        running.follow();
        running.send(Intent::Start);
        if let Some(start) = &running.start_at_sign_in {
            running.send(Intent::OpensAtLogin { on: start.is_on() });
        }
        Ok(running)
    }

    fn take(&self, snapshot: Snapshot) {
        self.shown.take(snapshot);
    }

    fn send(&self, intent: Intent) {
        self.model.send(intent);
    }

    fn stop(&self) {
        self.launching.take();
        self.tray.take();
        self.model.shutdown();
    }

    fn follow(self: &Rc<Self>) {
        let on = |part: Part, act: fn(&Running, &Shown)| {
            let running = Rc::downgrade(self);
            self.shown.connect_changed(part, move |shown| {
                if let Some(running) = running.upgrade() {
                    act(&running, shown);
                }
            });
        };
        // The window answers each request itself once it is made.
        on(Part::WindowRequest, |running, shown| {
            let serial = shown.snapshot().window_request.serial;
            if serial > running.asked.replace(serial)
                && let Some(app) = running.app.upgrade()
            {
                app.window();
            }
        });
        on(Part::Answers, |running, shown| {
            running.outside.answered(&shown.snapshot().answers);
        });
        for part in [
            Part::StatusItem,
            Part::Sections,
            Part::MenuNotices,
            Part::MenuAccountsNote,
            Part::Reading,
        ] {
            on(part, |running, shown| running.show_item(shown));
        }
        on(Part::QuickActions, |running, shown| {
            running.presence.follow_quick_actions(&shown.snapshot());
        });
        self.show_item(&self.shown);
        let feed = self.app.upgrade().and_then(|app| {
            let served = app.imp().served.borrow();
            served.as_ref().map(|served| served.feed.clone())
        });
        if let Some(feed) = feed {
            widget_feed::follow(&feed, &self.shown, &self.model);
        }
    }

    fn show_item(&self, shown: &Shown) {
        // A tray that cannot show it says so as it is told, which the model hears.
        let _ = self.presence.show(&shown.snapshot());
    }

    fn heard(&self, heard: Heard) {
        match heard {
            Heard::Tray | Heard::NoTray => {
                let status_item = heard == Heard::Tray;
                let app = self.app.upgrade();
                let was = self.tray.borrow().is_some();
                if status_item
                    && !was
                    && let Some(app) = &app
                {
                    self.tray.replace(Some(app.hold()));
                    // Its wait at sign-in is over.
                    app.set_inactivity_timeout(0);
                }
                self.launching.take();
                if !status_item && was {
                    self.tray.take();
                    if let Some(app) = &app {
                        app.close_hidden_window();
                    }
                }
                if status_item != was {
                    self.send(Intent::SurfacesChanged {
                        surfaces: Surfaces { status_item },
                    });
                }
            }
            Heard::Activated { token } => {
                self.token.replace(token);
                self.send(Intent::Show { place: None });
            }
            Heard::Clicked { id, token } => {
                self.token.replace(token);
                if let Some(picked) = self.presence.picked(&id) {
                    self.pick(picked);
                }
            }
            Heard::Glanced => self.send(Intent::Glanced),
        }
    }

    fn pick(&self, picked: Picked) {
        match picked {
            Picked::Send(intent) => self.send(intent),
            Picked::Switch(account) => self.outside.switch(account),
            Picked::AddAccount => self.outside.add_account(),
            Picked::Link(link) => self.open_link(&link, None),
            Picked::Quit => self.quit(),
        }
    }

    fn open_link(&self, link: &str, parent: Option<&gtk::Window>) {
        // As the other apps open one: whether a browser took it is not the model's to hear.
        let _ = self.opener.open(link, parent);
    }
}

impl Host for Running {
    fn send(&self, intent: Intent) {
        Running::send(self, intent);
    }

    fn open(&self, link: &str, parent: &gtk::Window) {
        self.open_link(link, Some(parent));
    }

    fn keeps_running(&self) -> bool {
        self.tray.borrow().is_some()
    }

    fn quit(&self) {
        if let Some(app) = self.app.upgrade() {
            app.quit();
        }
    }

    fn data(&self) -> &DataFolder {
        &self.data
    }

    fn token(&self) -> Option<String> {
        self.token.take()
    }

    fn local_time(&self) -> Arc<dyn LocalTime> {
        Arc::clone(&self.local_time)
    }

    fn start_at_sign_in(&self) -> Option<&dyn StartAtSignIn> {
        self.start_at_sign_in.as_deref()
    }

    fn own_folders(&self) -> Vec<PathBuf> {
        self.own_folders.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_dir_names_no_installed_program() {
        let environment = HashMap::from([("HOME".to_owned(), "/home/someone".to_owned())]);

        let launch = app_launch(environment.clone());

        assert_eq!(
            launch.app_location, None,
            "cargo test is never Meson's install"
        );
        assert_eq!(launch.environment, environment);
        assert_eq!(launch.front, Front::Gnome);
        assert_eq!(launch.account_windows, None);
        assert!(launch.handover.is_none());
        assert_eq!(launch.widget_feed, None);
    }
}
