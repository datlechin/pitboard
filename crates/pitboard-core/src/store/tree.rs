//! Moving a tree login's items, which is only ever done by rename.
//!
//! A login that lives in a folder is moved, never copied: a copy would leave a second live
//! session behind, and a copy interrupted halfway is a login that is neither here nor
//! there. So every move here is one `rename(2)` on one volume, that never lands on
//! something already there, made durable by syncing both directories, and checked by the
//! inode it carries. Nothing here deletes anything but a park, and only from inside the
//! parks directory.
//!
//! The tree switch decides what moves where; this is how a move is done.

use crate::context::Context;
use crate::error::Error;
use crate::provider::desktop::paths::{
    PARK_PREFIX, desktop_home, parks_dir, strays_dir, support_dir,
};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Component, Path, PathBuf};

/// A rename that was made, after which its directories could not be synced: the item is at
/// `to` now, only perhaps not yet on disk. Carried inside the `io::Error`
/// [`rename_durably`] answers, and found again by [`moved_not_synced`].
#[derive(Debug, thiserror::Error)]
#[error("moved to {}, but the move could not be synced to disk: {source}", to.display())]
pub(crate) struct MovedNotSynced {
    pub to: PathBuf,
    /// The inode that moved, as [`rename_durably`] answers on success.
    pub inode: u64,
    #[source]
    pub source: io::Error,
}

/// The move `error` says was made, where [`rename_durably`] failed only after its rename.
pub(crate) fn moved_not_synced(error: &io::Error) -> Option<&MovedNotSynced> {
    error.get_ref()?.downcast_ref()
}

/// Moves `from` to `to` by one rename, and answers the inode it moved.
///
/// Refused when anything at all is at `to`, a dangling link or an empty directory
/// included, which a plain rename would replace without a word. A missing parent of `to`
/// is made, private. Both parents are synced afterwards, so the move survives a power cut,
/// and what arrived is checked to be what left.
///
/// Every error but one means nothing moved. The one is a sync that failed after the
/// rename, when the item is at `to`: that error carries a [`MovedNotSynced`], which
/// [`moved_not_synced`] finds, so a caller records the move as made.
pub(crate) fn rename_durably(from: &Path, to: &Path) -> io::Result<u64> {
    match std::fs::symlink_metadata(to) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} is already there", to.display()),
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let before = inode(from)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not there to move", from.display()),
        )
    })?;
    // Both found before the rename, so nothing that can fail for want of them comes after.
    let from_parent = parent(from)?;
    let to_parent = parent(to)?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(to_parent)?;
    crate::host::fs::rename_exclusive(from, to)?;
    if let Err(source) = fsync_dir(from_parent).and_then(|()| fsync_dir(to_parent)) {
        return Err(io::Error::new(
            source.kind(),
            MovedNotSynced {
                to: to.to_path_buf(),
                inode: before,
                source,
            },
        ));
    }
    match inode(to)? {
        Some(after) if after == before => Ok(before),
        _ => Err(io::Error::other(format!(
            "{} is not what was moved there",
            to.display()
        ))),
    }
}

fn parent(path: &Path) -> io::Result<&Path> {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} has no directory", path.display()),
            )
        })
}

/// The inode at `path`, the path itself and not where a link points. `None` where there is
/// nothing.
pub(crate) fn inode(path: &Path) -> io::Result<Option<u64>> {
    match std::fs::symlink_metadata(path) {
        Ok(found) => Ok(Some(found.ino())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Refuses unless Claude's data folder `a` and Pitboard's parks `b` are on one volume,
/// where a rename moves and never copies.
pub(crate) fn same_device(ctx: &Context, a: &Path, b: &Path) -> Result<(), Error> {
    let host = ctx.host();
    let support = host
        .device_of(a)
        .map_err(|source| Error::DesktopDataInaccessible {
            path: a.to_path_buf(),
            source,
        })?;
    let parks = host.device_of(b).map_err(|source| Error::HomeUnwritable {
        path: b.to_path_buf(),
        source,
    })?;
    if support == parks {
        Ok(())
    } else {
        Err(Error::DifferentVolume {
            support_dir: a.to_path_buf(),
            parks_dir: b.to_path_buf(),
        })
    }
}

/// Makes `path` a directory only this user can reach, or refuses one that is not: a link,
/// something other than a directory, another user's, or one with any bit for group or
/// other. A directory already there is never changed, only refused.
pub(crate) fn ensure_private_dir(path: &Path) -> Result<(), Error> {
    let unwritable = |source| Error::HomeUnwritable {
        path: path.to_path_buf(),
        source,
    };
    let found = match std::fs::symlink_metadata(path) {
        Ok(found) => found,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)
                .map_err(unwritable)?;
            std::fs::symlink_metadata(path).map_err(unwritable)?
        }
        Err(e) => return Err(unwritable(e)),
    };
    refuse_unless_private(path, &found)
}

/// Refuses `found`, what is at `path`, unless it is a directory only this user can reach,
/// and not a link to one.
fn refuse_unless_private(path: &Path, found: &std::fs::Metadata) -> Result<(), Error> {
    let refuse = |why: String| Error::HomeUnwritable {
        path: path.to_path_buf(),
        source: io::Error::new(io::ErrorKind::PermissionDenied, why),
    };
    if found.file_type().is_symlink() {
        return Err(refuse(
            "it is a link, and Pitboard keeps logins only in a directory of its own".into(),
        ));
    }
    if !found.is_dir() {
        return Err(refuse("it is not a directory".into()));
    }
    // SAFETY: `getuid` reads this process's user id, touches no memory, and cannot fail.
    let me = unsafe { libc::getuid() };
    if found.uid() != me {
        return Err(refuse("it belongs to another user".into()));
    }
    if found.mode() & 0o077 != 0 {
        return Err(refuse(format!(
            "others can reach it (mode {:o}); only you should",
            found.mode() & 0o777
        )));
    }
    Ok(())
}

/// Syncs the directory at `path`, so the renames in it are on disk.
pub(crate) fn fsync_dir(path: &Path) -> io::Result<()> {
    #[cfg(test)]
    if SYNC_FAILS.with(std::cell::Cell::get) {
        return Err(io::Error::other("a sync a test made fail"));
    }
    std::fs::File::open(path)?.sync_all()
}

#[cfg(test)]
thread_local! {
    /// Makes every sync on this thread fail, for a test of what follows a move.
    pub(crate) static SYNC_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Deletes the park called `name`, and nothing else: refused unless the name is one park's
/// in the parks directory, the park is not a link and really is inside it, and the parks
/// directory and Pitboard's desktop directory above it are both private directories of
/// this user's, neither a link. A park already gone is nothing to do.
pub(crate) fn delete_park(ctx: &Context, name: &str) -> Result<(), Error> {
    let parks = parks_dir(ctx);
    let refuse = |path: &Path, why: &str| Error::HomeUnwritable {
        path: path.to_path_buf(),
        source: io::Error::new(io::ErrorKind::PermissionDenied, why.to_string()),
    };
    let mut parts = Path::new(name).components();
    let one_name = matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    );
    if !one_name || !name.starts_with(PARK_PREFIX) {
        return Err(refuse(&parks, "not the name of a park of Pitboard's"));
    }
    let unwritable = |path: &Path| {
        let path = path.to_path_buf();
        move |source| Error::HomeUnwritable { path, source }
    };
    // From the top down, so a link above the parks dir is refused before anything is read
    // through it.
    for dir in [desktop_home(ctx), parks.clone()] {
        match std::fs::symlink_metadata(&dir) {
            Ok(found) => refuse_unless_private(&dir, &found)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(unwritable(&dir)(e)),
        }
    }
    let park = parks.join(name);
    match std::fs::symlink_metadata(&park) {
        Ok(found) if found.file_type().is_symlink() || !found.is_dir() => {
            return Err(refuse(&park, "a park is a directory, and this is not one"));
        }
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(unwritable(&park)(e)),
    }
    let canonical = park.canonicalize().map_err(unwritable(&park))?;
    let home = parks.canonicalize().map_err(unwritable(&parks))?;
    if canonical.parent() != Some(home.as_path()) {
        return Err(refuse(&park, "it is not inside the parks directory"));
    }
    std::fs::remove_dir_all(&park).map_err(unwritable(&park))?;
    fsync_dir(&parks).map_err(unwritable(&parks))
}

/// Where `path` sits in the data folder, or in the parks directory, kept under a stray's
/// directory so whoever looks can tell what it was. Its own name where it is in neither.
fn stray_place(ctx: &Context, path: &Path) -> Result<PathBuf, Error> {
    support_dir(ctx)
        .and_then(|dir| path.strip_prefix(dir).ok().map(Path::to_path_buf))
        .or_else(|| {
            path.strip_prefix(parks_dir(ctx))
                .ok()
                .map(Path::to_path_buf)
        })
        .filter(|rest| rest.components().all(|c| matches!(c, Component::Normal(_))))
        .filter(|rest| !rest.as_os_str().is_empty())
        .or_else(|| path.file_name().map(PathBuf::from))
        .ok_or_else(|| Error::HomeUnwritable {
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "nothing there to move aside"),
        })
}

/// Things moved aside into the strays directory, never deleted, each keeping where in the
/// data folder (or in a park) it was. One directory under the strays directory holds
/// everything one switch or one recovery sets aside, named for when the first thing was.
/// Nothing is made until something is set aside, so a run with no strays leaves no
/// directory. Each stray keeps its place inside it, and
/// one that would land on something already there gets a directory of its own instead:
/// nothing set aside is ever written over, and every move stays a rename inside Pitboard's
/// home, on one volume.
#[derive(Debug, Default)]
pub(crate) struct Strays {
    slot: Option<PathBuf>,
}

impl Strays {
    pub(crate) fn new() -> Strays {
        Strays::default()
    }

    /// The run's directory, chosen the first time it is asked for. Not made here.
    fn chosen(&mut self, ctx: &Context) -> Result<PathBuf, Error> {
        if let Some(slot) = &self.slot {
            return Ok(slot.clone());
        }
        let slot = stray_slot(ctx)?;
        self.slot = Some(slot.clone());
        Ok(slot)
    }

    /// The run's directory, made, for a file Pitboard writes into it.
    pub(crate) fn dir(&mut self, ctx: &Context) -> Result<PathBuf, Error> {
        let slot = self.chosen(ctx)?;
        ensure_private_dir(&slot)?;
        Ok(slot)
    }

    /// Moves `path` into the run's directory at its place, and answers where it went.
    pub(crate) fn set_aside(&mut self, ctx: &Context, path: &Path) -> Result<PathBuf, Error> {
        let place = stray_place(ctx, path)?;
        let mut target = self.chosen(ctx)?.join(&place);
        if std::fs::symlink_metadata(&target).is_ok() {
            target = stray_slot(ctx)?.join(&place);
        }
        rename_durably(path, &target).map_err(|source| Error::HomeUnwritable {
            path: target.clone(),
            source,
        })?;
        Ok(target)
    }
}

/// A directory of its own under the strays directory, named for now, for one thing set
/// aside. The strays directory is made; the slot is not, and nothing is in it.
pub(crate) fn stray_slot(ctx: &Context) -> Result<PathBuf, Error> {
    ensure_private_dir(&desktop_home(ctx))?;
    let strays = strays_dir(ctx);
    ensure_private_dir(&strays)?;
    let now = ctx.now_millis();
    let mut taken = 0;
    loop {
        let dir = strays.join((now + taken).to_string());
        match std::fs::symlink_metadata(&dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(dir),
            Err(source) => return Err(Error::HomeUnwritable { path: dir, source }),
            Ok(_) => taken += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::memory::MemoryHost;
    use crate::provider::desktop::paths::{parks_dir, strays_dir};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::PathBuf;
    use std::sync::Arc;

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "pitboard-tree-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch dir");
        Scratch(root)
    }

    /// A machine whose Pitboard home and Claude's data folder are both inside `root`.
    fn machine(root: &Path) -> (Context, Arc<MemoryHost>) {
        let mem = MemoryHost::new();
        let ctx = Context::new(root.to_path_buf())
            .with_pitboard_home(root.join(".pitboard"))
            .with_desktop_dir(root.join("Claude").to_string_lossy().into())
            .with_memory_stores(Arc::clone(&mem));
        (ctx, mem)
    }

    #[test]
    fn a_rename_never_lands_on_something_already_there() {
        let s = scratch("no-clobber");
        let from = s.0.join("from");
        let to = s.0.join("to");
        std::fs::write(&from, b"incoming").unwrap();
        std::fs::write(&to, b"already there").unwrap();

        let err = rename_durably(&from, &to).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "{err}");
        assert_eq!(std::fs::read(&to).unwrap(), b"already there");
        assert_eq!(std::fs::read(&from).unwrap(), b"incoming");

        // Nor on a dangling link, which `exists` would call absent.
        std::fs::remove_file(&to).unwrap();
        std::os::unix::fs::symlink(s.0.join("nowhere"), &to).unwrap();
        assert!(rename_durably(&from, &to).is_err());
        assert!(std::fs::symlink_metadata(&to).unwrap().is_symlink());
        assert!(!s.0.join("nowhere").exists());

        // Nor on an empty directory, which a plain rename would replace without a word.
        let dir_from = s.0.join("dir-from");
        let dir_to = s.0.join("dir-to");
        std::fs::create_dir(&dir_from).unwrap();
        std::fs::create_dir(&dir_to).unwrap();
        assert!(rename_durably(&dir_from, &dir_to).is_err());
        assert!(dir_from.is_dir());
    }

    #[test]
    fn a_move_keeps_the_inode() {
        let s = scratch("inode");
        let from = s.0.join("Local Storage");
        std::fs::create_dir_all(from.join("leveldb")).unwrap();
        std::fs::write(from.join("leveldb/000005.ldb"), b"rows").unwrap();
        let before = inode(&from).unwrap().expect("there");

        // Into a parent that does not exist yet, which is created private.
        let to = s.0.join("parks/pitboard-tree-a-1/Local Storage");
        let moved = rename_durably(&from, &to).unwrap();

        assert_eq!(moved, before);
        assert_eq!(inode(&to).unwrap(), Some(before));
        assert_eq!(inode(&from).unwrap(), None);
        assert_eq!(
            std::fs::read(to.join("leveldb/000005.ldb")).unwrap(),
            b"rows"
        );
        let parent = std::fs::metadata(to.parent().unwrap()).unwrap();
        assert_eq!(parent.permissions().mode() & 0o777, 0o700);
    }

    /// Once the rename is done the item is at `to`, whatever then fails: a sync that fails
    /// afterwards says so, and says what moved, rather than reading as a move not made.
    #[test]
    fn a_sync_that_fails_after_the_move_says_the_move_was_made() {
        let s = scratch("sync");
        let from = s.0.join("Cookies");
        std::fs::write(&from, b"jar").unwrap();
        let before = inode(&from).unwrap().expect("there");
        let to = s.0.join("parks/pitboard-tree-a-1/Cookies");

        SYNC_FAILS.with(|fails| fails.set(true));
        let err = rename_durably(&from, &to).unwrap_err();
        SYNC_FAILS.with(|fails| fails.set(false));

        let moved = moved_not_synced(&err).expect("told apart from a move not made");
        assert_eq!(moved.inode, before);
        assert_eq!(moved.to, to);
        assert_eq!(inode(&to).unwrap(), Some(before));
        assert_eq!(inode(&from).unwrap(), None);

        // A move refused before the rename is not one.
        std::fs::write(&from, b"another").unwrap();
        let refused = rename_durably(&from, &to).unwrap_err();
        assert!(moved_not_synced(&refused).is_none());
    }

    #[test]
    fn a_park_on_another_volume_is_refused() {
        let s = scratch("volume");
        let (ctx, mem) = machine(&s.0);
        let support = s.0.join("Claude");
        let parks = parks_dir(&ctx);
        std::fs::create_dir_all(&support).unwrap();
        std::fs::create_dir_all(&parks).unwrap();

        same_device(&ctx, &support, &parks).expect("one scratch dir is one volume");

        mem.device(&parks, std::fs::metadata(&support).unwrap().dev() + 1);
        match same_device(&ctx, &support, &parks) {
            Err(Error::DifferentVolume {
                support_dir,
                parks_dir,
            }) => {
                assert_eq!(support_dir, support);
                assert_eq!(parks_dir, parks);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_parks_dir_that_others_can_write_is_refused() {
        let s = scratch("private");
        let fresh = s.0.join("desktop/parks");
        ensure_private_dir(&fresh).expect("made");
        let made = std::fs::symlink_metadata(&fresh).unwrap();
        assert!(made.is_dir());
        assert_eq!(made.permissions().mode() & 0o777, 0o700);
        ensure_private_dir(&fresh).expect("already private");

        let open = s.0.join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o770)).unwrap();
        assert!(ensure_private_dir(&open).is_err());
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o705)).unwrap();
        assert!(ensure_private_dir(&open).is_err());
        assert_eq!(
            std::fs::metadata(&open).unwrap().permissions().mode() & 0o777,
            0o705,
            "refused, not quietly changed"
        );

        // A link to a private directory is still a link: whoever made it chose where parks go.
        let link = s.0.join("link");
        std::os::unix::fs::symlink(&fresh, &link).unwrap();
        assert!(ensure_private_dir(&link).is_err());

        let file = s.0.join("file");
        std::fs::write(&file, b"").unwrap();
        assert!(ensure_private_dir(&file).is_err());
    }

    #[test]
    fn delete_park_never_leaves_the_parks_dir() {
        let s = scratch("delete");
        let (ctx, _mem) = machine(&s.0);
        let parks = parks_dir(&ctx);
        ensure_private_dir(&parks).unwrap();
        let outside = s.0.join("outside");
        std::fs::create_dir_all(outside.join("pitboard-tree-x")).unwrap();
        std::fs::write(outside.join("keep"), b"keep").unwrap();

        // A park that is a link to somewhere else is not deleted, and nor is where it points.
        std::os::unix::fs::symlink(&outside, parks.join("pitboard-tree-link-1")).unwrap();
        assert!(delete_park(&ctx, "pitboard-tree-link-1").is_err());
        assert!(outside.join("keep").exists());

        // Names that are not a park's, or that climb out.
        std::fs::create_dir_all(parks.join("other")).unwrap();
        for name in [
            "other",
            "",
            "..",
            "pitboard-tree-../outside",
            "../outside/pitboard-tree-x",
            "pitboard-tree-a/../../outside",
            "/tmp/pitboard-tree-a",
        ] {
            assert!(delete_park(&ctx, name).is_err(), "{name}");
        }
        assert!(parks.join("other").is_dir());
        assert!(outside.join("pitboard-tree-x").is_dir());
        assert!(outside.join("keep").exists());

        // A parks dir that is itself a link is not followed.
        let real = s.0.join("real-parks");
        std::fs::create_dir_all(real.join("pitboard-tree-a-1")).unwrap();
        let (ctx2, _mem2) = machine(&s.0.join("second"));
        ensure_private_dir(parks_dir(&ctx2).parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&real, parks_dir(&ctx2)).unwrap();
        assert!(delete_park(&ctx2, "pitboard-tree-a-1").is_err());
        assert!(real.join("pitboard-tree-a-1").is_dir());

        // Nor is Pitboard's desktop directory, when it is a link to somewhere else with a
        // parks dir of its own.
        let elsewhere = s.0.join("elsewhere");
        std::fs::create_dir_all(elsewhere.join("parks/pitboard-tree-a-1")).unwrap();
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(
            elsewhere.join("parks"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let (ctx3, _mem3) = machine(&s.0.join("third"));
        let home3 = parks_dir(&ctx3).parent().unwrap().to_path_buf();
        ensure_private_dir(home3.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &home3).unwrap();
        assert!(delete_park(&ctx3, "pitboard-tree-a-1").is_err());
        assert!(elsewhere.join("parks/pitboard-tree-a-1").is_dir());

        // Nor one that others can write in, where anybody could have put what is in it.
        let (ctx4, _mem4) = machine(&s.0.join("fourth"));
        let parks4 = parks_dir(&ctx4);
        ensure_private_dir(&parks4).unwrap();
        std::fs::create_dir_all(parks4.join("pitboard-tree-a-1")).unwrap();
        std::fs::set_permissions(&parks4, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(delete_park(&ctx4, "pitboard-tree-a-1").is_err());
        assert!(parks4.join("pitboard-tree-a-1").is_dir());
        std::fs::set_permissions(&parks4, std::fs::Permissions::from_mode(0o700)).unwrap();

        // With no desktop directory at all there is no park to delete.
        let (ctx5, _mem5) = machine(&s.0.join("fifth"));
        delete_park(&ctx5, "pitboard-tree-a-1").unwrap();
        assert!(!parks_dir(&ctx5).exists());

        // A real park goes, and only it.
        std::fs::create_dir_all(parks.join("pitboard-tree-a-1/Local Storage")).unwrap();
        delete_park(&ctx, "pitboard-tree-a-1").unwrap();
        assert!(!parks.join("pitboard-tree-a-1").exists());
        assert!(parks.join("other").is_dir());
        // One already gone is nothing to do.
        delete_park(&ctx, "pitboard-tree-a-1").unwrap();
    }

    /// Something in the way is moved aside, never deleted, and keeps its place in the folder.
    #[test]
    fn a_stray_is_moved_aside_with_its_place_kept() {
        let s = scratch("strays");
        let (ctx, _mem) = machine(&s.0);
        let support = s.0.join("Claude");
        let item = support.join("IndexedDB/https_claude.ai_0.indexeddb.leveldb");
        std::fs::create_dir_all(&item).unwrap();
        std::fs::write(item.join("LOG"), b"log").unwrap();
        let before = inode(&item).unwrap();

        let moved = Strays::new().set_aside(&ctx, &item).unwrap();

        assert!(moved.starts_with(strays_dir(&ctx)));
        assert!(moved.ends_with("IndexedDB/https_claude.ai_0.indexeddb.leveldb"));
        assert_eq!(inode(&moved).unwrap(), before);
        assert!(!item.exists());
        let second = support.join("IndexedDB/https_claude.ai_0.indexeddb.leveldb");
        std::fs::create_dir_all(&second).unwrap();
        let again = Strays::new().set_aside(&ctx, &second).unwrap();
        assert_ne!(again, moved, "a second stray never lands on the first");
        assert!(moved.join("LOG").exists());
    }

    /// One run's strays share a directory, each at its place; one that would land on
    /// something set aside before goes to a directory of its own instead of over it.
    #[test]
    fn a_runs_strays_share_one_directory_and_never_land_on_each_other() {
        let s = scratch("strays-shared");
        let (ctx, _mem) = machine(&s.0);
        let support = s.0.join("Claude");
        let mut strays = Strays::new();
        let mut moved = Vec::new();
        for item in ["Local Storage", "Session Storage"] {
            let path = support.join(item);
            std::fs::create_dir_all(&path).unwrap();
            moved.push(strays.set_aside(&ctx, &path).unwrap());
        }
        let dir = strays.dir(&ctx).unwrap();
        assert!(
            moved.iter().all(|m| m.parent() == Some(dir.as_path())),
            "{moved:?}"
        );
        assert_eq!(std::fs::read_dir(strays_dir(&ctx)).unwrap().count(), 1);

        let again = support.join("Local Storage");
        std::fs::create_dir_all(&again).unwrap();
        let elsewhere = strays.set_aside(&ctx, &again).unwrap();
        assert_ne!(elsewhere.parent(), Some(dir.as_path()));
        assert!(moved[0].is_dir(), "the first is where it was put");
        assert!(elsewhere.is_dir());

        // A run that sets nothing aside makes nothing.
        let before = std::fs::read_dir(strays_dir(&ctx)).unwrap().count();
        let _ = Strays::new();
        assert_eq!(std::fs::read_dir(strays_dir(&ctx)).unwrap().count(), before);
    }
}
