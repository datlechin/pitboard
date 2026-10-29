//! Something asked for that did not happen, as an Adwaita alert in front of the window: what
//! was being done and why, and the button that puts it right where one does, which is then its
//! default, or the docs about it under what it says. Read either way, it is put away.

use std::rc::Rc;

use adw::prelude::*;
use pitboard_model::{AlertText, Front, Intent, NextStep};

use crate::window::Host;

/// The response that puts it right.
pub const ACT: &str = "act";
/// The response that only puts it away.
pub const CLOSE: &str = "close";

/// The alert saying `failure`, which tells `host` how it was answered while `live` says the
/// model still shows it.
pub fn alert(
    failure: &AlertText,
    host: Rc<dyn Host>,
    live: impl Fn() -> bool + 'static,
) -> adw::AlertDialog {
    let words = pitboard_model::app_words(Front::Gnome).window;
    let alert = adw::AlertDialog::new(Some(&failure.title), Some(&failure.message));
    let act = match &failure.next {
        Some(NextStep::Act { choice }) => Some(choice.clone()),
        _ => None,
    };
    // A failure's next step is its default button, beside which OK would read as taking it.
    match &act {
        Some(choice) => {
            alert.add_response(CLOSE, &words.close);
            alert.add_response(ACT, &choice.title);
            alert.set_response_enabled(ACT, choice.enabled);
            alert.set_response_appearance(ACT, adw::ResponseAppearance::Suggested);
            alert.set_default_response(Some(ACT));
        }
        None => {
            alert.add_response(CLOSE, &words.ok);
            alert.set_default_response(Some(CLOSE));
        }
    }
    alert.set_close_response(CLOSE);
    if let Some(NextStep::Learn { link }) = &failure.next {
        let learn = gtk::Button::builder()
            .label(&link.title)
            .halign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let (host, link) = (host.clone(), link.link.clone());
        learn.connect_clicked(move |button| {
            if let Some(window) = button.root().and_downcast::<gtk::Window>() {
                host.open(&link, &window);
            }
        });
        alert.set_extra_child(Some(&learn));
    }
    alert.connect_response(None, move |_, response| {
        if !live() {
            return;
        }
        host.send(Intent::DismissFailure);
        if let (ACT, Some(choice)) = (response, &act) {
            host.send(choice.intent.clone());
        }
    });
    alert
}
