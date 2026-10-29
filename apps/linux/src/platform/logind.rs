//! Waking from sleep, as logind says it on the system bus: `PrepareForSleep(false)` once the
//! system is back, from the owner of `org.freedesktop.login1` alone (docs/facts/linux-app.md,
//! Waking). Where no logind owns that name, as in a container, nothing is heard.

const LOGIND: &str = "org.freedesktop.login1";

/// Hearing logind on a connection, until dropped.
pub struct Waking {
    _signal: gio::SignalSubscription,
}

/// Calls `woke` each time logind says the system woke on `bus`, the system bus in use, on the
/// thread-default main context. Subscribing asks the bus to start nothing.
pub fn hear(bus: &gio::DBusConnection, woke: impl Fn() + 'static) -> Waking {
    let signal = bus.subscribe_to_signal(
        Some(LOGIND),
        Some("org.freedesktop.login1.Manager"),
        Some("PrepareForSleep"),
        Some("/org/freedesktop/login1"),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            if signal.parameters.get::<(bool,)>() == Some((false,)) {
                woke();
            }
        },
    );
    Waking { _signal: signal }
}
