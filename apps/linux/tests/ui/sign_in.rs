//! Add Account and Sign In Again in their dialog, and Name and Rename in theirs: signing in with
//! the code the browser shows and naming the account added, Escape while signing in and on
//! Rename, and a failure said inside the sheet it happened in, in sight.

use adw::prelude::*;
use pitboard_linux::dialogs::name::SheetDialog;
use pitboard_linux::dialogs::sign_in::SignIn;
use pitboard_linux::window::AccountGroup;
use pitboard_model::{Intent, RowEnd};

use super::views::{Viewed, button, drawn, escape, every, in_sight};

/// The code a Claude Code sign-in in a fixture takes, as its browser page shows one.
const CODE: &str = "fixture-code#state";

fn listed(viewed: &Viewed) -> Vec<String> {
    (every::<AccountGroup>(&viewed.window).iter())
        .map(|group| group.title().to_string())
        .collect()
}

// Add Account's dialog, signing in to the tool it starts on.
fn signing_in(viewed: &Viewed) -> SignIn {
    viewed
        .window
        .activate_action("pitboard.add-account", None)
        .expect("the window's Add Account");
    let dialog = viewed.dialog().downcast::<SignIn>().expect("Add Account");
    let step = viewed
        .shown
        .snapshot()
        .sheet_text
        .clone()
        .expect("its words")
        .confirm;
    button(&dialog, &step).emit_clicked();
    viewed.until("the code asked for", |shown| {
        shown
            .signing_in
            .as_ref()
            .is_some_and(|running| running.wants_code)
    });
    dialog
}

fn code_field(dialog: &SignIn) -> adw::EntryRow {
    every::<adw::EntryRow>(dialog).remove(0)
}

#[gtk::test]
fn adding_an_account_signs_in_with_the_fixtures_code_and_names_it() {
    let viewed = Viewed::of("oneTool");
    let dialog = signing_in(&viewed);
    let code = code_field(&dialog);
    assert!(code.is_visible());

    code.set_text(CODE);
    button(
        &dialog,
        &pitboard_model::app_words(pitboard_model::Front::Gnome)
            .sheet
            .submit_code,
    )
    .emit_clicked();

    viewed.until("the account added and asked a name", |shown| {
        matches!(shown.sheet, Some(pitboard_model::Sheet::Name { .. }))
    });
    let alert = viewed.dialog().downcast::<SheetDialog>().expect("Name");
    let name = every::<adw::EntryRow>(&alert).remove(0);
    assert_eq!(
        name.text(),
        "travel",
        "the name dana@travel.example suggests"
    );
    // Listed once it is read back, which may come after the sheet that names it.
    viewed.until("the account listed", |shown| {
        (shown.sections.iter())
            .flat_map(|section| &section.accounts)
            .any(|item| item.title == "travel")
    });
    assert!(listed(&viewed).contains(&"travel".to_owned()));
    name.set_text("third");
    let done = viewed
        .shown
        .snapshot()
        .sheet_text
        .clone()
        .expect("its words")
        .confirm;
    button(&alert, &done).emit_clicked();
    viewed.until("named", |shown| {
        shown.sheet.is_none()
            && (shown.sections.iter())
                .flat_map(|section| &section.accounts)
                .any(|item| item.title == "third")
    });
    assert!(
        listed(&viewed).contains(&"third".to_owned()),
        "{:?}",
        listed(&viewed)
    );
    assert!(
        viewed.window.visible_dialog().is_none(),
        "the sheet stayed up"
    );
}

#[gtk::test]
fn escape_while_signing_in_cancels_it() {
    let viewed = Viewed::of("oneTool");
    let dialog = signing_in(&viewed);
    let cancel = viewed
        .shown
        .snapshot()
        .signing_in_text
        .clone()
        .expect("its words")
        .cancel;
    assert!(
        !dialog.can_close(),
        "Escape would close it without cancelling"
    );
    viewed.asked.sent.borrow_mut().clear();

    escape(&code_field(&dialog));

    viewed.until("cancelled", |shown| {
        shown.sheet.is_none() && shown.signing_in.is_none()
    });
    assert_eq!(viewed.sent(), [cancel.intent, Intent::CloseSheet]);
    assert!(
        !listed(&viewed).contains(&"travel".to_owned()),
        "added all the same"
    );
}

// Renamed to the name another account has, the dialog stays up and says why, and the name typed
// stays there to correct.
#[gtk::test]
fn a_failure_stays_in_the_dialog() {
    let viewed = Viewed::of("oneTool");
    let items = viewed.items();
    let (taken, other) = (
        items
            .iter()
            .find(|item| matches!(item.end, Some(RowEnd::InUse { .. }))),
        items
            .iter()
            .find(|item| matches!(item.end, Some(RowEnd::Button { .. }))),
    );
    let (taken, other) = (taken.expect("one in use"), other.expect("another"));
    let group = (every::<AccountGroup>(&viewed.window).into_iter())
        .find(|group| group.title() == other.title)
        .expect("its group");
    group
        .activate_action("account.rename", None)
        .expect("its menu's Rename");
    let alert = viewed.dialog().downcast::<SheetDialog>().expect("Rename");
    let name = every::<adw::EntryRow>(&alert).remove(0);

    name.set_text(&taken.title);
    let save = viewed
        .shown
        .snapshot()
        .sheet_text
        .clone()
        .expect("its words")
        .confirm;
    button(&alert, &save).emit_clicked();

    viewed.until("refused", |shown| shown.sheet_failure.is_some());
    drawn(&alert);
    let failure = viewed.shown.snapshot().sheet_failure.clone().expect("said");
    let said: Vec<String> = (every::<gtk::Label>(&alert).iter())
        .filter(|label| in_sight(*label))
        .map(|label| label.label().to_string())
        .collect();
    assert!(
        said.iter().any(|said| said.contains(&failure.title)),
        "{said:?}"
    );
    assert_eq!(
        viewed.window.visible_dialog().as_ref(),
        Some(alert.upcast_ref())
    );
    assert_eq!(
        name.text(),
        taken.title,
        "what was typed is there to correct"
    );
}

// Rename asks in a dialog as Add Account does: Cancel and Save in its header, and Escape is its
// Cancel.
#[gtk::test]
fn rename_asks_in_a_dialog_whose_header_holds_its_buttons_and_escape_is_cancel() {
    let viewed = Viewed::of("oneTool");
    let other = (viewed.items().into_iter())
        .find(|item| matches!(item.end, Some(RowEnd::Button { .. })))
        .expect("an account not in use");
    let group = (every::<AccountGroup>(&viewed.window).into_iter())
        .find(|group| group.title() == other.title)
        .expect("its group");
    group
        .activate_action("account.rename", None)
        .expect("its menu's Rename");
    let dialog = viewed.dialog();
    let save = (viewed.shown.snapshot().sheet_text.clone())
        .expect("its words")
        .confirm;
    let cancel = pitboard_model::app_words(pitboard_model::Front::Gnome)
        .window
        .cancel;

    let header = every::<adw::HeaderBar>(&dialog).remove(0);
    let _ = (button(&header, &cancel), button(&header, &save));
    viewed.sent_since();
    escape(&every::<adw::EntryRow>(&dialog).remove(0));

    viewed.until("closed", |shown| shown.sheet.is_none());
    assert_eq!(viewed.sent(), [Intent::CloseSheet]);
    assert!(viewed.window.visible_dialog().is_none());
}
