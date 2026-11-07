//! Preferences: each control sends what the person changed, including a reversal before the
//! model answers, and what the model shows sends nothing; Advanced reads again what a terminal can
//! change each time it is shown; and start at sign-in is the app's own to turn on, after which
//! the model is told what the desktop has.

use adw::prelude::*;
use pitboard_linux::platform::autostart::StartAtSignIn;
use pitboard_linux::platform::stand_in;
use pitboard_linux::preferences::Preferences;
use pitboard_model::{AtLimit, Front, Intent, Place, SettingsSection, Snapshot, SwitchOrder};

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
fn show_pace_is_independent_of_automation_and_sends_only_a_changed_value() {
    let viewed = Viewed::of("oneTool");
    let preferences = opened(&viewed);
    viewed.until("display preferences read", |shown| {
        shown.machine.pace.enabled
    });
    let mut snapshot = shown_in(&viewed);
    let switch = row::<adw::SwitchRow>(&preferences, &snapshot.machine.pace.title);
    assert_eq!(switch.is_active(), snapshot.machine.pace.on);
    assert_eq!(
        switch.subtitle().as_deref(),
        Some(snapshot.machine.pace.note.as_str())
    );
    viewed.sent_since();

    snapshot.machine.at_limit.enabled = false;
    snapshot.machine.warm.enabled = false;
    snapshot.machine.pace.on = true;
    preferences.show(&snapshot);
    assert!(
        switch.is_sensitive(),
        "display remains available without automation"
    );
    assert!(switch.is_active());
    assert!(viewed.sent_since().is_empty(), "drawing sends nothing");
    switch.set_active(true);
    assert!(viewed.sent_since().is_empty());
    switch.set_active(false);
    assert_eq!(viewed.sent_since(), [Intent::SetShowPace { on: false }]);

    snapshot.machine.pace.enabled = false;
    preferences.show(&snapshot);
    assert!(!switch.is_sensitive());
    switch.set_active(false);
    assert!(
        viewed.sent_since().is_empty(),
        "unread preferences send nothing"
    );
}

#[gtk::test]
fn a_setting_sends_only_a_value_that_differs() {
    let viewed = Viewed::of("oneTool");
    let preferences = opened(&viewed);
    viewed.until("the preferences read", |shown| {
        shown.machine.at_limit.enabled && shown.machine.warm.enabled
    });
    let machine = shown_in(&viewed).machine;
    let (heading, share_label, order_label) = (
        machine.at_limit.heading.clone(),
        machine.at_limit.share_label.clone(),
        machine.at_limit.order_label.clone(),
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
    assert_eq!(viewed.sent_since(), [Intent::SetAtLimitShare { at: 91 }]);
    viewed.until("the share shown", |shown| shown.machine.at_limit.at == 91);
    combo.set_selected(index(AtLimit::Notify));
    assert_eq!(
        viewed.sent_since(),
        [Intent::SetAtLimitChoice {
            choice: AtLimit::Notify
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

    // Which account goes first, under the share.
    let order = row::<adw::ComboRow>(&preferences, &order_label);
    let orders = shown_in(&viewed).machine.at_limit.orders;
    let at = |wanted| {
        (orders.iter())
            .position(|listed| listed.order == wanted)
            .unwrap() as u32
    };
    assert_eq!(order.selected(), at(SwitchOrder::MostLeft));
    order.set_selected(at(SwitchOrder::MostLeft));
    assert_eq!(viewed.sent_since(), []);
    order.set_selected(at(SwitchOrder::ResetsSoonest));
    assert_eq!(
        viewed.sent_since(),
        [Intent::SetOrder {
            order: SwitchOrder::ResetsSoonest
        }]
    );
    viewed.until("the order shown", |shown| {
        shown.machine.at_limit.order == SwitchOrder::ResetsSoonest
    });
    assert_eq!(viewed.sent_since(), [], "the order shown sends nothing");
    combo.set_selected(index(AtLimit::Nothing));
    viewed.until("nothing at the share", |shown| {
        shown.machine.at_limit.choice == AtLimit::Nothing
    });
    assert!(!order.is_visible(), "hidden with the share");
}

#[gtk::test]
fn weekly_controls_preserve_the_other_period_and_remember_the_disabled_share() {
    let viewed = Viewed::of("oneTool");
    let preferences = opened(&viewed);
    viewed.until("the preferences read", |shown| {
        shown.machine.at_limit.enabled
    });
    viewed.tell(Intent::SetAtLimit {
        choice: AtLimit::Notify,
        at: 93,
    });
    viewed.tell(Intent::SetWeeklyLimit { on: true, at: 99 });
    viewed.until("independent shares shown", |shown| {
        shown.machine.at_limit.at == 93 && shown.machine.at_limit.weekly.at == 99
    });
    let shown = shown_in(&viewed).machine.at_limit;
    let share = row::<adw::SpinRow>(&preferences, &shown.share_label);
    let weekly = row::<adw::SwitchRow>(&preferences, &shown.weekly.title);
    let weekly_share = row::<adw::SpinRow>(&preferences, &shown.weekly.share_label);
    assert_eq!(
        weekly.subtitle().as_deref(),
        Some(shown.weekly.note.as_str())
    );
    assert_eq!(
        weekly_share.subtitle().as_deref(),
        Some(shown.weekly.range.as_str())
    );
    viewed.sent_since();

    share.set_value(94.0);
    assert_eq!(viewed.sent_since(), [Intent::SetAtLimitShare { at: 94 }]);
    viewed.until("short share changed", |shown| {
        shown.machine.at_limit.at == 94
    });
    assert_eq!(weekly_share.value(), 99.0);
    assert_eq!(shown_in(&viewed).machine.at_limit.weekly.at, 99);
    weekly_share.set_value(98.0);
    assert_eq!(
        viewed.sent_since(),
        [Intent::SetWeeklyLimitShare { at: 98 }]
    );
    viewed.until("weekly share changed", |shown| {
        shown.machine.at_limit.weekly.at == 98
    });
    assert_eq!(share.value(), 94.0);
    assert_eq!(shown_in(&viewed).machine.at_limit.at, 94);
    weekly_share.set_value(98.0);
    weekly.set_active(true);
    assert!(
        viewed.sent_since().is_empty(),
        "unchanged controls send nothing"
    );

    weekly.set_active(false);
    assert_eq!(
        viewed.sent_since(),
        [Intent::SetWeeklyLimitEnabled { on: false }]
    );
    viewed.until("weekly trigger disabled", |shown| {
        !shown.machine.at_limit.weekly.on
    });
    assert!(!weekly_share.is_sensitive());
    assert_eq!(weekly_share.value(), 98.0);
    assert!(weekly.is_sensitive());
    weekly.set_active(true);
    assert_eq!(
        viewed.sent_since(),
        [Intent::SetWeeklyLimitEnabled { on: true }]
    );
    viewed.until("weekly trigger enabled again", |shown| {
        shown.machine.at_limit.weekly.on
    });
    assert!(weekly_share.is_sensitive());
    assert_eq!(weekly_share.value(), 98.0);

    viewed.tell(Intent::SetAtLimit {
        choice: AtLimit::Nothing,
        at: 94,
    });
    viewed.until("nothing at either limit", |shown| {
        shown.machine.at_limit.choice == AtLimit::Nothing
    });
    assert!(!weekly.is_visible() && !weekly_share.is_visible());
    assert_eq!(weekly_share.value(), 98.0);
    viewed.tell(Intent::SetAtLimit {
        choice: AtLimit::Notify,
        at: 94,
    });
    viewed.until("limit controls restored", |shown| {
        shown.machine.at_limit.choice == AtLimit::Notify
    });
    assert!(weekly.is_visible() && weekly_share.is_visible());
    assert_eq!(weekly_share.value(), 98.0);
    assert_eq!(share.value(), 94.0);
    assert!(
        viewed.sent_since().is_empty(),
        "model changes are not sent back"
    );
}

#[gtk::test]
fn weekly_controls_send_nothing_while_drawing_or_before_preferences_are_read() {
    let viewed = Viewed::of("oneTool");
    let preferences = opened(&viewed);
    viewed.until("the preferences read", |shown| {
        shown.machine.at_limit.enabled
    });
    let mut snapshot = shown_in(&viewed);
    let weekly = row::<adw::SwitchRow>(&preferences, &snapshot.machine.at_limit.weekly.title);
    let share = row::<adw::SpinRow>(&preferences, &snapshot.machine.at_limit.weekly.share_label);
    viewed.sent_since();

    snapshot.machine.at_limit.weekly.on = false;
    snapshot.machine.at_limit.weekly.at = 99;
    snapshot.machine.at_limit.weekly.share_enabled = false;
    preferences.show(&snapshot);
    assert_eq!(share.value(), 99.0);
    assert!(!weekly.is_active() && !share.is_sensitive());
    assert!(viewed.sent_since().is_empty(), "drawing sends nothing");
    share.set_value(98.0);
    assert!(
        viewed.sent_since().is_empty(),
        "a disabled number sends nothing"
    );

    snapshot.machine.at_limit.enabled = false;
    snapshot.machine.at_limit.weekly.enabled = false;
    preferences.show(&snapshot);
    assert!(!weekly.is_sensitive() && !share.is_sensitive());
    weekly.set_active(true);
    share.set_value(97.0);
    assert!(
        viewed.sent_since().is_empty(),
        "unread preferences send nothing"
    );
}

#[gtk::test]
fn limit_controls_send_rapid_reversals_before_any_snapshot_returns() {
    let viewed = Viewed::of("oneTool");
    let preferences = opened(&viewed);
    viewed.until("the preferences read", |shown| {
        shown.machine.at_limit.enabled
    });
    let shown = shown_in(&viewed).machine.at_limit;
    let choice = row::<adw::ComboRow>(&preferences, &shown.heading);
    let share = row::<adw::SpinRow>(&preferences, &shown.share_label);
    let weekly = row::<adw::SwitchRow>(&preferences, &shown.weekly.title);
    let weekly_share = row::<adw::SpinRow>(&preferences, &shown.weekly.share_label);
    let index = |choice| {
        shown
            .choices
            .iter()
            .position(|listed| listed.choice == choice)
            .unwrap() as u32
    };
    viewed.sent_since();

    // Do not iterate the main context between events: listener snapshots remain queued.
    weekly_share.set_value(99.0);
    weekly.set_active(false);
    weekly.set_active(true);
    weekly_share.set_value(f64::from(shown.weekly.at));
    share.set_value(80.0);
    choice.set_selected(index(AtLimit::Nothing));
    share.set_value(f64::from(shown.at));
    choice.set_selected(index(shown.choice));
    assert_eq!(
        viewed.sent_since(),
        [
            Intent::SetWeeklyLimitShare { at: 99 },
            Intent::SetWeeklyLimitEnabled { on: false },
            Intent::SetWeeklyLimitEnabled { on: true },
            Intent::SetWeeklyLimitShare {
                at: shown.weekly.at
            },
            Intent::SetAtLimitShare { at: 80 },
            Intent::SetAtLimitChoice {
                choice: AtLimit::Nothing
            },
            Intent::SetAtLimitShare { at: shown.at },
            Intent::SetAtLimitChoice {
                choice: shown.choice
            },
        ]
    );
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
