use super::folder::awaited::{Wait, WaitStep};
use super::{Error, Readied, Result, Settled, identify, purge};
use crate::context::Context;
use crate::in_use::{self, InUse};
use crate::permit::Permit;
use crate::provider::Owner;
use crate::provider::folder::Folder;
use crate::provider::{self, LoginKind, Provider, ProviderId};
use crate::state::{Account, Key, Park, State};
use crate::warning::Warning;
use crate::{home, lock, park, state, store};
use serde_json::{Value, json};
use std::fs::{File, TryLockError};
use std::path::PathBuf;

#[derive(Debug)]
pub enum Enrolled {
    /// The account signed in now, recorded without parking: its first switch parks it.
    Current { email: String },
    /// Another account, signed in privately and parked.
    SignedIn { email: String },
    /// An enrolled account signed in to again: its parked login is now the new one.
    Renewed { email: String },
    /// The account signed in now, signed in to: its new login is the one in use now, and
    /// nothing was parked. `again` when it was enrolled already, and not when this enrolled
    /// it, which a browser that signs in to the session it already has makes likely.
    InUse { email: String, again: bool },
}

// Not yet enrolled. Dropping a program's deletes its private directory and what it kept.
pub struct SignIn {
    provider: ProviderId,
    how: How,
    ctx: Context,
    /// What it was reserved with: what enrolling it writes with, and what lets dropping it
    /// remove what it reserved.
    permit: Permit,
    /// The lock on `signin.lock` that makes this the one sign-in at a time, let go of as this
    /// is dropped.
    one_at_a_time: File,
}

// Without the context and the reservation, which say nothing of the sign-in itself.
impl std::fmt::Debug for SignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignIn")
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

// A program's sign-in as its enrolment reads it.
struct Stored<'a> {
    permit: Permit,
    document: &'a Value,
    // where the sign-in ran, whose own record names the login it made
    dir: &'a std::path::Path,
}

enum How {
    Privately {
        tool: &'static dyn Provider,
        dir: PathBuf,
        document: Value,
    },
    // Inside the tool's own app, whose folder holds the login.
    InItsApp {
        app: &'static dyn Folder,
        // the account parked for it; None where nothing is
        put_back: Option<Key>,
    },
}

impl SignIn {
    /// Which tool this login is for.
    pub fn provider(&self) -> ProviderId {
        self.provider
    }

    pub(super) fn in_its_app(
        ctx: &Context,
        permit: Permit,
        one_at_a_time: File,
        app: &'static dyn Folder,
        put_back: Option<Key>,
    ) -> SignIn {
        SignIn {
            provider: app.id(),
            how: How::InItsApp { app, put_back },
            ctx: ctx.clone(),
            permit,
            one_at_a_time,
        }
    }

    #[cfg(test)]
    fn stored(&self) -> Stored<'_> {
        match &self.how {
            How::Privately { document, dir, .. } => Stored {
                permit: self.permit,
                document,
                dir,
            },
            How::InItsApp { .. } => panic!("a sign-in inside an app stores no document"),
        }
    }

    // What the tool stored once its sign-in ended. An app's folder is the login: nothing is read.
    fn read_back(&mut self) -> Result<()> {
        match &mut self.how {
            How::Privately {
                tool,
                dir,
                document,
            } => {
                *document = signed_in_document(&self.ctx, *tool, dir)?;
                Ok(())
            }
            How::InItsApp { .. } => Ok(()),
        }
    }
}

impl Drop for SignIn {
    // Unlocked, not only closed: a process another thread is starting holds a copy of the file
    // until it runs its program, which kept the lock for milliseconds (ARCHITECTURE.md's
    // Measured facts) and refused a sign-in started meanwhile.
    fn drop(&mut self) {
        match &self.how {
            How::Privately { tool, dir, .. } => {
                tool.discard_signin(&self.ctx, self.permit, dir);
                let _ = crate::host::fs::remove_dir_all(self.permit, dir);
            }
            How::InItsApp { .. } => {}
        }
        let _ = self.one_at_a_time.unlock();
    }
}

// The lock that makes a sign-in the one at a time, never waited for.
pub(crate) fn one_at_a_time(ctx: &Context, permit: Permit) -> Result<File> {
    let home = home::ensure(ctx, permit).map_err(|source| Error::HomeUnwritable {
        path: home::dir(ctx),
        source,
    })?;
    let lock_path = home.join("signin.lock");
    let one_at_a_time =
        crate::host::fs::open_private_lock(permit, &lock_path).map_err(|source| {
            Error::HomeUnwritable {
                path: lock_path.clone(),
                source,
            }
        })?;
    match one_at_a_time.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Err(Error::SignInInProgress),
        Err(TryLockError::Error(source)) => {
            return Err(Error::HomeUnwritable {
                path: lock_path,
                source,
            });
        }
    }
    Ok(one_at_a_time)
}

// The one folder every program's private sign-in uses, so Claude Code's slot for it is fixed.
pub(crate) fn signin_dir(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("signin")
}

// Each tool's login a sign-in in `dir` may have left in its slot, with the one-at-a-time lock
// held.
pub(crate) fn discard_signins(ctx: &Context, permit: Permit, dir: &std::path::Path) {
    for &which in ProviderId::ALL {
        match provider::of(which) {
            LoginKind::Document(each) => each.discard_signin(ctx, permit, dir),
            // No private directory is an app's.
            LoginKind::Folder(_) => {}
        }
    }
}

// A program's sign-in: the one-at-a-time lock and the private directory it signs in to. It
// takes no other lock, so a switch meanwhile goes ahead and a second sign-in is refused.
fn reserve_signin(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
) -> Result<(SignIn, PathBuf)> {
    let one_at_a_time = one_at_a_time(ctx, permit)?;
    let dir = signin_dir(ctx);
    // A sign-in that was killed rather than finished never ran its cleanup, so a login can
    // be sitting in the scratch slot with nothing naming it. The directory is always the
    // same one, so the slot is too, and this is the moment it can be cleared safely: the
    // lock above means no other sign-in is using it. Every tool's leftovers, because the
    // one that was killed need not be the one starting now.
    discard_signins(ctx, permit, &dir);
    let _ = crate::host::fs::remove_dir_all(permit, &dir);
    crate::host::fs::create_private_dir(permit, &dir).map_err(|source| Error::HomeUnwritable {
        path: dir.clone(),
        source,
    })?;
    let pending = SignIn {
        provider: tool.id(),
        how: How::Privately {
            tool,
            dir: dir.clone(),
            document: Value::Null,
        },
        ctx: ctx.clone(),
        permit,
        one_at_a_time,
    };
    Ok((pending, dir))
}

/// A sign-in that never ran. The crash matrix needs the state a finished sign-in leaves,
/// and running a tool's own login inside a test is neither possible nor wanted. The app
/// model's tests in `pitboard-ffi` need it for the same reason, through `testing`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn planted(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
    document: Value,
) -> Result<SignIn> {
    let (mut pending, _) = reserve_signin(ctx, permit, tool)?;
    if let How::Privately { document: held, .. } = &mut pending.how {
        *held = document;
    }
    Ok(pending)
}

/// Run the tool's own sign-in in a private directory, where the live login is never
/// touched, and read back the login it stored there.
pub(crate) fn sign_in(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
) -> Result<SignIn> {
    let (mut pending, dir) = reserve_signin(ctx, permit, tool)?;
    // Pitboard never sees the sign-in; it reads the login the tool stores once it is done.
    // What the tool prints goes to stderr, so `--json` output stays one JSON line.
    let finished = tool
        .sign_in(ctx, permit, &dir)
        .and_then(|mut command| {
            command.stdout(std::io::stderr());
            crate::host::proc::start(command)?.wait()
        })
        .map_err(|e| started(tool.id(), &e))?
        .success();
    if !finished {
        return Err(Error::SignInIncomplete);
    }
    pending.read_back()?;
    Ok(pending)
}

// Asked before a browser opens, so nobody signs in to an account no switch could reach.
pub(crate) fn ready_to_sign_in(ctx: &Context, driver: &'static dyn Provider) -> Result<()> {
    let tool = driver.id();
    if tool == ProviderId::Claude && ctx.custom_oauth() {
        return Err(Error::CustomOauthEndpoint);
    }
    state::load(ctx)?;
    super::live_store(ctx, driver)?;
    // A scratch home that does not isolate the tool would have a sign-in write over the login.
    if let provider::Isolation::NotIsolated { reason } = driver.private_signin_isolation(ctx) {
        return Err(Error::SignInNotIsolated { reason });
    }
    if driver.program(ctx).is_none() {
        return Err(Error::ProgramMissing {
            tool,
            program: ctx
                .program_for(tool)
                .unwrap_or(std::path::Path::new(tool.program()))
                .display()
                .to_string(),
        });
    }
    Ok(())
}

// Windows alone, which is not released, so macOS and Linux say what they said before.
pub(crate) fn running_already(ctx: &Context, which: ProviderId) -> Option<String> {
    if ctx.os != crate::host::Os::Windows || which != ProviderId::Codex {
        return None;
    }
    match super::still_holding(ctx, which) {
        super::StillHolding::These(holding) => {
            Some(crate::words::codex_runs_before_a_sign_in(&holding))
        }
        super::StillHolding::Nothing | super::StillHolding::Unknown => None,
    }
}

fn started(which: ProviderId, e: &std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::NotFound => Error::ProgramNotFound { tool: which },
        _ => Error::SignInIncomplete,
    }
}

fn signed_in_document(
    ctx: &Context,
    tool: &'static dyn Provider,
    dir: &std::path::Path,
) -> Result<Value> {
    let raw = tool
        .login_from_sign_in(ctx, dir)?
        .ok_or(Error::SignInIncomplete)?;
    serde_json::from_str(&raw).map_err(|e| Error::LiveCredentialShapeUnexpected {
        tool: tool.id(),
        detail: e.to_string(),
    })
}

// A sign-in a front end follows through `said`.
pub struct WatchedSignIn {
    tool: Running,
    said: Said,
    steps: Steps,
    pending: SignIn,
}

// Where a sign-in waited for inside an app has got to, for a front end that words it itself.
#[derive(Clone, Default)]
pub struct Steps(std::sync::Arc<std::sync::Mutex<Option<WaitStep>>>);

impl Steps {
    // None for a program's sign-in.
    pub fn now(&self) -> Option<WaitStep> {
        self.0.lock().ok().and_then(|step| *step)
    }

    pub(super) fn set(&self, step: WaitStep) {
        if let Ok(mut now) = self.0.lock() {
            *now = Some(step);
        }
    }
}

// A script plays the program only where a test or a fixture says.
enum Running {
    Program(crate::host::proc::Tool),
    InItsApp(Wait),
    #[cfg(any(test, feature = "test-support"))]
    Scripted(Box<dyn ScriptedSignIn>),
}

/// A tool's own sign-in, played rather than run, for a test or a fixture that may start no
/// program: everything up to the tool's program is the real core's, which reserves the
/// private directory, reads back the login the script stores there as it reads the tool's,
/// and enrols it. Only a context made with `Context::with_sign_in_script` has one, and only a
/// build with `test-support` has the seam.
#[cfg(any(test, feature = "test-support"))]
pub trait SignInScript: Send + Sync + std::fmt::Debug {
    /// Starts `which`'s sign-in into the private directory `dir`, as `label` names the
    /// account somebody means to sign in to, where the caller said which: the person at the
    /// browser, whom a program never knows of. What the tool says goes to `say`, and dropping
    /// it is the tool having stopped saying anything.
    fn start(
        &self,
        which: ProviderId,
        label: Option<&str>,
        dir: &std::path::Path,
        say: std::sync::mpsc::Sender<String>,
    ) -> Box<dyn ScriptedSignIn>;
}

/// One sign-in a `SignInScript` started, as its program would be driven.
#[cfg(any(test, feature = "test-support"))]
pub trait ScriptedSignIn: Send + Sync {
    /// A line typed back. False where nothing reads it any more, as a closed stdin is.
    fn typed(&mut self, line: &str) -> bool;
    /// Waits for it to end, and says whether it ended signed in, with the login stored in its
    /// directory where the tool stores one.
    fn wait(&mut self) -> bool;
    /// Stops it, as a kill stops the program.
    fn stop(&mut self);
}

/// What a watched sign-in's tool says, read apart from the sign-in itself.
///
/// Reading waits until the tool says something, and Codex prints its address and then
/// nothing until the browser is done. A reader that held the sign-in while it waited held
/// up a paste or a cancel for as long, so what it says is its own handle, and stopping the
/// tool is what ends the reading.
#[derive(Debug, Clone)]
pub struct Said(std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<String>>>);

impl Said {
    /// The next thing the tool said, or `None` once it has finished saying anything.
    /// Blocks, so a caller reads it on a thread of its own.
    pub fn next(&self) -> Option<String> {
        self.0.lock().ok()?.recv().ok()
    }

    // `next` for a test, which then fails where nothing is said rather than hang.
    #[cfg(test)]
    pub(crate) fn within(
        &self,
        wait: std::time::Duration,
    ) -> std::result::Result<String, std::sync::mpsc::RecvTimeoutError> {
        self.0.lock().expect("one reader").recv_timeout(wait)
    }
}

impl std::fmt::Debug for WatchedSignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchedSignIn")
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

impl WatchedSignIn {
    /// Which tool's sign-in this is.
    pub fn provider(&self) -> ProviderId {
        self.pending.provider
    }

    /// What the tool says, for a reader on a thread of its own that must not hold up a
    /// paste or a cancel while it waits.
    pub fn said(&self) -> Said {
        self.said.clone()
    }

    pub fn steps(&self) -> Steps {
        self.steps.clone()
    }

    // The account parked for this sign-in, which a switch to it puts back in use.
    pub fn put_back(&self) -> Option<Key> {
        match &self.pending.how {
            How::Privately { .. } => None,
            How::InItsApp { put_back, .. } => put_back.clone(),
        }
    }

    pub(super) fn awaited(
        pending: SignIn,
        wait: Wait,
        said: std::sync::mpsc::Receiver<String>,
        steps: Steps,
    ) -> WatchedSignIn {
        WatchedSignIn {
            tool: Running::InItsApp(wait),
            said: Said(std::sync::Arc::new(std::sync::Mutex::new(said))),
            steps,
            pending,
        }
    }

    /// Types a line back, for a code the tool reads from stdin while it waits on the
    /// browser.
    pub fn paste(&mut self, line: &str) -> Result<()> {
        match &mut self.tool {
            Running::Program(tool) => {
                use std::io::Write;
                let stdin = tool.child().stdin.as_mut().ok_or(Error::SignInIncomplete)?;
                writeln!(stdin, "{line}").map_err(|_| Error::SignInIncomplete)?;
                stdin.flush().map_err(|_| Error::SignInIncomplete)
            }
            // Nothing reads a code.
            Running::InItsApp(_) => Err(Error::SignInIncomplete),
            #[cfg(any(test, feature = "test-support"))]
            Running::Scripted(run) => {
                if run.typed(line) {
                    Ok(())
                } else {
                    Err(Error::SignInIncomplete)
                }
            }
        }
    }

    /// Waits for it to finish and hands back the login it stored.
    pub fn finish(mut self) -> Result<SignIn> {
        let finished = match &mut self.tool {
            Running::Program(tool) => tool.wait().map_err(|_| Error::SignInIncomplete)?.success(),
            Running::InItsApp(wait) => {
                wait.finish()?;
                true
            }
            #[cfg(any(test, feature = "test-support"))]
            Running::Scripted(run) => run.wait(),
        };
        if !finished {
            return Err(Error::SignInIncomplete);
        }
        let mut pending = self.pending;
        pending.read_back()?;
        Ok(pending)
    }

    /// Stops it, and returns once its tool has ended and been waited for and the one sign-in
    /// at a time has been let go of, so a sign-in started after this returns is not refused as
    /// one already waiting. What it may have written is discarded by `SignIn`'s own cleanup.
    pub fn cancel(mut self) {
        match &mut self.tool {
            Running::Program(tool) => tool.stop(),
            Running::InItsApp(wait) => wait.cancel(),
            #[cfg(any(test, feature = "test-support"))]
            Running::Scripted(run) => {
                run.stop();
                let _ = run.wait();
            }
        }
    }
}

pub(crate) fn sign_in_watched_as(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
    key: &Key,
) -> Result<WatchedSignIn> {
    start_watched(ctx, permit, tool, Some(&key.label))
}

fn start_watched(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
    label: Option<&str>,
) -> Result<WatchedSignIn> {
    let which = tool.id();
    let (pending, dir) = reserve_signin(ctx, permit, tool)?;
    #[cfg(any(test, feature = "test-support"))]
    if let Some(script) = ctx.sign_in_script() {
        let (say, said) = std::sync::mpsc::channel();
        return Ok(WatchedSignIn {
            tool: Running::Scripted(script.start(which, label, &dir, say)),
            said: Said(std::sync::Arc::new(std::sync::Mutex::new(said))),
            steps: Steps::default(),
            pending,
        });
    }
    // Only a script playing the tool is told whom the sign-in is for.
    let _ = label;
    let first = running_already(ctx, which);
    tool.sign_in(ctx, permit, &dir)
        .and_then(|command| watch(command, pending, first))
        .map_err(|e| started(which, &e))
}

// `first` said before anything the tool says.
fn watch(
    mut command: std::process::Command,
    pending: SignIn,
    first: Option<String>,
) -> std::io::Result<WatchedSignIn> {
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut tool = crate::host::proc::start(command)?;
    let child = tool.child();
    let pid = child.id();
    let (say, said) = std::sync::mpsc::channel();
    if let Some(first) = first {
        let _ = say.send(first);
    }
    // Codex writes its address to stderr.
    if let Some(stdout) = child.stdout.take() {
        let say = say.clone();
        std::thread::spawn(move || crate::host::proc::pump(stdout, &say, pid));
    }
    if let Some(stderr) = child.stderr.take() {
        let say = say.clone();
        std::thread::spawn(move || crate::host::proc::pump(stderr, &say, pid));
    }
    Ok(WatchedSignIn {
        tool: Running::Program(tool),
        said: Said(std::sync::Arc::new(std::sync::Mutex::new(said))),
        steps: Steps::default(),
        pending,
    })
}

/// Enroll the account signed in to `which` now, or with `signed_in`, the one a sign-in just
/// produced.
pub(crate) fn enroll(
    settled: Settled,
    key: &Key,
    signed_in: Option<SignIn>,
) -> Result<(Enrolled, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    match signed_in {
        // A sign-in was run for one tool; filing its login under another would be an
        // account of the wrong tool under the name somebody chose.
        Some(login) if login.provider != key.provider => Err(Error::Usage(format!(
            "that sign-in was {}'s, and `{key}` is a {} account",
            login.provider.name(),
            key.provider.name()
        ))),
        Some(login) => match &login.how {
            How::Privately {
                tool,
                document,
                dir,
            } => {
                let stored = Stored {
                    permit: login.permit,
                    document,
                    dir,
                };
                from_sign_in(&ctx, *tool, key, &mut state, &stored)
            }
            How::InItsApp { app, .. } => super::folder::awaited::enroll(
                Settled {
                    _exclusive,
                    state,
                    ctx,
                    permit,
                },
                key,
                *app,
            ),
        },
        None => match provider::of(key.provider) {
            LoginKind::Document(tool) => record_current(&ctx, permit, tool, key, &mut state),
            LoginKind::Folder(app) => super::folder::enroll(
                Settled {
                    _exclusive,
                    state,
                    ctx,
                    permit,
                },
                key,
                app,
            ),
        },
    }
}

/// A label names one account of its tool for good: its own, or one not enrolled under
/// another label.
pub(super) fn claim(state: &State, key: &Key, owner: &Owner) -> Result<()> {
    if let Some(taken) = state.get(key)
        && !taken.owned_by(owner)
    {
        return Err(Error::LabelTaken {
            label: key.typed(),
            who: crate::words::login(key.provider, &taken.email, taken.email == owner.email),
        });
    }
    if let Some(existing) = state.account_of(key.provider, owner)
        && existing.label != key.label
    {
        return Err(Error::AlreadyEnrolled {
            tool: key.provider,
            email: owner.email.clone(),
            label: existing.key().typed(),
            asked: key.typed(),
            waited: false,
        });
    }
    Ok(())
}

fn record_current(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
    key: &Key,
    state: &mut State,
) -> Result<(Enrolled, Vec<Warning>)> {
    let which = key.provider;
    let label = key.label.as_str();
    // Through the store itself rather than the provider's reading of it, so a locked
    // keychain says so in the store's own words instead of reading as a strange login. A
    // document with no account in it is nobody signed in: Claude Code's after a `/logout`
    // still holds the machine's MCP tokens.
    let (stored, found) = identify::look(ctx, permit, state, tool)?;
    let identify::Live::Login(stored) = stored else {
        return Err(Error::LiveCredentialAbsent {
            tool: which,
            label: None,
            spelling: crate::words::spelled(ctx, which),
        });
    };
    let lost = stored.lock_warning(which);
    let identify::Login {
        document: live,
        owner,
        ..
    } = stored;
    claim(state, key, &owner)?;
    let existing = state.get(key);
    let parked = existing.and_then(|a| a.parked.clone());
    // Enrolling the account that is signed in is using it.
    let last_used_at = Some(ctx.now());
    state.upsert(account(
        which,
        state.id_of(which, &owner),
        label,
        &owner,
        parked,
        last_used_at,
        tool.plan(&live),
    ));
    // Recorded once the account is enrolled, so the activity log names it.
    let (_, noticed) = identify::record(state, which, found, ctx.now());
    state::save(ctx, permit, state)?;
    identify::write_down(ctx, permit, &noticed);
    Ok((
        Enrolled::Current { email: owner.email },
        lost.into_iter().collect(),
    ))
}

/// Enrol the account a sign-in produced: put in use when it is the account signed in now,
/// and parked when it is any other.
///
/// Somebody signs in again to the account in use because its login is broken or about to
/// lapse. Parked, the new login left the tool on the old one, and the next switch away
/// parked the old one over it.
fn from_sign_in(
    ctx: &Context,
    tool: &'static dyn Provider,
    key: &Key,
    state: &mut State,
    login: &Stored<'_>,
) -> Result<(Enrolled, Vec<Warning>)> {
    let owner = identify::asked(&ctx.signed_in_at(login.dir), state, tool, login.document)
        .map_err(|e| {
            let login = crate::error::Unidentified::SignIn {
                label: state.typed(key),
            };
            super::unidentified_as(ctx, tool.id(), login, e)
        })?;
    claim(state, key, &owner)?;
    let (now, lost) = signed_in_now(ctx, login.permit, state, tool, &owner);
    let said = |(enrolled, warnings): (Enrolled, Vec<Warning>)| {
        (enrolled, lost.into_iter().chain(warnings).collect())
    };
    match now {
        SignedInNow::Theirs(live, first) => {
            install_signed_in(ctx, tool, key, state, login, &owner, &live, &first).map(said)
        }
        SignedInNow::NotTheirs => park_signed_in(ctx, tool, key, state, login, &owner).map(said),
        SignedInNow::Untold(why) => {
            // Somebody signing in to the account Pitboard last saw in use most likely wants
            // its broken login replaced, and somebody adding an account while nobody can name
            // the login in use, as on a first run, may expect it in use. Parking is neither,
            // so it is said. Signing in again to another parked account renews its park.
            let renews_a_park = state.get(key).is_some()
                && !state
                    .account_in_use(key.provider)
                    .is_some_and(|account| account.is(key));
            let (enrolled, mut warnings) = park_signed_in(ctx, tool, key, state, login, &owner)?;
            if !renews_a_park {
                warnings.push(Warning::SignInParkedNotInUse {
                    tool: key.provider,
                    label: state.typed(key),
                    why: identify::untold(&why),
                    spelling: crate::words::spelled(ctx, key.provider),
                });
            }
            Ok(said((enrolled, warnings)))
        }
    }
}

/// Whose the tool's live login is, as far as a sign-in to `owner`'s account needs to know.
///
/// Read and recorded the way a switch reads it. Writing over a login whose account is not
/// known could lose that account's only login, so only a login known to be `owner`'s is
/// written over.
enum SignedInNow {
    /// `owner`'s: where it is, and what it held.
    Theirs(provider::LiveStore, Value),
    /// Another account's, or nobody's.
    NotTheirs,
    /// It could not be read, or its account could not be told, for this reason.
    Untold(Error),
}

/// [`SignedInNow`], with what telling it says of a lock lost while renewing the login stored.
fn signed_in_now(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    tool: &'static dyn Provider,
    owner: &Owner,
) -> (SignedInNow, Option<Warning>) {
    match identify::now(ctx, permit, state, tool) {
        Ok(identify::Live::Login(login)) => {
            let lost = login.lock_warning(tool.id());
            let identify::Login {
                store,
                document,
                owner: found,
                ..
            } = login;
            let now = if found.same_login(owner) {
                SignedInNow::Theirs(store, document)
            } else {
                SignedInNow::NotTheirs
            };
            (now, lost)
        }
        Ok(identify::Live::Nothing) => (SignedInNow::NotTheirs, None),
        Err(e) => (SignedInNow::Untold(e), None),
    }
}

/// Put the new login of the account signed in now in place of its old one, under the rules
/// a switch writes by, and record the account as `record_current` does. Nothing is parked,
/// and a park the account already holds is kept. The old login is dropped, which is what
/// the tool's own sign-in does to the login it replaces.
#[allow(clippy::too_many_arguments)]
fn install_signed_in(
    ctx: &Context,
    tool: &'static dyn Provider,
    key: &Key,
    state: &mut State,
    login: &Stored<'_>,
    owner: &Owner,
    live: &provider::LiveStore,
    first: &Value,
) -> Result<(Enrolled, Vec<Warning>)> {
    let permit = login.permit;
    let which = key.provider;
    let name = state.typed(key);
    let named = in_use::named(ctx, which);
    // Said as a sign-in's failure rather than a switch's: the new login goes with the
    // sign-in, so nothing was lost is not true of it, and the way on is signing in again.
    let not_kept = |detail: String| Error::SignInNotKept {
        tool: which,
        label: name.clone(),
        detail,
    };
    let slice = tool
        .slice(login.document)
        .map_err(|e| super::shape(which, e))?;
    super::kept(ctx, tool, &name, login.document, owner)?;
    let guard = super::write_lock(ctx, permit, tool)?;
    let Readied {
        before_raw,
        next,
        on_the_command_line,
        ..
    } = super::ready(ctx, tool, live, first, owner, &slice, &name).map_err(|e| match e {
        Error::SignedInAccountChanged => not_kept(format!(
            "{} was signed in to another account meanwhile",
            which.name()
        )),
        other => other,
    })?;

    let written = on_the_command_line.into_iter().collect::<Vec<_>>();
    match super::install_with(
        ctx,
        which,
        |body| store::write_raw(&live.chain, permit, &live.service, body),
        || store::read_raw(&live.chain, &live.service),
        &next,
        &before_raw,
        &name,
        &name,
    ) {
        Ok(()) => {}
        // The old login is where it was, and the new one goes with the sign-in.
        Err(Error::SwitchRolledBack { detail, .. }) => return Err(not_kept(detail)),
        Err(Error::SwitchUnverified { detail, .. } | Error::SwitchCorrupted { detail, .. }) => {
            return Err(not_installed(
                ctx, tool, key, state, login, owner, detail, written,
            ));
        }
        Err(other) => return Err(other),
    }
    crate::fault::point("enroll.installed");

    // Read back as a switch reads back: a write that landed has not necessarily held.
    let lock_lost = guard.as_ref().is_some_and(lock::Guard::compromised);
    let lost = match super::holds(tool, live) {
        Ok(true) => None,
        Ok(false) => Some("it was gone again before Pitboard finished".to_string()),
        Err(unreadable) => Some(unreadable.to_string()),
    };
    if let Some(detail) = lost {
        return Err(not_installed(
            ctx, tool, key, state, login, owner, detail, written,
        ));
    }

    let again = state.get(key).is_some();
    let parked = state.get(key).and_then(|a| a.parked.clone());
    state.upsert(account(
        which,
        state.id_of(which, owner),
        &key.label,
        owner,
        parked,
        Some(ctx.now()),
        tool.plan(login.document),
    ));
    let installed = InUse {
        owner: Some(owner.clone()),
        login: tool.fingerprint(&slice),
        known_at: ctx.now(),
        named,
    };
    state.identified(which, installed, ctx.now());
    state::save(ctx, permit, state)?;
    crate::fault::point("enroll.recorded");
    drop(guard);

    // A session of a tool that never reads its login again goes on with the old one, and
    // writes it back over the new one when it refreshes. Where nobody could tell what is
    // running, that is said, since one may be.
    let still_running = match super::still_holding(ctx, which) {
        super::StillHolding::Nothing => None,
        super::StillHolding::These(holding) => Some(Warning::SessionsKeepTheOldLogin {
            label: name.clone(),
            holding,
        }),
        super::StillHolding::Unknown => Some(Warning::SessionsUnknownAfterSignIn {
            tool: which,
            label: name.clone(),
        }),
    };
    let warnings = written
        .into_iter()
        .chain(still_running)
        .chain(lock_lost.then_some(Warning::LockCompromised { tool: which }))
        .collect();
    Ok((
        Enrolled::InUse {
            email: owner.email.clone(),
            again,
        },
        warnings,
    ))
}

/// A new login that could not be confirmed in use, where the tool may be left without the
/// old one too. The new one is the one copy of the account's login known to be good, so it
/// is parked, as a sign-in of any other account would be, before the failure is reported.
/// Where the write landed and could not be read back, the slot holds the new login as well:
/// no renewal spends that copy, and the next change that can read the slot drops it.
///
/// What writing it warned about is carried on the error with what parking it warns about,
/// because both happened whatever became of them.
#[allow(clippy::too_many_arguments)]
fn not_installed(
    ctx: &Context,
    tool: &'static dyn Provider,
    key: &Key,
    state: &mut State,
    login: &Stored<'_>,
    owner: &Owner,
    detail: String,
    mut warnings: Vec<Warning>,
) -> Error {
    let parked = match park_signed_in(ctx, tool, key, state, login, owner) {
        Ok((_, parking)) => {
            warnings.extend(parking);
            true
        }
        Err(_) => false,
    };
    Error::SignInNotInstalled {
        tool: key.provider,
        label: state.typed(key),
        detail,
        parked,
        spelling: crate::words::spelled(ctx, key.provider),
        warnings,
    }
}

fn park_signed_in(
    ctx: &Context,
    tool: &'static dyn Provider,
    key: &Key,
    state: &mut State,
    login: &Stored<'_>,
    owner: &Owner,
) -> Result<(Enrolled, Vec<Warning>)> {
    let permit = login.permit;
    let label = key.label.as_str();
    let id = state.id_of(key.provider, owner);
    let slice = tool
        .slice(login.document)
        .map_err(|e| super::shape(key.provider, e))?;
    let (fresh, parking) = park::keep(ctx, permit, tool, &key.typed(), &id, &slice)?;
    let service = fresh.service.clone();
    // The window the roadmap named: the login is in the vault and nothing on the machine
    // says so yet.
    crate::fault::point("enroll.park_stored");
    let existing = state.get(key);
    let previous = existing.and_then(|a| a.parked.clone());
    let renewed = existing.is_some();
    let last_used_at = existing.and_then(|a| a.last_used_at);
    state.upsert(account(
        key.provider,
        id,
        label,
        owner,
        previous,
        last_used_at,
        tool.plan(login.document),
    ));
    state.park(key, fresh);
    // Unrecorded, the new login would be an item nothing refers to, never deleted.
    state::save(ctx, permit, state).inspect_err(|_| {
        let _ = store::vault_delete(ctx, permit, &service);
    })?;
    crate::fault::point("enroll.park_recorded");
    purge(ctx, permit, state);
    let email = owner.email.clone();
    let enrolled = if renewed {
        Enrolled::Renewed { email }
    } else {
        Enrolled::SignedIn { email }
    };
    Ok((enrolled, parking.into_iter().collect()))
}

/// What Pitboard records about a newly enrolled account.
///
/// For Claude Code, only what Anthropic just confirmed: leaving the rest out makes Claude
/// Code fetch its own profile after a switch rather than trust a copy Pitboard wrote.
pub(super) fn account(
    which: ProviderId,
    id: String,
    label: &str,
    owner: &Owner,
    parked: Option<Park>,
    last_used_at: Option<i64>,
    plan: Option<String>,
) -> Account {
    Account {
        last_used_at,
        replaced_at: None,
        plan,
        label: label.to_string(),
        id,
        account_uuid: owner.account_uuid.clone(),
        email: owner.email.clone(),
        parked,
        detail: detail(which, owner),
    }
}

fn detail(which: ProviderId, owner: &Owner) -> state::Detail {
    match which {
        ProviderId::Claude => state::Detail::Claude {
            organization_uuid: owner.organization_uuid.clone(),
            oauth_account: json!({
                "accountUuid": owner.account_uuid,
                "emailAddress": owner.email,
                "organizationUuid": owner.organization_uuid,
            }),
        },
        ProviderId::Codex => state::Detail::Codex {
            workspace_id: Some(owner.organization_uuid.clone()).filter(|id| !id.is_empty()),
        },
        ProviderId::Desktop => state::Detail::Desktop {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::scripted::Trouble;
    use crate::store::memory::Fault;
    use crate::switch::harness::{
        self, NOW, Rig, Tool, codex_id, codex_login, codex_machine, hold, login_of, machine, oauth,
        renews, signed_in,
    };
    use crate::switch::{Due, Outcome, renew_due, settle, switch};
    use crate::time::FixedClock;
    use std::sync::Arc;
    use std::time::Duration;

    type Make = fn(&str) -> Rig;
    /// A machine of each tool: `here` signed in, `there` parked and ready.
    const MACHINES: [(&str, Make); 2] = [("claude", machine), ("codex", codex_machine)];

    /// Whether the login in use is the one this refresh token belongs to.
    fn in_use(m: &Rig, refresh: &str) -> bool {
        m.live().is_some_and(|live| {
            provider::document(m.which).fingerprint(&live) == store::fingerprint(refresh)
        })
    }

    /// Enrols what a sign-in left under `label`, the way the next command would.
    fn enrolled_as(m: &Rig, label: &str, login: SignIn) -> Result<(Enrolled, Vec<Warning>)> {
        let settled = settle(&m.ctx, Permit::for_a_test(), Some(m.which))
            .expect("nothing to recover")
            .0;
        enroll(settled, &m.key(label), Some(login))
    }

    fn park_of(m: &Rig, label: &str) -> Option<Park> {
        state::load(&m.ctx)
            .expect("state")
            .get(&m.key(label))
            .and_then(|a| a.parked.clone())
    }

    /// `here` in another organisation, signed in through `refresh`.
    fn here_in(m: &Rig, org: &str, refresh: &str) -> SignIn {
        m.api.owned_by(
            &format!("access-{refresh}"),
            Owner {
                account_uuid: "here".into(),
                email: "here@example.com".into(),
                organization_uuid: org.into(),
            },
        );
        planted(
            &m.ctx,
            Permit::for_a_test(),
            provider::document(ProviderId::Claude),
            harness::document(refresh),
        )
        .expect("a sign-in")
    }

    /// One Anthropic account in two organisations is two logins, each on its own plan. The
    /// account uuid is the person in both, and identified by it alone the second was
    /// refused as already enrolled, and a switch to it said the first was already in use.
    #[test]
    fn one_account_in_two_organisations_is_two_logins() {
        let m = machine("two-organisations");
        let (enrolled, _) = enrolled_as(&m, "team", here_in(&m, "org-team", "team-refresh"))
            .expect("the other organisation is a login of its own");
        assert!(
            matches!(enrolled, Enrolled::SignedIn { .. }),
            "{enrolled:?}"
        );

        let state = state::load(&m.ctx).expect("state");
        let here = state.get(&m.key("here")).expect("here");
        let team = state.get(&m.key("team")).expect("team");
        assert_eq!(here.account_uuid, team.account_uuid, "one person");
        assert_eq!(team.id, "here_org-team");
        assert!(
            team.parked
                .as_ref()
                .is_some_and(|p| p.service.starts_with("pitboard-park-here_org-team-")),
            "{:?}",
            team.parked
        );
        assert_eq!(
            team.claude().expect("Claude").oauth_account["accountUuid"],
            "here",
            "Claude Code's config is given the account uuid it knows"
        );

        let settled = settle(&m.ctx, Permit::for_a_test(), Some(m.which))
            .expect("nothing to recover")
            .0;
        let (outcome, _) = switch(settled, &m.key("team")).expect("a switch");
        assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
        assert!(in_use(&m, "team-refresh"));
        assert!(
            park_of(&m, "here").is_some_and(|p| p.service.starts_with("pitboard-park-here-1")),
            "the first organisation's login is parked under the id it was enrolled with"
        );

        let again = enrolled_as(&m, "personal", here_in(&m, "org-team", "team-again"))
            .expect_err("the same organisation is the same login");
        assert!(matches!(again, Error::AlreadyEnrolled { .. }), "{again}");
    }

    /// Both logins are the same person's, so naming the one that holds a label by its email
    /// alone names the one being enrolled too.
    #[test]
    fn a_label_another_organisation_holds_is_said_to_be_that() {
        let m = machine("label-in-another-organisation");
        let refused = enrolled_as(&m, "here", here_in(&m, "org-team", "team-refresh"))
            .expect_err("`here` is the first organisation's");
        assert_eq!(
            refused.to_string(),
            "`here` already refers to here@example.com in another organisation. Choose a \
             different label."
        );
    }

    /// Somebody signs in again to the account in use, whose login is broken or about to
    /// lapse. The new login is the one the tool uses from now on, and nothing is parked:
    /// a park of the account in use is a copy the next switch away would only replace.
    #[test]
    fn signing_in_again_to_the_account_in_use_puts_the_new_login_in_use() {
        for (tool, make) in MACHINES {
            let m = make("again-in-use");
            let vault = m.mem.vault().services();

            let (enrolled, _) = enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2"))
                .unwrap_or_else(|e| panic!("{tool}: {e}"));

            assert!(
                matches!(
                    enrolled,
                    Enrolled::InUse { ref email, again: true } if email == "here@example.com"
                ),
                "{tool}: {enrolled:?}"
            );
            assert!(
                in_use(&m, "here-refresh-2"),
                "{tool}: the new login is in use"
            );
            assert_eq!(m.mem.vault().services(), vault, "{tool}: nothing is parked");
            assert!(park_of(&m, "here").is_none(), "{tool}");
            let state = state::load(&m.ctx).expect("state");
            assert_eq!(
                state.account_in_use(m.which).map(|a| a.label.as_str()),
                Some("here"),
                "{tool}"
            );
            assert_eq!(
                state.get(&m.key("here")).and_then(|a| a.last_used_at),
                Some(NOW),
                "{tool}: signing in to the account in use is using it"
            );
            hold(&m, &format!("{tool}, after signing in again"));
        }
    }

    /// A park the account in use already holds, from before it was signed in to with the
    /// tool itself, is kept as it is: this sign-in is about the login in use.
    #[test]
    fn signing_in_again_to_the_account_in_use_keeps_the_park_it_holds() {
        for (tool, make) in MACHINES {
            let m = make("again-keeps-park");
            let (uuid, older) = match Tool::of(m.which) {
                Tool::Claude => ("here".to_string(), oauth("here-older", 30)),
                Tool::Codex => (codex_id("here"), codex_login("here", "here-older")),
            };
            let service = park::reserve(&m.ctx, Permit::for_a_test(), &uuid).expect("a free name");
            let held = park::store_at(
                &m.ctx,
                Permit::for_a_test(),
                provider::document(m.which),
                &service,
                &older,
            )
            .expect("parked");
            let mut state = state::load(&m.ctx).expect("state");
            state.park(&m.key("here"), held.clone());
            state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
            let vault = m.mem.vault().services();

            enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2"))
                .unwrap_or_else(|e| panic!("{tool}: {e}"));

            assert!(in_use(&m, "here-refresh-2"), "{tool}");
            assert_eq!(
                park_of(&m, "here"),
                Some(held),
                "{tool}: the park is untouched"
            );
            assert_eq!(m.mem.vault().services(), vault, "{tool}");
            hold(&m, &format!("{tool}, after signing in again beside a park"));
        }
    }

    /// The failure this replaces: the new login was parked beside the old, and the next
    /// switch away parked the old one over it, which for Codex could be a login whose
    /// chain was already revoked. Now the switch parks what is in use, the new login.
    #[test]
    fn the_next_switch_away_parks_the_new_login_not_the_old() {
        for (tool, make) in MACHINES {
            let m = make("again-then-switch");
            enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2"))
                .unwrap_or_else(|e| panic!("{tool}: {e}"));

            let settled = settle(&m.ctx, Permit::for_a_test(), Some(m.which))
                .expect("nothing to recover")
                .0;
            let (outcome, _) = switch(settled, &m.key("there"))
                .unwrap_or_else(|e| panic!("{tool}: the switch away: {e}"));
            assert!(matches!(outcome, Outcome::Switched { .. }), "{tool}");

            let parked = park_of(&m, "here").expect("the outgoing login is parked");
            assert_eq!(
                parked.login,
                store::fingerprint("here-refresh-2"),
                "{tool}: the new login is parked, not the one it replaced"
            );
            hold(&m, &format!("{tool}, after the switch away"));
        }
    }

    /// Another account's sign-in is parked, and the account in use is left exactly as it
    /// was, as before.
    #[test]
    fn a_sign_in_of_another_account_is_parked_beside_the_one_in_use() {
        for (tool, make) in MACHINES {
            let m = make("another-parked");
            let live = m.live();

            let (enrolled, _) = enrolled_as(&m, "third", signed_in(&m, "third", "third-refresh"))
                .unwrap_or_else(|e| panic!("{tool}: {e}"));

            assert!(
                matches!(enrolled, Enrolled::SignedIn { .. }),
                "{tool}: {enrolled:?}"
            );
            assert_eq!(m.live(), live, "{tool}: the login in use is untouched");
            assert_eq!(
                park_of(&m, "third").map(|p| p.login),
                Some(store::fingerprint("third-refresh")),
                "{tool}"
            );
            hold(&m, &format!("{tool}, after another account's sign-in"));
        }
    }

    /// A sign-in whose profile Anthropic refuses is named as the sign-in recorded it, in its
    /// own directory, for the same account and organisation Anthropic named.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test writes the sign-in's config as Claude Code writes it"
    )]
    fn a_sign_in_whose_profile_is_refused_is_named_as_the_sign_in_recorded_it() {
        let m = machine("named-by-sign-in");
        m.api
            .owned_by("access-third-refresh", harness::owner("third"));
        m.api.profile_trouble(
            "access-third-refresh",
            Trouble::Answered(400, r#"{"error":{"type":"invalid_request_error"}}"#),
        );
        let recorded = |who: &str| {
            let owner = harness::owner(who);
            json!({"oauthAccount": {
                "accountUuid": owner.account_uuid,
                "emailAddress": owner.email,
                "organizationUuid": owner.organization_uuid,
            }})
            .to_string()
        };

        let elsewhere = signed_in(&m, "third", "third-refresh");
        std::fs::write(
            elsewhere.stored().dir.join(".claude.json"),
            recorded("here"),
        )
        .expect("the sign-in's config");
        let refused = enrolled_as(&m, "third", elsewhere).expect_err("nobody to name it by");
        assert_eq!(refused.code(), "identity_unverifiable");

        let signed = signed_in(&m, "third", "third-refresh");
        std::fs::write(signed.stored().dir.join(".claude.json"), recorded("third"))
            .expect("the sign-in's config");
        let (enrolled, _) = enrolled_as(&m, "third", signed).expect("enrolled");
        assert!(
            matches!(&enrolled, Enrolled::SignedIn { email } if email == "third@example.com"),
            "{enrolled:?}"
        );
    }

    #[test]
    fn enrolling_records_the_plan_the_login_says() {
        for ((tool, make), plan) in MACHINES.into_iter().zip(["Max 20x", "Pro"]) {
            let m = make("enrol-plan");
            let mut login = login_of(&m, "third", "third-refresh");
            if m.which == ProviderId::Claude {
                login["claudeAiOauth"]["subscriptionType"] = json!("max");
                login["claudeAiOauth"]["rateLimitTier"] = json!("default_claude_max_20x");
            }
            let signed = planted(
                &m.ctx,
                Permit::for_a_test(),
                provider::document(m.which),
                login,
            )
            .expect("a sign-in");

            enrolled_as(&m, "third", signed).unwrap_or_else(|e| panic!("{tool}: {e}"));

            let state = state::load(&m.ctx).expect("state");
            let third = state.get(&m.key("third")).expect("enrolled");
            assert_eq!(third.plan.as_deref(), Some(plan), "{tool}");
        }
    }

    /// Writing over a login whose account nobody can name could lose that account's only
    /// login, so a sign-in to `here` while the login in use cannot be told apart is parked
    /// beside it, as it always was. Each is a login renewed since its account was last
    /// named, so its fingerprint names nobody: for Claude Code one Anthropic refuses or does
    /// not answer about, for Codex one whose ID token cannot be read.
    #[test]
    fn a_login_in_use_whose_account_cannot_be_told_is_not_written_over() {
        let mut cases: Vec<(String, Rig)> = Vec::new();
        for (name, trouble) in [
            ("refused", Trouble::Unauthorized),
            ("offline", Trouble::Offline),
        ] {
            let m = machine(&format!("untold-{name}"));
            m.sign_in(&harness::document("here-renewed"));
            m.api.token_trouble("access-here-renewed", trouble);
            cases.push((format!("claude, {name}"), m));
        }
        let m = codex_machine("untold");
        let mut unreadable = codex_login("here", "here-renewed");
        unreadable["tokens"]["id_token"] = "not a token".into();
        m.sign_in(&unreadable);
        cases.push(("codex".into(), m));

        for (case, m) in cases {
            let live = m.live();

            let (enrolled, warnings) =
                enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2"))
                    .unwrap_or_else(|e| panic!("{case}: {e}"));

            assert!(
                matches!(enrolled, Enrolled::Renewed { .. }),
                "{case}: {enrolled:?}"
            );
            assert_eq!(m.live(), live, "{case}: the login in use is untouched");
            assert_eq!(
                park_of(&m, "here").map(|p| p.login),
                Some(store::fingerprint("here-refresh-2")),
                "{case}: the new login is parked"
            );
            // `here` is the account Pitboard last saw in use, so the person most likely
            // meant to replace its login, and is told that did not happen and why.
            let said = warnings
                .iter()
                .find(|w| w.code() == "sign_in_parked_not_in_use")
                .unwrap_or_else(|| panic!("{case}: {warnings:?}"))
                .to_string();
            assert!(
                said.contains("parked the new login for `"),
                "{case}: {said}"
            );
            assert!(
                said.contains("goes on with the login it has"),
                "{case}: {said}"
            );
            assert!(said.contains("sign in to `"), "{case}: {said}");
        }
    }

    /// The report of 16 October 2026: Claude Code was signed in before Pitboard, to a login
    /// Anthropic would not name, and each account added was parked without a word.
    #[test]
    fn an_account_added_while_the_login_in_use_cannot_be_named_says_it_was_parked() {
        let m = machine("first-run-untold");
        state::save(&m.ctx, Permit::for_a_test(), &State::default()).expect("a first run");
        m.api.token_trouble(
            "access-here-refresh",
            Trouble::Answered(400, r#"{"error":{"type":"invalid_request_error"}}"#),
        );
        m.api
            .owned_by("access-third-refresh", harness::owner("third"));

        let (enrolled, warnings) =
            enrolled_as(&m, "third", signed_in(&m, "third", "third-refresh")).expect("parked");

        assert!(
            matches!(enrolled, Enrolled::SignedIn { .. }),
            "{enrolled:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.code() == "sign_in_parked_not_in_use"),
            "{warnings:?}"
        );
    }

    /// Claude Code's config names `here` and Pitboard finds no login where it reads: the
    /// login is somewhere Pitboard does not look, so whose it is cannot be told, and `here`
    /// is still the account Pitboard last saw in use. A sign-in to `here` is parked and said
    /// as for any login whose account cannot be told, and the record still names `here`.
    #[test]
    fn a_sign_in_while_the_login_claude_code_names_cannot_be_found_is_parked_and_said() {
        let m = machine("untold-elsewhere");
        m.mem.live().delete_everything();

        let (enrolled, warnings) =
            enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2")).expect("parked");

        assert!(matches!(enrolled, Enrolled::Renewed { .. }), "{enrolled:?}");
        let said: Vec<String> = warnings
            .iter()
            .filter(|w| w.code() == "sign_in_parked_not_in_use")
            .map(ToString::to_string)
            .collect();
        assert_eq!(said.len(), 1, "{warnings:?}");
        assert!(
            said[0]
                .contains("its config names here@example.com, and Pitboard cannot find that login"),
            "{}",
            said[0]
        );
        assert_eq!(
            state::load(&m.ctx)
                .expect("state")
                .account_in_use(ProviderId::Claude)
                .map(Account::key),
            Some(m.key("here"))
        );
        assert!(harness::audit_lines(&m, "in-use").is_empty());
    }

    /// Signing in again to a parked account renews its park whoever is signed in, as it
    /// always did, and is not warned about.
    #[test]
    fn a_sign_in_to_a_parked_account_is_not_warned_about_the_login_in_use() {
        let m = machine("untold-parked");
        m.sign_in(&harness::document("here-renewed"));
        m.api
            .token_trouble("access-here-renewed", Trouble::Unauthorized);

        let (enrolled, warnings) =
            enrolled_as(&m, "there", signed_in(&m, "there", "there-refresh-2"))
                .unwrap_or_else(|e| panic!("{e}"));

        assert!(matches!(enrolled, Enrolled::Renewed { .. }), "{enrolled:?}");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// A browser often signs in to the session it already has, so a sign-in under a new
    /// label can be the account signed in now before anything enrolled it. It is enrolled
    /// with its new login in use, and says it was enrolled rather than signed in again.
    #[test]
    fn a_first_sign_in_to_the_account_in_use_enrols_it_with_the_new_login_in_use() {
        for (tool, make) in MACHINES {
            let m = make("first-in-use");
            let mut state = state::load(&m.ctx).expect("state");
            state.accounts.retain(|a| a.label != "here");
            state.in_use.remove(m.which.code());
            state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");

            let (enrolled, _) =
                enrolled_as(&m, "personal", signed_in(&m, "here", "here-refresh-2"))
                    .unwrap_or_else(|e| panic!("{tool}: {e}"));

            assert!(
                matches!(enrolled, Enrolled::InUse { again: false, .. }),
                "{tool}: {enrolled:?}"
            );
            assert!(in_use(&m, "here-refresh-2"), "{tool}");
            let state = state::load(&m.ctx).expect("state");
            assert_eq!(
                state.account_in_use(m.which).map(|a| a.label.as_str()),
                Some("personal"),
                "{tool}"
            );
            hold(&m, &format!("{tool}, after enrolling the account in use"));
        }
    }

    // Codex 0.164.0 would sign the new login out and delete it as it starts
    // (`codex_login_restrictions`), so it is not put in place, and nothing else moves.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test sets up its scratch files as a person or a tool would"
    )]
    fn a_new_codex_login_its_settings_rule_out_is_not_put_in_use() {
        let m = codex_machine("restricted-sign-in");
        std::fs::write(
            m.ctx_home().join(".codex").join("config.toml"),
            "forced_chatgpt_workspace_id = \"there\"\n",
        )
        .expect("a config");
        let vault = m.mem.vault().services();

        let refused =
            enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2")).expect_err("refused");

        assert_eq!(refused.code(), "login_restricted", "{refused:?}");
        assert!(in_use(&m, "here-refresh"), "the login in use stays");
        assert_eq!(m.mem.vault().services(), vault, "nothing is parked");
    }

    /// Somebody signs the tool in to another account between the first read and the one
    /// made under the tool's lock. Nothing is written over the account now signed in.
    #[test]
    fn the_account_in_use_changing_before_the_write_is_refused_and_nothing_is_written() {
        for (tool, make) in MACHINES {
            let m = make("changed-before-write");
            let key = m.key("here");
            let login = signed_in(&m, "here", "here-refresh-2");
            let login = login.stored();
            let state = state::load(&m.ctx).expect("state");
            let owner =
                identify::asked(&m.ctx, &state, provider::document(m.which), login.document)
                    .expect("whose");
            let mut state = state::load(&m.ctx).expect("state");
            let (SignedInNow::Theirs(live, first), _) = signed_in_now(
                &m.ctx,
                Permit::for_a_test(),
                &mut state,
                provider::document(m.which),
                &owner,
            ) else {
                panic!("{tool}: `here` is signed in");
            };

            m.sign_in(&login_of(&m, "other", "other-refresh"));
            let vault = m.mem.vault().services();
            let recorded = serde_json::to_value(state::load(&m.ctx).expect("state")).unwrap();
            let refused = install_signed_in(
                &m.ctx,
                provider::document(m.which),
                &key,
                &mut state,
                &login,
                &owner,
                &live,
                &first,
            )
            .expect_err("refused");

            assert_eq!(refused.code(), "sign_in_not_kept", "{tool}: {refused}");
            assert!(
                refused
                    .to_string()
                    .contains("was signed in to another account meanwhile"),
                "{tool}: {refused}"
            );
            assert!(in_use(&m, "other-refresh"), "{tool}: the other login stays");
            assert_eq!(m.mem.vault().services(), vault, "{tool}");
            assert_eq!(
                serde_json::to_value(state::load(&m.ctx).expect("state")).unwrap(),
                recorded,
                "{tool}: and nothing is recorded"
            );
        }
    }

    /// A write that fails and changes nothing leaves the old login in use, and the new one
    /// goes with the sign-in. That is said as a sign-in's failure: the new login was not
    /// kept, and signing in again is the way on.
    #[test]
    fn a_new_login_that_cannot_be_written_leaves_the_old_one_in_use() {
        for (tool, make) in MACHINES {
            let m = make("again-write-fails");
            let vault = m.mem.vault().services();
            let login = signed_in(&m, "here", "here-refresh-2");
            m.fault_live(Fault::FailWrite("refused".into()));

            let failed = enrolled_as(&m, "here", login).expect_err("the write failed");

            assert_eq!(failed.code(), "sign_in_not_kept", "{tool}: {failed}");
            let said = failed.to_string();
            assert!(said.contains("was not kept"), "{tool}: {said}");
            assert!(
                said.contains(&format!("pitboard enroll {} --sign-in", m.key("here"))),
                "{tool}: {said}"
            );
            assert!(!said.contains("nothing was lost"), "{tool}: {said}");
            assert!(in_use(&m, "here-refresh"), "{tool}");
            assert_eq!(m.mem.vault().services(), vault, "{tool}");
        }
    }

    /// The new login was written and was gone again before it was read back. The tool may
    /// have no login for the account now, so the new one, the one copy known to be good, is
    /// parked rather than thrown away, and the failure says so.
    #[test]
    fn a_new_login_that_did_not_hold_is_parked_rather_than_lost() {
        for (tool, make) in MACHINES {
            let m = make("again-did-not-hold");
            let login = signed_in(&m, "here", "here-refresh-2");
            m.fault_live(Fault::DeletedAfterWrite);

            let failed = enrolled_as(&m, "here", login).expect_err("it did not hold");

            assert!(
                matches!(failed, Error::SignInNotInstalled { parked: true, .. }),
                "{tool}: {failed:?}"
            );
            assert!(
                failed.to_string().contains("parked, so it is not lost"),
                "{tool}: {failed}"
            );
            assert_eq!(
                park_of(&m, "here").map(|p| p.login),
                Some(store::fingerprint("here-refresh-2")),
                "{tool}"
            );
            hold(&m, &format!("{tool}, after a new login that did not hold"));
        }
    }

    /// The new login was written and could not be read back: the store locked as it took
    /// the write, or at the read that follows. The slot holds the new login, and it is parked
    /// as well, one refresh token in two places that nothing records. A renewal of the copy
    /// would spend the token the tool is using, so none is made: the next renewal once the
    /// slot can be read drops the copy instead, and so does the next change.
    #[test]
    fn a_new_login_that_could_not_be_read_back_is_not_kept_beside_itself() {
        for (tool, make) in MACHINES {
            for locked in ["by the write", "after it"] {
                for next in ["renewal", "change"] {
                    let at = format!("{tool}, locked {locked}, then a {next}");
                    let m = make(&format!("unread-{}-{next}", locked.replace(' ', "-")));
                    let login = signed_in(&m, "here", "here-refresh-2");
                    let (store, service) = m.live_store();
                    let failed = if locked == "by the write" {
                        store.fault(&service, Fault::LocksAfterWrite);
                        enrolled_as(&m, "here", login)
                    } else {
                        crate::fault::meanwhile(
                            "enroll.installed",
                            move || store.fault(&service, Fault::Unreadable("locked".into())),
                            || enrolled_as(&m, "here", login),
                        )
                    }
                    .expect_err("it could not be read back");
                    assert!(
                        matches!(failed, Error::SignInNotInstalled { parked: true, .. }),
                        "{at}: {failed:?}"
                    );
                    assert_eq!(
                        park_of(&m, "here").map(|p| p.login),
                        Some(store::fingerprint("here-refresh-2")),
                        "{at}: parked as well"
                    );

                    m.live_store().0.heal_all();
                    match next {
                        "renewal" => {
                            // Late enough that the copy's access token has lapsed, which is
                            // when a renewal would take it.
                            let later = m
                                .ctx
                                .clone()
                                .with_clock(Arc::new(FixedClock::at(NOW + 11 * 86_400)));
                            renews(&m, "here-refresh-2", "here-refresh-3");
                            let renewed = renew_due(&later, Permit::for_a_test(), Due::ToBeAsked);
                            assert!(
                                renewed.iter().all(|(key, _)| key.label != "here"),
                                "{at}: {renewed:?}"
                            );
                        }
                        _ => {
                            settle(&m.ctx, Permit::for_a_test(), None).expect("the next change");
                        }
                    }
                    assert!(in_use(&m, "here-refresh-2"), "{at}");
                    assert!(park_of(&m, "here").is_none(), "{at}: the copy is dropped");
                    hold(&m, &at);
                }
            }
        }
    }

    /// A running `codex` keeps the login it started with and writes it back when it
    /// refreshes its token, so a new login put in use is said to need them restarted.
    /// Claude Code sessions read the new login by themselves, and nothing is said.
    #[test]
    fn sessions_running_on_the_old_login_are_counted_and_warned_about() {
        for (tool, make) in MACHINES {
            let m = make("again-sessions");
            m.mem.runs("codex", 2);
            m.mem.runs("claude", 2);

            let (_, warnings) = enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2"))
                .unwrap_or_else(|e| panic!("{tool}: {e}"));

            let said = warnings
                .iter()
                .find(|w| w.code() == "sessions_keep_old_login");
            match Tool::of(m.which) {
                Tool::Claude => assert!(said.is_none(), "{warnings:?}"),
                Tool::Codex => {
                    let text = said.expect("warned").to_string();
                    assert!(text.contains("2 `codex` sessions"), "{text}");
                    assert!(text.contains("`codex/here`'s old login"), "{text}");
                    assert!(text.contains("Quit them and start again"), "{text}");
                    assert!(text.contains("put the old login back"), "{text}");
                }
            }
            assert!(
                warnings
                    .iter()
                    .all(|w| w.code() != "sessions_still_running"),
                "{tool}: nobody switched away from anything: {warnings:?}"
            );
        }
    }

    /// Where the process list cannot be read, nobody can say whether a running `codex` has
    /// the old login to write back, and the sign-in says so rather than nothing. Claude Code
    /// sessions read the new login by themselves, and nothing is said.
    #[test]
    fn a_process_list_nobody_could_read_is_said_after_a_sign_in() {
        for (tool, make) in MACHINES {
            let m = make("again-listless");
            m.mem.without_a_process_list();

            let (_, warnings) = enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2"))
                .unwrap_or_else(|e| panic!("{tool}: {e}"));

            let said = warnings.iter().find(|w| w.code() == "sessions_unknown");
            match Tool::of(m.which) {
                Tool::Claude => assert!(said.is_none(), "{warnings:?}"),
                Tool::Codex => {
                    let text = said
                        .unwrap_or_else(|| panic!("warned: {warnings:?}"))
                        .to_string();
                    assert!(
                        text.starts_with(
                            "Pitboard could not tell whether Codex sessions started before \
                             this sign-in are still running"
                        ),
                        "{text}"
                    );
                    assert!(text.contains("`codex/here`'s old login"), "{text}");
                    assert!(text.contains("put the old login back"), "{text}");
                }
            }
            assert!(
                warnings
                    .iter()
                    .all(|w| w.code() != "sessions_keep_old_login"),
                "{tool}: nothing is said to be running: {warnings:?}"
            );
        }
    }

    /// A sign-in a script plays, as a fixture's is, standing in for whichever tool it is
    /// told of and for the person at the browser.
    #[derive(Debug, Default)]
    struct Played {
        /// Each sign-in started: its tool, whom it is for, and its private directory.
        started: std::sync::Mutex<Vec<(ProviderId, Option<String>, PathBuf)>>,
        /// The login each one stores once a line is typed back, as its tool writes it.
        login: std::sync::Mutex<Option<(Arc<crate::host::memory::MemoryHost>, String)>>,
        /// Every line typed back to any of them.
        typed: Arc<std::sync::Mutex<Vec<String>>>,
    }

    /// What a played sign-in is told.
    enum Told {
        Typed,
        Stopped,
    }

    struct Playing {
        told: std::sync::mpsc::Sender<Told>,
        typed: Arc<std::sync::Mutex<Vec<String>>>,
        ended: Option<std::thread::JoinHandle<bool>>,
    }

    impl SignInScript for Played {
        fn start(
            &self,
            which: ProviderId,
            label: Option<&str>,
            dir: &std::path::Path,
            say: std::sync::mpsc::Sender<String>,
        ) -> Box<dyn ScriptedSignIn> {
            self.started.lock().expect("a test's own lock").push((
                which,
                label.map(str::to_owned),
                dir.to_path_buf(),
            ));
            let login = self.login.lock().expect("a test's own lock").clone();
            let dir = dir.to_path_buf();
            let (told, hears) = std::sync::mpsc::channel();
            let ended = std::thread::spawn(move || {
                let _ = say.send("Opening browser to sign in\u{2026}\n".into());
                match hears.recv() {
                    Ok(Told::Typed) => {}
                    Ok(Told::Stopped) | Err(_) => return false,
                }
                let Some((host, document)) = login else {
                    return false;
                };
                // Where each tool stores the login of a sign-in into `dir`.
                match Tool::of(which) {
                    Tool::Claude => host.live().plant(
                        &provider::claude::slot::service_for_dir(&dir.to_string_lossy()),
                        &document,
                    ),
                    Tool::Codex => host
                        .file_at(dir.join("auth.json"))
                        .plant("auth.json", &document),
                }
                let _ = say.send("Login successful.\n".into());
                true
            });
            Box::new(Playing {
                told,
                typed: Arc::clone(&self.typed),
                ended: Some(ended),
            })
        }
    }

    impl ScriptedSignIn for Playing {
        fn typed(&mut self, line: &str) -> bool {
            self.typed
                .lock()
                .expect("a test's own lock")
                .push(line.into());
            self.told.send(Told::Typed).is_ok()
        }

        fn wait(&mut self) -> bool {
            self.ended
                .take()
                .is_some_and(|ended| ended.join().unwrap_or(false))
        }

        fn stop(&mut self) {
            let _ = self.told.send(Told::Stopped);
        }
    }

    /// `ctx` with each tool's program named where there is none.
    fn nowhere(ctx: &Context) -> Context {
        let missing = ctx.home().join("no-such-directory");
        ctx.clone()
            .with_claude_program(missing.join("claude"))
            .with_codex_program(missing.join("codex"))
    }

    /// A test or a fixture that may start no program plays the tool's sign-in with a script,
    /// and everything else is the core's own: the private directory it reserves, the login
    /// read back from where the tool stores it there, and the enrolment. The script is told
    /// whom the sign-in is for, as the person at the browser knows, and hears what is typed
    /// back. Each tool's program is named where there is none, so a seam that let the tool's
    /// program start would fail to start one, never start this machine's own.
    #[test]
    fn a_scripted_sign_in_enrols_what_it_stored_without_starting_the_tool() {
        for (tool, make) in MACHINES {
            let m = make("scripted");
            let played = Arc::new(Played::default());
            *played.login.lock().expect("a test's own lock") = Some((
                Arc::clone(&m.mem),
                login_of(&m, "newcomer", "newcomer-refresh").to_string(),
            ));
            let ctx = nowhere(&m.ctx).with_sign_in_script(Arc::clone(&played) as _);

            let mut watched = sign_in_watched_as(
                &ctx,
                Permit::for_a_test(),
                provider::document(m.which),
                &m.key("newcomer"),
            )
            .unwrap_or_else(|e| panic!("{tool}: {e}"));
            let said = watched.said();
            assert_eq!(
                said.next().as_deref(),
                Some("Opening browser to sign in\u{2026}\n"),
                "{tool}"
            );
            watched.paste("code#state").expect("typed back");
            let login = watched.finish().unwrap_or_else(|e| panic!("{tool}: {e}"));
            assert_eq!(
                said.next().as_deref(),
                Some("Login successful.\n"),
                "{tool}"
            );
            assert_eq!(
                said.next(),
                None,
                "{tool}: the tool has stopped saying anything"
            );

            assert_eq!(
                *played.typed.lock().expect("a test's own lock"),
                ["code#state"],
                "{tool}"
            );
            let started = played.started.lock().expect("a test's own lock").clone();
            assert_eq!(started.len(), 1, "{tool}");
            assert_eq!(
                (started[0].0, started[0].1.as_deref()),
                (m.which, Some("newcomer")),
                "{tool}"
            );
            assert!(
                started[0].2.starts_with(home::dir(&m.ctx)),
                "{tool}: the core's own private directory"
            );
            let (enrolled, _) =
                enrolled_as(&m, "newcomer", login).unwrap_or_else(|e| panic!("{tool}: {e}"));
            assert!(
                matches!(enrolled, Enrolled::SignedIn { ref email } if email == "newcomer@example.com"),
                "{tool}: {enrolled:?}"
            );
            assert!(park_of(&m, "newcomer").is_some(), "{tool}: parked");
        }
    }

    /// A played sign-in that is stopped ends what it says and enrols nothing, as a killed
    /// tool does, and the next one can start.
    #[test]
    fn a_stopped_scripted_sign_in_enrols_nothing() {
        for (tool, make) in MACHINES {
            let m = make("scripted-stop");
            let played = Arc::new(Played::default());
            let ctx = nowhere(&m.ctx).with_sign_in_script(Arc::clone(&played) as _);
            let watched = start_watched(
                &ctx,
                Permit::for_a_test(),
                provider::document(m.which),
                None,
            )
            .unwrap_or_else(|e| panic!("{tool}: {e}"));
            let said = watched.said();
            assert!(said.next().is_some(), "{tool}");
            watched.cancel();
            assert_eq!(said.next(), None, "{tool}");
            let started = played.started.lock().expect("a test's own lock").clone();
            assert_eq!(started[0].1, None, "{tool}: nobody said whom it was for");
            assert!(park_of(&m, "newcomer").is_none(), "{tool}");
            reserve_signin(&ctx, Permit::for_a_test(), provider::document(m.which))
                .expect("a stopped sign-in lets the next one start");
        }
    }

    #[test]
    fn before_a_codex_sign_in_on_windows_what_codex_runs_is_said() {
        let mem = crate::host::memory::MemoryHost::new();
        let on = |os| {
            let env = crate::context::Environment::on(
                os,
                [("HOME", "/nowhere"), ("USERPROFILE", r"C:\nowhere")],
            );
            Context::for_command_line(&env).with_memory_stores(Arc::clone(&mem))
        };
        assert_eq!(
            running_already(&on(crate::host::Os::Windows), ProviderId::Codex),
            None
        );
        mem.runs("codex", 2);
        assert_eq!(
            running_already(&on(crate::host::Os::Windows), ProviderId::Codex).as_deref(),
            Some(
                "2 `codex` sessions are running. A Codex sign-in open in them ends as this one \
                 starts.\n"
            )
        );
        assert_eq!(
            running_already(&on(crate::host::Os::Windows), ProviderId::Claude),
            None
        );
        assert_eq!(
            running_already(&on(crate::host::Os::MacOs), ProviderId::Codex),
            None
        );

        let home = tempfile::tempdir().expect("a scratch home");
        let ctx = Context::new(home.path().to_path_buf())
            .with_pitboard_home(home.path().join(".pitboard"))
            .with_memory_stores(Arc::clone(&mem));
        let (pending, _) = reserve_signin(
            &ctx,
            Permit::for_a_test(),
            provider::document(ProviderId::Codex),
        )
        .expect("reserved");
        let mut quiet = std::process::Command::new(std::env::current_exe().expect("this test"));
        quiet.args(["--list", "--exact", "pitboard-no-such-test"]);
        let watched = watch(quiet, pending, Some("first\n".into())).expect("started");
        let said = watched.said();
        assert_eq!(
            said.within(Duration::from_secs(20)).as_deref(),
            Ok("first\n")
        );
        let _ = watched.finish();
    }

    // A browser a Codex sign-in opens on Windows inherits codex.exe's piped stdout and stderr,
    // so the pipe stays open after codex exits. The watch must stop reading once codex itself
    // is gone, or said() never ends and a finished sign-in is never enrolled.
    #[cfg(windows)]
    #[test]
    fn a_sign_in_goes_quiet_when_the_tool_exits_though_its_browser_holds_the_pipes() {
        use crate::helper::{self, Job};
        let home = tempfile::tempdir().expect("a scratch home");
        let ctx = Context::new(home.path().to_path_buf())
            .with_pitboard_home(home.path().join(".pitboard"));
        let (pending, _) = reserve_signin(
            &ctx,
            Permit::for_a_test(),
            provider::document(ProviderId::Codex),
        )
        .expect("reserved");
        let dir = tempfile::tempdir().expect("a scratch folder");
        let exe = std::env::current_exe().expect("this test");
        let browser_file = dir.path().join("browser.state");
        let codex_file = dir.path().join("codex.state");
        // Keeps the browser stand-in waiting while this test runs; its Drop releases it.
        let _held = helper::hold(&browser_file);
        let command = helper::goes(&exe, &codex_file, (&exe, Job::Waits, &browser_file));
        let watched = watch(command, pending, None).expect("started");
        let said = watched.said();
        let (ended, ends) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while said.next().is_some() {}
            let _ = ended.send(());
        });
        assert!(
            ends.recv_timeout(Duration::from_secs(20)).is_ok(),
            "said ends once the tool exits, though the browser still holds the pipes"
        );
        let _ = watched.finish();
    }

    /// A cancel has let go of the core's one sign-in at a time by the time it returns, even
    /// while a copy of its lock's descriptor is open elsewhere, as one is in every process
    /// another thread is starting until that process runs its program. Closing the file let
    /// go of it only once every copy was closed, so the next sign-in was refused as one
    /// already waiting for as long as that took, which ARCHITECTURE.md's Measured facts give.
    #[test]
    fn a_cancelled_sign_in_lets_the_next_start_while_a_copy_of_its_lock_is_open() {
        for (tool, make) in MACHINES {
            let m = make("lock-copied");
            let played = Arc::new(Played::default());
            let ctx = nowhere(&m.ctx).with_sign_in_script(Arc::clone(&played) as _);
            let watched = start_watched(
                &ctx,
                Permit::for_a_test(),
                provider::document(m.which),
                None,
            )
            .unwrap_or_else(|e| panic!("{tool}: {e}"));
            let copy = watched
                .pending
                .one_at_a_time
                .try_clone()
                .expect("a copy of the lock's descriptor");
            watched.cancel();
            let next = reserve_signin(&ctx, Permit::for_a_test(), provider::document(m.which));
            drop(copy);
            assert!(
                next.is_ok(),
                "{tool}: the next sign-in refused: {:?}",
                next.err()
            );
        }
    }

    /// Codex prints its address and then nothing until the browser is done, so somebody
    /// pressing Cancel nearly always finds a reader waiting. The cancel must not wait with
    /// it, and stopping the tool must end the reading rather than leave it waiting forever.
    #[test]
    fn a_cancel_does_not_wait_on_a_tool_that_says_nothing() {
        let patience = Duration::from_secs(20);
        let m = codex_machine("cancel");
        let (pending, _) = reserve_signin(
            &m.ctx,
            Permit::for_a_test(),
            provider::document(ProviderId::Codex),
        )
        .expect("reserved");
        // Says its address, then nothing, and ends by itself only once `patience` is up.
        let silent = crate::helper::command(
            &std::env::current_exe().expect("this test"),
            crate::helper::Job::SaysAnAddress,
            &m.ctx_home().join("silent"),
        );
        let watched = watch(silent, pending, None).expect("started");
        let said = watched.said();
        let (heard, hears) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Some(text) = said.next() {
                let _ = heard.send(Some(text));
            }
            let _ = heard.send(None);
        });
        assert!(hears.recv_timeout(patience).expect("the address").is_some());

        let (cancelled, cancels) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            watched.cancel();
            let _ = cancelled.send(());
        });
        cancels
            .recv_timeout(patience)
            .expect("the cancel did not wait on the tool");
        // libtest's own lines may still be on their way.
        let ended = std::iter::from_fn(|| hears.recv_timeout(patience).ok()).find(Option::is_none);
        assert_eq!(ended, Some(None), "the reading ended");
        reserve_signin(
            &m.ctx,
            Permit::for_a_test(),
            provider::document(ProviderId::Codex),
        )
        .expect("a cancelled sign-in lets the next one start");
    }
}
