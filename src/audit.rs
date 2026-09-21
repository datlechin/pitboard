//! One line per change pitboard makes. Labels, codes and times only — no email addresses or
//! account identifiers — so it is safe to paste into a bug report.

use crate::{home, time};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

/// Rotated once past this size, keeping one previous file.
const LIMIT_BYTES: u64 = 256 * 1024;

fn path() -> PathBuf {
    home::dir().join("audit.log")
}

/// A failure to audit never fails the operation it describes.
pub fn record(verb: &str, subject: &str, outcome: &str) {
    let _ = append(&line(time::now(), verb, subject, outcome));
}

fn line(at: i64, verb: &str, subject: &str, outcome: &str) -> String {
    let clean = |s: &str| s.replace(['\n', '\r', '\t'], " ");
    format!(
        "{}\t{}\t{}\t{}\n",
        time::format_local(at, "%Y-%m-%dT%H:%M:%S%z"),
        clean(verb),
        clean(subject),
        clean(outcome)
    )
}

fn append(line: &str) -> std::io::Result<()> {
    home::ensure()?;
    let path = path();
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LIMIT_BYTES) {
        std::fs::rename(&path, path.with_extension("log.1"))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)?
        .write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_one_line_whatever_the_input() {
        let l = line(1_789_935_600, "use", "work\ninjected", "ok");
        assert_eq!(
            l.matches('\n').count(),
            1,
            "a label must not be able to forge entries"
        );
        assert_eq!(l.split('\t').count(), 4);
    }
}
