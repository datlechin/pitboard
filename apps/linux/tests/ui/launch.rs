//! How the program starts: in its fixture under the debug build's name, at sign-in, held where a
//! tray shows it and ended where none does, and as a second launch, which hands what it was given
//! to the first.

use super::harness::{
    App, HOMES, Owned, Screen, WATCHER, activate, eventually, has_owner, left_in, owned, session,
};
use super::words;
use pitboard_linux::config::{APP_ID, Build};

/// Whether the stand-in tray has shown the fixture's accounts, which it shows once the model has
/// read them.
pub(crate) fn read(app: &App) -> bool {
    app.stand_in("tray.jsonl").iter().any(|line| {
        let menu = line["show"]["menu"].as_array().cloned().unwrap_or_default();
        menu.iter().any(|entry| {
            (entry["item"]["id"].as_str()).is_some_and(|id| id.starts_with("account/"))
        })
    })
}

/// Closes the window by its header bar's own button, which quits where no tray keeps Pitboard.
pub(crate) fn close(screen: &Screen) {
    screen.window();
    let close = screen.waits_for("the header bar's close button", |seen| {
        seen.role == atspi::Role::Button && seen.name == "Close"
    });
    screen.press(&close, "click");
}

#[test]
fn a_fixture_launch_owns_the_devel_name_and_leaves_its_homes_as_its_world_made_them() {
    let bus = session();
    let mut app = App::launch("oneTool", &[]);

    owned(&bus);
    assert_eq!(APP_ID, Build::Debug.app_id());
    assert!(!has_owner(&bus, Build::Release.app_id()));
    let screen = Screen::new();
    let window = screen.window();
    assert_eq!(window.name, words::app().app_name);
    eventually("the accounts read", || read(&app).then_some(()));
    assert!(
        app.fixture().join("home/.pitboard/state.json").is_file(),
        "the world keeps its accounts in its own home"
    );
    close(&screen);

    assert!(app.ends("the app quits").success());
    for home in HOMES.iter().filter(|home| **home != "TMPDIR") {
        let left = left_in(&app.home(home), home);
        assert!(left.is_empty(), "{home} holds {left:?}");
    }
    let tmp: Vec<_> = std::fs::read_dir(app.home("TMPDIR"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(tmp, ["pitboard-fixture"]);
}

#[test]
fn a_second_launch_hands_its_link_to_the_first_and_quits() {
    let bus = session();
    let _watcher = Owned::new(&bus, WATCHER);
    let mut first = App::launch("oneTool", &["--sign-in"]);
    owned(&bus);
    eventually("the accounts read", || read(&first).then_some(()));
    let screen = Screen::new();
    assert!(screen.windows().is_empty());

    let second = first.again(&["pitboard-debug://open?url=https%3A%2F%2Fclaude.ai%2Fchat%2F1f2e"]);

    assert!(second.status.success(), "{second:?}");
    assert!(
        second.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    screen.window();
    assert!(first.ended().is_none(), "the first runs on");
}

#[test]
fn signing_in_with_a_tray_starts_held_with_no_window() {
    let bus = session();
    let _watcher = Owned::new(&bus, WATCHER);
    let mut app = App::launch("oneTool", &["--sign-in"]);

    owned(&bus);
    eventually("the accounts read", || read(&app).then_some(()));
    let screen = Screen::new();
    // GTK puts a program on the screen with its first window.
    assert!(
        screen.app().is_none(),
        "on the screen at sign-in: {}",
        screen.described()
    );
    assert!(app.ended().is_none(), "not held");

    activate(&bus, "add-account", None);
    screen.window();
}

// Nothing would show it, so it ends: the watch service is what runs with no tray.
#[test]
fn signing_in_with_no_tray_ends() {
    let bus = session();
    let mut app = App::launch("oneTool", &["--sign-in"]);

    owned(&bus);

    assert!(app.ends("the app quits").success());
    assert!(!has_owner(&bus, APP_ID));
}
