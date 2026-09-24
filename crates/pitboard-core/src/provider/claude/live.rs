//! Where Claude Code's live credential is, and how a private sign-in's is read back.
//!
//! This is the half of the old `Platform` trait that was never about the operating system.
//! `MacOs::read_signin` called `slot::service_for_dir` from inside the trait impl, so a
//! trait whose name said "which machine" decided which keychain item Claude Code reads. The
//! machine still answers "is there a keychain here"; which item, and which file behind it,
//! is answered here.

use super::{paths as claude, slot};
use crate::context::Context;
use crate::store::{Error, Live, RawStore};
use std::path::{Path, PathBuf};

/// The plaintext file Claude Code demotes to, and reads on a machine with no keychain.
pub(crate) fn credential_file(ctx: &Context) -> PathBuf {
    PathBuf::from(claude::storage_dir(ctx)).join(slot::CRED_FILE)
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
pub(crate) fn chain(ctx: &Context) -> Live {
    let host = ctx.host();
    let mut backends: Vec<Box<dyn RawStore>> = Vec::new();
    if let Some(keychain) = host.foreign_keychain(ctx, &slot::account_name(ctx)) {
        backends.push(keychain);
    }
    backends.push(host.file(credential_file(ctx)));
    Live::of(backends)
}

/// The credential Claude Code left for a config directory during a private sign-in: the
/// keychain item its own hashing names, or the file inside that directory where there is
/// no keychain.
pub(crate) fn read_signin(ctx: &Context, dir: &Path) -> Result<Option<String>, Error> {
    match ctx.host().foreign_keychain(ctx, &slot::account_name(ctx)) {
        Some(keychain) => keychain.read(&slot::service_for_dir(&dir.to_string_lossy())),
        None => ctx.host().file(dir.join(slot::CRED_FILE)).read(""),
    }
}

/// Deletes an item Claude Code created for a private sign-in.
///
/// It refuses any name that could be a real login: the default slot, and whatever this
/// context's own `CLAUDE_CONFIG_DIR` hashes to. A scratch directory's hash is safe by
/// construction, and those two are the only names in the family that are not.
pub(crate) fn discard_signin(ctx: &Context, dir: &Path) -> Result<(), Error> {
    let service = slot::service_for_dir(&dir.to_string_lossy());
    if service == slot::LIVE_SERVICE || service == claude::live_service(ctx) {
        return Err(Error::Write(format!("refusing to delete {service}")));
    }
    match ctx.host().foreign_keychain(ctx, &slot::account_name(ctx)) {
        Some(keychain) => keychain.delete(&service),
        None => ctx.host().file(dir.join(slot::CRED_FILE)).delete(""),
    }
}
