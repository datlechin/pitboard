//! The built program launched into a fixture as somebody would start it from a terminal, with
//! homes of the launch's own, on the private bus and the headless display `headless.sh` gives the
//! tests: what it exports, read over D-Bus, and what it shows, found and pressed over AT-SPI.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::{AccessibilityConnection, ObjectRefOwned, RelationType, Role, State};
use futures_lite::future::block_on;
use gio::prelude::*;
use pitboard_linux::config::{APP_ID, OBJECT_PATH};

/// How long anything a test waits for may take.
pub(crate) const PATIENCE: Duration = Duration::from_secs(30);

/// Each launch's own homes, every one of which a fixture leaves as it found it.
pub(crate) const HOMES: [&str; 9] = [
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "TMPDIR",
    "PITBOARD_HOME",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
];

// What headless.sh gives the tests, which a launch passes on: the display, its private bus, and
// the settings and renderer the script names. Nothing else of the test's environment.
const PASSED: [&str; 8] = [
    "PATH",
    "LANG",
    "XDG_RUNTIME_DIR",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "GSETTINGS_BACKEND",
    "GDK_BACKEND",
    "GSK_RENDERER",
];

/// What a launch left in its home `home`, beside what the GL driver keeps of itself: Mesa keeps
/// its shader cache in `$XDG_CACHE_HOME/mesa_shader_cache` for any program it draws for
/// (docs/facts/linux-app.md, The UI tests), which no line of Pitboard's writes.
pub(crate) fn left_in(home: &Path, name: &str) -> Vec<PathBuf> {
    (std::fs::read_dir(home).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| !(name == "XDG_CACHE_HOME" && path.ends_with("mesa_shader_cache")))
        .collect()
}

/// `what` once `found` finds it, within `PATIENCE`.
pub(crate) fn eventually<T>(what: &str, mut found: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(found) = found() {
            return found;
        }
        assert!(Instant::now() < deadline, "{what}: not within {PATIENCE:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A launch of the program, killed if a test ends while it runs.
pub(crate) struct App {
    child: Child,
    scratch: tempfile::TempDir,
    fixture: String,
}

impl App {
    /// The program in the fixture `fixture`, with `arguments`.
    pub(crate) fn launch(fixture: &str, arguments: &[&str]) -> App {
        let scratch = tempfile::tempdir().unwrap();
        for home in HOMES {
            std::fs::create_dir(scratch.path().join(home)).unwrap();
        }
        let child = program(scratch.path(), fixture, arguments)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        App {
            child,
            scratch,
            fixture: fixture.into(),
        }
    }

    /// Another launch with the same homes, to its end.
    pub(crate) fn again(&self, arguments: &[&str]) -> Output {
        let mut again = program(self.scratch.path(), &self.fixture, arguments);
        let mut child = again
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        eventually("the second launch ends", || child.try_wait().unwrap());
        child.wait_with_output().unwrap()
    }

    pub(crate) fn home(&self, name: &str) -> PathBuf {
        self.scratch.path().join(name)
    }

    /// Where the fixture's world is: `pitboard-fixture` in the launch's temporary folder.
    pub(crate) fn fixture(&self) -> PathBuf {
        self.home("TMPDIR").join("pitboard-fixture")
    }

    /// What a stand-in wrote in the fixture's folder, a JSON value a line, none while nothing.
    pub(crate) fn stand_in(&self, file: &str) -> Vec<serde_json::Value> {
        let path = self.fixture().join("linux").join(file);
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// The scratch folder every home is in.
    pub(crate) fn scratch(&self) -> &Path {
        self.scratch.path()
    }

    pub(crate) fn ended(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().unwrap()
    }

    pub(crate) fn ends(&mut self, what: &str) -> ExitStatus {
        eventually(what, || self.ended())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "a launch passes on what headless.sh gave the tests, and nothing else"
)]
fn passed() -> Vec<(&'static str, OsString)> {
    PASSED
        .iter()
        .filter_map(|name| Some((*name, std::env::var_os(name)?)))
        .collect()
}

fn program(scratch: &Path, fixture: &str, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pitboard-app"));
    command
        .args(arguments)
        .env_clear()
        .envs(passed())
        .envs(HOMES.map(|home| (home, scratch.join(home))))
        .env("PITBOARD_FIXTURE", fixture);
    command
}

/// The private session bus the tests run on.
pub(crate) fn session() -> gio::DBusConnection {
    gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap()
}

fn call(
    bus: &gio::DBusConnection,
    destination: &str,
    path: &str,
    interface: &str,
    method: &str,
    arguments: Option<&glib::Variant>,
) -> Result<glib::Variant, glib::Error> {
    bus.call_sync(
        Some(destination),
        path,
        interface,
        method,
        arguments,
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
    )
}

fn bus_call(bus: &gio::DBusConnection, method: &str, name: &str) -> glib::Variant {
    let (bus_name, path) = ("org.freedesktop.DBus", "/org/freedesktop/DBus");
    call(
        bus,
        bus_name,
        path,
        bus_name,
        method,
        Some(&(name,).to_variant()),
    )
    .unwrap_or_else(|e| panic!("{method}: {e}"))
}

pub(crate) fn has_owner(bus: &gio::DBusConnection, name: &str) -> bool {
    bus_call(bus, "NameHasOwner", name)
        .get::<(bool,)>()
        .unwrap()
        .0
}

/// Waits for the program to own its name, as it does once registered.
pub(crate) fn owned(bus: &gio::DBusConnection) {
    eventually("the app owns its name", || {
        has_owner(bus, APP_ID).then_some(())
    });
}

/// What `org.gtk.Actions` exports at `path`: each action's name, and the type it takes.
pub(crate) fn described(bus: &gio::DBusConnection, path: &str) -> BTreeMap<String, String> {
    let described = call(bus, APP_ID, path, "org.gtk.Actions", "DescribeAll", None)
        .unwrap_or_else(|e| panic!("DescribeAll at {path}: {e}"));
    let (actions,) = described
        .get::<(BTreeMap<String, (bool, String, Vec<glib::Variant>)>,)>()
        .expect("a{s(bgav)}");
    (actions.into_iter())
        .map(|(name, (_, parameter, _))| (name, parameter))
        .collect()
}

/// Activates the app's action `name` over the bus, as GNOME Shell does a launcher's action.
pub(crate) fn activate(bus: &gio::DBusConnection, name: &str, parameter: Option<glib::Variant>) {
    let parameters: Vec<glib::Variant> = parameter.into_iter().collect();
    let platform = std::collections::HashMap::<String, glib::Variant>::new();
    let arguments = (name, parameters, platform).to_variant();
    call(
        bus,
        APP_ID,
        OBJECT_PATH,
        "org.freedesktop.Application",
        "ActivateAction",
        Some(&arguments),
    )
    .unwrap_or_else(|e| panic!("ActivateAction {name}: {e}"));
}

/// Every object path under the app's own that exports `interface`, its own included.
pub(crate) fn exporting(bus: &gio::DBusConnection, interface: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut paths = vec![OBJECT_PATH.to_owned()];
    while let Some(path) = paths.pop() {
        let xml = call(
            bus,
            APP_ID,
            &path,
            "org.freedesktop.DBus.Introspectable",
            "Introspect",
            None,
        )
        .unwrap_or_else(|e| panic!("Introspect {path}: {e}"))
        .get::<(String,)>()
        .unwrap()
        .0;
        let node = gio::DBusNodeInfo::for_xml(&xml).unwrap();
        if node.lookup_interface(interface).is_some() {
            found.push(path.clone());
        }
        for child in node.nodes() {
            let name = child.path().expect("a child node is named");
            paths.push(format!("{}/{name}", path.trim_end_matches('/')));
        }
    }
    found.sort();
    found
}

/// A name of the test's own on the bus, until dropped: a stand-in for a tray's watcher.
pub(crate) struct Owned<'a> {
    bus: &'a gio::DBusConnection,
    name: &'static str,
}

impl<'a> Owned<'a> {
    pub(crate) fn new(bus: &'a gio::DBusConnection, name: &'static str) -> Owned<'a> {
        let reply = call(
            bus,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(name, 4u32).to_variant()),
        )
        .unwrap();
        assert_eq!(reply.get::<(u32,)>(), Some((1,)), "{name} was not owned");
        Owned { bus, name }
    }
}

impl Drop for Owned<'_> {
    fn drop(&mut self) {
        let _ = bus_call(self.bus, "ReleaseName", self.name);
    }
}

/// What an object on the screen is, as AT-SPI says it.
#[derive(Debug, Clone)]
pub(crate) struct Seen {
    pub(crate) at: ObjectRefOwned,
    pub(crate) name: String,
    pub(crate) role: Role,
    // the id GTK 4.22 gives it from its GtkBuilder id
    pub(crate) id: String,
    pub(crate) showing: bool,
}

/// The screen, over AT-SPI's own bus.
pub(crate) struct Screen {
    connection: AccessibilityConnection,
}

impl Screen {
    pub(crate) fn new() -> Screen {
        Screen {
            connection: block_on(AccessibilityConnection::new()).expect("the AT-SPI bus"),
        }
    }

    fn proxy(&self, at: &ObjectRefOwned) -> Option<AccessibleProxy<'_>> {
        block_on(
            at.clone()
                .into_accessible_proxy(self.connection.connection()),
        )
        .ok()
    }

    fn seen(&self, at: &ObjectRefOwned) -> Option<Seen> {
        let proxy = self.proxy(at)?;
        block_on(async {
            let mut name = proxy.name().await.ok()?;
            if name.is_empty() {
                name = self.labelled_by(&proxy).await;
            }
            Some(Seen {
                at: at.clone(),
                name,
                role: proxy.get_role().await.ok()?,
                id: proxy.accessible_id().await.unwrap_or_default(),
                showing: proxy.get_state().await.ok()?.contains(State::Showing),
            })
        })
    }

    // What a screen reader reads of an object GTK names by a label of its own, as it names a
    // popover menu's item: the text of each object it is labelled by (docs/facts/linux-app.md,
    // The UI tests).
    async fn labelled_by(&self, proxy: &AccessibleProxy<'_>) -> String {
        let mut said = Vec::new();
        let relations = proxy.get_relation_set().await.unwrap_or_default();
        let labels = (relations.into_iter())
            .filter(|(relation, _)| *relation == RelationType::LabelledBy)
            .flat_map(|(_, labels)| labels);
        for label in labels {
            let Ok(label) = label
                .into_accessible_proxy(self.connection.connection())
                .await
            else {
                continue;
            };
            if let Ok(proxies) = label.proxies().await
                && let Ok(text) = proxies.text().await
                && let Ok(text) = text.get_text(0, -1).await
            {
                said.push(text);
            }
        }
        said.join(" ")
    }

    fn children(&self, at: &ObjectRefOwned) -> Vec<ObjectRefOwned> {
        let Some(proxy) = self.proxy(at) else {
            return Vec::new();
        };
        block_on(proxy.get_children()).unwrap_or_default()
    }

    /// The app, as GTK 4.22 names its root: by its program's name, whatever the application's
    /// name is (docs/facts/linux-app.md, The UI tests).
    pub(crate) fn app(&self) -> Option<Seen> {
        let root = block_on(self.connection.root_accessible_on_registry()).ok()?;
        let apps = block_on(root.get_children()).ok()?;
        (apps.iter())
            .filter_map(|app| self.seen(app))
            .find(|app| app.name == "pitboard-app")
    }

    /// Every app on the screen, with its windows and whether each shows, for a test that fails.
    pub(crate) fn described(&self) -> String {
        let Ok(root) = block_on(self.connection.root_accessible_on_registry()) else {
            return "no registry".into();
        };
        let apps = block_on(root.get_children()).unwrap_or_default();
        (apps.iter())
            .filter_map(|app| self.seen(app))
            .map(|app| {
                let windows: Vec<_> = (self.children(&app.at).iter())
                    .filter_map(|window| self.seen(window))
                    .map(|window| {
                        format!(
                            "{:?} {:?} showing {}",
                            window.name, window.role, window.showing
                        )
                    })
                    .collect();
                format!("{:?}: {windows:?}", app.name)
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Every object under `under`, a line each, indented by its depth, for a test that fails.
    pub(crate) fn tree(&self, under: &Seen) -> String {
        let mut lines = Vec::new();
        let mut next = vec![(under.at.clone(), 0)];
        while let Some((at, depth)) = next.pop() {
            if let Some(seen) = self.seen(&at) {
                lines.push(format!(
                    "{}{:?} {:?} #{} showing {}",
                    "  ".repeat(depth),
                    seen.role,
                    seen.name,
                    seen.id,
                    seen.showing
                ));
            }
            for child in self.children(&at).into_iter().rev() {
                next.push((child, depth + 1));
            }
        }
        lines.join("\n")
    }

    /// The app's first window, once it shows one.
    pub(crate) fn window(&self) -> Seen {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(window) = self.windows().into_iter().next() {
                return window;
            }
            assert!(
                Instant::now() < deadline,
                "no window within {PATIENCE:?}: {}",
                self.described()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The app's windows on the screen.
    pub(crate) fn windows(&self) -> Vec<Seen> {
        let Some(app) = self.app() else {
            return Vec::new();
        };
        (self.children(&app.at).iter())
            .filter_map(|window| self.seen(window))
            .filter(|window| window.role == Role::Frame && window.showing)
            .collect()
    }

    /// The first object under `under`, breadth first, that `matches`.
    pub(crate) fn find(&self, under: &Seen, matches: impl Fn(&Seen) -> bool) -> Option<Seen> {
        let mut next = std::collections::VecDeque::from([under.at.clone()]);
        while let Some(at) = next.pop_front() {
            for child in self.children(&at) {
                if let Some(seen) = self.seen(&child) {
                    if matches(&seen) {
                        return Some(seen);
                    }
                    next.push_back(child);
                }
            }
        }
        None
    }

    /// Every one of the app's objects that `matches`, in the order of its tree.
    pub(crate) fn every(&self, matches: impl Fn(&Seen) -> bool) -> Vec<Seen> {
        let mut found = Vec::new();
        let Some(app) = self.app() else {
            return found;
        };
        let mut next = self.children(&app.at);
        next.reverse();
        while let Some(at) = next.pop() {
            if let Some(seen) = self.seen(&at)
                && matches(&seen)
            {
                found.push(seen);
            }
            next.extend(self.children(&at).into_iter().rev());
        }
        found
    }

    /// The first of the app's objects that `matches`, once it shows one: the app's whole tree
    /// said where none does.
    pub(crate) fn waits_for(&self, what: &str, matches: impl Fn(&Seen) -> bool) -> Seen {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let app = self.app();
            if let Some(found) = app.as_ref().and_then(|app| self.find(app, &matches)) {
                return found;
            }
            if Instant::now() >= deadline {
                let tree = app.map_or_else(|| self.described(), |app| self.tree(&app));
                panic!("{what}: not within {PATIENCE:?}:\n{tree}");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The object its GtkBuilder id names.
    pub(crate) fn by_id(&self, id: &str) -> Seen {
        self.waits_for(id, |seen| seen.id == id)
    }

    /// The main menu's button, which GTK's menu button holds: what a click opens the menu by.
    pub(crate) fn main_menu(&self) -> Seen {
        let menu = self.by_id("main_menu");
        self.find(&menu, |seen| seen.role == Role::ToggleButton)
            .expect("a menu button holds a toggle button")
    }

    /// Does what an object's action named `action` does, as a screen reader's user would: by
    /// the name GTK gives it whatever the locale, as `GetName` says it.
    pub(crate) fn press(&self, seen: &Seen, action: &str) {
        let proxy = self.proxy(&seen.at).expect("still on the screen");
        block_on(async {
            let actions = proxy.proxies().await.unwrap().action().await.unwrap();
            let mut offered = Vec::new();
            for index in 0..actions.n_actions().await.unwrap() {
                offered.push(actions.get_name(index).await.unwrap());
            }
            let index = (offered.iter())
                .position(|offered| offered == action)
                .unwrap_or_else(|| panic!("{} offers no {action}: {offered:?}", seen.name));
            let index = i32::try_from(index).unwrap();
            assert!(actions.do_action(index).await.unwrap(), "{action} refused");
        });
    }
}
