//! What the app exports on the bus: its outside actions and nothing of its window's, each as the
//! launcher and a notification ask for it.

use std::collections::BTreeMap;

use glib::prelude::*;

use super::harness::{App, Screen, activate, described, eventually, exporting, owned, session};
use super::launch::read;
use super::words;
use pitboard_linux::config::OBJECT_PATH;
use pitboard_linux::outside;
use pitboard_model::{Front, OutsideAnswer};

#[test]
fn the_app_exports_only_its_outside_actions() {
    let bus = session();
    let _app = App::launch("oneTool", &[]);
    owned(&bus);
    let screen = Screen::new();
    screen.window();

    let exported = exporting(&bus, "org.gtk.Actions");

    let expected: BTreeMap<String, String> = [
        (outside::NOTICE_CLICKED, "s"),
        (outside::SWITCH_FROM_OUTSIDE, "s"),
        (outside::ADD_ACCOUNT, ""),
        (outside::REFRESH, ""),
    ]
    .into_iter()
    .map(|(name, parameter)| (name.to_owned(), parameter.to_owned()))
    .collect();
    let every: Vec<_> = (exported.iter())
        .map(|path| (path.clone(), described(&bus, path)))
        .collect();
    assert_eq!(every, [(OBJECT_PATH.to_owned(), expected)]);
}

#[test]
fn each_launcher_action_is_one_the_app_exports_and_says_its_words() {
    let bus = session();
    let app = App::launch("oneTool", &["--sign-in"]);
    owned(&bus);
    let exported = described(&bus, OBJECT_PATH);
    let menu = words::app().menu;
    let said = BTreeMap::from([
        (outside::ADD_ACCOUNT, menu.add_account),
        (outside::REFRESH, menu.refresh),
    ]);

    let actions = words::launcher_actions();

    assert_eq!(actions.len(), said.len());
    for (action, name) in &actions {
        assert_eq!(
            exported.get(action).map(String::as_str),
            Some(""),
            "{action}"
        );
        assert_eq!(Some(name), said.get(action.as_str()), "{action}");
    }
    // As a launcher that starts the program asks, and as GNOME Shell asks over the bus.
    let screen = Screen::new();
    let refreshed = app.again(&["--refresh"]);
    assert!(refreshed.status.success(), "{refreshed:?}");
    activate(&bus, outside::REFRESH, None);
    assert!(screen.windows().is_empty(), "Refresh opens no window");
    let added = app.again(&["--add-account"]);
    assert!(added.status.success(), "{added:?}");
    screen.window();
}

#[test]
fn a_switch_from_outside_that_is_refused_is_said_in_a_notification() {
    let bus = session();
    let app = App::launch("oneTool", &["--sign-in"]);
    owned(&bus);
    eventually("the accounts read", || read(&app).then_some(()));

    activate(
        &bus,
        outside::SWITCH_FROM_OUTSIDE,
        Some("nobody".to_variant()),
    );

    let posted = eventually("the refusal posted", || {
        app.stand_in("notifications.jsonl").into_iter().next()
    });
    let said = posted["post"].clone();
    let refused = pitboard_model::outside_refused(
        Front::Gnome,
        OutsideAnswer {
            request: 1,
            done: false,
            said: said["body"].as_str().unwrap().into(),
            question: None,
        },
    );
    assert_eq!(said["id"], refused.id);
    assert_eq!(said["title"], refused.title);
    assert_eq!(said["clicked"], refused.clicked);
    assert!(!refused.body.is_empty());
}

// GTK's popover menu, which an AT-SPI client opens and chooses from as a person would (M12).
#[test]
fn help_in_the_main_menu_opens_the_docs_in_the_browser() {
    let bus = session();
    let app = App::launch("oneTool", &[]);
    owned(&bus);
    let screen = Screen::new();
    let menu = screen.main_menu();

    screen.press(&menu, "click");
    let help = screen.waits_for("Help in the main menu", |seen| {
        seen.role == atspi::Role::MenuItem && seen.name == words::app().menu.help
    });
    screen.press(&help, "click");

    eventually("the browser opens the docs", || {
        let opened = app.stand_in("browser.jsonl");
        (opened == [serde_json::json!({ "open": pitboard_linux::presence::HELP })]).then_some(())
    });
}
