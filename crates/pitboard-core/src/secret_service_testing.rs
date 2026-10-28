//! The Secret Service a test starts on Linux, on a bus of its own: `dbus-daemon` with a config
//! the test writes, one socket in its scratch folder and no folder of services, so nothing is
//! started on demand, and `gnome-keyring-daemon` with its home, data and runtime folders there
//! too, its login keyring made and unlocked with a password of the test's. Neither is given any
//! variable of this process's but `PATH`, so neither reaches the person's session, and a build
//! for tests reaches no other bus (`host/linux/bus.rs`).
//!
//! It refuses where either program is not installed, naming the package, as the stand-in
//! refuses where it is not built. Each method panics on what a test's own bus should never do.

use secret_service::EncryptionType;
use secret_service::blocking::SecretService as Client;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SECRETS: &str = "org.freedesktop.secrets";
// An alias nothing names, asked after a test's own calls: once the monitor has it, it has them.
const ASKED_SO_FAR: &str = "pitboard-test-asked-so-far";
// How long a daemon has to come up before a test is told it did not.
const STARTED_WITHIN: Duration = Duration::from_secs(20);

/// A Secret Service of a test's own, stopped when dropped.
pub struct SecretService {
    address: String,
    folder: PathBuf,
    bus: Child,
    keyring: Option<Child>,
    asked: Arc<Mutex<Vec<String>>>,
}

/// One item of the default collection, as a search finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub attributes: BTreeMap<String, String>,
    pub secret: String,
}

/// A program the test's Secret Service needs is not installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    program: &'static str,
    package: &'static str,
}

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the Secret Service the tests start needs `{}`, which is not installed: install the \
             `{}` package",
            self.program, self.package
        )
    }
}

impl std::error::Error for Missing {}

impl SecretService {
    /// A bus in `scratch` with gnome-keyring on it, its login keyring the default collection,
    /// unlocked.
    pub fn start(scratch: &Path) -> Result<SecretService, Missing> {
        let mut service = SecretService::without_a_provider(scratch)?;
        service.keyring = Some(service.keyring()?);
        service.wait_for_the_default_collection();
        Ok(service)
    }

    /// A bus in `scratch` with nothing on it, as a session where no keyring runs.
    pub fn without_a_provider(scratch: &Path) -> Result<SecretService, Missing> {
        assert!(
            scratch.is_absolute(),
            "a test named its scratch folder {} relative to wherever it runs",
            scratch.display()
        );
        let folder = scratch.to_path_buf();
        std::fs::create_dir_all(&folder).expect("the Secret Service's folder");
        let config = folder.join("bus.conf");
        std::fs::write(&config, bus_config(&folder.join("bus"))).expect("the bus's config");
        let mut bus = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(log(&folder, "dbus-daemon.log"))
            .spawn()
            .map_err(|e| missing(&e, "dbus-daemon", "dbus-daemon"))?;
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(bus.stdout.take().expect("its output")),
            &mut line,
        )
        .expect("dbus-daemon says where it listens");
        let address = line.trim().to_owned();
        assert!(
            !address.is_empty(),
            "dbus-daemon did not start: {}",
            folder.display()
        );
        let asked = Arc::new(Mutex::new(Vec::new()));
        watch(&address, Arc::clone(&asked));
        Ok(SecretService {
            address,
            folder,
            bus,
            keyring: None,
            asked,
        })
    }

    /// Where the bus is, as `PITBOARD_TEST_BUS` and `Context::with_test_bus` take it.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Locks the default collection, as a screen lock or a sign-out leaves it.
    pub fn lock(&self) {
        let client = self.client();
        client
            .get_default_collection()
            .and_then(|collection| collection.lock())
            .expect("the default collection locks");
    }

    /// Takes the provider off the bus, as a keyring that stopped does.
    pub fn stop(&mut self) {
        if let Some(mut keyring) = self.keyring.take() {
            let _ = keyring.kill();
            let _ = keyring.wait();
        }
        let start = Instant::now();
        while self.owned() {
            assert!(
                start.elapsed() < STARTED_WITHIN,
                "the keyring stays on the bus"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Every item of the default collection, which must be unlocked.
    pub fn items(&self) -> Vec<Item> {
        let client = self.client();
        let collection = client
            .get_default_collection()
            .expect("a default collection");
        let mut items: Vec<Item> = collection
            .get_all_items()
            .expect("its items")
            .iter()
            .map(|item| Item {
                label: item.get_label().expect("its label"),
                attributes: item
                    .get_attributes()
                    .expect("its attributes")
                    .into_iter()
                    .collect(),
                secret: String::from_utf8(item.get_secret().expect("its secret")).expect("text"),
            })
            .collect();
        items.sort_by(|a, b| a.attributes.cmp(&b.attributes));
        items
    }

    /// The `service` of each item of the default collection, which a locked one says too.
    pub fn names(&self) -> Vec<String> {
        let client = self.client();
        let collection = client
            .get_default_collection()
            .expect("a default collection");
        let mut names: Vec<String> = collection
            .get_all_items()
            .expect("its items")
            .iter()
            .filter_map(|item| {
                item.get_attributes()
                    .expect("its attributes")
                    .remove("service")
            })
            .collect();
        names.sort();
        names
    }

    /// An item another program keeps in the default collection.
    pub fn plant(&self, label: &str, attributes: &[(&str, &str)], secret: &str) {
        let client = self.client();
        let collection = client
            .get_default_collection()
            .expect("a default collection");
        collection
            .create_item(
                label,
                attributes.iter().copied().collect::<HashMap<_, _>>(),
                secret.as_bytes(),
                true,
                "text/plain",
            )
            .expect("an item planted");
    }

    /// Each method of the Secret Service's interfaces called on this bus so far, in order.
    pub fn asked(&self) -> Vec<String> {
        let marks = || {
            self.recorded()
                .iter()
                .filter(|m| *m == ASKED_SO_FAR)
                .count()
        };
        let before = marks();
        let _ = zbus::blocking::Proxy::new(
            &self.connection(),
            SECRETS,
            "/org/freedesktop/secrets",
            "org.freedesktop.Secret.Service",
        )
        .and_then(|service| {
            service.call::<_, _, zbus::zvariant::OwnedObjectPath>("ReadAlias", &(ASKED_SO_FAR,))
        });
        let start = Instant::now();
        while marks() == before {
            assert!(
                start.elapsed() < STARTED_WITHIN,
                "the bus's monitor fell behind"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        self.recorded()
            .into_iter()
            .filter(|member| member != ASKED_SO_FAR)
            .collect()
    }

    fn recorded(&self) -> Vec<String> {
        self.asked
            .lock()
            .expect("a poisoned record is a failed test")
            .clone()
    }

    fn keyring(&self) -> Result<Child, Missing> {
        let mut folders = Vec::new();
        for name in ["home", "data", "runtime"] {
            let folder = self.folder.join(name);
            std::fs::create_dir_all(&folder).expect("a keyring folder");
            folders.push(folder);
        }
        crate::host::fs::testing::make_private(&folders[2]).expect("a private runtime folder");
        let mut keyring = Command::new("gnome-keyring-daemon")
            .args(["--foreground", "--components=secrets", "--unlock"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &folders[0])
            .env("XDG_DATA_HOME", &folders[1])
            .env("XDG_RUNTIME_DIR", &folders[2])
            .env("DBUS_SESSION_BUS_ADDRESS", &self.address)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(log(&self.folder, "gnome-keyring-daemon.log"))
            .spawn()
            .map_err(|e| missing(&e, "gnome-keyring-daemon", "gnome-keyring"))?;
        // `--unlock` reads the login keyring's password up to the end of its input.
        let mut input = keyring.stdin.take().expect("its input");
        input
            .write_all(b"pitboard-test")
            .expect("the keyring's password");
        drop(input);
        Ok(keyring)
    }

    fn connection(&self) -> zbus::blocking::Connection {
        zbus::blocking::connection::Builder::address(self.address.as_str())
            .and_then(zbus::blocking::connection::Builder::build)
            .expect("the test's bus answers")
    }

    fn client<'a>(&self) -> Client<'a> {
        Client::connect_with_existing(EncryptionType::Dh, self.connection())
            .expect("the test's Secret Service answers")
    }

    fn owned(&self) -> bool {
        zbus::blocking::fdo::DBusProxy::new(&self.connection())
            .expect("the bus")
            .name_has_owner(SECRETS.try_into().expect("a bus name"))
            .expect("the bus says who owns a name")
    }

    // gnome-keyring takes the name before it has made the login keyring and named it `default`.
    fn wait_for_the_default_collection(&self) {
        let start = Instant::now();
        while !(self.owned() && self.client().get_default_collection().is_ok()) {
            assert!(
                start.elapsed() < STARTED_WITHIN,
                "gnome-keyring made no default collection: {}",
                self.folder.join("gnome-keyring-daemon.log").display()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for SecretService {
    fn drop(&mut self) {
        for child in self
            .keyring
            .iter_mut()
            .chain(std::iter::once(&mut self.bus))
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// A session bus that listens on `socket` alone, lets everyone on it do anything, and has no
// folder of services, so nothing is activated: the name is the keyring's only where it runs.
fn bus_config(socket: &Path) -> String {
    format!(
        r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
        socket.display()
    )
}

fn log(folder: &Path, name: &str) -> Stdio {
    std::fs::File::create(folder.join(name)).map_or_else(|_| Stdio::null(), Stdio::from)
}

fn missing(error: &std::io::Error, program: &'static str, package: &'static str) -> Missing {
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::NotFound,
        "{program} did not start: {error}"
    );
    Missing { program, package }
}

// Each call to the Secret Service's interfaces, recorded by a monitor of the bus as it passes.
fn watch(address: &str, asked: Arc<Mutex<Vec<String>>>) {
    let connection = zbus::blocking::connection::Builder::address(address)
        .and_then(zbus::blocking::connection::Builder::build)
        .expect("the test's bus answers");
    let messages = zbus::blocking::MessageIterator::from(&connection);
    zbus::blocking::fdo::MonitoringProxy::new(&connection)
        .expect("a monitor")
        .become_monitor(&[], 0)
        .expect("the bus lets its own user watch it");
    std::thread::spawn(move || {
        for message in messages.flatten() {
            let header = message.header();
            if header.message_type() != zbus::message::Type::MethodCall {
                continue;
            }
            let (Some(interface), Some(member)) = (header.interface(), header.member()) else {
                continue;
            };
            if !interface.as_str().starts_with("org.freedesktop.Secret.") {
                continue;
            }
            let marked = member.as_str() == "ReadAlias"
                && message.body().deserialize::<&str>().ok() == Some(ASKED_SO_FAR);
            asked
                .lock()
                .expect("a poisoned record is a failed test")
                .push(
                    if marked {
                        ASKED_SO_FAR
                    } else {
                        member.as_str()
                    }
                    .to_owned(),
                );
        }
    });
}
