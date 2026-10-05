//! Claude Desktop's key, `Claude Safe Storage`, read from the login keychain through
//! `/usr/bin/security`. What each answer means for live usage is the provider's, in
//! `provider/desktop/safe_storage.rs`; this is only how macOS is asked.

use crate::context::Context;
use crate::host::SECURITY;
use crate::provider::desktop::safe_storage::{
    ITEM_ACCOUNT, ITEM_SERVICE, ItemStamp, KeyRead, KeyReadError, SafeStorage,
};
use std::path::Path;
use std::process::{Command, ExitStatus, Output};
use std::time::Duration;
use zeroize::Zeroizing;

/// The keychain, through `/usr/bin/security`.
#[derive(Debug)]
pub(super) struct SecurityCli;

impl SafeStorage for SecurityCli {
    fn stamp(&self, _ctx: &Context) -> Result<ItemStamp, KeyReadError> {
        never_from_a_test();
        stamp_with(Path::new(SECURITY))
    }

    fn password(&self, _ctx: &Context, how: KeyRead) -> Result<Zeroizing<Vec<u8>>, KeyReadError> {
        never_from_a_test();
        password_with(Path::new(SECURITY), how.limit())
    }
}

/// The item is somebody's real key, so a test that reaches it has forgotten to script it,
/// and stops before `security` runs. That covers the integration tests and the command line
/// they run, which build this crate with `test-support` rather than as a test.
fn never_from_a_test() {
    #[cfg(any(test, feature = "test-support"))]
    panic!("a test reached Claude's real key; script it with ScriptedSafeStorage");
}

/// `security` exits with the low byte of the `OSStatus`.
const INTERACTION_NOT_ALLOWED: i32 = 36;
const ITEM_NOT_FOUND: i32 = 44;
const AUTH_FAILED: i32 = 51;
const USER_CANCELED: i32 = 128;

/// Reading the attributes never asks: the register's `security_attributes_never_prompt`,
/// measured by experiment U-K5. Five seconds is the ceiling for a locked keychain.
const STAMP_LIMIT: Duration = Duration::from_secs(5);

fn find(with_password: bool) -> Vec<&'static str> {
    let mut args = vec!["find-generic-password"];
    if with_password {
        args.push("-w");
    }
    args.extend(["-s", ITEM_SERVICE]);
    if let Some(account) = ITEM_ACCOUNT {
        args.extend(["-a", account]);
    }
    args
}

fn run(program: &Path, args: &[&str], limit: Duration) -> Result<Output, KeyReadError> {
    let mut command = Command::new(program);
    command.args(args);
    super::helper::output_within(command, b"", limit).map_err(|e| unanswered(program, &e))
}

/// How `security` exited, what it printed, wiped when dropped, and what it said on stderr.
type Secret = (ExitStatus, Zeroizing<Vec<u8>>, Vec<u8>);

/// [`run`] for the password, whose output is read into memory that is wiped when dropped.
fn run_secret(program: &Path, args: &[&str], limit: Duration) -> Result<Secret, KeyReadError> {
    let mut command = Command::new(program);
    command.args(args);
    super::helper::secret_within(command, limit).map_err(|e| unanswered(program, &e))
}

/// What a `security` that could not be run, or did not finish in time, comes to.
fn unanswered(program: &Path, e: &std::io::Error) -> KeyReadError {
    if e.kind() == std::io::ErrorKind::TimedOut {
        KeyReadError::TimedOut
    } else {
        KeyReadError::Other(format!("{} did not answer: {e}", program.display()))
    }
}

fn classify(status: ExitStatus, stderr: &[u8]) -> Result<(), KeyReadError> {
    match status.code() {
        Some(0) => Ok(()),
        Some(INTERACTION_NOT_ALLOWED) => Err(KeyReadError::NoGui),
        Some(USER_CANCELED) => Err(KeyReadError::Denied),
        Some(AUTH_FAILED) => Err(KeyReadError::AuthFailed),
        Some(ITEM_NOT_FOUND) => Err(KeyReadError::Missing),
        Some(code) => Err(KeyReadError::Other(format!(
            "security exited {code}: {}",
            String::from_utf8_lossy(stderr).trim()
        ))),
        None => Err(KeyReadError::Other("security exited on a signal".into())),
    }
}

fn stamp_with(program: &Path) -> Result<ItemStamp, KeyReadError> {
    let out = run(program, &find(false), STAMP_LIMIT)?;
    classify(out.status, &out.stderr)?;
    let listing = String::from_utf8_lossy(&out.stdout);
    match (date_of(&listing, "cdat"), date_of(&listing, "mdat")) {
        (Some(cdat), Some(mdat)) => Ok(ItemStamp { cdat, mdat }),
        _ => Err(KeyReadError::Other(
            "security listed the item without its dates".into(),
        )),
    }
}

fn password_with(program: &Path, limit: Duration) -> Result<Zeroizing<Vec<u8>>, KeyReadError> {
    let (status, mut password, stderr) = run_secret(program, &find(true), limit)?;
    classify(status, &stderr)?;
    // `-w` ends the password with a newline, which is not part of it.
    if password.last() == Some(&b'\n') {
        password.pop();
    }
    if password.is_empty() {
        return Err(KeyReadError::Other("security gave an empty key".into()));
    }
    Ok(password)
}

/// One date attribute of a `find-generic-password` listing, whose line reads
/// `    "cdat"<timedate>=0x3230...  "20260727084307Z\000"`.
fn date_of(listing: &str, attribute: &str) -> Option<String> {
    let label = format!("\"{attribute}\"<timedate>=");
    let line = listing
        .lines()
        .map(str::trim_start)
        .find(|line| line.starts_with(&label))?;
    let quoted = line.rsplit_once("  \"").map(|(_, rest)| rest)?;
    let date = quoted.strip_suffix('"')?;
    let date = date.strip_suffix("\\000").unwrap_or(date);
    (!date.is_empty()).then(|| date.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A stand-in for `security` in a scratch directory, which prints `stdout` and exits
    /// `code`. The real program is never run here: the item is somebody's real key.
    struct Fake(PathBuf);

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
        }
    }

    fn fake(name: &str, script: &str) -> Fake {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-safe-storage-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("security");
        // Written by a child, never this process: a descriptor open for writing here would
        // be inherited by a fork on another test's thread, and running the program while
        // that child lives fails with ETXTBSY, as `write_program` in the CLI's tests found.
        let status = Command::new("/bin/sh")
            .args(["-c", "printf %s \"$2\" > \"$1\" && chmod 755 \"$1\"", "sh"])
            .arg(&program)
            .arg(format!("#!/bin/sh\n{script}"))
            .status()
            .unwrap();
        assert!(status.success(), "could not write {}", program.display());
        Fake(program)
    }

    const LISTING: &str = r#"keychain: "/Users/someone/Library/Keychains/login.keychain-db"
version: 512
class: "genp"
attributes:
    0x00000007 <blob>="Claude Safe Storage"
    "acct"<blob>="Claude Key"
    "cdat"<timedate>=0x32303236303732373038343330375A00  "20260727084307Z\000"
    "mdat"<timedate>=0x32303236303732373038343330375A00  "20260801120000Z\000"
    "svce"<blob>="Claude Safe Storage"
"#;

    #[test]
    fn the_stamp_is_read_from_the_attributes() {
        let program = fake("stamp", &format!("cat <<'EOF'\n{LISTING}EOF\n"));
        assert_eq!(
            stamp_with(&program.0).unwrap(),
            ItemStamp {
                cdat: "20260727084307Z".into(),
                mdat: "20260801120000Z".into(),
            }
        );
    }

    /// The listing is asked for without `-w` or `-g`, so it never makes macOS ask.
    #[test]
    fn the_stamp_never_asks_for_the_password() {
        assert!(!find(false).contains(&"-w"));
        assert!(!find(false).contains(&"-g"));
        assert!(find(true).contains(&"-w"));
        assert_eq!(&find(false)[1..3], ["-s", "Claude Safe Storage"]);
    }

    #[test]
    fn the_password_loses_only_its_newline() {
        let program = fake("password", "printf 'not-a-real-password\\n'\n");
        let password = password_with(&program.0, Duration::from_secs(5)).unwrap();
        assert_eq!(password.as_slice(), b"not-a-real-password");
    }

    /// Each exit `security` is known to give is read as what it means, and 36 is not a
    /// refusal: it says only that nobody could have been asked.
    #[test]
    fn each_exit_means_what_security_says() {
        for (code, expected) in [
            (36, KeyReadError::NoGui),
            (128, KeyReadError::Denied),
            (51, KeyReadError::AuthFailed),
            (44, KeyReadError::Missing),
        ] {
            let program = fake(&format!("exit-{code}"), &format!("exit {code}\n"));
            assert_eq!(
                password_with(&program.0, Duration::from_secs(5)).unwrap_err(),
                expected,
                "exit {code}"
            );
            assert_eq!(stamp_with(&program.0).unwrap_err(), expected, "exit {code}");
        }
        let program = fake("exit-1", "echo 'something else' >&2\nexit 1\n");
        assert!(matches!(
            password_with(&program.0, Duration::from_secs(5)),
            Err(KeyReadError::Other(detail)) if detail.contains("something else")
        ));
    }

    /// A question nobody answers is closed when its time is up, and the read gives up.
    #[test]
    fn a_question_nobody_answers_times_out() {
        let program = fake("silent", "exec sleep 30\n");
        let started = std::time::Instant::now();
        assert_eq!(
            password_with(&program.0, Duration::from_millis(300)).unwrap_err(),
            KeyReadError::TimedOut
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
