//! Name, for the account a sign-in just added, Rename, and Clean Up, for the login Claude Code
//! left in a file, as an Adwaita dialog: Cancel at the start of its header, where the sheet has
//! one, and the button that saves at its end. Name and Rename take the name in an entry and say
//! why what is typed cannot be one as it is typed, as the model's `check_name` answers; the
//! button that saves is held back meanwhile. Each stays up until the model closes its sheet,
//! saying inside what went wrong, and nothing in it can be pressed while what it saves is being
//! saved. It cannot close by itself, so Escape is its Cancel.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pitboard_model::{FailureText, Front, Intent, NameCheck, Sheet, Snapshot, Tool};

use crate::window::Host;

// What the sheet asks for, which says what its button sends.
#[derive(Debug, Clone, Copy)]
enum Asks {
    // a name for an account of `tool`
    Name { tool: Tool },
    // to put away the login left in a file
    Stow,
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/sheet.ui")]
    pub struct SheetDialog {
        #[template_child]
        pub(super) cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) confirm: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) message: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) fields: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub(super) name: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub(super) problem: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) failure: TemplateChild<gtk::Label>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) sheet: OnceCell<Sheet>,
        pub(super) asks: OnceCell<Asks>,
        // the model's name, which the entry starts on once
        pub(super) filled: Cell<bool>,
        pub(super) saving: Cell<bool>,
        pub(super) failure_shown: RefCell<Option<FailureText>>,
        // the model has let it go, or it has told the model it closed
        pub(super) withdrawn: Cell<bool>,
        pub(super) told: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SheetDialog {
        const NAME: &'static str = "PitboardSheetDialog";
        type Type = super::SheetDialog;
        type ParentType = adw::Dialog;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SheetDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let words = pitboard_model::app_words(Front::Gnome);
            self.cancel.set_label(&words.window.cancel);
            self.name.set_title(&words.sheet.name);
            let dialog = self.obj();
            let weak = dialog.downgrade();
            self.cancel.connect_clicked(move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.dismiss();
                }
            });
            let weak = dialog.downgrade();
            self.confirm.connect_clicked(move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.confirmed();
                }
            });
            let weak = dialog.downgrade();
            self.name.connect_changed(move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.typed();
                }
            });
            let weak = dialog.downgrade();
            self.name.connect_entry_activated(move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.confirmed();
                }
            });
        }
    }

    impl WidgetImpl for SheetDialog {}

    impl AdwDialogImpl for SheetDialog {
        // Escape: its Cancel, but while what it saves is being saved.
        fn close_attempt(&self) {
            if !self.saving.get() {
                self.obj().dismiss();
            }
        }

        fn closed(&self) {
            if !self.withdrawn.get() {
                self.obj().tell();
            }
            self.parent_closed();
        }
    }
}

glib::wrapper! {
    pub struct SheetDialog(ObjectSubclass<imp::SheetDialog>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl SheetDialog {
    /// The dialog for `sheet`, a Name or a Rename sheet, which names an account of `tool`.
    pub fn naming(sheet: Sheet, tool: Tool, host: Rc<dyn Host>) -> SheetDialog {
        let dialog = SheetDialog::new(sheet, Asks::Name { tool }, host);
        dialog.imp().fields.set_visible(true);
        dialog
    }

    /// The dialog for the Clean Up sheet.
    pub fn stowing(host: Rc<dyn Host>) -> SheetDialog {
        SheetDialog::new(Sheet::Stow, Asks::Stow, host)
    }

    fn new(sheet: Sheet, asks: Asks, host: Rc<dyn Host>) -> SheetDialog {
        let dialog: SheetDialog = glib::Object::new();
        let imp = dialog.imp();
        // Name's account is added under the name it starts on, so its button is its one.
        imp.cancel.set_visible(!matches!(sheet, Sheet::Name { .. }));
        let _ = imp.host.set(host);
        let _ = imp.sheet.set(sheet);
        let _ = imp.asks.set(asks);
        dialog
    }

    /// The model's sheet it shows: another, even of the same kind, is put up afresh.
    pub fn sheet(&self) -> &Sheet {
        self.imp().sheet.get().expect("made with its sheet")
    }

    fn asks(&self) -> Asks {
        *self.imp().asks.get().expect("made with what it asks")
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
        let (title, message, confirm, saving) = match (self.asks(), &snapshot.sheet_text) {
            (Asks::Name { .. }, Some(text)) => {
                if !imp.filled.replace(true) {
                    imp.name.set_text(&text.name);
                }
                (
                    &text.title,
                    text.message.clone(),
                    &text.confirm,
                    text.saving,
                )
            }
            (Asks::Name { .. }, None) => return,
            (Asks::Stow, _) => {
                let Some(stow) = &snapshot.stow_text else {
                    return;
                };
                let said: Vec<&str> = std::iter::once(stow.message.as_str())
                    .chain(stow.looking.as_deref())
                    .collect();
                (&stow.title, said.join("\n\n"), &stow.confirm, stow.saving)
            }
        };
        self.set_title(title);
        imp.message.set_label(&message);
        imp.confirm.set_label(confirm);
        imp.saving.set(saving);
        imp.name.set_sensitive(!saving);
        imp.cancel.set_sensitive(!saving);
        self.show_failure(snapshot.sheet_failure.as_ref());
        self.enable(snapshot);
        super::fit_height(self.upcast_ref());
    }

    fn show_failure(&self, failure: Option<&FailureText>) {
        let imp = self.imp();
        imp.failure.set_visible(failure.is_some());
        if let Some(failure) = failure {
            imp.failure
                .set_label(&format!("{}\n{}", failure.title, failure.message));
            let fresh = imp.failure_shown.borrow().as_ref().map(|f| f.id) != Some(failure.id);
            if fresh {
                self.announce(
                    &format!("{} {}", failure.title, failure.message),
                    gtk::AccessibleAnnouncementPriority::High,
                );
            }
        }
        imp.failure_shown.replace(failure.cloned());
    }

    // The button that saves, held back where it cannot be pressed now.
    fn enable(&self, snapshot: &Snapshot) {
        let imp = self.imp();
        let can = match self.asks() {
            Asks::Name { tool } => self.check(tool).intent.is_some(),
            Asks::Stow => (snapshot.stow_text.as_ref()).is_some_and(|stow| stow.can_confirm),
        };
        imp.confirm.set_sensitive(can && !imp.saving.get());
        self.set_default_widget(Some(&*imp.confirm));
    }

    // Why what is typed cannot be a name, and the button that saves it held back meanwhile.
    fn typed(&self) {
        let Asks::Name { tool } = self.asks() else {
            return;
        };
        let imp = self.imp();
        let check = self.check(tool);
        imp.problem.set_visible(check.problem.is_some());
        imp.problem
            .set_label(check.problem.as_deref().unwrap_or_default());
        imp.confirm
            .set_sensitive(check.intent.is_some() && !imp.saving.get());
        super::fit_height(self.upcast_ref());
    }

    fn check(&self, tool: Tool) -> NameCheck {
        let typed = self.imp().name.text().to_string();
        pitboard_model::check_name(self.sheet().clone(), tool, typed)
    }

    fn confirmed(&self) {
        let imp = self.imp();
        if imp.withdrawn.get() || imp.saving.get() || !imp.confirm.is_sensitive() {
            return;
        }
        let intent = match self.asks() {
            Asks::Name { tool } => self.check(tool).intent,
            Asks::Stow => Some(Intent::Stow),
        };
        // What went wrong was about the name it was asked with.
        if let Some(intent) = intent {
            self.host().send(intent);
        }
    }

    // Closed by the person: what closing tells the model, once, and then it goes.
    fn dismiss(&self) {
        self.tell();
        self.force_close();
    }

    fn tell(&self) {
        if !self.imp().told.replace(true) {
            self.host().send(Intent::CloseSheet);
        }
    }
}
