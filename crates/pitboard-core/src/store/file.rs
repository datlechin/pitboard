//! One file as a credential store. Claude Code falls back to one on macOS and has only one
//! on Linux; Codex keeps its whole login in one.

use super::{Backend, Error, RawStore};
use crate::atomic;
use crate::permit::Permit;
use std::path::PathBuf;

/// A tool that keeps its login in a file keeps exactly one per directory, so the service
/// name selects nothing here.
pub(crate) struct PlainFile {
    path: PathBuf,
}

impl PlainFile {
    pub(crate) fn at(path: PathBuf) -> PlainFile {
        PlainFile { path }
    }
}

impl RawStore for PlainFile {
    fn kind(&self) -> Backend {
        Backend::File
    }

    fn contains(&self, _service: &str) -> Result<bool, Error> {
        super::exists(&self.path)
    }

    fn read(&self, _service: &str) -> Result<Option<String>, Error> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::Unreadable(format!(
                "cannot read {}: {e}",
                self.path.display()
            ))),
        }
    }

    fn write(&self, permit: Permit, service: &str, contents: &str) -> Result<(), Error> {
        let path = &self.path;
        // Windows keeps the access list the tool's file has (`fs::copy_access`).
        let perms = match crate::host::OS {
            crate::host::Os::MacOs | crate::host::Os::Linux => atomic::Perms::Secret,
            crate::host::Os::Windows => atomic::Perms::MatchExisting,
        };
        // A tool that never ran has no folder yet, which its own sign-in would make.
        if let Some(folder) = path.parent()
            && !folder.exists()
        {
            crate::host::fs::create_private_dir(permit, folder)
                .map_err(|e| Error::Write(format!("cannot make {}: {e}", folder.display())))?;
        }
        atomic::write(permit, path, contents.as_bytes(), perms)
            .map_err(|e| Error::Write(format!("cannot write {}: {e}", path.display())))?;
        match self.read(service)? {
            Some(back) if back == contents => Ok(()),
            _ => Err(Error::NotDurable(format!(
                "{} does not hold what was written",
                path.display()
            ))),
        }
    }

    fn delete(&self, permit: Permit, _service: &str) -> Result<(), Error> {
        match crate::host::fs::remove_file(permit, &self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Write(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A tool that never ran has no folder yet, which the first account put in use needs.
    #[test]
    fn a_login_written_where_the_tools_folder_is_missing_makes_it_private() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let folder = scratch.path().join(".claude");
        let file = PlainFile::at(folder.join(".credentials.json"));
        file.write(Permit::for_a_test(), "", r#"{"claudeAiOauth":{}}"#)
            .expect("written");
        assert_eq!(
            file.read("").expect("read").as_deref(),
            Some(r#"{"claudeAiOauth":{}}"#)
        );
        let access = crate::host::fs::access(&folder).expect("its access");
        assert!(!access.shared, "{}", access.described);
    }
}
