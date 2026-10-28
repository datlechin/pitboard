//! A stand-in for what asks the app over the bus: a tray's host reading the status item and its
//! menu, GNOME Shell and KRunner. It calls on a connection of its own and waits on the test's main
//! context, where the app answers, and hears the signals of what it listens to.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gio::prelude::*;

pub(crate) type Properties = BTreeMap<String, glib::Variant>;

/// An object of the app's, by the name it is reached at.
pub(crate) struct At {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) interface: &'static str,
}

pub(crate) struct Host {
    connection: gio::DBusConnection,
    heard: Rc<RefCell<Vec<(String, glib::Variant)>>>,
    listening: Vec<gio::SignalSubscription>,
}

impl Host {
    pub(crate) fn new(connection: gio::DBusConnection) -> Host {
        Host {
            connection,
            heard: Rc::default(),
            listening: Vec::new(),
        }
    }

    // Answered once everything the app sent before its reply has been handled here.
    pub(crate) fn call(
        &self,
        at: &At,
        method: &str,
        arguments: glib::Variant,
    ) -> Result<glib::Variant, glib::Error> {
        let context = glib::MainContext::ref_thread_default();
        let reply = context.block_on(self.connection.call_future(
            Some(&at.name),
            &at.path,
            at.interface,
            method,
            Some(&arguments),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            10_000,
        ));
        while context.iteration(false) {}
        reply
    }

    pub(crate) fn properties(&self, at: &At) -> Properties {
        let all = At {
            name: at.name.clone(),
            path: at.path.clone(),
            interface: "org.freedesktop.DBus.Properties",
        };
        let reply = self
            .call(&all, "GetAll", (at.interface,).to_variant())
            .unwrap_or_else(|e| panic!("{} {}: {e}", at.path, at.interface));
        reply.get::<(Properties,)>().unwrap().0
    }

    pub(crate) fn listen(&mut self, at: &At) {
        let heard = self.heard.clone();
        self.listening.push(self.connection.subscribe_to_signal(
            Some(&at.name),
            Some(at.interface),
            None,
            Some(&at.path),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                heard
                    .borrow_mut()
                    .push((signal.signal_name.to_owned(), signal.parameters.clone()));
            },
        ));
    }

    /// Each signal heard since last asked, by its name, with its parameters.
    pub(crate) fn heard(&self) -> Vec<(String, glib::Variant)> {
        self.heard.take()
    }
}
