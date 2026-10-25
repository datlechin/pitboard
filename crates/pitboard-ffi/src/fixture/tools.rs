//! Each tool's own sign-in on a fixture's machine, played through pitboard-core's seam for a
//! sign-in that starts no program, together with the person at the browser. Everything
//! around it is the real core's: the private directory it reserves, the login it reads back
//! from where the tool stores it there, and the enrolment.
//!
//! Each prints and reads what its tool's register records: `sign_in_output` and
//! `sign_in_takes_another_code` for Claude Code, so a test types a code with its `#`, such as
//! `fixture-code#state`, and `codex_login_prints_its_address` for Codex, which finishes by
//! itself once the browser would have come back.
//!
//! Whoever signs in is the person the account is for: one enrolled already under that name
//! signs in again as themselves, and anybody else is somebody new, `<name>@example.com`, or
//! `dana@travel.example` for an add given no name, with a five-hour limit barely touched.

use super::worlds::{Person, Stage};
use pitboard_core::provider::ProviderId;
use pitboard_core::testing::{
    Ran, ScriptedSignIn, SignInScript, Started, WarmScript, service_for_dir,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a tool takes between the pieces it prints, as the macOS app's fixture took.
const BETWEEN: Duration = Duration::from_millis(200);

/// How long Codex's browser takes to come back once its address is printed.
const BROWSER: Duration = Duration::from_millis(1_500);

/// The address Claude Code's sign-in prints for a browser that did not open, in the shape the
/// register records: the manual one, on claude.com. Nothing opens it: a UI test only copies
/// it.
pub(crate) const CLAUDE_ADDRESS: &str =
    "https://claude.com/cai/oauth/authorize?code=true&state=pitboard-fixture";

/// The address Codex's sign-in prints, in the shape the register records: on the issuer,
/// auth.openai.com.
pub(crate) const CODEX_ADDRESS: &str =
    "https://auth.openai.com/oauth/authorize?response_type=code&state=pitboard-fixture";

/// What Claude Code's sign-in writes, to stderr, for a line typed back that it refuses:
/// the register's `sign_in_takes_another_code` holds it whole.
pub(crate) const CLAUDE_REFUSES: &str =
    "Invalid code. Please make sure the full code was copied.\n";

/// Claude Code's headless warm-up on a fixture's machine, which starts no program.
// It prints what print mode prints for an answered request (`headless_result`), and starts the
// five-hour period of the account whose access token it was handed.
pub(crate) struct Warmer {
    machine: Arc<Stage>,
}

impl Warmer {
    pub(crate) fn new(machine: Arc<Stage>) -> Warmer {
        Warmer { machine }
    }
}

impl std::fmt::Debug for Warmer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Warmer")
    }
}

impl WarmScript for Warmer {
    fn run(&self, started: &Started) -> Ran {
        if let Some(access) = started.access_token() {
            self.machine.started_period(access);
        }
        Ran {
            exit: Some(0),
            stdout: r#"{"type":"result","subtype":"success","is_error":false,"result":"OK","api_error_status":null}"#.into(),
            timed_out: false,
        }
    }
}

/// The browser and each tool's sign-in on a fixture's machine.
pub(crate) struct Browser {
    machine: Arc<Stage>,
}

impl Browser {
    pub(crate) fn new(machine: Arc<Stage>) -> Browser {
        Browser { machine }
    }
}

impl std::fmt::Debug for Browser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Browser")
            .field("root", &self.machine.root())
            .finish()
    }
}

/// What a played sign-in is told from outside.
enum Told {
    /// A line typed back.
    Typed(String),
    /// Stopped, as a kill stops the program.
    Stopped,
}

impl SignInScript for Browser {
    fn start(
        &self,
        which: ProviderId,
        label: Option<&str>,
        dir: &Path,
        say: Sender<String>,
    ) -> Box<dyn ScriptedSignIn> {
        let person = self.machine.signing_in_as(which, label);
        let (told, hears) = channel();
        let machine = Arc::clone(&self.machine);
        let dir = dir.to_path_buf();
        let ended = std::thread::Builder::new()
            .name("pitboard-fixture-sign-in".into())
            .spawn(move || play(&machine, &person, &dir, &say, &hears))
            .ok();
        Box::new(Playing { told, ended })
    }
}

/// One sign-in under way, as its program would be driven.
struct Playing {
    told: Sender<Told>,
    /// Its thread, which says whether it ended signed in. `None` once waited for, and where
    /// the system would not start one, which is a sign-in that never finishes.
    ended: Option<JoinHandle<bool>>,
}

impl ScriptedSignIn for Playing {
    fn typed(&mut self, line: &str) -> bool {
        self.told.send(Told::Typed(line.to_owned())).is_ok()
    }

    fn wait(&mut self) -> bool {
        self.ended
            .take()
            .is_some_and(|ended| ended.join().unwrap_or(false))
    }

    fn stop(&mut self) {
        let _ = self.told.send(Told::Stopped);
    }
}

/// `person`'s tool signs in, as it prints and reads, into `dir`. Whether it ended signed in;
/// `say` goes with it, which is the tool having stopped saying anything.
fn play(
    machine: &Stage,
    person: &Person,
    dir: &Path,
    say: &Sender<String>,
    hears: &Receiver<Told>,
) -> bool {
    let printed = match person.tool {
        ProviderId::Claude => vec![
            "Opening browser to sign in\u{2026}\n".to_owned(),
            format!("If the browser didn't open, visit: {CLAUDE_ADDRESS}\n"),
            "Paste code here if prompted > ".to_owned(),
        ],
        ProviderId::Codex => vec![
            "Starting local login server on http://localhost:1455.\n".to_owned(),
            format!(
                "If your browser did not open, navigate to this URL to authenticate:\n\n\
                 {CODEX_ADDRESS}\n"
            ),
        ],
        // A tool the fixture has no sign-in of fails to start one.
        _ => return false,
    };
    // A line typed back before the tool reads one waits for it, as it would in its stdin.
    let mut typed = Vec::new();
    let last = printed.len() - 1;
    for (at, piece) in printed.into_iter().enumerate() {
        if say.send(piece).is_err() {
            return false;
        }
        if at < last && stopped_within(hears, BETWEEN, &mut typed) {
            return false;
        }
    }
    let done = match person.tool {
        // It reads the code the browser shows, however long that takes, and another in
        // place of each it refuses.
        ProviderId::Claude => {
            let mut typed = typed.into_iter();
            loop {
                let line = match typed.next() {
                    Some(line) => line,
                    None => match hears.recv() {
                        Ok(Told::Typed(line)) => line,
                        Ok(Told::Stopped) | Err(_) => break false,
                    },
                };
                if is_whole_code(&line) {
                    break true;
                }
                if say.send(CLAUDE_REFUSES.to_owned()).is_err() {
                    break false;
                }
            }
        }
        // It reads nothing typed back, and the browser comes back by itself.
        ProviderId::Codex => !stopped_within(hears, BROWSER, &mut typed),
        _ => false,
    };
    if done {
        machine.signed_in_privately(person, dir);
    }
    done
}

// A code and a state, neither empty, whatever the state says (`sign_in_takes_another_code`).
fn is_whole_code(line: &str) -> bool {
    let mut halves = line.trim().split('#');
    let code = halves.next().unwrap_or_default();
    let state = halves.next().unwrap_or_default();
    !code.is_empty() && !state.is_empty()
}

/// Whether it is stopped within `wait`, keeping every line typed back meanwhile in `typed`,
/// in the order it was typed.
fn stopped_within(hears: &Receiver<Told>, wait: Duration, typed: &mut Vec<String>) -> bool {
    let until = Instant::now() + wait;
    loop {
        match hears.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok(Told::Typed(line)) => typed.push(line),
            Ok(Told::Stopped) | Err(RecvTimeoutError::Disconnected) => return true,
            Err(RecvTimeoutError::Timeout) => return false,
        }
    }
}

impl Stage {
    /// Whom a sign-in of `which`'s tool for `label` signs in as: the person enrolled under
    /// it, or somebody new.
    fn signing_in_as(&self, which: ProviderId, label: Option<&str>) -> Person {
        let Some(label) = label else {
            return Person {
                tool: which,
                id: format!("travel-{}", which.code()),
                email: "dana@travel.example".into(),
            };
        };
        self.enrolled_as(which, label).unwrap_or_else(|| Person {
            tool: which,
            id: format!("{label}-{}", which.code()),
            email: format!("{label}@example.com"),
        })
    }

    /// A new login of `person`'s, stored in `dir` where their tool stores the login of a
    /// sign-in into it: Claude Code in the keychain item named for the directory, Codex in
    /// `auth.json` inside it.
    fn signed_in_privately(&self, person: &Person, dir: &Path) {
        let now = self.now_on_its_clock();
        let login = self.login(person, now + 86_400, now + 30 * 86_400);
        match person.tool {
            ProviderId::Claude => self
                .host
                .live()
                .plant(&service_for_dir(&dir.to_string_lossy()), &login.document),
            ProviderId::Codex => self
                .host
                .file_at(PathBuf::from(dir).join("auth.json"))
                .plant("auth.json", &login.document),
            _ => {}
        }
    }
}
