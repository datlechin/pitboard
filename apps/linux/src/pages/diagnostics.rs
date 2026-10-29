//! Diagnostics: what `pitboard doctor` checks, for when a failed switch's one line is not enough
//! to act on. A row sums the checks up, then a group for Pitboard's and each tool's, a row a
//! check with its level's symbol, what it found and what to do, and the button that does it or
//! the docs about it. Check Again makes every check again, and Copy Report puts doctor's report
//! on the clipboard, with email addresses and account identifiers hidden, as the model gives it.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pitboard_model::{CheckGroup, CheckLine, ChecksShown, Front, Intent, Level, Pane, Part};

use crate::dialogs::question;
use crate::shown::Shown;
use crate::window::Host;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/diagnostics.ui")]
    pub struct DiagnosticsPage {
        #[template_child]
        pub(super) window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub(super) check_again: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) copy_report: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) waiting: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) checks: TemplateChild<adw::PreferencesPage>,
        #[template_child]
        pub(super) summary: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) summary_mark: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) checking: TemplateChild<adw::Spinner>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) report: RefCell<Option<String>>,
        // the groups shown, and the groups of rows drawn for them after the summary
        pub(super) groups: RefCell<Vec<CheckGroup>>,
        pub(super) drawn: RefCell<Vec<adw::PreferencesGroup>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DiagnosticsPage {
        const NAME: &'static str = "PitboardDiagnosticsPage";
        type Type = super::DiagnosticsPage;
        type ParentType = adw::NavigationPage;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DiagnosticsPage {
        fn constructed(&self) {
            self.parent_constructed();
            let page = self.obj();
            let words = pitboard_model::app_words(Front::Gnome).window;
            page.set_title(&words.diagnostics);
            self.window_title.set_title(&words.diagnostics);
            self.checking
                .upcast_ref::<gtk::Widget>()
                .update_property(&[gtk::accessible::Property::Label(&words.checking)]);
            for (button, label, help) in [
                (
                    &self.check_again,
                    &words.check_again,
                    &words.check_again_help,
                ),
                (
                    &self.copy_report,
                    &words.copy_report,
                    &words.copy_report_help,
                ),
            ] {
                button.set_tooltip_text(Some(help));
                button.update_property(&[gtk::accessible::Property::Label(label)]);
            }
            let weak = page.downgrade();
            self.check_again.connect_clicked(move |_| {
                if let Some(page) = weak.upgrade() {
                    page.read_again();
                }
            });
            let weak = page.downgrade();
            self.copy_report.connect_clicked(move |_| {
                if let Some(page) = weak.upgrade() {
                    page.copy_report();
                }
            });
        }
    }

    impl WidgetImpl for DiagnosticsPage {}

    impl NavigationPageImpl for DiagnosticsPage {
        fn showing(&self) {
            self.parent_showing();
            self.obj().read_again();
        }
    }
}

glib::wrapper! {
    pub struct DiagnosticsPage(ObjectSubclass<imp::DiagnosticsPage>)
        @extends adw::NavigationPage, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl DiagnosticsPage {
    pub(crate) fn follow(&self, shown: &Shown, host: Rc<dyn Host>) {
        let _ = self.imp().host.set(host);
        self.show(&shown.snapshot().machine.checks);
        let page = self.downgrade();
        shown.connect_changed(Part::Machine, move |shown| {
            if let Some(page) = page.upgrade() {
                page.show(&shown.snapshot().machine.checks);
            }
        });
    }

    fn host(&self) -> Rc<dyn Host> {
        Rc::clone(self.imp().host.get().expect("followed with its host"))
    }

    /// Asks the model to make every check again, as each showing of the page and Check Again do.
    pub fn read_again(&self) {
        if let Some(host) = self.imp().host.get() {
            host.send(Intent::PaneShown { pane: Pane::Checks });
        }
    }

    fn copy_report(&self) {
        if let Some(report) = self.imp().report.borrow().as_deref() {
            self.clipboard().set_text(report);
        }
    }

    fn show(&self, checks: &ChecksShown) {
        let imp = self.imp();
        imp.window_title
            .set_subtitle(checks.checked.as_deref().unwrap_or_default());
        imp.check_again.set_sensitive(!checks.checking);
        imp.copy_report.set_sensitive(checks.report.is_some());
        imp.report.replace(checks.report.clone());
        if let Some(waiting) = &checks.waiting {
            imp.waiting.set_title(waiting);
            if imp.waiting.paintable().is_none() {
                imp.waiting
                    .set_paintable(Some(&adw::SpinnerPaintable::new(Some(&*imp.waiting))));
            }
            imp.stack.set_visible_child_name("waiting");
            return;
        }
        imp.summary
            .set_title(checks.summary.as_deref().unwrap_or_default());
        imp.summary
            .set_subtitle(checks.verdict.as_deref().unwrap_or_default());
        mark(&imp.summary_mark, checks.level, None);
        imp.checking.set_visible(checks.checking);
        if *imp.groups.borrow() != checks.groups {
            self.draw(&checks.groups);
        }
        imp.stack.set_visible_child_name("checks");
    }

    // A preferences page adds a group only at its end at libadwaita 1.7, so every group after
    // the summary is drawn again.
    fn draw(&self, groups: &[CheckGroup]) {
        let imp = self.imp();
        for drawn in imp.drawn.take() {
            imp.checks.remove(&drawn);
        }
        let drawn = groups
            .iter()
            .map(|group| {
                let drawn = adw::PreferencesGroup::builder()
                    .title(glib::markup_escape_text(&group.heading))
                    .build();
                for line in &group.lines {
                    drawn.add(&self.row(line));
                }
                imp.checks.add(&drawn);
                drawn
            })
            .collect();
        imp.drawn.replace(drawn);
        imp.groups.replace(groups.to_vec());
    }

    fn row(&self, line: &CheckLine) -> adw::ActionRow {
        let said: Vec<&str> = std::iter::once(line.detail.as_str())
            .chain(line.advice.as_deref())
            .filter(|said| !said.is_empty())
            .collect();
        let row = adw::ActionRow::builder()
            .title(&line.name)
            .subtitle(said.join("\n"))
            .use_markup(false)
            .subtitle_selectable(true)
            .build();
        let level = gtk::Image::new();
        mark(&level, line.level, Some(&line.spoken_level));
        row.add_prefix(&level);
        let host = self.host();
        if let Some(action) = &line.action {
            let button = gtk::Button::builder()
                .label(&action.title)
                .valign(gtk::Align::Center)
                .sensitive(action.enabled)
                .build();
            let action = action.clone();
            button.connect_clicked(move |button| {
                let (host, intent) = (host.clone(), action.intent.clone());
                question::asking_first(button, action.confirm.as_ref(), move || {
                    host.send(intent.clone());
                });
            });
            row.add_suffix(&button);
        } else if let Some(learn) = &line.learn {
            let button = gtk::Button::builder()
                .label(&learn.title)
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            let link = learn.link.clone();
            button.connect_clicked(move |button| {
                if let Some(window) = button.root().and_downcast::<gtk::Window>() {
                    host.open(&link, &window);
                }
            });
            row.add_suffix(&button);
        }
        row
    }
}

/// `image` as the symbol of `level`, in its colour, said as `spoken` where nothing beside it says
/// the level.
fn mark(image: &gtk::Image, level: Level, spoken: Option<&str>) {
    let (icon, class) = match level {
        Level::Ok => ("object-select-symbolic", "success"),
        Level::Warn => ("dialog-warning-symbolic", "warning"),
        Level::Fail => ("dialog-error-symbolic", "error"),
    };
    image.set_icon_name(Some(icon));
    image.set_css_classes(&[class]);
    if let Some(spoken) = spoken {
        image.update_property(&[gtk::accessible::Property::Label(spoken)]);
    }
}
