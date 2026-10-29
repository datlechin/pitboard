//! The questions the model asks before what cannot be undone, as Adwaita alerts: Return answers
//! with the model's default, closing with Cancel, and Remove's button is drawn destructive.

use adw::prelude::*;
use pitboard_linux::dialogs::question::{CANCEL, CONFIRM};
use pitboard_linux::window::AccountGroup;
use pitboard_model::{ActionKind, Intent, RowEnd};

use super::views::{Viewed, button, every};

// Removing the account not in use, from its group's menu, which asks first.
fn removing(viewed: &Viewed) -> (adw::AlertDialog, pitboard_model::AccountAction) {
    let item = (viewed.items().into_iter())
        .find(|item| matches!(item.end, Some(RowEnd::Button { .. })))
        .expect("an account not in use");
    let remove = (item.actions.iter())
        .find(|action| action.kind == ActionKind::Remove)
        .expect("Remove")
        .clone();
    let title = glib::markup_escape_text(&item.title);
    let group = (every::<AccountGroup>(&viewed.window).into_iter())
        .find(|group| group.title() == title)
        .expect("its group");
    group
        .activate_action("account.remove", None)
        .expect("its menu's Remove");
    let alert = viewed
        .dialog()
        .downcast::<adw::AlertDialog>()
        .expect("an alert");
    (alert, remove)
}

#[gtk::test]
fn a_question_answers_with_its_default_and_close_responses() {
    let viewed = Viewed::of("oneTool");
    let (alert, remove) = removing(&viewed);
    let question = remove.confirm.clone().expect("Remove asks first");
    assert_eq!(alert.heading().as_deref(), Some(question.title.as_str()));
    assert_eq!(alert.body(), question.message);
    let default = if question.confirm_is_default {
        CONFIRM
    } else {
        CANCEL
    };
    assert_eq!(alert.default_response().as_deref(), Some(default));
    assert_eq!(alert.close_response(), CANCEL);

    // As Escape closes it.
    alert.close();
    viewed.until("closed", |_| viewed.window.visible_dialog().is_none());
    assert_eq!(viewed.sent(), [] as [Intent; 0], "closing it sends nothing");

    let (alert, remove) = removing(&viewed);
    // As Return answers it.
    alert
        .activate_action("default.activate", None)
        .expect("the alert's default");
    viewed.until("answered", |_| viewed.window.visible_dialog().is_none());
    let expected: Vec<Intent> = if question.confirm_is_default {
        remove.intent.into_iter().collect()
    } else {
        Vec::new()
    };
    assert_eq!(viewed.sent(), expected);
}

#[gtk::test]
fn remove_is_drawn_destructive() {
    let viewed = Viewed::of("oneTool");
    let (alert, remove) = removing(&viewed);
    let question = remove.confirm.expect("Remove asks first");
    assert!(question.destructive);

    assert_eq!(
        alert.response_appearance(CONFIRM),
        adw::ResponseAppearance::Destructive
    );
    let confirm = button(&alert, &question.confirm);
    assert!(confirm.has_css_class("destructive-action"));
    confirm.emit_clicked();
    viewed.until("answered", |_| viewed.window.visible_dialog().is_none());
    assert_eq!(viewed.sent(), remove.intent.into_iter().collect::<Vec<_>>());
}
