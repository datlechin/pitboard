//! The app's presence on the session bus, each on a bus of the test's own: the status item and
//! its menu, read and driven by stand-ins for a tray's watcher and host, and the search
//! provider and runner, asked by stand-ins for GNOME Shell and KRunner.
#![cfg(target_os = "linux")]

mod host;
mod watcher;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gio::prelude::*;
use pitboard_linux::platform::krunner::{self, Run, Runner};
use pitboard_linux::platform::search::{self, Asked, Found, Provider};
use pitboard_linux::platform::tray::{
    Desktop, Heard, Status, StatusItem, ToolTip, TrayEntry, TrayItem,
};

use host::{At, Host, Properties};
use watcher::Watcher;

const ID: &str = "com.usepitboard.Pitboard";
const TITLE: &str = "Pitboard";
const APP: &str = "/com/usepitboard/Pitboard";

struct Session {
    address: String,
}

impl Session {
    fn connect(&self) -> gio::DBusConnection {
        gio::DBusConnection::for_address_sync(
            &self.address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            gio::Cancellable::NONE,
        )
        .unwrap()
    }

    fn owned(&self, name: &str) -> bool {
        bus(&self.connect(), "NameHasOwner", (name,).to_variant()) == (true,).to_variant()
    }

    // The main context's iterations until `done`, for ten seconds at most.
    fn until(&self, done: impl Fn() -> bool) {
        let context = glib::MainContext::ref_thread_default();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "not done in ten seconds");
            if !context.iteration(false) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        while context.iteration(false) {}
    }
}

// `test` on a bus of its own, which starts what `services` holds, and a main context of its own.
fn on_a_bus(services: Option<&Path>, test: impl FnOnce(&Session)) {
    let test_bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);
    if let Some(services) = services {
        test_bus.add_service_dir(services.to_str().unwrap());
    }
    test_bus.up();
    let session = Session {
        address: test_bus.bus_address().unwrap().into(),
    };
    glib::MainContext::new()
        .with_thread_default(|| test(&session))
        .unwrap();
    test_bus.down();
}

// The bus's own answer, which its process gives whatever this thread is doing.
fn bus(connection: &gio::DBusConnection, method: &str, arguments: glib::Variant) -> glib::Variant {
    connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            method,
            Some(&arguments),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .unwrap_or_else(|e| panic!("{method}: {e}"))
}

fn collected<T: 'static>() -> (Rc<RefCell<Vec<T>>>, impl Fn(T) + 'static) {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let collect = {
        let collected = collected.clone();
        move |told| collected.borrow_mut().push(told)
    };
    (collected, collect)
}

fn entry(id: &str, label: &str) -> TrayEntry {
    TrayEntry::Item {
        id: id.into(),
        label: label.into(),
        enabled: true,
        radio: None,
    }
}

fn radio(id: &str, label: &str, chosen: bool) -> TrayEntry {
    TrayEntry::Item {
        id: id.into(),
        label: label.into(),
        enabled: true,
        radio: Some(chosen),
    }
}

fn face() -> TrayItem {
    TrayItem {
        status: Status::Active,
        icon_name: "com.usepitboard.Pitboard-plenty-symbolic".into(),
        attention_icon_name: "com.usepitboard.Pitboard-plenty-attention-symbolic".into(),
        tool_tip: ToolTip {
            title: "Pitboard".into(),
            text: "work: 5-hour 80% left".into(),
        },
        menu: vec![
            entry("open", "Open Pitboard"),
            TrayEntry::Separator,
            radio("switch/work", "work_1", true),
            radio("switch/home", "home", false),
            TrayEntry::Item {
                id: "add".into(),
                label: "Add Account…".into(),
                enabled: false,
                radio: None,
            },
            TrayEntry::Separator,
            entry("quit", "Quit Pitboard"),
        ],
    }
}

fn properties<const N: usize>(pairs: [(&str, glib::Variant); N]) -> Properties {
    pairs
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}

// An item shown with `face()` on the app's connection, its name owned, and a host to ask it.
struct Shown {
    item: Desktop,
    host: Host,
    sni: At,
    menu: At,
}

impl Shown {
    fn new(session: &Session, told: impl Fn(Heard) + 'static) -> Shown {
        let item = Desktop::new(&session.connect(), ID, TITLE, told);
        item.show(&face()).unwrap();
        session.until(|| session.owned(item.name()));
        let at = |path: &str, interface| At {
            name: item.name().to_owned(),
            path: path.to_owned(),
            interface,
        };
        let (sni, menu) = (
            at("/StatusNotifierItem", "org.kde.StatusNotifierItem"),
            at("/MenuBar", "com.canonical.dbusmenu"),
        );
        Shown {
            host: Host::new(session.connect()),
            item,
            sni,
            menu,
        }
    }

    // One whose bus has no watcher, from once that is said, with what it hears after.
    fn with_no_tray(session: &Session) -> (Shown, Rc<RefCell<Vec<Heard>>>) {
        let (heard, told) = collected();
        let shown = Shown::new(session, told);
        session.until(|| !heard.borrow().is_empty());
        assert_eq!(heard.take(), [Heard::NoTray]);
        (shown, heard)
    }

    // GetLayout's revision, then the entry asked for, then its children, each by its number.
    fn layout(
        &self,
        parent: i32,
        depth: i32,
        names: &[&str],
    ) -> (u32, (i32, Properties), Vec<(i32, Properties)>) {
        let reply = self
            .host
            .call(&self.menu, "GetLayout", (parent, depth, names).to_variant())
            .unwrap();
        // `Vec<glib::Variant>` takes each child out of its one variant, as the AppIndicator
        // extension does, so what is left must be the entry.
        let (revision, (number, properties, children)) = reply
            .get::<(u32, (i32, Properties, Vec<glib::Variant>))>()
            .unwrap_or_else(|| panic!("{}", reply.print(true)));
        let children = children
            .iter()
            .map(|child| {
                let (number, properties, grandchildren) = child
                    .get::<(i32, Properties, Vec<glib::Variant>)>()
                    .unwrap_or_else(|| {
                        panic!("a child is {}: {}", child.type_(), reply.print(true))
                    });
                assert!(grandchildren.is_empty(), "{number} has children");
                (number, properties)
            })
            .collect();
        (revision, (number, properties), children)
    }

    fn numbers(&self) -> Vec<i32> {
        self.layout(0, -1, &[])
            .2
            .into_iter()
            .map(|(number, _)| number)
            .collect()
    }

    fn event(&self, number: i32, event: &str) -> Result<glib::Variant, glib::Error> {
        let arguments = (number, event, 0i32.to_variant(), 0u32).to_variant();
        self.host.call(&self.menu, "Event", arguments)
    }
}

fn refused_as_invalid(reply: Result<glib::Variant, glib::Error>) {
    let refused = reply.expect_err("answered");
    assert!(refused.matches(gio::DBusError::InvalidArgs), "{refused}");
}

#[test]
fn an_item_registers_once_a_watcher_appears_and_again_after_it_returns() {
    on_a_bus(None, |session| {
        let (heard, told) = collected();
        let shown = Shown::new(session, told);
        session.until(|| !heard.borrow().is_empty());
        let watcher = Watcher::new(session.connect());

        watcher.appear();
        session.until(|| heard.borrow().len() == 2);
        watcher.leave();
        session.until(|| heard.borrow().len() == 3);
        watcher.appear();
        session.until(|| heard.borrow().len() == 4);

        assert_eq!(
            heard.take(),
            [Heard::NoTray, Heard::Tray, Heard::NoTray, Heard::Tray]
        );
        let name = shown.item.name();
        assert_eq!(
            name,
            format!("org.kde.StatusNotifierItem-{}-1", std::process::id())
        );
        assert_eq!(watcher.registered(), [name, name]);
    });
}

// A watcher the bus could start is not started: the item asks only a watcher that runs.
#[test]
fn with_no_watcher_nothing_registers_and_the_tray_is_said_to_be_absent() {
    let services = tempfile::tempdir().unwrap();
    let started = services.path().join("started");
    std::fs::write(
        services.path().join(format!("{}.service", watcher::NAME)),
        format!(
            "[D-BUS Service]\nName={}\nExec=/bin/sh -c 'touch {}'\n",
            watcher::NAME,
            started.display()
        ),
    )
    .unwrap();
    on_a_bus(Some(services.path()), |session| {
        let (heard, told) = collected();
        let shown = Shown::new(session, told);
        session.until(|| !heard.borrow().is_empty());
        shown.host.properties(&shown.sni);

        assert_eq!(heard.take(), [Heard::NoTray]);
        assert!(!started.exists(), "a watcher was started");

        // As the bus would have, for anything that asked it to.
        let asking = gio::Cancellable::new();
        session.connect().call(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "StartServiceByName",
            Some(&(watcher::NAME, 0u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            Some(&asking),
            |_| {},
        );
        session.until(|| started.exists());
        asking.cancel();
    });
}

#[test]
fn status_tooltip_and_icon_follow_what_the_item_is_given() {
    on_a_bus(None, |session| {
        let (_, told) = collected();
        let mut shown = Shown::new(session, told);
        shown.host.listen(&shown.sni);
        let read = shown.host.properties(&shown.sni);
        let tool_tip =
            |text: &str| ("", Vec::<(i32, i32, Vec<u8>)>::new(), "Pitboard", text).to_variant();
        let no_pixmap = Vec::<(i32, i32, Vec<u8>)>::new().to_variant();
        assert_eq!(
            read,
            properties([
                ("Category", "ApplicationStatus".to_variant()),
                ("Id", ID.to_variant()),
                ("Title", TITLE.to_variant()),
                ("Status", "Active".to_variant()),
                ("WindowId", 0i32.to_variant()),
                ("IconThemePath", "".to_variant()),
                (
                    "Menu",
                    glib::variant::ObjectPath::try_from("/MenuBar")
                        .unwrap()
                        .to_variant()
                ),
                ("ItemIsMenu", false.to_variant()),
                (
                    "IconName",
                    "com.usepitboard.Pitboard-plenty-symbolic".to_variant()
                ),
                ("IconPixmap", no_pixmap.clone()),
                ("OverlayIconName", "".to_variant()),
                ("OverlayIconPixmap", no_pixmap.clone()),
                (
                    "AttentionIconName",
                    "com.usepitboard.Pitboard-plenty-attention-symbolic".to_variant()
                ),
                ("AttentionIconPixmap", no_pixmap),
                ("AttentionMovieName", "".to_variant()),
                ("ToolTip", tool_tip("work: 5-hour 80% left")),
            ])
        );

        let mut out = face();
        out.status = Status::NeedsAttention;
        out.icon_name = "com.usepitboard.Pitboard-out-symbolic".into();
        out.tool_tip.text = "work is out until 3:45 PM".into();
        shown.item.show(&out).unwrap();
        let read = shown.host.properties(&shown.sni);
        assert_eq!(
            shown.host.heard(),
            [
                ("NewStatus".to_owned(), ("NeedsAttention",).to_variant()),
                ("NewIcon".to_owned(), ().to_variant()),
                ("NewToolTip".to_owned(), ().to_variant()),
            ]
        );
        assert_eq!(read["Status"], "NeedsAttention".to_variant());
        assert_eq!(
            read["IconName"],
            "com.usepitboard.Pitboard-out-symbolic".to_variant()
        );
        assert_eq!(read["ToolTip"], tool_tip("work is out until 3:45 PM"));

        shown.item.show(&out).unwrap();
        shown.host.properties(&shown.sni);
        let again = shown.host.heard();
        assert!(again.is_empty(), "{again:?}");
    });
}

#[test]
fn the_layout_holds_each_entry_with_its_toggle_and_whether_it_is_enabled() {
    on_a_bus(None, |session| {
        let (_, told) = collected();
        let shown = Shown::new(session, told);
        let label = |text: &str| ("label", text.to_variant());
        let separator = || properties([("type", "separator".to_variant())]);
        let chosen = |on: i32| {
            [
                ("toggle-type", "radio".to_variant()),
                ("toggle-state", on.to_variant()),
            ]
        };

        let (revision, root, entries) = shown.layout(0, -1, &[]);
        assert!(revision > 0);
        assert_eq!(
            root,
            (
                0,
                properties([("children-display", "submenu".to_variant())])
            )
        );
        let numbers: Vec<i32> = entries.iter().map(|(number, _)| *number).collect();
        let mut distinct = numbers.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), numbers.len(), "{numbers:?}");
        assert!(!numbers.contains(&0), "{numbers:?}");
        let [work, home] = [chosen(1), chosen(0)];
        assert_eq!(
            entries
                .into_iter()
                .map(|(_, properties)| properties)
                .collect::<Vec<_>>(),
            [
                properties([label("Open Pitboard")]),
                separator(),
                // An underscore alone marks a key to press, so the label doubles it.
                properties([label("work__1"), work[0].clone(), work[1].clone()]),
                properties([label("home"), home[0].clone(), home[1].clone()]),
                properties([label("Add Account…"), ("enabled", false.to_variant())]),
                separator(),
                properties([label("Quit Pitboard")]),
            ]
        );

        assert!(shown.layout(0, 0, &[]).2.is_empty());
        assert_eq!(
            shown.layout(numbers[2], -1, &["toggle-state", "visible"]),
            (
                revision,
                (
                    numbers[2],
                    properties([("toggle-state", 1i32.to_variant())])
                ),
                vec![]
            )
        );
        let group = shown
            .host
            .call(
                &shown.menu,
                "GetGroupProperties",
                (vec![numbers[4], 999], vec!["enabled"]).to_variant(),
            )
            .unwrap();
        assert_eq!(
            group.get::<(Vec<(i32, Properties)>,)>().unwrap().0,
            [(numbers[4], properties([("enabled", false.to_variant())]))]
        );
        let every = shown
            .host
            .call(
                &shown.menu,
                "GetGroupProperties",
                (Vec::<i32>::new(), vec!["label"]).to_variant(),
            )
            .unwrap();
        assert_eq!(every.get::<(Vec<(i32, Properties)>,)>().unwrap().0.len(), 8);
        let label = shown
            .host
            .call(
                &shown.menu,
                "GetProperty",
                (numbers[0], "label").to_variant(),
            )
            .unwrap();
        assert_eq!(label, ("Open Pitboard".to_variant(),).to_variant());
        refused_as_invalid(shown.host.call(
            &shown.menu,
            "GetProperty",
            (numbers[0], "enabled").to_variant(),
        ));
        refused_as_invalid(shown.host.call(
            &shown.menu,
            "GetLayout",
            (999i32, -1i32, Vec::<String>::new()).to_variant(),
        ));

        let menu = shown.host.properties(&shown.menu);
        assert_eq!(
            menu,
            properties([
                ("Version", 3u32.to_variant()),
                ("TextDirection", "ltr".to_variant()),
                ("Status", "normal".to_variant()),
                ("IconThemePath", Vec::<String>::new().to_variant()),
            ])
        );
    });
}

// Ubuntu 26.04's AppIndicator extension takes each child out of one variant and reads the entry
// there (`dbusMenu.js:382-383`), and threw on a child boxed twice.
#[test]
fn the_layout_boxes_each_entry_once_as_the_appindicator_extension_reads_it() {
    on_a_bus(None, |session| {
        let (_, told) = collected();
        let shown = Shown::new(session, told);

        let reply = shown
            .host
            .call(
                &shown.menu,
                "GetLayout",
                (0i32, -1i32, vec!["type"]).to_variant(),
            )
            .unwrap();
        let children = reply.child_value(1).child_value(2);
        assert_eq!(children.n_children(), face().menu.len());
        for at in 0..children.n_children() {
            let child = children.child_value(at).as_variant().unwrap();
            assert_eq!(
                child.type_().as_str(),
                "(ia{sv}av)",
                "{}",
                reply.print(true)
            );
        }
    });
}

#[test]
fn a_new_menu_raises_the_revision_and_says_so() {
    on_a_bus(None, |session| {
        let (_, told) = collected();
        let mut shown = Shown::new(session, told);
        shown.host.listen(&shown.menu);
        let (before, _, entries) = shown.layout(0, -1, &[]);

        let mut more = face();
        more.menu.insert(5, entry("refresh", "Refresh"));
        shown.item.show(&more).unwrap();
        let (after, _, now) = shown.layout(0, -1, &[]);
        assert_eq!(after, before + 1);
        assert_eq!(
            shown.host.heard(),
            [("LayoutUpdated".to_owned(), (after, 0i32).to_variant())]
        );
        let mut kept: Vec<i32> = now.iter().map(|(number, _)| *number).collect();
        let added = kept.remove(5);
        assert_eq!(
            kept,
            entries
                .iter()
                .map(|(number, _)| *number)
                .collect::<Vec<_>>(),
            "an entry that stays keeps its number"
        );
        assert!(!kept.contains(&added));

        shown.item.show(&more).unwrap();
        assert_eq!(shown.layout(0, -1, &[]).0, after);
        let again = shown.host.heard();
        assert!(again.is_empty(), "{again:?}");
    });
}

// Plasma 6.3.6 reads again only the properties a layout holds, and the AppIndicator extension
// none of an entry it knows, so a change to an entry that stays is said by its properties.
#[test]
fn an_entry_that_changes_says_what_changed_and_keeps_its_number() {
    on_a_bus(None, |session| {
        let (_, told) = collected();
        let mut shown = Shown::new(session, told);
        shown.host.listen(&shown.menu);
        let (before, _, entries) = shown.layout(0, -1, &[]);
        let number = |at: usize| entries[at].0;

        let mut switched = face();
        switched.menu[2] = radio("switch/work", "work_1", false);
        switched.menu[3] = radio("switch/home", "home", true);
        switched.menu[4] = entry("add", "Add Account…");
        shown.item.show(&switched).unwrap();
        let (after, _, now) = shown.layout(0, -1, &[]);

        assert_eq!(after, before);
        assert_eq!(
            now.iter().map(|(number, _)| *number).collect::<Vec<_>>(),
            entries
                .iter()
                .map(|(number, _)| *number)
                .collect::<Vec<_>>()
        );
        let updated = vec![
            (number(2), properties([("toggle-state", 0i32.to_variant())])),
            (number(3), properties([("toggle-state", 1i32.to_variant())])),
        ];
        let removed = vec![(number(4), vec!["enabled"])];
        assert_eq!(
            shown.host.heard(),
            [(
                "ItemsPropertiesUpdated".to_owned(),
                (updated, removed).to_variant()
            )]
        );
    });
}

#[test]
fn a_clicked_entry_is_told_once_by_its_id() {
    on_a_bus(None, |session| {
        let (shown, heard) = Shown::with_no_tray(session);
        let numbers = shown.numbers();

        shown.event(numbers[3], "clicked").unwrap();
        shown.event(numbers[1], "clicked").unwrap();
        shown.event(numbers[4], "clicked").unwrap();
        shown.event(numbers[0], "hovered").unwrap();
        refused_as_invalid(shown.event(999, "clicked"));
        let events = |numbers: &[i32]| {
            let events: Vec<_> = numbers
                .iter()
                .map(|number| (*number, "clicked", 0i32.to_variant(), 0u32))
                .collect();
            (events,).to_variant()
        };
        let unknown = shown
            .host
            .call(&shown.menu, "EventGroup", events(&[numbers[6], 999]))
            .unwrap();
        assert_eq!(unknown, (vec![999i32],).to_variant());
        refused_as_invalid(shown.host.call(&shown.menu, "EventGroup", events(&[999])));

        assert_eq!(
            heard.take(),
            [
                Heard::Clicked {
                    id: "switch/home".into(),
                    token: None,
                },
                Heard::Clicked {
                    id: "quit".into(),
                    token: None,
                },
            ]
        );
    });
}

#[test]
fn about_to_show_is_told_as_a_glance() {
    on_a_bus(None, |session| {
        let (shown, heard) = Shown::with_no_tray(session);
        let numbers = shown.numbers();
        let about_to_show = |number: i32| {
            shown
                .host
                .call(&shown.menu, "AboutToShow", (number,).to_variant())
        };

        assert_eq!(about_to_show(0).unwrap(), (false,).to_variant());
        assert_eq!(about_to_show(numbers[0]).unwrap(), (false,).to_variant());
        shown.event(0, "opened").unwrap();
        let group = shown
            .host
            .call(
                &shown.menu,
                "AboutToShowGroup",
                (vec![0i32, 999],).to_variant(),
            )
            .unwrap();
        assert_eq!(group, (Vec::<i32>::new(), vec![999i32]).to_variant());
        refused_as_invalid(about_to_show(999));

        assert_eq!(heard.take(), [Heard::Glanced, Heard::Glanced]);
    });
}

// Plasma 6.3.6 provides a token before Activate, SecondaryActivate and ContextMenu, and the
// AppIndicator extension before Activate and a click on an entry.
#[test]
fn activate_hands_over_the_token_the_host_provided() {
    on_a_bus(None, |session| {
        let (shown, heard) = Shown::with_no_tray(session);
        let numbers = shown.numbers();
        let call = |method: &str, arguments: glib::Variant| {
            shown.host.call(&shown.sni, method, arguments).unwrap();
        };

        call("ProvideXdgActivationToken", ("token-1",).to_variant());
        call("Activate", (12i32, 34i32).to_variant());
        call("Activate", (12i32, 34i32).to_variant());
        call("SecondaryActivate", (12i32, 34i32).to_variant());
        call("Scroll", (1i32, "vertical").to_variant());
        call("ProvideXdgActivationToken", ("token-2",).to_variant());
        call("ContextMenu", (12i32, 34i32).to_variant());
        shown.event(numbers[0], "clicked").unwrap();

        assert_eq!(
            heard.take(),
            [
                Heard::Activated {
                    token: Some("token-1".into()),
                },
                Heard::Activated { token: None },
                Heard::Clicked {
                    id: "open".into(),
                    token: Some("token-2".into()),
                },
            ]
        );
    });
}

fn found(id: &str, name: &str) -> Found {
    Found {
        id: id.into(),
        name: name.into(),
        description: format!("Switch to {name}"),
        icon: "com.usepitboard.Pitboard".into(),
    }
}

// The model's answers for the terms the tests search for.
fn answers(terms: &[String]) -> Vec<Found> {
    let terms: Vec<&str> = terms.iter().map(String::as_str).collect();
    match terms.as_slice() {
        ["wo"] => vec![
            found("switch/work", "work"),
            found("switch/workshop", "workshop"),
        ],
        ["work"] => vec![found("switch/work", "work")],
        ["ho"] => vec![found("switch/home", "home")],
        _ => Vec::new(),
    }
}

type Matched = (String, String, String, i32, f64, Properties);

// GNOME's search provider and KRunner's runner on the app's connection, as the app serves both,
// with what each was asked and told.
struct Searched {
    _provider: Provider,
    _runner: Runner,
    host: Host,
    shell: At,
    krunner: At,
    terms: Rc<RefCell<Vec<Vec<String>>>>,
    asked: Rc<RefCell<Vec<Asked>>>,
    runs: Rc<RefCell<Vec<Run>>>,
}

impl Searched {
    fn new(session: &Session) -> Searched {
        let app = session.connect();
        let terms = Rc::new(RefCell::new(Vec::new()));
        let results = |terms: &Rc<RefCell<Vec<Vec<String>>>>| {
            let terms = terms.clone();
            move |asked: &[String]| {
                terms.borrow_mut().push(asked.to_vec());
                answers(asked)
            }
        };
        let (asked, ask) = collected();
        let (runs, run) = collected();
        let at = |path: String, interface| At {
            name: app.unique_name().unwrap().into(),
            path,
            interface,
        };
        Searched {
            _provider: Provider::serve(&app, APP, results(&terms), ask).unwrap(),
            _runner: Runner::serve(&app, APP, results(&terms), run).unwrap(),
            host: Host::new(session.connect()),
            shell: at(search::path(APP), "org.gnome.Shell.SearchProvider2"),
            krunner: at(krunner::path(APP), "org.kde.krunner1"),
            terms,
            asked,
            runs,
        }
    }

    fn shell(&self, method: &str, arguments: glib::Variant) -> glib::Variant {
        self.host
            .call(&self.shell, method, arguments)
            .unwrap_or_else(|e| panic!("{method}: {e}"))
    }

    fn krunner(&self, method: &str, arguments: glib::Variant) -> glib::Variant {
        self.host
            .call(&self.krunner, method, arguments)
            .unwrap_or_else(|e| panic!("{method}: {e}"))
    }

    fn initial(&self, terms: &[&str]) -> Vec<String> {
        let reply = self.shell("GetInitialResultSet", (terms,).to_variant());
        reply.get::<(Vec<String>,)>().unwrap().0
    }

    fn subsearch(&self, previous: &[&str], terms: &[&str]) -> Vec<String> {
        let reply = self.shell("GetSubsearchResultSet", (previous, terms).to_variant());
        reply.get::<(Vec<String>,)>().unwrap().0
    }

    fn activate(&self, id: &str) {
        self.shell("ActivateResult", (id, vec!["wo"], 0u32).to_variant());
    }

    fn matched(&self, query: &str) -> Vec<Matched> {
        let reply = self.krunner("Match", (query,).to_variant());
        reply.get::<(Vec<Matched>,)>().unwrap().0
    }

    fn run(&self, id: &str, action: &str) {
        self.krunner("Run", (id, action).to_variant());
    }
}

#[test]
fn the_initial_results_are_what_the_results_answer_for_the_terms() {
    on_a_bus(None, |session| {
        let searched = Searched::new(session);

        assert_eq!(
            searched.initial(&["wo"]),
            ["switch/work", "switch/workshop"]
        );
        assert_eq!(
            searched.subsearch(&["switch/work", "switch/workshop"], &["work"]),
            ["switch/work"]
        );
        // A subsearch answers no more than the results it narrows.
        assert!(
            searched
                .subsearch(&["switch/workshop"], &["work"])
                .is_empty()
        );
        assert_eq!(
            searched.terms.take(),
            [vec!["wo"], vec!["work"], vec!["work"]]
        );
    });
}

#[test]
fn metas_carry_name_description_and_icon() {
    on_a_bus(None, |session| {
        let searched = Searched::new(session);
        searched.initial(&["wo"]);

        let reply = searched.shell(
            "GetResultMetas",
            (vec!["switch/workshop", "switch/never"],).to_variant(),
        );
        let (metas,) = reply.get::<(Vec<Properties>,)>().unwrap();
        assert_eq!(metas.len(), 1, "{metas:?}");
        let meta = &metas[0];
        assert_eq!(
            meta.keys().map(String::as_str).collect::<Vec<_>>(),
            ["description", "icon", "id", "name"]
        );
        assert_eq!(meta["id"], "switch/workshop".to_variant());
        assert_eq!(meta["name"], "workshop".to_variant());
        assert_eq!(meta["description"], "Switch to workshop".to_variant());
        // As GNOME Shell 48.7 reads it (`js/ui/remoteSearch.js:225-226`).
        let icon = gio::Icon::deserialize(&meta["icon"]).unwrap();
        let themed = icon.downcast::<gio::ThemedIcon>().unwrap();
        assert!(
            themed
                .names()
                .iter()
                .any(|name| name == "com.usepitboard.Pitboard"),
            "{:?}",
            themed.names()
        );
    });
}

#[test]
fn activating_a_result_tells_its_id() {
    on_a_bus(None, |session| {
        let searched = Searched::new(session);
        searched.initial(&["wo"]);

        searched.activate("switch/workshop");

        assert_eq!(
            searched.asked.take(),
            [Asked::Activate("switch/workshop".into())]
        );
    });
}

#[test]
fn launching_a_search_opens_the_window() {
    on_a_bus(None, |session| {
        let searched = Searched::new(session);

        searched.shell("LaunchSearch", (vec!["wo"], 0u32).to_variant());

        assert_eq!(searched.asked.take(), [Asked::Open]);
    });
}

// Each search hands on an id only it gave, and only until the next search begins.
#[test]
fn an_id_never_given_activates_nothing() {
    on_a_bus(None, |session| {
        let searched = Searched::new(session);

        searched.activate("switch/work");
        searched.initial(&["wo"]);
        searched.initial(&["ho"]);
        searched.activate("switch/work");
        searched.run("switch/home", "");
        searched.matched("wo");
        searched.krunner("Teardown", ().to_variant());
        searched.matched("ho");
        searched.run("switch/work", "");

        assert!(searched.asked.borrow().is_empty(), "{:?}", searched.asked);
        assert!(searched.runs.borrow().is_empty(), "{:?}", searched.runs);
    });
}

#[test]
fn a_match_answers_the_results_with_their_relevance() {
    on_a_bus(None, |session| {
        let searched = Searched::new(session);
        let matched = |id: &str, text: &str, relevance: f64| {
            let subtext = properties([("subtext", format!("Switch to {text}").to_variant())]);
            // The fourth is the category's relevance, KRunner 6's Moderate.
            let icon = "com.usepitboard.Pitboard";
            (id.into(), text.into(), icon.into(), 50, relevance, subtext)
        };

        assert_eq!(
            searched.matched("wo"),
            [
                matched("switch/work", "work", 1.0),
                matched("switch/workshop", "workshop", 0.5),
            ]
        );
        assert!(searched.matched("work  home").is_empty());
        assert_eq!(
            searched.terms.take(),
            [vec!["wo"], vec!["work", "home"]],
            "KRunner's query is the terms GNOME Shell would give"
        );
        let actions = searched.krunner("Actions", ().to_variant());
        assert_eq!(
            actions,
            (Vec::<(String, String, String)>::new(),).to_variant()
        );
        let config = searched.krunner("Config", ().to_variant());
        assert_eq!(config, (Properties::new(),).to_variant());
    });
}

#[test]
fn run_tells_the_id_after_its_token() {
    on_a_bus(None, |session| {
        let searched = Searched::new(session);
        searched.matched("wo");

        searched.krunner("SetActivationToken", ("token",).to_variant());
        searched.run("switch/work", "");
        searched.run("switch/workshop", "");
        searched.run("switch/work", "an-action");

        assert_eq!(
            searched.runs.take(),
            [
                Run {
                    id: "switch/work".into(),
                    token: Some("token".into()),
                },
                Run {
                    id: "switch/workshop".into(),
                    token: None,
                },
            ]
        );
    });
}

// KRunner tears its session down before it runs a match (docs/facts/linux-app.md, Search and
// KRunner).
#[test]
fn a_run_after_teardown_tells_the_id_with_the_token_set_after_it() {
    on_a_bus(None, |session| {
        let searched = Searched::new(session);
        searched.matched("w");
        searched.matched("wo");

        searched.krunner("Teardown", ().to_variant());
        searched.krunner("SetActivationToken", ("token",).to_variant());
        searched.run("switch/workshop", "");

        assert_eq!(
            searched.runs.take(),
            [Run {
                id: "switch/workshop".into(),
                token: Some("token".into()),
            }]
        );
    });
}
