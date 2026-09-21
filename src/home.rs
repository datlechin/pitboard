//! pitboard's own directory, held at 0700 rather than left to the umask: park file names
//! contain account identifiers, so on a shared machine listing it would be a leak.

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
    std::fs::create_dir_all(&path)?;
    restrict(&path)?;
    Ok(path)
}

pub fn restrict(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
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
    fn ensure_creates_the_directory_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = std::env::temp_dir().join(format!("pitboard-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("nested")).unwrap();
        restrict(&scratch).unwrap();
        let mode = std::fs::metadata(&scratch).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o700,
            "the home directory must not be listable by others"
        );
        std::fs::remove_dir_all(&scratch).unwrap();
    }
}
