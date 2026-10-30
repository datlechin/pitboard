//! What closing the window does, with a tray and without, and what a fixture writes as it runs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use adw::prelude::*;

use super::harness::{App, Owned, Screen, WATCHER, activate, eventually, owned, session};
use super::launch::{close, read};
use super::views::Viewed;
use pitboard_linux::presence::HELP;

#[test]
fn without_a_tray_closing_the_window_quits() {
    let bus = session();
    let mut app = App::launch("oneTool", &[]);
    owned(&bus);
    let screen = Screen::new();

    close(&screen);

    assert!(app.ends("the app quits").success());
}

// A watcher of the test's own owns the name, which is all a fixture's tray hears of one.
#[test]
fn with_one_closing_hides_it() {
    let bus = session();
    let _watcher = Owned::new(&bus, WATCHER);
    let mut app = App::launch("oneTool", &[]);
    owned(&bus);
    eventually("the status item worded", || {
        let shown = app.stand_in("tray.jsonl");
        let tip = shown.last()?["show"]["tool_tip"]["text"]
            .as_str()?
            .to_owned();
        (!tip.is_empty()).then_some(())
    });
    let screen = Screen::new();

    close(&screen);

    eventually("the window hidden", || {
        screen.windows().is_empty().then_some(())
    });
    std::thread::sleep(Duration::from_secs(1));
    assert!(app.ended().is_none(), "the tray keeps Pitboard running");
    activate(&bus, "add-account", None);
    screen.window();
}

// The compositor's close, which a Plasma logout sends each window once, reaches the window as
// `close()` does.
#[gtk::test]
fn with_one_closing_with_a_dialog_up_closes_the_dialog_and_hides_the_window() {
    let viewed = Viewed::of("oneTool");
    let window = &viewed.window;
    window
        .activate_action("pitboard.preferences", None)
        .expect("the main menu's Preferences");
    viewed.dialog();

    window.close();

    assert!(window.visible_dialog().is_none(), "Preferences up");
    assert!(!window.is_visible(), "the window up");
}

// What a tray kept running with its window hidden quits as the tray goes, as closing the window
// would have with none.
#[test]
fn the_tray_going_while_the_window_is_hidden_quits() {
    let bus = session();
    let watcher = Owned::new(&bus, WATCHER);
    let mut app = App::launch("oneTool", &[]);
    owned(&bus);
    eventually("the accounts read", || read(&app).then_some(()));
    let screen = Screen::new();
    close(&screen);
    eventually("the window hidden", || {
        screen.windows().is_empty().then_some(())
    });
    std::thread::sleep(Duration::from_secs(1));
    assert!(app.ended().is_none(), "the tray keeps Pitboard running");

    drop(watcher);

    assert!(app.ends("the app quits").success());
    assert!(
        app.fixture().join("linux/window.json").is_file(),
        "the window's size kept as it closed"
    );
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

#[test]
fn a_fixture_writes_nothing_outside_its_folder() {
    let bus = session();
    let mut app = App::launch("oneTool", &[]);
    owned(&bus);
    eventually("the accounts read", || read(&app).then_some(()));
    let screen = Screen::new();
    let menu = screen.main_menu();
    screen.press(&menu, "click");
    let help = screen.waits_for("Help in the main menu", |seen| {
        seen.role == atspi::Role::MenuItem && seen.name == super::words::app().menu.help
    });
    screen.press(&help, "click");
    activate(&bus, "switch-from-outside", Some("nobody".to_variant()));
    eventually("the refusal posted", || {
        (!app.stand_in("notifications.jsonl").is_empty()).then_some(())
    });
    close(&screen);
    assert!(app.ends("the app quits").success());

    let fixture = app.fixture();
    let mesa = app.home("XDG_CACHE_HOME").join("mesa_shader_cache");
    let outside: Vec<_> = files(app.scratch())
        .into_iter()
        .filter(|file| !file.starts_with(&fixture) && !file.starts_with(&mesa))
        .collect();
    assert!(
        outside.is_empty(),
        "written outside the fixture's folder: {outside:?}"
    );
    assert_eq!(
        app.stand_in("browser.jsonl"),
        [serde_json::json!({ "open": HELP })]
    );
    assert!(
        fixture.join("linux/window.json").is_file(),
        "the window's size, in the fixture"
    );
}
