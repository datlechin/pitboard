//! Preferences, an Adwaita preferences dialog of three pages: General ([`general`]), Limits
//! ([`limits`]) and Advanced ([`advanced`]), each over what the model shows of this machine.
//!
//! A control sends its intent only for a value that differs from what the model shows, and the
//! model's own values drawn into the controls send nothing. Advanced asks the model to read the
//! schedule, the watch service and the command line each time it is shown, since a terminal can
//! change any of them while the app runs. What Uninstall takes back once the model has removed
//! Pitboard is [`advanced`]'s too.

pub mod advanced;
pub mod general;
mod limits;

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pitboard_model::{Front, MachineShown, SettingsSection, Snapshot};

use crate::window::Host;

/// The page each section is on.
const GENERAL: &str = "general";
const LIMITS: &str = "limits";
const ADVANCED: &str = "advanced";

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/preferences.ui")]
    pub struct Preferences {
        #[template_child]
        pub(super) general: TemplateChild<adw::PreferencesPage>,
        #[template_child]
        pub(super) start_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) start_at_sign_in: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) start_failed: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) version: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) version_number: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) limits: TemplateChild<adw::PreferencesPage>,
        #[template_child]
        pub(super) at_limit_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) at_limit: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub(super) share: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) terms: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) warm_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) warm_from: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) warm_to: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) none_to_warm: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) warm_standing: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) advanced: TemplateChild<adw::PreferencesPage>,
        #[template_child]
        pub(super) renew: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) renew_standing: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) watch_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) watch: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) watch_standing: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) command_line_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) path: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) looking: TemplateChild<adw::Spinner>,
        #[template_child]
        pub(super) command_line_standing: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) uninstall_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) uninstall: TemplateChild<adw::ButtonRow>,
        #[template_child]
        pub(super) removal: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) removing: TemplateChild<adw::Spinner>,
        #[template_child]
        pub(super) uninstall_again: TemplateChild<gtk::Button>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        // the snapshot drawn, which a control's value is told apart from
        pub(super) shown: RefCell<Option<Snapshot>>,
        // while a snapshot is drawn, so what it sets in a control sends nothing
        pub(super) drawing: Cell<bool>,
        // each warm-up switch, by its account's qualified label
        pub(super) warm_rows: RefCell<Vec<(String, adw::SwitchRow)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Preferences {
        const NAME: &'static str = "PitboardPreferences";
        type Type = super::Preferences;
        type ParentType = adw::PreferencesDialog;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Preferences {
        fn constructed(&self) {
            self.parent_constructed();
            let words = pitboard_model::app_words(Front::Gnome).settings;
            self.obj().set_title(&words.title);
            for (page, title) in [
                (&self.general, &words.general),
                (&self.limits, &words.limits),
                (&self.advanced, &words.advanced),
            ] {
                page.set_title(title);
            }
        }
    }

    impl WidgetImpl for Preferences {}
    impl AdwDialogImpl for Preferences {}
    impl PreferencesDialogImpl for Preferences {}
}

glib::wrapper! {
    pub struct Preferences(ObjectSubclass<imp::Preferences>)
        @extends adw::PreferencesDialog, adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl Preferences {
    /// Over what `snapshot` shows, sending what is set to `host`.
    pub fn new(host: Rc<dyn Host>, snapshot: &Snapshot) -> Preferences {
        let preferences: Preferences = glib::Object::new();
        let _ = preferences.imp().host.set(host);
        general::follow(&preferences);
        limits::follow(&preferences);
        advanced::follow(&preferences);
        preferences.show(snapshot);
        general::read(&preferences);
        let weak = preferences.downgrade();
        preferences.connect_visible_page_name_notify(move |_| {
            if let Some(preferences) = weak.upgrade() {
                preferences.visited();
            }
        });
        preferences
    }

    fn host(&self) -> Rc<dyn Host> {
        Rc::clone(self.imp().host.get().expect("made with its host"))
    }

    /// Draws what `snapshot` shows of this machine and of Uninstall.
    pub fn show(&self, snapshot: &Snapshot) {
        let imp = self.imp();
        imp.shown.replace(Some(snapshot.clone()));
        imp.drawing.set(true);
        general::show(self, snapshot);
        limits::show(self, &snapshot.machine);
        advanced::show(self, snapshot);
        imp.drawing.set(false);
    }

    /// Shows the page `section` is on, the warm-up's hours focused for `WarmUp`.
    pub fn open_at(&self, section: SettingsSection) {
        let page = match section {
            SettingsSection::General => GENERAL,
            SettingsSection::Limits | SettingsSection::WarmUp => LIMITS,
            SettingsSection::Advanced => ADVANCED,
        };
        if self.visible_page_name().as_deref() == Some(page) {
            self.visited();
        } else {
            self.set_visible_page_name(page);
        }
        if section == SettingsSection::WarmUp {
            self.set_focus(Some(&*self.imp().warm_from));
        }
    }

    // Advanced is shown: what a terminal may have changed since is read again.
    fn visited(&self) {
        if self.visible_page_name().as_deref() == Some(ADVANCED) {
            advanced::visited(&self.host());
        }
    }

    // What the snapshot drawn shows of this machine, where a person's change is told apart from
    // it; none while one is being drawn, whose values the controls take without sending them.
    fn machine(&self) -> Option<MachineShown> {
        let imp = self.imp();
        if imp.drawing.get() {
            return None;
        }
        imp.shown
            .borrow()
            .as_ref()
            .map(|shown| shown.machine.clone())
    }
}

/// `text` for a group's title or description, which libadwaita reads as markup.
fn escaped(text: Option<&str>) -> Option<glib::GString> {
    text.map(glib::markup_escape_text)
}

/// `label` saying `lines`, a line each, and hidden while there are none.
fn say(label: &gtk::Label, lines: &[Option<&str>]) {
    let lines: Vec<&str> = lines.iter().flatten().copied().collect();
    label.set_label(&lines.join("\n"));
    label.set_visible(!lines.is_empty());
}
