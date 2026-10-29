//! The main window: a navigation view whose root page is the accounts, under a header bar with
//! the window's title and when the accounts were read, Add Account, Refresh and the main menu.
//!
//! What the window does is its own actions, installed on its class under `pitboard.`, which GTK
//! keeps in the window's own action muxer and exports nowhere. It is an `AdwWindow`, since
//! GtkApplication exports an application window's own action group on the bus
//! (docs/facts/linux-app.md, What the app exports), so no other process reaches what the window
//! does. Its keys are bound on its class too. Closing it hides it while a tray keeps Pitboard
//! running, and quits Pitboard where none does. It keeps its size in the app's data folder, and
//! tells the model it was looked at each time it maps. Its dialogs are [`crate::dialogs`]'s, and
//! a link dropped on it is handed to the model, which says what Open Link shows of it.

mod account_group;
mod accounts;
mod accounts_page;
mod limit_row;
mod notices;

pub use account_group::AccountGroup;
pub use accounts_page::AccountsPage;
pub use limit_row::{LimitRow, level_name};
pub use notices::NoticeRow;

use std::cell::OnceCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use pitboard_model::{Front, Intent, Part};
use serde::{Deserialize, Serialize};

use crate::data_folder::DataFolder;
use crate::dialogs::Dialogs;
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
}

const SIZE: &str = "window.json";

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
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) shown: OnceCell<Shown>,
        pub(super) dialogs: OnceCell<Rc<Dialogs>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "PitboardWindow";
        type Type = super::Window;
        type ParentType = adw::Window;

        fn class_init(class: &mut Self::Class) {
            AccountsPage::ensure_type();
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
            class.install_action("pitboard.help", None, |window, _, _| {
                window.host().open(HELP, window.upcast_ref());
            });
            class.install_action("pitboard.quit", None, |window, _, _| window.host().quit());
            let control = gdk::ModifierType::CONTROL_MASK;
            class.add_binding_action(gdk::Key::n, control, "pitboard.add-account");
            class.add_binding_action(gdk::Key::r, control, "pitboard.refresh");
            class.add_binding_action(gdk::Key::q, control, "pitboard.quit");
            class.add_binding_action(gdk::Key::w, control, "window.close");
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {}

    impl WidgetImpl for Window {
        fn map(&self) {
            self.parent_map();
            self.obj().host().send(Intent::Glanced);
        }
    }

    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            let window = self.obj();
            window.keep_size();
            let host = window.host();
            if host.keeps_running() {
                window.set_visible(false);
                glib::Propagation::Stop
            } else {
                host.quit();
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
        let _ = imp
            .dialogs
            .set(Dialogs::follow(&window, shown, window.host()));
        let host = window.host();
        window.add_controller(links::drop_target(move |text| {
            host.send(Intent::LinkArrived { text });
        }));
        window.follow(shown);
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
        let menu = gio::Menu::new();
        menu.append(Some(&gnome.open_link), Some("pitboard.open-link"));
        menu.append(Some(&words.menu.help), Some("pitboard.help"));
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
    }

    fn kept_size(&self) -> Option<Size> {
        let file = self.host().data().file(SIZE).ok()?;
        serde_json::from_slice(&std::fs::read(file).ok()?).ok()
    }

    /// Keeps the window's size for its next launch: the size it has unless maximized.
    pub fn keep_size(&self) {
        let (width, height) = self.default_size();
        let size = Size {
            width,
            height,
            maximized: self.is_maximized(),
        };
        if let Ok(file) = self.host().data().file(SIZE) {
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
        let _ = crate::dialogs::open_link::OpenLink::new(host);
        window.destroy();
    }
}
