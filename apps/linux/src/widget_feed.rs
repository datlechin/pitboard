//! The widget feed on the bus, which a Plasma widget reads: `com.usepitboard.Pitboard.WidgetFeed`
//! at `<app's path>/WidgetFeed`, on the app's own connection (docs/facts/linux-app.md, The Plasma
//! widget). `Feed` is the model's widget feed, the glance every app's widgets read, and `Revision`
//! the widget revision it was taken at. `PropertiesChanged` carries both as the revision moves,
//! and only then, so a widget follows the feed with no poll.
//!
//! The interface has no method and neither property can be set: a widget reads here, and asks
//! for a switch through the app's `switch-from-outside` action alone ([`crate::outside`]). Before
//! the first read, and once Pitboard is removed, the model has no feed, and `Feed` is empty.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use gio::prelude::*;
use pitboard_model::{Part, PitboardModel};

use crate::resources;
use crate::shown::Shown;

pub const INTERFACE: &str = "com.usepitboard.Pitboard.WidgetFeed";

/// Where under the app's object path the feed is served.
pub const OBJECT: &str = "WidgetFeed";

pub fn path(app: &str) -> String {
    format!("{app}/{OBJECT}")
}

/// The feed on the bus, until dropped.
pub struct WidgetFeed {
    connection: gio::DBusConnection,
    path: String,
    registration: Option<gio::RegistrationId>,
    fed: Rc<RefCell<Fed>>,
}

// What the feed says, as of the revision it was taken at.
#[derive(Default)]
struct Fed {
    revision: u64,
    feed: String,
}

impl Fed {
    // `Feed` or `Revision`, the only names GDBus asks for.
    fn property(&self, name: &str) -> glib::Variant {
        match name {
            "Revision" => self.revision.to_variant(),
            _ => self.feed.to_variant(),
        }
    }
}

impl WidgetFeed {
    /// Serves the feed under `app`, the app's object path, empty until it is first shown.
    pub fn serve(connection: &gio::DBusConnection, app: &str) -> Result<WidgetFeed, glib::Error> {
        let fed = Rc::new(RefCell::new(Fed::default()));
        let reading = fed.clone();
        // GDBus answers only the properties the interface declares, and refuses every Set.
        let registration = connection
            .register_object(&path(app), &resources::interface(INTERFACE))
            .property(move |_, _, _, _, name| reading.borrow().property(name))
            .build()?;
        Ok(WidgetFeed {
            connection: connection.clone(),
            path: path(app),
            registration: Some(registration),
            fed,
        })
    }

    /// Shows `feed`, the model's at `revision`, none before the first read and once Pitboard is
    /// removed. A revision already shown says nothing, since a snapshot can name every part.
    pub fn show(&self, revision: u64, feed: Option<String>) -> Result<(), glib::Error> {
        let changed = {
            let mut fed = self.fed.borrow_mut();
            if fed.revision == revision {
                return Ok(());
            }
            fed.revision = revision;
            fed.feed = feed.unwrap_or_default();
            BTreeMap::from([
                ("Feed".to_owned(), fed.property("Feed")),
                ("Revision".to_owned(), fed.property("Revision")),
            ])
        };
        self.connection.emit_signal(
            None,
            &self.path,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            Some(&(INTERFACE, changed, Vec::<String>::new()).to_variant()),
        )
    }
}

impl Drop for WidgetFeed {
    fn drop(&mut self) {
        if let Some(registration) = self.registration.take() {
            let _ = self.connection.unregister_object(registration);
        }
    }
}

/// Shows `model`'s feed now and at each widget revision `shown` takes, for as long as `feed` is
/// served. The model sets its feed before the snapshot that moves the revision.
pub fn follow(
    feed: &Rc<WidgetFeed>,
    shown: &Shown,
    model: &Arc<PitboardModel>,
) -> glib::SignalHandlerId {
    let (feed, model) = (Rc::downgrade(feed), Arc::clone(model));
    let show = move |shown: &Shown| {
        if let Some(feed) = feed.upgrade() {
            // Only a connection that has closed cannot say it, and nobody hears that one.
            let _ = feed.show(shown.snapshot().widget_revision, model.widget_feed());
        }
    };
    show(shown);
    shown.connect_changed(Part::WidgetRevision, show)
}
