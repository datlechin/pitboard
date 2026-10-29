//! The program as a launch starts it, on a bus and in homes of the test's own: what quits before
//! it owns a name.
#![cfg(target_os = "linux")]

use std::cell::RefCell;
#[cfg(feature = "fixture")]
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

use glib::prelude::*;
use pitboard_linux::config::APP_ID;
use pitboard_linux::launch::OUTSIDE_A_FIXTURE;

const HOMES: [&str; 10] = [
    "HOME",
    "PITBOARD_HOME",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "XDG_RUNTIME_DIR",
    "TMPDIR",
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

struct Quit {
    output: std::process::Output,
    // every connection the bus announced, the witness's last
    announced: Vec<String>,
    witness: String,
    owned: bool,
}

// The built program, started with `variables` beside fresh homes and given `arguments`, on a bus
// of the test's own. The bus announces each connection's unique name in NameOwnerChanged, as it
// does the witness's here, so a program announced nowhere before the witness never reached the
// bus.
fn launched(
    homes: &[(&str, std::path::PathBuf)],
    variables: &[(&str, &str)],
    arguments: &[&str],
) -> Quit {
    let test_bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);
    test_bus.up();
    let address = test_bus
        .bus_address()
        .expect("the test's bus has an address");
    let context = glib::MainContext::new();
    let quit = context
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
                .args(arguments)
                .env_clear()
                .envs(homes.iter().map(|(name, folder)| (name, folder)))
                .envs(variables.iter().copied())
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
            Quit {
                output,
                announced: announced.take(),
                witness: witness_name,
                owned: owned.get::<(bool,)>().unwrap().0,
            }
        })
        .unwrap();
    test_bus.down();
    quit
}

fn homes(scratch: &Path) -> Vec<(&'static str, std::path::PathBuf)> {
    HOMES
        .iter()
        .map(|name| {
            let folder = scratch.join(name);
            std::fs::create_dir(&folder).unwrap();
            (*name, folder)
        })
        .collect()
}

fn quit_before_the_bus(quit: &Quit, said: &str) {
    assert_eq!(quit.output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&quit.output.stderr),
        format!("pitboard-app: {said}\n")
    );
    assert!(quit.output.stdout.is_empty());
    assert_eq!(
        quit.announced,
        std::slice::from_ref(&quit.witness),
        "the program reached the bus"
    );
    assert!(!quit.owned, "{APP_ID} has an owner");
}

// As D-Bus activation, autostart or a link would start it: on the session's homes.
#[test]
fn a_debug_build_outside_a_fixture_quits_before_it_owns_a_name() {
    let scratch = tempfile::tempdir().unwrap();
    let homes = homes(scratch.path());

    let quit = launched(&homes, &[], &[]);

    quit_before_the_bus(&quit, OUTSIDE_A_FIXTURE);
    for (name, folder) in &homes {
        assert!(is_empty(folder), "{name} holds what the program wrote");
    }
}

// Before it owns a name, so the world of a fixture already running stays as it was.
#[test]
fn a_fixture_with_a_wrong_name_is_refused_with_every_name() {
    let scratch = tempfile::tempdir().unwrap();
    let homes = homes(scratch.path());
    let tmp = &homes.iter().find(|(name, _)| *name == "TMPDIR").unwrap().1;
    let left = tmp.join("pitboard-fixture").join("left by a launch");
    std::fs::create_dir(left.parent().unwrap()).unwrap();
    std::fs::write(&left, "kept").unwrap();

    let quit = launched(&homes, &[("PITBOARD_FIXTURE", "twoTool")], &[]);

    let refused =
        pitboard_model::fixture_refusal(pitboard_model::Front::Gnome, "twoTool").expect("refused");
    quit_before_the_bus(&quit, &refused.to_string());
    let said = String::from_utf8_lossy(&quit.output.stderr).into_owned();
    for name in pitboard_model::fixture_names() {
        assert!(said.contains(&name), "{said}");
    }
    assert_eq!(std::fs::read_to_string(&left).unwrap(), "kept");
    for (name, folder) in homes.iter().filter(|(name, _)| *name != "TMPDIR") {
        assert!(is_empty(folder), "{name} holds what the program wrote");
    }
}

// The long options GLib's help lists, of each line that begins with options, `--` and
// `=ARGUMENT` left out.
#[cfg(feature = "fixture")]
fn listed<'a>(help: impl Iterator<Item = &'a str>) -> BTreeSet<String> {
    help.flat_map(|line| {
        line.split_whitespace()
            .map(|word| word.trim_end_matches(','))
            .take_while(|word| word.starts_with('-'))
            .filter_map(|word| word.strip_prefix("--"))
            .map(|name| name.split('=').next().unwrap_or(name).to_owned())
            .collect::<Vec<_>>()
    })
    .collect()
}

// Debian Policy 12.1 asks each program of a package for a manual page. GLib's own help, which
// quits before the program owns its name, lists what the program takes: the options the app adds,
// under Application Options, and GApplication's and GOption's own. A launch outside a fixture
// quits before GLib reads any.
#[cfg(feature = "fixture")]
#[test]
fn the_manual_page_names_every_option_the_app_adds_and_only_options_the_program_takes() {
    let scratch = tempfile::tempdir().unwrap();
    let homes = homes(scratch.path());
    let fixture = pitboard_model::fixture_names().remove(0);

    let quit = launched(&homes, &[("PITBOARD_FIXTURE", &fixture)], &["--help-all"]);

    let help = String::from_utf8(quit.output.stdout.clone()).unwrap();
    assert_eq!(quit.output.status.code(), Some(0), "{help}");
    // The bus announces a name taken as it does a connection.
    assert!(
        !quit.announced.iter().any(|name| name == APP_ID),
        "the program took {APP_ID}"
    );
    let takes = listed(help.lines());
    let mut section = "";
    let adds = listed(help.lines().filter(|line| {
        if !line.starts_with(' ') {
            section = line;
        }
        section == "Application Options:"
    }));
    assert!(
        !adds.is_empty(),
        "GLib's help lists no Application Options: {help}"
    );
    let manual = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join("pitboard-app.1"),
    )
    .unwrap();
    // Each `.B` or `.BR` line of the OPTIONS section, its `\-` a hyphen.
    let named: BTreeSet<String> = manual
        .split_once(".SH OPTIONS\n")
        .and_then(|(_, rest)| rest.split("\n.SH ").next())
        .expect("the manual page has an OPTIONS section")
        .lines()
        .filter(|line| line.starts_with(".B"))
        .flat_map(|line| {
            line.replace("\\-", "-")
                .split_whitespace()
                .filter_map(|word| word.strip_prefix("--"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();

    assert!(
        adds.is_subset(&named),
        "the manual page names {named:?}, not each of {adds:?}"
    );
    assert!(
        named.is_subset(&takes),
        "the program takes {takes:?}, not each of {named:?}"
    );
}
