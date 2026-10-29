//! The first launch's welcome: a dialog over the window that comes once, as the model asks for
//! it, whose Add brings up the sheet it starts over it, and which goes on to the accounts.

use adw::prelude::*;
use pitboard_linux::dialogs::sign_in::SignIn;
use pitboard_linux::dialogs::welcome::Welcome;
use pitboard_model::{Intent, Place, Sheet, Tool, WelcomeStep};

use super::views::{Viewed, button, every};

#[gtk::test]
fn the_welcome_shows_once_and_adds_from_itself() {
    let viewed = Viewed::once("firstLaunch", |shown| {
        shown.window_request.place == Some(Place::Welcome) && !shown.welcome.tools.is_empty()
    });
    let welcome = viewed
        .dialog()
        .downcast::<Welcome>()
        .expect("the welcome, at the first launch");
    let shown = viewed.shown.snapshot().welcome.clone();
    let status = every::<adw::StatusPage>(&welcome).remove(0);
    assert_eq!(status.title(), shown.title);
    let claude = (shown.tools.iter())
        .find(|tool| tool.code == "claude")
        .expect("Claude Code found");
    let WelcomeStep::Add { choice } = &claude.step else {
        panic!("{:?}", claude.step);
    };
    assert_eq!(
        choice.intent,
        Intent::PresentSheet {
            sheet: Sheet::Add {
                tool: Some(Tool::Claude)
            }
        }
    );
    viewed.sent_since();

    button(&welcome, &choice.title).emit_clicked();

    assert_eq!(viewed.sent_since(), std::slice::from_ref(&choice.intent));
    viewed.until("the sheet up", |shown| shown.sheet.is_some());
    let sheet = viewed.dialog().downcast::<SignIn>().expect("Add Account");
    assert!(welcome.parent().is_some(), "the welcome stays under it");
    sheet.close();
    viewed.until("the sheet closed", |shown| shown.sheet.is_none());
    assert_eq!(
        viewed.dialog().downcast::<Welcome>().ok(),
        Some(welcome.clone()),
        "the welcome, once its sheet has gone"
    );
    viewed.sent_since();

    button(&welcome, &shown.done.title).emit_clicked();

    viewed.until("the accounts", |shown| {
        shown.window_request.place == Some(Place::Accounts)
    });
    assert_eq!(
        viewed.sent_since(),
        std::slice::from_ref(&shown.done.intent)
    );
    assert!(viewed.window.visible_dialog().is_none());
    let serial = viewed.shown.snapshot().window_request.serial;
    viewed.tell(Intent::Show { place: None });
    viewed.until("the window asked for again", |shown| {
        shown.window_request.serial > serial
    });
    assert!(
        viewed.window.visible_dialog().is_none(),
        "the welcome comes once"
    );
}
