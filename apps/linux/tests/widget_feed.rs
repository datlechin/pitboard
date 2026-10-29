//! The widget feed as a Plasma widget reads it, each on a bus of the test's own: served as the app
//! serves it, following a fixture's model, and read and heard on a connection of its own, as a
//! widget's is.
#![cfg(target_os = "linux")]

use std::cell::{OnceCell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gio::prelude::*;
use pitboard_linux::listener;
use pitboard_linux::local_time::DesktopClock;
use pitboard_linux::shown::Shown;
use pitboard_linux::widget_feed::{self, INTERFACE, WidgetFeed};
use pitboard_model::{
    Changed, Front, Intent, Notice, Notifier, OutsideAsk, Part, PitboardModel, PlatformError,
    RemovalShown, Snapshot,
};
use serde_json::Value;

const APP: &str = "/com/usepitboard/Pitboard";
const PATIENCE: Duration = Duration::from_secs(30);

struct Unposted;

impl Notifier for Unposted {
    fn post(&self, _: Notice) -> Result<(), PlatformError> {
        Ok(())
    }
}

// `test` on a bus of its own, with a main context of its own, given the bus's address.
fn on_a_bus(test: impl FnOnce(&str)) {
    let bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);
    bus.up();
    let address = bus.bus_address().unwrap().to_string();
    glib::MainContext::new()
        .with_thread_default(|| test(&address))
        .unwrap();
    bus.down();
}

fn connect(address: &str) -> gio::DBusConnection {
    gio::DBusConnection::for_address_sync(
        address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .unwrap()
}

// The main context's iterations until `done`, within `PATIENCE`.
fn until(what: &str, done: impl Fn() -> bool) {
    let context = glib::MainContext::ref_thread_default();
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "{what}: not within {PATIENCE:?}");
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    while context.iteration(false) {}
}

/// The feed as a widget has it: read, and heard as it changes, on a connection of its own.
struct Widget {
    connection: gio::DBusConnection,
    app: String,
    heard: Rc<RefCell<Vec<(String, u64)>>>,
    _listening: gio::SignalSubscription,
}

impl Widget {
    /// Reading the feed `app`, the connection that serves it, as a widget does, from once the bus
    /// has its match for what the feed says.
    fn reading(address: &str, app: &gio::DBusConnection) -> Widget {
        let connection = connect(address);
        let app = app.unique_name().unwrap().to_string();
        let heard = Rc::new(RefCell::new(Vec::new()));
        let hearing = heard.clone();
        let listening = connection.subscribe_to_signal(
            Some(&app),
            Some("org.freedesktop.DBus.Properties"),
            Some("PropertiesChanged"),
            Some(&widget_feed::path(APP)),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let (interface, changed, invalidated) = signal
                    .parameters
                    .get::<(String, BTreeMap<String, glib::Variant>, Vec<String>)>()
                    .unwrap();
                assert_eq!(interface, INTERFACE);
                assert!(invalidated.is_empty(), "{invalidated:?}");
                hearing.borrow_mut().push(fed(&changed));
            },
        );
        // The bus takes a connection's messages in order, so it has the match once it answers.
        connection
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetId",
                None,
                None,
                gio::DBusCallFlags::NONE,
                -1,
                gio::Cancellable::NONE,
            )
            .unwrap();
        Widget {
            connection,
            app,
            heard,
            _listening: listening,
        }
    }

    // Answered once everything the app sent before its reply has been handled here.
    fn call(
        &self,
        interface: &str,
        method: &str,
        arguments: Option<glib::Variant>,
    ) -> Result<glib::Variant, glib::Error> {
        let context = glib::MainContext::ref_thread_default();
        let reply = context.block_on(self.connection.call_future(
            Some(&self.app),
            &widget_feed::path(APP),
            interface,
            method,
            arguments.as_ref(),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            10_000,
        ));
        while context.iteration(false) {}
        reply
    }

    /// The feed and its revision, as `GetAll` answers them.
    fn feed(&self) -> (String, u64) {
        let all = self
            .call(
                "org.freedesktop.DBus.Properties",
                "GetAll",
                Some((INTERFACE,).to_variant()),
            )
            .unwrap_or_else(|e| panic!("GetAll: {e}"));
        let (properties,) = all.get::<(BTreeMap<String, glib::Variant>,)>().unwrap();
        assert_eq!(
            properties.keys().collect::<Vec<_>>(),
            ["Feed", "Revision"],
            "every property the interface has"
        );
        fed(&properties)
    }

    /// Each feed and revision heard since last asked.
    fn heard(&self) -> Vec<(String, u64)> {
        self.heard.take()
    }

    /// The introspection of the feed's object.
    fn introspected(&self) -> String {
        let xml = self
            .call("org.freedesktop.DBus.Introspectable", "Introspect", None)
            .unwrap();
        xml.get::<(String,)>().unwrap().0
    }
}

fn fed(properties: &BTreeMap<String, glib::Variant>) -> (String, u64) {
    let feed = properties["Feed"]
        .get::<String>()
        .expect("Feed is a string");
    let revision = properties["Revision"]
        .get::<u64>()
        .expect("Revision is a u64");
    (feed, revision)
}

fn parsed(feed: &str) -> Value {
    serde_json::from_str(feed).unwrap_or_else(|e| panic!("{e}: {feed}"))
}

/// A fixture's model, started, its snapshots taken into a store, which the feed follows on
/// `served`, as the app's own connection serves it.
struct Fed {
    model: Arc<PitboardModel>,
    shown: Shown,
    _feed: Rc<WidgetFeed>,
    // each widget revision the store took, in order, the one it was made with first
    revisions: Rc<RefCell<Vec<u64>>>,
    _listening: glib::JoinHandle<()>,
    _scratch: tempfile::TempDir,
}

impl Fed {
    fn started(world: &str, served: &gio::DBusConnection) -> Fed {
        let scratch = tempfile::tempdir().unwrap();
        let store: Rc<OnceCell<Shown>> = Rc::default();
        let taking = store.clone();
        let (told, listening) = listener::listening(move |snapshot| {
            if let Some(shown) = taking.get() {
                shown.take(snapshot);
            }
        });
        let model = PitboardModel::fixture_in(
            world.into(),
            Front::Gnome,
            scratch.path().to_string_lossy().into_owned(),
            told,
            None,
            Arc::new(Unposted),
            DesktopClock::following(None, None).local_time(),
        )
        .unwrap();
        let shown = Shown::new(model.snapshot());
        let _ = store.set(shown.clone());
        let feed = Rc::new(WidgetFeed::serve(served, APP).unwrap());
        widget_feed::follow(&feed, &shown, &model);
        let revisions = Rc::new(RefCell::new(vec![shown.snapshot().widget_revision]));
        let taken = revisions.clone();
        shown.connect_changed(Part::WidgetRevision, move |shown| {
            taken.borrow_mut().push(shown.snapshot().widget_revision);
        });
        model.send(Intent::Start);
        Fed {
            model,
            shown,
            _feed: feed,
            revisions,
            _listening: listening,
            _scratch: scratch,
        }
    }

    fn until(&self, what: &str, done: impl Fn(&Snapshot) -> bool) {
        until(what, || done(&self.shown.snapshot()));
    }

    /// Each revision the store moved to, once each, as a widget is to hear them.
    fn moved(&self) -> Vec<u64> {
        let mut moved: Vec<u64> = Vec::new();
        let mut last = 0;
        for &revision in self.revisions.borrow().iter() {
            if revision != last {
                moved.push(revision);
                last = revision;
            }
        }
        moved
    }
}

impl Drop for Fed {
    fn drop(&mut self) {
        self.model.shutdown();
    }
}

#[test]
fn the_feed_is_the_models_and_changes_only_with_its_revision() {
    on_a_bus(|address| {
        let served = connect(address);
        let widget = Widget::reading(address, &served);
        let fed = Fed::started("oneTool", &served);
        fed.until("the accounts read", |shown| {
            !shown.reading && shown.widget_revision > 0
        });

        let (feed, revision) = widget.feed();
        assert_eq!(revision, fed.shown.snapshot().widget_revision);
        let glance = parsed(&feed);
        assert_eq!(glance["schema"], 1);
        let models = parsed(&fed.model.widget_feed().expect("the model's, once read"));
        assert_eq!(glance["accounts"], models["accounts"]);
        // As the widget offers Switch.
        let ready = (glance["accounts"].as_array().unwrap().iter())
            .find(|account| {
                account["in_use"] == false
                    && (account["standing"] == "ready" || account["standing"] == "expiring")
            })
            .unwrap_or_else(|| panic!("no account to switch to: {glance:#}"))["id"]
            .as_str()
            .unwrap()
            .to_owned();

        // As the widget's Switch asks, by the feed's id.
        fed.model.send(Intent::Outside {
            request: 1,
            ask: OutsideAsk::Switch {
                account: ready.clone(),
            },
        });
        until("the switch fed", || {
            let (feed, _) = widget.feed();
            (parsed(&feed)["accounts"].as_array().unwrap().iter())
                .any(|account| account["id"] == ready.as_str() && account["in_use"] == true)
        });

        let (last, at) = widget.feed();
        let heard = widget.heard();
        assert_eq!(heard.last(), Some(&(last.clone(), at)));
        assert_eq!(
            heard
                .iter()
                .map(|(_, revision)| *revision)
                .collect::<Vec<_>>(),
            fed.moved(),
            "once for each revision moved to, and for nothing else"
        );

        // One that names every part, at the revision shown, says nothing.
        let again = Snapshot {
            revision: fed.shown.snapshot().revision + 1,
            changed: Changed {
                since: 0,
                parts: pitboard_model::every_part(),
            },
            ..fed.shown.snapshot().clone()
        };
        assert!(fed.shown.take(again).contains(&Part::WidgetRevision));
        assert_eq!(widget.feed(), (last, at));
        assert_eq!(widget.heard(), []);
    });
}

#[test]
fn the_feed_interface_offers_nothing_to_call() {
    on_a_bus(|address| {
        let served = connect(address);
        let feed = WidgetFeed::serve(&served, APP).unwrap();
        feed.show(3, Some(r#"{"schema":1}"#.into())).unwrap();
        let widget = Widget::reading(address, &served);

        let xml = widget.introspected();
        let start = xml
            .find(&format!(r#"<interface name="{INTERFACE}">"#))
            .unwrap_or_else(|| panic!("{xml}"));
        let interface = &xml[start..start + xml[start..].find("</interface>").unwrap()];
        assert!(!interface.contains("<method"), "{interface}");
        assert!(!interface.contains("<signal"), "{interface}");
        let properties: Vec<&str> = interface.split("<property").skip(1).collect();
        assert_eq!(properties.len(), 2, "{interface}");
        for property in properties {
            assert!(property.contains(r#"access="read""#), "{property}");
        }

        for (name, value) in [("Feed", "{}".to_variant()), ("Revision", 9u64.to_variant())] {
            let set = widget.call(
                "org.freedesktop.DBus.Properties",
                "Set",
                Some((INTERFACE, name, value).to_variant()),
            );
            assert!(set.is_err(), "{name} was set");
        }
        let called = widget.call(INTERFACE, "Switch", Some(("someone",).to_variant()));
        let refused = called.expect_err("a method");
        assert!(refused.matches(gio::DBusError::UnknownMethod), "{refused}");
        assert_eq!(widget.feed(), (r#"{"schema":1}"#.to_owned(), 3));
    });
}

#[test]
fn after_uninstall_the_feed_is_empty() {
    on_a_bus(|address| {
        let served = connect(address);
        let widget = Widget::reading(address, &served);
        let fed = Fed::started("oneTool", &served);
        fed.until("the accounts read", |shown| {
            !shown.reading && shown.widget_revision > 0
        });
        let (feed, read) = widget.feed();
        assert!(!feed.is_empty());

        fed.model.send(Intent::Uninstall);
        fed.until("Pitboard removed", |shown| {
            matches!(
                shown.removal,
                Some(RemovalShown::Done | RemovalShown::Stopped { .. })
            )
        });

        assert_eq!(fed.shown.snapshot().removal, Some(RemovalShown::Done));
        let (feed, removed) = widget.feed();
        assert_eq!(feed, "");
        assert!(removed > read);
        assert_eq!(widget.heard().last(), Some(&(String::new(), removed)));
    });
}
