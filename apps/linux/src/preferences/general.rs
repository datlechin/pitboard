//! General: start at sign-in, which the app turns on and off itself, through the desktop's
//! autostart entry or a fixture's stand-in, and then tells the model what the desktop has; and
//! the version. Where no status item keeps Pitboard running, it says that closing the window
//! quits it.

use adw::prelude::*;
use adw::subclass::prelude::*;
use pitboard_model::{Front, Intent, Snapshot};

use super::{Preferences, escaped};
use crate::platform::Failed;
use crate::window::Host;

pub(super) fn follow(preferences: &Preferences) {
    let imp = preferences.imp();
    let words = pitboard_model::app_words(Front::Gnome).settings;
    imp.start_at_sign_in.set_title(&words.open_at_login);
    imp.start_at_sign_in.set_subtitle(&words.open_at_login_note);
    imp.version.set_title(&words.version);
    imp.version_number.set_label(env!("CARGO_PKG_VERSION"));
    let weak = preferences.downgrade();
    imp.start_at_sign_in.connect_active_notify(move |row| {
        if let Some(preferences) = weak.upgrade()
            && !preferences.imp().drawing.get()
        {
            turned(&preferences, row.is_active());
        }
    });
}

/// Reads whether the app starts at sign-in, as the desktop has it now.
pub(super) fn read(preferences: &Preferences) {
    let imp = preferences.imp();
    let host = preferences.host();
    let start = host.start_at_sign_in();
    imp.start_group.set_visible(start.is_some());
    if let Some(start) = start {
        imp.drawing.set(true);
        imp.start_at_sign_in.set_active(start.is_on());
        imp.drawing.set(false);
    }
}

pub(super) fn show(preferences: &Preferences, snapshot: &Snapshot) {
    let quits = snapshot
        .status_item
        .is_none()
        .then(|| pitboard_model::gnome_words().close_note);
    preferences
        .imp()
        .start_group
        .set_description(escaped(quits.as_deref()).as_deref());
}

fn turned(preferences: &Preferences, on: bool) {
    let imp = preferences.imp();
    let failed = start_at_sign_in(&*preferences.host(), on);
    imp.start_failed.set_label(
        failed
            .as_ref()
            .map(|failed| failed.0.as_str())
            .unwrap_or_default(),
    );
    imp.start_failed.set_visible(failed.is_some());
    // Where it could not change, the switch shows what the desktop still has.
    read(preferences);
}

/// Turns start at sign-in `on` where it is not, and tells the model what the desktop then has,
/// as Preferences and the welcome do: what failed, in the desktop's words.
pub(crate) fn start_at_sign_in(host: &dyn Host, on: bool) -> Option<Failed> {
    let start = host.start_at_sign_in()?;
    if start.is_on() == on {
        return None;
    }
    let failed = if on {
        start.turn_on().err()
    } else {
        start.turn_off().err()
    };
    host.send(Intent::OpensAtLogin { on: start.is_on() });
    failed
}
