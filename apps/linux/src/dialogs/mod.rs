//! The window's dialogs, each an Adwaita one, put up and taken down as the model says.
//!
//! The model keeps the sheet over the window: Add Account and Sign In Again are a dialog of
//! their own ([`sign_in`]), and Name, Rename and Clean Up ask in an alert ([`name`]). A sheet
//! the person closed stays down until the model has heard and moved on, and one the model lets
//! go closes telling it nothing. What the window asks by itself, quitting an app or replacing a
//! login, and a failure the model says, are alerts in front of everything ([`question`],
//! [`failure`]), each shown once and taken down when the model drops it. Open Link
//! ([`open_link`]) comes up as a link arrives, and as the window's menu asks for it.

pub mod failure;
pub mod name;
pub mod open_link;
pub mod question;
pub mod sign_in;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use pitboard_model::{AlertText, Part, Sheet, Snapshot, WindowQuestion};

use crate::shown::Shown;
use crate::window::Host;
use name::SheetAlert;
use open_link::OpenLink;
use sign_in::SignIn;

// The dialog up for the model's sheet.
enum Up {
    SignIn(SignIn),
    Alert(Rc<SheetAlert>),
}

impl Up {
    fn sheet(&self) -> &Sheet {
        match self {
            Up::SignIn(dialog) => dialog.sheet(),
            Up::Alert(alert) => alert.sheet(),
        }
    }

    fn dialog(&self) -> adw::Dialog {
        match self {
            Up::SignIn(dialog) => dialog.clone().upcast(),
            Up::Alert(alert) => alert.alert.clone().upcast(),
        }
    }

    fn show(&self, snapshot: &Snapshot) {
        match self {
            Up::SignIn(dialog) => dialog.show(snapshot),
            Up::Alert(alert) => alert.show(snapshot),
        }
    }

    fn withdraw(&self) {
        match self {
            Up::SignIn(dialog) => dialog.withdraw(),
            Up::Alert(alert) => alert.withdraw(),
        }
    }
}

// What the window asks by itself, or says went wrong.
#[derive(Clone, PartialEq)]
enum WindowAlert {
    Failed(AlertText),
    Asking(WindowQuestion),
}

impl WindowAlert {
    fn of(snapshot: &Snapshot) -> Option<WindowAlert> {
        let asked = snapshot
            .quit_question
            .as_ref()
            .or(snapshot.replace_question.as_ref());
        (snapshot.failure_alert.clone().map(WindowAlert::Failed))
            .or_else(|| asked.cloned().map(WindowAlert::Asking))
    }
}

// The alert up, what it says, and whether its answer still reaches the model.
struct AlertUp {
    alert: WindowAlert,
    dialog: adw::AlertDialog,
    live: Rc<Cell<bool>>,
}

pub struct Dialogs {
    over: glib::WeakRef<gtk::Widget>,
    host: Rc<dyn Host>,
    shown: Shown,
    sheet: RefCell<Option<Up>>,
    dismissed: RefCell<Option<Sheet>>,
    alert: RefCell<Option<AlertUp>>,
    open_link: RefCell<Option<OpenLink>>,
    // the link Open Link was closed on, which stays put away until the model has heard
    put_away: Cell<Option<u64>>,
}

impl Dialogs {
    /// The dialogs over `over`, following what `shown` shows.
    pub fn follow(over: &impl IsA<gtk::Widget>, shown: &Shown, host: Rc<dyn Host>) -> Rc<Dialogs> {
        let dialogs = Rc::new(Dialogs {
            over: over.upcast_ref::<gtk::Widget>().downgrade(),
            host,
            shown: shown.clone(),
            sheet: RefCell::default(),
            dismissed: RefCell::default(),
            alert: RefCell::default(),
            open_link: RefCell::default(),
            put_away: Cell::default(),
        });
        let on = |parts: &[Part], act: fn(&Rc<Dialogs>, &Snapshot)| {
            for part in parts {
                let weak = Rc::downgrade(&dialogs);
                shown.connect_changed(*part, move |shown| {
                    if let Some(dialogs) = weak.upgrade() {
                        let snapshot = shown.snapshot().clone();
                        act(&dialogs, &snapshot);
                    }
                });
            }
        };
        on(
            &[
                Part::Sheet,
                Part::SheetText,
                Part::SigningIn,
                Part::SigningInText,
                Part::StowText,
                Part::SheetFailure,
            ],
            Dialogs::follow_sheet,
        );
        on(
            &[
                Part::FailureAlert,
                Part::QuitQuestion,
                Part::ReplaceQuestion,
            ],
            Dialogs::follow_alert,
        );
        on(&[Part::AccountWindows], Dialogs::follow_link);
        let snapshot = shown.snapshot().clone();
        dialogs.follow_sheet(&snapshot);
        dialogs.follow_alert(&snapshot);
        dialogs.follow_link(&snapshot);
        dialogs
    }

    fn follow_sheet(self: &Rc<Self>, snapshot: &Snapshot) {
        let sheet = snapshot.sheet.clone();
        if *self.dismissed.borrow() != sheet {
            self.dismissed.replace(None);
        }
        if let Some(up) = self.sheet.borrow().as_ref()
            && Some(up.sheet()) == sheet.as_ref()
        {
            up.show(snapshot);
            return;
        }
        if let Some(up) = self.sheet.take() {
            up.withdraw();
        }
        let (Some(sheet), Some(over)) = (sheet, self.over.upgrade()) else {
            return;
        };
        if self.dismissed.borrow().as_ref() == Some(&sheet) {
            return;
        }
        let up = match sheet {
            Sheet::Add { .. } | Sheet::SignInAgain { .. } => {
                Up::SignIn(SignIn::new(sheet.clone(), self.host.clone()))
            }
            Sheet::Name { .. } | Sheet::Rename { .. } | Sheet::Stow => {
                Up::Alert(SheetAlert::new(sheet.clone(), self.host.clone()))
            }
        };
        up.show(snapshot);
        let dialog = up.dialog();
        let weak = Rc::downgrade(self);
        dialog.connect_closed(move |_| {
            if let Some(dialogs) = weak.upgrade() {
                dialogs.sheet_closed(&sheet);
            }
        });
        self.sheet.replace(Some(up));
        dialog.present(Some(&over));
    }

    // Closed by the person, a sheet stays down until the model has heard and moved on. One the
    // model let go was taken out of `sheet` first.
    fn sheet_closed(&self, sheet: &Sheet) {
        let closed = (self.sheet.borrow().as_ref()).is_some_and(|up| up.sheet() == sheet);
        if closed {
            self.sheet.take();
            self.dismissed.replace(Some(sheet.clone()));
        }
    }

    // Each is shown once: one answered stays answered until the model drops it.
    fn follow_alert(self: &Rc<Self>, snapshot: &Snapshot) {
        let alert = WindowAlert::of(snapshot);
        if self.alert.borrow().as_ref().map(|up| &up.alert) == alert.as_ref() {
            return;
        }
        if let Some(up) = self.alert.take() {
            up.live.set(false);
            if up.dialog.parent().is_some() {
                up.dialog.force_close();
            }
        }
        let (Some(alert), Some(over)) = (alert, self.over.upgrade()) else {
            return;
        };
        let live = Rc::new(Cell::new(true));
        let dialog = match &alert {
            WindowAlert::Failed(text) => {
                let live = live.clone();
                failure::alert(text, self.host.clone(), move || live.get())
            }
            WindowAlert::Asking(asked) => {
                let (live, host, asked) = (live.clone(), self.host.clone(), asked.clone());
                question::alert(&asked.question, move |yes| {
                    if live.get() {
                        host.send(if yes {
                            asked.confirmed.clone()
                        } else {
                            asked.declined.clone()
                        });
                    }
                })
            }
        };
        dialog.present(Some(&over));
        self.alert.replace(Some(AlertUp {
            alert,
            dialog,
            live,
        }));
    }

    // Open Link comes up as a link arrives, and shows the one waiting while it is up.
    fn follow_link(self: &Rc<Self>, snapshot: &Snapshot) {
        let shown = &snapshot.account_windows;
        if let Some(dialog) = self.open_link.borrow().as_ref() {
            dialog.show(shown);
            return;
        }
        let arrived = shown.picker.as_ref().map(|picker| picker.arrival);
        if arrived.is_some() && arrived != self.put_away.get() {
            self.show_open_link();
        }
    }

    /// Opens Open Link, where it is not open.
    pub fn show_open_link(self: &Rc<Self>) {
        let Some(over) = self.over.upgrade() else {
            return;
        };
        if self.open_link.borrow().is_some() {
            return;
        }
        let dialog = OpenLink::new(self.host.clone());
        dialog.show(&self.shown.snapshot().account_windows);
        let weak = Rc::downgrade(self);
        dialog.connect_closed(move |_| {
            if let Some(dialogs) = weak.upgrade() {
                dialogs.open_link.take();
                let waiting = dialogs.shown.snapshot().account_windows.picker.clone();
                dialogs.put_away.set(waiting.map(|picker| picker.arrival));
            }
        });
        dialog.present(Some(&over));
        self.open_link.replace(Some(dialog));
    }
}
