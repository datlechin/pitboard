//! One line per change pitboard makes: when, which front end asked, what was asked, of
//! what, and how it ended. Labels, codes and times only, no email addresses or account
//! identifiers, so it is safe to paste into a bug report.

use crate::context::Context;
use crate::{home, time};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

/// Rotated once past this size, keeping one previous file.
const LIMIT_BYTES: u64 = 256 * 1024;

fn path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("audit.log")
}

/// A failure to audit never fails the operation it describes.
pub fn record(ctx: &Context, verb: &str, subject: &str, outcome: &str) {
    let _ = append(ctx, &line(ctx.now(), &ctx.caller, verb, subject, outcome));
}

fn line(at: i64, caller: &str, verb: &str, subject: &str, outcome: &str) -> String {
    let clean = |s: &str| s.replace(['\n', '\r', '\t'], " ");
    format!(
        "{}\t{}\t{}\t{}\t{}\n",
        time::local(at, "%Y-%m-%dT%H:%M:%S%:z"),
        clean(caller),
        clean(verb),
        clean(subject),
        clean(outcome)
    )
}

fn append(ctx: &Context, line: &str) -> std::io::Result<()> {
    // Never makes the home itself. Every change settles first, which makes it; and after an
    // uninstall there is no home to write into and nothing left to describe.
    if !std::fs::metadata(home::dir(ctx)).is_ok_and(|m| m.is_dir()) {
        return Ok(());
    }
    let path = path(ctx);
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

/// One recorded change, as `read` hands it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Local time, as it was written.
    pub at: String,
    /// Which front end asked. Lines written before this was recorded say `unknown`.
    pub caller: String,
    pub verb: String,
    pub subject: String,
    /// `ok`, or the stable code of whatever stopped it.
    pub outcome: String,
}

/// The newest `limit` changes, oldest first. The rotated file is read too, so asking for
/// more than the current file holds still answers.
pub fn read(ctx: &Context, limit: usize) -> Vec<Entry> {
    let read = |p: PathBuf| std::fs::read_to_string(p).unwrap_or_default();
    let mut text = read(path(ctx).with_extension("log.1"));
    text.push_str(&read(path(ctx)));
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    lines[lines.len().saturating_sub(limit)..]
        .iter()
        .map(|line| {
            let mut fields = line.split('\t');
            let mut next = || fields.next().unwrap_or_default().to_string();
            let (at, second, third, fourth) = (next(), next(), next(), next());
            match fields.next() {
                // Written before the caller had a column of its own.
                None => Entry {
                    at,
                    caller: "unknown".into(),
                    verb: second,
                    subject: third,
                    outcome: fourth,
                },
                Some(outcome) => Entry {
                    at,
                    caller: second,
                    verb: third,
                    subject: fourth,
                    outcome: outcome.to_string(),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines written before the caller had a column of its own still read, because a log is
    /// only useful if the version that wrote it does not matter.
    #[test]
    fn a_line_from_before_the_caller_column_still_reads() {
        let four = "2026-09-22T01:00:00+07:00\tuse\twork\tok";
        let five = "2026-09-22T01:01:00+07:00\tapp\tuse\tpersonal\tok";
        let home = tempdir("audit-shapes");
        let ctx = Context::new(home.clone()).with_pitboard_home(home.clone());
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("audit.log"), format!("{four}\n{five}\n")).unwrap();

        let entries = read(&ctx, 10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].caller, "unknown");
        assert_eq!(entries[0].verb, "use");
        assert_eq!(entries[0].subject, "work");
        assert_eq!(entries[0].outcome, "ok");
        assert_eq!(entries[1].caller, "app");
        assert_eq!(entries[1].verb, "use");
        assert_eq!(entries[1].outcome, "ok");
    }

    /// The seam the rest of this crate's time judgements hang on: what gets written is the
    /// context's idea of now, not the machine's.
    #[test]
    fn a_change_is_stamped_with_the_contexts_clock() {
        use crate::time::FixedClock;
        use std::sync::Arc;

        let home = tempdir("audit-clock");
        std::fs::create_dir_all(&home).unwrap();
        let clock = Arc::new(FixedClock::at(1_760_000_000));
        let ctx = Context::new(home.clone())
            .with_pitboard_home(home.clone())
            .with_caller("cli".into())
            .with_clock(clock.clone());

        record(&ctx, "use", "work", "ok");
        clock.advance(3600);
        record(&ctx, "use", "personal", "ok");

        let entries = read(&ctx, 10);
        assert_eq!(entries.len(), 2);
        let stamp = |epoch| time::local(epoch, "%Y-%m-%dT%H:%M:%S%:z");
        assert_eq!(entries[0].at, stamp(1_760_000_000));
        assert_eq!(
            entries[1].at,
            stamp(1_760_003_600),
            "an hour later, because we said so"
        );
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pitboard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_line_is_one_line_whatever_the_input() {
        let l = line(1_789_935_600, "cli", "use", "work\ninjected", "ok");
        assert_eq!(
            l.matches('\n').count(),
            1,
            "a label must not be able to forge entries"
        );
        assert_eq!(l.split('\t').count(), 5);
    }
}
