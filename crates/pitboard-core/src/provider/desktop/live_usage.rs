//! Live usage: Claude Desktop's usage asked of claude.ai with the app's own session, which
//! takes Claude's key out of the keychain. Off until somebody turns it on.
//!
//! The key opens every session on the machine, and reading it can put macOS's question in
//! front of somebody. So only [`enable`], which somebody asked for, ever reads it in a way
//! that may ask; every other read gives up after ten seconds. A refusal, a question nobody
//! answered, an item that is gone or an answer `security` is not known to give is a reason
//! to wait to be allowed again rather than ask on every refresh; a session that cannot show
//! the question, or an item whose attributes cannot be read for a moment, says nothing
//! about approval and changes none. The key is
//! read at most once per process for each version of the keychain item, and a version
//! Pitboard was not allowed is never read.
//!
//! A read that gives up leaves macOS's question on screen (experiment U-K3), so waiting to
//! be allowed after one is what keeps a refresh in the same process from stacking a second
//! question on the first; another process, or another `enable`, can still ask. After Always
//! Allow a background shell read the key without asking (U-K1b), so refreshes need nobody
//! there.

// Turned on and off by the service and asked by status, which this file is the boundary for.
#![allow(dead_code)]

use super::cookies;
use super::crypto::{self, CryptoError};
use super::paths::{PARK_PREFIX, cookies_db, desktop_home, parks_dir, support_dir};
use super::safe_storage::{ItemStamp, KeyRead, KeyReadError};
pub(crate) use super::types::{Approval, LiveUsage};
use crate::context::Context;
use crate::error::{Cause, Error};
use crate::state::{Account, Detail};
use crate::status::Stale;
use crate::usage::Snapshot;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use zeroize::Zeroizing;

/// The key, once read, with the stamp of the item it was read from.
type Kept = (ItemStamp, Zeroizing<[u8; 16]>);

/// The keys this process has read, by Pitboard home. A process serves one home, so this
/// is one key; the tests' homes are many, and each keeps its own.
static CACHE: Mutex<BTreeMap<PathBuf, Kept>> = Mutex::new(BTreeMap::new());

fn cache() -> MutexGuard<'static, BTreeMap<PathBuf, Kept>> {
    // A panic while it was held cannot have left half a key: every write is one insert.
    CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The key read for the item as it is stamped now, if this process read it.
fn cached(ctx: &Context) -> Option<Kept> {
    cache().get(&crate::home::dir(ctx)).cloned()
}

fn keep(ctx: &Context, stamp: ItemStamp, key: Zeroizing<[u8; 16]>) {
    cache().insert(crate::home::dir(ctx), (stamp, key));
}

/// Held across everything that reads or writes live usage's state, and across a read of
/// the key. A refresh asks for every row at once, each on its own thread: the first row
/// to need the key reads it while the rest wait, then use the key it kept, so macOS is
/// asked once; and no row saves the state over what another has just recorded.
static GATE: Mutex<()> = Mutex::new(());

/// The gate, taken before the cache whenever both are.
fn gate() -> MutexGuard<'static, ()> {
    // It guards no data of its own: a panic while it was held left nothing half done.
    GATE.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The state each home's live usage was in when this process found it must wait to be
/// allowed again and could not write that down. While the saved state is still that one,
/// the wait holds for the rest of the process, so the password is not read on every
/// refresh; a state written since, by turning live usage on or off here or anywhere,
/// ends it.
static UNSAVED: Mutex<BTreeMap<PathBuf, LiveUsage>> = Mutex::new(BTreeMap::new());

fn unsaved() -> MutexGuard<'static, BTreeMap<PathBuf, LiveUsage>> {
    // Every write is one insert or one removal: a panic cannot have left half of one.
    UNSAVED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether this process found live usage must wait to be allowed again while `state` was
/// the one saved, and could not save that.
fn waits_unsaved(ctx: &Context, state: &LiveUsage) -> bool {
    let mut unsaved = unsaved();
    let home = crate::home::dir(ctx);
    match unsaved.get(&home) {
        Some(found) if found == state => true,
        Some(_) => {
            unsaved.remove(&home);
            false
        }
        None => false,
    }
}

/// Forgets the key this process read, so the next use reads it again or not at all.
pub(crate) fn forget_key(ctx: &Context) {
    cache().remove(&crate::home::dir(ctx));
}

/// Whether live usage is on and allowed: `~/.pitboard/desktop/live-usage.json`.
pub(crate) fn state_file(ctx: &Context) -> PathBuf {
    desktop_home(ctx).join("live-usage.json")
}

/// The state as last saved. Off, and never asked about, where there is none or it cannot
/// be read, so a damaged file never turns live usage on.
pub(crate) fn load(ctx: &Context) -> LiveUsage {
    std::fs::read(state_file(ctx))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

/// Saves the state, readable by its owner alone.
pub(crate) fn save(ctx: &Context, state: &LiveUsage) -> Result<(), Error> {
    let path = state_file(ctx);
    let failed = |source| Error::StateWriteFailed {
        path: path.clone(),
        source,
    };
    crate::host::fs::create_private_dir(&desktop_home(ctx)).map_err(failed)?;
    let body = serde_json::to_vec_pretty(state).map_err(|e| failed(std::io::Error::other(e)))?;
    crate::atomic::write(&path, &body, crate::atomic::Perms::Secret).map_err(failed)
}

/// Records that live usage waits to be allowed again, for `reason`, and forgets the key.
/// Returns what a reading says in the meantime. Called with the gate held, so the state
/// it changes is the one saved last.
fn needs_approval(ctx: &Context, _held: &MutexGuard<'_, ()>, reason: &str) -> Stale {
    forget_key(ctx);
    let saved = load(ctx);
    if saved.approval != Approval::NeedsApproval || saved.reason.as_deref() != Some(reason) {
        let mut state = saved.clone();
        state.approval = Approval::NeedsApproval;
        state.reason = Some(reason.to_string());
        state.changed_at = Some(ctx.now());
        // Not saved, the wait is kept in memory instead, or the next refresh would read
        // the password again.
        if save(ctx, &state).is_err() {
            unsaved().insert(crate::home::dir(ctx), saved);
        }
    }
    Stale::LiveUsageNeedsApproval
}

/// Turns live usage on: reads Claude's key the one way that may ask macOS's question, and
/// proves it opens the session the app is signed in with before taking it as allowed.
///
/// Only somebody's own request reaches this: a command run at a terminal, or the app's
/// button. Nothing in the background ever does.
pub(crate) fn enable(ctx: &Context) -> Result<LiveUsage, Error> {
    let refused = |reason: &str| Error::LiveUsageNotAllowed {
        reason: reason.to_string(),
        detail: None,
    };
    // What the keychain said, where its code alone would say only that something failed.
    let refused_by = |e: &KeyReadError| Error::LiveUsageNotAllowed {
        reason: e.reason().to_string(),
        detail: match e {
            KeyReadError::Other(detail) => Some(detail.clone()),
            _ => None,
        },
    };
    // Read first so a missing item is refused before macOS is made to ask about it.
    ctx.safe_storage().stamp(ctx).map_err(|e| refused_by(&e))?;
    let password = match ctx.safe_storage().password(ctx, KeyRead::Approve) {
        Ok(password) => password,
        // Nothing was asked, so nothing was refused: approval stays as it was.
        Err(e @ (KeyReadError::NoGui | KeyReadError::Other(_))) => {
            return Err(refused_by(&e));
        }
        Err(other) => {
            needs_approval(ctx, &gate(), other.reason());
            return Err(refused(other.reason()));
        }
    };
    let key = crypto::derive_key(&password);
    match proven(ctx, &key) {
        Ok(()) => {}
        Err(Unproven::WrongKey) => {
            needs_approval(ctx, &gate(), "key_does_not_decrypt");
            return Err(refused("key_does_not_decrypt"));
        }
        // Nothing was refused, and nothing is known about the key either.
        Err(Unproven::NoSession) => return Err(refused("no_session")),
    }
    // Read again after the answer: Always Allow rewrites the item's access list, and the
    // stamp kept has to be the one the next refresh will see.
    let stamp = ctx.safe_storage().stamp(ctx).map_err(|e| refused_by(&e))?;
    // Taken only now: holding it while macOS may be asking, which can take minutes, would
    // hold back every refresh in the meantime.
    let _held = gate();
    let mut state = load(ctx);
    state.enabled = true;
    state.approval = Approval::Granted;
    state.reason = None;
    state.changed_at = Some(ctx.now());
    state.stamp = Some(stamp.clone());
    save(ctx, &state)?;
    unsaved().remove(&crate::home::dir(ctx));
    keep(ctx, stamp, key);
    Ok(state)
}

/// Why a key read to turn live usage on was not shown to open a session.
enum Unproven {
    /// It does not open the live folder's session, or with nobody signed in there, any
    /// park's.
    WrongKey,
    /// There is no session anywhere to try it on.
    NoSession,
}

/// Whether `key` opens the session of the live folder, or where nobody is signed in there,
/// of a park, newest first. The live jar is this Mac's own, so a key that does not open it
/// is not this Mac's key; a park may have been encrypted elsewhere, so one it does not
/// open only fails to prove it, and an older park may still do so.
fn proven(ctx: &Context, key: &[u8; 16]) -> Result<(), Unproven> {
    let parks = std::fs::read_dir(parks_dir(ctx))
        .map(|entries| {
            let mut parks: Vec<(std::time::SystemTime, PathBuf)> = entries
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with(PARK_PREFIX))
                .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
                .collect();
            parks.sort();
            parks.into_iter().rev().map(|(_, path)| path).collect()
        })
        .unwrap_or_else(|_| Vec::new());
    let live = support_dir(ctx).map(|root| (true, root));
    let parks = parks.into_iter().map(|root| (false, root));
    let mut wrong = false;
    for (is_live, root) in live.into_iter().chain(parks) {
        let Ok(table) = ctx.host().cookie_table(&cookies_db(&root)) else {
            continue;
        };
        match open(key, &table, "sessionKey") {
            Ok(Some(_)) => return Ok(()),
            Ok(None) | Err(Opened::Unreadable) => continue,
            Err(Opened::WrongKey) if is_live => return Err(Unproven::WrongKey),
            Err(Opened::WrongKey) => wrong = true,
        }
    }
    Err(if wrong {
        Unproven::WrongKey
    } else {
        Unproven::NoSession
    })
}

/// Turns live usage off. Approval is kept, so turning it on again with the same key asks
/// nothing new; the key is forgotten at once.
pub(crate) fn disable(ctx: &Context) -> Result<LiveUsage, Error> {
    let _held = gate();
    forget_key(ctx);
    let mut state = load(ctx);
    if state.enabled {
        state.enabled = false;
        state.changed_at = Some(ctx.now());
    }
    save(ctx, &state)?;
    Ok(state)
}

/// Claude's key, for a reading: from memory, or read once without a question that could
/// wait on anybody. Never read unless live usage is on, allowed, and the item is still the
/// one that was allowed.
pub(crate) fn key(ctx: &Context) -> Result<Zeroizing<[u8; 16]>, Stale> {
    // Held to the end, so rows asking at once wait for one read and use what it kept.
    let held = gate();
    let state = load(ctx);
    if !state.enabled {
        return Err(Stale::LiveUsageOff);
    }
    if state.approval != Approval::Granted || waits_unsaved(ctx, &state) {
        return Err(Stale::LiveUsageNeedsApproval);
    }
    // The stamp is read without the password and never asks.
    let stamp = match ctx.safe_storage().stamp(ctx) {
        Ok(stamp) => stamp,
        Err(KeyReadError::NoGui) => return Err(Stale::LiveUsageNeedsGui),
        // The item allowed is gone, or macOS refuses even its attributes: what was allowed
        // can no longer be read, and asking again on every refresh would not change that.
        Err(e @ (KeyReadError::Missing | KeyReadError::Denied | KeyReadError::AuthFailed)) => {
            return Err(needs_approval(ctx, &held, e.reason()));
        }
        // Nothing asks here, so a slow or odd answer is the keychain's moment, which says
        // nothing about whether Pitboard is allowed: nothing is recorded.
        Err(KeyReadError::TimedOut | KeyReadError::Other(_)) => {
            return Err(Stale::LoginUnreadable);
        }
    };
    if state.stamp.as_ref() != Some(&stamp) {
        return Err(needs_approval(ctx, &held, "item_changed"));
    }
    if let Some((kept, key)) = cached(ctx)
        && kept == stamp
    {
        return Ok(key);
    }
    match ctx.safe_storage().password(ctx, KeyRead::Refresh) {
        Ok(password) => {
            let key = crypto::derive_key(&password);
            keep(ctx, stamp, key.clone());
            Ok(key)
        }
        // Says only that this session cannot show the question, not that it was refused.
        Err(KeyReadError::NoGui) => Err(Stale::LiveUsageNeedsGui),
        // A no, a password not accepted, a question nobody answered, an item that is gone,
        // or an exit `security` is not known to give: reading again on every refresh would
        // put the question back in front of somebody, or run `security` for every row.
        Err(other) => Err(needs_approval(ctx, &held, other.reason())),
    }
}

/// Why a cookie could not be read with a key.
enum Opened {
    /// The key is not the one the jar was encrypted with.
    WrongKey,
    /// The jar or the value is in a form Pitboard was not written for.
    Unreadable,
}

/// The cleartext of the cookie `name` in `table`, if there is one.
fn open(
    key: &[u8; 16],
    table: &cookies::CookieTable,
    name: &str,
) -> Result<Option<Zeroizing<String>>, Opened> {
    if table.meta_version != cookies::EXPECTED_META {
        return Err(Opened::Unreadable);
    }
    let Some(row) = cookies::cookie(table, name) else {
        return Ok(None);
    };
    match crypto::decrypt(key, &row.host_key, &row.encrypted_value, table.meta_version) {
        Ok(plain) => Ok(Some(plain)),
        Err(CryptoError::BadPadding | CryptoError::HostHashMismatch) => Err(Opened::WrongKey),
        Err(CryptoError::NotV10 | CryptoError::BadLength | CryptoError::NotUtf8) => {
            Err(Opened::Unreadable)
        }
    }
}

/// `account`'s usage, asked of claude.ai with the session in the data folder at `root`,
/// the live one or the account's park: the key is the machine's, so it opens either.
///
/// A key that does not open the live jar is not this Mac's key, and live usage waits to be
/// allowed again. One that does not open a park says only that the park was encrypted with
/// another, copied from another Mac or left from before Claude made its key again: that
/// park is unreadable, and nothing else changes, so allowing again would not loop on it.
pub(crate) fn ask(ctx: &Context, root: &Path, account: &Account) -> Result<Snapshot, Stale> {
    let key = key(ctx)?;
    let is_live = support_dir(ctx).is_some_and(|live| live == root);
    let wrong_key = || {
        if is_live {
            needs_approval(ctx, &gate(), "key_does_not_decrypt")
        } else {
            Stale::ParkUnreadable
        }
    };
    let table = ctx
        .host()
        .cookie_table(&cookies_db(root))
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Stale::NothingSignedIn,
            _ => Stale::LoginUnreadable,
        })?;
    let session = match open(&key, &table, "sessionKey") {
        Ok(Some(session)) => session,
        Ok(None) => return Err(Stale::NothingSignedIn),
        Err(Opened::WrongKey) => return Err(wrong_key()),
        Err(Opened::Unreadable) => return Err(Stale::LoginUnreadable),
    };
    let in_jar = match open(&key, &table, "lastActiveOrg") {
        Ok(org) => org.map(|org| org.to_string()),
        Err(Opened::WrongKey) => return Err(wrong_key()),
        Err(Opened::Unreadable) => None,
    };
    let known = match &account.detail {
        Detail::Desktop {
            organization_uuid, ..
        } => organization_uuid.clone(),
        _ => None,
    };
    let org = in_jar
        .filter(|org| !org.is_empty())
        .or(known)
        .ok_or(Stale::DesktopOrgUnknown)?;
    match ctx.web().usage(ctx, &org, &session) {
        Ok(mut snapshot) => {
            snapshot.account_uuid = Some(account.account_uuid.clone());
            // Only the time is written, and only while still allowed: a row that found
            // approval withdrawn while this one was out is not undone, nor noted as fine.
            let _held = gate();
            let mut state = load(ctx);
            if state.enabled && state.approval == Approval::Granted {
                state.last_ok_at = Some(ctx.now());
                let _ = save(ctx, &state);
            }
            Ok(snapshot)
        }
        // Cloudflare stopping the request knows nothing of the session, and saying
        // claude.ai answered badly would send somebody looking in the wrong place.
        Err(crate::api::ApiError::Blocked { .. }) => Err(Stale::BotCheck),
        // A claude.ai session cannot be renewed by Pitboard, parked or not.
        Err(e) => Err(match Cause::of(&e) {
            Cause::TokenExpired => Stale::SessionExpired,
            Cause::RateLimited => Stale::RateLimited,
            Cause::Unreachable => Stale::Unreachable,
            Cause::ServerError => Stale::ServerError,
            Cause::AnswerNotUnderstood => Stale::AnswerNotUnderstood,
            Cause::LoginRefused => Stale::LoginRefused,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::scripted::{KeyTrouble, ScriptedApi, ScriptedSafeStorage, Trouble};
    use crate::host::memory::MemoryHost;
    use crate::provider::desktop::crypto;
    use crate::provider::desktop::types::{CookieRow, CookieTable};
    use crate::state::Detail;
    use crate::time::{Clock, FixedClock};
    use crate::usage::{Source, Window};
    use std::sync::Arc;

    const NOW: i64 = 1_790_000_000;
    /// Claude's key on the scripted machine, which never was anybody's.
    const PASSWORD: &str = "not-a-real-password";
    const SESSION: &str = "pitboard-test-session";
    const ORG: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const ACCOUNT: &str = "11111111-1111-1111-1111-111111111111";

    struct Desk {
        ctx: Context,
        mem: Arc<MemoryHost>,
        api: Arc<ScriptedApi>,
        keychain: Arc<ScriptedSafeStorage>,
        root: PathBuf,
    }

    impl Drop for Desk {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    impl Desk {
        fn live(&self) -> PathBuf {
            self.root.join("Claude")
        }

        /// A park of `uuid`'s, written `at`, whose jar holds `jar`.
        fn park(&self, uuid: &str, at: i64, jar: CookieTable) -> PathBuf {
            let park = parks_dir(&self.ctx).join(format!("{PARK_PREFIX}{uuid}-{at}"));
            std::fs::create_dir_all(&park).unwrap();
            let db = cookies_db(&park);
            std::fs::write(&db, b"").unwrap();
            self.mem.plant_cookies(&db, jar);
            park
        }

        /// The live folder signed out: its jar holds no session.
        fn signed_out(&self) {
            self.mem.plant_cookies(
                &cookies_db(&self.live()),
                CookieTable {
                    meta_version: cookies::EXPECTED_META,
                    rows: Vec::new(),
                },
            );
        }
    }

    fn jar(password: &str, org: Option<&str>) -> CookieTable {
        let key = crypto::derive_key(password.as_bytes());
        let row = |name: &str, value: &str| CookieRow {
            host_key: ".claude.ai".into(),
            name: name.into(),
            encrypted_value: crypto::encrypt(&key, ".claude.ai", value),
            expires_utc: 0,
        };
        let mut rows = vec![row("sessionKey", SESSION)];
        rows.extend(org.map(|org| row("lastActiveOrg", org)));
        CookieTable {
            meta_version: cookies::EXPECTED_META,
            rows,
        }
    }

    /// A machine with Claude Desktop signed in, its jar encrypted with `PASSWORD`, and a
    /// keychain that holds `keychain`'s answers.
    fn desk(name: &str, keychain: Arc<ScriptedSafeStorage>) -> Desk {
        let root =
            std::env::temp_dir().join(format!("pitboard-live-usage-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Claude")).unwrap();
        let mem = MemoryHost::new();
        let api = ScriptedApi::new();
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_desktop_dir(root.join("Claude").to_string_lossy().into())
            .with_memory_stores(Arc::clone(&mem))
            .with_scripted_api(Arc::clone(&api))
            .with_scripted_safe_storage(Arc::clone(&keychain))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        let db = cookies_db(&root.join("Claude"));
        std::fs::write(&db, b"").unwrap();
        mem.plant_cookies(&db, jar(PASSWORD, Some(ORG)));
        api.web_using(SESSION, used(42.0));
        Desk {
            ctx,
            mem,
            api,
            keychain,
            root,
        }
    }

    fn used(percent: f64) -> Snapshot {
        Snapshot {
            windows: vec![Window {
                kind: "five_hour".into(),
                scope: None,
                percent,
                resets_at: None,
                is_active: false,
                severity: None,
                length_seconds: Some(5 * 3600),
            }],
            observed_at: Some(NOW),
            account_uuid: None,
            source: Source::Live,
            verified: true,
        }
    }

    fn account(org: Option<&str>) -> Account {
        Account {
            label: "desk".into(),
            account_uuid: ACCOUNT.into(),
            email: String::new(),
            parked: None,
            last_used_at: None,
            detail: Detail::Desktop {
                organization_uuid: org.map(str::to_owned),
                session_fingerprint: "f".into(),
                session_expires_at: None,
            },
        }
    }

    /// Live usage turned on and granted, as from a new process that has not read the key.
    fn granted(d: &Desk) {
        let state = enable(&d.ctx).expect("turned on");
        assert_eq!(state.approval, Approval::Granted);
        forget_key(&d.ctx);
    }

    /// Reading Claude's key asks macOS, which may put a question in front of somebody, so
    /// nothing but turning live usage on may ever ask for it: not a switch, a sign-in, a
    /// status, doctor, a renewal, the schedule, the status line, nor reading whose login a
    /// folder holds, and not a status with live usage off or waiting to be allowed again.
    /// None of them even read its stamp.
    ///
    /// The machine is one where everything that could want the key is there: Claude Desktop
    /// signed in with a jar encrypted with it, a second account parked with one too, both
    /// enrolled. The keychain is the forbidding script, which counts every read and panics
    /// at one that may ask.
    #[test]
    fn nothing_but_enable_ever_reads_the_password() {
        let d = desk("forbidden", ScriptedSafeStorage::forbidding());
        let other = "22222222-2222-2222-2222-222222222222";
        std::fs::write(
            crate::provider::desktop::paths::config_file(&d.live()),
            serde_json::json!({ "lastKnownAccountUuid": ACCOUNT }).to_string(),
        )
        .unwrap();
        let park = d.park(other, NOW - 60, jar(PASSWORD, Some(ORG)));
        std::fs::write(
            crate::provider::desktop::paths::config_file(&park),
            serde_json::json!({ "lastKnownAccountUuid": other }).to_string(),
        )
        .unwrap();
        let mut state = crate::state::State::default();
        state.accounts.push(account(Some(ORG)));
        let mut parked = account(Some(ORG));
        parked.label = "away".into();
        parked.account_uuid = other.into();
        parked.parked = Some(crate::state::Park {
            service: park.file_name().unwrap().to_string_lossy().into_owned(),
            parked_at: NOW - 60,
            refresh_fingerprint: "f".into(),
            access_expires_at: None,
            refresh_expires_at: None,
        });
        state.accounts.push(parked);
        state.set_active(crate::provider::ProviderId::Desktop, Some("desk".into()));
        crate::state::save(&d.ctx, &state).unwrap();
        let ctx = d
            .ctx
            .clone()
            .with_desktop_app(d.root.join("Claude.app").to_string_lossy().into())
            .with_search_path(d.root.join("bin").to_string_lossy().into());
        let pitboard = crate::service::Pitboard::new(ctx.clone());

        let everything_but_enable = || {
            let _ = pitboard.status_offline();
            let _ = pitboard.status(true);
            let _ = pitboard.doctor();
            let _ = pitboard.renew();
            let _ = pitboard.schedule();
            let _ = pitboard.statusline("{}");
            let _ = pitboard.holding(crate::provider::ProviderId::Desktop);
            for root in [d.live(), park.clone()] {
                let _ = crate::provider::desktop::identity::identify_tree(&ctx, &root);
            }
            let _ = crate::provider::desktop::history::local_usage(&ctx, &account(Some(ORG)));
            let _ = pitboard.switch_to("away");
            let _ = pitboard.switch_to("desk");
            let _ = pitboard.enroll_current("again");
            let _ = pitboard.rename("away", "elsewhere");
            let _ = pitboard.forget("elsewhere");
            let _ = pitboard.status(true);
        };

        // Live usage off.
        everything_but_enable();
        assert_eq!(key(&ctx).err(), Some(Stale::LiveUsageOff));
        assert_eq!(
            ask(&ctx, &d.live(), &account(Some(ORG))).err(),
            Some(Stale::LiveUsageOff)
        );

        // On, and waiting to be allowed again.
        save(
            &ctx,
            &LiveUsage {
                enabled: true,
                approval: Approval::NeedsApproval,
                reason: Some("denied".into()),
                ..LiveUsage::default()
            },
        )
        .unwrap();
        everything_but_enable();
        assert_eq!(key(&ctx).err(), Some(Stale::LiveUsageNeedsApproval));
        assert_eq!(
            ask(&ctx, &d.live(), &account(Some(ORG))).err(),
            Some(Stale::LiveUsageNeedsApproval)
        );

        assert_eq!(d.keychain.password_reads(), 0);
        assert_eq!(d.keychain.stamp_reads(), 0);

        // The count is not blind: with the key in the item, turning it on reads it, once,
        // the way that may ask.
        d.keychain
            .now_holding(PASSWORD)
            .changed_at("20260727084307Z");
        enable(&ctx).expect("turned on");
        assert_eq!(d.keychain.password_reads(), 1);
        assert_eq!(d.keychain.approval_reads(), 1);
        assert!(d.keychain.stamp_reads() > 0);
    }

    /// A refresh that nobody answers gives up after ten seconds, and from then on waits
    /// to be allowed again instead of asking on every refresh.
    #[test]
    fn a_refresh_gives_up_after_ten_seconds() {
        assert_eq!(KeyRead::Refresh.limit(), std::time::Duration::from_secs(10));
        let d = desk("timed-out", ScriptedSafeStorage::holding(PASSWORD));
        granted(&d);
        d.keychain.refusing(KeyTrouble::TimedOut);

        assert_eq!(key(&d.ctx).err(), Some(Stale::LiveUsageNeedsApproval));
        let state = load(&d.ctx);
        assert_eq!(state.approval, Approval::NeedsApproval);
        assert_eq!(state.reason.as_deref(), Some("timed_out"));
        assert_eq!(state.changed_at, Some(NOW));
        assert!(state.enabled, "still on, waiting to be allowed again");
        assert_eq!(
            d.keychain.password_reads(),
            2,
            "the approval and one refresh"
        );

        assert_eq!(key(&d.ctx).err(), Some(Stale::LiveUsageNeedsApproval));
        assert_eq!(
            ask(&d.ctx, &d.live(), &account(None)).err(),
            Some(Stale::LiveUsageNeedsApproval)
        );
        assert_eq!(
            d.keychain.password_reads(),
            2,
            "never asked again by itself"
        );
    }

    /// Exit 36 says this session cannot show macOS's question, which says nothing about
    /// whether Pitboard is allowed: approval is left as it was.
    #[test]
    fn exit_36_leaves_approval_alone() {
        let d = desk("no-gui", ScriptedSafeStorage::holding(PASSWORD));
        granted(&d);
        let before = load(&d.ctx);
        d.keychain.refusing(KeyTrouble::NoGui);

        assert_eq!(key(&d.ctx).err(), Some(Stale::LiveUsageNeedsGui));
        assert_eq!(
            ask(&d.ctx, &d.live(), &account(None)).err(),
            Some(Stale::LiveUsageNeedsGui)
        );
        assert_eq!(load(&d.ctx), before);

        // Turning it on from such a session is refused the same way, and changes nothing.
        let refused = enable(&d.ctx).expect_err("no screen to ask on");
        assert!(
            matches!(&refused, Error::LiveUsageNotAllowed { reason, .. } if reason == "no_gui"),
            "{refused:?}"
        );
        assert_eq!(load(&d.ctx), before);
    }

    /// A no, or a password that was not accepted, waits to be allowed again.
    #[test]
    fn denied_or_auth_failed_needs_approval() {
        for (trouble, reason) in [
            (KeyTrouble::Denied, "denied"),
            (KeyTrouble::AuthFailed, "auth_failed"),
        ] {
            let d = desk(reason, ScriptedSafeStorage::holding(PASSWORD));
            granted(&d);
            d.keychain.refusing(trouble);
            assert_eq!(key(&d.ctx).err(), Some(Stale::LiveUsageNeedsApproval));
            let state = load(&d.ctx);
            assert_eq!(state.approval, Approval::NeedsApproval);
            assert_eq!(state.reason.as_deref(), Some(reason));

            let refused = enable(&d.ctx).expect_err("still refused");
            assert!(
                matches!(&refused, Error::LiveUsageNotAllowed { reason: r, .. } if r == reason),
                "{refused:?}"
            );
            assert_eq!(load(&d.ctx).approval, Approval::NeedsApproval);
        }
    }

    /// A key Claude made again is another key: allowing the old one says nothing about it,
    /// so it is not read, even with the old one still in memory.
    #[test]
    fn a_changed_item_needs_approval_without_reading() {
        let d = desk("changed", ScriptedSafeStorage::holding(PASSWORD));
        enable(&d.ctx).expect("turned on");
        assert_eq!(d.keychain.password_reads(), 1);
        d.keychain.changed_at("20261001120000Z");

        assert_eq!(key(&d.ctx).err(), Some(Stale::LiveUsageNeedsApproval));
        let state = load(&d.ctx);
        assert_eq!(state.approval, Approval::NeedsApproval);
        assert_eq!(state.reason.as_deref(), Some("item_changed"));
        assert_eq!(d.keychain.password_reads(), 1, "the new key is never read");
        assert_eq!(d.api.calls(), 0);
    }

    /// Answering Always Allow rewrites the item's access list, which may change its `mdat`:
    /// the stamp kept is the one read after the answer, so the first refresh does not take
    /// the answer itself for a new key and ask again.
    #[test]
    fn allowing_does_not_make_the_item_look_changed() {
        let d = desk("allowed", ScriptedSafeStorage::holding(PASSWORD));
        d.keychain.allowing_changes_it("20261003090000Z");
        enable(&d.ctx).expect("turned on");

        assert!(key(&d.ctx).is_ok());
        let state = load(&d.ctx);
        assert_eq!(state.approval, Approval::Granted);
        assert_eq!(state.reason, None);
        assert_eq!(d.keychain.password_reads(), 1);
    }

    /// One read of the key per process: every refresh after the first is answered from
    /// memory, so macOS is asked about it once.
    #[test]
    fn the_key_is_read_once_per_process() {
        let d = desk("once", ScriptedSafeStorage::holding(PASSWORD));
        enable(&d.ctx).expect("turned on");
        for _ in 0..3 {
            let read = ask(&d.ctx, &d.live(), &account(None)).expect("asked");
            assert_eq!(read.windows[0].percent, 42.0);
            assert_eq!(read.account_uuid.as_deref(), Some(ACCOUNT));
        }
        assert_eq!(d.keychain.password_reads(), 1, "the approval was enough");
        assert_eq!(load(&d.ctx).last_ok_at, Some(NOW));
        assert!(d.api.asked().iter().all(|asked| matches!(
            asked,
            crate::api::scripted::Asked::WebUsage { org, session } if org == ORG && session == SESSION
        )));

        // A new process reads it once more, without asking anybody.
        forget_key(&d.ctx);
        for _ in 0..3 {
            ask(&d.ctx, &d.live(), &account(None)).expect("asked");
        }
        assert_eq!(d.keychain.password_reads(), 2);
        assert_eq!(d.keychain.approval_reads(), 1);
    }

    /// A refresh reads the live folder and every park at once, each on its own thread. The
    /// first to need the key reads it, and the rest wait for that read and use what it
    /// kept, so one refresh asks macOS once however many rows it has.
    #[test]
    fn rows_read_at_once_share_one_read_of_the_key() {
        let d = desk("at-once", ScriptedSafeStorage::holding(PASSWORD));
        granted(&d);
        d.keychain.slow(std::time::Duration::from_millis(200));
        let rows = 8;
        let start = std::sync::Barrier::new(rows);
        std::thread::scope(|scope| {
            for _ in 0..rows {
                scope.spawn(|| {
                    start.wait();
                    ask(&d.ctx, &d.live(), &account(None)).expect("asked");
                });
            }
        });
        assert_eq!(
            d.keychain.password_reads(),
            2,
            "the approval and one refresh, for {rows} rows"
        );
        assert_eq!(d.api.calls(), rows);
    }

    /// Killing `security` leaves macOS's question on screen (experiment U-K3), so a refresh
    /// whose read gives up must not let its other rows read again: each would leave one
    /// more question behind. The first row's timeout is recorded, and the rest that waited
    /// on it are told approval is needed without reading. This guards how the gate already
    /// behaved, and holds within one process only: the gate is not shared between
    /// processes, so two reading at once can each leave a question.
    #[test]
    fn rows_waiting_on_a_read_that_gave_up_do_not_read_again() {
        let d = desk("at-once-timed-out", ScriptedSafeStorage::holding(PASSWORD));
        granted(&d);
        d.keychain
            .slow(std::time::Duration::from_millis(200))
            .refusing(KeyTrouble::TimedOut);
        let rows = 8;
        let start = std::sync::Barrier::new(rows);
        let answers: Vec<Option<Stale>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..rows)
                .map(|_| {
                    scope.spawn(|| {
                        start.wait();
                        ask(&d.ctx, &d.live(), &account(None)).err()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(
            answers
                .iter()
                .all(|a| *a == Some(Stale::LiveUsageNeedsApproval)),
            "{answers:?}"
        );
        assert_eq!(
            d.keychain.password_reads(),
            2,
            "the approval and the one refresh that gave up, for {rows} rows"
        );
        assert_eq!(load(&d.ctx).reason.as_deref(), Some("timed_out"));
        assert_eq!(d.api.calls(), 0);
    }

    /// claude.ai, answering only after `meanwhile` has run to its end on another thread.
    struct Meanwhile {
        web: Arc<ScriptedApi>,
        meanwhile: Box<dyn Fn() + Send + Sync>,
    }

    impl std::fmt::Debug for Meanwhile {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Meanwhile").field("web", &self.web).finish()
        }
    }

    impl crate::provider::desktop::web::ClaudeWeb for Meanwhile {
        fn usage(
            &self,
            ctx: &Context,
            org: &str,
            session_key: &str,
        ) -> Result<Snapshot, crate::api::ApiError> {
            (self.meanwhile)();
            crate::provider::desktop::web::ClaudeWeb::usage(&*self.web, ctx, org, session_key)
        }
    }

    /// A reading that succeeds while another row finds live usage must wait to be allowed
    /// again records nothing over it: approval stays withdrawn, and no success is noted
    /// for a key that is no longer allowed.
    #[test]
    fn a_success_does_not_undo_approval_withdrawn_meanwhile() {
        let d = desk("withdrawn", ScriptedSafeStorage::holding(PASSWORD));
        granted(&d);
        let other = d.ctx.clone();
        let keychain = Arc::clone(&d.keychain);
        let mut ctx = d.ctx.clone();
        ctx.web = Arc::new(Meanwhile {
            web: Arc::clone(&d.api),
            meanwhile: Box::new(move || {
                // Claude made its key again while the request was out.
                keychain.changed_at("20261001120000Z");
                let other = other.clone();
                std::thread::spawn(move || key(&other).err())
                    .join()
                    .unwrap();
            }),
        });

        ask(&ctx, &d.live(), &account(None)).expect("claude.ai answered");
        let state = load(&d.ctx);
        assert_eq!(state.approval, Approval::NeedsApproval);
        assert_eq!(state.reason.as_deref(), Some("item_changed"));
        assert_eq!(state.last_ok_at, None, "no success is noted over it");
    }

    /// A key that opens nothing is not kept, and is not taken as allowed.
    #[test]
    fn a_key_that_does_not_decrypt_is_not_kept() {
        let d = desk("wrong-key", ScriptedSafeStorage::holding("somebody else's"));
        let refused = enable(&d.ctx).expect_err("the key opens nothing");
        assert!(
            matches!(&refused, Error::LiveUsageNotAllowed { reason, .. } if reason == "key_does_not_decrypt"),
            "{refused:?}"
        );
        assert_ne!(load(&d.ctx).approval, Approval::Granted);
        assert!(cached(&d.ctx).is_none());

        // Allowed once, and then the key stops opening the jar.
        d.keychain.now_holding(PASSWORD);
        granted(&d);
        d.keychain.now_holding("somebody else's");
        assert_eq!(
            ask(&d.ctx, &d.live(), &account(None)).err(),
            Some(Stale::LiveUsageNeedsApproval)
        );
        let state = load(&d.ctx);
        assert_eq!(state.approval, Approval::NeedsApproval);
        assert_eq!(state.reason.as_deref(), Some("key_does_not_decrypt"));
        assert!(cached(&d.ctx).is_none());
        assert_eq!(d.api.calls(), 0, "nothing was sent");
    }

    /// The organisation asked about is the one the jar names, else the one the account is
    /// known to belong to, and never a guess.
    #[test]
    fn the_org_comes_from_the_jar_or_the_account() {
        let d = desk("org", ScriptedSafeStorage::holding(PASSWORD));
        enable(&d.ctx).expect("turned on");
        // The same machine, with a jar that names no organisation.
        let mem = MemoryHost::new();
        let ctx = d.ctx.clone().with_memory_stores(Arc::clone(&mem));
        mem.plant_cookies(&cookies_db(&d.live()), jar(PASSWORD, None));
        assert_eq!(
            ask(&ctx, &d.live(), &account(None)).err(),
            Some(Stale::DesktopOrgUnknown)
        );
        ask(&ctx, &d.live(), &account(Some(ORG))).expect("the account's own");

        // A session claude.ai no longer takes is an expired one.
        d.api.web_trouble(SESSION, Trouble::Unauthorized);
        assert_eq!(
            ask(&d.ctx, &d.live(), &account(None)).err(),
            Some(Stale::SessionExpired)
        );
    }

    /// An answer `security` is not known to give, to a read of the password, waits to be
    /// allowed again like any other failed read: reading again on every row of every
    /// refresh would run `security` over and over, and could put the question in front of
    /// somebody each time. Turning it on through the same trouble is refused with what
    /// `security` said, and turning it on once the keychain answers works again.
    #[test]
    fn an_odd_answer_to_a_refresh_waits_to_be_allowed_again() {
        let d = desk("odd", ScriptedSafeStorage::holding(PASSWORD));
        granted(&d);
        d.keychain.refusing(KeyTrouble::Other);

        assert_eq!(key(&d.ctx).err(), Some(Stale::LiveUsageNeedsApproval));
        let state = load(&d.ctx);
        assert_eq!(state.approval, Approval::NeedsApproval);
        assert_eq!(state.reason.as_deref(), Some("other"));
        assert!(state.enabled, "still on, waiting to be allowed again");
        for _ in 0..3 {
            assert_eq!(
                ask(&d.ctx, &d.live(), &account(None)).err(),
                Some(Stale::LiveUsageNeedsApproval)
            );
        }
        assert_eq!(
            d.keychain.password_reads(),
            2,
            "the approval and one refresh, never again by itself"
        );
        let waiting = load(&d.ctx);

        let refused = enable(&d.ctx).expect_err("the keychain answered oddly");
        assert!(
            matches!(&refused, Error::LiveUsageNotAllowed { reason, .. } if reason == "other"),
            "{refused:?}"
        );
        // What `security` said is kept, so the refusal says more than that something failed.
        assert!(
            matches!(&refused, Error::LiveUsageNotAllowed { detail: Some(detail), .. }
                if detail.contains("scripted")),
            "{refused:?}"
        );
        assert!(refused.to_string().contains("scripted"), "{refused}");
        assert_eq!(
            load(&d.ctx),
            waiting,
            "nothing was asked, so nothing changed"
        );

        d.keychain.now_holding(PASSWORD);
        enable(&d.ctx).expect("allowed again");
        ask(&d.ctx, &d.live(), &account(None)).expect("answered again");
    }

    /// Waiting to be allowed again holds for the rest of the process even where it could
    /// not be written down, so a home that cannot be written to does not have the password
    /// read on every refresh. Turning live usage on again ends the wait.
    #[test]
    fn waiting_to_be_allowed_holds_when_it_cannot_be_saved() {
        use std::os::unix::fs::PermissionsExt;
        let d = desk("unsaved", ScriptedSafeStorage::holding(PASSWORD));
        granted(&d);
        let before = load(&d.ctx);
        d.keychain.refusing(KeyTrouble::TimedOut);
        let home = desktop_home(&d.ctx);
        let writable = |mode| {
            std::fs::set_permissions(&home, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        writable(0o500);

        let first = key(&d.ctx).err();
        let later: Vec<_> = (0..3)
            .map(|_| ask(&d.ctx, &d.live(), &account(None)).err())
            .collect();
        let reads = d.keychain.password_reads();
        let after = load(&d.ctx);
        writable(0o700);

        assert_eq!(first, Some(Stale::LiveUsageNeedsApproval));
        assert_eq!(after, before, "nothing could be written");
        assert!(
            later
                .iter()
                .all(|said| *said == Some(Stale::LiveUsageNeedsApproval)),
            "{later:?}"
        );
        assert_eq!(
            reads, 2,
            "the approval and one refresh, never again by itself"
        );

        d.keychain.now_holding(PASSWORD);
        enable(&d.ctx).expect("allowed again");
        ask(&d.ctx, &d.live(), &account(None)).expect("answered again");
    }

    /// claude.ai's bot check stopping a request says nothing about the session or the key:
    /// it is named for what it is, rather than read as claude.ai answering badly, and
    /// approval is left as it was.
    #[test]
    fn a_bot_check_is_named_for_what_it_is() {
        let d = desk("bot-check", ScriptedSafeStorage::holding(PASSWORD));
        granted(&d);
        let before = load(&d.ctx);
        d.api.web_trouble(SESSION, Trouble::BotCheck);
        assert_eq!(
            ask(&d.ctx, &d.live(), &account(Some(ORG))).err(),
            Some(Stale::BotCheck)
        );
        assert_eq!(load(&d.ctx), before);
        assert_eq!(Stale::BotCheck.code(), "bot_check");
        let said = Stale::BotCheck
            .explanation_for(crate::provider::ProviderId::Desktop)
            .expect("worth a word");
        assert!(said.contains("bot check"), "{said}");
    }

    /// The item's attributes are read without asking anybody, so what goes wrong there is
    /// told apart: a session with no screen says so, an item that is gone waits to be
    /// allowed again, and a keychain that is slow or odd for a moment changes nothing.
    #[test]
    fn a_stamp_that_cannot_be_read_is_told_apart() {
        for (trouble, said, approval) in [
            (
                KeyTrouble::NoGui,
                Stale::LiveUsageNeedsGui,
                Approval::Granted,
            ),
            (
                KeyTrouble::TimedOut,
                Stale::LoginUnreadable,
                Approval::Granted,
            ),
            (KeyTrouble::Other, Stale::LoginUnreadable, Approval::Granted),
            (
                KeyTrouble::Missing,
                Stale::LiveUsageNeedsApproval,
                Approval::NeedsApproval,
            ),
        ] {
            let d = desk(
                &format!("stamp-{trouble:?}"),
                ScriptedSafeStorage::holding(PASSWORD),
            );
            granted(&d);
            d.keychain.unlisted(trouble);
            assert_eq!(key(&d.ctx).err(), Some(said), "{trouble:?}");
            let state = load(&d.ctx);
            assert_eq!(state.approval, approval, "{trouble:?}");
            if approval == Approval::NeedsApproval {
                assert_eq!(state.reason.as_deref(), Some("item_missing"));
            }
            assert_eq!(
                d.keychain.password_reads(),
                1,
                "{trouble:?}: only the approval"
            );
        }
    }

    /// A park whose jar another key encrypted, copied from another Mac or left from before
    /// Claude made its key again, says nothing about this Mac's key: only that park reads
    /// as unreadable, and live usage stays allowed for everything else.
    #[test]
    fn a_park_the_key_does_not_open_is_that_park_alone() {
        let d = desk("foreign-park", ScriptedSafeStorage::holding(PASSWORD));
        granted(&d);
        let park = d.park(ACCOUNT, NOW - 60, jar("somebody else's", Some(ORG)));

        assert_eq!(
            ask(&d.ctx, &park, &account(None)).err(),
            Some(Stale::ParkUnreadable)
        );
        let state = load(&d.ctx);
        assert_eq!(state.approval, Approval::Granted);
        assert_eq!(state.reason, None);
        assert!(cached(&d.ctx).is_some(), "the key is kept");
        assert_eq!(d.api.calls(), 0, "nothing was sent for the park");

        ask(&d.ctx, &d.live(), &account(None)).expect("the live folder still answers");
        assert_eq!(
            d.keychain.password_reads(),
            2,
            "the approval and one refresh"
        );
    }

    /// Turning live usage on proves the key opens a session first. With nobody signed in
    /// and nothing parked there is nothing to prove it on, so it is not taken as allowed,
    /// and nothing is recorded either: nobody refused anything.
    #[test]
    fn enable_needs_a_session_to_prove_the_key_on() {
        let d = desk("no-session", ScriptedSafeStorage::holding(PASSWORD));
        d.signed_out();
        let refused = enable(&d.ctx).expect_err("nothing to prove it on");
        assert!(
            matches!(&refused, Error::LiveUsageNotAllowed { reason, .. } if reason == "no_session"),
            "{refused:?}"
        );
        assert_eq!(load(&d.ctx), LiveUsage::default());
        assert!(cached(&d.ctx).is_none());

        // A park another key encrypted proves nothing either way about this Mac's key.
        d.park(ACCOUNT, NOW - 120, jar("somebody else's", Some(ORG)));
        assert!(enable(&d.ctx).is_err());
        assert_ne!(load(&d.ctx).approval, Approval::Granted);

        // A newer park the key opens is proof enough.
        d.park(ACCOUNT, NOW - 60, jar(PASSWORD, Some(ORG)));
        let state = enable(&d.ctx).expect("proven on the park");
        assert_eq!(state.approval, Approval::Granted);
    }

    /// The state file says whether Pitboard may read a key that opens every session on the
    /// machine, so it is the owner's alone.
    #[test]
    fn the_state_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = desk("private", ScriptedSafeStorage::holding(PASSWORD));
        assert_eq!(load(&d.ctx), LiveUsage::default());
        let state = enable(&d.ctx).expect("turned on");
        assert_eq!(load(&d.ctx), state);
        let file = state_file(&d.ctx);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(file.parent().unwrap()), 0o700);
        let raw = std::fs::read_to_string(&file).unwrap();
        assert!(!raw.contains(PASSWORD) && !raw.contains(SESSION), "{raw}");

        let off = disable(&d.ctx).expect("turned off");
        assert!(!off.enabled);
        assert_eq!(off.approval, Approval::Granted, "approval is kept");
        assert!(cached(&d.ctx).is_none());
        assert_eq!(mode(&file), 0o600);
    }
}
