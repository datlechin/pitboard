// Another process for a test: this test program run again as the one test
// `helper::tests::a_helper_serves`, told its job and its file through its environment. It
// says so on its standard output once its job is under way, and stops once its standard input
// closes.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const JOB: &str = "PITBOARD_TEST_HELPER_JOB";
const FILE: &str = "PITBOARD_TEST_HELPER_FILE";
// The helper a `Starts` helper starts: its program, its job and its file.
#[cfg(windows)]
const NEXT: [&str; 3] = [
    "PITBOARD_TEST_HELPER_NEXT",
    "PITBOARD_TEST_HELPER_NEXT_JOB",
    "PITBOARD_TEST_HELPER_NEXT_FILE",
];
// libtest's own, before any argument a test hands the helper.
const ITS_OWN: [&str; 4] = [
    "helper::tests::a_helper_serves",
    "--exact",
    "--nocapture",
    "--test-threads=1",
];
const UNDER_WAY: &str = "pitboard-helper: under way";
const SAID: &str = "pitboard-helper: ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Job {
    // An exclusive lock on the file, as another Pitboard run holds one of its own.
    Locks,
    // The file open to read, sharing its reading and writing but not its deletion, as some
    // editors and scanners open one.
    #[cfg(windows)]
    HoldsWithoutDelete,
    // The file read whole, again and again, as Pitboard reads a file it replaces.
    Reads,
    // A sign-in's address on standard error, then nothing for 30 s, as `codex login` waits.
    SaysAnAddress,
    // Its process id written to the file, then on while `<file>.held` is there.
    #[cfg(windows)]
    Waits,
    // The same, listening on 127.0.0.1:1455 as `codex login` does.
    #[cfg(windows)]
    HoldsPort,
    // The helper `NEXT` names started, then the same as `Waits`; `refused` and the error
    // in the file where it could not be.
    #[cfg(windows)]
    Starts,
    // The same, started with `CREATE_BREAKAWAY_FROM_JOB`, as Codex starts its daemons.
    #[cfg(windows)]
    StartsBreakingAway,
    // Starts `NEXT` the way a tool starts a browser, which inherits this helper's piped
    // stdout and stderr, then exits at once while that child stays, as Codex's browser does.
    #[cfg(windows)]
    StartsAndGoes,
    // Every argument after libtest's own, as JSON.
    #[cfg(windows)]
    Records,
    // `none` where `GetConsoleWindow` gives no window, `some` where it gives one.
    #[cfg(windows)]
    SaysItsConsole,
}

impl Job {
    fn code(self) -> &'static str {
        match self {
            Job::Locks => "locks",
            #[cfg(windows)]
            Job::HoldsWithoutDelete => "holds-without-delete",
            Job::Reads => "reads",
            Job::SaysAnAddress => "says-an-address",
            #[cfg(windows)]
            Job::Waits => "waits",
            #[cfg(windows)]
            Job::HoldsPort => "holds-port",
            #[cfg(windows)]
            Job::Starts => "starts",
            #[cfg(windows)]
            Job::StartsBreakingAway => "starts-breaking-away",
            #[cfg(windows)]
            Job::StartsAndGoes => "starts-and-goes",
            #[cfg(windows)]
            Job::Records => "records",
            #[cfg(windows)]
            Job::SaysItsConsole => "says-its-console",
        }
    }

    fn named(code: &str) -> Option<Job> {
        [
            Job::Locks,
            #[cfg(windows)]
            Job::HoldsWithoutDelete,
            Job::Reads,
            Job::SaysAnAddress,
            #[cfg(windows)]
            Job::Waits,
            #[cfg(windows)]
            Job::HoldsPort,
            #[cfg(windows)]
            Job::Starts,
            #[cfg(windows)]
            Job::StartsBreakingAway,
            #[cfg(windows)]
            Job::StartsAndGoes,
            #[cfg(windows)]
            Job::Records,
            #[cfg(windows)]
            Job::SaysItsConsole,
        ]
        .into_iter()
        .find(|job| job.code() == code)
    }
}

pub(crate) struct Helper {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Helper {
    pub(crate) fn start(job: Job, file: &Path) -> Helper {
        Helper::start_as(
            &std::env::current_exe().expect("this test's program"),
            job,
            file,
        )
    }

    // `program` is a copy of this test's program, under another name.
    pub(crate) fn start_as(program: &Path, job: Job, file: &Path) -> Helper {
        let mut child = command(program, job, file)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the helper starts");
        let stdin = child.stdin.take();
        let mut stdout = BufReader::new(child.stdout.take().expect("its output"));
        let mut line = String::new();
        // libtest names the test on the same line, before the helper speaks.
        while !line.contains(UNDER_WAY) {
            line.clear();
            let read = stdout.read_line(&mut line).expect("the helper's output");
            assert!(read > 0, "the helper ended before its job was under way");
        }
        Helper {
            child,
            stdin,
            stdout,
        }
    }

    #[cfg(windows)]
    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    // What the helper said once it stopped.
    pub(crate) fn stop(mut self) -> String {
        drop(self.stdin.take());
        let mut rest = String::new();
        self.stdout
            .read_to_string(&mut rest)
            .expect("the helper's output");
        let ended = self.child.wait().expect("the helper ends");
        assert!(ended.success(), "the helper failed: {rest}");
        rest.lines()
            .filter_map(|line| line.split_once(SAID).map(|(_, said)| said.to_owned()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// A helper for a test to start as it would a tool.
pub(crate) fn command(program: &Path, job: Job, file: &Path) -> Command {
    let mut command = Command::new(program);
    command.args(ITS_OWN).env(JOB, job.code()).env(FILE, file);
    command
}

// The same, starting `next` with `job` and `file` of its own.
#[cfg(windows)]
pub(crate) fn starting(
    program: &Path,
    breaking_away: bool,
    file: &Path,
    next: (&Path, Job, &Path),
) -> Command {
    let (next, job, next_file) = next;
    let starts = if breaking_away {
        Job::StartsBreakingAway
    } else {
        Job::Starts
    };
    let mut command = command(program, starts, file);
    command
        .env(NEXT[0], next)
        .env(NEXT[1], job.code())
        .env(NEXT[2], next_file);
    command
}

// Starts `next` the way a browser is started, then exits at once while that child stays.
#[cfg(windows)]
pub(crate) fn goes(program: &Path, file: &Path, next: (&Path, Job, &Path)) -> Command {
    let (next, job, next_file) = next;
    let mut command = command(program, Job::StartsAndGoes, file);
    command
        .env(NEXT[0], next)
        .env(NEXT[1], job.code())
        .env(NEXT[2], next_file);
    command
}

// What a helper wrote whole at `file`, waited for until it has.
#[cfg(windows)]
pub(crate) fn written(file: &Path) -> String {
    let asked = std::time::Instant::now();
    loop {
        if let Ok(said) = std::fs::read_to_string(file) {
            return said;
        }
        assert!(
            asked.elapsed() < std::time::Duration::from_secs(20),
            "nothing was written at {}",
            file.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(windows)]
#[allow(
    clippy::disallowed_methods,
    reason = "a helper writes what it says as a tool writes its files"
)]
fn write_whole(file: &Path, said: &str) {
    let part = file.with_extension("part");
    std::fs::write(&part, said).expect("written");
    std::fs::rename(&part, file).expect("moved into place");
}

#[cfg(windows)]
fn held(file: &Path) -> std::path::PathBuf {
    let mut held = file.as_os_str().to_owned();
    held.push(".held");
    std::path::PathBuf::from(held)
}

// What keeps a waiting helper going, made before it starts and taken away to let it go.
#[cfg(windows)]
#[allow(
    clippy::disallowed_methods,
    reason = "a scratch file only its presence says anything with"
)]
pub(crate) fn hold(file: &Path) -> Held {
    let held = held(file);
    std::fs::write(&held, "").expect("a file to wait on");
    Held(held)
}

#[cfg(windows)]
pub(crate) struct Held(std::path::PathBuf);

#[cfg(windows)]
impl Drop for Held {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(windows)]
fn waits(file: &Path) {
    write_whole(file, &std::process::id().to_string());
    while held(file).exists() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

// A folder on an exFAT volume, which keeps no POSIX rename and no access list. On CI,
// test-as-standard-user.ps1's -ExFat mounts one and names it in PITBOARD_TEST_VOLUME.
#[cfg(windows)]
#[allow(
    clippy::disallowed_methods,
    reason = "CI names the volume in the test's environment"
)]
pub(crate) fn exfat(name: &str) -> std::path::PathBuf {
    let volume = std::env::var_os("PITBOARD_TEST_VOLUME")
        .expect("PITBOARD_TEST_VOLUME names a folder on an exFAT volume");
    let folder = Path::new(&volume).join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

// What a reading helper counted: its reads, and of them the whole, torn, missing and failed.
#[cfg(windows)]
pub(crate) fn counts(said: &str) -> [u64; 5] {
    let first = said.lines().next().unwrap_or_default();
    let numbers: Vec<u64> = first
        .split_whitespace()
        .filter_map(|word| word.parse().ok())
        .collect();
    numbers
        .try_into()
        .unwrap_or_else(|_| panic!("a reading helper's counts: {said}"))
}

// A whole record of `round`, as long as a login: the round eight bytes at a time.
pub(crate) fn record(round: u64) -> Vec<u8> {
    round.to_le_bytes().repeat(512)
}

fn whole(read: &[u8]) -> bool {
    read.len() == 4096 && read.chunks(8).all(|chunk| chunk == &read[..8])
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a helper is told its job through its environment, and holds a file as another program does"
)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn a_helper_serves() {
        let (Some(job), Some(file)) = (std::env::var_os(JOB), std::env::var_os(FILE)) else {
            return;
        };
        let job = Job::named(&job.to_string_lossy()).expect("a job this helper knows");
        let file = std::path::PathBuf::from(file);
        let closed = Arc::new(AtomicBool::new(false));
        let watched = Arc::clone(&closed);
        let watching = std::thread::spawn(move || {
            let _ = std::io::stdin().read_to_end(&mut Vec::new());
            watched.store(true, Ordering::Release);
        });
        let said = match job {
            Job::Locks => {
                let held = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&file)
                    .expect("the file to lock");
                held.lock().expect("locked");
                println!("{UNDER_WAY}");
                watching.join().expect("watched");
                String::new()
            }
            #[cfg(windows)]
            Job::HoldsWithoutDelete => {
                use std::os::windows::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
                let _held = std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                    .open(&file)
                    .expect("the file to hold");
                println!("{UNDER_WAY}");
                watching.join().expect("watched");
                String::new()
            }
            Job::Reads => {
                println!("{UNDER_WAY}");
                let (mut reads, mut whole_reads, mut torn, mut missing) = (0u64, 0u64, 0u64, 0u64);
                let mut failed = Vec::new();
                while !closed.load(Ordering::Acquire) {
                    reads += 1;
                    match crate::atomic::read(&file, |file| std::fs::read(file)) {
                        Ok(read) if whole(&read) => whole_reads += 1,
                        Ok(_) => torn += 1,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => missing += 1,
                        Err(e) => failed.push(e.to_string()),
                    }
                }
                let mut said = format!(
                    "reads {reads} whole {whole_reads} torn {torn} missing {missing} failed {}",
                    failed.len()
                );
                failed.sort();
                failed.dedup();
                for each in failed {
                    said.push_str(&format!("\n{SAID}{each}"));
                }
                said
            }
            Job::SaysAnAddress => {
                eprintln!("https://auth.openai.com/");
                std::thread::sleep(std::time::Duration::from_secs(30));
                String::new()
            }
            #[cfg(windows)]
            Job::Waits => {
                waits(&file);
                String::new()
            }
            #[cfg(windows)]
            Job::HoldsPort => {
                let _port = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 1455))
                    .expect("Codex's port");
                waits(&file);
                String::new()
            }
            #[cfg(windows)]
            Job::Starts | Job::StartsBreakingAway | Job::StartsAndGoes => {
                use std::os::windows::process::CommandExt;
                let [next, next_job, next_file] = NEXT.map(|name| {
                    std::env::var_os(name).unwrap_or_else(|| panic!("{name} names the next"))
                });
                let next_job =
                    Job::named(&next_job.to_string_lossy()).expect("a job this helper knows");
                let mut next = command(Path::new(&next), next_job, Path::new(&next_file));
                // Null stdio for the child, as webbrowser gives the browser, but std still sets
                // bInheritHandles, so the child inherits this helper's piped stdout and stderr.
                next.stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if job == Job::StartsBreakingAway {
                    next.creation_flags(
                        windows_sys::Win32::System::Threading::CREATE_BREAKAWAY_FROM_JOB,
                    );
                }
                match next.spawn() {
                    // StartsAndGoes exits at once, the child holding the pipes; the others wait.
                    Ok(_) if job == Job::StartsAndGoes => {}
                    Ok(_) => waits(&file),
                    Err(e) => write_whole(&file, &format!("refused {e}")),
                }
                String::new()
            }
            #[cfg(windows)]
            Job::Records => {
                let handed: Vec<String> = std::env::args().skip(1 + ITS_OWN.len()).collect();
                write_whole(&file, &serde_json::to_string(&handed).expect("JSON"));
                String::new()
            }
            #[cfg(windows)]
            Job::SaysItsConsole => {
                // SAFETY: no argument; the window is only compared with null.
                let none =
                    unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() }.is_null();
                write_whole(&file, if none { "none" } else { "some" });
                String::new()
            }
        };
        println!("{SAID}{said}");
    }

    #[test]
    fn a_record_is_whole_only_as_written() {
        let one = record(7);
        assert!(whole(&one));
        assert!(!whole(&one[..100]));
        assert!(!whole(&[]));
        let mut mixed = record(7);
        mixed[2048..].copy_from_slice(&record(8)[2048..]);
        assert!(!whole(&mixed));
    }

    #[test]
    fn a_held_lock_is_held_until_the_helper_stops() {
        let dir = tempfile::tempdir().expect("a scratch folder");
        let file = dir.path().join("state.lock");
        let helper = Helper::start(Job::Locks, &file);
        let mine = std::fs::File::open(&file).expect("opened");
        assert!(matches!(
            mine.try_lock_shared(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert_eq!(helper.stop(), "");
        mine.try_lock_shared()
            .expect("free once the helper stopped");
    }
}
