//! A fixture's stand-ins for the desktop's services. Each writes what the desktop's own would
//! have done, a JSON object a line, in a file of its own under `<fixture folder>/linux/`, where
//! a UI test reads it: in a fixture, the app starts nothing at sign-in, posts nothing to the
//! shell, shows nothing in a tray and opens no browser.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::Failed;
use super::autostart::StartAtSignIn;
use super::links::Opener;
use super::notifications::{Notice, Notifier};
use super::tray::{self, StatusItem, TrayItem};

/// The fixture's folder the stand-ins write in.
pub const FOLDER: &str = "linux";

struct Lines(PathBuf);

impl Lines {
    fn in_fixture(fixture: &Path, file: &str) -> Lines {
        Lines(fixture.join(FOLDER).join(file))
    }

    // One write of the whole line, which O_APPEND puts after every other.
    fn write(&self, line: &impl Serialize) -> Result<(), Failed> {
        let failed = |e: &dyn std::fmt::Display| Failed(format!("{}: {e}", self.0.display()));
        let mut text = serde_json::to_string(line).map_err(|e| failed(&e))?;
        text.push('\n');
        if let Some(folder) = self.0.parent() {
            std::fs::create_dir_all(folder).map_err(|e| failed(&e))?;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.0)
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .map_err(|e| failed(&e))
    }

    fn last<T: DeserializeOwned>(&self) -> Option<T> {
        let text = std::fs::read_to_string(&self.0).ok()?;
        serde_json::from_str(text.lines().last()?).ok()
    }
}

#[derive(Serialize, Deserialize)]
struct Switched {
    on: bool,
}

/// `autostart.jsonl`: `{"on":true}` or `{"on":false}` for each change, the last of which it
/// reads as whether it is on.
pub struct Autostart(Lines);

impl Autostart {
    pub fn new(fixture: &Path) -> Autostart {
        Autostart(Lines::in_fixture(fixture, "autostart.jsonl"))
    }
}

impl StartAtSignIn for Autostart {
    fn is_on(&self) -> bool {
        self.0
            .last::<Switched>()
            .is_some_and(|switched| switched.on)
    }

    fn turn_on(&self) -> Result<(), Failed> {
        self.0.write(&Switched { on: true })
    }

    fn turn_off(&self) -> Result<bool, Failed> {
        if !self.is_on() {
            return Ok(false);
        }
        self.0.write(&Switched { on: false }).map(|()| true)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Posted<'a> {
    Post(&'a Notice),
    Withdraw(&'a str),
}

/// `notifications.jsonl`: `{"post":<notice>}` and `{"withdraw":<id>}`.
pub struct Notifications(Lines);

impl Notifications {
    pub fn new(fixture: &Path) -> Notifications {
        Notifications(Lines::in_fixture(fixture, "notifications.jsonl"))
    }
}

impl Notifier for Notifications {
    fn post(&self, notice: Notice) -> Result<(), Failed> {
        self.0.write(&Posted::Post(&notice))
    }

    fn withdraw(&self, id: String) -> Result<(), Failed> {
        self.0.write(&Posted::Withdraw(&id))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Shown<'a> {
    Show(&'a TrayItem),
    Hide,
}

/// `tray.jsonl`: `{"show":<item>}` and `"hide"`.
pub struct Tray(Lines);

impl Tray {
    pub fn new(fixture: &Path) -> Tray {
        Tray(Lines::in_fixture(fixture, "tray.jsonl"))
    }

    /// Tells `told` that a tray shows the item while `org.kde.StatusNotifierWatcher` has an
    /// owner on `connection`, as a UI test's stand-in watcher owns it there, and that none does
    /// while it has none: what the desktop's item tells, with nothing registered with the
    /// watcher. Until dropped.
    pub fn hear(connection: &gio::DBusConnection, told: impl Fn(tray::Heard) + 'static) -> Hearing {
        let told = std::rc::Rc::new(told);
        let gone = told.clone();
        let watcher = gio::bus_watch_name_on_connection(
            connection,
            tray::WATCHER,
            gio::BusNameWatcherFlags::NONE,
            move |_, _, _| told(tray::Heard::Tray),
            move |_, _| gone(tray::Heard::NoTray),
        );
        Hearing(Some(Box::new(move || gio::bus_unwatch_name(watcher))))
    }
}

/// The watch [`Tray::hear`] keeps.
// gio 0.22 exports another `WatcherId` than the one its name watch returns.
pub struct Hearing(Option<Box<dyn FnOnce()>>);

impl Drop for Hearing {
    fn drop(&mut self) {
        if let Some(unwatch) = self.0.take() {
            unwatch();
        }
    }
}

impl StatusItem for Tray {
    fn show(&self, item: &TrayItem) -> Result<(), Failed> {
        self.0.write(&Shown::Show(item))
    }

    fn hide(&self) -> Result<(), Failed> {
        self.0.write(&Shown::Hide)
    }
}

#[derive(Serialize)]
struct Opened<'a> {
    open: &'a str,
}

/// `browser.jsonl`: `{"open":<link>}`.
pub struct Browser(Lines);

impl Browser {
    pub fn new(fixture: &Path) -> Browser {
        Browser(Lines::in_fixture(fixture, "browser.jsonl"))
    }
}

impl Opener for Browser {
    fn open(&self, link: &str, _parent: Option<&gtk::Window>) -> Result<(), Failed> {
        self.0.write(&Opened { open: link })
    }
}
