//! Advanced: keeping saved sign-ins fresh, the watch service where no status item keeps
//! Pitboard running, the command line a terminal runs, and Uninstall.
//!
//! Uninstall asks the model's question, and the model removes Pitboard from this machine. Once
//! it has, the app takes back what is its own in the person's home, which no package manager
//! reaches: its start at sign-in and its data and cache folders. Then it says how the package
//! manager removes the app, which only a terminal can, and quits.

use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pitboard_model::{Front, Intent, PackageRemoval, RemovalShown, Snapshot};

use super::{Preferences, escaped, say};
use crate::config;
use crate::dialogs::question;
use crate::platform::leftovers;
use crate::window::Host;

/// The response of the alert once Pitboard is removed, which quits the app.
pub const QUIT: &str = "quit";

pub(super) fn follow(preferences: &Preferences) {
    let imp = preferences.imp();
    let words = pitboard_model::app_words(Front::Gnome).settings;
    imp.renew.set_title(&words.keep_saved_sign_ins_fresh);
    imp.watch
        .set_title(&pitboard_model::gnome_words().watch_service);
    imp.command_line_group
        .set_title(&glib::markup_escape_text(&words.command_line));
    imp.path.set_title(&words.path);
    imp.looking
        .upcast_ref::<gtk::Widget>()
        .update_property(&[gtk::accessible::Property::Label(
            &words.looking_for_command_line,
        )]);
    imp.uninstall_group
        .set_title(&glib::markup_escape_text(&words.uninstall));
    imp.uninstall_group
        .set_description(Some(&glib::markup_escape_text(&words.uninstall_note)));
    let weak = preferences.downgrade();
    imp.renew.connect_active_notify(move |row| {
        if let Some(preferences) = weak.upgrade()
            && let Some(machine) = preferences.machine()
            && row.is_active() != machine.schedule.on
        {
            preferences.host().send(Intent::SetSchedule {
                on: row.is_active(),
            });
        }
    });
    let weak = preferences.downgrade();
    imp.watch.connect_active_notify(move |row| {
        if let Some(preferences) = weak.upgrade()
            && let Some(watching) = preferences.machine().and_then(|m| m.watch_service)
            && row.is_active() != watching.on
        {
            preferences.host().send(Intent::SetWatchService {
                on: row.is_active(),
            });
        }
    });
    let weak = preferences.downgrade();
    imp.uninstall.connect_activated(move |row| {
        let Some(preferences) = weak.upgrade() else {
            return;
        };
        let Some(offer) =
            (preferences.imp().shown.borrow().as_ref()).map(|shown| shown.uninstall.clone())
        else {
            return;
        };
        let host = preferences.host();
        question::asking_first(row, offer.confirm.as_ref(), move || {
            host.send(offer.intent.clone());
        });
    });
    let weak = preferences.downgrade();
    imp.uninstall_again.connect_clicked(move |_| {
        let Some(preferences) = weak.upgrade() else {
            return;
        };
        let again = match preferences.imp().shown.borrow().as_ref() {
            Some(Snapshot {
                removal: Some(RemovalShown::Stopped { again, .. }),
                ..
            }) => again.intent.clone(),
            _ => return,
        };
        preferences.host().send(again);
    });
}

pub(super) fn show(preferences: &Preferences, snapshot: &Snapshot) {
    let imp = preferences.imp();
    let machine = &snapshot.machine;
    let schedule = &machine.schedule;
    imp.renew.set_subtitle(&schedule.about);
    imp.renew.set_active(schedule.on);
    imp.renew.set_sensitive(schedule.enabled);
    say(
        &imp.renew_standing,
        &[schedule.note.as_deref(), schedule.failed.as_deref()],
    );

    imp.watch_group.set_visible(machine.watch_service.is_some());
    if let Some(watching) = &machine.watch_service {
        imp.watch.set_subtitle(&watching.about);
        imp.watch.set_active(watching.on);
        imp.watch.set_sensitive(watching.enabled);
        say(
            &imp.watch_standing,
            &[watching.note.as_deref(), watching.failed.as_deref()],
        );
    }

    let command_line = &machine.command_line;
    imp.command_line_group
        .set_description(escaped(command_line.update_note.as_deref()).as_deref());
    imp.path
        .set_subtitle(command_line.in_terminal.as_deref().unwrap_or_default());
    imp.looking.set_visible(command_line.in_terminal.is_none());
    say(
        &imp.command_line_standing,
        &[
            command_line.cannot_link.as_deref(),
            command_line.terminal_note.as_deref(),
        ],
    );

    imp.uninstall.set_title(&snapshot.uninstall.title);
    imp.uninstall.set_sensitive(snapshot.uninstall.enabled);
    let (removal, removing, again) = (&imp.removal, &imp.removing, &imp.uninstall_again);
    match &snapshot.removal {
        Some(RemovalShown::Removing { title }) => {
            removal.set_title(title);
            removal.set_subtitle("");
            removing.set_visible(true);
            again.set_visible(false);
            removal.set_visible(true);
        }
        Some(RemovalShown::Stopped {
            title,
            line,
            again: choice,
        }) => {
            removal.set_title(title);
            removal.set_subtitle(line);
            removing.set_visible(false);
            again.set_label(&choice.title);
            again.set_sensitive(choice.enabled);
            again.set_visible(true);
            removal.set_visible(true);
        }
        Some(RemovalShown::Done) | None => removal.set_visible(false),
    }
}

/// Asks the model to read again what a terminal may have changed while the app ran.
pub(super) fn visited(host: &Rc<dyn Host>) {
    host.send(Intent::ReadSchedule);
    host.send(Intent::LookForCommandLine);
}

/// Once the model has removed Pitboard: takes back what is the app's own in the person's home,
/// and says, in an alert whose one button quits the app, what the package manager removes and
/// the command for it, where the build names its package.
pub fn take_back(host: Rc<dyn Host>) -> adw::AlertDialog {
    let removed = leftovers::remove(host.start_at_sign_in(), &host.own_folders());
    let stayed: Vec<String> = (removed.stayed.iter())
        .map(|(_, failed)| failed.0.clone())
        .collect();
    let removal = pitboard_model::package_removal(config::PACKAGE, &stayed);
    let alert = removal_alert(&removal);
    alert.connect_response(None, move |_, _| host.quit());
    alert
}

/// The alert saying `removal`, its command in a row beside a button that copies it.
pub fn removal_alert(removal: &PackageRemoval) -> adw::AlertDialog {
    let alert = adw::AlertDialog::new(Some(&removal.title), Some(&removal.message));
    let words = pitboard_model::app_words(Front::Gnome);
    let copy = pitboard_model::gnome_words().copy_command;
    if let Some(command) = &removal.command {
        let row = adw::ActionRow::builder()
            .title(command)
            .use_markup(false)
            .title_selectable(true)
            .css_classes(["monospace"])
            .build();
        let button = gtk::Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text(&copy)
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        button.update_property(&[gtk::accessible::Property::Label(&copy)]);
        let run = command.clone();
        button.connect_clicked(move |button| button.clipboard().set_text(&run));
        row.add_suffix(&button);
        let commands = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        commands.append(&row);
        alert.set_extra_child(Some(&commands));
    }
    alert.add_response(QUIT, &words.menu.quit);
    alert.set_default_response(Some(QUIT));
    alert.set_close_response(QUIT);
    alert
}
