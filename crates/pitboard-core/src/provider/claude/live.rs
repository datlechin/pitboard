//! Where Claude Code's live credential is, and how a private sign-in's is read back.
//!
//! This is the half of the old `Platform` trait that was never about the operating system.
//! `MacOs::read_signin` called `slot::service_for_dir` from inside the trait impl, so a
//! trait whose name said "which machine" decided which keychain item Claude Code reads. The
//! machine still answers "is there a keychain here"; which item, and which file behind it,
//! is answered here.

use super::{paths as claude, slot};
use crate::context::Context;
use crate::host::{OS, Os};
use crate::service::Permit;
use crate::store::{self, Backend, Error, Live, RawStore, Unbuilt};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The plaintext file Claude Code demotes to, and reads on a machine with no keychain.
pub(crate) fn credential_file(ctx: &Context) -> PathBuf {
    PathBuf::from(claude::storage_dir(ctx)).join(slot::CRED_FILE)
}

/// Claude Code's store on Windows, which Pitboard does not read or write yet. Its storage
/// backends are `keychain`, `plaintext` and `windows-credman`, the last behind the
/// `tengu_windows_credman` flag or `CLAUDE_CODE_FORCE_WINDOWS_CREDMAN` (the register's
/// `no_keyring_off_macos`), and which one it uses on Windows is the register's pending
/// `windows_backend_choice`. So the file alone is never the chain: it could read a login that
/// is not the one in use, and write one nothing reads. W22 reads the file where Claude Code
/// reads it and refuses wherever Credential Manager may hold the login, and W23 follows
/// Claude Code there.
fn not_on_windows_yet() -> Unbuilt {
    Unbuilt::new(
        Backend::Unknown,
        "Pitboard does not switch Claude Code on Windows yet",
    )
}

/// The backends that may hold Claude Code's login, in the order it looks.
///
/// Settled in 2.1.278: Claude Code builds its live chain as keychain with a plaintext
/// fallback, and the successor backend (`tengu_hover_rest`) replaces what backs the
/// fallback half and only for a caller that hands a backend in. An ordinary `claude` hands
/// none in, so the keychain stays first.
///
/// Resolved on every call, never cached: Claude Code moves the credential between backends
/// when a keychain write fails for good, so a remembered answer goes wrong without warning.
/// From 2.1.281 a locked keychain whose item the process has seen does not move it.
///
/// On Windows it is one store that refuses every call ([`not_on_windows_yet`]).
pub(crate) fn chain(ctx: &Context) -> Live {
    match OS {
        Os::MacOs | Os::Linux => {
            let host = ctx.host();
            let mut backends: Vec<Box<dyn RawStore>> = Vec::new();
            if let Some(keychain) = host.foreign_secrets(ctx, &slot::account_name(ctx)) {
                backends.push(keychain);
            }
            backends.push(host.file(credential_file(ctx)));
            Live::of(backends)
        }
        Os::Windows => Live::of(vec![Box::new(not_on_windows_yet())]),
    }
}

/// What reading `.credentials.json` gives while the keychain holds the login in use: what it
/// holds, whole, or why it cannot be read where it is there. `None` where it is not there or
/// is the store in use. Claude Code deletes the file after a keychain write only where the
/// keychain held nothing before (the register's `fallback_outlives_keychain_writes`), so
/// what a sign-in left there while the keychain was locked stays through every switch. A
/// session that cannot read the keychain, as one started over SSH, signs in with a login in
/// it, and one already running keeps its own while the file is there, whatever it holds and
/// whether or not it can be read: it looks at the file, and never reads it to decide
/// (`fallback_file_pins_session_login`).
///
/// `None` too where whether it is there cannot be told: the keychain cannot be read, which
/// reading the login in use says as well, or the file cannot even be looked at, which
/// Claude Code's own look cannot either, and takes for a file that is not there.
pub(crate) fn behind(ctx: &Context) -> Option<Result<String, Error>> {
    let (live, service) = (chain(ctx), claude::live_service(ctx));
    let file = store::behind(&live, &service).ok()??;
    // Gone between the look and the read: not there.
    file.read(&service).transpose()
}

/// What `.credentials.json` holds, as a JSON object: an empty one where it holds none, which
/// holds nothing of anybody's.
pub(crate) fn document_in(held: &str) -> Value {
    serde_json::from_str::<Value>(held)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
}

/// Whether `document`, what `.credentials.json` holds, holds a login: a `claudeAiOauth` with
/// a token in it, to sign in with or to renew with. A file that is not a JSON object, or
/// holds no such token, signs nobody in, and nothing can tell whose it is. What every
/// warning, doctor and `pitboard stow` tell it by.
pub(crate) fn holds_a_login(document: &Value) -> bool {
    let oauth = document.get("claudeAiOauth");
    ["accessToken", "refreshToken"].into_iter().any(|token| {
        oauth
            .and_then(|oauth| oauth.get(token))
            .and_then(Value::as_str)
            .is_some_and(|token| !token.is_empty())
    })
}

/// The login in what `.credentials.json` holds, whole, where it holds one.
pub(crate) fn login_in(held: &str) -> Option<Value> {
    Some(document_in(held)).filter(holds_a_login)
}

/// The credential Claude Code left for a config directory during a private sign-in: the
/// keychain item its own hashing names, or the file inside that directory where there is
/// no keychain. Refused on Windows, as the chain is.
pub(crate) fn read_signin(ctx: &Context, dir: &Path) -> Result<Option<String>, Error> {
    match OS {
        Os::MacOs | Os::Linux => {}
        Os::Windows => return not_on_windows_yet().read(""),
    }
    match ctx.host().foreign_secrets(ctx, &slot::account_name(ctx)) {
        Some(keychain) => keychain.read(&slot::service_for_dir(&dir.to_string_lossy())),
        None => ctx.host().file(dir.join(slot::CRED_FILE)).read(""),
    }
}

/// Deletes an item Claude Code created for a private sign-in.
///
/// It refuses any name that could be a real login: the default slot, and whatever this
/// context's own `CLAUDE_CONFIG_DIR` hashes to. A scratch directory's hash is safe by
/// construction, and those two are the only names in the family that are not. Refused on
/// Windows, as the chain is.
pub(crate) fn discard_signin(ctx: &Context, permit: Permit, dir: &Path) -> Result<(), Error> {
    match OS {
        Os::MacOs | Os::Linux => {}
        Os::Windows => return not_on_windows_yet().delete(permit, ""),
    }
    let service = slot::service_for_dir(&dir.to_string_lossy());
    if service == slot::LIVE_SERVICE || service == claude::live_service(ctx) {
        return Err(Error::Write(format!("refusing to delete {service}")));
    }
    match ctx.host().foreign_secrets(ctx, &slot::account_name(ctx)) {
        Some(keychain) => keychain.delete(permit, &service),
        None => ctx
            .host()
            .file(dir.join(slot::CRED_FILE))
            .delete(permit, ""),
    }
}
