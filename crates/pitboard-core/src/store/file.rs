//! The plaintext backend Claude Code falls back to on macOS and always uses elsewhere.

use super::{Backend, Error, RawStore};
use crate::atomic;
use crate::context::Context;
use std::path::PathBuf;

/// Claude Code keeps exactly one plaintext credential per storage directory, so the service
/// name selects nothing here.
pub(super) struct PlainFile {
    path: PathBuf,
}

impl PlainFile {
    /// The file Claude Code reads.
    pub(super) fn live(ctx: &Context) -> PlainFile {
        PlainFile {
            path: super::credential_file(ctx),
        }
    }

    fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

impl RawStore for PlainFile {
    fn kind(&self) -> Backend {
        Backend::File
    }

    fn contains(&self, _service: &str) -> Result<bool, Error> {
        super::exists(&self.path())
    }

    fn read(&self, _service: &str) -> Result<Option<String>, Error> {
        match std::fs::read_to_string(self.path()) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::Unreadable(format!(
                "cannot read {}: {e}",
                self.path().display()
            ))),
        }
    }

    fn write(&self, service: &str, contents: &str) -> Result<(), Error> {
        let path = self.path();
        atomic::write(&path, contents.as_bytes(), atomic::Perms::Secret)
            .map_err(|e| Error::Write(format!("cannot write {}: {e}", path.display())))?;
        match self.read(service)? {
            Some(back) if back == contents => Ok(()),
            _ => Err(Error::NotDurable(format!(
                "{} does not hold what was written",
                path.display()
            ))),
        }
    }

    fn delete(&self, _service: &str) -> Result<(), Error> {
        match std::fs::remove_file(self.path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Write(e.to_string())),
        }
    }
}
