//! Limits: what Pitboard does once a limit of the account in use reaches a share, the share, and
//! whether Anthropic's and OpenAI's terms allow it; then the warm-up's hours and each Claude Code
//! account's switch.
//!
//! libadwaita 1.7 has no time picker, so each of the warm-up's hours is a spin row over the
//! minutes of the day, a quarter of an hour a step, which says its time as the person's clock
//! does and takes no typing: it is turned, as its arrows and keys turn it. Midnight ends a day
//! as a clock shows it, 00:00, which the model reads as the day's end.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pitboard_model::{AtLimit, Front, Intent, MachineShown};

use super::{Preferences, escaped, say};

const DAY: u16 = 24 * 60;

pub(super) fn follow(preferences: &Preferences) {
    let imp = preferences.imp();
    let words = pitboard_model::app_words(Front::Gnome).settings;
    imp.warm_from.set_title(&words.from);
    imp.warm_to.set_title(&words.to);
    let weak = preferences.downgrade();
    imp.at_limit.connect_selected_notify(move |_| {
        if let Some(preferences) = weak.upgrade() {
            chosen(&preferences);
        }
    });
    imp.share.connect_output(|row| {
        row.set_text(&format!("{}%", row.value()));
        true
    });
    imp.share.connect_input(|row| {
        let typed = row.text();
        let typed = typed.trim().trim_end_matches('%').trim_end();
        Some(typed.parse::<f64>().map_err(|_| ()))
    });
    let weak = preferences.downgrade();
    imp.share.connect_value_notify(move |_| {
        if let Some(preferences) = weak.upgrade() {
            chosen(&preferences);
        }
    });
    let weak = preferences.downgrade();
    imp.terms.connect_activated(move |row| {
        let (Some(preferences), Some(window)) =
            (weak.upgrade(), row.root().and_downcast::<gtk::Window>())
        else {
            return;
        };
        if let Some(shown) = preferences.imp().shown.borrow().as_ref() {
            preferences
                .host()
                .open(&shown.machine.at_limit.terms.link, &window);
        }
    });
    let clock = preferences.host().local_time();
    for hour in [&imp.warm_from, &imp.warm_to] {
        let clock = clock.clone();
        hour.connect_output(move |row| {
            let minutes = row.value() as u16 % DAY;
            // A clock that cannot say the time leaves the minutes said as they are.
            if let Ok(said) = clock.time_of_day(minutes) {
                row.set_text(&said);
            }
            true
        });
        // Nothing is typed in it, so what it says is its own value.
        hour.connect_input(|row| Some(Ok(row.value())));
        let weak = preferences.downgrade();
        hour.connect_value_notify(move |_| {
            if let Some(preferences) = weak.upgrade() {
                ranged(&preferences);
            }
        });
    }
}

pub(super) fn show(preferences: &Preferences, machine: &MachineShown) {
    let imp = preferences.imp();
    let at_limit = &machine.at_limit;
    imp.at_limit_group
        .set_title(&glib::markup_escape_text(&at_limit.title));
    imp.at_limit_group
        .set_description(escaped(at_limit.note.as_deref()).as_deref());
    imp.at_limit_group.set_sensitive(at_limit.enabled);
    imp.at_limit.set_title(&at_limit.heading);
    let titles: Vec<&str> = (at_limit.choices.iter())
        .map(|choice| choice.title.as_str())
        .collect();
    let listed = imp
        .at_limit
        .model()
        .and_downcast::<gtk::StringList>()
        .is_some_and(|list| {
            (0..list.n_items())
                .map(|i| list.string(i).unwrap_or_default())
                .eq(titles.iter().copied())
        });
    if !listed {
        imp.at_limit.set_model(Some(&gtk::StringList::new(&titles)));
    }
    let chosen = (at_limit.choices.iter()).position(|choice| choice.choice == at_limit.choice);
    imp.at_limit
        .set_selected(chosen.map_or(gtk::INVALID_LIST_POSITION, |i| i as u32));
    imp.at_limit
        .set_subtitle(at_limit.standing.as_deref().unwrap_or_default());
    imp.share.set_title(&at_limit.share_label);
    imp.share.set_subtitle(&at_limit.share_range);
    imp.share
        .set_range(f64::from(at_limit.lowest), f64::from(at_limit.highest));
    imp.share.set_value(f64::from(at_limit.at));
    imp.share
        .upcast_ref::<gtk::Widget>()
        .update_property(&[gtk::accessible::Property::Label(&at_limit.at_label)]);
    imp.share.set_visible(at_limit.choice != AtLimit::Nothing);
    imp.share.set_sensitive(at_limit.share_enabled);
    imp.terms.set_title(&at_limit.terms.link_title);
    imp.terms.set_subtitle(&at_limit.terms.sentence);

    let warm = &machine.warm;
    imp.warm_group
        .set_title(&glib::markup_escape_text(&warm.title));
    imp.warm_group
        .set_description(Some(&glib::markup_escape_text(&warm.note)));
    imp.warm_group.set_sensitive(warm.enabled);
    imp.warm_from.set_value(f64::from(warm.from % DAY));
    imp.warm_to.set_value(f64::from(warm.to % DAY));
    imp.none_to_warm
        .set_title(warm.none_enrolled.as_deref().unwrap_or_default());
    imp.none_to_warm.set_visible(warm.none_enrolled.is_some());
    accounts(preferences, machine);
    say(&imp.warm_standing, &[warm.standing.as_deref()]);
}

// A switch for each account, made again only where the accounts listed change.
fn accounts(preferences: &Preferences, machine: &MachineShown) {
    let imp = preferences.imp();
    let accounts = &machine.warm.accounts;
    let listed = imp
        .warm_rows
        .borrow()
        .iter()
        .map(|(qualified, _)| qualified.as_str())
        .eq(accounts.iter().map(|account| account.qualified.as_str()));
    if !listed {
        for (_, row) in imp.warm_rows.take() {
            imp.warm_group.remove(&row);
        }
        let rows = accounts
            .iter()
            .map(|account| {
                let row = adw::SwitchRow::builder().use_markup(false).build();
                let (weak, qualified) = (preferences.downgrade(), account.qualified.clone());
                row.connect_active_notify(move |row| {
                    if let Some(preferences) = weak.upgrade() {
                        warmed(&preferences, &qualified, row.is_active());
                    }
                });
                imp.warm_group.add(&row);
                (account.qualified.clone(), row)
            })
            .collect();
        imp.warm_rows.replace(rows);
    }
    for ((_, row), account) in imp.warm_rows.borrow().iter().zip(accounts) {
        row.set_title(&account.title);
        row.set_subtitle(account.standing.as_deref().unwrap_or_default());
        row.set_active(account.on);
    }
}

// What happens at a limit, or its share, chosen by the person.
fn chosen(preferences: &Preferences) {
    let Some(machine) = preferences.machine() else {
        return;
    };
    let imp = preferences.imp();
    let at_limit = &machine.at_limit;
    let Some(choice) =
        (at_limit.choices.get(imp.at_limit.selected() as usize)).map(|choice| choice.choice)
    else {
        return;
    };
    let at = imp.share.value() as u8;
    if (choice, at) != (at_limit.choice, at_limit.at) {
        preferences.host().send(Intent::SetAtLimit { choice, at });
    }
}

fn ranged(preferences: &Preferences) {
    let Some(machine) = preferences.machine() else {
        return;
    };
    let imp = preferences.imp();
    let (from, to) = (imp.warm_from.value() as u16, imp.warm_to.value() as u16);
    let warm = &machine.warm;
    if (from, to) != (warm.from % DAY, warm.to % DAY) {
        preferences.host().send(Intent::SetWarmRange { from, to });
    }
}

fn warmed(preferences: &Preferences, qualified: &str, on: bool) {
    let Some(machine) = preferences.machine() else {
        return;
    };
    let account = (machine.warm.accounts.iter()).find(|account| account.qualified == qualified);
    if let Some(account) = account
        && account.on != on
    {
        preferences.host().send(Intent::SetWarm {
            account: account.account.clone(),
            on,
        });
    }
}
