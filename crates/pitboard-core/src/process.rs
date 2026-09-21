//! Running a helper program with a deadline. A helper that never answers must not hold
//! pitboard's lock, or an app's worker, forever.

use std::io::{self, Read, Write};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// `command`'s output, with `input` on its stdin. Past `limit` the process is killed and the
/// answer is `TimedOut`. Its output is drained on other threads, so a chatty helper cannot
/// fill a pipe and stall.
pub fn output_within(mut command: Command, input: &[u8], limit: Duration) -> io::Result<Output> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
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
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
