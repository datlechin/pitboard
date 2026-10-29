//! Open Link: a link dropped on the window or pasted in it is handed to the model, which says
//! what Open Link shows of it, and Open in Browser hands it to the browser, which in a fixture
//! is the stand-in opener.

use adw::prelude::*;
use atspi::Role;
use pitboard_linux::dialogs::open_link::OpenLink;
use pitboard_model::{Intent, PickerShown};

use super::harness::{App, Owned, Screen, WATCHER, eventually, owned, session};
use super::launch::read;
use super::views::{self, Viewed, button, every};

const LINK: &str = "https://claude.ai/chat/1f2e";

// What Open Link shows of a link waiting, as the model words it.
fn waiting(viewed: &Viewed) -> (String, String) {
    viewed.until("the link waiting", |shown| {
        shown.account_windows.picker.is_some()
    });
    match viewed
        .shown
        .snapshot()
        .account_windows
        .picker
        .clone()
        .map(|p| p.shown)
    {
        Some(PickerShown::NoWindows {
            title, link_text, ..
        }) => (title, link_text),
        other => panic!("{other:?}"),
    }
}

fn says(dialog: &OpenLink) -> (String, Vec<String>) {
    let status = every::<adw::StatusPage>(dialog).remove(0);
    let labels = (every::<gtk::Label>(dialog).iter())
        .filter(|label| label.is_visible())
        .map(|label| label.label().to_string())
        .collect();
    (status.title().to_string(), labels)
}

#[gtk::test]
fn a_dropped_link_opens_open_link() {
    let viewed = Viewed::of("oneTool");
    let controllers = viewed.window.observe_controllers();
    let target = (0..controllers.n_items())
        .filter_map(|i| controllers.item(i).and_downcast::<gtk::DropTarget>())
        .next()
        .expect("the window takes a drop");

    let taken: bool = target.emit_by_name(
        "drop",
        &[&glib::BoxedValue(LINK.to_value()), &10.0f64, &10.0f64],
    );

    assert!(taken);
    assert_eq!(viewed.sent(), [Intent::LinkArrived { text: LINK.into() }]);
    let (title, link) = waiting(&viewed);
    let dialog = viewed.dialog().downcast::<OpenLink>().expect("Open Link");
    let (shown, labels) = says(&dialog);
    assert_eq!(shown, title);
    assert!(labels.contains(&link), "{labels:?}");
}

#[gtk::test]
fn a_pasted_link_opens_open_link() {
    let viewed = Viewed::of("oneTool");
    viewed
        .window
        .activate_action("pitboard.open-link", None)
        .expect("the main menu's Open Link");
    let dialog = viewed.dialog().downcast::<OpenLink>().expect("Open Link");
    let empty = viewed.shown.snapshot().account_windows.no_link.clone();
    assert_eq!(says(&dialog).0, empty.title);
    dialog.clipboard().set_text(&format!(" {LINK}\n"));

    button(&dialog, &pitboard_model::gnome_words().paste_link).emit_clicked();

    let (title, link) = waiting(&viewed);
    assert_eq!(
        viewed.sent(),
        [Intent::LinkArrived {
            text: format!(" {LINK}\n")
        }]
    );
    let (shown, labels) = says(&dialog);
    assert_eq!(shown, title);
    assert!(labels.contains(&link), "{labels:?}");
}

// A link handed over as a second launch hands one, which opens Open Link, and its button
// pressed over AT-SPI: the fixture's stand-in opener writes what the browser would have opened.
#[test]
fn open_in_browser_hands_the_link_to_the_opener() {
    let shared = format!(
        "pitboard-debug://open?url={}",
        glib::Uri::escape_string(LINK, None, false)
    );
    let picker = views::shown_in(
        "oneTool",
        &[Intent::LinkArrived {
            text: shared.clone(),
        }],
        |shown| shown.account_windows.picker.is_some(),
    )
    .account_windows
    .picker
    .expect("the link waiting");
    let PickerShown::NoWindows { in_browser, .. } = picker.shown else {
        panic!("{:?}", picker.shown);
    };
    let bus = session();
    let _watcher = Owned::new(&bus, WATCHER);
    let app = App::launch("oneTool", &["--sign-in"]);
    owned(&bus);
    eventually("the accounts read", || read(&app).then_some(()));

    let second = app.again(&[&shared]);
    assert!(second.status.success(), "{second:?}");
    let screen = Screen::new();
    let open = screen.waits_for("Open in Browser", |seen| {
        seen.role == Role::Button && seen.name == in_browser && seen.showing
    });
    screen.press(&open, "click");

    eventually("the browser opens the link", || {
        let opened = app.stand_in("browser.jsonl");
        (opened == [serde_json::json!({ "open": LINK })]).then_some(())
    });
}
