//! A notice as GLib's `gtk` backend posts it to GNOME Shell's `org.gtk.Notifications`, read by a
//! stand-in for that server on a bus of the test's own. GLib picks a backend once for the
//! process, so each backend's tests are a program of their own.
#![cfg(target_os = "linux")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gio::prelude::*;
use pitboard_linux::config::APP_ID;
use pitboard_linux::platform::notifications::{Button, Desktop, Notice, Notifier};

// GNOME Shell 48.7's `data/dbus-interfaces/org.gtk.Notifications.xml`.
const SERVER: &str = r#"
<node>
  <interface name="org.gtk.Notifications">
    <method name="AddNotification">
      <arg name="app_id" type="s" direction="in"/>
      <arg name="id" type="s" direction="in"/>
      <arg name="notification" type="a{sv}" direction="in"/>
    </method>
    <method name="RemoveNotification">
      <arg name="app_id" type="s" direction="in"/>
      <arg name="id" type="s" direction="in"/>
    </method>
  </interface>
</node>"#;

const NAME: &str = "org.gtk.Notifications";

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

fn notice() -> Notice {
    Notice {
        id: "run-out/work".into(),
        title: "work is out until 3:45 PM".into(),
        body: "Claude Code switched to home.".into(),
        clicked: "open/work".into(),
        button: Some(Button {
            title: "Switch Back".into(),
            clicked: "switch-back/work".into(),
        }),
    }
}

#[test]
fn a_notice_posts_its_default_action_and_button_as_app_actions() {
    let test_bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);
    test_bus.up();
    let server = connect(&test_bus.bus_address().unwrap());
    // On TestDBus's session bus, as GNOME Shell's server is on the session's.
    let app = gio::Application::new(Some(APP_ID), gio::ApplicationFlags::empty());
    let desktop = Rc::new(Desktop::new(&app));
    let calls = Rc::new(RefCell::new(Vec::new()));
    let interface = gio::DBusNodeInfo::for_xml(SERVER)
        .unwrap()
        .lookup_interface(NAME)
        .unwrap();
    let registration = server
        .register_object("/org/gtk/Notifications", &interface)
        .method_call({
            let (calls, app) = (calls.clone(), app.clone());
            move |_, _, _, _, method, parameters, invocation| {
                invocation.return_value(None);
                calls.borrow_mut().push((method.to_owned(), parameters));
                app.quit();
            }
        })
        .build()
        .unwrap();
    own(&server, NAME);
    app.connect_activate(move |_| {
        let desktop: &Desktop = &desktop;
        // From a thread of its own, as the model posts.
        std::thread::scope(|scope| {
            scope.spawn(move || desktop.post(notice()).unwrap());
        });
    });

    run(&app);
    server.unregister_object(registration).unwrap();
    server.close_sync(gio::Cancellable::NONE).unwrap();
    drop(app);
    test_bus.down();

    let calls = calls.take();
    let expected = glib::Variant::parse(
        None,
        &format!(
            "('{APP_ID}', 'run-out/work', {{
              'title': <'work is out until 3:45 PM'>,
              'body': <'Claude Code switched to home.'>,
              'priority': <'normal'>,
              'default-action': <'app.notice-clicked'>,
              'default-action-target': <'open/work'>,
              'buttons': <[
                {{'label': <'Switch Back'>, 'action': <'app.notice-clicked'>,
                  'target': <'switch-back/work'>}}
              ]>
            }})"
        ),
    )
    .unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "AddNotification");
    assert_eq!(calls[0].1, expected, "{}", calls[0].1.print(true));
}
