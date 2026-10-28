//! A fixture's stand-ins, each doing what the app asks of the desktop, beside folders that stand
//! for the homes and on a bus of the test's own that stands for the session's.
#![cfg(target_os = "linux")]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pitboard_linux::platform::autostart::StartAtSignIn;
use pitboard_linux::platform::links::Opener;
use pitboard_linux::platform::notifications::{Button, Notice, Notifier, Priority};
use pitboard_linux::platform::stand_in;
use pitboard_linux::platform::tray::{Status, StatusItem, ToolTip, TrayEntry, TrayItem};

const BUS: &str = "org.freedesktop.DBus";

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

fn get_id(connection: &gio::DBusConnection) {
    connection
        .call_sync(
            Some(BUS),
            "/org/freedesktop/DBus",
            BUS,
            "GetId",
            None,
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .unwrap();
}

// Every file under `root`, by its path from there.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                found.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    found.sort();
    found
}

fn lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn work(stand_ins: &Path) {
    let autostart = stand_in::Autostart::new(stand_ins);
    assert!(!autostart.is_on());
    autostart.turn_on().unwrap();
    assert!(autostart.is_on());
    assert!(autostart.turn_off().unwrap());
    assert!(!autostart.is_on());
    assert!(!autostart.turn_off().unwrap());

    let notifications = stand_in::Notifications::new(stand_ins);
    notifications
        .post(Notice {
            id: "run-out/work".into(),
            title: "work is out until 3:45 PM".into(),
            body: "Claude Code switched to home.".into(),
            priority: Priority::High,
            clicked: "open/work".into(),
            buttons: vec![Button {
                title: "Switch Back".into(),
                clicked: "switch-back/work".into(),
            }],
        })
        .unwrap();
    notifications.withdraw("run-out/work".into()).unwrap();

    let tray = stand_in::Tray::new(stand_ins);
    tray.show(&TrayItem {
        status: Status::NeedsAttention,
        icon_name: "com.usepitboard.Pitboard-low-symbolic".into(),
        attention_icon_name: "com.usepitboard.Pitboard-low-attention-symbolic".into(),
        tool_tip: ToolTip {
            title: "Pitboard".into(),
            text: "work 92%".into(),
        },
        menu: vec![
            TrayEntry::Item {
                id: "open".into(),
                label: "Open Pitboard".into(),
                enabled: true,
                radio: None,
            },
            TrayEntry::Separator,
            TrayEntry::Item {
                id: "switch/work".into(),
                label: "work".into(),
                enabled: true,
                radio: Some(true),
            },
        ],
    })
    .unwrap();
    tray.hide().unwrap();

    stand_in::Browser::new(stand_ins)
        .open("https://claude.ai/chat/1f2e", None)
        .unwrap();
}

// The bus announces each connection's unique name, so a stand-in that reached the bus would be
// announced before the witness.
#[test]
fn a_fixtures_stand_ins_write_only_in_its_folder() {
    let scratch = tempfile::tempdir().unwrap();
    let homes = ["home", "config", "data", "cache", "state", "runtime"];
    for home in homes {
        std::fs::create_dir(scratch.path().join(home)).unwrap();
    }
    let fixture = scratch.path().join("pitboard-fixture");
    std::fs::create_dir(&fixture).unwrap();
    let test_bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);
    test_bus.up();
    let address = test_bus.bus_address().unwrap();
    let context = glib::MainContext::new();
    let (announced, witness) = context
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
            get_id(&bus);

            work(&fixture);

            let witness = connect(&address);
            let name = witness.unique_name().unwrap().to_string();
            get_id(&bus);
            while context.iteration(false) {}
            witness.close_sync(gio::Cancellable::NONE).unwrap();
            bus.close_sync(gio::Cancellable::NONE).unwrap();
            (announced.take(), name)
        })
        .unwrap();
    test_bus.down();

    assert_eq!(announced, [witness], "a stand-in reached the bus");
    let folder = Path::new("pitboard-fixture").join(stand_in::FOLDER);
    assert_eq!(
        files(scratch.path()),
        [
            "autostart.jsonl",
            "browser.jsonl",
            "notifications.jsonl",
            "tray.jsonl"
        ]
        .map(|file| folder.join(file))
    );
    for home in homes {
        assert!(files(&scratch.path().join(home)).is_empty(), "{home}");
    }
    let written = fixture.join(stand_in::FOLDER);
    assert_eq!(
        lines(&written.join("autostart.jsonl")),
        [
            serde_json::json!({"on": true}),
            serde_json::json!({"on": false})
        ]
    );
    assert_eq!(
        lines(&written.join("notifications.jsonl")),
        [
            serde_json::json!({"post": {
                "id": "run-out/work",
                "title": "work is out until 3:45 PM",
                "body": "Claude Code switched to home.",
                "priority": "high",
                "clicked": "open/work",
                "buttons": [{"title": "Switch Back", "clicked": "switch-back/work"}],
            }}),
            serde_json::json!({"withdraw": "run-out/work"}),
        ]
    );
    let tray = lines(&written.join("tray.jsonl"));
    assert_eq!(tray[0]["show"]["status"], "NeedsAttention");
    assert_eq!(
        tray[0]["show"]["menu"][2],
        serde_json::json!({"item": {
            "id": "switch/work", "label": "work", "enabled": true, "radio": true,
        }})
    );
    assert_eq!(tray[1], serde_json::json!("hide"));
    assert_eq!(
        lines(&written.join("browser.jsonl")),
        [serde_json::json!({"open": "https://claude.ai/chat/1f2e"})]
    );
}
