//! Bringing an account under pitboard's care.
//!
//! A parked copy is only safe if the live slot is replaced the moment it is taken; otherwise
//! Claude Code keeps rotating the same token and the copy goes stale. So the account signed
//! in now is recorded but not parked. Its first switch parks it at exactly that moment,
//! and any other account is signed in inside a private directory, where the live slot is
//! never touched and the vault is the new login's only holder.

use super::{Error, Result, Settled, identify_document, purge, slice_of};
use crate::api::Owner;
use crate::context::Context;
use crate::provider::ProviderId;
use crate::provider::claude::live as claude_live;
use crate::provider::claude::paths as claude;
use crate::state::{Account, Park, State};
use crate::{home, park, state, store};
use serde_json::{Value, json};
use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug)]
pub enum Enrolled {
    /// The account signed in now, recorded without parking: its first switch parks it.
    Current { email: String },
    /// Another account, signed in privately and parked.
    SignedIn { email: String },
    /// An enrolled account signed in to again: its parked login is now the new one.
    Renewed { email: String },
}

/// A login Claude Code stored for pitboard in a private directory, not yet enrolled. Dropping
/// it deletes that directory and the credential Claude Code kept for it.
pub struct SignIn {
    dir: PathBuf,
    document: Value,
    ctx: Context,
    _one_at_a_time: File,
}

impl Drop for SignIn {
    fn drop(&mut self) {
        let _ = claude_live::discard_signin(&self.ctx, &self.dir);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Run Claude Code's own sign-in in a private directory, where the live login is never
/// touched. It waits on a person in a browser, so it takes no lock but its own: a switch
/// meanwhile goes ahead, and a second sign-in is refused rather than queued.
/// Takes the one-sign-in-at-a-time lock and prepares the private directory Claude Code will
/// sign in to. Both the inherited and the watched sign-in start here.
fn reserve_signin(ctx: &Context) -> Result<SignIn> {
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
    // lock above means no other sign-in is using it.
    let _ = claude_live::discard_signin(ctx, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    home::create_private(&dir).map_err(|source| Error::HomeUnwritable {
        path: dir.clone(),
        source,
    })?;
    Ok(SignIn {
        dir,
        document: Value::Null,
        ctx: ctx.clone(),
        _one_at_a_time: one_at_a_time,
    })
}

/// A sign-in that never ran. The crash matrix needs the state a finished sign-in leaves,
/// and running Claude Code's own login inside a test is neither possible nor wanted.
#[cfg(test)]
pub(super) fn planted(ctx: &Context, document: Value) -> Result<SignIn> {
    let mut pending = reserve_signin(ctx)?;
    pending.document = document;
    Ok(pending)
}

pub fn sign_in(ctx: &Context) -> Result<SignIn> {
    let mut pending = reserve_signin(ctx)?;
    // pitboard never sees the sign-in; it reads the login Claude Code stores once it is done.
    // What Claude Code prints goes to stderr, so `--json` output stays one JSON line.
    let finished = login(ctx, &pending.dir)
        .stdout(std::io::stderr())
        .status()
        .map_err(started)?
        .success();
    if !finished {
        return Err(Error::SignInIncomplete);
    }
    pending.document = signed_in_document(ctx, &pending.dir)?;
    Ok(pending)
}

/// Claude Code's own sign-in, pointed at a private directory so the live login is never
/// touched. Measured in 2.1.278: it opens the browser itself and finishes through a
/// loopback callback, printing progress with `stdout.write` and reading stdin only as the
/// fallback for a pasted code. So it needs no terminal: pipes are enough.
fn login(ctx: &Context, dir: &std::path::Path) -> Command {
    let mut command = Command::new(&ctx.claude_program);
    command
        .args(["auth", "login"])
        .env("CLAUDE_CONFIG_DIR", dir)
        .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR");
    command
}

fn started(e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::NotFound => Error::ClaudeNotFound,
        _ => Error::SignInIncomplete,
    }
}

fn signed_in_document(ctx: &Context, dir: &std::path::Path) -> Result<Value> {
    let raw = claude_live::read_signin(ctx, dir)?.ok_or(Error::SignInIncomplete)?;
    serde_json::from_str(&raw).map_err(|e| Error::LiveCredentialShapeUnexpected {
        detail: e.to_string(),
    })
}

/// The same sign-in, watched rather than inherited: an app has no terminal to hand over, so
/// it reads what Claude Code prints and can type the fallback code back.
pub struct WatchedSignIn {
    child: std::process::Child,
    said: std::sync::mpsc::Receiver<String>,
    pending: SignIn,
}

impl WatchedSignIn {
    /// The next thing Claude Code said, or `None` once it has finished saying anything.
    /// Blocks, so a caller reads it on a thread of its own.
    pub fn next_line(&self) -> Option<String> {
        self.said.recv().ok()
    }

    /// Types a line back, for the code Claude Code asks to be pasted when the browser
    /// cannot reach its callback.
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
        pending.document = signed_in_document(&pending.ctx.clone(), &pending.dir.clone())?;
        Ok(pending)
    }

    /// Stops it. What it may have written is discarded by `SignIn`'s own cleanup.
    pub fn cancel(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts the sign-in with its output piped, for a caller that will show it.
pub fn sign_in_watched(ctx: &Context) -> Result<WatchedSignIn> {
    let pending = reserve_signin(ctx)?;
    let mut child = login(ctx, &pending.dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(started)?;
    let (say, said) = std::sync::mpsc::channel();
    // Claude Code writes the browser URL and the paste prompt without a newline after them,
    // so this reads by chunk rather than by line and lets the caller decide what to show.
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
        said,
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
    which: ProviderId,
    label: &str,
    signed_in: Option<SignIn>,
) -> Result<Enrolled> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    match signed_in {
        // A watched sign-in is Claude Code's own, and is the only one pitboard drives.
        Some(login) => park_signed_in(&ctx, label, &mut state, &login),
        None => record_current(&ctx, which, label, &mut state),
    }
}

/// A label names one account for good: its own, or one not enrolled under another label.
fn claim(state: &State, label: &str, owner: &Owner) -> Result<()> {
    if let Some(taken) = state.get(label)
        && taken.account_uuid != owner.account_uuid
    {
        return Err(Error::LabelTaken {
            label: label.to_string(),
            email: taken.email.clone(),
        });
    }
    if let Some(existing) = state.by_uuid(&owner.account_uuid)
        && existing.label != label
    {
        return Err(Error::AlreadyEnrolled {
            email: owner.email.clone(),
            label: existing.label.clone(),
        });
    }
    Ok(())
}

fn record_current(
    ctx: &Context,
    which: ProviderId,
    label: &str,
    state: &mut State,
) -> Result<Enrolled> {
    let live = crate::provider::of(which)
        .read_live(ctx)
        .map_err(|e| Error::LiveCredentialShapeUnexpected {
            detail: e.to_string(),
        })?
        .ok_or_else(|| nothing_signed_in(ctx, which))?
        .raw;
    let owner = identify_document(ctx, which, &live)?;
    claim(state, label, &owner)?;
    let existing = state.get(label);
    let parked = existing.and_then(|a| a.parked.clone());
    // Enrolling the account that is signed in is using it.
    let last_used_at = Some(ctx.now());
    state.upsert(account(which, label, &owner, parked, last_used_at, &live));
    state.set_active(which, Some(label.to_string()));
    state::save(ctx, state)?;
    Ok(Enrolled::Current { email: owner.email })
}

/// Nothing is signed in to this tool, said in that tool's own words.
fn nothing_signed_in(ctx: &Context, which: ProviderId) -> Error {
    match which {
        ProviderId::Claude => claude::nothing_signed_in(ctx),
        ProviderId::Codex => Error::LiveCredentialAbsent,
    }
}

fn park_signed_in(
    ctx: &Context,
    label: &str,
    state: &mut State,
    login: &SignIn,
) -> Result<Enrolled> {
    let owner = identify_document(ctx, ProviderId::Claude, &login.document)?;
    claim(state, label, &owner)?;
    let service = park::reserve(ctx, &owner.account_uuid)?;
    let fresh = park::store_at(ctx, &service, &slice_of(&login.document)?)?;
    // The window the roadmap named: the login is in the vault and nothing on the machine
    // says so yet.
    crate::fault::point("enroll.park_stored");
    let existing = state.get(label);
    let previous = existing.and_then(|a| a.parked.clone());
    let renewed = existing.is_some();
    let last_used_at = existing.and_then(|a| a.last_used_at);
    state.upsert(account(
        ProviderId::Claude,
        label,
        &owner,
        previous,
        last_used_at,
        &login.document,
    ));
    state.park(label, fresh);
    // Unrecorded, the new login would be an item nothing refers to, never deleted.
    state::save(ctx, state).inspect_err(|_| {
        let _ = store::vault_delete(ctx, &service);
    })?;
    crate::fault::point("enroll.park_recorded");
    purge(ctx, state);
    Ok(if renewed {
        Enrolled::Renewed { email: owner.email }
    } else {
        Enrolled::SignedIn { email: owner.email }
    })
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
