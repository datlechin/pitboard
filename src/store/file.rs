//! The plaintext backend Claude Code falls back to, and always uses off macOS.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub fn read(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Write through a sibling temp file so a reader never sees a half-written credential.
pub fn write(path: &Path, contents: &str) -> Result<(), String> {
    let dir = path.parent().ok_or("credential path has no parent")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".credentials.{}.tmp", std::process::id()));

    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn writes_are_atomic_and_private() {
        let dir = std::env::temp_dir().join(format!("pitboard-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(".credentials.json");
        write(&p, "{\"a\":1}").unwrap();
        assert_eq!(read(&p).unwrap().as_deref(), Some("{\"a\":1}"));
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        write(&p, "{\"a\":2}").unwrap();
        assert_eq!(read(&p).unwrap().as_deref(), Some("{\"a\":2}"));
        assert!(
            std::fs::read_dir(&dir).unwrap().count() == 1,
            "no temp file left behind"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        assert_eq!(
            read(Path::new("/nonexistent/pitboard/x.json")).unwrap(),
            None
        );
    }
}
