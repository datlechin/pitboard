//! Where Claude Code's live credential is, and how a private sign-in's is read back. The
//! machine answers whether there is a keychain; which item, and which file behind it, is
//! answered here.

use super::{credman, paths as claude, slot};
use crate::context::Context;
use crate::host::{OS, Os};
use crate::permit::Permit;
use crate::store::{self, Backend, Error, Live, RawStore, Refused};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The plaintext file Claude Code demotes to, and reads on a machine with no keychain.
pub(crate) fn credential_file(ctx: &Context) -> PathBuf {
    PathBuf::from(claude::storage_dir(ctx)).join(slot::CRED_FILE)
}

// Claude Code's chain in the order it reads, resolved on every call: it moves the login between
// backends when a keychain write fails for good (`live_chain_order`). On Windows its store is
// Credential Manager with the file behind it where anything turns that on, and every write goes
// there (`credman_migration`); else the file alone, unless Credential Manager holds a login,
// where every call refuses (`windows_backend_choice`).
pub(crate) fn chain(ctx: &Context) -> Live {
    let host = ctx.host();
    match OS {
        Os::MacOs | Os::Linux => {
            let mut backends: Vec<Box<dyn RawStore>> = Vec::new();
            if let Some(keychain) = host.foreign_secrets(ctx, &slot::account_name(ctx)) {
                backends.push(keychain);
            }
            backends.push(host.file(credential_file(ctx)));
            Live::of(backends)
        }
        Os::Windows if credman::turned_on(ctx).is_some() => Live::taken_over_by_the_first(vec![
            Box::new(credman::Store::of(ctx)),
            host.file(credential_file(ctx)),
        ]),
        Os::Windows => match credman::ambiguity(ctx) {
            None => Live::of(vec![host.file(credential_file(ctx))]),
            Some(refused) => Live::of(vec![Box::new(Refused::new(Backend::Unknown, refused))]),
        },
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

// The login in the file [`behind`] reads, where it holds one, as the provider and doctor read it.
pub(crate) fn login_behind(ctx: &Context) -> Option<Result<Option<Value>, Error>> {
    Some(behind(ctx)?.map(|held| login_in(&held)))
}

// What a private sign-in left: the keychain item its own hashing names, or the file inside the
// folder where there is no keychain. On Windows the item its hashing names in Credential Manager,
// where something turned that store on for it, then the file.
pub(crate) fn login_from_sign_in(ctx: &Context, dir: &Path) -> Result<Option<String>, Error> {
    let service = slot::service_for_dir(&dir.to_string_lossy());
    let file = ctx.host().file(dir.join(slot::CRED_FILE));
    match OS {
        Os::MacOs | Os::Linux => match ctx.host().foreign_secrets(ctx, &slot::account_name(ctx)) {
            Some(keychain) => keychain.read(&service),
            None => file.read(""),
        },
        Os::Windows => {
            let left = Live::of(vec![Box::new(credman::Store::of(ctx)), file]);
            store::read_raw(&left, &service)
        }
    }
}

// Never a name that could be a real login: the default slot, or the one this context's own
// `CLAUDE_CONFIG_DIR` hashes to.
pub(crate) fn discard_signin(ctx: &Context, permit: Permit, dir: &Path) -> Result<(), Error> {
    let service = slot::service_for_dir(&dir.to_string_lossy());
    if service == slot::LIVE_SERVICE || service == claude::live_service(ctx) {
        return Err(Error::Write(format!("refusing to delete {service}")));
    }
    let file = ctx.host().file(dir.join(slot::CRED_FILE));
    match OS {
        Os::MacOs | Os::Linux => match ctx.host().foreign_secrets(ctx, &slot::account_name(ctx)) {
            Some(keychain) => keychain.delete(permit, &service),
            None => file.delete(permit, ""),
        },
        Os::Windows => {
            credman::Store::of(ctx).delete(permit, &service)?;
            file.delete(permit, "")
        }
    }
}
