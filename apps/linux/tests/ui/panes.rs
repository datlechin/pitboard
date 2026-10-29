//! Diagnostics and Activity, pushed over the accounts: each reads what it shows again as it is
//! shown, Copy Report puts doctor's redacted report on the clipboard, and the activity lists
//! each change in its columns.

use adw::prelude::*;
use pitboard_linux::pages::{ActivityPage, DiagnosticsPage};
use pitboard_model::{Front, Intent, Pane};

use super::views::{PATIENCE, Viewed, every};

// Runs the main loop until `drawn` holds: a column view makes its rows' cells as it is laid out.
fn until_drawn(drawn: impl Fn() -> bool) {
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + PATIENCE;
    while !drawn() {
        assert!(
            std::time::Instant::now() < deadline,
            "not drawn within {PATIENCE:?}"
        );
        if !context.iteration(false) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

#[gtk::test]
fn diagnostics_copies_the_redacted_report() {
    let viewed = Viewed::of("oneTool");
    let emails: Vec<String> = (viewed.items().into_iter())
        .map(|item| item.email)
        .filter(|email| !email.is_empty())
        .collect();
    assert!(!emails.is_empty());

    viewed
        .window
        .activate_action("pitboard.diagnostics", None)
        .expect("the main menu's Diagnostics");
    assert_eq!(
        viewed.sent_since(),
        [Intent::PaneShown { pane: Pane::Checks }],
        "the checks made again as it shows"
    );
    viewed.until("the report", |shown| {
        shown.machine.checks.report.is_some() && !shown.machine.checks.checking
    });
    let page = every::<DiagnosticsPage>(&viewed.window).remove(0);
    let words = pitboard_model::app_words(Front::Gnome).window;
    let copy = (every::<gtk::Button>(&page).into_iter())
        .find(|button| button.tooltip_text().as_deref() == Some(words.copy_report_help.as_str()))
        .expect("Copy Report");
    assert!(copy.is_sensitive());

    copy.emit_clicked();

    let copied = glib::MainContext::default()
        .block_on(page.clipboard().read_text_future())
        .unwrap()
        .map(String::from);
    let report = viewed.shown.snapshot().machine.checks.report.clone();
    assert_eq!(copied, report);
    let copied = copied.unwrap();
    for email in &emails {
        assert!(!copied.contains(email.as_str()), "{email} in the report");
    }

    let check_again = (every::<gtk::Button>(&page).into_iter())
        .find(|button| button.tooltip_text().as_deref() == Some(words.check_again_help.as_str()))
        .expect("Check Again");
    check_again.emit_clicked();
    assert_eq!(
        viewed.sent_since(),
        [Intent::PaneShown { pane: Pane::Checks }]
    );
}

#[gtk::test]
fn activity_lists_each_change_in_its_columns() {
    let viewed = Viewed::of("oneTool");

    viewed
        .window
        .activate_action("pitboard.activity", None)
        .expect("the main menu's Activity");

    assert_eq!(
        viewed.sent_since(),
        [Intent::PaneShown {
            pane: Pane::Activity
        }],
        "the log read again as it shows"
    );
    viewed.until("the log read", |shown| {
        !shown.machine.activity.lines.is_empty()
    });
    let lines = viewed.shown.snapshot().machine.activity.lines.clone();
    let page = every::<ActivityPage>(&viewed.window).remove(0);
    let table = every::<gtk::ColumnView>(&page).remove(0);
    let words = pitboard_model::app_words(Front::Gnome).activity;
    let columns = table.columns();
    let titles: Vec<String> = (0..columns.n_items())
        .filter_map(|i| columns.item(i).and_downcast::<gtk::ColumnViewColumn>())
        .map(|column| column.title().unwrap_or_default().into())
        .collect();
    assert_eq!(
        titles,
        [
            words.date,
            words.change,
            words.account,
            words.result,
            words.by
        ]
    );
    assert_eq!(table.model().unwrap().n_items() as usize, lines.len());

    let said = || -> Vec<(String, bool)> {
        (every::<gtk::Label>(&table).into_iter())
            .filter(|label| label.is_mapped())
            .map(|label| (label.label().into(), label.has_css_class("dim-label")))
            .collect()
    };
    let newest = &lines[0];
    until_drawn(|| said().iter().any(|(text, _)| *text == newest.change));
    let said = said();
    for (cell, quiet) in [
        (&newest.date, false),
        (&newest.change, false),
        (&newest.account, false),
        (&newest.result, newest.done),
        (&newest.asked_by, true),
    ] {
        assert!(
            said.contains(&(cell.clone(), quiet)),
            "{cell:?} quiet {quiet}: {said:?}"
        );
    }
}
