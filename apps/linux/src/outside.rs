//! The app's only actions on the session bus: the `app.` actions GApplication exports at the
//! app's object path, each what somebody outside the window asks, with an id the model issued. A
//! notification's click and its buttons are `notice-clicked`; a search's, KRunner's or a Plasma
//! widget's switch is `switch-from-outside`, with the account's id in the widget feed; the
//! launcher's Add Account and Refresh are `add-account` and `refresh`. Everything the window
//! does is a widget's action, which no other process reaches.
//!
//! Any process in the session can activate them, so none does more than a click in the app
//! (ARCHITECTURE.md, The apps). A switch asked from outside is a request the model answers by its
//! number and asks no question for, and one it refuses is said in a notification, since nothing
//! outside waits for the answer.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use pitboard_model::{Front, Intent, Notifier, OutsideAnswer, OutsideAsk, PitboardModel, Sheet};

use crate::notices::Notices;
use crate::platform::notifications;

pub const NOTICE_CLICKED: &str = notifications::CLICKED;
pub const SWITCH_FROM_OUTSIDE: &str = "switch-from-outside";
pub const ADD_ACCOUNT: &str = "add-account";
pub const REFRESH: &str = "refresh";

/// Every action the app exports, and none else.
pub const ACTIONS: [&str; 4] = [NOTICE_CLICKED, SWITCH_FROM_OUTSIDE, ADD_ACCOUNT, REFRESH];

/// What the window's own Add Account sends, which opens the window on its sheet.
pub fn add_account_intent() -> Intent {
    Intent::PresentSheet {
        sheet: Sheet::Add { tool: None },
    }
}

pub struct Outside {
    model: Arc<PitboardModel>,
    notices: Notices,
    // the last request numbered, which numbers the next
    requests: Cell<u64>,
    // the requests asked and not answered yet
    asked: RefCell<BTreeSet<u64>>,
}

impl Outside {
    pub fn new(model: Arc<PitboardModel>, notices: Notices) -> Rc<Outside> {
        Rc::new(Outside {
            model,
            notices,
            requests: Cell::new(0),
            asked: RefCell::default(),
        })
    }

    /// The actions, for the app to add to its own.
    pub fn actions(self: &Rc<Self>) -> [gio::SimpleAction; 4] {
        let with_text = |name: &str, act: fn(&Outside, String)| {
            let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
            let outside = Rc::downgrade(self);
            action.connect_activate(move |_, text| {
                let (Some(outside), Some(text)) = (outside.upgrade(), text.and_then(|t| t.str()))
                else {
                    return;
                };
                act(&outside, text.to_owned());
            });
            action
        };
        let plain = |name: &str, act: fn(&Outside)| {
            let action = gio::SimpleAction::new(name, None);
            let outside = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(outside) = outside.upgrade() {
                    act(&outside);
                }
            });
            action
        };
        [
            with_text(NOTICE_CLICKED, Outside::notice_clicked),
            with_text(SWITCH_FROM_OUTSIDE, Outside::switch),
            plain(ADD_ACCOUNT, Outside::add_account),
            plain(REFRESH, Outside::refresh),
        ]
    }

    fn notice_clicked(&self, argument: String) {
        self.model.send(Intent::NotificationClicked { argument });
    }

    /// A switch to the account the widget feed calls `account`, as a request of its own.
    pub fn switch(&self, account: String) {
        let request = self.requests.get() + 1;
        self.requests.set(request);
        self.asked.borrow_mut().insert(request);
        self.model.send(Intent::Outside {
            request,
            ask: OutsideAsk::Switch { account },
        });
    }

    pub fn add_account(&self) {
        self.model.send(add_account_intent());
    }

    pub fn refresh(&self) {
        self.model.send(Intent::Refresh { asked: true });
    }

    /// Each time the model's answers change: a request of this app's that was refused is said
    /// once, the question its answer offers left unasked.
    pub fn answered(&self, answers: &[OutsideAnswer]) {
        for answer in answers {
            if !self.asked.borrow_mut().remove(&answer.request) || answer.done {
                continue;
            }
            // A notification the desktop refused is said nowhere else: the window says the
            // same of every notification, which Preferences shows.
            let _ = self.notices.post(pitboard_model::outside_refused(
                Front::Gnome,
                answer.clone(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Failed;
    use gio::prelude::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Posted(Mutex<Vec<notifications::Notice>>);

    impl notifications::Notifier for Posted {
        fn post(&self, notice: notifications::Notice) -> Result<(), Failed> {
            self.0.lock().unwrap().push(notice);
            Ok(())
        }

        fn withdraw(&self, _id: String) -> Result<(), Failed> {
            Ok(())
        }
    }

    fn answer(request: u64, done: bool, said: &str) -> OutsideAnswer {
        OutsideAnswer {
            request,
            done,
            said: said.into(),
            question: None,
        }
    }

    // A model over homes of its own, never started, which takes what it is sent and reads nothing.
    fn outside() -> (
        Rc<Outside>,
        Arc<Posted>,
        Arc<PitboardModel>,
        tempfile::TempDir,
    ) {
        let (model, scratch) = crate::testing::unstarted();
        let posted = Arc::new(Posted::default());
        let outside = Outside::new(Arc::clone(&model), Notices(posted.clone()));
        (outside, posted, model, scratch)
    }

    #[test]
    fn a_refused_request_of_the_apps_own_is_said_once_and_one_done_never() {
        let (outside, posted, model, _scratch) = outside();
        outside.switch("35c0d1c9".into());
        outside.switch("35c0d1c9".into());
        let refused = answer(1, false, "personal is already in use.");

        outside.answered(&[
            refused.clone(),
            answer(2, true, "Switched to personal."),
            answer(7, false, "Pitboard is being uninstalled."),
        ]);
        outside.answered(&[
            refused.clone(),
            answer(8, false, "Pitboard is being uninstalled."),
        ]);

        let expected =
            crate::notices::posted(pitboard_model::outside_refused(Front::Gnome, refused));
        assert_eq!(*posted.0.lock().unwrap(), [expected]);
        model.shutdown();
    }

    #[test]
    fn each_action_sends_what_a_click_in_the_app_sends() {
        let (outside, _posted, model, _scratch) = outside();
        let app = gio::SimpleActionGroup::new();
        for action in outside.actions() {
            app.add_action(&action);
        }
        let mut names = app.list_actions();
        names.sort();
        let mut expected = ACTIONS.map(glib::GString::from).to_vec();
        expected.sort();
        assert_eq!(names, expected);
        for name in ACTIONS {
            let takes_text = app.action_parameter_type(name).is_some();
            assert_eq!(
                takes_text,
                [NOTICE_CLICKED, SWITCH_FROM_OUTSIDE].contains(&name),
                "{name}"
            );
        }
        app.activate_action(SWITCH_FROM_OUTSIDE, Some(&"35c0d1c9".to_variant()));
        app.activate_action(SWITCH_FROM_OUTSIDE, Some(&"35c0d1c9".to_variant()));
        assert_eq!(
            outside.requests.get(),
            2,
            "each switch is a request of its own"
        );
        model.shutdown();
    }
}
