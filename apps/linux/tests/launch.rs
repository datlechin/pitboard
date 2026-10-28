//! The program as a launch starts it, on a bus and in homes of the test's own.
#![cfg(target_os = "linux")]

use std::cell::RefCell;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

use glib::prelude::*;
use pitboard_linux::config::APP_ID;
use pitboard_linux::launch::NOTHING_TO_RUN;

const HOMES: [&str; 9] = [
    "HOME",
    "PITBOARD_HOME",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "XDG_RUNTIME_DIR",
];

const BUS: &str = "org.freedesktop.DBus";

fn bus_call(
    bus: &gio::DBusConnection,
    method: &str,
    arguments: Option<&glib::Variant>,
) -> glib::Variant {
    bus.call_sync(
        Some(BUS),
        "/org/freedesktop/DBus",
        BUS,
        method,
        arguments,
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
    )
    .unwrap_or_else(|e| panic!("{method}: {e}"))
}

fn is_empty(folder: &Path) -> bool {
    std::fs::read_dir(folder).unwrap().next().is_none()
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

// The bus announces each connection's unique name in NameOwnerChanged, as it does the witness's
// here, so a program announced nowhere before the witness never reached the bus at all.
#[test]
fn a_build_with_no_model_quits_before_it_owns_a_name() {
    let scratch = tempfile::tempdir().unwrap();
    let homes: Vec<_> = HOMES
        .iter()
        .map(|name| (*name, scratch.path().join(name)))
        .collect();
    for (_, folder) in &homes {
        std::fs::create_dir(folder).unwrap();
    }
    let test_bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);
    test_bus.up();
    let address = test_bus
        .bus_address()
        .expect("the test's bus has an address");
    let context = glib::MainContext::new();
    let (output, announced, witness, owned) = context
        .with_thread_default(|| {
            let bus = connect(&address);
            let announced = Rc::new(RefCell::new(Vec::new()));
            let _subscription = bus.subscribe_to_signal(
                Some(BUS),
                Some(BUS),
                Some("NameOwnerChanged"),
                Some("/org/freedesktop/DBus"),
                None,
                gio::DBusSignalFlags::NONE,
                {
                    let announced = announced.clone();
                    move |signal| {
                        let (name, _, _) =
                            signal.parameters.get::<(String, String, String)>().unwrap();
                        announced.borrow_mut().push(name);
                    }
                },
            );
            // A call answered after the match rule was sent, so the bus applies it from here.
            bus_call(&bus, "GetId", None);

            let output = Command::new(env!("CARGO_BIN_EXE_pitboard-app"))
                .env_clear()
                .envs(homes.iter().map(|(name, folder)| (name, folder)))
                .env("DBUS_SESSION_BUS_ADDRESS", address.as_str())
                .output()
                .unwrap();

            let witness = connect(&address);
            let witness_name = witness.unique_name().unwrap().to_string();
            // Answered after every signal the bus sent before it.
            let owned = bus_call(&bus, "NameHasOwner", Some(&(APP_ID,).to_variant()));
            while context.iteration(false) {}
            witness.close_sync(gio::Cancellable::NONE).unwrap();
            bus.close_sync(gio::Cancellable::NONE).unwrap();
            (
                output,
                announced.take(),
                witness_name,
                owned.get::<(bool,)>().unwrap().0,
            )
        })
        .unwrap();
    test_bus.down();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("pitboard-app: {NOTHING_TO_RUN}\n")
    );
    assert!(output.stdout.is_empty());
    assert_eq!(announced, [witness], "the program reached the bus");
    assert!(!owned, "{APP_ID} has an owner");
    for (name, folder) in &homes {
        assert!(is_empty(folder), "{name} holds what the program wrote");
    }
}
