//! The main window: a navigation view whose root page is the accounts, under a header bar with
//! the window's title and when the accounts were read, Add Account, Refresh and the main menu.
//!
//! What the window does is its own actions, installed on its class under `pitboard.`, which GTK
//! keeps in the window's own action muxer and exports nowhere. It is an `AdwWindow`, since
//! GtkApplication exports an application window's own action group on the bus
//! (docs/facts/linux-app.md, What the app exports), so no other process reaches what the window
//! does. Its keys are bound on its class too ([`keys`]). Closing it, from the compositor or
//! Ctrl+W, closes its dialogs first, then hides it while a tray keeps Pitboard running, and where
//! none does closes it, which ends Pitboard once nothing else holds the app. It keeps its size in
//! the app's data folder, and tells the model it was looked at each time it maps. Its dialogs are
//! [`crate::dialogs`]'s, and a link dropped on it is handed to the model, which says what Open
//! Link shows of it.
//!
//! It answers each window request of the model's once: it comes forward, with the activation
//! token what asked for it came with, and shows the place asked for, the accounts, Diagnostics
//! or Activity pushed over them, Preferences on its page, or the first launch's welcome.

mod account_group;
mod accounts;
mod accounts_page;
mod keys;
mod limit_row;
mod notices;

pub use account_group::AccountGroup;
pub use accounts_page::AccountsPage;
pub use keys::Shortcut;
pub use limit_row::{LimitRow, level_name};
pub use notices::NoticeRow;

use std::cell::{Cell, OnceCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::translate::IntoGlib;
use gtk::{gio, glib};
use pitboard_model::{Front, Intent, LocalTime, Part, Place, RemovalShown, WindowRequest};
use serde::{Deserialize, Serialize};

use crate::data_folder::DataFolder;
use crate::dialogs::Dialogs;
use crate::pages::{ActivityPage, DiagnosticsPage};
use crate::platform::autostart::StartAtSignIn;
use crate::platform::links;
use crate::presence::HELP;
use crate::shown::Shown;

/// What the window asks of the app it is the window of.
pub trait Host {
    fn send(&self, intent: Intent);
    /// The browser opens `link`, over `parent`.
    fn open(&self, link: &str, parent: &gtk::Window);
    /// Whether a tray keeps Pitboard running once the window is closed.
    fn keeps_running(&self) -> bool;
    fn quit(&self);
    fn data(&self) -> &DataFolder;
    /// The activation token what last asked for the window came with, once.
    fn token(&self) -> Option<String>;
    /// The person's clock, which Preferences says the warm-up's hours by.
    fn local_time(&self) -> Arc<dyn LocalTime>;
    /// Start at sign-in, where the build is installed where an entry can start it: none for one
    /// Cargo built alone.
    fn start_at_sign_in(&self) -> Option<&dyn StartAtSignIn>;
    /// The app's own folders in the person's home, which Uninstall takes back.
    fn own_folders(&self) -> Vec<PathBuf>;
}

const SIZE: &str = "window.json";

// The tags of the window's pages, as window.ui names them.
const ACCOUNTS: &str = "accounts";
const DIAGNOSTICS: &str = "diagnostics";
const ACTIVITY: &str = "activity";

// What the window remembers of itself, as the Windows app keeps `placement.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Size {
    width: i32,
    height: i32,
    maximized: bool,
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) navigation: TemplateChild<adw::NavigationView>,
        #[template_child]
        pub(super) accounts: TemplateChild<adw::NavigationPage>,
        #[template_child]
        pub(super) window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub(super) add_account: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) refresh: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) main_menu: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub(super) accounts_page: TemplateChild<AccountsPage>,
        #[template_child]
        pub(super) diagnostics: TemplateChild<DiagnosticsPage>,
        #[template_child]
        pub(super) activity: TemplateChild<ActivityPage>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) shown: OnceCell<Shown>,
        pub(super) dialogs: OnceCell<Rc<Dialogs>>,
        // the window request it last answered
        pub(super) answered: Cell<u64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "PitboardWindow";
        type Type = super::Window;
        type ParentType = adw::Window;

        fn class_init(class: &mut Self::Class) {
            AccountsPage::ensure_type();
            DiagnosticsPage::ensure_type();
            ActivityPage::ensure_type();
            class.bind_template();
            class.install_action("pitboard.add-account", None, |window, _, _| {
                window.host().send(crate::outside::add_account_intent());
            });
            class.install_action("pitboard.refresh", None, |window, _, _| {
                window.host().send(Intent::Refresh { asked: true });
            });
            class.install_action("pitboard.open-link", None, |window, _, _| {
                if let Some(dialogs) = window.imp().dialogs.get() {
                    dialogs.show_open_link();
                }
            });
            class.install_action("pitboard.diagnostics", None, |window, _, _| {
                window.show_page(DIAGNOSTICS);
            });
            class.install_action("pitboard.activity", None, |window, _, _| {
                window.show_page(ACTIVITY);
            });
            class.install_action("pitboard.preferences", None, |window, _, _| {
                if let Some(dialogs) = window.imp().dialogs.get() {
                    dialogs.show_preferences(None);
                }
            });
            class.install_action("pitboard.shortcuts", None, |window, _, _| {
                if let Some(dialogs) = window.imp().dialogs.get() {
                    dialogs.show_shortcuts();
                }
            });
            class.install_action("pitboard.help", None, |window, _, _| {
                window.host().open(HELP, window.upcast_ref());
            });
            class.install_action("pitboard.about", None, |window, _, _| {
                if let Some(dialogs) = window.imp().dialogs.get() {
                    dialogs.show_about();
                }
            });
            class.install_action("pitboard.quit", None, |window, _, _| window.host().quit());
            for shortcut in Shortcut::ALL {
                if let Some(action) = shortcut.action() {
                    let (key, modifiers) = shortcut.keys();
                    class.add_binding_action(key, modifiers, action);
                }
            }
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().take_close_requests();
        }
    }

    impl WidgetImpl for Window {
        fn map(&self) {
            self.parent_map();
            self.obj().host().send(Intent::Glanced);
        }
    }

    impl WindowImpl for Window {
        // Closed, it lets go of the app, which ends where nothing else holds it.
        fn close_request(&self) -> glib::Propagation {
            let window = self.obj();
            window.close_dialogs();
            window.keep_size();
            if window.host().keeps_running() {
                window.set_visible(false);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    }

    impl AdwWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::Window, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native,
            gtk::Root, gtk::ShortcutManager;
}

impl Window {
    /// The window over what `shown` shows, of the app `app` names, where there is one.
    pub fn new(app: Option<&adw::Application>, shown: &Shown, host: Rc<dyn Host>) -> Window {
        let window: Window = glib::Object::builder().property("application", app).build();
        let imp = window.imp();
        let _ = imp.host.set(host);
        let _ = imp.shown.set(shown.clone());
        window.words();
        if let Some(size) = window.kept_size() {
            window.set_default_size(size.width, size.height);
            window.set_maximized(size.maximized);
        }
        imp.accounts_page.follow(shown, window.host());
        imp.diagnostics.follow(shown, window.host());
        imp.activity.follow(shown, window.host());
        let _ = imp
            .dialogs
            .set(Dialogs::follow(&window, shown, window.host()));
        let host = window.host();
        window.add_controller(links::drop_target(move |text| {
            host.send(Intent::LinkArrived { text });
        }));
        window.follow(shown);
        // Made for a request, it answers it.
        let request = shown.snapshot().window_request;
        window.answer(request);
        window
    }

    fn host(&self) -> Rc<dyn Host> {
        Rc::clone(self.imp().host.get().expect("made with its host"))
    }

    fn words(&self) {
        let imp = self.imp();
        let words = pitboard_model::app_words(Front::Gnome);
        let gnome = pitboard_model::gnome_words();
        self.set_title(Some(&words.app_name));
        imp.accounts.set_title(&words.window.accounts);
        imp.window_title.set_title(&words.window.accounts);
        imp.add_account
            .set_tooltip_text(Some(&words.window.add_account_help));
        imp.add_account
            .update_property(&[gtk::accessible::Property::Label(&words.window.add_account)]);
        imp.refresh
            .set_tooltip_text(Some(&words.window.refresh_help));
        imp.refresh
            .update_property(&[gtk::accessible::Property::Label(&words.window.refresh)]);
        imp.main_menu.set_tooltip_text(Some(&gnome.main_menu));
        // GNOME's primary menu: the app's own items, then Preferences, Keyboard Shortcuts, Help
        // and About in one section at its end, and no Quit (docs/facts/linux-app.md, GNOME's
        // words).
        let (own, standard) = (gio::Menu::new(), gio::Menu::new());
        own.append(Some(&gnome.open_link), Some("pitboard.open-link"));
        own.append(
            Some(&words.window.diagnostics),
            Some("pitboard.diagnostics"),
        );
        own.append(Some(&words.window.activity), Some("pitboard.activity"));
        standard.append(Some(&words.menu.settings), Some("pitboard.preferences"));
        standard.append(Some(&gnome.keyboard_shortcuts), Some("pitboard.shortcuts"));
        standard.append(Some(&words.menu.help), Some("pitboard.help"));
        standard.append(Some(&words.menu.about), Some("pitboard.about"));
        let menu = gio::Menu::new();
        menu.append_section(None, &own);
        menu.append_section(None, &standard);
        imp.main_menu.set_menu_model(Some(&menu));
    }

    fn follow(&self, shown: &Shown) {
        let updated = |window: &Window, shown: &Shown| {
            window
                .imp()
                .window_title
                .set_subtitle(&shown.snapshot().updated_window);
        };
        let reading = |window: &Window, shown: &Shown| {
            window.action_set_enabled("pitboard.refresh", !shown.snapshot().reading);
        };
        updated(self, shown);
        reading(self, shown);
        let window = self.downgrade();
        shown.connect_changed(Part::UpdatedWindow, move |shown| {
            if let Some(window) = window.upgrade() {
                updated(&window, shown);
            }
        });
        let window = self.downgrade();
        shown.connect_changed(Part::Reading, move |shown| {
            if let Some(window) = window.upgrade() {
                reading(&window, shown);
            }
        });
        let window = self.downgrade();
        shown.connect_changed(Part::WindowRequest, move |shown| {
            if let Some(window) = window.upgrade() {
                let request = shown.snapshot().window_request;
                window.answer(request);
            }
        });
    }

    // A request answered once, as each app opens its window once for each serial.
    fn answer(&self, request: WindowRequest) {
        let answered = &self.imp().answered;
        if request.serial <= answered.get() {
            return;
        }
        answered.set(request.serial);
        self.present_with(self.host().token().as_deref());
        self.show_place(request.place);
    }

    fn show_place(&self, place: Option<Place>) {
        let dialogs = self.imp().dialogs.get();
        match place {
            None => {}
            Some(Place::Accounts) => {
                self.imp().navigation.pop_to_tag(ACCOUNTS);
            }
            Some(Place::Checks) => self.show_page(DIAGNOSTICS),
            Some(Place::Activity) => self.show_page(ACTIVITY),
            Some(Place::Settings { section }) => {
                if let Some(dialogs) = dialogs {
                    dialogs.show_preferences(Some(section));
                }
            }
            Some(Place::Welcome) => {
                if let Some(dialogs) = dialogs {
                    dialogs.show_welcome();
                }
            }
        }
    }

    // Pushed over the accounts, or, where it is shown already, read again.
    fn show_page(&self, tag: &str) {
        let imp = self.imp();
        let navigation = &imp.navigation;
        if navigation.visible_page_tag().as_deref() == Some(tag) {
            match tag {
                DIAGNOSTICS => imp.diagnostics.read_again(),
                _ => imp.activity.read_again(),
            }
            return;
        }
        navigation.pop_to_tag(ACCOUNTS);
        navigation.push_by_tag(tag);
    }

    fn kept_size(&self) -> Option<Size> {
        let file = self.host().data().file(SIZE);
        serde_json::from_slice(&std::fs::read(file).ok()?).ok()
    }

    /// Keeps the window's size for its next launch: the size it has unless maximized. Not once
    /// Pitboard is removed, whose app took its folder back.
    pub fn keep_size(&self) {
        let removed = (self.imp().shown.get())
            .is_some_and(|shown| shown.snapshot().removal == Some(RemovalShown::Done));
        if removed {
            return;
        }
        let (width, height) = self.default_size();
        let size = Size {
            width,
            height,
            maximized: self.is_maximized(),
        };
        if let Ok(file) = self.host().data().file_to_write(SIZE) {
            // A size not kept is the default size next time, which says nothing to the person.
            let _ = std::fs::write(file, serde_json::to_vec(&size).expect("a size is JSON"));
        }
    }

    /// Brings the window forward, with the activation token where one came with what asked.
    pub fn present_with(&self, token: Option<&str>) {
        if let Some(token) = token {
            self.set_startup_id(token);
        }
        self.present();
    }

    // libadwaita's dialog host answers close-request first, closing the top dialog and ending the
    // request, which kept the window up through a logout (docs/facts/linux-app.md, The dialogs).
    fn take_close_requests(&self) {
        let Some(host) = self.child() else {
            return;
        };
        let request = glib::subclass::SignalId::lookup("close-request", gtk::Window::static_type())
            .expect("GtkWindow's signal");
        // SAFETY: the window and its child live through the call, which blocks handlers and runs
        // none.
        unsafe {
            glib::gobject_ffi::g_signal_handlers_block_matched(
                self.upcast_ref::<glib::Object>().as_ptr(),
                glib::gobject_ffi::G_SIGNAL_MATCH_ID | glib::gobject_ffi::G_SIGNAL_MATCH_DATA,
                request.into_glib(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                host.as_ptr().cast(),
            );
        }
    }

    // Top first, as Escape closes each: one that stays, as a sign-in under way does, keeps those
    // under it.
    fn close_dialogs(&self) {
        while let Some(dialog) = self.visible_dialog() {
            dialog.close();
            if self.visible_dialog().as_ref() == Some(&dialog) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Shows(DataFolder);

    impl Host for Shows {
        fn send(&self, _: Intent) {}
        fn open(&self, _: &str, _: &gtk::Window) {}
        fn keeps_running(&self) -> bool {
            false
        }
        fn quit(&self) {}
        fn data(&self) -> &DataFolder {
            &self.0
        }
        fn token(&self) -> Option<String> {
            None
        }
        fn local_time(&self) -> Arc<dyn LocalTime> {
            crate::local_time::DesktopClock::following(None, None).local_time()
        }
        fn start_at_sign_in(&self) -> Option<&dyn StartAtSignIn> {
            None
        }
        fn own_folders(&self) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    // gtk4-builder-tool cannot read a template whose parent is libadwaita's, so each view is
    // built from its own, with GTK's and libadwaita's warnings fatal (docs/facts/linux-app.md,
    // The checks).
    #[gtk::test]
    fn each_view_is_built_from_its_template_with_no_warning() {
        adw::init().unwrap();
        crate::resources::register();
        let fatal = glib::LogLevels::LEVEL_WARNING | glib::LogLevels::LEVEL_CRITICAL;
        for domain in ["Gtk", "Adwaita", "GLib-GObject"] {
            glib::log_set_fatal_mask(Some(domain), fatal);
        }
        let scratch = tempfile::tempdir().unwrap();
        let shown = Shown::new(crate::testing::snapshot(1));

        let window = Window::new(None, &shown, Rc::new(Shows(DataFolder::at(scratch.path()))));

        let words = pitboard_model::app_words(Front::Gnome);
        let imp = window.imp();
        assert_eq!(window.title().as_deref(), Some(words.app_name.as_str()));
        assert_eq!(imp.window_title.title(), words.window.accounts);
        assert_eq!(imp.window_title.subtitle(), shown.snapshot().updated_window);
        assert_eq!(
            imp.accounts_page.visible_child_name().as_deref(),
            Some("status"),
            "nothing read yet"
        );
        let host: Rc<dyn Host> = Rc::new(Shows(DataFolder::at(scratch.path())));
        let _ = AccountGroup::new(host.clone());
        let _ = LimitRow::default();
        let _ = NoticeRow::new(host.clone());
        let add = pitboard_model::Sheet::Add { tool: None };
        let _ = crate::dialogs::sign_in::SignIn::new(add, host.clone());
        let _ = crate::dialogs::name::SheetDialog::stowing(host.clone());
        let _ = crate::dialogs::open_link::OpenLink::new(host.clone());
        let _ = crate::preferences::Preferences::new(host.clone(), &shown.snapshot());
        let _ = crate::dialogs::welcome::Welcome::new(host);
        let _ = crate::dialogs::shortcuts::Shortcuts::default();
        let _ = crate::dialogs::about::about();
        window.destroy();
    }

    // Nothing is kept until the window closes, so showing it makes no folder.
    #[gtk::test]
    fn a_window_reads_its_size_without_making_the_apps_folder() {
        adw::init().unwrap();
        crate::resources::register();
        let scratch = tempfile::tempdir().unwrap();
        let folder = scratch.path().join("data").join(crate::config::APP_ID);
        let host = Rc::new(Shows(DataFolder::at(&folder)));

        let window = Window::new(None, &Shown::new(crate::testing::snapshot(1)), host);

        assert!(!folder.exists(), "made by reading the size");
        window.keep_size();
        assert!(folder.join(SIZE).is_file());
        window.destroy();
    }

    // Ctrl+W is `window.close`, which asks as the compositor's close does.
    #[gtk::test]
    fn without_a_tray_ctrl_w_with_a_dialog_up_closes_the_dialog_then_the_window() {
        adw::init().unwrap();
        crate::resources::register();
        let scratch = tempfile::tempdir().unwrap();
        let host = Rc::new(Shows(DataFolder::at(scratch.path())));
        let window = Window::new(None, &Shown::new(crate::testing::snapshot(1)), host);
        window.present();
        window
            .activate_action("pitboard.preferences", None)
            .unwrap();
        let preferences = window.visible_dialog().expect("Preferences");
        let closed = Rc::new(Cell::new(false));
        let told = closed.clone();
        preferences.connect_closed(move |_| told.set(true));

        window.activate_action("window.close", None).unwrap();

        assert!(closed.get(), "Preferences up");
        let window: gtk::Widget = window.upcast();
        assert!(
            !gtk::Window::list_toplevels().contains(&window),
            "the window up"
        );
    }

    fn descendants<T: IsA<gtk::Widget>>(widget: &gtk::Widget) -> Vec<T> {
        let mut found = Vec::new();
        let mut child = widget.first_child();
        while let Some(widget) = child {
            found.extend(widget.clone().downcast::<T>());
            found.extend(descendants(&widget));
            child = widget.next_sibling();
        }
        found
    }

    // A status page's description is markup (Adw-1.gir, AdwStatusPage:description), and what
    // the model says there can name a path or a pasted link's host, either of which may hold
    // `&` or `<`.
    #[gtk::test]
    fn a_status_pages_description_shows_the_models_text_as_written() {
        adw::init().unwrap();
        crate::resources::register();
        let fatal = glib::LogLevels::LEVEL_WARNING | glib::LogLevels::LEVEL_CRITICAL;
        for domain in ["Gtk", "Adwaita", "GLib-GObject"] {
            glib::log_set_fatal_mask(Some(domain), fatal);
        }
        let said = "/home/a&b/<c>: Permission denied (os error 13)";
        let mut snapshot = crate::testing::snapshot(1);
        snapshot.accounts_shown = pitboard_model::AccountsShown::ReadFailed {
            title: "Can’t read accounts".into(),
            detail: said.into(),
            retry: pitboard_model::Choice {
                title: "Try Again".into(),
                intent: Intent::Refresh { asked: true },
                enabled: true,
            },
        };
        snapshot.machine.activity.empty = Some(pitboard_model::EmptyList {
            title: "No activity".into(),
            detail: said.into(),
        });
        snapshot.welcome.lines = vec![said.into()];
        snapshot.account_windows.picker = Some(pitboard_model::LinkPicker {
            arrival: 1,
            armed: true,
            shown: pitboard_model::PickerShown::Refused {
                title: "Can’t open this link".into(),
                reason: said.into(),
            },
        });
        let scratch = tempfile::tempdir().unwrap();
        let host: Rc<dyn Host> = Rc::new(Shows(DataFolder::at(scratch.path())));

        let window = Window::new(None, &Shown::new(snapshot.clone()), host.clone());
        let open_link = crate::dialogs::open_link::OpenLink::new(host.clone());
        open_link.show(&snapshot.account_windows);
        let welcome = crate::dialogs::welcome::Welcome::new(host);
        welcome.show(&snapshot.welcome);
        // A dialog holds what it shows only once it is presented (`adw_dialog_present`).
        window.present();
        open_link.present(Some(&window));
        welcome.present(Some(&window));

        let views: [(&str, gtk::Widget); 3] = [
            ("the window", window.clone().upcast()),
            ("Open Link", open_link.upcast()),
            ("the welcome", welcome.upcast()),
        ];
        for (view, widget) in views {
            let pages = descendants::<adw::StatusPage>(&widget);
            let shows = |page: &adw::StatusPage| {
                descendants::<gtk::Label>(page.upcast_ref())
                    .iter()
                    .any(|label| label.text() == said)
            };
            assert!(pages.iter().any(shows), "{view}");
        }
        window.destroy();
    }
}
