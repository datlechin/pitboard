//! pitboard's own directory. Every directory pitboard creates is 0700, whatever the umask:
//! park file names contain account identifiers, so on a shared machine a listing would leak.

use crate::error::{Error, Result};
use std::io;
use std::path::{Path, PathBuf};

/// Directory names that mean a sync client would copy this to another machine, where a
/// parked login must never go.
const SYNCED: [&str; 5] = [
    "Dropbox",
    "Google Drive",
    "OneDrive",
    "com~apple~CloudDocs",
    "Sync",
];

pub fn dir() -> PathBuf {
    std::env::var_os("PITBOARD_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".pitboard")
        })
}

pub fn ensure() -> io::Result<PathBuf> {
    let path = dir();
    create_private(&path)?;
    Ok(path)
}

/// Create `path` and any missing parent as 0700. A directory that already exists keeps its
/// mode: it may be one the user named, and `pitboard doctor` reports it if others can read it.
pub fn create_private(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

pub fn check_location(path: &Path) -> Result<()> {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy();
    match SYNCED.iter().find(|marker| text.contains(**marker)) {
        Some(marker) => Err(Error::StateOnSyncedDrive {
            path: resolved.clone(),
            marker: (*marker).to_string(),
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloud_synced_location_is_refused() {
        for synced in [
            "/Users/x/Library/Mobile Documents/com~apple~CloudDocs/pitboard",
            "/home/x/Dropbox/pitboard",
            "/home/x/OneDrive/tools/pitboard",
        ] {
            assert!(check_location(Path::new(synced)).is_err(), "{synced}");
        }
        assert!(check_location(Path::new("/Users/x/.pitboard")).is_ok());
        assert!(check_location(Path::new("/home/x/.pitboard")).is_ok());
    }

    #[test]
    fn created_directories_are_private_and_existing_ones_keep_their_mode() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let scratch = std::env::temp_dir().join(format!("pitboard-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);

        create_private(&scratch.join("nested")).unwrap();
        assert_eq!(
            mode(&scratch),
            0o700,
            "a created parent must be private too"
        );
        assert_eq!(mode(&scratch.join("nested")), 0o700);

        std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o755)).unwrap();
        create_private(&scratch).unwrap();
        assert_eq!(
            mode(&scratch),
            0o755,
            "a directory the user named is theirs to set"
        );
        std::fs::remove_dir_all(&scratch).unwrap();
    }
}
