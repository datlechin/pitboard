//! A watch on one folder: told as a file it names there is written, and when it may have
//! missed one or has ended.
//!
//! The system's own notice of a change, through `notify`: inotify on Linux, FSEvents on macOS
//! and ReadDirectoryChangesW on Windows, on the folder alone and nothing below it. A file is
//! known by its name alone, since FSEvents names it by its path with every link resolved,
//! `/private/var` for `/var`, and the others by the path watched. A read of a file is no change
//! of it, and neither is a change of another file. Where the system dropped what it had to
//! tell, the watch says it may have missed a write and goes on; where the folder itself went,
//! or the system stopped watching it, it says it has ended, and hears nothing more.
//!
//! The host says where a watch would not hear every write ([`super::Host::watches`]), and
//! nothing is watched there.

use super::Host;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a watch heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Heard {
    /// A file it names was made, written, renamed, removed or given another time.
    Written,
    /// The system dropped what it had to tell: a write may have gone unheard. It goes on.
    Missed,
    /// The folder went, or the system stopped watching it: nothing more is heard.
    Ended,
}

/// A watch, which stops as it is dropped.
#[derive(Debug)]
pub(crate) struct Watch {
    _watcher: super::os::watch::Watcher,
}

/// Watches `dir` for a change to a file named one of `names`, telling `heard` on the system's
/// own threads. Refused where `dir` is not a folder that is there, where the host says a watch
/// there would not hear every write, and where the system will not watch it.
pub(crate) fn start(
    host: &dyn Host,
    dir: &Path,
    names: &[OsString],
    heard: Arc<dyn Fn(Heard) + Send + Sync>,
) -> std::io::Result<Watch> {
    if !dir.is_dir() {
        return Err(std::io::ErrorKind::NotFound.into());
    }
    if !host.watches(dir) {
        return Err(std::io::ErrorKind::Unsupported.into());
    }
    let folder = Folder {
        canonical: std::fs::canonicalize(dir).ok(),
        given: dir.to_path_buf(),
        names: names.to_vec(),
    };
    let told = Arc::clone(&heard);
    let handler = move |event: notify::Result<notify::Event>| {
        if let Some(said) = folder.heard(event) {
            told(said);
        }
    };
    let watcher = super::os::watch::Watcher::on(dir, handler, move || heard(Heard::Ended))
        .map_err(std::io::Error::other)?;
    Ok(Watch { _watcher: watcher })
}

/// The folder watched, as each system names it in an event.
struct Folder {
    given: PathBuf,
    canonical: Option<PathBuf>,
    names: Vec<OsString>,
}

impl Folder {
    fn heard(&self, event: notify::Result<notify::Event>) -> Option<Heard> {
        use notify::EventKind;
        use notify::event::ModifyKind;
        // A system that cannot say what it saw has stopped watching.
        let Ok(event) = event else {
            return Some(Heard::Ended);
        };
        if event.need_rescan() {
            return Some(Heard::Missed);
        }
        let itself = event.paths.iter().any(|path| self.is(path));
        match event.kind {
            EventKind::Access(_) => None,
            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) if itself => {
                Some(Heard::Ended)
            }
            _ => event
                .paths
                .iter()
                .any(|path| !self.is(path) && self.names_file(path))
                .then_some(Heard::Written),
        }
    }

    fn is(&self, path: &Path) -> bool {
        path == self.given || self.canonical.as_deref() == Some(path)
    }

    fn names_file(&self, path: &Path) -> bool {
        path.file_name()
            .is_some_and(|name| self.names.iter().any(|named| named == name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{
        AccessKind, AccessMode, CreateKind, DataChange, Event, EventKind, Flag, MetadataKind,
        ModifyKind, RemoveKind, RenameMode,
    };

    fn folder() -> Folder {
        Folder {
            given: PathBuf::from("/var/folders/x/.pitboard"),
            canonical: Some(PathBuf::from("/private/var/folders/x/.pitboard")),
            names: vec!["state.json".into(), "usage.json".into()],
        }
    }

    fn of(kind: EventKind, paths: &[&str]) -> Option<Heard> {
        let event = paths
            .iter()
            .fold(Event::new(kind), |event, path| event.add_path(path.into()));
        folder().heard(Ok(event))
    }

    /// Each system's events for the core's atomic replace of a file it names, by name in the
    /// folder however the system spells the folder, and nothing for its temporary.
    #[test]
    fn a_named_file_is_heard_by_its_name_alone() {
        let into = EventKind::Modify(ModifyKind::Name(RenameMode::To));
        let both = EventKind::Modify(ModifyKind::Name(RenameMode::Both));
        assert_eq!(
            of(into, &["/var/folders/x/.pitboard/state.json"]),
            Some(Heard::Written)
        );
        assert_eq!(
            of(
                EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
                &["/private/var/folders/x/.pitboard/usage.json"]
            ),
            Some(Heard::Written),
            "FSEvents' path"
        );
        assert_eq!(
            of(
                both,
                &[
                    "/var/folders/x/.pitboard/.state.json.7.0.pitboard",
                    "/var/folders/x/.pitboard/state.json"
                ]
            ),
            Some(Heard::Written)
        );
        for kind in [
            EventKind::Create(CreateKind::File),
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
            EventKind::Remove(RemoveKind::File),
            EventKind::Any,
        ] {
            assert_eq!(
                of(kind, &["/var/folders/x/.pitboard/state.json"]),
                Some(Heard::Written),
                "{kind:?}"
            );
        }
        assert_eq!(
            of(into, &["/var/folders/x/.pitboard/.state.json.7.0.pitboard"]),
            None
        );
        assert_eq!(of(into, &["/var/folders/x/.pitboard/told.json"]), None);
    }

    /// Opening or closing a file reads it, which inotify tells as it tells a write.
    #[test]
    fn a_read_is_not_a_write() {
        for kind in [
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            EventKind::Access(AccessKind::Close(AccessMode::Read)),
            EventKind::Access(AccessKind::Close(AccessMode::Write)),
        ] {
            assert_eq!(
                of(kind, &["/var/folders/x/.pitboard/state.json"]),
                None,
                "{kind:?}"
            );
        }
    }

    /// The folder removed or moved away ends the watch, as inotify names it and as FSEvents
    /// does; anything else of the folder's own, such as its time, says nothing.
    #[test]
    fn the_folder_going_ends_the_watch() {
        assert_eq!(
            of(
                EventKind::Remove(RemoveKind::Folder),
                &["/var/folders/x/.pitboard"]
            ),
            Some(Heard::Ended)
        );
        assert_eq!(
            of(
                EventKind::Modify(ModifyKind::Name(RenameMode::From)),
                &["/var/folders/x/.pitboard"]
            ),
            Some(Heard::Ended)
        );
        assert_eq!(
            of(
                EventKind::Remove(RemoveKind::Folder),
                &["/private/var/folders/x/.pitboard"]
            ),
            Some(Heard::Ended),
            "FSEvents' path"
        );
        assert_eq!(
            of(
                EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
                &["/var/folders/x/.pitboard"]
            ),
            None
        );
        assert_eq!(
            folder().heard(Err(notify::Error::generic("unreadable"))),
            Some(Heard::Ended),
            "a system that cannot say what it saw"
        );
    }

    /// An overflow, or events FSEvents dropped, may have hidden a write, and the watch goes on.
    #[test]
    fn what_the_system_dropped_may_have_been_a_write() {
        let rescan = Event::new(EventKind::Other).set_flag(Flag::Rescan);
        assert_eq!(folder().heard(Ok(rescan)), Some(Heard::Missed));
    }
}
