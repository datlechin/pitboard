//! Waking from sleep, as logind says it on the system bus: `PrepareForSleep(false)` once the
//! system is back, from the owner of `org.freedesktop.login1` alone (docs/facts/linux-app.md,
//! Waking). Where no logind owns that name, as in a container, that is said, and nothing else.

use std::rc::Rc;

const LOGIND: &str = "org.freedesktop.login1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    Woke,
    NoLogind,
}

/// Hearing logind on a connection, until dropped.
pub struct Waking {
    _signal: gio::SignalSubscription,
    // gio 0.22 exports another `WatcherId` than the one its name watch returns.
    unwatch: Option<Box<dyn FnOnce()>>,
}

impl Drop for Waking {
    fn drop(&mut self) {
        if let Some(unwatch) = self.unwatch.take() {
            unwatch();
        }
    }
}

/// Tells `told` what logind says on `bus`, the system bus in use, on the thread-default main
/// context. Neither asks the bus to start logind.
pub fn hear(bus: &gio::DBusConnection, told: impl Fn(Heard) + 'static) -> Waking {
    let told = Rc::new(told);
    let signal = bus.subscribe_to_signal(
        Some(LOGIND),
        Some("org.freedesktop.login1.Manager"),
        Some("PrepareForSleep"),
        Some("/org/freedesktop/login1"),
        None,
        gio::DBusSignalFlags::NONE,
        {
            let told = told.clone();
            move |signal| {
                if signal.parameters.get::<(bool,)>() == Some((false,)) {
                    told(Heard::Woke);
                }
            }
        },
    );
    let watcher = gio::bus_watch_name_on_connection(
        bus,
        LOGIND,
        gio::BusNameWatcherFlags::NONE,
        |_, _, _| {},
        move |_, _| told(Heard::NoLogind),
    );
    Waking {
        _signal: signal,
        unwatch: Some(Box::new(move || gio::bus_unwatch_name(watcher))),
    }
}
