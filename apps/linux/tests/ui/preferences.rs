//! Preferences: each control sends what the person set only where it differs from what the model
//! shows, and what the model shows sends nothing; Advanced reads again what a terminal can
//! change each time it is shown; and start at sign-in is the app's own to turn on, after which
//! the model is told what the desktop has.

use adw::prelude::*;
use pitboard_linux::platform::autostart::StartAtSignIn;
use pitboard_linux::platform::stand_in;
use pitboard_linux::preferences::Preferences;
use pitboard_model::{AtLimit, Front, Intent, Place, SettingsSection, Snapshot};

use super::views::{Viewed, row};

fn opened(viewed: &Viewed) -> Preferences {
    viewed
        .window
        .activate_action("pitboard.preferences", None)
        .expect("the main menu's Preferences");
    viewed
        .dialog()
        .downcast::<Preferences>()
        .expect("Preferences")
}

fn shown_in(viewed: &Viewed) -> Snapshot {
    viewed.shown.snapshot().clone()
}

fn reads_again(sent: &[Intent]) -> bool {
    sent.contains(&Intent::ReadSchedule) && sent.contains(&Intent::LookForCommandLine)
}

#[gtk::test]
fn a_setting_sends_only_a_value_that_differs() {
    let viewed = Viewed::of("oneTool");
    let preferences = opened(&viewed);
    viewed.until("the preferences read", |shown| {
        shown.machine.at_limit.enabled && shown.machine.warm.enabled
    });
    let machine = shown_in(&viewed).machine;
    let (heading, share_label) = (
        machine.at_limit.heading.clone(),
        machine.at_limit.share_label.clone(),
    );
    let words = pitboard_model::app_words(Front::Gnome).settings;
    viewed.sent_since();

    // What the model comes to show is drawn, and sends nothing.
    let (from, to) = (7 * 60, 21 * 60);
    viewed.tell(Intent::SetAtLimit {
        choice: AtLimit::Switch,
        at: 90,
    });
    viewed.tell(Intent::SetWarmRange { from, to });
    viewed.until("the model's values shown", |shown| {
        let machine = &shown.machine;
        machine.at_limit.choice == AtLimit::Switch
            && machine.at_limit.at == 90
            && (machine.warm.from, machine.warm.to) == (from, to)
    });
    let combo = row::<adw::ComboRow>(&preferences, &heading);
    let share = row::<adw::SpinRow>(&preferences, &share_label);
    let (warm_from, warm_to) = (
        row::<adw::SpinRow>(&preferences, &words.from),
        row::<adw::SpinRow>(&preferences, &words.to),
    );
    let choices = shown_in(&viewed).machine.at_limit.choices;
    let index = |choice| {
        (choices.iter())
            .position(|listed| listed.choice == choice)
            .unwrap() as u32
    };
    assert_eq!(combo.selected(), index(AtLimit::Switch));
    assert_eq!(share.value(), 90.0);
    assert_eq!((warm_from.value(), warm_to.value()), (420.0, 1260.0));
    assert_eq!(viewed.sent_since(), [], "the model's values sent back");

    // Set to what it shows, a control sends nothing; set to another value, that value alone.
    share.set_value(90.0);
    combo.set_selected(index(AtLimit::Switch));
    assert_eq!(viewed.sent_since(), []);
    share.set_value(91.0);
    assert_eq!(
        viewed.sent_since(),
        [Intent::SetAtLimit {
            choice: AtLimit::Switch,
            at: 91
        }]
    );
    viewed.until("the share shown", |shown| shown.machine.at_limit.at == 91);
    combo.set_selected(index(AtLimit::Notify));
    assert_eq!(
        viewed.sent_since(),
        [Intent::SetAtLimit {
            choice: AtLimit::Notify,
            at: 91
        }]
    );
    viewed.until("the choice shown", |shown| {
        shown.machine.at_limit.choice == AtLimit::Notify
    });
    warm_from.set_value(warm_from.value() + 15.0);
    assert_eq!(
        viewed.sent_since(),
        [Intent::SetWarmRange {
            from: from + 15,
            to
        }]
    );
    viewed.until("the hours shown", |shown| {
        shown.machine.warm.from == from + 15
    });
    assert_eq!(viewed.sent_since(), [], "only what was set is sent");
}

#[gtk::test]
fn the_schedule_command_line_and_watch_service_are_read_on_each_visit() {
    let viewed = Viewed::of("oneTool");
    let preferences = opened(&viewed);
    assert!(
        !reads_again(&viewed.sent_since()),
        "General shows nothing a terminal changes"
    );

    preferences.set_visible_page_name("advanced");
    assert!(reads_again(&viewed.sent_since()));
    viewed.until("the command line found", |shown| {
        shown.machine.command_line.in_terminal.is_some()
    });
    let shown = shown_in(&viewed);
    let settings = pitboard_model::app_words(Front::Gnome).settings;
    let path = row::<adw::ActionRow>(&preferences, &settings.path);
    assert_eq!(
        Some(path.subtitle().unwrap_or_default().to_string()),
        shown.machine.command_line.in_terminal
    );
    let watching = shown
        .machine
        .watch_service
        .expect("offered where no status item keeps Pitboard running");
    let watch = row::<adw::SwitchRow>(&preferences, &pitboard_model::gnome_words().watch_service);
    assert!(watch.is_visible() && watch.parent().is_some_and(|list| list.is_visible()));
    assert_eq!(watch.is_active(), watching.on);
    assert_eq!(
        watch.subtitle().unwrap_or_default().as_str(),
        watching.about
    );

    preferences.set_visible_page_name("general");
    preferences.set_visible_page_name("advanced");
    assert!(reads_again(&viewed.sent_since()), "each visit reads again");

    preferences.close();
    viewed.until("closed", |_| viewed.window.visible_dialog().is_none());
    viewed.tell(Intent::Show {
        place: Some(Place::Settings {
            section: SettingsSection::Advanced,
        }),
    });
    let preferences = viewed
        .dialog()
        .downcast::<Preferences>()
        .expect("Preferences, on the model's request");
    assert_eq!(preferences.visible_page_name().as_deref(), Some("advanced"));
    assert!(reads_again(&viewed.sent_since()));
}

#[gtk::test]
fn start_at_sign_in_writes_the_entry_and_tells_the_model() {
    let viewed = Viewed::of("oneTool");
    let preferences = opened(&viewed);
    let title = pitboard_model::app_words(Front::Gnome)
        .settings
        .open_at_login;
    let switch = row::<adw::SwitchRow>(&preferences, &title);
    assert!(!switch.is_active());
    viewed.sent_since();

    switch.set_active(true);

    assert_eq!(
        viewed.started_at_sign_in(),
        [serde_json::json!({ "on": true })]
    );
    assert_eq!(viewed.sent_since(), [Intent::OpensAtLogin { on: true }]);

    switch.set_active(false);

    assert_eq!(
        viewed.started_at_sign_in(),
        [
            serde_json::json!({ "on": true }),
            serde_json::json!({ "on": false })
        ]
    );
    assert_eq!(viewed.sent_since(), [Intent::OpensAtLogin { on: false }]);

    // Turned on elsewhere, as the welcome does.
    preferences.close();
    viewed.until("closed", |_| viewed.window.visible_dialog().is_none());
    stand_in::Autostart::new(&viewed.folder).turn_on().unwrap();
    let preferences = opened(&viewed);
    assert!(
        row::<adw::SwitchRow>(&preferences, &title).is_active(),
        "read from the desktop as it opens"
    );
    assert_eq!(viewed.sent_since(), [], "what it read is not sent back");
}
