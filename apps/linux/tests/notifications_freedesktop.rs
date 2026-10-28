//! A notice as GLib's `freedesktop` backend posts it, where nothing owns `org.gtk.Notifications`,
//! to a stand-in `org.freedesktop.Notifications` on a bus of the test's own, which clicks it.
//! GLib picks a backend once for the process, so each backend's tests are a program of their
//! own.
#![cfg(target_os = "linux")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gio::prelude::*;
use pitboard_linux::config::APP_ID;
use pitboard_linux::platform::notifications::{
    Button, CLICKED, Desktop, Notice, Notifier, Priority,
};

// GNOME Shell 48.7's `data/dbus-interfaces/org.freedesktop.Notifications.xml`, the methods
// GLib calls.
const SERVER: &str = r#"
<node>
  <interface name="org.freedesktop.Notifications">
    <method name="Notify">
      <arg type="s" direction="in"/>
      <arg type="u" direction="in"/>
      <arg type="s" direction="in"/>
      <arg type="s" direction="in"/>
      <arg type="s" direction="in"/>
      <arg type="as" direction="in"/>
      <arg type="a{sv}" direction="in"/>
      <arg type="i" direction="in"/>
      <arg type="u" direction="out"/>
    </method>
    <method name="CloseNotification">
      <arg type="u" direction="in"/>
    </method>
    <signal name="ActionInvoked">
      <arg type="u"/>
      <arg type="s"/>
    </signal>
  </interface>
</node>"#;

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

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

fn own(connection: &gio::DBusConnection, name: &str) {
    let reply = connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(name, 4u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .unwrap();
    assert_eq!(reply.get::<(u32,)>(), Some((1,)), "{name} was not owned");
}

// Runs the app on the main context until it quits, for ten seconds at most. A run's end, not
// a drop, lets go of the app's connection to the bus.
fn run(app: &gio::Application) {
    let timed_out = Rc::new(Cell::new(false));
    let timeout = glib::timeout_add_seconds_local_once(10, {
        let (timed_out, app) = (timed_out.clone(), app.clone());
        move || {
            timed_out.set(true);
            app.quit();
        }
    });
    let _held = app.hold();
    app.run_with_args(&["pitboard-app"]);
    assert!(!timed_out.get(), "not done in ten seconds");
    timeout.remove();
}

// What the server was asked to show: the actions as `Notify` lists them, a key then a label,
// and the hints.
struct Shown {
    actions: Vec<String>,
    hints: glib::VariantDict,
}

#[test]
fn a_click_on_a_freedesktop_server_reaches_the_notices_action_with_its_argument() {
    let test_bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);
    test_bus.up();
    let server = connect(&test_bus.bus_address().unwrap());
    let shown = Rc::new(RefCell::new(Vec::new()));
    // The server clicks the first notice as it shows it, and the second's button.
    let interface = gio::DBusNodeInfo::for_xml(SERVER)
        .unwrap()
        .lookup_interface(NAME)
        .unwrap();
    let registration = server
        .register_object(PATH, &interface)
        .method_call({
            let shown = shown.clone();
            move |server, _, _, _, method, parameters, invocation| {
                assert_eq!(method, "Notify");
                let actions: Vec<String> = parameters.child_get(5);
                let hints = glib::VariantDict::new(Some(&parameters.child_value(6)));
                let id = shown.borrow().len() as u32 + 7;
                let click = if id == 7 { "default" } else { &actions[2] }.to_owned();
                shown.borrow_mut().push(Shown { actions, hints });
                invocation.return_value(Some(&(id,).to_variant()));
                // After the answer, as a person clicks one only once it shows.
                server
                    .emit_signal(
                        None,
                        PATH,
                        NAME,
                        "ActionInvoked",
                        Some(&(id, click).to_variant()),
                    )
                    .unwrap();
            }
        })
        .build()
        .unwrap();
    own(&server, NAME);
    let app = gio::Application::new(Some(APP_ID), gio::ApplicationFlags::empty());
    let desktop = Rc::new(Desktop::new(&app));
    let notice = Notice {
        id: "replaced/work".into(),
        title: "work signed in elsewhere".into(),
        body: "Pitboard keeps the login it had.".into(),
        priority: Priority::Normal,
        clicked: "open/work".into(),
        buttons: vec![Button {
            title: "Sign In Again".into(),
            clicked: "sign-in/work".into(),
        }],
    };
    let clicked = Rc::new(RefCell::new(Vec::new()));
    let action = gio::SimpleAction::new(CLICKED, Some(glib::VariantTy::STRING));
    action.connect_activate({
        let (clicked, desktop, notice) = (clicked.clone(), desktop.clone(), notice.clone());
        let app = app.downgrade();
        move |_, argument| {
            let argument = argument.and_then(|argument| argument.str());
            clicked.borrow_mut().push(argument.map(String::from));
            match (clicked.borrow().len(), app.upgrade()) {
                (1, _) => desktop.post(notice.clone()).unwrap(),
                (_, Some(app)) => app.quit(),
                (_, None) => {}
            }
        }
    });
    app.add_action(&action);
    app.connect_activate(move |_| desktop.post(notice.clone()).unwrap());

    run(&app);
    server.unregister_object(registration).unwrap();
    server.close_sync(gio::Cancellable::NONE).unwrap();
    drop(app);
    test_bus.down();

    let (shown, clicked) = (shown.take(), clicked.take());
    assert_eq!(
        clicked,
        [
            Some("open/work".to_owned()),
            Some("sign-in/work".to_owned())
        ]
    );
    assert_eq!(
        shown[0].actions,
        [
            "default",
            "",
            "app.notice-clicked('sign-in/work')",
            "Sign In Again"
        ]
    );
    let entry = shown[0].hints.lookup::<String>("desktop-entry").unwrap();
    assert_eq!(entry.as_deref(), Some(APP_ID));
}
