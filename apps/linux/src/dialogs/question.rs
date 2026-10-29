//! A question the model asks before something is done that cannot be undone, as an Adwaita
//! alert: its title, its message, its button and Cancel, which closing the alert answers too.
//! Return answers what the model says it does, and only Remove's button is drawn destructive.

use adw::prelude::*;
use pitboard_model::{Front, Question};

/// The response of the question's own button.
pub const CONFIRM: &str = "confirm";
/// The response of Cancel, and of closing the alert any other way.
pub const CANCEL: &str = "cancel";

/// The alert that asks `question`, telling `answered` whether its button was pressed, once.
pub fn alert(question: &Question, answered: impl Fn(bool) + 'static) -> adw::AlertDialog {
    let alert = adw::AlertDialog::new(Some(&question.title), Some(&question.message));
    alert.add_response(
        CANCEL,
        &pitboard_model::app_words(Front::Gnome).window.cancel,
    );
    alert.add_response(CONFIRM, &question.confirm);
    let appearance = if question.destructive {
        adw::ResponseAppearance::Destructive
    } else if question.confirm_is_default {
        adw::ResponseAppearance::Suggested
    } else {
        adw::ResponseAppearance::Default
    };
    alert.set_response_appearance(CONFIRM, appearance);
    let default = if question.confirm_is_default {
        CONFIRM
    } else {
        CANCEL
    };
    alert.set_default_response(Some(default));
    alert.set_close_response(CANCEL);
    alert.connect_response(None, move |_, response| answered(response == CONFIRM));
    alert
}

/// Asks `question` over `over`, and sends what `confirmed` gives once its button is pressed.
pub fn ask(over: &impl IsA<gtk::Widget>, question: &Question, confirmed: impl Fn() + 'static) {
    alert(question, move |yes| {
        if yes {
            confirmed();
        }
    })
    .present(Some(over));
}

/// Does `does` at once where nothing is asked first, and otherwise once `question` is answered
/// with its button.
pub fn asking_first(
    over: &impl IsA<gtk::Widget>,
    question: Option<&Question>,
    does: impl Fn() + 'static,
) {
    match question {
        Some(question) => ask(over, question, does),
        None => does(),
    }
}
