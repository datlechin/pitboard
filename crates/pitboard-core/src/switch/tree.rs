//! Moving a login kept in a folder: Claude Desktop's, which is items of its data folder and
//! three keys of its `config.json`.
//!
//! The same two rules as a credential switch set the order of every step, kept the way a
//! folder can keep them. Nothing moves while the app has the folder open, so every move is
//! preceded by asking what is running. And what is parked is durable and recorded before
//! anything is installed in its place: until the outgoing account's park is in the state, a
//! run that dies is undone; after it, the run is finished. Nothing found in the data folder
//! is ever deleted: what is in the way of a move is set aside in the strays directory.

use super::tree_journal::{self, ConfigKeyNames, Item, Operation, TreeJournal};
use super::{Enrolled, Outcome, Settled, purge};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::provider::desktop::TreeIdentity;
use crate::provider::desktop::config::{self, ConfigKeys};
use crate::provider::desktop::identity::{self, LiveOwner};
use crate::provider::desktop::paths;
use crate::provider::{self, ProviderId, TreeLogin};
use crate::service::Warning;
use crate::state::{self, Account, Detail, Key, Park, State};
use crate::store::tree as moves;
use crate::{atomic, fault, holder};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Component, Path, PathBuf};

/// The account's keys of `config.json`, as a park keeps them.
pub(super) const CONFIG_KEYS_FILE: &str = "config-keys.json";

/// What a park says it holds.
pub(super) const MANIFEST_FILE: &str = "manifest.json";

/// A parked login kept for less than this is said to be running out, since nothing can
/// renew it.
const EXPIRES_SOON: i64 = 7 * 86_400;

/// What a park holds, written beside it when it is made. The session is told by its
/// fingerprint, never by the cookie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Manifest {
    pub account_uuid: String,
    pub fingerprint: String,
    pub items: Vec<String>,
    pub parked_at: i64,
}

/// A sign-out Pitboard made, waiting for somebody to sign in to another account in the app
/// and enrol it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Awaiting {
    /// The account that was parked, as its label.
    pub from_label: String,
    pub started_at: i64,
}

/// The sign-out Pitboard made that no enrolment has followed yet, where there is one.
pub fn awaiting_sign_in(ctx: &Context) -> Option<Awaiting> {
    let raw = std::fs::read_to_string(awaiting_path(ctx)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn awaiting_path(ctx: &Context) -> PathBuf {
    paths::desktop_home(ctx).join("awaiting.json")
}

pub(super) fn write_awaiting(ctx: &Context, awaiting: &Awaiting) -> Result<()> {
    moves::ensure_private_dir(&paths::desktop_home(ctx))?;
    write_secret_json(&awaiting_path(ctx), awaiting)
}

fn clear_awaiting(ctx: &Context) {
    let _ = std::fs::remove_file(awaiting_path(ctx));
}

/// The tool's folder login, or a usage error for a tool whose login is a credential.
pub(super) fn tree_of(which: ProviderId) -> Result<&'static dyn TreeLogin> {
    provider::of(which).tree().ok_or_else(|| {
        Error::Usage(format!(
            "{} keeps its login as a credential, not in a folder",
            which.name()
        ))
    })
}

/// Refuses while anything has the tool's folder open: a process from its bundle, a lock in
/// the folder naming a live process, or a process list that could not be read, which counts
/// as open.
pub(super) fn quiet(ctx: &Context, which: ProviderId) -> Result<()> {
    let tree = tree_of(which)?;
    let open = |pids: Vec<u32>, detail: Option<String>| Error::AppStillOpen {
        tool: which,
        name: which.program().to_string(),
        pids,
        detail,
        midway: false,
    };
    match holder::find_within(ctx, tree) {
        None => {
            return Err(open(
                Vec::new(),
                Some("the process list could not be read".into()),
            ));
        }
        Some(holding) if !holding.is_empty() => {
            return Err(open(
                holding
                    .iter()
                    .flat_map(|h| h.pids.iter().copied())
                    .collect(),
                None,
            ));
        }
        Some(_) => {}
    }
    // The lock is a second sign beside the process list, for a build that keeps one; the
    // build measured keeps none (experiment E14), and its absence passes.
    if let Some(root) = tree.root(ctx) {
        match holder::lock_holder(ctx, tree, &root) {
            Ok(None) => {}
            Ok(Some(pid)) => return Err(open(vec![pid], None)),
            Err(e) => {
                return Err(open(
                    Vec::new(),
                    Some(format!(
                        "its lock in the data folder could not be read: {e}"
                    )),
                ));
            }
        }
    }
    Ok(())
}

/// [`quiet`], asked once the record of intent is written: a refusal now leaves part of the
/// move made, and says so rather than that nothing moved.
pub(super) fn still_quiet(ctx: &Context, which: ProviderId) -> Result<()> {
    quiet(ctx, which).map_err(|e| match e {
        Error::AppStillOpen {
            tool,
            name,
            pids,
            detail,
            ..
        } => Error::AppStillOpen {
            tool,
            name,
            pids,
            detail,
            midway: true,
        },
        other => other,
    })
}

/// The live data folder, made ready for a move: there, quiet, and on one volume with a
/// private parks directory.
fn prepare(ctx: &Context, which: ProviderId) -> Result<(&'static dyn TreeLogin, PathBuf)> {
    let tree = tree_of(which)?;
    let root = tree
        .root(ctx)
        .filter(|root| root.is_dir())
        .ok_or(Error::LiveCredentialAbsent { tool: which })?;
    quiet(ctx, which)?;
    moves::ensure_private_dir(&paths::desktop_home(ctx))?;
    let parks = paths::parks_dir(ctx);
    moves::ensure_private_dir(&parks)?;
    moves::same_device(ctx, &root, &parks)?;
    Ok((tree, root))
}

/// The inode at `path` in the data folder or a park, or why it could not be read.
pub(super) fn inode_at(path: &Path) -> Result<Option<u64>> {
    moves::inode(path).map_err(|source| Error::DesktopDataInaccessible {
        path: path.to_path_buf(),
        source,
    })
}

/// One item moved by one rename. A move made and then not synced is a move made.
pub(super) fn move_item(from: &Path, to: &Path) -> Result<u64> {
    match moves::rename_durably(from, to) {
        Ok(inode) => Ok(inode),
        Err(e) => match moves::moved_not_synced(&e) {
            Some(moved) => Ok(moved.inode),
            None => Err(Error::DesktopDataInaccessible {
                path: from.to_path_buf(),
                source: e,
            }),
        },
    }
}

/// `value` as JSON at `path`, readable by this user only.
pub(super) fn write_secret_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let body = serde_json::to_vec_pretty(value).expect("a record of Pitboard's serialises");
    atomic::write(path, &body, atomic::Perms::Secret).map_err(|source| Error::HomeUnwritable {
        path: path.to_path_buf(),
        source,
    })
}

/// Record the session `found` as the one `key`'s account was last seen with.
pub(super) fn note_session(state: &mut State, key: &Key, found: &TreeIdentity) {
    let Some(mut account) = state.get(key).cloned() else {
        return;
    };
    let organization_uuid = match &account.detail {
        Detail::Desktop {
            organization_uuid, ..
        } => organization_uuid.clone(),
        _ => None,
    };
    account.detail = Detail::Desktop {
        organization_uuid,
        session_fingerprint: found.fingerprint.clone(),
        session_expires_at: found.expires_at,
    };
    state.upsert(account);
}

/// A name for a new park of the account `uuid`, free in the parks directory.
fn park_name(ctx: &Context, uuid: &str) -> String {
    let safe: String = uuid
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let parks = paths::parks_dir(ctx);
    let mut at = ctx.now_millis();
    loop {
        let name = format!("{}{safe}-{at}", paths::PARK_PREFIX);
        if std::fs::symlink_metadata(parks.join(&name)).is_err() {
            return name;
        }
        at += 1;
    }
}

/// The park `park` of `account`, checked before anything is written: one park's name, a
/// private directory, holding the account's session and keys.
pub(crate) fn check_park(
    ctx: &Context,
    label: &str,
    account: &Account,
    park: &Park,
) -> Result<PathBuf> {
    let corrupt = |detail: &str| Error::ParkedCredentialCorrupt {
        label: label.to_string(),
        detail: detail.to_string(),
    };
    let mut parts = Path::new(&park.service).components();
    let one_name = matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    );
    if !one_name || !park.service.starts_with(paths::PARK_PREFIX) {
        return Err(corrupt("its name is not one of Pitboard's parks"));
    }
    let dir = paths::parks_dir(ctx).join(&park.service);
    let found = match std::fs::symlink_metadata(&dir) {
        Ok(found) => found,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::ParkedCredentialMissing {
                label: label.to_string(),
            });
        }
        Err(source) => return Err(Error::HomeUnwritable { path: dir, source }),
    };
    if found.file_type().is_symlink() || !found.is_dir() {
        return Err(corrupt("it is not a directory"));
    }
    if found.mode() & 0o077 != 0 {
        return Err(corrupt("others can reach it"));
    }
    let manifest: Manifest = std::fs::read_to_string(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .ok_or_else(|| corrupt("it says nothing of whose it is"))?;
    if manifest.account_uuid != account.account_uuid {
        return Err(Error::ParkedLoginBelongsElsewhere {
            label: label.to_string(),
            email: "another account".into(),
        });
    }
    if !dir.join(CONFIG_KEYS_FILE).is_file() {
        return Err(corrupt("the keys of its config are missing"));
    }
    match identity::session_of(ctx, &dir)? {
        Some(session)
            if session.fingerprint == manifest.fingerprint
                && session.fingerprint == park.refresh_fingerprint => {}
        _ => return Err(corrupt("its session is not the one Pitboard parked")),
    }
    Ok(dir)
}

/// The keys of `config.json` an outgoing account has, by name only.
fn key_names(keys: &ConfigKeys) -> Vec<String> {
    keys.0.keys().cloned().collect()
}

/// Enrol the account signed in to the app now. Nothing moves, so no journal is written.
pub(super) fn enroll_current(
    ctx: &Context,
    key: &Key,
    state: &mut State,
) -> Result<(Enrolled, Vec<Warning>)> {
    let which = key.provider;
    let tree = tree_of(which)?;
    let root = tree
        .root(ctx)
        .ok_or(Error::LiveCredentialAbsent { tool: which })?;
    // Quit first, so what the app keeps in memory of the sign-in is on disk.
    quiet(ctx, which)?;
    let live = tree
        .identify(ctx, &root)?
        .ok_or(Error::LiveCredentialAbsent { tool: which })?;
    let owner = match identity::whose(state, Some(live.clone())) {
        // Enrolling under the label it is unconfirmed for is the confirmation.
        Err(Error::DesktopIdentityUnconfirmed { label })
            if label == key.label
                && state
                    .get(key)
                    .is_some_and(|a| a.account_uuid == live.account_uuid) =>
        {
            LiveOwner::Enrolled(key.clone())
        }
        other => other?,
    };
    let previous = state.active_for(which).map(str::to_string);
    let enrolled = match owner {
        LiveOwner::Nobody => return Err(Error::LiveCredentialAbsent { tool: which }),
        LiveOwner::Enrolled(found) if found != *key => {
            return Err(Error::AlreadyEnrolled {
                tool: which,
                email: "the account in Claude".into(),
                label: found.typed(),
            });
        }
        LiveOwner::Enrolled(_) => {
            // A park of the account signed in now is an older session of it, put aside
            // before somebody signed in to it again in the app: lapsed, or about to be. It
            // must not stay beside the login in use, so it goes.
            if let Some(old) = state.get(key).and_then(|a| a.parked.clone()) {
                state.discard(&old.service);
            }
            note_session(state, key, &live);
            state.used(key, ctx.now());
            Enrolled::InUse {
                email: String::new(),
                again: true,
            }
        }
        LiveOwner::NotEnrolled(found) => {
            if let Some(taken) = state.get(key)
                && taken.account_uuid != found.account_uuid
            {
                return Err(Error::LabelTaken {
                    label: key.typed(),
                    email: if taken.email.is_empty() {
                        "another account".into()
                    } else {
                        taken.email.clone()
                    },
                });
            }
            state.upsert(Account {
                label: key.label.clone(),
                account_uuid: found.account_uuid.clone(),
                email: String::new(),
                parked: None,
                last_used_at: Some(ctx.now()),
                detail: Detail::Desktop {
                    organization_uuid: None,
                    session_fingerprint: found.fingerprint.clone(),
                    session_expires_at: found.expires_at,
                },
            });
            Enrolled::Current {
                email: String::new(),
            }
        }
    };
    let mut warnings = Vec::new();
    if let Some(previous) = previous.filter(|label| *label != key.label) {
        let previous = Key::new(which, previous);
        if state.get(&previous).is_some_and(|a| a.parked.is_none()) {
            warnings.push(Warning::ReplacedOutsidePitboard {
                tool: which,
                label: state.typed(&previous),
            });
        }
    }
    state.set_active(which, Some(key.label.clone()));
    state::save(ctx, state)?;
    clear_awaiting(ctx);
    let pending = purge(ctx, state);
    warnings.extend((pending > 0).then_some(Warning::ParksPendingRemoval(pending)));
    Ok((enrolled, warnings))
}

/// The items `journal` says are the outgoing account's, moved into a new park, with its
/// keys and manifest written beside them, checked, and recorded in the state. Steps S2 to
/// S6 of a switch, and of a sign-out.
fn park_out(
    ctx: &Context,
    which: ProviderId,
    root: &Path,
    state: &mut State,
    from_key: &Key,
    outgoing: &TreeIdentity,
    journal: &TreeJournal,
) -> Result<Park> {
    let name = journal
        .from_park
        .clone()
        .expect("a journal with an outgoing account names its park");
    let parks = paths::parks_dir(ctx);
    let dir = parks.join(&name);
    let unwritable = |path: &Path| {
        let path = path.to_path_buf();
        move |source| Error::HomeUnwritable { path, source }
    };
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .map_err(unwritable(&dir))?;
    moves::fsync_dir(&parks).map_err(unwritable(&parks))?;

    let mut moved = Vec::new();
    for item in journal.items.iter().filter(|i| i.from_inode.is_some()) {
        // Asked again before every move: the app may have been opened since the last.
        still_quiet(ctx, which)?;
        move_item(&root.join(&item.path), &dir.join(&item.path))?;
        moved.push(item.path.clone());
        if moved.len() == 1 {
            fault::point("tree.item_parked");
        }
    }
    fault::point("tree.live_parked");

    let keys = config::read_keys(&paths::config_file(root))?;
    write_secret_json(&dir.join(CONFIG_KEYS_FILE), &keys.0)?;
    write_secret_json(
        &dir.join(MANIFEST_FILE),
        &Manifest {
            account_uuid: journal.from_uuid.clone().unwrap_or_default(),
            fingerprint: outgoing.fingerprint.clone(),
            items: moved,
            parked_at: ctx.now(),
        },
    )?;
    fault::point("tree.park_stored");

    // The park is the session that was signed in, item for item, before it is recorded as
    // the account's.
    let parked_session = identity::session_of(ctx, &dir)?;
    let mut whole = parked_session.is_some_and(|s| s.fingerprint == outgoing.fingerprint);
    for item in journal.items.iter().filter(|i| i.from_inode.is_some()) {
        whole &= inode_at(&dir.join(&item.path))? == item.from_inode;
    }
    if !whole {
        return Err(Error::SwitchUnverified {
            tool: which,
            from: state.typed(from_key),
            to: journal
                .to_label
                .as_ref()
                .map(|label| state.typed(&Key::new(which, label.as_str())))
                .unwrap_or_default(),
            detail: "what was parked is not what was signed in".into(),
        });
    }

    note_session(state, from_key, outgoing);
    let park = Park {
        service: name,
        parked_at: ctx.now(),
        refresh_fingerprint: outgoing.fingerprint.clone(),
        access_expires_at: outgoing.expires_at,
        refresh_expires_at: outgoing.expires_at,
    };
    state.park(from_key, park.clone());
    state::save(ctx, state)?;
    fault::point("tree.park_recorded");
    Ok(park)
}

/// Every Claude Desktop park running out within a week, which nothing can renew.
fn expiring(ctx: &Context, state: &State, which: ProviderId) -> Vec<Warning> {
    state
        .accounts
        .iter()
        .filter(|a| a.provider() == which)
        .filter_map(|a| {
            let expires_at = a.parked.as_ref()?.refresh_expires_at?;
            (expires_at < ctx.now() + EXPIRES_SOON).then(|| Warning::ParkExpiresSoon {
                label: state.typed(&a.key()),
                expires_at,
            })
        })
        .collect()
}

/// Switch the app's folder to `key`'s account: the account signed in now is parked, and
/// `key`'s park is moved into its place.
pub(super) fn switch(settled: Settled, key: &Key) -> Result<(Outcome, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    let ctx = &ctx;
    let which = key.provider;
    let target = state
        .get(key)
        .cloned()
        .ok_or_else(|| Error::AccountUnknown {
            label: key.typed(),
            enrolled: state.labels(which),
        })?;

    // S0: nothing written yet.
    let (tree, root) = prepare(ctx, which)?;
    let live = tree.identify(ctx, &root)?;
    let from_key = match identity::whose(&state, live.clone())? {
        LiveOwner::Enrolled(found) if found == *key => {
            // A park of the account in use is an older copy of it, as when enrolling it
            // again: it must not stay beside the login in use, so it goes.
            let old = target.parked.as_ref().map(|p| p.service.clone());
            let mut warnings = Vec::new();
            if old.is_some() || state.active_for(which) != Some(key.label.as_str()) {
                if let Some(old) = &old {
                    state.discard(old);
                }
                if let Some(live) = &live {
                    note_session(&mut state, key, live);
                }
                state.set_active(which, Some(key.label.clone()));
                state.used(key, ctx.now());
                state::save(ctx, &state)?;
            }
            if old.is_some() {
                let pending = purge(ctx, &mut state);
                warnings.extend((pending > 0).then_some(Warning::ParksPendingRemoval(pending)));
            }
            return Ok((
                Outcome::AlreadyActive {
                    label: state.typed(key),
                },
                warnings,
            ));
        }
        LiveOwner::Enrolled(found) => Some(found),
        LiveOwner::Nobody => None,
        LiveOwner::NotEnrolled(_) => {
            return Err(Error::LiveAccountNotEnrolled {
                tool: which,
                email: "the account in Claude".into(),
            });
        }
    };
    let to = state.typed(key);
    let held = target.parked.clone().ok_or_else(|| Error::NothingParked {
        tool: which,
        label: to.clone(),
    })?;
    if !held.restorable_at(ctx.now()) {
        return Err(Error::ParkedLoginExpired { label: to });
    }
    let incoming = check_park(ctx, &to, &target, &held)?;
    let incoming_keys = config::read_keys(&incoming.join(CONFIG_KEYS_FILE))?;
    let from = from_key.as_ref().map(|k| state.typed(k));
    let from_uuid = from_key
        .as_ref()
        .and_then(|k| state.get(k))
        .map(|a| a.account_uuid.clone());

    // S1: the record of intent, naming every item by the inode it has now.
    let mut items = Vec::new();
    for item in tree.items() {
        let from_inode = match from_key {
            Some(_) => inode_at(&root.join(item.path))?,
            None => None,
        };
        items.push(Item {
            path: item.path.to_string(),
            from_inode,
            to_inode: inode_at(&incoming.join(item.path))?,
        });
    }
    let live_keys = config::read_keys(&paths::config_file(&root))?;
    let journal = TreeJournal {
        operation: Operation::Switch,
        from_label: from_key.as_ref().map(|k| k.label.clone()),
        from_uuid: from_uuid.clone(),
        from_fingerprint: live
            .as_ref()
            .filter(|_| from_key.is_some())
            .map(|l| l.fingerprint.clone()),
        to_label: Some(key.label.clone()),
        to_uuid: Some(target.account_uuid.clone()),
        to_fingerprint: Some(held.refresh_fingerprint.clone()),
        from_park: from_uuid.as_deref().map(|uuid| park_name(ctx, uuid)),
        to_park: Some(held.service.clone()),
        items,
        config_keys: ConfigKeyNames {
            from: key_names(&live_keys),
            to: key_names(&incoming_keys),
        },
        ..TreeJournal::new(ctx, which, &root)
    };
    tree_journal::write(ctx, &journal)?;
    fault::point("tree.journal_written");

    // S2 to S6: the outgoing account parked and recorded. After this, a run that dies is
    // finished rather than undone.
    let parked = match (&from_key, &live) {
        (Some(from_key), Some(outgoing)) => Some(park_out(
            ctx, which, &root, &mut state, from_key, outgoing, &journal,
        )?),
        _ => None,
    };

    // S8: the incoming account's items into the folder, and whatever the app made there
    // since the outgoing account left set aside.
    let mut strays = 0;
    // Everything this switch sets aside shares one directory under the strays directory.
    let mut set_aside = moves::Strays::new();
    let mut installed = 0;
    for item in &journal.items {
        still_quiet(ctx, which)?;
        let there = root.join(&item.path);
        if inode_at(&there)?.is_some() {
            set_aside.set_aside(ctx, &there)?;
            strays += 1;
        }
        if item.to_inode.is_some() {
            move_item(&incoming.join(&item.path), &there)?;
            installed += 1;
            if installed == 1 {
                fault::point("tree.item_installed");
            }
        }
    }
    fault::point("tree.installed");

    // S9: the account's keys of the config. The app rewrites its config as it runs, so it
    // is asked once more, as before every move. A folder nobody was signed in to can still
    // hold keys the app wrote since, which no park holds, so they are set aside with the
    // strays rather than written over.
    still_quiet(ctx, which)?;
    if from_key.is_none() {
        let left = config::read_keys(&paths::config_file(&root))?;
        if !left.0.is_empty() && left != incoming_keys {
            let slot = set_aside.dir(ctx)?;
            write_secret_json(&slot.join(CONFIG_KEYS_FILE), &left.0)?;
            strays += 1;
        }
    }
    config::splice(&paths::config_file(&root), &incoming_keys)?;
    fault::point("tree.config_spliced");

    // S10: the folder is the incoming account's. When it cannot be told, the record of
    // intent stays, and the next run finishes this.
    let unverified = |detail: String| Error::SwitchUnverified {
        tool: which,
        from: from.clone().unwrap_or_default(),
        to: to.clone(),
        detail,
    };
    let now_live = match tree.identify(ctx, &root) {
        Ok(Some(found))
            if found.account_uuid == target.account_uuid
                && found.fingerprint == held.refresh_fingerprint =>
        {
            found
        }
        Ok(_) => {
            return Err(unverified(
                "the data folder does not hold the session that was parked".into(),
            ));
        }
        Err(e) => return Err(unverified(e.to_string())),
    };

    // S11: recorded.
    state.set_active(which, Some(key.label.clone()));
    state.discard(&held.service);
    state.used(key, ctx.now());
    note_session(&mut state, key, &now_live);
    state::save(ctx, &state)?;
    fault::point("tree.recorded");

    // S12: done, and the incoming park's leftovers deleted. Somebody is signed in again, so
    // a sign-out waiting for one is over.
    tree_journal::clear(ctx);
    clear_awaiting(ctx);
    let pending = purge(ctx, &mut state);

    let warnings = (strays > 0)
        .then(|| Warning::StraysKept {
            path: paths::strays_dir(ctx),
            count: strays,
        })
        .into_iter()
        .chain(expiring(ctx, &state, which))
        .chain((pending > 0).then_some(Warning::ParksPendingRemoval(pending)))
        .collect();
    let adoption = provider::of(which).adoption();
    let outcome = match (from, parked) {
        (Some(from), Some(parked)) => Outcome::Switched {
            provider: which,
            from,
            to,
            parked,
            adoption,
        },
        _ => Outcome::Installed {
            provider: which,
            to,
            adoption,
        },
    };
    Ok((outcome, warnings))
}

/// Park the account signed in to the app and leave it signed out, so another account can be
/// signed in to there and enrolled, without signing out in the app, which may end the
/// session for good.
pub fn sign_out(settled: Settled, which: ProviderId) -> Result<(Outcome, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    let ctx = &ctx;
    let (tree, root) = prepare(ctx, which)?;
    let live = tree.identify(ctx, &root)?;
    let from_key = match identity::whose(&state, live.clone())? {
        LiveOwner::Nobody => {
            return Ok((Outcome::AlreadySignedOut { provider: which }, Vec::new()));
        }
        LiveOwner::NotEnrolled(_) => {
            return Err(Error::LiveAccountNotEnrolled {
                tool: which,
                email: "the account in Claude".into(),
            });
        }
        LiveOwner::Enrolled(found) => found,
    };
    let outgoing = live.expect("an enrolled owner is somebody signed in");
    let from = state.typed(&from_key);

    let mut items = Vec::new();
    for item in tree.items() {
        items.push(Item {
            path: item.path.to_string(),
            from_inode: inode_at(&root.join(item.path))?,
            to_inode: None,
        });
    }
    let live_keys = config::read_keys(&paths::config_file(&root))?;
    let journal = TreeJournal {
        operation: Operation::SignOut,
        from_label: Some(from_key.label.clone()),
        from_uuid: Some(outgoing.account_uuid.clone()),
        from_fingerprint: Some(outgoing.fingerprint.clone()),
        from_park: Some(park_name(ctx, &outgoing.account_uuid)),
        items,
        config_keys: ConfigKeyNames {
            from: key_names(&live_keys),
            to: Vec::new(),
        },
        ..TreeJournal::new(ctx, which, &root)
    };
    tree_journal::write(ctx, &journal)?;
    fault::point("tree.journal_written");

    let parked = park_out(
        ctx, which, &root, &mut state, &from_key, &outgoing, &journal,
    )?;
    still_quiet(ctx, which)?;
    config::splice(&paths::config_file(&root), &ConfigKeys::default())?;
    fault::point("tree.config_spliced");
    match tree.identify(ctx, &root) {
        Ok(None) => {}
        Ok(Some(_)) => {
            return Err(Error::SwitchUnverified {
                tool: which,
                from,
                to: String::new(),
                detail: "the data folder still holds a session".into(),
            });
        }
        Err(e) => {
            return Err(Error::SwitchUnverified {
                tool: which,
                from,
                to: String::new(),
                detail: e.to_string(),
            });
        }
    }

    state.set_active(which, None);
    state::save(ctx, &state)?;
    fault::point("tree.recorded");
    write_awaiting(
        ctx,
        &Awaiting {
            from_label: from_key.label.clone(),
            started_at: ctx.now(),
        },
    )?;
    tree_journal::clear(ctx);
    let pending = purge(ctx, &mut state);
    let warnings = (pending > 0)
        .then_some(Warning::ParksPendingRemoval(pending))
        .into_iter()
        .collect();
    Ok((
        Outcome::SignedOut {
            provider: which,
            from,
            parked,
            adoption: provider::of(which).adoption(),
        },
        warnings,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::harness::{APP_PATH, DesktopMachine, NOW, desktop_machine, jar};
    use super::*;

    /// A refusal after the record of intent was written, which must not say that nothing
    /// moved: something did, and the next run finishes or undoes it.
    fn assert_stopped_partway(refused: &Error) {
        assert_eq!(refused.code(), "app_opened_midway", "{refused:?}");
        assert_eq!(refused.exit_code(), 3);
        let said = refused.to_string();
        assert!(!said.contains("nothing was moved"), "{said}");
        assert!(said.contains("partway"), "{said}");
    }

    fn here(m: &DesktopMachine) -> Key {
        m.key("here")
    }

    fn there(m: &DesktopMachine) -> Key {
        m.key("there")
    }

    fn switch_to(m: &DesktopMachine, label: &str) -> Result<(Outcome, Vec<Warning>)> {
        let (settled, _) = super::super::settle(&m.ctx, Some(ProviderId::Desktop))?;
        super::super::switch(settled, &m.key(label))
    }

    fn signing_out(m: &DesktopMachine) -> Result<(Outcome, Vec<Warning>)> {
        let (settled, _) = super::super::settle(&m.ctx, Some(ProviderId::Desktop))?;
        sign_out(settled, ProviderId::Desktop)
    }

    fn enrolling(m: &DesktopMachine, label: &str) -> Result<(Enrolled, Vec<Warning>)> {
        let (settled, _) = super::super::settle(&m.ctx, Some(ProviderId::Desktop))?;
        super::super::enroll(settled, &m.key(label), None)
    }

    #[test]
    fn a_switch_moves_only_the_allowlist() {
        let m = desktop_machine("allowlist");
        let support = m.support();
        let config_before: serde_json::Value =
            serde_json::from_slice(&std::fs::read(support.join("config.json")).unwrap()).unwrap();
        let untouched = [
            "claude_desktop_config.json",
            "plan-usage-history.json",
            "Cache/data_0",
            "IndexedDB/https_other.example_0.indexeddb.leveldb/000003.log",
        ];
        // Dated well before the switch, so a rewrite in it shows however coarse the clock.
        let long_ago = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for path in untouched {
            std::fs::File::options()
                .write(true)
                .open(support.join(path))
                .unwrap()
                .set_modified(long_ago)
                .unwrap();
        }
        let seen = |path: &str| {
            let at = support.join(path);
            (
                moves::inode(&at).unwrap(),
                std::fs::metadata(&at).unwrap().modified().unwrap(),
                std::fs::read(&at).unwrap(),
            )
        };
        let before: Vec<_> = untouched.iter().map(|p| seen(p)).collect();

        let (outcome, warnings) = switch_to(&m, "there").expect("the switch");
        assert!(
            matches!(&outcome, Outcome::Switched { from, to, .. } if from == "desktop/here" && to == "desktop/there"),
            "{outcome:?}"
        );
        assert!(warnings.is_empty(), "{warnings:?}");

        for (path, before) in untouched.iter().zip(before) {
            assert_eq!(
                seen(path),
                before,
                "{path} belongs to the machine: same file, same contents, never written"
            );
        }
        let config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(support.join("config.json")).unwrap()).unwrap();
        assert_eq!(config["lastKnownAccountUuid"], "there");
        assert_eq!(config["oauth:tokenCache"], "cache-there");
        assert_eq!(config["locale"], config_before["locale"]);

        let found = identity::identify_tree(&m.ctx, &support).unwrap().unwrap();
        assert_eq!(found.account_uuid, "there");
        let state = state::load(&m.ctx).unwrap();
        assert_eq!(state.active_for(ProviderId::Desktop), Some("there"));
        assert!(state.get(&there(&m)).unwrap().parked.is_none());
        let park = state.get(&here(&m)).unwrap().parked.clone().unwrap();
        let dir = paths::parks_dir(&m.ctx).join(&park.service);
        for item in [
            "Cookies",
            "Cookies-journal",
            "Local Storage",
            "Session Storage",
            "IndexedDB/https_claude.ai_0.indexeddb.leveldb",
            "WebStorage",
        ] {
            assert!(dir.join(item).exists(), "{item} is parked");
        }
        let keys = config::read_keys(&dir.join(CONFIG_KEYS_FILE)).unwrap();
        assert_eq!(keys.0["lastKnownAccountUuid"], "here");
        assert!(
            !paths::parks_dir(&m.ctx).join(m.there_park()).exists(),
            "the installed park is gone"
        );
        assert!(tree_journal::pending(&m.ctx).is_none());
    }

    #[test]
    fn nothing_moves_while_claude_runs() {
        let m = desktop_machine("runs");
        let before = m.inodes();
        let pid = m.mem.runs_within(APP_PATH);
        let refused = switch_to(&m, "there").expect_err("Claude is open");
        assert!(
            matches!(&refused, Error::AppStillOpen { pids, detail: None, .. } if *pids == vec![pid]),
            "{refused:?}"
        );
        assert_eq!(refused.code(), "app_still_open");
        assert!(
            refused.to_string().contains("nothing was moved"),
            "{refused}"
        );
        assert_eq!(m.inodes(), before);
        assert!(tree_journal::pending(&m.ctx).is_none());
    }

    #[test]
    fn nothing_moves_when_the_process_list_cannot_be_read() {
        let m = desktop_machine("no-list");
        let before = m.inodes();
        m.mem.process_list_fails();
        let refused = switch_to(&m, "there").expect_err("counted as open");
        assert!(
            matches!(
                &refused,
                Error::AppStillOpen {
                    detail: Some(_),
                    ..
                }
            ),
            "{refused:?}"
        );
        assert_eq!(m.inodes(), before);
        // Not knowing is not the same as Claude being open, and the app says it apart.
        assert_eq!(refused.code(), "app_state_unknown");
        assert!(
            refused
                .to_string()
                .contains("could not list what is running"),
            "{refused}"
        );
    }

    #[test]
    fn claude_starting_midway_stops_the_next_move() {
        let m = desktop_machine("midway");
        let before = m.inodes();
        let mem = std::sync::Arc::clone(&m.mem);
        let refused = fault::meanwhile(
            "tree.item_parked",
            move || {
                mem.runs_within(APP_PATH);
            },
            || switch_to(&m, "there"),
        )
        .expect_err("the app opened partway");
        assert!(matches!(refused, Error::AppStillOpen { .. }), "{refused:?}");
        assert_stopped_partway(&refused);
        // One item moved, and the record of intent says so.
        assert!(tree_journal::pending(&m.ctx).is_some());
        let waiting = super::super::settle(&m.ctx, Some(ProviderId::Desktop))
            .err()
            .expect("recovery waits while the app is open");
        assert!(
            matches!(waiting, Error::RecoveryWaiting { .. }),
            "{waiting:?}"
        );

        m.mem.quits_within();
        let recovered = m
            .recover()
            .expect("recovered")
            .expect("the interrupted switch was found");
        assert!(!recovered.finished, "undone, not finished");
        assert_eq!(m.inodes(), before, "every item is back where it was");
        assert_eq!(m.whole("here"), super::super::harness::Whole::Live);
        assert_eq!(m.whole("there"), super::super::harness::Whole::Parked);
    }

    /// Claude opened after the last item moved and before the config is written: the
    /// config is the app's while it runs, so it is left alone, and the run quit for. The
    /// next run once Claude is quit finishes the switch.
    #[test]
    fn claude_starting_before_the_config_is_written_leaves_it_alone() {
        let m = desktop_machine("before-config");
        let config = m.support().join("config.json");
        let before = std::fs::read(&config).unwrap();
        let mem = std::sync::Arc::clone(&m.mem);
        let refused = fault::meanwhile(
            "tree.installed",
            move || {
                mem.runs_within(APP_PATH);
            },
            || switch_to(&m, "there"),
        )
        .expect_err("the app opened before the config");
        assert!(matches!(refused, Error::AppStillOpen { .. }), "{refused:?}");
        assert_stopped_partway(&refused);
        assert_eq!(
            std::fs::read(&config).unwrap(),
            before,
            "the config is untouched"
        );
        assert!(tree_journal::pending(&m.ctx).is_some());

        m.mem.quits_within();
        let recovered = m
            .recover()
            .expect("recovered")
            .expect("the interrupted switch was found");
        assert!(recovered.finished, "finished, not undone");
        assert_eq!(m.whole("there"), super::super::harness::Whole::Live);
        assert_eq!(m.whole("here"), super::super::harness::Whole::Parked);
        let spliced: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
        assert_eq!(spliced["lastKnownAccountUuid"], "there");
        assert_eq!(spliced["locale"], "en-US");
    }

    /// The same for a sign-out: Claude opened once the account is parked, so the config
    /// still names it, and the next run once Claude is quit finishes the sign-out.
    #[test]
    fn claude_starting_before_a_sign_out_writes_the_config_leaves_it_alone() {
        let m = desktop_machine("sign-out-before-config");
        let config = m.support().join("config.json");
        let before = std::fs::read(&config).unwrap();
        let mem = std::sync::Arc::clone(&m.mem);
        let refused = fault::meanwhile(
            "tree.park_recorded",
            move || {
                mem.runs_within(APP_PATH);
            },
            || signing_out(&m),
        )
        .expect_err("the app opened before the config");
        assert!(matches!(refused, Error::AppStillOpen { .. }), "{refused:?}");
        assert_stopped_partway(&refused);
        assert_eq!(
            std::fs::read(&config).unwrap(),
            before,
            "the config is untouched"
        );
        assert!(tree_journal::pending(&m.ctx).is_some());

        m.mem.quits_within();
        let recovered = m
            .recover()
            .expect("recovered")
            .expect("the interrupted sign-out was found");
        assert!(recovered.finished && recovered.signed_out, "{recovered:?}");
        assert_eq!(identity::identify_tree(&m.ctx, &m.support()).unwrap(), None);
        assert_eq!(m.whole("here"), super::super::harness::Whole::Parked);
        let spliced: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
        for key in paths::CONFIG_KEYS {
            assert!(spliced.get(key).is_none(), "{key} is the account's");
        }
        assert_eq!(spliced["locale"], "en-US");
    }

    #[test]
    fn sign_out_leaves_claude_signed_out_and_the_account_parked() {
        let m = desktop_machine("sign-out");
        let (outcome, _) = signing_out(&m).expect("signed out");
        let Outcome::SignedOut { from, parked, .. } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(from, "desktop/here");
        assert_eq!(identity::identify_tree(&m.ctx, &m.support()).unwrap(), None);
        let config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(m.support().join("config.json")).unwrap())
                .unwrap();
        for key in paths::CONFIG_KEYS {
            assert!(config.get(key).is_none(), "{key} is the account's");
        }
        assert_eq!(config["locale"], "en-US");
        let state = state::load(&m.ctx).unwrap();
        assert_eq!(state.active_for(ProviderId::Desktop), None);
        assert_eq!(state.get(&here(&m)).unwrap().parked.as_ref(), Some(&parked));
        assert_eq!(
            awaiting_sign_in(&m.ctx).map(|a| a.from_label),
            Some("here".to_string())
        );
        assert_eq!(m.whole("here"), super::super::harness::Whole::Parked);

        // Signed out already is nothing to do.
        let (again, _) = signing_out(&m).expect("nothing to do");
        assert!(
            matches!(
                again,
                Outcome::AlreadySignedOut {
                    provider: ProviderId::Desktop
                }
            ),
            "{again:?}"
        );

        // And a switch from signed out installs without parking anything.
        let (installed, _) = switch_to(&m, "there").expect("installed");
        assert!(
            matches!(&installed, Outcome::Installed { to, .. } if to == "desktop/there"),
            "{installed:?}"
        );
        assert_eq!(m.whole("there"), super::super::harness::Whole::Live);
        assert_eq!(m.whole("here"), super::super::harness::Whole::Parked);
    }

    #[test]
    fn enroll_refuses_while_claude_runs() {
        let m = desktop_machine("enroll-runs");
        m.mem.runs_within(APP_PATH);
        let refused = enrolling(&m, "here").expect_err("Claude is open");
        assert!(matches!(refused, Error::AppStillOpen { .. }), "{refused:?}");
    }

    #[test]
    fn enroll_records_uuid_and_fingerprint() {
        let m = desktop_machine("enroll-new");
        // Somebody signed in to a third account in the app.
        let support = m.support();
        m.plant_live("third", "v10third");
        let (enrolled, warnings) = enrolling(&m, "third").expect("enrolled");
        assert!(matches!(enrolled, Enrolled::Current { .. }), "{enrolled:?}");
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, Warning::ReplacedOutsidePitboard { label, .. } if label == "desktop/here")),
            "{warnings:?}"
        );
        let state = state::load(&m.ctx).unwrap();
        let account = state.get(&m.key("third")).unwrap();
        assert_eq!(account.account_uuid, "third");
        let live = identity::identify_tree(&m.ctx, &support).unwrap().unwrap();
        assert!(matches!(
            &account.detail,
            Detail::Desktop { session_fingerprint, .. } if *session_fingerprint == live.fingerprint
        ));
        assert_eq!(state.active_for(ProviderId::Desktop), Some("third"));

        // The same account under another label is refused, and so is a label in use.
        let refused = enrolling(&m, "again").expect_err("enrolled already");
        assert!(
            matches!(refused, Error::AlreadyEnrolled { .. }),
            "{refused:?}"
        );
        m.plant_live("fourth", "v10fourth");
        let refused = enrolling(&m, "there").expect_err("a label in use");
        assert!(matches!(refused, Error::LabelTaken { .. }), "{refused:?}");
    }

    #[test]
    fn an_unconfirmed_session_moves_nothing() {
        let m = desktop_machine("unconfirmed");
        // `there`'s parked session in the folder under `here`'s uuid: the uuid is stale or
        // the session moved, and either way it cannot be filed under one of them.
        m.plant_live("here", "v10there");
        let before = m.inodes();
        let refused = switch_to(&m, "there").expect_err("unconfirmed");
        assert!(
            matches!(&refused, Error::DesktopIdentityUnconfirmed { label } if label == "there"),
            "{refused:?}"
        );
        assert_eq!(m.inodes(), before);
        assert!(tree_journal::pending(&m.ctx).is_none());
    }

    /// A session Pitboard has not seen under `here`'s uuid may be another account signed in
    /// after Log out left `here`'s uuid behind (E1b): experiment E4 saw the uuid written only
    /// where none was named. Until that is measured nothing moves, and the refusal names
    /// `here`, whose enrolling confirms it.
    #[test]
    fn a_new_session_under_a_known_uuid_moves_nothing() {
        let m = desktop_machine("resigned");
        m.plant_live("here", "v10here-again");
        let before = m.inodes();
        let refused = switch_to(&m, "there").expect_err("unconfirmed");
        assert!(
            matches!(&refused, Error::DesktopIdentityUnconfirmed { label } if label == "here"),
            "{refused:?}"
        );
        assert_eq!(m.inodes(), before);
        assert!(tree_journal::pending(&m.ctx).is_none());
    }

    /// Switching to the account signed in now while it still has a park: the park is an
    /// older copy of it, and must not stay beside the login in use, as enrolling it again
    /// also drops it. The session in use is the one recorded.
    #[test]
    fn switching_to_the_account_in_use_drops_its_park() {
        let m = desktop_machine("in-use-parked");
        let park = m.there_park();
        // `there`'s parked session, signed in to the app as well.
        m.plant_live("there", "v10there");
        let live = identity::identify_tree(&m.ctx, &m.support())
            .expect("a readable folder")
            .expect("signed in");
        let (outcome, _) = switch_to(&m, "there").expect("already in use");
        assert!(
            matches!(&outcome, Outcome::AlreadyActive { label } if label == "desktop/there"),
            "{outcome:?}"
        );
        let state = state::load(&m.ctx).unwrap();
        assert_eq!(state.active_for(ProviderId::Desktop), Some("there"));
        let account = state.get(&there(&m)).expect("there is kept");
        assert_eq!(account.parked, None);
        assert!(
            matches!(
                &account.detail,
                Detail::Desktop { session_fingerprint, .. } if *session_fingerprint == live.fingerprint
            ),
            "{:?}",
            account.detail
        );
        assert!(
            !crate::provider::desktop::paths::parks_dir(&m.ctx)
                .join(&park)
                .exists(),
            "the old park is deleted"
        );
        assert!(expiring(&m.ctx, &state, ProviderId::Desktop).is_empty());
    }

    #[test]
    fn enrolling_the_same_label_confirms_it() {
        let m = desktop_machine("confirm");
        m.plant_live("here", "v10someone");
        let (enrolled, _) = enrolling(&m, "here").expect("confirmed");
        assert!(
            matches!(enrolled, Enrolled::InUse { again: true, .. }),
            "{enrolled:?}"
        );
        let (outcome, _) = switch_to(&m, "there").expect("now it moves");
        assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
    }

    /// `there`'s park lapsed, so somebody put `here` aside, signed in to `there` again in
    /// Claude, and enrolled it under its own label. The lapsed park is not kept beside the
    /// login now in use: nothing could install it, and it would be warned about forever.
    #[test]
    fn enrolling_again_over_a_lapsed_park_drops_the_park() {
        let m = desktop_machine("re-enroll-lapsed");
        let mut state = state::load(&m.ctx).unwrap();
        let mut account = state.get(&there(&m)).unwrap().clone();
        account.parked.as_mut().unwrap().refresh_expires_at = Some(NOW - 1);
        state.upsert(account);
        state::save(&m.ctx, &state).unwrap();
        signing_out(&m).expect("signed out");
        m.plant_live("there", "v10there-again");

        let (enrolled, _) = enrolling(&m, "there").expect("enrolled again");
        assert!(
            matches!(enrolled, Enrolled::InUse { again: true, .. }),
            "{enrolled:?}"
        );
        let state = state::load(&m.ctx).unwrap();
        assert_eq!(state.active_for(ProviderId::Desktop), Some("there"));
        assert_eq!(state.get(&there(&m)).unwrap().parked, None);
        assert!(expiring(&m.ctx, &state, ProviderId::Desktop).is_empty());
        assert!(state.discarded.is_empty(), "{:?}", state.discarded);
        assert!(
            !paths::parks_dir(&m.ctx).join(m.there_park()).exists(),
            "the lapsed park is deleted"
        );
        assert!(
            state.get(&here(&m)).unwrap().parked.is_some(),
            "here's is kept"
        );
    }

    #[test]
    fn what_claude_made_while_signed_out_goes_to_strays() {
        let m = desktop_machine("strays");
        signing_out(&m).expect("signed out");
        // The app opened signed out and made storage of its own.
        let made = m.support().join("Local Storage");
        std::fs::create_dir_all(&made).unwrap();
        std::fs::write(made.join("made-by-claude"), b"x").unwrap();
        let made_inode = moves::inode(&made.join("made-by-claude")).unwrap().unwrap();

        let (_, warnings) = switch_to(&m, "there").expect("installed");
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, Warning::StraysKept { count: 1, .. })),
            "{warnings:?}"
        );
        assert!(m.strays_hold(made_inode), "set aside, never deleted");
        assert_eq!(m.whole("there"), super::super::harness::Whole::Live);
    }

    /// Everything one switch sets aside goes into one directory under the strays directory,
    /// each item at its place in the data folder, rather than a directory per item: the lab
    /// run of 4 October 2026 found seven for one switch.
    #[test]
    fn one_switch_sets_its_strays_aside_in_one_directory() {
        let m = desktop_machine("strays-one-slot");
        signing_out(&m).expect("signed out");
        // The app opened signed out, made storage of its own and wrote keys to its config.
        let mut made = Vec::new();
        for item in ["Local Storage", "Session Storage", "WebStorage"] {
            let dir = m.support().join(item);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("made-by-claude"), b"x").unwrap();
            made.push(moves::inode(&dir.join("made-by-claude")).unwrap().unwrap());
        }
        let path = m.support().join("config.json");
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        config["oauth:tokenCacheV2"] = serde_json::json!("cache-written-signed-out");
        std::fs::write(&path, config.to_string()).unwrap();

        let (_, warnings) = switch_to(&m, "there").expect("installed");
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, Warning::StraysKept { count: 4, .. })),
            "{warnings:?}"
        );
        let slots: Vec<_> = std::fs::read_dir(paths::strays_dir(&m.ctx))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(slots.len(), 1, "{slots:?}");
        let slot = &slots[0];
        for item in ["Local Storage", "Session Storage", "WebStorage"] {
            assert!(slot.join(item).join("made-by-claude").is_file(), "{item}");
        }
        assert!(slot.join(CONFIG_KEYS_FILE).is_file());
        for inode in made {
            assert!(m.strays_hold(inode), "set aside, never deleted");
        }
        assert_eq!(m.whole("there"), super::super::harness::Whole::Live);
    }

    /// Claude can write an account's keys into its config while Pitboard counts nobody as
    /// signed in. A switch writes the incoming account's keys over them, so they are set
    /// aside with the strays first, never lost.
    #[test]
    fn config_keys_claude_wrote_while_signed_out_are_set_aside() {
        let m = desktop_machine("strays-config");
        signing_out(&m).expect("signed out");
        let path = m.support().join("config.json");
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        config["oauth:tokenCacheV2"] = serde_json::json!("cache-written-signed-out");
        std::fs::write(&path, config.to_string()).unwrap();

        let (_, warnings) = switch_to(&m, "there").expect("installed");
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, Warning::StraysKept { count: 1, .. })),
            "{warnings:?}"
        );
        let mut kept = Vec::new();
        super::super::harness::files_under(&paths::strays_dir(&m.ctx), &mut kept);
        let saved = kept
            .iter()
            .filter(|p| p.file_name().is_some_and(|n| n == CONFIG_KEYS_FILE))
            .map(|p| config::read_keys(p).unwrap())
            .find(|keys| keys.0.contains_key("oauth:tokenCacheV2"))
            .expect("the keys are set aside");
        assert_eq!(
            saved.0["oauth:tokenCacheV2"],
            serde_json::json!("cache-written-signed-out")
        );
        assert_eq!(m.whole("there"), super::super::harness::Whole::Live);

        // A folder that holds no keys of an account has nothing to set aside.
        let m = desktop_machine("strays-no-config");
        signing_out(&m).expect("signed out");
        let (_, warnings) = switch_to(&m, "there").expect("installed");
        assert!(
            !warnings
                .iter()
                .any(|w| matches!(w, Warning::StraysKept { .. })),
            "{warnings:?}"
        );
    }

    #[test]
    fn an_expired_park_is_not_installed() {
        let m = desktop_machine("expired");
        let mut state = state::load(&m.ctx).unwrap();
        let mut account = state.get(&there(&m)).unwrap().clone();
        account.parked.as_mut().unwrap().refresh_expires_at = Some(NOW - 1);
        state.upsert(account);
        state::save(&m.ctx, &state).unwrap();
        let before = m.inodes();
        let refused = switch_to(&m, "there").expect_err("expired");
        assert!(
            matches!(refused, Error::ParkedLoginExpired { .. }),
            "{refused:?}"
        );
        assert_eq!(m.inodes(), before);
    }

    #[test]
    fn a_park_naming_another_account_is_refused() {
        let m = desktop_machine("elsewhere");
        let dir = paths::parks_dir(&m.ctx).join(m.there_park());
        let mut manifest: Manifest =
            serde_json::from_slice(&std::fs::read(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
        manifest.account_uuid = "someone-else".into();
        write_secret_json(&dir.join(MANIFEST_FILE), &manifest).unwrap();
        let before = m.inodes();
        let refused = switch_to(&m, "there").expect_err("not there's");
        assert!(
            matches!(refused, Error::ParkedLoginBelongsElsewhere { .. }),
            "{refused:?}"
        );
        assert_eq!(m.inodes(), before);

        // A session that is not the one parked is refused too.
        let m = desktop_machine("elsewhere-session");
        let dir = paths::parks_dir(&m.ctx).join(m.there_park());
        m.mem
            .plant_cookies(&dir.join("Cookies"), jar("v10other", NOW + 86_400));
        let refused = switch_to(&m, "there").expect_err("another session");
        assert!(
            matches!(refused, Error::ParkedCredentialCorrupt { .. }),
            "{refused:?}"
        );
    }

    #[test]
    fn two_tools_with_one_uuid_keep_their_parks_apart() {
        let m = desktop_machine("two-tools");
        // Claude Code's account of the same uuid, parked in the vault.
        let service = crate::park::reserve(&m.ctx, "there").unwrap();
        let parked = crate::park::store_at(
            &m.ctx,
            ProviderId::Claude,
            &service,
            &super::super::harness::oauth("code-refresh", 30),
        )
        .unwrap();
        let mut state = state::load(&m.ctx).unwrap();
        state.accounts.push(super::super::harness::account(
            "there",
            "there",
            Some(parked.clone()),
        ));
        state::save(&m.ctx, &state).unwrap();

        switch_to(&m, "there").expect("the Desktop switch");
        let state = state::load(&m.ctx).unwrap();
        assert_eq!(
            state
                .get(&Key::new(ProviderId::Claude, "there"))
                .unwrap()
                .parked
                .as_ref(),
            Some(&parked),
            "Claude Code's park is not Claude Desktop's"
        );
        assert!(m.mem.vault().peek(&service).is_some());
        assert_ne!(service, m.there_park());

        // A Claude Code login of the same uuid that a run wrote down and never recorded. The
        // sweep gives it back to Claude Code's account, never to Claude Desktop's.
        let m = desktop_machine("two-tools-adopt");
        let mut state = state::load(&m.ctx).unwrap();
        state
            .accounts
            .push(super::super::harness::account("there", "there", None));
        state::save(&m.ctx, &state).unwrap();
        let service = crate::park::reserve(&m.ctx, "there").unwrap();
        crate::park::store_at(
            &m.ctx,
            ProviderId::Claude,
            &service,
            &super::super::harness::oauth("orphan-refresh", 30),
        )
        .unwrap();
        super::super::settle(&m.ctx, None).expect("swept");
        let state = state::load(&m.ctx).unwrap();
        let held = |key: &Key| {
            state
                .get(key)
                .and_then(|a| a.parked.as_ref())
                .map(|p| p.service.clone())
        };
        assert_eq!(held(&Key::new(ProviderId::Claude, "there")), Some(service));
        assert_eq!(held(&there(&m)), Some(m.there_park()));
        // The lookup both the sweep and doctor's check of held-back accounts go through.
        // Doctor names a held-back account by its service, and both tools' is Anthropic, so
        // which account the lookup answers is the whole of what can go wrong there.
        assert_eq!(
            state
                .vault_owner_of_park("there")
                .map(crate::state::Account::provider),
            Some(ProviderId::Claude)
        );
    }

    #[test]
    fn vault_code_never_sees_a_tree_name() {
        let m = desktop_machine("vault");
        m.mem
            .vault()
            .plant(&m.there_park(), "a vault item of the same name");

        // Neither the list of names Pitboard writes down nor asking the store itself takes
        // the folder park for a vault item, or the vault item for a park of an account's.
        let before = state::load(&m.ctx).unwrap();
        super::super::settle(&m.ctx, None).expect("swept");
        let (settled, _) = super::super::settle(&m.ctx, None).unwrap();
        super::super::repair(settled).expect("repaired");
        assert!(m.mem.vault().peek(&m.there_park()).is_some());
        assert!(paths::parks_dir(&m.ctx).join(m.there_park()).is_dir());
        let after = state::load(&m.ctx).unwrap();
        let parks = |state: &State| {
            state
                .accounts
                .iter()
                .map(|a| (a.key(), a.parked.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(parks(&after), parks(&before), "nobody's park changed hands");
        assert!(after.discarded.is_empty(), "nothing listed for deletion");

        switch_to(&m, "there").expect("switched");
        assert!(
            m.mem.vault().peek(&m.there_park()).is_some(),
            "a tree park's name is deleted from the parks directory, never the vault"
        );
        assert!(!paths::parks_dir(&m.ctx).join(m.there_park()).exists());
    }
}
