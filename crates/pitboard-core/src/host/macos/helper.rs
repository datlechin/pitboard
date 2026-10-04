//! Running a helper program with a deadline. A helper that never answers must not hold
//! Pitboard's lock, or an app's worker, forever.
//!
//! macOS is the system whose answers come from helpers: `security` for the keychain, `ps`
//! for the process list, and `sqlite3` and `plutil` for what Claude Desktop keeps. Linux
//! reads `/proc` and has no keychain to call.

use std::{
    io::{self, Read, Write},
    process::{ChildStdout, Command, ExitStatus, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

/// `command`'s output, with `input` on its stdin. Past `limit` the process is killed and the
/// answer is `TimedOut`. Its output is drained on other threads, so a chatty helper cannot
/// fill a pipe and stall.
pub(super) fn output_within(command: Command, input: &[u8], limit: Duration) -> io::Result<Output> {
    let (status, stdout, stderr) = run_within(command, input, limit, |pipe| {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// The most of a secret [`secret_within`] keeps. A password is far shorter; anything past
/// this is read and thrown away, so the helper never stalls on a full pipe.
const SECRET_LIMIT: usize = 4096;

/// [`output_within`] for a helper that prints a secret, with nothing on its stdin. Its
/// stdout is read into one buffer sized up front, which never grows and so leaves no copy
/// behind, and which is wiped when it is dropped.
pub(super) fn secret_within(
    command: Command,
    limit: Duration,
) -> io::Result<(ExitStatus, Zeroizing<Vec<u8>>, Vec<u8>)> {
    run_within(command, b"", limit, |pipe| {
        let mut bytes = Zeroizing::new(Vec::with_capacity(SECRET_LIMIT));
        let _ = pipe.take(SECRET_LIMIT as u64).read_to_end(&mut bytes);
        let _ = io::copy(pipe, &mut io::sink());
        bytes
    })
}

/// Runs `command` with `input` on its stdin, past `limit` kills it, and reads its stdout
/// with `read`, on another thread like its stderr.
fn run_within<T: Default + Send + 'static>(
    mut command: Command,
    input: &[u8],
    limit: Duration,
    read: fn(&mut ChildStdout) -> T,
) -> io::Result<(ExitStatus, T, Vec<u8>)> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> thread::JoinHandle<Vec<u8>> {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    }
    let stdout = child.stdout.take();
    let stdout = thread::spawn(move || stdout.map(|mut pipe| read(&mut pipe)).unwrap_or_default());
    let stderr = drain(child.stderr.take());
    // Dropping stdin closes it, which is how a helper reading commands learns there are no
    // more. A helper that exits without reading is not an error.
    if let Some(mut stdin) = child.stdin.take() {
        match stdin.write_all(input) {
            Err(e) if e.kind() != io::ErrorKind::BrokenPipe => return Err(e),
            _ => {}
        }
    }

    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("no answer within {}s", limit.as_secs()),
            ));
        }
        thread::sleep(Duration::from_millis(5));
    };
    Ok((
        status,
        stdout.join().unwrap_or_default(),
        stderr.join().unwrap_or_default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A secret is kept whole and no larger than the buffer sized for it, and a helper that
    /// prints more than that still finishes rather than stalling on a full pipe.
    #[test]
    fn a_secret_is_read_into_one_buffer_that_never_grows() {
        let mut short = Command::new("/bin/sh");
        short.args(["-c", "printf 'not-a-real-password'; echo oops >&2; exit 3"]);
        let (status, secret, stderr) = secret_within(short, Duration::from_secs(5)).unwrap();
        assert_eq!(status.code(), Some(3));
        assert_eq!(secret.as_slice(), b"not-a-real-password");
        assert_eq!(secret.capacity(), SECRET_LIMIT);
        assert_eq!(stderr, b"oops\n");

        let mut long = Command::new("/bin/sh");
        long.args(["-c", "head -c 200000 /dev/zero"]);
        let (status, secret, _) = secret_within(long, Duration::from_secs(5)).unwrap();
        assert!(status.success());
        assert_eq!(secret.len(), SECRET_LIMIT);
        assert_eq!(secret.capacity(), SECRET_LIMIT);
    }

    #[test]
    fn a_prompt_answer_is_returned_whole() {
        let mut cat = Command::new("cat");
        cat.arg("-");
        let out = output_within(cat, b"hello", Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hello");
    }

    #[test]
    fn a_helper_that_never_answers_is_stopped_at_the_deadline() {
        let mut sleep = Command::new("sleep");
        sleep.arg("30");
        let started = Instant::now();
        let err = output_within(sleep, b"", Duration::from_millis(200)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_large_answer_cannot_stall_the_helper() {
        let mut yes = Command::new("head");
        yes.args(["-c", "1000000", "/dev/zero"]);
        let out = output_within(yes, b"", Duration::from_secs(10)).unwrap();
        assert_eq!(out.stdout.len(), 1_000_000);
    }
}
