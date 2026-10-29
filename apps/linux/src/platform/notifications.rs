//! The app's notifications, as GNotification posts them: to GNOME Shell's
//! `org.gtk.Notifications` where it runs, else to `org.freedesktop.Notifications`, the backend
//! GLib picks once for the process (docs/facts/linux-app.md, Notifications).
//!
//! A click on a notification or on one of its buttons activates `app.notice-clicked` with the
//! argument the notice gave it, the one action of the app's a notification reaches. GNOME
//! Shell asks it of the app over the bus, starting the app where it is not running; a
//! freedesktop server tells the running app alone.

use gio::prelude::*;
use serde::Serialize;

use super::Failed;

/// The app's action a click activates, with the notice's or the button's argument.
pub const CLICKED: &str = "notice-clicked";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    // the same for the same news, so a newer one replaces it
    pub id: String,
    pub title: String,
    pub body: String,
    // what a click on the notification hands back
    pub clicked: String,
    pub button: Option<Button>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Button {
    pub title: String,
    // what a click on it hands back
    pub clicked: String,
}

/// The desktop's notifications, or a fixture's stand-in. Asked on any thread: the model posts
/// on a thread of its own.
pub trait Notifier: Send + Sync {
    fn post(&self, notice: Notice) -> Result<(), Failed>;
}

/// The desktop's, through the app's GApplication, on the main context the app was made on,
/// whichever thread asks.
pub struct Desktop {
    app: glib::SendWeakRef<gio::Application>,
    context: glib::MainContext,
}

impl Desktop {
    pub fn new(app: &impl IsA<gio::Application>) -> Desktop {
        Desktop {
            app: app.upcast_ref::<gio::Application>().downgrade().into(),
            context: glib::MainContext::ref_thread_default(),
        }
    }

    // A task of the context's own, so it runs on the app's thread even where another thread
    // asks while nothing runs the context. Nothing once the app is gone.
    fn on_main(&self, act: impl FnOnce(&gio::Application) + Send + 'static) {
        let app = self.app.clone();
        self.context.spawn(async move {
            if let Some(app) = app.upgrade() {
                act(&app);
            }
        });
    }
}

impl Notifier for Desktop {
    fn post(&self, notice: Notice) -> Result<(), Failed> {
        self.on_main(move |app| app.send_notification(Some(&notice.id), &notification(&notice)));
        Ok(())
    }
}

// At GNotification's default priority, normal.
fn notification(notice: &Notice) -> gio::Notification {
    let notification = gio::Notification::new(&notice.title);
    notification.set_body(Some(&notice.body));
    let action = format!("app.{CLICKED}");
    notification.set_default_action_and_target_value(&action, Some(&notice.clicked.to_variant()));
    if let Some(button) = &notice.button {
        notification.add_button_with_target_value(
            &button.title,
            &action,
            Some(&button.clicked.to_variant()),
        );
    }
    notification
}
