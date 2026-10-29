//! Activity: Pitboard's log, newest first, whichever front end made each change, a line a
//! change in its columns: when, what, to which account, how it ended, quieter where it ended as
//! asked, and who asked. Refresh reads the log again.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use pitboard_model::{ActivityLine, ActivityShown, Front, Intent, Pane, Part};

use crate::shown::Shown;
use crate::window::Host;

// A column: its title, what it says of a line, whether it is quieter for that line, and whether
// it takes the room left.
struct Column {
    title: String,
    says: fn(&ActivityLine) -> &str,
    quiet: fn(&ActivityLine) -> bool,
    expands: bool,
    numeric: bool,
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/activity.ui")]
    pub struct ActivityPage {
        #[template_child]
        pub(super) refresh: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) empty: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) lines: TemplateChild<gtk::ColumnView>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        // the lines shown, each a `BoxedAnyObject` of its `ActivityLine`
        pub(super) store: OnceCell<gio::ListStore>,
        pub(super) shown: RefCell<Vec<ActivityLine>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ActivityPage {
        const NAME: &'static str = "PitboardActivityPage";
        type Type = super::ActivityPage;
        type ParentType = adw::NavigationPage;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ActivityPage {
        fn constructed(&self) {
            self.parent_constructed();
            let page = self.obj();
            let words = pitboard_model::app_words(Front::Gnome);
            page.set_title(&words.window.activity);
            self.refresh
                .set_tooltip_text(Some(&words.activity.refresh_help));
            self.refresh
                .update_property(&[gtk::accessible::Property::Label(&words.window.refresh)]);
            let weak = page.downgrade();
            self.refresh.connect_clicked(move |_| {
                if let Some(page) = weak.upgrade() {
                    page.read_again();
                }
            });
            let store = gio::ListStore::new::<glib::BoxedAnyObject>();
            self.lines
                .set_model(Some(&gtk::NoSelection::new(Some(store.clone()))));
            for column in columns(&words.activity) {
                self.lines.append_column(&column_of(column));
            }
            let _ = self.store.set(store);
        }
    }

    impl WidgetImpl for ActivityPage {}

    impl NavigationPageImpl for ActivityPage {
        fn showing(&self) {
            self.parent_showing();
            self.obj().read_again();
        }
    }
}

glib::wrapper! {
    pub struct ActivityPage(ObjectSubclass<imp::ActivityPage>)
        @extends adw::NavigationPage, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ActivityPage {
    pub(crate) fn follow(&self, shown: &Shown, host: Rc<dyn Host>) {
        let _ = self.imp().host.set(host);
        self.show(&shown.snapshot().machine.activity);
        let page = self.downgrade();
        shown.connect_changed(Part::Machine, move |shown| {
            if let Some(page) = page.upgrade() {
                page.show(&shown.snapshot().machine.activity);
            }
        });
    }

    /// Asks the model to read the log again, as each showing of the page and Refresh do.
    pub fn read_again(&self) {
        if let Some(host) = self.imp().host.get() {
            host.send(Intent::PaneShown {
                pane: Pane::Activity,
            });
        }
    }

    fn show(&self, activity: &ActivityShown) {
        let imp = self.imp();
        if let Some(empty) = &activity.empty {
            imp.empty.set_title(&empty.title);
            imp.empty
                .set_description(Some(&glib::markup_escape_text(&empty.detail)));
            imp.stack.set_visible_child_name("empty");
        } else {
            imp.stack.set_visible_child_name("lines");
        }
        if *imp.shown.borrow() == activity.lines {
            return;
        }
        let store = imp.store.get().expect("made with its store");
        let lines: Vec<glib::BoxedAnyObject> = (activity.lines.iter().cloned())
            .map(glib::BoxedAnyObject::new)
            .collect();
        store.splice(0, store.n_items(), &lines);
        imp.shown.replace(activity.lines.clone());
    }
}

fn columns(words: &pitboard_model::ActivityWords) -> [Column; 5] {
    let loud = |_: &ActivityLine| false;
    [
        Column {
            title: words.date.clone(),
            says: |line| &line.date,
            quiet: loud,
            expands: false,
            numeric: true,
        },
        Column {
            title: words.change.clone(),
            says: |line| &line.change,
            quiet: loud,
            expands: true,
            numeric: false,
        },
        Column {
            title: words.account.clone(),
            says: |line| &line.account,
            quiet: loud,
            expands: true,
            numeric: false,
        },
        Column {
            title: words.result.clone(),
            says: |line| &line.result,
            quiet: |line| line.done,
            expands: true,
            numeric: false,
        },
        Column {
            title: words.by.clone(),
            says: |line| &line.asked_by,
            quiet: |_| true,
            expands: false,
            numeric: false,
        },
    ]
}

fn column_of(column: Column) -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    let numeric = column.numeric;
    factory.connect_setup(move |_, item| {
        let label = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        if numeric {
            label.add_css_class("numeric");
        }
        list_item(item).set_child(Some(&label));
    });
    let (says, quiet) = (column.says, column.quiet);
    factory.connect_bind(move |_, item| {
        let item = list_item(item);
        let (Some(label), Some(line)) = (
            item.child().and_downcast::<gtk::Label>(),
            item.item().and_downcast::<glib::BoxedAnyObject>(),
        ) else {
            return;
        };
        let line = line.borrow::<ActivityLine>();
        label.set_text(says(&line));
        if quiet(&line) {
            label.add_css_class("dim-label");
        } else {
            label.remove_css_class("dim-label");
        }
    });
    let made = gtk::ColumnViewColumn::new(Some(&column.title), Some(factory));
    made.set_expand(column.expands);
    made.set_resizable(true);
    made
}

fn list_item(item: &glib::Object) -> &gtk::ListItem {
    item.downcast_ref()
        .expect("a column view's factory is given list items")
}
