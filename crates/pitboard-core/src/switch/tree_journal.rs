//! Finishing or undoing what an interrupted move of a folder login started.
//!
//! A tree switch writes this record before it moves anything, naming every item it may
//! move by the inode it has on each side. Which way recovery goes is decided by the state
//! alone: until the outgoing account's park is recorded there, the run is undone, item for
//! item back where it was; once it is, the run is finished. Every item is judged before any
//! is moved, by its inode, so an item found where neither side put it stops recovery with
//! nothing changed, and is never guessed at. Whatever the app made in the meantime is set
//! aside in the strays directory, never deleted, and recovery waits while the app is open.

use super::tree::{self, CONFIG_KEYS_FILE, MANIFEST_FILE};
use super::{Abandoned, Error, Recovered, Result};
use crate::atomic;
use crate::context::Context;
use crate::provider::ProviderId;
use crate::provider::desktop::config::{self, ConfigKeys};
use crate::provider::desktop::paths;
use crate::service::Warning;
use crate::state::{self, Key, State};
use crate::store::tree as moves;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// What kind of record this is, so one is never read as another's.
const KIND: &str = "tree";

/// The form of the record this Pitboard writes.
const VERSION: u32 = 1;

/// What the interrupted run was doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Operation {
    /// One account parked, if one was signed in, and another installed.
    Switch,
    /// The account signed in parked, and the app left signed out.
    SignOut,
}

/// One item the run may move, by its inode on each side. `None` where that side had none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Item {
    pub(super) path: String,
    pub(super) from_inode: Option<u64>,
    pub(super) to_inode: Option<u64>,
}

/// The names of the keys of `config.json` on each side. Names only: the values are the
/// account's, and stay in its park.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ConfigKeyNames {
    pub(super) from: Vec<String>,
    pub(super) to: Vec<String>,
}

/// The record of intent of a tree switch or sign-out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct TreeJournal {
    pub(super) kind: String,
    pub(super) version: u32,
    pub(super) provider: ProviderId,
    pub(super) operation: Operation,
    pub(super) started_at: i64,
    /// The data folder the run moved items of. Recovered only against that same folder.
    pub(super) support_dir: PathBuf,
    /// The volume it was on.
    pub(super) device: Option<u64>,
    pub(super) from_label: Option<String>,
    pub(super) from_uuid: Option<String>,
    pub(super) from_fingerprint: Option<String>,
    pub(super) to_label: Option<String>,
    pub(super) to_uuid: Option<String>,
    pub(super) to_fingerprint: Option<String>,
    /// The park the outgoing account's items go into, made by the run.
    pub(super) from_park: Option<String>,
    /// The park the incoming account's items come from.
    pub(super) to_park: Option<String>,
    pub(super) items: Vec<Item>,
    pub(super) config_keys: ConfigKeyNames,
}

impl TreeJournal {
    /// A record of nothing yet, for the data folder `root` of `which`.
    pub(super) fn new(ctx: &Context, which: ProviderId, root: &Path) -> TreeJournal {
        TreeJournal {
            kind: KIND.into(),
            version: VERSION,
            provider: which,
            operation: Operation::Switch,
            started_at: ctx.now(),
            support_dir: root.to_path_buf(),
            device: ctx.host().device_of(root).ok(),
            from_label: None,
            from_uuid: None,
            from_fingerprint: None,
            to_label: None,
            to_uuid: None,
            to_fingerprint: None,
            from_park: None,
            to_park: None,
            items: Vec::new(),
            config_keys: ConfigKeyNames::default(),
        }
    }

    fn side(&self, label: &Option<String>) -> Option<Key> {
        label
            .as_ref()
            .map(|label| Key::new(self.provider, label.as_str()))
    }

    /// The two sides, as a sentence names them; empty for a side there is none of.
    fn named(&self, state: &State) -> (String, String) {
        let name = |key: Option<Key>| key.map(|k| state.typed(&k)).unwrap_or_default();
        (
            name(self.side(&self.from_label)),
            name(self.side(&self.to_label)),
        )
    }

    /// Everything a record must say for recovery to act on it. A record written by this
    /// Pitboard always does; one that does not was changed afterwards.
    fn check(&self) -> std::result::Result<(), String> {
        if self.kind != KIND || self.version != VERSION {
            return Err(format!(
                "it is a {} record of version {}",
                self.kind, self.version
            ));
        }
        let tree = crate::provider::of(self.provider)
            .tree()
            .ok_or("it names a tool whose login is not a folder")?;
        let from = [
            self.from_label.is_some(),
            self.from_uuid.is_some(),
            self.from_fingerprint.is_some(),
            self.from_park.is_some(),
        ];
        if from.iter().any(|&b| b) && !from.iter().all(|&b| b) {
            return Err("it names part of an outgoing account".into());
        }
        let to = [
            self.to_label.is_some(),
            self.to_uuid.is_some(),
            self.to_fingerprint.is_some(),
            self.to_park.is_some(),
        ];
        match self.operation {
            Operation::SignOut if to.iter().any(|&b| b) || !from[0] => {
                return Err("a sign-out names an incoming account, or no outgoing one".into());
            }
            Operation::Switch if !to.iter().all(|&b| b) => {
                return Err("a switch names no incoming account".into());
            }
            _ => {}
        }
        for park in [&self.from_park, &self.to_park].into_iter().flatten() {
            if !one_park(park) {
                return Err(format!("`{park}` is not the name of a park of Pitboard's"));
            }
        }
        for item in &self.items {
            if !tree.items().iter().any(|known| known.path == item.path) {
                return Err(format!("`{}` is not an item of the account's", item.path));
            }
            if self.operation == Operation::SignOut && item.to_inode.is_some() {
                return Err("a sign-out installs nothing".into());
            }
        }
        Ok(())
    }
}

/// Whether `name` is one park's name, inside the parks directory.
fn one_park(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    ) && name.starts_with(paths::PARK_PREFIX)
}

/// How recovery treats a record it cannot act on now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reconcile {
    /// The command that follows is about this tool, and cannot go ahead until it is done.
    Required,
    /// The command is about no tool in particular. A folder login it cannot settle now is
    /// left for later, so the app being open never stops anything else.
    IfQuiet,
}

fn path(ctx: &Context) -> PathBuf {
    paths::desktop_home(ctx).join("journal.json")
}

/// Durable before anything moves. Refused when a record is there already: that one is for
/// recovery, and is never written over.
pub(super) fn write(ctx: &Context, journal: &TreeJournal) -> Result<()> {
    moves::ensure_private_dir(&paths::desktop_home(ctx))?;
    let path = path(ctx);
    if std::fs::symlink_metadata(&path).is_ok() {
        return Err(Error::RecoveryFailed {
            path,
            source: std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "an interrupted switch's record is already there",
            ),
        });
    }
    let body = serde_json::to_vec_pretty(journal).expect("a record of Pitboard's serialises");
    atomic::write(&path, &body, atomic::Perms::Secret)
        .map_err(|source| Error::RecoveryFailed { path, source })
}

/// The run reached a state the state file fully describes.
pub(super) fn clear(ctx: &Context) {
    let _ = std::fs::remove_file(path(ctx));
}

/// The interrupted folder switch the next command about its tool will finish or undo, as
/// the two sides it names; empty for a side there is none of.
pub fn pending(ctx: &Context) -> Option<(String, String)> {
    let journal = read(ctx).ok().flatten()?;
    let name = |key: Option<Key>| key.map(|k| k.typed()).unwrap_or_default();
    Some((
        name(journal.side(&journal.from_label)),
        name(journal.side(&journal.to_label)),
    ))
}

/// The switch a run left recorded when it failed partway, said as a warning for that
/// failure. Names are as typed, the way `pending` says them.
pub fn unfinished(ctx: &Context) -> Option<Warning> {
    let journal = read(ctx).ok().flatten()?;
    let name = |key: Option<Key>| key.map(|k| k.typed()).unwrap_or_default();
    Some(Warning::SwitchUnfinished {
        tool: journal.provider,
        from: name(journal.side(&journal.from_label)),
        to: name(journal.side(&journal.to_label)),
    })
}

/// An interrupted folder switch left waiting because its app is open, said the way a
/// command that goes ahead says it, for a report that only reads and so never settles it.
/// Nothing is moved and the record is kept.
pub fn waiting(ctx: &Context, state: &State) -> Option<Warning> {
    let journal = read(ctx).ok().flatten()?;
    let tool = journal.provider;
    tree::quiet(ctx, tool).is_err().then(|| {
        let (from, to) = journal.named(state);
        Warning::RecoveryWaiting { tool, from, to }
    })
}

/// The record, where there is one. Written atomically, so a record that does not parse, or
/// does not say what every record says, was damaged afterwards and says nothing of how far
/// its run got.
fn read(ctx: &Context) -> Result<Option<TreeJournal>> {
    let path = path(ctx);
    let raw = match std::fs::read(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(Error::RecoveryFailed { path, source }),
    };
    let journal: TreeJournal =
        serde_json::from_slice(&raw).map_err(|source| Error::RecoveryRecordCorrupt {
            path: path.clone(),
            source,
        })?;
    journal
        .check()
        .map_err(|why| Error::RecoveryRecordCorrupt {
            path,
            source: <serde_json::Error as serde::de::Error>::custom(why),
        })?;
    Ok(Some(journal))
}

/// One step of putting things back, decided before any is taken.
enum Step {
    /// Set aside what is at this path.
    Stray(PathBuf),
    /// Move this item from the first path to the second.
    Move(PathBuf, PathBuf),
}

/// The tool whose folder switch was interrupted, where a record of one can be read.
pub(super) fn tool(ctx: &Context) -> Option<ProviderId> {
    read(ctx).ok().flatten().map(|journal| journal.provider)
}

/// Finish or undo the interrupted run, if there was one: `Warning::Recovered` for a run
/// settled now, and in `IfQuiet` mode `Warning::RecoveryWaiting` for one left for later
/// because the app is open.
///
/// Only the app being open leaves a run for later. A record that cannot be read, one from
/// another data folder, an item in neither place or a move that fails is an error in either
/// mode: the command that follows must not go ahead as if every item were accounted for,
/// and uninstalling or repairing past an unfinished run could delete the only copy of a
/// login.
pub(super) fn reconcile(
    ctx: &Context,
    state: &mut State,
    mode: Reconcile,
) -> Result<Option<Warning>> {
    let Some(journal) = read(ctx)? else {
        return Ok(None);
    };
    match settle_journal(ctx, state, &journal) {
        Ok(recovered) => Ok(Some(Warning::Recovered(recovered))),
        Err(Error::RecoveryWaiting { tool, from, to }) if mode == Reconcile::IfQuiet => {
            Ok(Some(Warning::RecoveryWaiting { tool, from, to }))
        }
        // The app opened between two moves. What has moved is where the record says it
        // goes, so the next run picks up from there.
        Err(Error::AppStillOpen { tool, .. }) if mode == Reconcile::IfQuiet => {
            let (from, to) = journal.named(state);
            Ok(Some(Warning::RecoveryWaiting { tool, from, to }))
        }
        Err(e) => Err(e),
    }
}

fn settle_journal(ctx: &Context, state: &mut State, journal: &TreeJournal) -> Result<Recovered> {
    let which = journal.provider;
    let tree = tree::tree_of(which)?;
    let (from, to) = journal.named(state);

    // Another data folder than the one the run moved items of: the items it names are not
    // here to judge.
    let root = tree.root(ctx);
    if root.as_deref() != Some(journal.support_dir.as_path()) {
        return Err(Error::RecoveryElsewhere {
            tool: which,
            from,
            to,
            slot: journal.support_dir.display().to_string(),
        });
    }
    let root = journal.support_dir.clone();
    if tree::quiet(ctx, which).is_err() {
        return Err(Error::RecoveryWaiting {
            tool: which,
            from,
            to,
        });
    }

    let undetermined = |detail: String| Error::RecoveryUndetermined {
        tool: which,
        from: from.clone(),
        to: to.clone(),
        detail,
    };

    // Recorded as parked is the point after which the run is finished rather than undone.
    let parked = match (&journal.from_uuid, &journal.from_park) {
        (Some(uuid), Some(park)) => state
            .by_uuid(which, uuid)
            .and_then(|a| a.parked.as_ref())
            .is_some_and(|p| p.service == *park),
        _ => false,
    };
    let forward = journal.from_uuid.is_none() || parked;
    let parks = paths::parks_dir(ctx);

    if !forward {
        let from_park = parks.join(journal.from_park.as_deref().unwrap_or_default());
        let mut steps = Vec::new();
        for item in &journal.items {
            let parked = tree::inode_at(&from_park.join(&item.path))?;
            let there = root.join(&item.path);
            let live = tree::inode_at(&there)?;
            match (parked, live) {
                (Some(p), live) if Some(p) == item.from_inode => {
                    if live.is_some() {
                        steps.push(Step::Stray(there.clone()));
                    }
                    steps.push(Step::Move(from_park.join(&item.path), there));
                }
                (None, live) if live == item.from_inode => {}
                (None, Some(_)) if item.from_inode.is_none() => steps.push(Step::Stray(there)),
                _ => {
                    return Err(undetermined(format!(
                        "`{}` is in neither place the switch put it",
                        item.path
                    )));
                }
            }
        }
        let mut strays = moves::Strays::new();
        take(ctx, which, steps, &mut strays)?;
        // What is left of the park the run began is Pitboard's own records of it, and the
        // folders the moves made. Anything else in it is set aside, never deleted.
        let _ = std::fs::remove_file(from_park.join(CONFIG_KEYS_FILE));
        let _ = std::fs::remove_file(from_park.join(MANIFEST_FILE));
        remove_empty(&from_park);
        if std::fs::symlink_metadata(&from_park).is_ok() {
            strays.set_aside(ctx, &from_park)?;
        }
        clear(ctx);
        return Ok(Recovered {
            from,
            to,
            finished: false,
            signed_out: journal.operation == Operation::SignOut,
        });
    }

    let to_park = journal.to_park.as_deref().map(|name| parks.join(name));
    let mut steps = Vec::new();
    for item in &journal.items {
        let there = root.join(&item.path);
        let live = tree::inode_at(&there)?;
        if item.from_inode.is_some() && live == item.from_inode {
            return Err(undetermined(format!(
                "`{}` is still signed in after it was parked",
                item.path
            )));
        }
        let incoming = match &to_park {
            Some(dir) => tree::inode_at(&dir.join(&item.path))?,
            None => None,
        };
        match (incoming, live) {
            (Some(i), live) if Some(i) == item.to_inode => {
                if live.is_some() {
                    steps.push(Step::Stray(there.clone()));
                }
                let from = to_park.as_ref().expect("an inode was read there");
                steps.push(Step::Move(from.join(&item.path), there));
            }
            (None, Some(l)) if Some(l) == item.to_inode => {}
            (None, None) if item.to_inode.is_none() => {}
            (None, Some(_)) if item.to_inode.is_none() => steps.push(Step::Stray(there)),
            _ => {
                return Err(undetermined(format!(
                    "`{}` is in neither place the switch put it",
                    item.path
                )));
            }
        }
    }
    take(ctx, which, steps, &mut moves::Strays::new())?;
    crate::fault::point("tree.recovery_moved");

    // The app rewrites its config as it runs, so it is asked once more, as before every move.
    tree::still_quiet(ctx, which)?;
    let config = paths::config_file(&root);
    let to_key = journal.side(&journal.to_label);
    match journal.operation {
        Operation::Switch => {
            let keys = to_park.as_ref().map(|dir| dir.join(CONFIG_KEYS_FILE));
            match keys.filter(|keys| keys.is_file()) {
                Some(keys) => config::splice(&config, &config::read_keys(&keys)?)?,
                // Spliced and the park deleted already, when the folder says so.
                None if verified(ctx, journal, &root)? => {}
                None => {
                    return Err(undetermined(
                        "the incoming account's keys of the config are gone".into(),
                    ));
                }
            }
            if !verified(ctx, journal, &root)? {
                return Err(Error::SwitchUnverified {
                    tool: which,
                    from: from.clone(),
                    to: to.clone(),
                    detail: "the data folder does not hold the session that was parked".into(),
                });
            }
            let to_key = to_key.expect("a switch names its incoming account");
            if state.get(&to_key).is_some() {
                state.set_active(which, Some(to_key.label.clone()));
                state.used(&to_key, ctx.now());
                if let Some(found) = tree.identify(ctx, &root)? {
                    tree::note_session(state, &to_key, &found);
                }
            }
            if let Some(park) = &journal.to_park {
                state.discard(park);
            }
            state::save(ctx, state)?;
        }
        Operation::SignOut => {
            config::splice(&config, &ConfigKeys::default())?;
            if tree.identify(ctx, &root)?.is_some() {
                return Err(Error::SwitchUnverified {
                    tool: which,
                    from: from.clone(),
                    to: String::new(),
                    detail: "the data folder still holds a session".into(),
                });
            }
            state.set_active(which, None);
            state::save(ctx, state)?;
            tree::write_awaiting(
                ctx,
                &tree::Awaiting {
                    from_label: journal.from_label.clone().unwrap_or_default(),
                    started_at: journal.started_at,
                },
            )?;
        }
    }
    clear(ctx);
    Ok(Recovered {
        from,
        to,
        finished: true,
        signed_out: journal.operation == Operation::SignOut,
    })
}

/// Whether the folder at `root` holds the incoming account's session: by its fingerprint,
/// or by its cookie jar being the very file that was parked, which the app may since have
/// written a renewed session into.
fn verified(ctx: &Context, journal: &TreeJournal, root: &Path) -> Result<bool> {
    let tree = tree::tree_of(journal.provider)?;
    let Some(found) = tree.identify(ctx, root)? else {
        return Ok(false);
    };
    if Some(&found.account_uuid) != journal.to_uuid.as_ref() {
        return Ok(false);
    }
    if Some(&found.fingerprint) == journal.to_fingerprint.as_ref() {
        return Ok(true);
    }
    let jar = journal.items.iter().find(|i| i.path == "Cookies");
    let live = tree::inode_at(&paths::cookies_db(root))?;
    Ok(jar.is_some_and(|i| i.to_inode.is_some() && i.to_inode == live))
}

/// Take the steps, each one after asking again whether the app has opened. What is set
/// aside goes into the recovery's one directory of strays.
fn take(
    ctx: &Context,
    which: ProviderId,
    steps: Vec<Step>,
    strays: &mut moves::Strays,
) -> Result<()> {
    for step in steps {
        tree::still_quiet(ctx, which)?;
        match step {
            Step::Stray(path) => {
                strays.set_aside(ctx, &path)?;
            }
            Step::Move(from, to) => {
                tree::move_item(&from, &to)?;
            }
        }
    }
    Ok(())
}

/// Removes every empty directory under `dir`, and `dir` itself if it ends up empty. A
/// directory with anything in it is left as it is.
fn remove_empty(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            remove_empty(&entry.path());
        }
    }
    let _ = std::fs::remove_dir(dir);
}

/// Throws away a record that cannot be finished, moving nothing. Every item stays where it
/// is, in the data folder or a park, so the worst case is a park nothing names, which is
/// never deleted. A record that cannot be read is thrown away too: it was damaged after it
/// was written, and recovery would refuse it for ever.
pub(super) fn abandon(ctx: &Context, state: &mut State) -> Result<Option<Abandoned>> {
    let path = path(ctx);
    if std::fs::symlink_metadata(&path).is_err() {
        return Ok(None);
    }
    let Ok(Some(journal)) = read(ctx) else {
        clear(ctx);
        return Ok(Some(Abandoned {
            from: String::new(),
            to: String::new(),
            kept: 0,
        }));
    };
    let parks = paths::parks_dir(ctx);
    let mut kept = 0;
    for park in [&journal.from_park, &journal.to_park].into_iter().flatten() {
        let dir = parks.join(park);
        kept += journal
            .items
            .iter()
            .filter(|item| std::fs::symlink_metadata(dir.join(&item.path)).is_ok())
            .count();
    }
    let (from, to) = journal.named(state);
    clear(ctx);
    Ok(Some(Abandoned { from, to, kept }))
}

#[cfg(test)]
mod tests {
    use super::super::harness::{APP_PATH, NOW, TREE_POINTS, Whole, desktop_machine, jar};
    use super::*;
    use std::collections::BTreeSet;

    /// What must be true after a killed Claude Desktop switch has been recovered, wherever
    /// it died.
    fn hold(m: &super::super::harness::DesktopMachine, at: &str, before: &BTreeSet<u64>) {
        assert!(pending(&m.ctx).is_none(), "{at}: the record is gone");
        let here = m.whole("here");
        let there = m.whole("there");
        assert!(
            matches!(
                (&here, &there),
                (Whole::Live, Whole::Parked) | (Whole::Parked, Whole::Live)
            ),
            "{at}: one account signed in and one parked, all of each, got {here:?} and {there:?}"
        );
        let after: BTreeSet<u64> = m.inodes().into_values().collect();
        assert!(
            before.is_subset(&after),
            "{at}: nothing that was there is gone: {:?}",
            before.difference(&after).collect::<Vec<_>>()
        );
        let state = state::load(&m.ctx).unwrap();
        let live = crate::provider::desktop::identity::identify_tree(&m.ctx, &m.support())
            .unwrap()
            .expect("somebody signed in");
        let active = state
            .active_for(ProviderId::Desktop)
            .expect("an active account");
        assert_eq!(
            state.get(&m.key(active)).unwrap().account_uuid,
            live.account_uuid,
            "{at}: the state says who is signed in"
        );
    }

    #[test]
    fn every_tree_point_recovers_to_one_whole_account() {
        for point in TREE_POINTS {
            let m = desktop_machine(&format!("recovers-{}", point.replace('.', "-")));
            let before: BTreeSet<u64> = m.inodes().into_values().collect();
            assert_eq!(
                m.crash_at(point).unwrap_err(),
                point,
                "the switch must reach {point}, or the case proves nothing"
            );
            let recovered = m
                .recover()
                .unwrap_or_else(|e| panic!("{point}: recovery refused: {e}"));
            assert!(recovered.is_some(), "{point}: an interrupted switch found");
            hold(&m, point, &before);
            m.recover()
                .unwrap_or_else(|e| panic!("{point}: the second recovery refused: {e}"));
            hold(&m, &format!("{point}, recovered twice"), &before);

            // And the machine still switches.
            let (settled, _) = super::super::settle(&m.ctx, Some(ProviderId::Desktop)).unwrap();
            let back = if m.whole("here") == Whole::Live {
                "there"
            } else {
                "here"
            };
            super::super::switch(settled, &m.key(back))
                .unwrap_or_else(|e| panic!("{point}: the next switch refused: {e}"));
            hold(&m, &format!("{point}, switched again"), &before);
        }
    }

    #[test]
    fn every_tree_point_waits_while_claude_runs() {
        for point in TREE_POINTS {
            let m = desktop_machine(&format!("waits-{}", point.replace('.', "-")));
            assert_eq!(m.crash_at(point).unwrap_err(), point);
            let during = m.inodes();
            m.mem.runs_within(APP_PATH);
            let waiting = m.recover().expect_err("Claude is open");
            assert!(
                matches!(waiting, Error::RecoveryWaiting { .. }),
                "{point}: {waiting:?}"
            );
            assert_eq!(m.inodes(), during, "{point}: nothing moved");
            assert!(pending(&m.ctx).is_some(), "{point}: the record is kept");

            // A command about no tool in particular is not held up by it.
            assert!(super::super::settle(&m.ctx, None).is_ok(), "{point}");
            assert!(pending(&m.ctx).is_some());

            m.mem.quits_within();
            m.recover().expect("recovered once Claude quit");
            assert!(pending(&m.ctx).is_none());
        }
    }

    #[test]
    fn every_tree_point_sends_what_claude_made_to_strays() {
        for point in TREE_POINTS {
            if point == "tree.recorded" || point == "tree.config_spliced" {
                continue;
            }
            let m = desktop_machine(&format!("strays-{}", point.replace('.', "-")));
            let before: BTreeSet<u64> = m.inodes().into_values().collect();
            assert_eq!(m.crash_at(point).unwrap_err(), point);
            // The app opened on whatever was left, and made a session of its own.
            let cookies = m.support().join("Cookies");
            if cookies.exists() {
                continue;
            }
            std::fs::write(&cookies, b"made by Claude").unwrap();
            m.mem.plant_cookies(&cookies, jar("v10made", NOW + 86_400));
            let made = moves::inode(&cookies).unwrap().unwrap();

            m.recover()
                .unwrap_or_else(|e| panic!("{point}: recovery refused: {e}"));
            assert!(m.strays_hold(made), "{point}: set aside, never deleted");
            hold(&m, point, &before);
        }
    }

    #[test]
    fn a_tree_journal_from_elsewhere_is_left_alone() {
        let m = desktop_machine("elsewhere");
        assert_eq!(
            m.crash_at("tree.item_parked").unwrap_err(),
            "tree.item_parked"
        );
        let during = m.inodes();
        let ctx = m
            .ctx
            .clone()
            .with_desktop_dir(m.support().join("other").to_string_lossy().into_owned());
        let refused = super::super::settle(&ctx, Some(ProviderId::Desktop))
            .err()
            .expect("another data folder");
        assert!(
            matches!(refused, Error::RecoveryElsewhere { .. }),
            "{refused:?}"
        );
        assert_eq!(m.inodes(), during);
        assert!(pending(&m.ctx).is_some());
    }

    #[test]
    fn a_damaged_tree_journal_is_corrupt_not_guessed() {
        let m = desktop_machine("damaged");
        assert_eq!(
            m.crash_at("tree.item_parked").unwrap_err(),
            "tree.item_parked"
        );
        let during = m.inodes();
        let record = path(&m.ctx);
        let mut journal: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();

        // An item outside the account's, which recovery would otherwise move.
        journal["items"][0]["path"] = "../../escape".into();
        std::fs::write(&record, journal.to_string()).unwrap();
        let refused = m.recover().expect_err("damaged");
        assert!(
            matches!(refused, Error::RecoveryRecordCorrupt { .. }),
            "{refused:?}"
        );

        std::fs::write(&record, b"{not json").unwrap();
        let refused = m.recover().expect_err("damaged");
        assert!(
            matches!(refused, Error::RecoveryRecordCorrupt { .. }),
            "{refused:?}"
        );
        assert_eq!(m.inodes(), during, "nothing moved on a guess");

        // Abandoning it throws the record away and moves nothing.
        let abandoned = super::super::abandon(&m.ctx).unwrap();
        assert!(abandoned.is_some());
        assert!(pending(&m.ctx).is_none());
        assert_eq!(m.inodes(), during);
    }

    #[test]
    fn an_item_in_neither_place_is_undetermined() {
        let m = desktop_machine("neither");
        assert_eq!(
            m.crash_at("tree.live_parked").unwrap_err(),
            "tree.live_parked"
        );
        // Something took `Session Storage` out of the park the run made.
        let journal = read(&m.ctx).unwrap().unwrap();
        let park = paths::parks_dir(&m.ctx).join(journal.from_park.unwrap());
        let gone = m.support().join("elsewhere");
        std::fs::rename(park.join("Session Storage"), &gone).unwrap();
        let during = m.inodes();
        let refused = m.recover().expect_err("undetermined");
        assert!(
            matches!(refused, Error::RecoveryUndetermined { .. }),
            "{refused:?}"
        );
        assert_eq!(
            m.inodes(),
            during,
            "nothing moved while one item is unknown"
        );
        assert!(pending(&m.ctx).is_some());
    }

    #[test]
    fn a_desktop_interruption_never_blocks_claude_code() {
        let m = desktop_machine("never-blocks");
        assert_eq!(
            m.crash_at("tree.item_parked").unwrap_err(),
            "tree.item_parked"
        );
        m.mem.runs_within(APP_PATH);
        assert!(super::super::settle(&m.ctx, Some(ProviderId::Claude)).is_ok());
        assert!(super::super::settle(&m.ctx, Some(ProviderId::Codex)).is_ok());
        assert!(
            pending(&m.ctx).is_some(),
            "left for a Claude Desktop command"
        );
    }

    /// A command about no tool in particular goes ahead while Claude is open, and says the
    /// interrupted run is waiting rather than saying nothing.
    #[test]
    fn a_command_about_no_tool_warns_while_claude_runs() {
        let m = desktop_machine("no-tool-waiting");
        assert_eq!(
            m.crash_at("tree.item_parked").unwrap_err(),
            "tree.item_parked"
        );
        m.mem.runs_within(APP_PATH);
        let (_, found) =
            super::super::settle(&m.ctx, None).expect("a command about no tool goes ahead");
        assert!(
            matches!(
                found.as_slice(),
                [Warning::RecoveryWaiting { tool: ProviderId::Desktop, from, to }]
                    if from == "desktop/here" && to == "desktop/there"
            ),
            "{found:?}"
        );
        assert_eq!(
            pending(&m.ctx),
            Some(("desktop/here".to_string(), "desktop/there".to_string())),
            "what status reports as waiting"
        );
    }

    /// Recovery writes the config last, and Claude opening after the items are back in place
    /// leaves the config to Claude: the run waits for the next one, which finishes it.
    #[test]
    fn claude_starting_before_recovery_writes_the_config_leaves_it_alone() {
        let m = desktop_machine("recovery-before-config");
        assert_eq!(m.crash_at("tree.installed").unwrap_err(), "tree.installed");
        let config = m.support().join("config.json");
        let before = std::fs::read(&config).unwrap();
        let mem = std::sync::Arc::clone(&m.mem);
        let refused = crate::fault::meanwhile(
            "tree.recovery_moved",
            move || {
                mem.runs_within(APP_PATH);
            },
            || m.recover(),
        )
        .expect_err("the app opened before the config");
        assert!(matches!(refused, Error::AppStillOpen { .. }), "{refused:?}");
        assert_eq!(refused.code(), "app_opened_midway", "{refused:?}");
        assert!(
            !refused.to_string().contains("nothing was moved"),
            "{refused}"
        );
        assert_eq!(
            std::fs::read(&config).unwrap(),
            before,
            "the config is untouched"
        );
        assert!(
            pending(&m.ctx).is_some(),
            "the record stays for the next run"
        );

        m.mem.quits_within();
        let recovered = m
            .recover()
            .expect("recovered")
            .expect("the interrupted switch was found");
        assert!(recovered.finished, "finished, not undone");
        assert_eq!(m.whole("there"), Whole::Live);
        assert_eq!(m.whole("here"), Whole::Parked);
    }

    /// Only the app being open leaves a run for later. A record that cannot be trusted stops
    /// a command about no tool too, rather than letting it go ahead as if every item were
    /// accounted for.
    #[test]
    fn a_damaged_tree_journal_stops_a_command_about_no_tool() {
        let m = desktop_machine("no-tool-damaged");
        assert_eq!(
            m.crash_at("tree.item_parked").unwrap_err(),
            "tree.item_parked"
        );
        let during = m.inodes();
        std::fs::write(path(&m.ctx), b"{not json").unwrap();
        let refused = super::super::settle(&m.ctx, None)
            .err()
            .expect("a damaged record");
        assert!(
            matches!(refused, Error::RecoveryRecordCorrupt { .. }),
            "{refused:?}"
        );
        assert_eq!(m.inodes(), during);
    }

    /// An interrupted switch of Claude Code and one of Claude Desktop settled by the same
    /// command are both reported.
    #[test]
    fn both_interrupted_runs_are_reported() {
        let m = desktop_machine("both");
        assert_eq!(
            m.crash_at("tree.park_recorded").unwrap_err(),
            "tree.park_recorded"
        );
        // Claude Code killed switching from `a` to `b` before its park was stored, with `a`
        // still signed in.
        let live = super::super::harness::document("a-refresh");
        let claude = crate::provider::of(ProviderId::Claude);
        m.mem.live().plant(
            &crate::provider::claude::paths::live_service(&m.ctx),
            &live.to_string(),
        );
        let vault = super::super::journal::Journal {
            provider: ProviderId::Claude,
            started_at: NOW,
            from_label: "a".into(),
            from_uuid: "a".into(),
            to_label: "b".into(),
            to_uuid: "b".into(),
            park_service: "pitboard-a-1760000000000".into(),
            incoming_service: "pitboard-b-1759999999000".into(),
            from_fingerprint: claude.fingerprint(&live),
            to_fingerprint: "another".into(),
            slot: None,
        };
        super::super::journal::write_journal(&m.ctx, &vault).unwrap();

        let (_, found) = super::super::settle(&m.ctx, Some(ProviderId::Desktop)).unwrap();
        let recovered: Vec<_> = found
            .iter()
            .filter_map(|w| match w {
                Warning::Recovered(r) => Some((r.to.as_str(), r.finished)),
                _ => None,
            })
            .collect();
        assert_eq!(
            recovered,
            [("b", false), ("desktop/there", true)],
            "{found:?}"
        );
        assert!(!super::super::interrupted(&m.ctx));
        assert!(pending(&m.ctx).is_none());
    }

    /// An interrupted sign-out is reported as one, with the account it was signing out as
    /// its subject, and its tool is known before it is recovered.
    #[test]
    fn an_interrupted_sign_out_is_reported_as_one() {
        let m = desktop_machine("sign-out-recovered");
        let killed = crate::fault::killing("tree.config_spliced", || {
            let (settled, _) = super::super::settle(&m.ctx, Some(ProviderId::Desktop))?;
            super::super::sign_out(settled, ProviderId::Desktop)
        });
        assert_eq!(killed.unwrap_err(), "tree.config_spliced");
        assert_eq!(
            super::super::interrupted_tool(&m.ctx),
            Some(ProviderId::Desktop)
        );

        let recovered = m.recover().unwrap().expect("an interrupted sign-out");
        assert!(recovered.signed_out && recovered.finished, "{recovered:?}");
        assert_eq!(recovered.subject(), "desktop/here -> signed out");
        assert_eq!(
            recovered.to_string(),
            "an earlier sign-out of `desktop/here` was interrupted; it had in fact finished, \
             and Pitboard has recorded that"
        );
        assert_eq!(super::super::interrupted_tool(&m.ctx), None);
        assert_eq!(m.whole("here"), super::super::harness::Whole::Parked);
    }
}
