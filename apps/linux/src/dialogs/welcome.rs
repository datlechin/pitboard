//! The welcome, at the first launch, as a dialog over the window: what Pitboard does and where it
//! stays, each tool found with what adding its account takes, start at sign-in, and Continue.
//! What a tool's Add starts, a sheet, a question or a failure, comes up over it, since the model
//! asks for each over the welcome while it is shown and libadwaita puts each dialog over the
//! ones before. However it closes, it goes on to the accounts.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pitboard_model::{WelcomeShown, WelcomeStep, WelcomeTool};

use crate::config;
use crate::preferences::general;
use crate::window::Host;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/welcome.ui")]
    pub struct Welcome {
        #[template_child]
        pub(super) status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) lives: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) tools: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) start_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) start_at_sign_in: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) start_failed: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) done: TemplateChild<gtk::Button>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) shown: RefCell<Option<WelcomeShown>>,
        pub(super) tool_rows: RefCell<Vec<adw::ActionRow>>,
        // while what the model shows is drawn, so its switch sends nothing
        pub(super) drawing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Welcome {
        const NAME: &'static str = "PitboardWelcome";
        type Type = super::Welcome;
        type ParentType = adw::Dialog;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Welcome {
        fn constructed(&self) {
            self.parent_constructed();
            let welcome = self.obj();
            self.status.set_icon_name(Some(config::APP_ID));
            welcome.set_default_widget(Some(&*self.done));
            let weak = welcome.downgrade();
            self.done.connect_clicked(move |_| {
                if let Some(welcome) = weak.upgrade() {
                    welcome.close();
                }
            });
            let weak = welcome.downgrade();
            self.start_at_sign_in.connect_active_notify(move |row| {
                if let Some(welcome) = weak.upgrade()
                    && !welcome.imp().drawing.get()
                {
                    welcome.turned(row.is_active());
                }
            });
        }
    }

    impl WidgetImpl for Welcome {}

    impl AdwDialogImpl for Welcome {
        fn closed(&self) {
            if let (Some(host), Some(shown)) = (self.host.get(), self.shown.borrow().as_ref()) {
                host.send(shown.done.intent.clone());
            }
            self.parent_closed();
        }
    }
}

glib::wrapper! {
    pub struct Welcome(ObjectSubclass<imp::Welcome>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl Welcome {
    pub fn new(host: Rc<dyn Host>) -> Welcome {
        let welcome: Welcome = glib::Object::new();
        let imp = welcome.imp();
        imp.start_group
            .set_visible(host.start_at_sign_in().is_some());
        let _ = imp.host.set(host);
        welcome
    }

    fn host(&self) -> Rc<dyn Host> {
        Rc::clone(self.imp().host.get().expect("made with its host"))
    }

    /// Shows what the model's welcome says.
    pub fn show(&self, welcome: &WelcomeShown) {
        let imp = self.imp();
        imp.drawing.set(true);
        self.set_title(&welcome.title);
        imp.status.set_title(&welcome.title);
        let says = welcome
            .lines
            .first()
            .map(|line| glib::markup_escape_text(line));
        imp.status.set_description(says.as_deref());
        let lives = welcome.lines.get(1);
        imp.lives
            .set_label(lives.map(String::as_str).unwrap_or_default());
        imp.lives.set_visible(lives.is_some());
        let tools_changed =
            (imp.shown.borrow().as_ref()).is_none_or(|shown| shown.tools != welcome.tools);
        if tools_changed {
            self.list(&welcome.tools);
        }
        let login = &welcome.open_at_login;
        imp.start_at_sign_in.set_title(&login.title);
        imp.start_at_sign_in.set_subtitle(&login.note);
        imp.start_at_sign_in.set_active(login.on);
        imp.done.set_label(&welcome.done.title);
        imp.done.set_sensitive(welcome.done.enabled);
        imp.shown.replace(Some(welcome.clone()));
        imp.drawing.set(false);
        super::fit_height(self.upcast_ref());
    }

    // A row a tool, made again as the tools change: its name, the account it has, and its step.
    fn list(&self, tools: &[WelcomeTool]) {
        let imp = self.imp();
        for row in imp.tool_rows.take() {
            imp.tools.remove(&row);
        }
        let rows = tools
            .iter()
            .map(|tool| {
                let row = adw::ActionRow::builder()
                    .title(&tool.name)
                    .subtitle(tool.account.as_deref().unwrap_or_default())
                    .use_markup(false)
                    .build();
                row.add_suffix(&self.step(&tool.step));
                imp.tools.add(&row);
                row
            })
            .collect();
        imp.tool_rows.replace(rows);
        imp.tools.set_visible(!tools.is_empty());
    }

    fn step(&self, step: &WelcomeStep) -> gtk::Widget {
        let host = self.host();
        match step {
            WelcomeStep::Added { said } => gtk::Label::builder()
                .label(said)
                .css_classes(["dim-label"])
                .build()
                .upcast(),
            // Reads the tool's login only now, on the click.
            WelcomeStep::Add { choice } => {
                let button = gtk::Button::builder()
                    .label(&choice.title)
                    .valign(gtk::Align::Center)
                    .sensitive(choice.enabled)
                    .build();
                let intent = choice.intent.clone();
                button.connect_clicked(move |_| host.send(intent.clone()));
                button.upcast()
            }
            WelcomeStep::Install { link } => {
                let button = gtk::Button::builder()
                    .label(&link.title)
                    .valign(gtk::Align::Center)
                    .css_classes(["flat"])
                    .build();
                let link = link.link.clone();
                button.connect_clicked(move |button| {
                    if let Some(window) = button.root().and_downcast::<gtk::Window>() {
                        host.open(&link, &window);
                    }
                });
                button.upcast()
            }
        }
    }

    fn turned(&self, on: bool) {
        let imp = self.imp();
        let host = self.host();
        let failed = general::start_at_sign_in(&*host, on);
        imp.start_failed.set_label(
            failed
                .as_ref()
                .map(|failed| failed.0.as_str())
                .unwrap_or_default(),
        );
        imp.start_failed.set_visible(failed.is_some());
        // Where it could not change, the switch shows what the desktop still has.
        if let Some(start) = host.start_at_sign_in() {
            imp.drawing.set(true);
            imp.start_at_sign_in.set_active(start.is_on());
            imp.drawing.set(false);
        }
        super::fit_height(self.upcast_ref());
    }
}
