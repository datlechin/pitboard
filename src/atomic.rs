//! The one durable write, for every file that must survive an interrupted run. The directory
//! is synced after the rename because ext4 and xfs can lose a rename across a crash even
//! when the contents were synced.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

pub enum Perms {
    /// 0600 regardless of umask or of what is already there.
    Secret,
    /// Exactly the mode of the file at the path now, tighter or looser than 0600. `Secret`
    /// when nothing is there yet, or when the path is a symlink.
    MatchExisting,
}

pub fn write(path: &Path, contents: &[u8], perms: Perms) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    std::fs::create_dir_all(dir)?;

    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let temp = dir.join(format!(
        ".{}.{}.pitboard",
        name.to_string_lossy(),
        std::process::id()
    ));

    let result = (|| -> io::Result<()> {
        // Created private and given the existing mode afterwards: too closed for a moment is
        // safe, too open is not. A symlink's own mode says nothing about its target, and the
        // rename replaces the link rather than following it, as Claude Code's writes do.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        if let Perms::MatchExisting = perms
            && let Ok(existing) = std::fs::symlink_metadata(path)
            && !existing.file_type().is_symlink()
        {
            let mode = existing.permissions().mode() & 0o777;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(mode))?;
        }
        std::fs::rename(&temp, path)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
        return result;
    }
    // Best effort: the data is already durable, this makes the rename durable too.
    let _ = File::open(dir).and_then(|d| d.sync_all());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pitboard-atomic-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_secret_is_private_however_the_umask_is_set() {
        let dir = scratch("secret");
        let path = dir.join("state.json");
        write(&path, b"{}", Perms::Secret).unwrap();
        assert_eq!(mode_of(&path), 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn replacing_leaves_no_temp_file_behind() {
        let dir = scratch("replace");
        let path = dir.join("f.json");
        write(&path, b"one", Perms::Secret).unwrap();
        write(&path, b"two", Perms::Secret).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn match_existing_keeps_a_file_as_open_as_it_already_was() {
        let dir = scratch("match");
        let path = dir.join("claude.json");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write(&path, b"new", Perms::MatchExisting).unwrap();
        assert_eq!(
            mode_of(&path),
            0o644,
            "tightening another tool's file would surprise it"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn match_existing_keeps_a_file_as_closed_as_it_already_was() {
        let dir = scratch("tighter");
        let path = dir.join("claude.json");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
        write(&path, b"new", Perms::MatchExisting).unwrap();
        assert_eq!(
            mode_of(&path),
            0o400,
            "loosening another tool's file is as wrong as tightening it"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn match_existing_defaults_to_private_when_nothing_is_there() {
        let dir = scratch("fresh");
        let path = dir.join("new.json");
        write(&path, b"{}", Perms::MatchExisting).unwrap();
        assert_eq!(mode_of(&path), 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_symlinked_destination_does_not_lend_its_permissions() {
        let dir = scratch("symlink");
        let target = dir.join("elsewhere");
        std::fs::write(&target, b"x").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o666)).unwrap();
        let path = dir.join("link.json");
        std::os::unix::fs::symlink(&target, &path).unwrap();

        write(&path, b"{}", Perms::MatchExisting).unwrap();
        assert_eq!(
            mode_of(&path),
            0o600,
            "a symlink's own mode is 0777 and must never be borrowed"
        );
        assert!(
            !std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the rename replaces the link rather than writing through it"
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"x",
            "the link target must be untouched"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
