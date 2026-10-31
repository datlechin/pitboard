//! Add Account and Sign In Again, as an Adwaita dialog: Cancel at the start of its header and
//! the step's button at its end. Before a sign-in it picks the tool and says what each needs;
//! while one is under way it says what it waits for, offers the sign-in page's address to copy
//! or open, and takes the code the browser shows; what went wrong is said in a row inside, where
//! what was picked or typed is still there. While a sign-in runs the dialog cannot close by
//! itself, so Escape is its Cancel where the model says so.
//!
//! The model runs one sign-in at a time, so the dialog shows the one running whichever sheet
//! started it, and the model closes the sheet once it is done.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pitboard_model::{
    FailureText, Front, Intent, Missing, NextStep, RunningSignIn, Sheet, SheetText, SheetTool,
    SigningInText, Snapshot, Tool,
};

use crate::window::Host;

// What the dialog shows of the model's sheet.
#[derive(Clone, PartialEq)]
enum Page {
    // the tool to sign in to, before a sign-in starts or while one is starting
    Form {
        text: SheetText,
        running: bool,
    },
    SigningIn {
        running: RunningSignIn,
        text: Box<SigningInText>,
    },
}

impl Page {
    fn of(snapshot: &Snapshot) -> Option<Page> {
        if let (Some(running), Some(text)) = (&snapshot.signing_in, &snapshot.signing_in_text) {
            return Some(Page::SigningIn {
                running: running.clone(),
                text: Box::new(text.clone()),
            });
        }
        snapshot.sheet_text.clone().map(|text| Page::Form {
            text,
            running: snapshot.signing_in.is_some(),
        })
    }

    // What closing it tells the model.
    fn closed(&self) -> Vec<Intent> {
        match self {
            Page::Form { .. } => vec![Intent::CloseSheet],
            Page::SigningIn { text, .. } => vec![text.cancel.intent.clone(), Intent::CloseSheet],
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/sign-in.ui")]
    pub struct SignIn {
        #[template_child]
        pub(super) cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) step: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) message: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) tools: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub(super) fields: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub(super) account: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) code: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub(super) note: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) refused: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) waiting: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) spinner: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) progress: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) steps: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) link: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) copy_link: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) open_link: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) missing_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub(super) missing: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) missing_link: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) failure_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub(super) failure: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) failure_next: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) undo: TemplateChild<gtk::Button>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) sheet: OnceCell<Sheet>,
        pub(super) page: RefCell<Option<Page>>,
        // the tool picked, which the model starts it on once and the person changes after
        pub(super) tool: Cell<Option<Tool>>,
        pub(super) shown_tools: RefCell<Vec<String>>,
        pub(super) failure_shown: RefCell<Option<FailureText>>,
        pub(super) announced: RefCell<Option<String>>,
        // the model has let it go, or it has told the model it closed
        pub(super) withdrawn: Cell<bool>,
        pub(super) told: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SignIn {
        const NAME: &'static str = "PitboardSignIn";
        type Type = super::SignIn;
        type ParentType = adw::Dialog;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SignIn {
        fn constructed(&self) {
            self.parent_constructed();
            let sheet = pitboard_model::app_words(Front::Gnome).sheet;
            self.copy_link.set_label(&sheet.copy_link);
            self.open_link.set_label(&sheet.open);
            self.code.set_title(&sheet.code);
            self.account.set_title(&sheet.account);
            let dialog = self.obj();
            let on = |button: &gtk::Button, act: fn(&super::SignIn)| {
                let dialog = dialog.downgrade();
                button.connect_clicked(move |_| {
                    if let Some(dialog) = dialog.upgrade() {
                        act(&dialog);
                    }
                });
            };
            on(&self.cancel, super::SignIn::dismiss);
            on(&self.step, super::SignIn::take_step);
            on(&self.copy_link, super::SignIn::copy_link);
            on(&self.open_link, super::SignIn::open_page);
            on(&self.missing_link, super::SignIn::open_missing);
            on(&self.failure_next, super::SignIn::take_next);
            on(&self.undo, super::SignIn::undo);
            let weak = dialog.downgrade();
            self.code.connect_changed(move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.enable();
                }
            });
            let weak = dialog.downgrade();
            self.code.connect_entry_activated(move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.take_step();
                }
            });
            let weak = dialog.downgrade();
            self.tools.connect_active_name_notify(move |tools| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.picked(tools.active_name().as_deref());
                }
            });
        }
    }

    impl WidgetImpl for SignIn {}

    impl AdwDialogImpl for SignIn {
        // Escape, while a sign-in under way keeps the dialog up: its Cancel, where the model
        // says Escape is that.
        fn close_attempt(&self) {
            let dialog = self.obj();
            if let Some(Page::SigningIn { text, .. }) = self.page.borrow().clone()
                && text.escape_cancels
                && text.cancel.enabled
            {
                dialog.dismiss();
            }
        }

        fn closed(&self) {
            let dialog = self.obj();
            if !self.withdrawn.get() {
                dialog.tell();
            }
            self.parent_closed();
        }
    }
}

glib::wrapper! {
    pub struct SignIn(ObjectSubclass<imp::SignIn>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl SignIn {
    /// The dialog for `sheet`, an Add Account or a Sign In Again sheet.
    pub fn new(sheet: Sheet, host: Rc<dyn Host>) -> SignIn {
        let dialog: SignIn = glib::Object::new();
        let imp = dialog.imp();
        let _ = imp.host.set(host);
        let _ = imp.sheet.set(sheet);
        dialog
    }

    /// The model's sheet it shows: another, even of the same kind, is put up afresh.
    pub fn sheet(&self) -> &Sheet {
        self.imp().sheet.get().expect("made with its sheet")
    }

    fn host(&self) -> Rc<dyn Host> {
        Rc::clone(self.imp().host.get().expect("made with its host"))
    }

    /// The model has let the sheet go: it closes, telling the model nothing.
    pub fn withdraw(&self) {
        self.imp().withdrawn.set(true);
        self.force_close();
    }

    /// Shows what `snapshot` says of the sheet. A sheet on its way out keeps what it showed.
    pub fn show(&self, snapshot: &Snapshot) {
        let imp = self.imp();
        let Some(page) = Page::of(snapshot) else {
            return;
        };
        let asked = matches!(&page, Page::SigningIn { running, .. } if running.wants_code)
            && !imp.code.is_visible();
        match &page {
            Page::Form { text, running } => self.show_form(text, *running),
            Page::SigningIn { running, text } => self.show_signing_in(running, text),
        }
        imp.page.replace(Some(page));
        self.show_failure(snapshot.sheet_failure.as_ref());
        self.enable();
        super::fit_height(self.upcast_ref());
        // Asked for once the tool wants it, with the code on the clipboard to paste.
        if asked {
            imp.code.grab_focus();
        }
    }

    fn show_form(&self, text: &SheetText, running: bool) {
        let imp = self.imp();
        if imp.tool.get().is_none() {
            imp.tool.set(Some(text.tool));
        }
        self.set_title(&text.title);
        let codes: Vec<String> = text.tools.iter().map(|tool| tool.code.clone()).collect();
        if *imp.shown_tools.borrow() != codes {
            imp.tools.remove_all();
            for tool in &text.tools {
                let toggle = adw::Toggle::builder()
                    .name(&tool.code)
                    .label(&tool.name)
                    .build();
                imp.tools.add(toggle);
            }
            imp.shown_tools.replace(codes);
        }
        if let Some(picked) = self.chosen(text) {
            imp.tools.set_active_name(Some(&picked.code));
        }
        imp.tools.set_visible(text.tools.len() > 1);
        imp.tools.set_sensitive(!text.saving && !running);
        imp.account.set_visible(text.account.is_some());
        imp.account
            .set_subtitle(text.account.as_deref().unwrap_or_default());
        imp.code.set_visible(false);
        imp.fields.set_visible(text.account.is_some());
        let chosen = self.chosen(text);
        imp.message
            .set_label(chosen.map_or(&text.message, |tool| &tool.message));
        self.show_missing(chosen.and_then(|tool| tool.missing.as_ref()));
        imp.waiting.set_visible(text.progress.is_some());
        imp.progress.set_visible(text.progress.is_some());
        imp.progress
            .set_label(text.progress.as_deref().unwrap_or_default());
        for hidden in [
            imp.note.upcast_ref::<gtk::Widget>(),
            imp.refused.upcast_ref(),
            imp.steps.upcast_ref(),
            imp.link.upcast_ref(),
            imp.undo.upcast_ref(),
        ] {
            hidden.set_visible(false);
        }
        imp.step
            .set_label(chosen.map_or(&text.confirm, |tool| &tool.confirm));
        imp.step.set_visible(true);
        imp.cancel
            .set_label(&pitboard_model::app_words(Front::Gnome).window.cancel);
        imp.cancel.set_sensitive(!text.saving);
        self.set_can_close(!text.saving);
    }

    fn show_signing_in(&self, running: &RunningSignIn, text: &SigningInText) {
        let imp = self.imp();
        let words = pitboard_model::app_words(Front::Gnome).sheet;
        self.set_title(&text.title);
        imp.message.set_label(&text.message);
        imp.tools.set_visible(false);
        imp.account.set_visible(false);
        self.show_missing(None);
        imp.waiting.set_visible(true);
        imp.progress.set_visible(false);
        // The message says what it waits for, so the spinner is named for a screen reader.
        imp.spinner
            .update_property(&[gtk::accessible::Property::Label(&text.waiting)]);
        while let Some(child) = imp.steps.first_child() {
            imp.steps.remove(&child);
        }
        for step in &text.steps {
            let said = gtk::Label::builder()
                .label(step)
                .wrap(true)
                .xalign(0.0)
                .build();
            imp.steps.append(&said);
        }
        imp.steps.set_visible(!text.steps.is_empty());
        imp.link.set_visible(text.offers_link);
        // Dimmed rather than left out until the tool prints the address, so the dialog does not
        // grow a moment after it opens.
        let page = running.url.is_some();
        imp.copy_link.set_sensitive(page);
        imp.open_link.set_sensitive(page);
        imp.open_link.set_tooltip_text(running.url.as_deref());
        imp.code.set_visible(running.wants_code);
        imp.fields.set_visible(running.wants_code);
        imp.note.set_visible(running.wants_code);
        imp.note.set_label(&text.code_note);
        imp.refused.set_visible(text.refused.is_some());
        imp.refused
            .set_label(text.refused.as_deref().unwrap_or_default());
        imp.step.set_visible(running.wants_code);
        imp.step.set_label(&words.submit_code);
        imp.undo.set_visible(text.undo.is_some());
        if let Some(undo) = &text.undo {
            imp.undo.set_label(&undo.title);
            imp.undo.set_sensitive(undo.enabled);
        }
        imp.cancel.set_label(&text.cancel.title);
        imp.cancel.set_sensitive(text.cancel.enabled);
        self.set_can_close(false);
        // What appears by itself is not read by a screen reader, so the dialog says it.
        if text.announce != *imp.announced.borrow()
            && let Some(said) = &text.announce
        {
            self.announce(said, gtk::AccessibleAnnouncementPriority::Medium);
        }
        imp.announced.replace(text.announce.clone());
    }

    // Where Pitboard did not find the tool picked: what it needs, and where to read how to get it.
    fn show_missing(&self, missing: Option<&Missing>) {
        let imp = self.imp();
        imp.missing_list.set_visible(missing.is_some());
        if let Some(missing) = missing {
            imp.missing.set_title(&missing.title);
            imp.missing.set_subtitle(&missing.detail);
            imp.missing_link.set_label(&missing.link_title);
        }
    }

    // A failure after another is said again, and the focus goes back to what it was about.
    fn show_failure(&self, failure: Option<&FailureText>) {
        let imp = self.imp();
        imp.failure_list.set_visible(failure.is_some());
        let fresh =
            failure.map(|failure| failure.id) != imp.failure_shown.borrow().as_ref().map(|f| f.id);
        imp.failure_shown.replace(failure.cloned());
        let Some(failure) = failure else {
            return;
        };
        imp.failure.set_title(&failure.title);
        let said: Vec<&str> = std::iter::once(failure.message.as_str())
            .chain(failure.warnings.iter().map(String::as_str))
            .collect();
        imp.failure.set_subtitle(&said.join("\n"));
        let (title, enabled, flat) = match &failure.next {
            NextStep::Act { choice } => (&choice.title, choice.enabled, false),
            NextStep::Learn { link } => (&link.title, true, true),
        };
        imp.failure_next.set_label(title);
        imp.failure_next.set_sensitive(enabled);
        if flat {
            imp.failure_next.add_css_class("flat");
        } else {
            imp.failure_next.remove_css_class("flat");
        }
        if fresh {
            self.announce(
                &format!("{} {}", failure.title, failure.message),
                gtk::AccessibleAnnouncementPriority::High,
            );
        }
    }

    // The tool picked, of those the sheet offers.
    fn chosen<'a>(&self, text: &'a SheetText) -> Option<&'a SheetTool> {
        let tool = self.imp().tool.get()?;
        text.tools.iter().find(|offered| offered.tool == tool)
    }

    fn picked(&self, code: Option<&str>) {
        let imp = self.imp();
        let Some(Page::Form { text, running }) = imp.page.borrow().clone() else {
            return;
        };
        let Some(picked) = text
            .tools
            .iter()
            .find(|tool| Some(tool.code.as_str()) == code)
        else {
            return;
        };
        if imp.tool.replace(Some(picked.tool)) == Some(picked.tool) {
            return;
        }
        // What went wrong was about the tool it was asked with.
        if imp.failure_shown.borrow().is_some() {
            self.host().send(Intent::DismissSheetFailure);
        }
        self.show_form(&text, running);
        self.enable();
        super::fit_height(self.upcast_ref());
    }

    // What the step's button sends, where it can be pressed now.
    fn step_intent(&self) -> Option<Intent> {
        let imp = self.imp();
        match imp.page.borrow().as_ref()? {
            Page::Form { text, running } => {
                let tool = imp.tool.get()?;
                let found = self.chosen(text).is_none_or(|tool| tool.missing.is_none());
                (!text.saving && !running && found)
                    .then(|| pitboard_model::check_name(self.sheet().clone(), tool, String::new()))
                    .and_then(|check| check.intent)
            }
            Page::SigningIn { running, .. } => {
                let code = imp.code.text();
                (running.wants_code && !code.is_empty())
                    .then(|| Intent::PasteCode { code: code.into() })
            }
        }
    }

    fn enable(&self) {
        let imp = self.imp();
        imp.step.set_sensitive(self.step_intent().is_some());
        self.set_default_widget(imp.step.is_visible().then_some(&*imp.step));
    }

    fn take_step(&self) {
        let Some(intent) = self.step_intent() else {
            return;
        };
        if matches!(intent, Intent::PasteCode { .. }) {
            self.imp().code.set_text("");
        }
        self.host().send(intent);
    }

    // Closed by the person: what closing tells the model, once, and then it goes.
    fn dismiss(&self) {
        self.tell();
        self.force_close();
    }

    fn tell(&self) {
        let imp = self.imp();
        if imp.told.replace(true) {
            return;
        }
        let closed = imp
            .page
            .borrow()
            .as_ref()
            .map_or_else(|| vec![Intent::CloseSheet], Page::closed);
        let host = self.host();
        for intent in closed {
            host.send(intent);
        }
    }

    fn page_url(&self) -> Option<String> {
        match self.imp().page.borrow().as_ref()? {
            Page::SigningIn { running, .. } => running.url.clone(),
            Page::Form { .. } => None,
        }
    }

    // As printed, for a browser's address bar.
    fn copy_link(&self) {
        if let Some(url) = self.page_url() {
            self.clipboard().set_text(&url);
        }
    }

    fn open_page(&self) {
        if let Some(url) = self.page_url() {
            self.open(&url);
        }
    }

    fn open_missing(&self) {
        let imp = self.imp();
        let link = match imp.page.borrow().as_ref() {
            Some(Page::Form { text, .. }) => self
                .chosen(text)
                .and_then(|tool| tool.missing.as_ref())
                .map(|missing| missing.link.clone()),
            _ => None,
        };
        if let Some(link) = link {
            self.open(&link);
        }
    }

    fn take_next(&self) {
        let next = self
            .imp()
            .failure_shown
            .borrow()
            .as_ref()
            .map(|f| f.next.clone());
        match next {
            Some(NextStep::Act { choice }) => self.host().send(choice.intent),
            Some(NextStep::Learn { link }) => self.open(&link.link),
            None => {}
        }
    }

    fn undo(&self) {
        if let Some(Page::SigningIn { text, .. }) = self.imp().page.borrow().as_ref()
            && let Some(undo) = &text.undo
        {
            self.host().send(undo.intent.clone());
        }
    }

    fn open(&self, link: &str) {
        if let Some(window) = self.root().and_downcast::<gtk::Window>() {
            self.host().open(link, &window);
        }
    }
}
