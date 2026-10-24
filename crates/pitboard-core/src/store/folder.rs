// The only code that touches `parks`. Which folder is a park or set aside, `state.json` says.

use crate::context::Context;
use crate::host::{Private, fs};
use crate::provider::folder::Keys;
use crate::service::Permit;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

const MANIFEST: &str = "manifest.json";
const KEYS: &str = "keys.json";
const ITEMS: &str = "items";
pub(crate) const VERSION: u32 = 1;
pub(crate) const ASIDE_PREFIX: &str = "pitboard-aside-";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Manifest {
    pub v: u32,
    pub tool: String,
    // the account it was parked for; None for an aside that never was a park
    pub account: Option<String>,
    // the uuid the settings' name key held
    pub named: Option<String>,
    // the fingerprint of the session it parked, written once
    pub login: Option<String>,
    #[serde(default)]
    pub session_expires_at: Option<i64>,
    pub parked_at: i64,
    pub machine: String,
    pub items: Vec<String>,
    // SHA-256, hex, of each account key's value as the settings held it
    pub keys: BTreeMap<String, String>,
    // the account's items a give-up left in the app's folder
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left_in_folder: Vec<String>,
}

pub(crate) fn parks_dir(ctx: &Context) -> PathBuf {
    crate::home::dir(ctx).join("parks")
}

pub(crate) fn at(ctx: &Context, name: &str) -> PathBuf {
    parks_dir(ctx).join(name)
}

pub(crate) fn keys_file(ctx: &Context, name: &str) -> PathBuf {
    at(ctx, name).join(KEYS)
}

pub(crate) fn item(ctx: &Context, name: &str, item: &str) -> PathBuf {
    at(ctx, name).join(ITEMS).join(item)
}

pub(crate) fn manifest_file(ctx: &Context, name: &str) -> PathBuf {
    at(ctx, name).join(MANIFEST)
}

// A park, or anything in it, not looked at: still there, as a vault not read is to the
// keychain's twin.
pub(crate) fn unread(label: &str, path: &Path, why: impl std::fmt::Display) -> crate::error::Error {
    crate::error::Error::Store(super::Error::ParkUnread {
        label: label.to_owned(),
        path: path.to_path_buf(),
        why: why.to_string(),
    })
}

// `parks`, made private where missing, on the volume `root` is on. Never changes a mode.
pub(crate) fn ready(ctx: &Context, permit: Permit, root: &Path) -> crate::error::Result<()> {
    use crate::error::Error;
    let parks = parks_dir(ctx);
    let unwritable = |source| Error::HomeUnwritable {
        path: parks.clone(),
        source,
    };
    if fs::private_dir(&parks).map_err(unwritable)? == Private::Missing {
        fs::create_private_dir(permit, &parks).map_err(unwritable)?;
    }
    match fs::private_dir(&parks).map_err(unwritable)? {
        Private::Yes => {}
        why => {
            return Err(Error::ParksNotPrivate {
                path: parks.clone(),
                why,
            });
        }
    }
    let volume = crate::fault::volume_of;
    if !fs::same_volume(volume(root), volume(&parks)).map_err(unwritable)? {
        return Err(Error::DifferentVolume {
            parks,
            root: root.to_path_buf(),
        });
    }
    Ok(())
}

// A new folder under `parks`, with its `items/`, both private: refused where anything is there.
pub(crate) fn make(ctx: &Context, permit: Permit, name: &str) -> io::Result<PathBuf> {
    kept_name(name)?;
    let folder = at(ctx, name);
    if std::fs::symlink_metadata(&folder).is_ok() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    fs::create_private_dir(permit, &folder.join(ITEMS))?;
    fs::sync_dir(&parks_dir(ctx));
    Ok(folder)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Moved {
    Synced,
    // the rename was made and a flush after it failed: the move is taken as made
    NotSynced,
}

// By rename alone, never over anything. `Err` only where the rename was not made.
pub(crate) fn move_in(permit: Permit, from: &Path, to: &Path) -> io::Result<Moved> {
    if crate::fault::fails("move_in") {
        return Err(io::Error::other("a test failed this move"));
    }
    fs::move_new(permit, from, to)?;
    let flushed = [from.parent(), to.parent()]
        .into_iter()
        .flatten()
        .try_for_each(fs::flush_dir);
    if flushed.is_err() || crate::fault::fails("move_in.flushed") {
        return Ok(Moved::NotSynced);
    }
    Ok(Moved::Synced)
}

// The parents of `item` between the folder's `items/` and it, made private where missing.
pub(crate) fn make_parents(ctx: &Context, permit: Permit, name: &str, of: &str) -> io::Result<()> {
    match item(ctx, name, of).parent() {
        Some(parent) => fs::create_private_dir(permit, parent),
        None => Ok(()),
    }
}

pub(crate) fn write_manifest(
    ctx: &Context,
    permit: Permit,
    name: &str,
    manifest: &Manifest,
) -> io::Result<()> {
    let body = serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?;
    crate::atomic::write(
        permit,
        &manifest_file(ctx, name),
        &body,
        crate::atomic::Perms::Secret,
    )
}

pub(crate) fn write_keys(ctx: &Context, permit: Permit, name: &str, keys: &[u8]) -> io::Result<()> {
    crate::atomic::write(
        permit,
        &keys_file(ctx, name),
        keys,
        crate::atomic::Perms::Secret,
    )
}

// The keys kept in the folder. A `keys.json` that is not there is `NotFound`; the inner
// refusal is one that does not parse.
pub(crate) fn keys(ctx: &Context, name: &str) -> io::Result<Result<Keys, String>> {
    std::fs::read(keys_file(ctx, name)).map(|raw| Keys::from_json(&raw))
}

// None: no manifest. Refused: one that does not read, or that a newer Pitboard wrote.
pub(crate) fn manifest(ctx: &Context, name: &str) -> Result<Option<Manifest>, String> {
    match read_manifest(ctx, name) {
        Ok(read) => read.map(Some),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("its manifest could not be read: {e}")),
    }
}

// The manifest. One that is not there is `NotFound`; the inner refusal is one that does not
// parse, or that a newer Pitboard wrote.
pub(crate) fn read_manifest(ctx: &Context, name: &str) -> io::Result<Result<Manifest, String>> {
    std::fs::read(manifest_file(ctx, name)).map(|raw| parsed(&raw))
}

fn parsed(raw: &[u8]) -> Result<Manifest, String> {
    let found: serde_json::Value =
        serde_json::from_slice(raw).map_err(|e| format!("its manifest is not JSON: {e}"))?;
    if found["v"].as_u64().is_some_and(|v| v > u64::from(VERSION)) {
        return Err("its manifest was written by a newer Pitboard".into());
    }
    serde_json::from_value(found)
        .map_err(|e| format!("its manifest is not one Pitboard writes: {e}"))
}

// Which of `of` are under the folder's `items/`, in the order given.
pub(crate) fn held_items(ctx: &Context, name: &str, of: &[String]) -> io::Result<Vec<String>> {
    let mut held = Vec::new();
    for listed in of {
        match std::fs::symlink_metadata(item(ctx, name, listed)) {
            Ok(_) => held.push(listed.clone()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(held)
}

// Whether the folder holds anything a login is made of: an item, or its keys.
pub(crate) fn holds_anything(ctx: &Context, name: &str) -> io::Result<bool> {
    let items = at(ctx, name).join(ITEMS);
    let any_item = match std::fs::read_dir(&items) {
        Ok(mut entries) => entries.next().is_some(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    };
    if any_item {
        return Ok(true);
    }
    match std::fs::symlink_metadata(keys_file(ctx, name)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

// One path component named as a park or an aside: the only folders Pitboard lists or removes.
fn kept_name(name: &str) -> io::Result<()> {
    let mut parts = Path::new(name).components();
    let one_name = matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    );
    if one_name && (crate::park::is_park_name(name) || name.starts_with(ASIDE_PREFIX)) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "not the name of a folder Pitboard keeps",
    ))
}

// Refused while any item is under `items/`. A temporary a kill left there is swept first.
pub(crate) fn remove_emptied(ctx: &Context, permit: Permit, name: &str) -> io::Result<()> {
    kept_name(name)?;
    let folder = at(ctx, name);
    match std::fs::symlink_metadata(&folder) {
        Ok(found) if found.is_dir() => {}
        Ok(_) => return Err(io::ErrorKind::InvalidInput.into()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    }
    crate::atomic::sweep(permit, &folder);
    let items = folder.join(ITEMS);
    let mut empty = Vec::new();
    if std::fs::symlink_metadata(&items).is_ok() {
        directories_only(&items, &mut empty)?;
    }
    for dir in empty.iter().rev() {
        fs::remove_dir(permit, dir)?;
    }
    for file in [KEYS, MANIFEST] {
        match fs::remove_file(permit, &folder.join(file)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    fs::remove_dir(permit, &folder)
}

// Every directory under `dir`, `dir` first, refused where any file or link is there.
fn directories_only(dir: &Path, found: &mut Vec<PathBuf>) -> io::Result<()> {
    let kind = std::fs::symlink_metadata(dir)?.file_type();
    if !kind.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::DirectoryNotEmpty,
            "an item is still there",
        ));
    }
    found.push(dir.to_path_buf());
    for entry in std::fs::read_dir(dir)? {
        directories_only(&entry?.path(), found)?;
    }
    Ok(())
}

// Only a park's or an aside's name, a directory right inside a private `parks`, never a link.
pub(crate) fn delete(ctx: &Context, permit: Permit, name: &str) -> io::Result<()> {
    let refuse = |why: &str| io::Error::new(io::ErrorKind::PermissionDenied, why.to_string());
    kept_name(name)?;
    let parks = parks_dir(ctx);
    match fs::private_dir(&parks)? {
        Private::Yes => {}
        Private::Missing => return Ok(()),
        _ => {
            return Err(refuse(
                "the parks folder is not a private directory of this user's",
            ));
        }
    }
    let folder = parks.join(name);
    match std::fs::symlink_metadata(&folder) {
        Ok(found) if found.file_type().is_symlink() || !found.is_dir() => {
            return Err(refuse("a parked login is a directory, and this is not one"));
        }
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    }
    if folder.canonicalize()?.parent() != Some(parks.canonicalize()?.as_path()) {
        return Err(refuse("it is not inside the parks folder"));
    }
    fs::remove_dir_all(permit, &folder)?;
    fs::sync_dir(&parks);
    Ok(())
}

// The folders of `parks` Pitboard can have made; None where there is no `parks`.
pub(crate) fn list(ctx: &Context) -> io::Result<Option<Vec<String>>> {
    let entries = match std::fs::read_dir(parks_dir(ctx)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && let Some(name) = entry.file_name().to_str()
            && kept_name(name).is_ok()
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(Some(names))
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::host::fs::testing;

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses the folder's file calls"
    )]
    fn a_parks_folder_others_can_reach_is_refused_and_its_mode_kept() {
        let (s, ctx) = scratch();
        let permit = Permit::for_a_test();
        let root = s.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        ready(&ctx, permit, &root).expect("made where missing");
        let parks = parks_dir(&ctx);
        assert!(testing::is_private(&parks).unwrap());

        testing::allow_changes(&parks).unwrap();
        let refused = ready(&ctx, permit, &root).unwrap_err();
        assert_eq!(refused.code(), "parks_not_private");
        assert!(
            !testing::is_private(&parks).unwrap(),
            "its mode is not changed"
        );
        testing::make_private(&parks).unwrap();

        std::fs::remove_dir(&parks).unwrap();
        testing::link_dir(&root, &parks).unwrap();
        let refused = ready(&ctx, permit, &root).unwrap_err();
        assert!(refused.to_string().contains("is a link"), "{refused}");
        std::fs::remove_file(&parks).unwrap();
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses the folder's file calls"
    )]
    fn a_park_on_another_volume_is_refused() {
        let (s, ctx) = scratch();
        let root = s.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        ready(&ctx, Permit::for_a_test(), &root).expect("one scratch folder is one volume");
        let parks = parks_dir(&ctx).to_string_lossy().into_owned();
        let refused = crate::fault::failing_at("another_volume", &parks, || {
            ready(&ctx, Permit::for_a_test(), &root)
        })
        .unwrap_err();
        match refused {
            crate::error::Error::DifferentVolume { parks, root: from } => {
                assert_eq!((parks, from), (parks_dir(&ctx), root));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses the folder's file calls"
    )]
    fn a_folder_is_made_once_and_a_move_never_lands_on_anything() {
        let (s, ctx) = scratch();
        let permit = Permit::for_a_test();
        let folder = make(&ctx, permit, "pitboard-aside-1").expect("made");
        assert!(testing::is_private(&folder).unwrap());
        assert!(testing::is_private(&folder.join(ITEMS)).unwrap());
        assert_eq!(
            make(&ctx, permit, "pitboard-aside-1").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(make(&ctx, permit, "not-a-park").is_err());

        let from = s.path().join("Cookies");
        std::fs::write(&from, b"jar").unwrap();
        let to = item(&ctx, "pitboard-aside-1", "Cookies");
        assert_eq!(move_in(permit, &from, &to).unwrap(), Moved::Synced);
        std::fs::write(&from, b"another").unwrap();
        assert_eq!(
            move_in(permit, &from, &to).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(&to).unwrap(), b"jar");
        std::fs::remove_file(&to).unwrap();
        let moved = crate::fault::failing("move_in.flushed", || move_in(permit, &from, &to));
        assert_eq!(
            moved.unwrap(),
            Moved::NotSynced,
            "the move is taken as made"
        );
        assert_eq!(std::fs::read(&to).unwrap(), b"another");
    }

    fn scratch() -> (tempfile::TempDir, Context) {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path();
        let ctx = Context::new(root.join("home")).with_pitboard_home(root.join("pitboard"));
        (scratch, ctx)
    }

    fn private(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        testing::make_private(path).unwrap();
    }

    const PARK: &str = "pitboard-park-desktop_a-1";

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn delete_never_leaves_parks() {
        let (s, ctx) = scratch();
        let permit = Permit::for_a_test();
        let parks = parks_dir(&ctx);
        private(parks.parent().unwrap());
        private(&parks);
        let outside = s.path().join("outside");
        std::fs::create_dir_all(outside.join(PARK)).unwrap();
        std::fs::write(outside.join("keep"), b"keep").unwrap();

        testing::link_dir(&outside, &parks.join("pitboard-park-desktop_link-1")).unwrap();
        assert!(delete(&ctx, permit, "pitboard-park-desktop_link-1").is_err());
        assert!(outside.join("keep").exists());

        std::fs::create_dir_all(parks.join("other")).unwrap();
        for name in [
            "other",
            "",
            "..",
            "pitboard-park-../outside",
            "../outside/pitboard-park-desktop_a-1",
            "pitboard-park-desktop_a-1/../../outside",
            "/tmp/pitboard-park-desktop_a-1",
            "pitboard-asides",
        ] {
            assert!(delete(&ctx, permit, name).is_err(), "{name}");
            assert!(remove_emptied(&ctx, permit, name).is_err(), "{name}");
        }
        assert!(parks.join("other").is_dir());
        assert!(outside.join(PARK).is_dir());
        assert!(parks.is_dir(), "the empty name is `parks` itself");
        // What a line `folder ../x` in `pending` would name: a folder beside `parks`.
        let beside = parks.parent().unwrap().join("x");
        std::fs::create_dir_all(&beside).unwrap();
        std::fs::write(beside.join(MANIFEST), b"{}").unwrap();
        assert!(remove_emptied(&ctx, permit, "../x").is_err());
        assert!(beside.join(MANIFEST).exists());
        assert_eq!(
            list(&ctx).unwrap(),
            Some(Vec::new()),
            "a folder under another name is none of Pitboard's"
        );

        std::fs::create_dir_all(parks.join(PARK)).unwrap();
        testing::open_to_others(&parks).unwrap();
        assert!(delete(&ctx, permit, PARK).is_err(), "others can reach it");
        testing::make_private(&parks).unwrap();

        std::fs::create_dir_all(parks.join(PARK).join("items/Local Storage")).unwrap();
        std::fs::create_dir_all(parks.join("pitboard-aside-7/items")).unwrap();
        delete(&ctx, permit, PARK).unwrap();
        delete(&ctx, permit, "pitboard-aside-7").unwrap();
        assert!(!parks.join(PARK).exists());
        assert!(!parks.join("pitboard-aside-7").exists());
        assert!(parks.join("other").is_dir());
        delete(&ctx, permit, PARK).unwrap();

        let (_s2, elsewhere) = scratch();
        let real = s.path().join("real-parks");
        std::fs::create_dir_all(real.join(PARK)).unwrap();
        private(&crate::home::dir(&elsewhere));
        testing::link_dir(&real, &parks_dir(&elsewhere)).unwrap();
        assert!(delete(&elsewhere, permit, PARK).is_err());
        assert!(real.join(PARK).is_dir());
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn remove_emptied_keeps_a_folder_that_holds_an_item() {
        let (_s, ctx) = scratch();
        let folder = at(&ctx, PARK);
        std::fs::create_dir_all(folder.join("items/IndexedDB")).unwrap();
        std::fs::write(folder.join("items/IndexedDB/x"), b"x").unwrap();
        std::fs::write(folder.join(KEYS), b"{}").unwrap();
        let refused = remove_emptied(&ctx, Permit::for_a_test(), PARK).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::DirectoryNotEmpty);
        assert!(folder.join("items/IndexedDB/x").exists());
        assert!(folder.join(KEYS).exists());
        std::fs::remove_file(folder.join("items/IndexedDB/x")).unwrap();
        testing::link(Path::new("/nowhere"), &folder.join("items/Cookies")).unwrap();
        assert!(
            remove_emptied(&ctx, Permit::for_a_test(), PARK).is_err(),
            "a link"
        );
        assert!(folder.join(KEYS).exists());
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn remove_emptied_takes_empty_directories_and_a_temporary() {
        let (_s, ctx) = scratch();
        let folder = at(&ctx, PARK);
        std::fs::create_dir_all(folder.join("items/IndexedDB/https_claude.ai_0.indexeddb.leveldb"))
            .unwrap();
        std::fs::write(folder.join(KEYS), b"{}").unwrap();
        std::fs::write(folder.join(MANIFEST), b"{}").unwrap();
        // A pid nothing runs as, as a run killed inside a write leaves it.
        std::fs::write(folder.join(".manifest.json.4194303.0.pitboard"), b"{}").unwrap();
        remove_emptied(&ctx, Permit::for_a_test(), PARK).unwrap();
        assert!(!folder.exists());
        remove_emptied(&ctx, Permit::for_a_test(), PARK).unwrap();
    }

    #[test]
    fn a_manifest_of_a_newer_version_is_refused() {
        let (_s, ctx) = scratch();
        let folder = at(&ctx, PARK);
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(manifest(&ctx, PARK), Ok(None));
        std::fs::write(folder.join(MANIFEST), br#"{"v": 2}"#).unwrap();
        assert!(manifest(&ctx, PARK).unwrap_err().contains("newer"));
        std::fs::write(folder.join(MANIFEST), b"not json").unwrap();
        assert!(manifest(&ctx, PARK).is_err());
        let written = Manifest {
            v: 1,
            tool: "desktop".into(),
            account: Some("desktop_a".into()),
            named: Some("a".into()),
            login: Some("0a1b2c3d4e5f6071".into()),
            session_expires_at: None,
            parked_at: 1,
            machine: "m".into(),
            items: vec!["Cookies".into()],
            keys: BTreeMap::new(),
            left_in_folder: Vec::new(),
        };
        std::fs::write(folder.join(MANIFEST), serde_json::to_vec(&written).unwrap()).unwrap();
        assert_eq!(manifest(&ctx, PARK), Ok(Some(written)));
        assert_eq!(list(&ctx).unwrap(), Some(vec![PARK.to_string()]));
    }
}
