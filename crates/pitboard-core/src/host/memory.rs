//! A machine in memory, where a test can make anything fail.
//!
//! It fakes what a real host offers and nothing above it, so the code under test is the
//! real one. Before this it faked `read_signin` with a map keyed by directory, which meant
//! no test ever ran Claude Code's own slot hashing on the sign-in path: the double answered
//! the question the code was supposed to answer.

use super::bundle::{self, Bundle, Listed};
use super::{Host, Process, Scheduler};
use crate::context::Context;
use crate::provider::desktop::types::CookieTable;
use crate::store::memory::MemoryStore;
use crate::store::{Backend, Cost, Error, RawStore};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The vault as one context opens it. A real keychain takes `PITBOARD_NO_ARGV` from the
/// context it is opened with, so above its ceiling it writes on the argument line or
/// refuses; this one does the same.
struct Vault {
    store: Arc<MemoryStore>,
    argument_line: bool,
}

impl RawStore for Vault {
    fn kind(&self) -> Backend {
        RawStore::kind(&self.store)
    }

    fn contains(&self, service: &str) -> Result<bool, Error> {
        RawStore::contains(&self.store, service)
    }

    fn read(&self, service: &str) -> Result<Option<String>, Error> {
        RawStore::read(&self.store, service)
    }

    fn write(&self, service: &str, contents: &str) -> Result<(), Error> {
        if let Some(cost) = self.cost(service, contents)
            && cost.refused()
        {
            return Err(Error::Write(format!(
                "this credential is {} bytes, past the {}-byte command limit",
                cost.needs, cost.limit
            )));
        }
        RawStore::write(&self.store, service, contents)
    }

    fn delete(&self, service: &str) -> Result<(), Error> {
        RawStore::delete(&self.store, service)
    }

    fn list(&self) -> Result<Option<Vec<String>>, Error> {
        RawStore::list(&self.store)
    }

    fn cost(&self, service: &str, contents: &str) -> Option<Cost> {
        RawStore::cost(&self.store, service, contents).map(|cost| Cost {
            second_route: self.argument_line,
            ..cost
        })
    }
}

/// A machine whose keychain and filesystem are both in memory, and whose scheduler writes
/// its files in the test's own home and asks no service manager to start them.
#[derive(Debug)]
pub struct MemoryHost {
    keychain: Arc<MemoryStore>,
    vault: Arc<MemoryStore>,
    files: Mutex<HashMap<PathBuf, Arc<MemoryStore>>>,
    running: Mutex<HashMap<String, Vec<Process>>>,
    /// What runs from inside a bundle, as `ps` would list it, every one started by launchd.
    within: Mutex<Vec<Process>>,
    /// Whether the process list cannot be read.
    list_fails: AtomicBool,
    /// Cookie jars, by the file they were planted on: its inode where it exists, so a jar
    /// moved by rename is still the same jar, and its path where it does not.
    cookies: Mutex<HashMap<JarKey, CookieTable>>,
    /// How every jar's read fails, where a test says it does.
    jar_fault: Mutex<Option<std::io::ErrorKind>>,
    /// Devices a test says paths are on, where they are not on the real one.
    devices: Mutex<HashMap<PathBuf, u64>>,
    /// The version each app bundle a test put somewhere says it is.
    versions: Mutex<HashMap<PathBuf, String>>,
    /// Whether every home parks in `vault`, the way every home on macOS parks in the login
    /// keychain. So by default, because that is where the rules about another Pitboard's
    /// parks are needed.
    shared_vault: AtomicBool,
    scheduler: Box<dyn Scheduler>,
    /// Whether the scheduler refuses the next schedule it is asked to start.
    refuse_start: Arc<AtomicBool>,
    /// Whether this machine has no scheduler at all.
    unscheduled: AtomicBool,
}

impl Default for MemoryHost {
    fn default() -> MemoryHost {
        let refuse_start = Arc::new(AtomicBool::new(false));
        MemoryHost {
            // Keychain, because that is the chain the interesting rules are written for.
            keychain: MemoryStore::of(Backend::Keychain),
            vault: MemoryStore::of(Backend::Keychain),
            files: Mutex::new(HashMap::new()),
            running: Mutex::new(HashMap::new()),
            within: Mutex::new(Vec::new()),
            list_fails: AtomicBool::new(false),
            cookies: Mutex::new(HashMap::new()),
            jar_fault: Mutex::new(None),
            devices: Mutex::new(HashMap::new()),
            versions: Mutex::new(HashMap::new()),
            shared_vault: AtomicBool::new(true),
            scheduler: super::os::pretend_scheduler(Arc::clone(&refuse_start)),
            refuse_start,
            unscheduled: AtomicBool::new(false),
        }
    }
}

impl MemoryHost {
    pub fn new() -> Arc<MemoryHost> {
        Arc::new(MemoryHost::default())
    }

    /// The keychain, where a tool that uses one keeps its live credential.
    pub fn live(&self) -> &Arc<MemoryStore> {
        &self.keychain
    }

    /// Where Pitboard's parked logins are.
    pub fn vault(&self) -> &Arc<MemoryStore> {
        &self.vault
    }

    /// The file at `path`, where a tool that keeps its login in a file keeps it.
    pub fn file_at(&self, path: PathBuf) -> Arc<MemoryStore> {
        Arc::clone(
            self.files
                .lock()
                .expect("a poisoned test host is a failed test")
                .entry(path)
                .or_insert_with(|| MemoryStore::of(Backend::File)),
        )
    }

    /// From now on the vault belongs to one home alone, the way Pitboard's vault of files
    /// does off macOS.
    pub fn vault_of_its_own(&self) {
        self.shared_vault.store(false, Ordering::SeqCst);
    }

    /// Say that `count` processes are running `program`, each started by its bare name, the
    /// way a shell starts one from `PATH`.
    pub fn runs(&self, program: &str, count: usize) {
        self.runs_at(program, &vec![program; count]);
    }

    /// Say that a process is running `program` from each of `paths`.
    pub fn runs_at(&self, program: &str, paths: &[&str]) {
        let processes = (1..)
            .zip(paths)
            .map(|(pid, path)| Process {
                pid,
                path: PathBuf::from(path),
            })
            .collect();
        self.running
            .lock()
            .expect("a poisoned test host is a failed test")
            .insert(program.to_string(), processes);
    }

    /// Say that a process is running from `path`, inside whatever bundle that is in, and
    /// answer its pid. The first is 100, apart from the pids [`MemoryHost::runs_at`] gives.
    pub fn runs_within(&self, path: &str) -> u32 {
        let mut within = self
            .within
            .lock()
            .expect("a poisoned test host is a failed test");
        let pid = 100 + u32::try_from(within.len()).expect("a test runs few processes");
        within.push(Process {
            pid,
            path: PathBuf::from(path),
        });
        pid
    }

    /// Everything [`MemoryHost::runs_within`] said was running has quit.
    pub fn quits_within(&self) {
        self.within
            .lock()
            .expect("a poisoned test host is a failed test")
            .clear();
    }

    /// From now on every cookie jar there is fails to read this way, as a jar the app is
    /// writing, or one `sqlite3` gave up waiting on, does.
    pub fn jar_fails(&self, kind: std::io::ErrorKind) {
        *self
            .jar_fault
            .lock()
            .expect("a poisoned test host is a failed test") = Some(kind);
    }

    /// From now on the process list cannot be read.
    pub fn process_list_fails(&self) {
        self.list_fails.store(true, Ordering::SeqCst);
    }

    /// Say that the cookie database at `path` holds `table`. Planted on the file's inode
    /// where it exists, so the jar travels with it when it is moved.
    // `CookieTable` is the crate's own, so a test outside the crate cannot plant one.
    #[allow(dead_code)]
    pub(crate) fn plant_cookies(&self, path: &Path, table: CookieTable) {
        self.cookies
            .lock()
            .expect("a poisoned test host is a failed test")
            .insert(jar_key(path), table);
    }

    /// Say that `path` is on device `dev`.
    pub fn device(&self, path: &Path, dev: u64) {
        self.devices
            .lock()
            .expect("a poisoned test host is a failed test")
            .insert(path.to_path_buf(), dev);
    }

    /// Say that the app bundle at `app` is version `version`.
    pub fn bundle(&self, app: &Path, version: &str) {
        self.versions
            .lock()
            .expect("a poisoned test host is a failed test")
            .insert(app.to_path_buf(), version.to_string());
    }

    /// The next time a schedule is to be started, the system will not start it.
    pub fn refuse_next_start(&self) {
        self.refuse_start.store(true, Ordering::SeqCst);
    }

    /// From now on there is no scheduler here.
    pub fn without_a_scheduler(&self) {
        self.unscheduled.store(true, Ordering::SeqCst);
    }
}

impl Host for MemoryHost {
    fn foreign_secrets(&self, _ctx: &Context, _account: &str) -> Option<Box<dyn RawStore>> {
        Some(Box::new(Arc::clone(&self.keychain)))
    }

    fn file(&self, path: PathBuf) -> Box<dyn RawStore> {
        Box::new(self.file_at(path))
    }

    fn vault(&self, ctx: &Context) -> Box<dyn RawStore> {
        Box::new(Vault {
            store: Arc::clone(&self.vault),
            argument_line: ctx.argv_fallback(),
        })
    }

    fn vault_is_shared(&self) -> bool {
        self.shared_vault.load(Ordering::SeqCst)
    }

    /// What a test said is running, and nothing on the machine running the tests.
    fn processes(&self, program: &str) -> Option<Vec<Process>> {
        Some(
            self.running
                .lock()
                .expect("a poisoned test host is a failed test")
                .get(program)
                .cloned()
                .unwrap_or_default(),
        )
    }

    /// What a test said runs inside a bundle, by the rule the real list is read by, every
    /// process started by launchd.
    fn processes_within(&self, bundle: Bundle<'_>, excluded: &[&str]) -> Option<Vec<Process>> {
        if self.list_fails.load(Ordering::SeqCst) {
            return None;
        }
        let listed: Vec<Listed> = self
            .within
            .lock()
            .expect("a poisoned test host is a failed test")
            .iter()
            .map(|p| Listed {
                pid: p.pid,
                ppid: 1,
                path: p.path.clone(),
            })
            .collect();
        Some(bundle::within(&listed, bundle, excluded))
    }

    /// Whether a test said the process runs, and nothing about the machine's own.
    fn pid_alive(&self, pid: u32) -> bool {
        self.program_of(pid).is_some()
    }

    /// What a test said process `pid` runs, and nothing about the machine's own.
    fn program_of(&self, pid: u32) -> Option<PathBuf> {
        let running = self
            .running
            .lock()
            .expect("a poisoned test host is a failed test")
            .values()
            .flatten()
            .find(|p| p.pid == pid)
            .map(|p| p.path.clone());
        running.or_else(|| {
            self.within
                .lock()
                .expect("a poisoned test host is a failed test")
                .iter()
                .find(|p| p.pid == pid)
                .map(|p| p.path.clone())
        })
    }

    /// Where a test said, or the real device.
    fn device_of(&self, path: &Path) -> std::io::Result<u64> {
        if let Some(dev) = self
            .devices
            .lock()
            .expect("a poisoned test host is a failed test")
            .get(path)
        {
            return Ok(*dev);
        }
        super::fs::device(path)
    }

    /// The jar a test planted on the file. A file that is not there is not found, as
    /// `sqlite3` would find it; one there with nothing planted cannot be read.
    fn cookie_table(&self, path: &Path) -> std::io::Result<CookieTable> {
        std::fs::symlink_metadata(path)?;
        if let Some(kind) = *self
            .jar_fault
            .lock()
            .expect("a poisoned test host is a failed test")
        {
            return Err(std::io::Error::new(kind, "a jar a test made fail"));
        }
        let cookies = self
            .cookies
            .lock()
            .expect("a poisoned test host is a failed test");
        cookies
            .get(&jar_key(path))
            .or_else(|| cookies.get(&JarKey::Path(path.to_path_buf())))
            .cloned()
            .ok_or_else(|| std::io::Error::other("nothing planted in this jar"))
    }

    /// The version a test said the bundle is. No bundle is read here.
    fn bundle_version(&self, app: &Path) -> Option<String> {
        self.versions
            .lock()
            .expect("a poisoned test host is a failed test")
            .get(app)
            .cloned()
    }

    fn scheduler(&self) -> Option<&dyn Scheduler> {
        (!self.unscheduled.load(Ordering::SeqCst)).then_some(self.scheduler.as_ref())
    }
}

/// Which jar a cookie database is: its inode where the file exists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum JarKey {
    Inode(u64, u64),
    Path(PathBuf),
}

fn jar_key(path: &Path) -> JarKey {
    use std::os::unix::fs::MetadataExt;
    match std::fs::metadata(path) {
        Ok(found) => JarKey::Inode(found.dev(), found.ino()),
        Err(_) => JarKey::Path(path.to_path_buf()),
    }
}
