//! The sheets that ask in an Adwaita alert: Name, for the account a sign-in just added, Rename,
//! and Clean Up, for the login Claude Code left in a file. Name and Rename take the name in an
//! entry under the message and say why what is typed cannot be one as it is typed, as the
//! model's `check_name` answers; the button that saves is held back meanwhile. Each stays up
//! until the model closes its sheet, saying inside what went wrong, and nothing in it can be
//! pressed while what it saves is being saved.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use pitboard_model::{FailureText, Front, Intent, Sheet, Snapshot};

use super::question::{CANCEL, CONFIRM};
use crate::window::Host;

pub struct SheetAlert {
    pub alert: adw::AlertDialog,
    sheet: Sheet,
    host: Rc<dyn Host>,
    // Name's and Rename's field, which the model's name starts in once
    entry: Option<adw::EntryRow>,
    problem: gtk::Label,
    failure: gtk::Label,
    filled: Cell<bool>,
    saving: Cell<bool>,
    failure_shown: RefCell<Option<FailureText>>,
    // a response came after the close attempt its button makes
    responded: Rc<Cell<bool>>,
    withdrawn: Cell<bool>,
}

impl SheetAlert {
    /// The alert for `sheet`, a Name, Rename or Clean Up sheet.
    pub fn new(sheet: Sheet, host: Rc<dyn Host>) -> Rc<SheetAlert> {
        let words = pitboard_model::app_words(Front::Gnome);
        let alert = adw::AlertDialog::new(None, None);
        let said = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let entry = matches!(sheet, Sheet::Name { .. } | Sheet::Rename { .. }).then(|| {
            let entry = adw::EntryRow::builder()
                .title(&words.sheet.name)
                .use_markup(false)
                .build();
            let fields = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .css_classes(["boxed-list"])
                .build();
            fields.append(&entry);
            said.append(&fields);
            entry
        });
        let note = |classes: &[&str]| {
            let label = gtk::Label::builder()
                .wrap(true)
                .xalign(0.0)
                .visible(false)
                .css_classes(classes.to_vec())
                .build();
            said.append(&label);
            label
        };
        let (problem, failure) = (note(&["caption", "error"]), note(&["error"]));
        alert.set_extra_child(Some(&said));
        // Name's account is added under the name it starts on, so Done is its one button.
        if !matches!(sheet, Sheet::Name { .. }) {
            alert.add_response(CANCEL, &words.window.cancel);
        }
        alert.add_response(CONFIRM, "");
        alert.set_response_appearance(CONFIRM, adw::ResponseAppearance::Suggested);
        alert.set_default_response(Some(CONFIRM));
        alert.set_close_response(CANCEL);
        // The model closes the sheet once it is done.
        alert.set_can_close(false);
        let shown = Rc::new(SheetAlert {
            alert,
            sheet,
            host,
            entry,
            problem,
            failure,
            filled: Cell::new(false),
            saving: Cell::new(false),
            failure_shown: RefCell::default(),
            responded: Rc::default(),
            withdrawn: Cell::new(false),
        });
        shown.follow();
        shown
    }

    fn follow(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.alert.connect_response(None, move |_, response| {
            if let Some(shown) = weak.upgrade() {
                shown.responded.set(true);
                shown.answered(response);
            }
        });
        // A response's button tries to close the alert before it says which it was
        // (`button_clicked_cb`, src/adw-alert-dialog.c at libadwaita 1.7.0), so an attempt no
        // response follows is Escape, or the close button of the window it is in.
        let weak = Rc::downgrade(self);
        self.alert.connect_close_attempt(move |_| {
            let weak = weak.clone();
            glib::idle_add_local_once(move || {
                if let Some(shown) = weak.upgrade()
                    && !shown.responded.replace(false)
                {
                    shown.answered(CANCEL);
                }
            });
        });
        if let Some(entry) = &self.entry {
            let weak = Rc::downgrade(self);
            entry.connect_changed(move |_| {
                if let Some(shown) = weak.upgrade() {
                    shown.typed();
                }
            });
            let weak = Rc::downgrade(self);
            entry.connect_entry_activated(move |_| {
                if let Some(shown) = weak.upgrade() {
                    shown.answered(CONFIRM);
                }
            });
        }
    }

    pub fn sheet(&self) -> &Sheet {
        &self.sheet
    }

    /// The model has let the sheet go: it closes, telling the model nothing.
    pub fn withdraw(&self) {
        self.withdrawn.set(true);
        self.alert.force_close();
    }

    /// Shows what `snapshot` says of the sheet. A sheet on its way out keeps what it showed.
    pub fn show(&self, snapshot: &Snapshot) {
        let alert = &self.alert;
        match (&self.entry, &snapshot.sheet_text, &snapshot.stow_text) {
            (Some(entry), Some(text), _) => {
                alert.set_heading(Some(&text.title));
                alert.set_body(&text.message);
                alert.set_response_label(CONFIRM, &text.confirm);
                if !self.filled.replace(true) {
                    entry.set_text(&text.name);
                }
                entry.set_sensitive(!text.saving);
                self.saving.set(text.saving);
            }
            (None, _, Some(stow)) => {
                alert.set_heading(Some(&stow.title));
                let said: Vec<&str> = std::iter::once(stow.message.as_str())
                    .chain(stow.looking.as_deref())
                    .collect();
                alert.set_body(&said.join("\n\n"));
                alert.set_response_label(CONFIRM, &stow.confirm);
                alert.set_response_enabled(CONFIRM, stow.can_confirm && !stow.saving);
                self.saving.set(stow.saving);
            }
            _ => return,
        }
        if alert.has_response(CANCEL) {
            alert.set_response_enabled(CANCEL, !self.saving.get());
        }
        self.show_failure(snapshot.sheet_failure.as_ref());
        self.typed();
    }

    fn show_failure(&self, failure: Option<&FailureText>) {
        self.failure.set_visible(failure.is_some());
        if let Some(failure) = failure {
            self.failure
                .set_label(&format!("{}\n{}", failure.title, failure.message));
            let fresh = self.failure_shown.borrow().as_ref().map(|f| f.id) != Some(failure.id);
            if fresh {
                // libadwaita's binding names no Accessible for its alert, which a widget is.
                (self.alert.upcast_ref::<gtk::Widget>()).announce(
                    &format!("{} {}", failure.title, failure.message),
                    gtk::AccessibleAnnouncementPriority::High,
                );
            }
        }
        self.failure_shown.replace(failure.cloned());
    }

    // Why what is typed cannot be a name, and the button that saves it held back meanwhile.
    fn typed(&self) {
        let Some(entry) = &self.entry else {
            return;
        };
        let check = self.check(&entry.text());
        self.problem.set_visible(check.problem.is_some());
        self.problem
            .set_label(check.problem.as_deref().unwrap_or_default());
        self.alert
            .set_response_enabled(CONFIRM, check.intent.is_some() && !self.saving.get());
    }

    fn check(&self, typed: &str) -> pitboard_model::NameCheck {
        let tool = match &self.sheet {
            Sheet::Name { account, .. } | Sheet::Rename { account } => account.tool,
            Sheet::SignInAgain { account } => account.tool,
            Sheet::Add { tool } => tool.unwrap_or(pitboard_model::Tool::Claude),
            Sheet::Stow => pitboard_model::Tool::Claude,
        };
        pitboard_model::check_name(self.sheet.clone(), tool, typed.to_owned())
    }

    fn answered(&self, response: &str) {
        if self.withdrawn.get() || self.saving.get() {
            return;
        }
        if response == CONFIRM {
            let intent = match &self.entry {
                Some(entry) => self.check(&entry.text()).intent,
                None => Some(Intent::Stow),
            };
            if let Some(intent) = intent {
                // What went wrong was about the name it was asked with.
                self.host.send(intent);
            }
        } else {
            self.withdrawn.set(true);
            self.host.send(Intent::CloseSheet);
            self.alert.force_close();
        }
    }
}
