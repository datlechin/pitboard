//! Waking as logind says it, from a stand-in for logind on a bus of the test's own, which plays
//! the system bus.
#![cfg(target_os = "linux")]

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gio::prelude::*;
use pitboard_linux::platform::logind::hear;

const LOGIND: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";

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

fn bus_call(connection: &gio::DBusConnection, method: &str, arguments: Option<&glib::Variant>) {
    connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            method,
            arguments,
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .unwrap_or_else(|e| panic!("{method}: {e}"));
}

// Sent, and passed on by the bus once it answers the call after it.
fn prepare_for_sleep(logind: &gio::DBusConnection, start: bool) {
    logind
        .emit_signal(
            None,
            PATH,
            MANAGER,
            "PrepareForSleep",
            Some(&(start,).to_variant()),
        )
        .unwrap();
    bus_call(logind, "GetId", None);
}

// The context's iterations until `done`, for ten seconds at most, then every one waiting.
fn iterate_until(context: &glib::MainContext, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "not done in ten seconds");
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    while context.iteration(false) {}
}

// Everything the bus passed on to `connection` before it answers a call, told.
fn settle(context: &glib::MainContext, connection: &gio::DBusConnection) {
    bus_call(connection, "GetId", None);
    while context.iteration(false) {}
}

#[test]
fn waking_is_told_once_and_going_to_sleep_is_not() {
    let test_bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);
    test_bus.up();
    let address = test_bus.bus_address().unwrap();
    let context = glib::MainContext::new();
    let told = context
        .with_thread_default(|| {
            let logind = connect(&address);
            bus_call(&logind, "RequestName", Some(&(LOGIND, 4u32).to_variant()));
            let system = connect(&address);
            let told = Rc::new(Cell::new(0));
            let _waking = hear(&system, {
                let told = told.clone();
                move || told.set(told.get() + 1)
            });
            settle(&context, &system);

            prepare_for_sleep(&logind, true);
            settle(&context, &system);
            assert_eq!(told.get(), 0, "going to sleep was told");
            prepare_for_sleep(&logind, false);
            iterate_until(&context, || told.get() > 0);
            settle(&context, &system);
            told.get()
        })
        .unwrap();
    test_bus.down();

    assert_eq!(told, 1);
}

// A signal from a connection that does not own logind's name is not logind's.
#[test]
fn only_logind_is_heard() {
    let test_bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);
    test_bus.up();
    let address = test_bus.bus_address().unwrap();
    let context = glib::MainContext::new();
    let told = context
        .with_thread_default(|| {
            let system = connect(&address);
            let told = Rc::new(Cell::new(0));
            let _waking = hear(&system, {
                let told = told.clone();
                move || told.set(told.get() + 1)
            });
            settle(&context, &system);

            let pretender = connect(&address);
            prepare_for_sleep(&pretender, false);
            settle(&context, &system);
            told.get()
        })
        .unwrap();
    test_bus.down();

    assert_eq!(told, 0);
}
