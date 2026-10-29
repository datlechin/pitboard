//! Keyboard Shortcuts, Ctrl+?, as a dialog of rows, each a key the window answers with its
//! keys as GTK says them. libadwaita 1.7 has no shortcuts dialog and GTK 4.18 deprecates its
//! shortcuts window and label, so the rows are libadwaita's own (docs/facts/linux-app.md, The
//! keyboard shortcuts) until the floor is libadwaita 1.8, whose `AdwShortcutsDialog` this
//! becomes.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::window::Shortcut;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/shortcuts.ui")]
    pub struct Shortcuts {
        #[template_child]
        pub(super) keys: TemplateChild<adw::PreferencesGroup>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Shortcuts {
        const NAME: &'static str = "PitboardShortcuts";
        type Type = super::Shortcuts;
        type ParentType = adw::Dialog;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Shortcuts {
        fn constructed(&self) {
            self.parent_constructed();
            let gnome = pitboard_model::gnome_words();
            let words = &gnome.shortcuts;
            self.obj().set_title(&gnome.keyboard_shortcuts);
            self.keys
                .set_title(&glib::markup_escape_text(&words.general));
            for shortcut in Shortcut::ALL {
                let row = adw::ActionRow::builder()
                    .title(shortcut.title(words))
                    .use_markup(false)
                    .build();
                let keys = gtk::Label::builder()
                    .label(shortcut.label())
                    .css_classes(["dim-label"])
                    .build();
                row.add_suffix(&keys);
                self.keys.add(&row);
            }
        }
    }

    impl WidgetImpl for Shortcuts {}
    impl AdwDialogImpl for Shortcuts {}
}

glib::wrapper! {
    pub struct Shortcuts(ObjectSubclass<imp::Shortcuts>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl Default for Shortcuts {
    fn default() -> Shortcuts {
        glib::Object::new()
    }
}
