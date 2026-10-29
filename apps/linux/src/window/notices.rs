//! The notices above the accounts, the most pressing first, and the setup step's tip after
//! them: a row each, with its severity's symbol, its title and what it says, its buttons and the
//! one that puts it away. A button the model asks a question for asks it first. The notices'
//! list follows the store's list model, so a notice keeps its row while it is shown. A
//! notice's countdown is drawn here, in the model's words, each second while its row is shown.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use pitboard_model::{Intent, PanelNotice, Question, SetupStep, Severity};

use super::Host;
use crate::dialogs::question;
use crate::shown::Row;

// What a button of a row does.
#[derive(Clone, PartialEq)]
enum Does {
    Send {
        intent: Intent,
        confirm: Option<Question>,
    },
    Open(String),
}

#[derive(Clone, PartialEq)]
struct Button {
    title: String,
    enabled: bool,
    // the one the row is for, drawn as GNOME draws the suggested action
    suggested: bool,
    does: Does,
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/notice-row.ui")]
    pub struct NoticeRow {
        #[template_child]
        pub(super) mark: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) buttons: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) dismiss: TemplateChild<gtk::Button>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) shown: RefCell<Vec<Button>>,
        pub(super) dismisses: RefCell<Option<Does>>,
        // what it says above its countdown, and the notice it counts down for
        pub(super) said: RefCell<String>,
        pub(super) counting: RefCell<Option<PanelNotice>>,
        pub(super) ticking: RefCell<Option<glib::SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NoticeRow {
        const NAME: &'static str = "PitboardNoticeRow";
        type Type = super::NoticeRow;
        type ParentType = adw::ActionRow;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for NoticeRow {
        fn constructed(&self) {
            self.parent_constructed();
            let row = self.obj().downgrade();
            self.dismiss.connect_clicked(move |_| {
                if let Some(row) = row.upgrade()
                    && let Some(does) = row.imp().dismisses.borrow().clone()
                {
                    row.act(&does);
                }
            });
        }

        fn dispose(&self) {
            self.obj().stop_counting();
        }
    }

    impl WidgetImpl for NoticeRow {
        fn map(&self) {
            self.parent_map();
            self.obj().count();
        }

        fn unmap(&self) {
            self.obj().stop_counting();
            self.parent_unmap();
        }
    }

    impl ListBoxRowImpl for NoticeRow {}
    impl PreferencesRowImpl for NoticeRow {}
    impl ActionRowImpl for NoticeRow {}
}

glib::wrapper! {
    pub struct NoticeRow(ObjectSubclass<imp::NoticeRow>)
        @extends adw::ActionRow, adw::PreferencesRow, gtk::ListBoxRow, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl NoticeRow {
    pub fn new(host: Rc<dyn Host>) -> NoticeRow {
        let row: NoticeRow = glib::Object::new();
        let _ = row.imp().host.set(host);
        row
    }

    /// Shows `notice`: its buttons and the one that puts it away, and its countdown, where it
    /// has one.
    pub fn show_notice(&self, notice: &PanelNotice) {
        let imp = self.imp();
        let icon = match notice.severity {
            Severity::Info => "dialog-information-symbolic",
            Severity::Warning => "dialog-warning-symbolic",
            Severity::Error => "dialog-error-symbolic",
        };
        imp.mark.set_icon_name(Some(icon));
        imp.mark
            .update_property(&[gtk::accessible::Property::Label(&notice.spoken_severity)]);
        self.set_title(&notice.title);
        let send = |intent: &Intent, confirm: &Option<Question>| Does::Send {
            intent: intent.clone(),
            confirm: confirm.clone(),
        };
        let mut buttons: Vec<Button> = (notice.actions.iter())
            .filter(|action| !action.dismisses)
            .map(|action| Button {
                title: action.title.clone(),
                enabled: action.enabled,
                suggested: false,
                does: send(&action.intent, &action.confirm),
            })
            .collect();
        buttons.extend(notice.learn.iter().map(|learn| Button {
            title: learn.title.clone(),
            enabled: true,
            suggested: false,
            does: Does::Open(learn.link.clone()),
        }));
        let dismiss = (notice.actions.iter())
            .find(|action| action.dismisses)
            .map(|action| (action.title.clone(), send(&action.intent, &action.confirm)));
        self.fill(notice.lines.join("\n"), buttons, dismiss);
        let counts = notice.until.is_some().then(|| notice.clone());
        imp.counting.replace(counts);
        self.count();
    }

    /// Shows the setup step as a tip: its first choice is what the step is for, and the next
    /// turns it down.
    pub fn show_tip(&self, step: &SetupStep) {
        let imp = self.imp();
        imp.mark.set_icon_name(Some("dialog-information-symbolic"));
        self.set_title(&step.title);
        let buttons = (step.actions.iter().enumerate())
            .map(|(at, choice)| Button {
                title: choice.title.clone(),
                enabled: choice.enabled,
                suggested: at == 0,
                does: Does::Send {
                    intent: choice.intent.clone(),
                    confirm: None,
                },
            })
            .collect();
        self.fill(step.detail.clone(), buttons, None);
        imp.counting.replace(None);
        self.count();
    }

    // Its buttons are made again only when they change, so one with the focus keeps it.
    fn fill(&self, said: String, buttons: Vec<Button>, dismiss: Option<(String, Does)>) {
        let imp = self.imp();
        imp.said.replace(said);
        if *imp.shown.borrow() != buttons {
            while let Some(child) = imp.buttons.first_child() {
                imp.buttons.remove(&child);
            }
            for shown in &buttons {
                let button = gtk::Button::with_label(&shown.title);
                button.set_sensitive(shown.enabled);
                if shown.suggested {
                    button.add_css_class("suggested-action");
                }
                let (row, does) = (self.downgrade(), shown.does.clone());
                button.connect_clicked(move |_| {
                    if let Some(row) = row.upgrade() {
                        row.act(&does);
                    }
                });
                imp.buttons.append(&button);
            }
            imp.shown.replace(buttons);
        }
        imp.buttons.set_visible(imp.buttons.first_child().is_some());
        imp.dismiss.set_visible(dismiss.is_some());
        if let Some((title, _)) = &dismiss {
            imp.dismiss.set_tooltip_text(Some(title));
            imp.dismiss
                .update_property(&[gtk::accessible::Property::Label(title)]);
        }
        imp.dismisses.replace(dismiss.map(|(_, does)| does));
    }

    fn act(&self, does: &Does) {
        let Some(host) = self.imp().host.get().cloned() else {
            return;
        };
        match does {
            Does::Send { intent, confirm } => {
                let intent = intent.clone();
                question::asking_first(self, confirm.as_ref(), move || host.send(intent.clone()));
            }
            Does::Open(link) => {
                if let Some(window) = self.root().and_downcast::<gtk::Window>() {
                    host.open(link, &window);
                }
            }
        }
    }

    // Ticks each second while it is shown and its countdown runs.
    fn count(&self) {
        let imp = self.imp();
        if !self.say() || !self.is_mapped() {
            self.stop_counting();
        } else if imp.ticking.borrow().is_none() {
            let row = self.downgrade();
            let source = glib::timeout_add_seconds_local(1, move || {
                let Some(row) = row.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if row.say() {
                    glib::ControlFlow::Continue
                } else {
                    // Gone with this return, so it is not removed again.
                    row.imp().ticking.take();
                    glib::ControlFlow::Break
                }
            });
            imp.ticking.replace(Some(source));
        }
    }

    // Says what it says, with its countdown under it where it has one: whether that still runs.
    fn say(&self) -> bool {
        let imp = self.imp();
        let now = glib::DateTime::now_utc().map_or(0, |now| now.to_unix());
        let counting = imp.counting.borrow();
        let countdown =
            (counting.as_ref()).and_then(|notice| pitboard_model::countdown(notice, now));
        let said = imp.said.borrow();
        self.set_subtitle(&match countdown {
            Some(countdown) if said.is_empty() => countdown,
            Some(countdown) => format!("{said}\n{countdown}"),
            None => said.clone(),
        });
        (counting.as_ref())
            .and_then(|notice| notice.until)
            .is_some_and(|until| until > now)
    }

    fn stop_counting(&self) {
        if let Some(source) = self.imp().ticking.take() {
            source.remove();
        }
    }
}

/// Has `list` show a row of each notice `notices` holds, in its order, each kept while its
/// notice is shown and showing it again as it changes.
pub(super) fn bind(list: &gtk::ListBox, notices: &gio::ListStore, host: Rc<dyn Host>) {
    list.bind_model(Some(notices), move |row| {
        let row = row.downcast_ref::<Row>().expect("rows alone");
        let shown = NoticeRow::new(host.clone());
        shown.show_notice(&row.notice());
        let weak = shown.downgrade();
        row.connect_changed(move |row| {
            if let Some(shown) = weak.upgrade() {
                shown.show_notice(&row.notice());
            }
        });
        shown.upcast()
    });
}
