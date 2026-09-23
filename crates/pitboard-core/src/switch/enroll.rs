//! Bringing an account under pitboard's care.
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

use super::{Error, Readied, Result, Settled, identify_document, nothing_signed_in, purge};
use crate::api::Owner;
use crate::context::Context;
use crate::provider::{self, ProviderId};
use crate::service::Warning;
use crate::state::{Account, Key, Park, State};
use crate::{home, lock, park, state, store};
use serde_json::{Value, json};
use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Enrolled {
    /// The account signed in now, recorded without parking: its first switch parks it.
    Current { email: String },
    /// Another account, signed in privately and parked.
    SignedIn { email: String },
    /// An enrolled account signed in to again: its parked login is now the new one.
    Renewed { email: String },
    /// The account signed in now, signed in to again: its new login is the one in use now,
    /// and nothing was parked.
    InUse { email: String },
}

/// A login a tool stored for pitboard in a private directory, not yet enrolled. Dropping it
/// deletes that directory and whatever the tool kept for it elsewhere.
pub struct SignIn {
    provider: ProviderId,
    dir: PathBuf,
    document: Value,
    ctx: Context,
    _one_at_a_time: File,
}

impl SignIn {
    /// Which tool this login is for.
    pub fn provider(&self) -> ProviderId {
        self.provider
    }
}

impl Drop for SignIn {
    fn drop(&mut self) {
        provider::of(self.provider).discard_signin(&self.ctx, &self.dir);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Takes the one-sign-in-at-a-time lock and prepares the private directory the tool will
/// sign in to. Both the inherited and the watched sign-in start here.
///
/// A sign-in waits on a person in a browser, so it takes no lock but its own: a switch
/// meanwhile goes ahead, and a second sign-in is refused rather than queued.
fn reserve_signin(ctx: &Context, which: ProviderId) -> Result<SignIn> {
    let home = home::ensure(ctx).map_err(|source| Error::HomeUnwritable {
        path: home::dir(ctx),
        source,
    })?;
    let lock_path = home.join("signin.lock");
    let one_at_a_time = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(&lock_path)
        .map_err(|source| Error::HomeUnwritable {
            path: lock_path.clone(),
            source,
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
        provider::of(tool).discard_signin(ctx, &dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    home::create_private(&dir).map_err(|source| Error::HomeUnwritable {
        path: dir.clone(),
        source,
    })?;
    Ok(SignIn {
        provider: which,
        dir,
        document: Value::Null,
        ctx: ctx.clone(),
        _one_at_a_time: one_at_a_time,
    })
}

/// A sign-in that never ran. The crash matrix needs the state a finished sign-in leaves,
/// and running a tool's own login inside a test is neither possible nor wanted.
#[cfg(test)]
pub(super) fn planted(ctx: &Context, which: ProviderId, document: Value) -> Result<SignIn> {
    let mut pending = reserve_signin(ctx, which)?;
    pending.document = document;
    Ok(pending)
}

/// Run the tool's own sign-in in a private directory, where the live login is never
/// touched, and read back the login it stored there.
pub fn sign_in(ctx: &Context, which: ProviderId) -> Result<SignIn> {
    let mut pending = reserve_signin(ctx, which)?;
    // pitboard never sees the sign-in; it reads the login the tool stores once it is done.
    // What the tool prints goes to stderr, so `--json` output stays one JSON line.
    let finished = provider::of(which)
        .sign_in(ctx, &pending.dir)
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
    child: std::process::Child,
    said: Said,
    pending: SignIn,
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

    /// Types a line back, for a code the tool asks to be pasted when the browser cannot
    /// reach its callback.
    pub fn paste(&mut self, line: &str) -> Result<()> {
        use std::io::Write;
        let stdin = self.child.stdin.as_mut().ok_or(Error::SignInIncomplete)?;
        writeln!(stdin, "{line}").map_err(|_| Error::SignInIncomplete)?;
        stdin.flush().map_err(|_| Error::SignInIncomplete)
    }

    /// Waits for it to finish and hands back the login it stored.
    pub fn finish(mut self) -> Result<SignIn> {
        let finished = self
            .child
            .wait()
            .map_err(|_| Error::SignInIncomplete)?
            .success();
        if !finished {
            return Err(Error::SignInIncomplete);
        }
        let mut pending = self.pending;
        pending.document =
            signed_in_document(&pending.ctx.clone(), pending.provider, &pending.dir.clone())?;
        Ok(pending)
    }

    /// Stops it. What it may have written is discarded by `SignIn`'s own cleanup.
    pub fn cancel(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts the sign-in with its output piped, for a caller that will show it.
pub fn sign_in_watched(ctx: &Context, which: ProviderId) -> Result<WatchedSignIn> {
    let pending = reserve_signin(ctx, which)?;
    let command = provider::of(which).sign_in(ctx, &pending.dir);
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
        child,
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
        None => record_current(&ctx, key, &mut state).map(|e| (e, Vec::new())),
    }
}

/// A label names one account of its tool for good: its own, or one not enrolled under
/// another label.
fn claim(state: &State, key: &Key, owner: &Owner) -> Result<()> {
    if let Some(taken) = state.get(key)
        && taken.account_uuid != owner.account_uuid
    {
        return Err(Error::LabelTaken {
            label: key.typed(),
            email: taken.email.clone(),
        });
    }
    if let Some(existing) = state.by_uuid(key.provider, &owner.account_uuid)
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

fn record_current(ctx: &Context, key: &Key, state: &mut State) -> Result<Enrolled> {
    let which = key.provider;
    let label = key.label.as_str();
    // Through the store itself rather than the provider's reading of it, so a locked
    // keychain says so in the store's own words instead of reading as a strange login.
    let store = super::live_store(ctx, which)?;
    let live =
        store::read(&store.chain, &store.service)?.ok_or_else(|| nothing_signed_in(ctx, which))?;
    // A document with no account in it is nobody signed in: Claude Code's after a
    // `/logout` still holds the machine's MCP tokens.
    match provider::of(which).slice(&live) {
        Err(provider::ProviderError::NoLogin { .. }) => return Err(nothing_signed_in(ctx, which)),
        Err(other) => return Err(super::shape(which, other)),
        Ok(_) => {}
    }
    let owner = identify_document(ctx, which, &live)?;
    claim(state, key, &owner)?;
    let existing = state.get(key);
    let parked = existing.and_then(|a| a.parked.clone());
    // Enrolling the account that is signed in is using it.
    let last_used_at = Some(ctx.now());
    state.upsert(account(which, label, &owner, parked, last_used_at, &live));
    state.set_active(which, Some(label.to_string()));
    state::save(ctx, state)?;
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
    match signed_in_now(ctx, key.provider, &owner) {
        Some((live, first)) => install_signed_in(ctx, key, state, login, &owner, &live, &first),
        None => park_signed_in(ctx, key, state, login, &owner),
    }
}

/// Where the tool's live login is and what it held, when that is `owner`'s login.
///
/// Read the way a switch reads it. Anything short of knowing it is `owner`'s is `None`: a
/// login that cannot be read, one whose account cannot be told, another account's, or
/// nobody's. Writing over a login whose account is not known could lose that account's
/// only login, so a new one is parked beside it instead.
fn signed_in_now(
    ctx: &Context,
    which: ProviderId,
    owner: &Owner,
) -> Option<(provider::LiveStore, Value)> {
    let live = super::live_store(ctx, which).ok()?;
    let (_, first) = super::read_live(ctx, which, &live).ok()?;
    let found = identify_document(ctx, which, &first).ok()?;
    (found.account_uuid == owner.account_uuid).then_some((live, first))
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
    let which = key.provider;
    let name = state.typed(key);
    let slice = provider::of(which)
        .slice(&login.document)
        .map_err(|e| super::shape(which, e))?;
    let Readied {
        guard,
        before_raw,
        next,
        on_the_command_line,
        ..
    } = super::ready(ctx, which, live, first, &owner.account_uuid, &slice, &name)?;

    match super::install_with(
        which,
        |body| store::write_raw(&live.chain, &live.service, body),
        || store::read_raw(&live.chain, &live.service),
        &next,
        &before_raw,
        &name,
        &name,
    ) {
        Ok(()) => {}
        // The old login is where it was, and the new one goes with the sign-in.
        Err(e @ Error::SwitchRolledBack { .. }) => return Err(e),
        Err(Error::SwitchUnverified { detail, .. } | Error::SwitchCorrupted { detail, .. }) => {
            return Err(not_installed(ctx, key, state, login, owner, detail));
        }
        Err(other) => return Err(other),
    }
    crate::fault::point("enroll.installed");

    // Read back as a switch reads back: a write that landed has not necessarily held.
    let lock_lost = guard.as_ref().is_some_and(lock::Guard::compromised);
    let lost = match super::holds(which, live) {
        Ok(true) => None,
        Ok(false) => Some("it was gone again before pitboard finished".to_string()),
        Err(unreadable) => Some(unreadable.to_string()),
    };
    if let Some(detail) = lost {
        return Err(not_installed(ctx, key, state, login, owner, detail));
    }

    let parked = state.get(key).and_then(|a| a.parked.clone());
    state.upsert(account(
        which,
        &key.label,
        owner,
        parked,
        Some(ctx.now()),
        &login.document,
    ));
    state.set_active(which, Some(key.label.clone()));
    state::save(ctx, state)?;
    crate::fault::point("enroll.recorded");
    drop(guard);

    // A session of a tool that never reads its login again goes on with the old one, and
    // writes it back over the new one when it refreshes.
    let still_running = super::running_sessions(ctx, which).map(|(program, count)| {
        Warning::SessionsKeepTheOldLogin {
            program,
            count,
            label: name.clone(),
        }
    });
    let warnings = on_the_command_line
        .into_iter()
        .chain(still_running)
        .chain(lock_lost.then_some(Warning::LockCompromised { tool: which }))
        .collect();
    Ok((
        Enrolled::InUse {
            email: owner.email.clone(),
        },
        warnings,
    ))
}

/// A new login that could not be put in use, where the tool may be left without the old
/// one too. The new one is the one copy of the account's login known to be good, so it is
/// parked, as a sign-in of any other account would be, before the failure is reported.
/// Where the slot could not be read back it may hold the new login as well, and the next
/// switch away parks over this copy.
fn not_installed(
    ctx: &Context,
    key: &Key,
    state: &mut State,
    login: &SignIn,
    owner: &Owner,
    detail: String,
) -> Error {
    let parked = park_signed_in(ctx, key, state, login, owner).is_ok();
    Error::SignInNotInstalled {
        tool: key.provider,
        label: state.typed(key),
        detail,
        parked,
    }
}

fn park_signed_in(
    ctx: &Context,
    key: &Key,
    state: &mut State,
    login: &SignIn,
    owner: &Owner,
) -> Result<(Enrolled, Vec<Warning>)> {
    let label = key.label.as_str();
    let slice = provider::of(login.provider)
        .slice(&login.document)
        .map_err(|e| super::shape(login.provider, e))?;
    let parking = park::price(
        ctx,
        login.provider,
        &key.typed(),
        &park::service_name(&owner.account_uuid, ctx.now_millis()),
        &slice,
    )?;
    let service = park::reserve(ctx, &owner.account_uuid)?;
    let fresh = park::store_at(ctx, key.provider, &service, &slice)?;
    // The window the roadmap named: the login is in the vault and nothing on the machine
    // says so yet.
    crate::fault::point("enroll.park_stored");
    let existing = state.get(key);
    let previous = existing.and_then(|a| a.parked.clone());
    let renewed = existing.is_some();
    let last_used_at = existing.and_then(|a| a.last_used_at);
    state.upsert(account(
        login.provider,
        label,
        owner,
        previous,
        last_used_at,
        &login.document,
    ));
    state.park(key, fresh);
    // Unrecorded, the new login would be an item nothing refers to, never deleted.
    state::save(ctx, state).inspect_err(|_| {
        let _ = store::vault_delete(ctx, &service);
    })?;
    crate::fault::point("enroll.park_recorded");
    purge(ctx, state);
    let email = owner.email.clone();
    let enrolled = if renewed {
        Enrolled::Renewed { email }
    } else {
        Enrolled::SignedIn { email }
    };
    Ok((enrolled, parking.into_iter().collect()))
}

/// What pitboard records about a newly enrolled account.
///
/// For Claude Code, only what Anthropic just confirmed: leaving the rest out makes Claude
/// Code fetch its own profile after a switch rather than trust a copy pitboard wrote. For
/// Codex there is no such cache to correct, and what is kept instead is what its own login
/// already said, which costs nothing to read and explains a limit somebody is surprised by.
fn account(
    which: ProviderId,
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
        label: label.to_string(),
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
        Machine, NOW, codex_id, codex_login, codex_machine, hold, login_of, machine, oauth,
        signed_in,
    };
    use crate::switch::{Outcome, settle, switch};
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
        let settled = settle(&m.ctx, Some(m.which)).expect("nothing to recover").0;
        enroll(settled, &m.key(label), Some(login))
    }

    fn park_of(m: &Machine, label: &str) -> Option<Park> {
        state::load(&m.ctx)
            .expect("state")
            .get(&m.key(label))
            .and_then(|a| a.parked.clone())
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
                matches!(enrolled, Enrolled::InUse { ref email } if email == "here@example.com"),
                "{tool}: {enrolled:?}"
            );
            assert!(
                in_use(&m, "here-refresh-2"),
                "{tool}: the new login is in use"
            );
            assert_eq!(m.mem.vault().services(), vault, "{tool}: nothing is parked");
            assert!(park_of(&m, "here").is_none(), "{tool}");
            let state = state::load(&m.ctx).expect("state");
            assert_eq!(state.active_for(m.which), Some("here"), "{tool}");
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
            let (uuid, older) = match m.which {
                ProviderId::Claude => ("here".to_string(), oauth("here-older", 30)),
                ProviderId::Codex => (codex_id("here"), codex_login("here", "here-older")),
            };
            let service = park::reserve(&m.ctx, &uuid).expect("a free name");
            let held = park::store_at(&m.ctx, m.which, &service, &older).expect("parked");
            let mut state = state::load(&m.ctx).expect("state");
            state.park(&m.key("here"), held.clone());
            state::save(&m.ctx, &state).expect("saved");
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

            let settled = settle(&m.ctx, Some(m.which)).expect("nothing to recover").0;
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
    /// beside it, as it always was. For Claude Code that is Anthropic refusing or not
    /// answering about the login in use; for Codex, a login whose ID token cannot be read.
    #[test]
    fn a_login_in_use_whose_account_cannot_be_told_is_not_written_over() {
        let mut cases: Vec<(String, Machine)> = Vec::new();
        for (name, trouble) in [
            ("refused", Trouble::Unauthorized),
            ("offline", Trouble::Offline),
        ] {
            let m = machine(&format!("untold-{name}"));
            m.api.token_trouble("access-here-refresh", trouble);
            cases.push((format!("claude, {name}"), m));
        }
        let m = codex_machine("untold");
        let mut unreadable = codex_login("here", "here-refresh");
        unreadable["tokens"]["id_token"] = "not a token".into();
        m.sign_in(&unreadable);
        cases.push(("codex".into(), m));

        for (case, m) in cases {
            let live = m.live();

            let (enrolled, _) = enrolled_as(&m, "here", signed_in(&m, "here", "here-refresh-2"))
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
        }
    }

    /// Somebody signs the tool in to another account between the first read and the one
    /// made under the tool's lock. Nothing is written over the account now signed in.
    #[test]
    fn the_account_in_use_changing_before_the_write_is_refused_and_nothing_is_written() {
        for (tool, make) in MACHINES {
            let m = make("changed-before-write");
            let key = m.key("here");
            let login = signed_in(&m, "here", "here-refresh-2");
            let owner = identify_document(&m.ctx, m.which, &login.document).expect("whose");
            let (live, first) =
                signed_in_now(&m.ctx, m.which, &owner).expect("`here` is signed in");

            m.sign_in(&login_of(&m, "other", "other-refresh"));
            let vault = m.mem.vault().services();
            let recorded = serde_json::to_value(state::load(&m.ctx).expect("state")).unwrap();
            let mut state = state::load(&m.ctx).expect("state");
            let refused =
                install_signed_in(&m.ctx, &key, &mut state, &login, &owner, &live, &first)
                    .expect_err("refused");

            assert_eq!(refused.code(), "signed_in_account_changed", "{tool}");
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
    /// goes with the sign-in: nothing was lost, and signing in again is the way on.
    #[test]
    fn a_new_login_that_cannot_be_written_leaves_the_old_one_in_use() {
        for (tool, make) in MACHINES {
            let m = make("again-write-fails");
            let vault = m.mem.vault().services();
            let login = signed_in(&m, "here", "here-refresh-2");
            m.fault_live(Fault::FailWrite("refused".into()));

            let failed = enrolled_as(&m, "here", login).expect_err("the write failed");

            assert_eq!(failed.code(), "switch_rolled_back", "{tool}: {failed}");
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
                failed.to_string().contains("parked instead"),
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

    /// Codex prints its address and then nothing until the browser is done, so somebody
    /// pressing Cancel nearly always finds a reader waiting. The cancel must not wait with
    /// it, and stopping the tool must end the reading rather than leave it waiting forever.
    #[test]
    fn a_cancel_does_not_wait_on_a_tool_that_says_nothing() {
        let m = codex_machine("cancel");
        let pending = reserve_signin(&m.ctx, ProviderId::Codex).expect("reserved");
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
        reserve_signin(&m.ctx, ProviderId::Codex)
            .expect("a cancelled sign-in lets the next one start");
    }
}
