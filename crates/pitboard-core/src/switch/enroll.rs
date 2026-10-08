//! Bringing an account under Pitboard's care.
//!
//! A parked copy is only safe if the live slot is replaced the moment it is taken; otherwise
//! the tool keeps rotating the same token and the copy goes stale, and for a tool whose
//! sign-out revokes what it finds, a copy left beside the live login is one the person's
//! own next sign-out would end. So the account signed in now is recorded but not parked.
//! Its first switch parks it at exactly that moment, and any other account is signed in
//! inside a private directory, where the live slot is never touched and the vault is the
//! new login's only holder.
//!
//! A sign-in to the account signed in now keeps the same rule. That account is not parked,
//! so its new login is put in use the way a switch puts one there, in place of the old.

use super::{Error, Readied, Result, Settled, identify, identify_document, purge};
use crate::api::Owner;
use crate::context::Context;
use crate::in_use::{self, InUse};
use crate::provider::{self, ProviderId};
use crate::service::{Permit, Warning};
use crate::state::{Account, Key, Park, State};
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

/// A login a tool stored for Pitboard in a private directory, not yet enrolled. Dropping it
/// deletes that directory and whatever the tool kept for it elsewhere.
pub struct SignIn {
    provider: ProviderId,
    dir: PathBuf,
    document: Value,
    ctx: Context,
    /// What it was reserved with: what enrolling it writes with, and what lets dropping it
    /// remove what it reserved.
    permit: Permit,
    /// The lock on `signin.lock` that makes this the one sign-in at a time, let go of as this
    /// is dropped.
    one_at_a_time: File,
}

impl SignIn {
    /// Which tool this login is for.
    pub fn provider(&self) -> ProviderId {
        self.provider
    }
}

impl Drop for SignIn {
    /// Clears the private directory, then lets go of the lock, so the next sign-in can start
    /// the moment this returns.
    ///
    /// Let go of by name rather than by closing the file: a lock taken with `flock` belongs to
    /// the open file, which every process another thread is starting holds a copy of until
    /// it runs its program, though std opens the file so that the program does not keep it.
    /// Closed, the lock stayed held until the last copy went, for up to milliseconds, as
    /// ARCHITECTURE.md's Measured facts say, and a sign-in started in that time was refused
    /// as one already waiting. Let go of, it is free whatever copies are still open.
    fn drop(&mut self) {
        provider::of(self.provider).discard_signin(&self.ctx, self.permit, &self.dir);
        let _ = crate::host::fs::remove_dir_all(self.permit, &self.dir);
        let _ = self.one_at_a_time.unlock();
    }
}

/// Takes the one-sign-in-at-a-time lock and prepares the private directory the tool will
/// sign in to. Both the inherited and the watched sign-in start here.
///
/// A sign-in waits on a person in a browser, so it takes no lock but its own: a switch
/// meanwhile goes ahead, and a second sign-in is refused rather than queued.
fn reserve_signin(ctx: &Context, permit: Permit, which: ProviderId) -> Result<SignIn> {
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

    let dir = home.join("signin");
    // A sign-in that was killed rather than finished never ran its cleanup, so a login can
    // be sitting in the scratch slot with nothing naming it. The directory is always the
    // same one, so the slot is too, and this is the moment it can be cleared safely: the
    // lock above means no other sign-in is using it. Every tool's leftovers, because the
    // one that was killed need not be the one starting now.
    for &tool in ProviderId::ALL {
        provider::of(tool).discard_signin(ctx, permit, &dir);
    }
    let _ = crate::host::fs::remove_dir_all(permit, &dir);
    crate::host::fs::create_private_dir(permit, &dir).map_err(|source| Error::HomeUnwritable {
        path: dir.clone(),
        source,
    })?;
    Ok(SignIn {
        provider: which,
        dir,
        document: Value::Null,
        ctx: ctx.clone(),
        permit,
        one_at_a_time,
    })
}

/// A sign-in that never ran. The crash matrix needs the state a finished sign-in leaves,
/// and running a tool's own login inside a test is neither possible nor wanted. The app
/// model's tests in `pitboard-ffi` need it for the same reason, through `testing`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn planted(
    ctx: &Context,
    permit: Permit,
    which: ProviderId,
    document: Value,
) -> Result<SignIn> {
    let mut pending = reserve_signin(ctx, permit, which)?;
    pending.document = document;
    Ok(pending)
}

/// Run the tool's own sign-in in a private directory, where the live login is never
/// touched, and read back the login it stored there.
pub fn sign_in(ctx: &Context, permit: Permit, which: ProviderId) -> Result<SignIn> {
    let mut pending = reserve_signin(ctx, permit, which)?;
    // Pitboard never sees the sign-in; it reads the login the tool stores once it is done.
    // What the tool prints goes to stderr, so `--json` output stays one JSON line.
    let finished = provider::of(which)
        .sign_in(ctx, permit, &pending.dir)
        .stdout(std::io::stderr())
        .status()
        .map_err(|e| started(which, e))?
        .success();
    if !finished {
        return Err(Error::SignInIncomplete);
    }
    pending.document = signed_in_document(ctx, which, &pending.dir)?;
    Ok(pending)
}

fn started(which: ProviderId, e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::NotFound => Error::ProgramNotFound { tool: which },
        _ => Error::SignInIncomplete,
    }
}

fn signed_in_document(ctx: &Context, which: ProviderId, dir: &std::path::Path) -> Result<Value> {
    let raw = provider::of(which)
        .read_signin(ctx, dir)?
        .ok_or(Error::SignInIncomplete)?;
    serde_json::from_str(&raw).map_err(|e| Error::LiveCredentialShapeUnexpected {
        tool: which,
        detail: e.to_string(),
    })
}

/// The same sign-in, watched rather than inherited: an app has no terminal to hand over, so
/// it reads what the tool prints and can type a fallback code back where the tool asks for
/// one.
pub struct WatchedSignIn {
    tool: Running,
    said: Said,
    pending: SignIn,
}

/// The tool a watched sign-in runs: its program, or, where a test or a fixture says, a script
/// playing it.
enum Running {
    Program(std::process::Child),
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
pub trait ScriptedSignIn: Send {
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
#[derive(Clone)]
pub struct Said(std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<String>>>);

impl Said {
    /// The next thing the tool said, or `None` once it has finished saying anything.
    /// Blocks, so a caller reads it on a thread of its own.
    pub fn next(&self) -> Option<String> {
        self.0.lock().ok()?.recv().ok()
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

    /// Types a line back, for a code the tool reads from stdin while it waits on the
    /// browser.
    pub fn paste(&mut self, line: &str) -> Result<()> {
        match &mut self.tool {
            Running::Program(child) => {
                use std::io::Write;
                let stdin = child.stdin.as_mut().ok_or(Error::SignInIncomplete)?;
                writeln!(stdin, "{line}").map_err(|_| Error::SignInIncomplete)?;
                stdin.flush().map_err(|_| Error::SignInIncomplete)
            }
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
            Running::Program(child) => child.wait().map_err(|_| Error::SignInIncomplete)?.success(),
            #[cfg(any(test, feature = "test-support"))]
            Running::Scripted(run) => run.wait(),
        };
        if !finished {
            return Err(Error::SignInIncomplete);
        }
        let mut pending = self.pending;
        pending.document =
            signed_in_document(&pending.ctx.clone(), pending.provider, &pending.dir.clone())?;
        Ok(pending)
    }

    /// Stops it, and returns once its tool has ended and been waited for and the one sign-in
    /// at a time has been let go of, so a sign-in started after this returns is not refused as
    /// one already waiting. What it may have written is discarded by `SignIn`'s own cleanup.
    pub fn cancel(mut self) {
        match &mut self.tool {
            Running::Program(child) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            #[cfg(any(test, feature = "test-support"))]
            Running::Scripted(run) => {
                run.stop();
                let _ = run.wait();
            }
        }
    }
}

/// Starts the sign-in with its output piped, for a caller that will show it.
pub fn sign_in_watched(ctx: &Context, permit: Permit, which: ProviderId) -> Result<WatchedSignIn> {
    start_watched(ctx, permit, which, None)
}

/// The same, for the account `key` names, which a script playing the tool is told of: the
/// person at the browser knows which account they mean, where the tool never does.
pub(crate) fn sign_in_watched_as(
    ctx: &Context,
    permit: Permit,
    key: &Key,
) -> Result<WatchedSignIn> {
    start_watched(ctx, permit, key.provider, Some(&key.label))
}

fn start_watched(
    ctx: &Context,
    permit: Permit,
    which: ProviderId,
    label: Option<&str>,
) -> Result<WatchedSignIn> {
    let pending = reserve_signin(ctx, permit, which)?;
    #[cfg(any(test, feature = "test-support"))]
    if let Some(script) = ctx.sign_in_script() {
        let (say, said) = std::sync::mpsc::channel();
        return Ok(WatchedSignIn {
            tool: Running::Scripted(script.start(which, label, &pending.dir, say)),
            said: Said(std::sync::Arc::new(std::sync::Mutex::new(said))),
            pending,
        });
    }
    // Only a script playing the tool is told whom the sign-in is for.
    let _ = label;
    let command = provider::of(which).sign_in(ctx, permit, &pending.dir);
    watch(command, pending).map_err(|e| started(which, e))
}

/// Runs `command` with its output piped, as the sign-in `pending` reserved.
fn watch(mut command: std::process::Command, pending: SignIn) -> std::io::Result<WatchedSignIn> {
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let (say, said) = std::sync::mpsc::channel();
    // Claude Code writes the browser URL and the paste prompt without a newline after them,
    // so this reads by chunk rather than by line and lets the caller decide what to show.
    // Codex writes its address to stderr, which is read the same way.
    for stream in [
        child.stdout.take().map(Readable::Out),
        child.stderr.take().map(Readable::Err),
    ]
    .into_iter()
    .flatten()
    {
        let say = say.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut reader: Box<dyn Read + Send> = match stream {
                Readable::Out(o) => Box::new(o),
                Readable::Err(e) => Box::new(e),
            };
            let mut buffer = [0_u8; 1024];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
                if say.send(text).is_err() {
                    break;
                }
            }
        });
    }
    Ok(WatchedSignIn {
        tool: Running::Program(child),
        said: Said(std::sync::Arc::new(std::sync::Mutex::new(said))),
        pending,
    })
}

enum Readable {
    Out(std::process::ChildStdout),
    Err(std::process::ChildStderr),
}

/// Enroll the account signed in to `which` now, or with `signed_in`, the one a sign-in just
/// produced.
pub fn enroll(
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
        Some(login) => from_sign_in(&ctx, key, &mut state, &login),
        None => record_current(&ctx, permit, key, &mut state).map(|e| (e, Vec::new())),
    }
}

/// A label names one account of its tool for good: its own, or one not enrolled under
/// another label.
fn claim(state: &State, key: &Key, owner: &Owner) -> Result<()> {
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
        });
    }
    Ok(())
}

fn record_current(ctx: &Context, permit: Permit, key: &Key, state: &mut State) -> Result<Enrolled> {
    let which = key.provider;
    let label = key.label.as_str();
    // Through the store itself rather than the provider's reading of it, so a locked
    // keychain says so in the store's own words instead of reading as a strange login. A
    // document with no account in it is nobody signed in: Claude Code's after a `/logout`
    // still holds the machine's MCP tokens.
    let (stored, found) = identify::look(ctx, state, which)?;
    let identify::Live::Login {
        document: live,
        owner,
        ..
    } = stored
    else {
        return Err(Error::LiveCredentialAbsent { tool: which });
    };
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
        &live,
    ));
    // Recorded once the account is enrolled, so the activity log names it.
    let (_, noticed) = identify::record(state, which, found, ctx.now());
    state::save(ctx, permit, state)?;
    identify::write_down(ctx, permit, &noticed);
    Ok(Enrolled::Current { email: owner.email })
}

/// Enrol the account a sign-in produced: put in use when it is the account signed in now,
/// and parked when it is any other.
///
/// Somebody signs in again to the account in use because its login is broken or about to
/// lapse. Parked, the new login left the tool on the old one, and the next switch away
/// parked the old one over it.
fn from_sign_in(
    ctx: &Context,
    key: &Key,
    state: &mut State,
    login: &SignIn,
) -> Result<(Enrolled, Vec<Warning>)> {
    let owner = identify_document(ctx, login.provider, &login.document)?;
    claim(state, key, &owner)?;
    match signed_in_now(ctx, login.permit, state, key.provider, &owner) {
        SignedInNow::Theirs(live, first) => {
            install_signed_in(ctx, key, state, login, &owner, &live, &first)
        }
        SignedInNow::NotTheirs => park_signed_in(ctx, key, state, login, &owner),
        SignedInNow::Untold(why) => {
            // Somebody signing in to the account Pitboard last saw in use most likely wants
            // its broken login replaced, and parking is not that, so it is said.
            let last_in_use = state
                .account_in_use(key.provider)
                .is_some_and(|account| account.is(key));
            let (enrolled, mut warnings) = park_signed_in(ctx, key, state, login, &owner)?;
            if last_in_use {
                warnings.push(Warning::SignInParkedNotInUse {
                    tool: key.provider,
                    label: state.typed(key),
                    why: untold(&why),
                });
            }
            Ok((enrolled, warnings))
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

fn signed_in_now(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    which: ProviderId,
    owner: &Owner,
) -> SignedInNow {
    match identify::now(ctx, permit, state, which) {
        Ok(identify::Live::Login {
            store,
            document,
            owner: found,
        }) if found.same_login(owner) => SignedInNow::Theirs(store, document),
        Ok(_) => SignedInNow::NotTheirs,
        Err(e) => SignedInNow::Untold(e),
    }
}

/// Why the login in use could not be told, in a few words. The warning it goes into says
/// what to do, and an error's own advice would be about something else.
fn untold(error: &Error) -> String {
    match error {
        Error::SessionExpired { tool } => format!("{} refused its access token", tool.service()),
        Error::IdentityUnverifiable { detail, .. }
        | Error::LiveCredentialShapeUnexpected { detail, .. } => detail.clone(),
        Error::LiveStoreUnsupported { reason, .. } => reason.clone(),
        Error::LiveCredentialElsewhere { email } => {
            format!("its config names {email}, and Pitboard cannot find that login")
        }
        Error::Store(e) => e.to_string(),
        other => other.code().replace('_', " "),
    }
}

/// Put the new login of the account signed in now in place of its old one, under the rules
/// a switch writes by, and record the account as `record_current` does. Nothing is parked,
/// and a park the account already holds is kept. The old login is dropped, which is what
/// the tool's own sign-in does to the login it replaces.
fn install_signed_in(
    ctx: &Context,
    key: &Key,
    state: &mut State,
    login: &SignIn,
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
    let slice = provider::of(which)
        .slice(&login.document)
        .map_err(|e| super::shape(which, e))?;
    let guard = super::write_lock(ctx, permit, which)?;
    let Readied {
        before_raw,
        next,
        on_the_command_line,
        ..
    } = super::ready(ctx, which, live, first, owner, &slice, &name).map_err(|e| match e {
        Error::SignedInAccountChanged => not_kept(format!(
            "{} was signed in to another account meanwhile",
            which.name()
        )),
        other => other,
    })?;

    let written = on_the_command_line.into_iter().collect::<Vec<_>>();
    match super::install_with(
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
                ctx, key, state, login, owner, detail, written,
            ));
        }
        Err(other) => return Err(other),
    }
    crate::fault::point("enroll.installed");

    // Read back as a switch reads back: a write that landed has not necessarily held.
    let lock_lost = guard.as_ref().is_some_and(lock::Guard::compromised);
    let lost = match super::holds(which, live) {
        Ok(true) => None,
        Ok(false) => Some("it was gone again before Pitboard finished".to_string()),
        Err(unreadable) => Some(unreadable.to_string()),
    };
    if let Some(detail) = lost {
        return Err(not_installed(
            ctx, key, state, login, owner, detail, written,
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
        &login.document,
    ));
    let installed = InUse {
        owner: Some(owner.clone()),
        login: provider::of(which).fingerprint(&slice),
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
fn not_installed(
    ctx: &Context,
    key: &Key,
    state: &mut State,
    login: &SignIn,
    owner: &Owner,
    detail: String,
    mut warnings: Vec<Warning>,
) -> Error {
    let parked = match park_signed_in(ctx, key, state, login, owner) {
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
        warnings,
    }
}

fn park_signed_in(
    ctx: &Context,
    key: &Key,
    state: &mut State,
    login: &SignIn,
    owner: &Owner,
) -> Result<(Enrolled, Vec<Warning>)> {
    let permit = login.permit;
    let label = key.label.as_str();
    let id = state.id_of(login.provider, owner);
    let slice = provider::of(login.provider)
        .slice(&login.document)
        .map_err(|e| super::shape(login.provider, e))?;
    let parking = park::price(
        ctx,
        login.provider,
        &key.typed(),
        &park::service_name(&id, ctx.now_millis()),
        &slice,
    )?;
    let service = park::reserve(ctx, permit, &id)?;
    let fresh = park::store_at(ctx, permit, key.provider, &service, &slice)?;
    // The window the roadmap named: the login is in the vault and nothing on the machine
    // says so yet.
    crate::fault::point("enroll.park_stored");
    let existing = state.get(key);
    let previous = existing.and_then(|a| a.parked.clone());
    let renewed = existing.is_some();
    let last_used_at = existing.and_then(|a| a.last_used_at);
    state.upsert(account(
        login.provider,
        id,
        label,
        owner,
        previous,
        last_used_at,
        &login.document,
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
/// Code fetch its own profile after a switch rather than trust a copy Pitboard wrote. For
/// Codex there is no such cache to correct, and what is kept instead is what its own login
/// already said, which costs nothing to read and explains a limit somebody is surprised by.
fn account(
    which: ProviderId,
    id: String,
    label: &str,
    owner: &Owner,
    parked: Option<Park>,
    last_used_at: Option<i64>,
    login: &Value,
) -> Account {
    let detail = match which {
        ProviderId::Claude => state::Detail::Claude {
            organization_uuid: owner.organization_uuid.clone(),
            oauth_account: json!({
                "accountUuid": owner.account_uuid,
                "emailAddress": owner.email,
                "organizationUuid": owner.organization_uuid,
            }),
        },
        ProviderId::Codex => {
            let claims = login["tokens"]["id_token"]
                .as_str()
                .and_then(crate::provider::jwt::claims)
                .unwrap_or(Value::Null);
            let openai = "https://api.openai.com/auth";
            state::Detail::Codex {
                workspace_id: Some(owner.organization_uuid.clone()).filter(|id| !id.is_empty()),
                plan: crate::provider::jwt::claim(&claims, &[openai, "chatgpt_plan_type"])
                    .map(str::to_owned),
            }
        }
    };
    Account {
        last_used_at,
        replaced_at: None,
        label: label.to_string(),
        id,
        account_uuid: owner.account_uuid.clone(),
        email: owner.email.clone(),
        parked,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::scripted::Trouble;
    use crate::store::memory::Fault;
    use crate::switch::harness::{
        self, Machine, NOW, codex_id, codex_login, codex_machine, hold, login_of, machine, oauth,
        renews, signed_in,
    };
    use crate::switch::{Due, Outcome, renew_due, settle, switch};
    use crate::time::FixedClock;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    type Make = fn(&str) -> Machine;
    /// A machine of each tool: `here` signed in, `there` parked and ready.
    const MACHINES: [(&str, Make); 2] = [("claude", machine), ("codex", codex_machine)];

    /// Whether the login in use is the one this refresh token belongs to.
    fn in_use(m: &Machine, refresh: &str) -> bool {
        m.live().is_some_and(|live| {
            provider::of(m.which).fingerprint(&live) == store::fingerprint(refresh)
        })
    }

    /// Enrols what a sign-in left under `label`, the way the next command would.
    fn enrolled_as(m: &Machine, label: &str, login: SignIn) -> Result<(Enrolled, Vec<Warning>)> {
        let settled = settle(&m.ctx, Permit::for_a_test(), Some(m.which))
            .expect("nothing to recover")
            .0;
        enroll(settled, &m.key(label), Some(login))
    }

    fn park_of(m: &Machine, label: &str) -> Option<Park> {
        state::load(&m.ctx)
            .expect("state")
            .get(&m.key(label))
            .and_then(|a| a.parked.clone())
    }

    /// `here` in another organisation, signed in through `refresh`.
    fn here_in(m: &Machine, org: &str, refresh: &str) -> SignIn {
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
            ProviderId::Claude,
            harness::document(refresh),
        )
        .expect("a sign-in")
    }

    /// One Anthropic account in two organisations is two logins, each on its own plan. The
    /// account uuid is the person in both, and identified by it alone the second was
    /// refused as already enrolled, and a switch to it said the first was already in use.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn signing_in_again_to_the_account_in_use_keeps_the_park_it_holds() {
        for (tool, make) in MACHINES {
            let m = make("again-keeps-park");
            let (uuid, older) = match m.which {
                ProviderId::Claude => ("here".to_string(), oauth("here-older", 30)),
                ProviderId::Codex => (codex_id("here"), codex_login("here", "here-older")),
            };
            let service = park::reserve(&m.ctx, Permit::for_a_test(), &uuid).expect("a free name");
            let held = park::store_at(&m.ctx, Permit::for_a_test(), m.which, &service, &older)
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
                parked.refresh_fingerprint,
                store::fingerprint("here-refresh-2"),
                "{tool}: the new login is parked, not the one it replaced"
            );
            hold(&m, &format!("{tool}, after the switch away"));
        }
    }

    /// Another account's sign-in is parked, and the account in use is left exactly as it
    /// was, as before.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
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
                park_of(&m, "third").map(|p| p.refresh_fingerprint),
                Some(store::fingerprint("third-refresh")),
                "{tool}"
            );
            hold(&m, &format!("{tool}, after another account's sign-in"));
        }
    }

    /// Writing over a login whose account nobody can name could lose that account's only
    /// login, so a sign-in to `here` while the login in use cannot be told apart is parked
    /// beside it, as it always was. Each is a login renewed since its account was last
    /// named, so its fingerprint names nobody: for Claude Code one Anthropic refuses or does
    /// not answer about, for Codex one whose ID token cannot be read.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_login_in_use_whose_account_cannot_be_told_is_not_written_over() {
        let mut cases: Vec<(String, Machine)> = Vec::new();
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
                park_of(&m, "here").map(|p| p.refresh_fingerprint),
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

    /// Claude Code's config names `here` and Pitboard finds no login where it reads: the
    /// login is somewhere Pitboard does not look, so whose it is cannot be told, and `here`
    /// is still the account Pitboard last saw in use. A sign-in to `here` is parked and said
    /// as for any login whose account cannot be told, and the record still names `here`.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
        assert!(harness::in_use_lines(&m).is_empty());
    }

    /// Only the account Pitboard last saw in use is warned about. Signing in again to a
    /// parked account renews its park whoever is signed in, as it always did.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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

    /// Somebody signs the tool in to another account between the first read and the one
    /// made under the tool's lock. Nothing is written over the account now signed in.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn the_account_in_use_changing_before_the_write_is_refused_and_nothing_is_written() {
        for (tool, make) in MACHINES {
            let m = make("changed-before-write");
            let key = m.key("here");
            let login = signed_in(&m, "here", "here-refresh-2");
            let owner = identify_document(&m.ctx, m.which, &login.document).expect("whose");
            let mut state = state::load(&m.ctx).expect("state");
            let SignedInNow::Theirs(live, first) =
                signed_in_now(&m.ctx, Permit::for_a_test(), &mut state, m.which, &owner)
            else {
                panic!("{tool}: `here` is signed in");
            };

            m.sign_in(&login_of(&m, "other", "other-refresh"));
            let vault = m.mem.vault().services();
            let recorded = serde_json::to_value(state::load(&m.ctx).expect("state")).unwrap();
            let refused =
                install_signed_in(&m.ctx, &key, &mut state, &login, &owner, &live, &first)
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
                park_of(&m, "here").map(|p| p.refresh_fingerprint),
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
                        park_of(&m, "here").map(|p| p.refresh_fingerprint),
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
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
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
            match m.which {
                ProviderId::Claude => assert!(said.is_none(), "{warnings:?}"),
                ProviderId::Codex => {
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
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_process_list_nobody_could_read_is_said_after_a_sign_in() {
        for (tool, make) in MACHINES {
            let m = make("again-listless");
            m.mem.without_a_process_list();

            let (_, warnings) = enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2"))
                .unwrap_or_else(|e| panic!("{tool}: {e}"));

            let said = warnings.iter().find(|w| w.code() == "sessions_unknown");
            match m.which {
                ProviderId::Claude => assert!(said.is_none(), "{warnings:?}"),
                ProviderId::Codex => {
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
                match which {
                    ProviderId::Claude => host.live().plant(
                        &crate::provider::claude::slot::service_for_dir(&dir.to_string_lossy()),
                        &document,
                    ),
                    ProviderId::Codex => host
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
        let missing = std::env::temp_dir().join("pitboard-no-such-directory");
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_scripted_sign_in_enrols_what_it_stored_without_starting_the_tool() {
        for (tool, make) in MACHINES {
            let m = make("scripted");
            let played = Arc::new(Played::default());
            *played.login.lock().expect("a test's own lock") = Some((
                Arc::clone(&m.mem),
                login_of(&m, "newcomer", "newcomer-refresh").to_string(),
            ));
            let ctx = nowhere(&m.ctx).with_sign_in_script(Arc::clone(&played) as _);

            let mut watched = sign_in_watched_as(&ctx, Permit::for_a_test(), &m.key("newcomer"))
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
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_stopped_scripted_sign_in_enrols_nothing() {
        for (tool, make) in MACHINES {
            let m = make("scripted-stop");
            let played = Arc::new(Played::default());
            let ctx = nowhere(&m.ctx).with_sign_in_script(Arc::clone(&played) as _);
            let watched = sign_in_watched(&ctx, Permit::for_a_test(), m.which)
                .unwrap_or_else(|e| panic!("{tool}: {e}"));
            let said = watched.said();
            assert!(said.next().is_some(), "{tool}");
            watched.cancel();
            assert_eq!(said.next(), None, "{tool}");
            let started = played.started.lock().expect("a test's own lock").clone();
            assert_eq!(started[0].1, None, "{tool}: nobody said whom it was for");
            assert!(park_of(&m, "newcomer").is_none(), "{tool}");
            reserve_signin(&ctx, Permit::for_a_test(), m.which)
                .expect("a stopped sign-in lets the next one start");
        }
    }

    /// A cancel has let go of the core's one sign-in at a time by the time it returns, even
    /// while a copy of its lock's descriptor is open elsewhere, as one is in every process
    /// another thread is starting until that process runs its program. Closing the file let
    /// go of it only once every copy was closed, so the next sign-in was refused as one
    /// already waiting for as long as that took, which ARCHITECTURE.md's Measured facts give.
    #[test]
    #[cfg(unix)]
    fn a_cancelled_sign_in_lets_the_next_start_while_a_copy_of_its_lock_is_open() {
        for (tool, make) in MACHINES {
            let m = make("lock-copied");
            let played = Arc::new(Played::default());
            let ctx = nowhere(&m.ctx).with_sign_in_script(Arc::clone(&played) as _);
            let watched = sign_in_watched(&ctx, Permit::for_a_test(), m.which)
                .unwrap_or_else(|e| panic!("{tool}: {e}"));
            let copy = watched
                .pending
                .one_at_a_time
                .try_clone()
                .expect("a copy of the lock's descriptor");
            watched.cancel();
            let next = reserve_signin(&ctx, Permit::for_a_test(), m.which);
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
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_cancel_does_not_wait_on_a_tool_that_says_nothing() {
        let m = codex_machine("cancel");
        let pending =
            reserve_signin(&m.ctx, Permit::for_a_test(), ProviderId::Codex).expect("reserved");
        let mut silent = std::process::Command::new("sleep");
        silent.arg("30");
        let watched = watch(silent, pending).expect("started");
        let said = watched.said();
        let reader = std::thread::spawn(move || said.next());
        std::thread::sleep(Duration::from_millis(100));

        let asked = Instant::now();
        watched.cancel();
        assert_eq!(reader.join().expect("the reader"), None);
        assert!(
            asked.elapsed() < Duration::from_secs(10),
            "the cancel waited on the tool"
        );
        reserve_signin(&m.ctx, Permit::for_a_test(), ProviderId::Codex)
            .expect("a cancelled sign-in lets the next one start");
    }
}
