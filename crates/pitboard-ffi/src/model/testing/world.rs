use crate::{AppCore, Made};
use pitboard_core::context::Context;
use pitboard_core::provider::ProviderId;
use pitboard_core::service;
use pitboard_core::state::Key;
use pitboard_core::testing::desktop::{self, FakeClaude, JarAs};
use pitboard_core::testing::{
    Clock, FixedClock, MemoryHost, ScratchMachine, ScriptedApi, live_service, stand_in,
    unsigned_token,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A machine of a test's own for the real core: a home in a scratch directory, Claude Code's
/// keychain, the vault and the scheduler in memory, and Anthropic scripted. Nothing of the
/// machine running the tests is read: no environment, no `PATH`, no login shell.
pub(crate) struct World {
    root: tempfile::TempDir,
    pitboard: PathBuf,
    pub host: Arc<MemoryHost>,
    pub api: Arc<ScriptedApi>,
    ctx: Context,
    // The machine's time when the world was made, which the model's own clock agrees with.
    now: i64,
    /// The command line inside the app, where a test gives it one.
    helper: Option<PathBuf>,
    /// Where the app looks for the `pitboard` a terminal would run: the test's own places
    /// alone, never this machine's.
    places: Vec<PathBuf>,
    /// The login shell's `PATH`, as the app looked in it, where a test says one.
    search_path: Option<String>,
    /// Handed to each core made here, so that `cores_gone` hears once every one has gone.
    cores: Option<Sender<()>>,
    cores_gone: Receiver<()>,
}

// A program's file name on this system, as the system runs it.
pub(crate) fn program_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

impl World {
    pub(crate) fn new(name: &str) -> World {
        // Named after the test, so a path in a failure says whose it is.
        let scratch = tempfile::Builder::new()
            .prefix(&format!("pitboard-model-{name}-"))
            .tempdir()
            .expect("a scratch home");
        let root = scratch.path();
        let claude = root.join("bin").join(program_name("claude"));
        std::fs::create_dir_all(root.join("bin")).expect("a scratch bin");
        stand_in::install(
            &claude,
            &stand_in::Script::refusing("a test's Claude Code, never meant to run\n"),
        )
        .expect("a Claude Code to find");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
            });
        let ScratchMachine { ctx, host, api } = ScratchMachine::at(root, |ctx| {
            ctx.with_pitboard_home(root.join(".pitboard"))
                .with_codex_home(root.join(".codex").to_string_lossy().into_owned())
                .with_user("tester".into())
                .with_search_path(String::new())
                .with_claude_program(claude)
                .with_scratch_claude(root.join("claude"))
                .with_awaited_every(Duration::from_millis(2))
                .with_caller("app".into())
                .with_lock_waiting(2_000, Duration::from_millis(10))
                .with_clock(Arc::new(FixedClock::at(now)) as Arc<dyn Clock>)
        });
        let (cores, cores_gone) = channel();
        World {
            pitboard: root.join(".pitboard"),
            root: scratch,
            host,
            api,
            ctx,
            now,
            helper: None,
            places: Vec::new(),
            search_path: None,
            cores: Some(cores),
            cores_gone,
        }
    }

    pub(crate) fn warming_by(&mut self, script: Arc<dyn pitboard_core::testing::WarmScript>) {
        self.ctx = self.ctx.clone().with_warm_script(script);
    }

    // As Anthropic answers the next read of `who`: no five-hour period running.
    pub(crate) fn resting(&self, who: &str) {
        let now = self.now;
        self.api.using(
            &format!("access-{who}"),
            pitboard_core::usage::Snapshot {
                windows: vec![pitboard_core::usage::Window {
                    kind: "session".into(),
                    scope: None,
                    percent: 0.0,
                    resets_at: None,
                    is_active: true,
                    severity: None,
                    length_seconds: Some(18_000),
                }],
                observed_at: Some(now),
                answered_at: Some(now),
                lists_every_limit: true,
                source: pitboard_core::usage::Source::Live,
            },
        );
    }

    // Claude Code's program taken away, as on a machine where it was never installed.
    pub(crate) fn without_claude_code(&self) {
        std::fs::remove_file(self.root.path().join("bin").join(program_name("claude")))
            .expect("the world's Claude Code");
    }

    /// The app's core over this machine, made from its parts as `AppCore::for_app` makes it
    /// from what the app was started with, with Claude Code's program found.
    pub(crate) fn core(&self) -> Arc<AppCore> {
        let ctx = self.ctx.clone();
        let (helper, places, search_path, alive) = (
            self.helper.clone(),
            self.places.clone(),
            self.search_path.clone(),
            self.cores.clone(),
        );
        Arc::new(AppCore::asking(
            move || {
                let _alive = &alive;
                let made = Made {
                    core: service::Pitboard::new(ctx.clone()),
                    search_path: search_path.clone(),
                    helper: helper.clone(),
                    command_line_places: places.clone(),
                };
                (made, false)
            },
            crate::ASK_AGAIN_AFTER,
        ))
    }

    // Where `build-app.sh` puts one in a copy of the app, or the MSI beside `Pitboard.exe`.
    pub(crate) fn app_with_a_command_line(&mut self) -> PathBuf {
        self.app_with_a_command_line_in("")
    }

    /// The same, with the app in `dir` of the scratch home: under `AppTranslocation/…`, the
    /// temporary copy macOS runs an app from where it was downloaded.
    pub(crate) fn app_with_a_command_line_in(&mut self, dir: &str) -> PathBuf {
        use pitboard_core::host::{OS, Os};
        let helper = match OS {
            Os::MacOs | Os::Linux => self
                .root
                .path()
                .join(dir)
                .join("Pitboard.app/Contents/Helpers/pitboard"),
            Os::Windows => self
                .root
                .path()
                .join(dir)
                .join("Pitboard")
                .join("bin")
                .join("pitboard.exe"),
        };
        a_program_at(&helper);
        self.ctx = self.ctx.clone().with_schedule_program(helper.clone());
        self.helper = Some(helper.clone());
        helper
    }

    // After the login shell's `PATH`, and nothing of this machine's.
    pub(crate) fn looks_for_pitboard_in(
        &mut self,
        places: Vec<PathBuf>,
        search_path: Option<String>,
    ) {
        self.places = places;
        self.search_path = search_path;
    }

    /// A directory of the scratch home.
    pub(crate) fn dir(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// Another front end on the same machine, as the command line in a terminal is.
    /// What this machine's core is made with.
    pub(crate) fn ctx(&self) -> &Context {
        &self.ctx
    }

    pub(crate) fn elsewhere(&self) -> service::Pitboard {
        service::Pitboard::new(self.ctx.clone().with_caller("cli".into()))
    }

    // The Claude this machine's core reads, in its scratch folder.
    pub(crate) fn claude(&self) -> FakeClaude {
        FakeClaude::on(Arc::clone(&self.host), &self.ctx, JarAs::StandIn)
    }

    // home enrolled, then work added by a sign-in inside Claude, which parks home
    pub(crate) fn desktop_pair(&self) {
        let claude = self.claude();
        desktop::plant(&claude.data(), &desktop::HOME, JarAs::StandIn);
        let pitboard = self.elsewhere();
        pitboard
            .enroll_current("desktop/home")
            .expect("home enrolled");
        desktop::signs_in_inside(&pitboard, &claude, "desktop/work", &desktop::WORK)
            .expect("work added by a sign-in inside Claude");
    }

    // A move of `desktop/work`'s login left waiting, as a Claude opened during it leaves one.
    pub(crate) fn desktop_move_waiting(&self) {
        desktop::leave_a_move_waiting(
            &self.ctx,
            &self.host,
            &Key::new(ProviderId::Desktop, "work"),
        );
    }

    /// Claude Code signed in as `who` and enrolled as `label`, with its five-hour window
    /// `percent` used, as Anthropic answers.
    pub(crate) fn enrolled(&self, label: &str, who: &str, percent: f64) {
        self.signed_in(who, percent);
        self.elsewhere()
            .enroll_current(label)
            .expect("the account signed in, enrolled");
    }

    /// Claude Code signed in as `who`, with its five-hour window `percent` used, and not
    /// enrolled.
    pub(crate) fn signed_in(&self, who: &str, percent: f64) {
        let login = self.claude_login(who, percent);
        self.host.live().plant(&live_service(&self.ctx), &login);
        std::fs::write(
            self.root.path().join(".claude.json"),
            format!(
                r#"{{"oauthAccount":{{"accountUuid":"{who}","emailAddress":"{who}@example.com","organizationUuid":"org-{who}"}}}}"#
            ),
        )
        .expect("Claude Code's config");
    }

    /// `who` signed in to Claude Code privately and enrolled as `label`, with its five-hour
    /// window `percent` used, which parks its login and leaves the account in use signed in,
    /// as `pitboard enroll <label> --sign-in` leaves it. No `claude` runs: the sign-in is
    /// planted as one that finished.
    pub(crate) fn parked(&self, label: &str, who: &str, percent: f64) {
        let login = pitboard_core::testing::signed_in(
            &self.ctx,
            ProviderId::Claude,
            &self.claude_login(who, percent),
        )
        .expect("a sign-in");
        self.elsewhere()
            .enroll_signed_in(label, login)
            .expect("the account signed in privately, enrolled");
    }

    /// `who`'s login left in `.credentials.json` behind the keychain, with its five-hour window
    /// `percent` used, as a sign-in where the keychain could not be read leaves it.
    pub(crate) fn left_in_a_file(&self, who: &str, percent: f64) {
        let login = self.claude_login(who, percent);
        self.left_file().plant(&live_service(&self.ctx), &login);
    }

    /// The file behind the keychain, `.credentials.json` in Claude Code's own directory.
    pub(crate) fn left_file(&self) -> Arc<pitboard_core::testing::MemoryStore> {
        self.host
            .file_at(self.root.path().join(".claude").join(".credentials.json"))
    }

    /// A switch to Claude Code's `label` that nothing can finish, as the core leaves one.
    /// Claude Code's keychain locks as the switch writes, so the switch cannot tell what it
    /// wrote and keeps its record and every copy. Claude Code then renews the login it has,
    /// which matches neither side the record kept, and its session expires, so Anthropic
    /// will not say whose it is either.
    pub(crate) fn stuck_switching_to(&self, label: &str) {
        let live = live_service(&self.ctx);
        self.host
            .live()
            .fault(&live, pitboard_core::testing::Fault::LocksOnWrite);
        let locked = self.elsewhere().switch_to(label);
        self.host.live().heal_all();
        assert_eq!(
            locked.expect_err("the keychain locked").error.code(),
            "switch_unverified"
        );
        let renewed = self.claude_login_as("renewed", "access-renewed-since", 0.0);
        self.api.token_trouble(
            "access-renewed-since",
            pitboard_core::testing::Trouble::Unauthorized,
        );
        self.host.live().plant(&live, &renewed);
    }

    /// A stand-in for `claude`, the compiled one the tests start in place of every program,
    /// which this machine's core runs as `claude auth login` from now on. It writes what
    /// 2.1.289 writes before it opens the browser, then reads each line typed back as the
    /// register's `sign_in_output` and `sign_in_takes_another_code` hold: one that is not
    /// `<code>#<state>` with both halves it refuses on stderr and reads on, and the first that
    /// is it takes, and ends signed in. It stores no login: `signed_in_privately` plants the
    /// one it would have.
    ///
    /// It writes down its process id before it says anything, and every line typed back to
    /// it, each file whole: under a name of its own, then moved into place, so a test reading
    /// one as it is written never finds it half there. Written in place, its process id was
    /// read empty, as CI met on Linux (run 37437853042).
    ///
    /// The stand-in runs on every system, as `claude.exe` on Windows, the name a cancel ends.
    pub(crate) fn claude_stand_in(&mut self) -> StandIn {
        self.a_claude_stand_in(false)
    }

    /// The same stand-in, whose first run starts a program of its own before it says
    /// anything, which outlives it with its output held open until the test lets go of it,
    /// as `codex login` does once Codex's npm launcher is killed: @openai/codex 0.149.1's
    /// `bin/codex.js` starts the program with its own output and hands on only `SIGINT`,
    /// `SIGTERM` and `SIGHUP`. Reading what the tool says then ends only once that program
    /// has, long after the tool was stopped and waited for.
    pub(crate) fn claude_stand_in_holding_its_output(&mut self) -> StandIn {
        self.a_claude_stand_in(true)
    }

    fn a_claude_stand_in(&mut self, holding: bool) -> StandIn {
        use pitboard_core::testing::stand_in::{self, Script, Step};
        let bin = self.root.path().join("bin");
        std::fs::create_dir_all(&bin).expect("a scratch bin");
        let stand_in = StandIn {
            pid: self.root.path().join("claude.pid"),
            typed: self.root.path().join("claude.typed"),
            holder: self.root.path().join("holder.pid"),
            holding: self.root.path().join("holding"),
        };
        let mut steps = vec![Step::WritesItsPid(stand_in.pid.clone())];
        // Another of itself, which holds the output it was started with for as long as
        // `holding` is there, and whose process id it writes down whole. Only the first run
        // starts one, so each test has one to let go of.
        if holding {
            steps.push(Step::HoldsOutput {
                while_present: stand_in.holding.clone(),
                pid: stand_in.holder.clone(),
            });
        }
        steps.extend([
            Step::Says("Opening browser to sign in\u{2026}\n".into()),
            Step::Says(
                "If the browser didn't open, visit: \
                 https://claude.com/cai/oauth/authorize?code=true&state=s\n"
                    .into(),
            ),
            Step::Says("Paste code here if prompted > ".into()),
            Step::ReadsCodes {
                typed: stand_in.typed.clone(),
                takes: "Login successful.\n".into(),
                refuses: "Invalid code. Please make sure the full code was copied.\n".into(),
            },
        ]);
        let program = bin.join(format!("claude{}", std::env::consts::EXE_SUFFIX));
        let script = Script::Plays {
            args: Some(vec!["auth".into(), "login".into()]),
            steps,
        };
        stand_in::install(&program, &script).unwrap_or_else(|e| panic!("no stand-in: {e}"));
        self.ctx = self.ctx.clone().with_claude_program(program);
        stand_in
    }

    /// `who` signed in to Claude Code privately, as `claude auth login` leaves it for Pitboard:
    /// a login stored in the keychain item Claude Code makes for Pitboard's private sign-in
    /// directory, with Anthropic scripted to say whose it is and that its five-hour window is
    /// `percent` used. `access` names its access token, so a new login of an account already
    /// here can be told from the one it has. Planted once the sign-in has started, since
    /// starting one clears whatever a sign-in before it left there.
    pub(crate) fn signed_in_privately(&self, who: &str, access: &str, percent: f64) {
        let dir = self.pitboard_dir().join("signin");
        let service = pitboard_core::testing::service_for_dir(&dir.to_string_lossy());
        let login = self.claude_login_as(who, access, percent);
        self.host.live().plant(&service, &login);
    }

    /// Whether the login Claude Code has in use is the one whose access token is `access`.
    pub(crate) fn claude_code_uses(&self, access: &str) -> bool {
        self.host
            .live()
            .peek(&live_service(&self.ctx))
            .is_some_and(|login| login.contains(&format!("\"accessToken\":\"{access}\"")))
    }

    /// A Claude Code login of `who`, in the shape Claude Code stores one, with Anthropic
    /// scripted to say whose it is and that its five-hour window is `percent` used.
    fn claude_login(&self, who: &str, percent: f64) -> String {
        self.claude_login_as(who, &format!("access-{who}"), percent)
    }

    /// The same, with its access token named `access`.
    fn claude_login_as(&self, who: &str, access: &str, percent: f64) -> String {
        let now = self.now;
        let access = access.to_owned();
        self.api.owned_by(
            &access,
            pitboard_core::provider::Owner {
                account_uuid: who.into(),
                email: format!("{who}@example.com"),
                organization_uuid: format!("org-{who}"),
            },
        );
        self.api.using(
            &access,
            pitboard_core::usage::Snapshot {
                windows: vec![pitboard_core::usage::Window {
                    kind: "session".into(),
                    scope: None,
                    percent,
                    resets_at: Some(now + 3_600),
                    is_active: true,
                    severity: None,
                    length_seconds: Some(18_000),
                }],
                observed_at: Some(now),
                answered_at: Some(now),
                lists_every_limit: true,
                source: pitboard_core::usage::Source::Live,
            },
        );
        format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{access}","refreshToken":"refresh-{who}","expiresAt":{},"refreshTokenExpiresAt":{},"scopes":["user:profile","user:inference"]}}}}"#,
            (now + 3_600) * 1_000,
            (now + 30 * 86_400) * 1_000,
        )
    }

    /// Claude Code's write lock taken, as a session writing its login holds it, until what
    /// this returns is dropped: proper-lockfile's directory, `.storage-write.lock` in Claude
    /// Code's config directory, where pitboard-core's `Claude::write_lock` and
    /// `lock::acquire` put it. A switch here waits on it until it is dropped, for as long as
    /// a test waits for anything, then refuses.
    pub(crate) fn claude_code_writing(&self) -> Writing {
        let held = self.root.path().join(".claude").join(".storage-write.lock");
        std::fs::create_dir_all(&held).expect("Claude Code's lock");
        Writing(held)
    }

    /// Codex signed in as `who` and enrolled as `label`, as `codex login` and then
    /// `pitboard enroll codex/<label>` in a terminal leave it.
    pub(crate) fn codex_enrolled(&self, label: &str, who: &str) {
        let home = self.root.path().join(".codex");
        std::fs::create_dir_all(&home).expect("a scratch Codex home");
        self.host
            .file_at(home.join("auth.json"))
            .plant("auth.json", &self.codex_login(who));
        self.elsewhere()
            .enroll_current(&format!("codex/{label}"))
            .expect("the Codex account signed in, enrolled");
    }

    /// `who` signed in to Codex privately and enrolled as `label`, which parks its login and
    /// leaves the account in use signed in, as `pitboard enroll codex/<label> --sign-in`
    /// leaves it. No `codex` runs: the sign-in is planted as one that finished.
    pub(crate) fn codex_parked(&self, label: &str, who: &str) {
        let login =
            pitboard_core::testing::signed_in(&self.ctx, ProviderId::Codex, &self.codex_login(who))
                .expect("a sign-in");
        self.elsewhere()
            .enroll_signed_in(&format!("codex/{label}"), login)
            .expect("the Codex account signed in privately, enrolled");
    }

    /// A Codex login of the ChatGPT account `who`, in the shape `codex login` writes one,
    /// with OpenAI scripted to answer for it.
    fn codex_login(&self, who: &str) -> String {
        let access = unsigned_token(
            &serde_json::json!({"exp": self.now + 10 * 86_400, "for": format!("refresh-{who}")}),
        );
        self.api.using(
            &access,
            pitboard_core::usage::Snapshot {
                windows: Vec::new(),
                observed_at: Some(self.now),
                answered_at: Some(self.now),
                lists_every_limit: false,
                source: pitboard_core::usage::Source::Live,
            },
        );
        pitboard_core::testing::codex_login(
            who,
            &format!("{who}@example.com"),
            &access,
            &format!("refresh-{who}"),
        )
        .to_string()
    }

    /// Pitboard's directory on this machine.
    pub(crate) fn pitboard_dir(&self) -> PathBuf {
        self.pitboard.clone()
    }

    // The one the schedule renews, which on Windows is not `.pitboard`.
    pub(crate) fn in_the_default_home(&mut self) {
        self.pitboard = service::Pitboard::new(self.ctx.clone()).default_directory();
        self.ctx = self.ctx.clone().with_pitboard_home(self.pitboard.clone());
    }

    /// Every account was last put in use an hour ago. Enrolling the account signed in puts
    /// it in use, and nothing switches away from an account by itself for five minutes after.
    pub(crate) fn in_use_an_hour_ago(&self) {
        let path = self.pitboard_dir().join("state.json");
        let mut state: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("the account index"))
                .expect("JSON");
        for account in state["accounts"].as_array_mut().expect("accounts") {
            if let Some(at) = account["last_used_at"].as_i64() {
                account["last_used_at"] = serde_json::json!(at - 3_600);
            }
        }
        std::fs::write(&path, state.to_string()).expect("the account index written");
    }

    /// Says the account index was written `seconds` later than it was. Two writes close
    /// together need not get two times on every file system, so a test says the later one
    /// came later rather than waiting for its time to move.
    pub(crate) fn index_written_later(&self, seconds: u64) {
        self.index_written(|written| written + Duration::from_secs(seconds));
    }

    /// Anthropic answers that `who`'s five-hour window is `percent` used from now on, measured
    /// now: a newer reading than the last, which the next read records.
    pub(crate) fn measures(&self, who: &str, percent: f64) {
        let _ = self.claude_login(who, percent);
    }

    /// Holds the lock usage readings are written under, as a session's status line holds it
    /// while it records, until what this returns is dropped. A read with a newer reading to
    /// record waits for it, and the lane of reads with it.
    pub(crate) fn recording(&self) -> std::fs::File {
        let dir = self.pitboard_dir();
        std::fs::create_dir_all(&dir).expect("Pitboard's directory");
        let lock = std::fs::File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join("usage.lock"))
            .expect("the readings' lock");
        lock.lock().expect("held");
        lock
    }

    /// Says the account index was written `seconds` earlier than it was, so that the next
    /// write moves its time, in whatever second it is made.
    pub(crate) fn index_written_earlier(&self, seconds: u64) {
        self.index_written(|written| written - Duration::from_secs(seconds));
    }

    fn index_written(&self, when: impl FnOnce(SystemTime) -> SystemTime) {
        let index = self.pitboard_dir().join("state.json");
        let file = std::fs::File::options()
            .write(true)
            .open(&index)
            .expect("the account index");
        let written = file
            .metadata()
            .and_then(|meta| meta.modified())
            .expect("when it was written");
        file.set_modified(when(written)).expect("another time");
    }
}

/// What a test's stand-in for `claude` left: its process id, every line typed back to it,
/// and, where it holds its output open, the process id of what holds it and the file it holds
/// it for.
pub(crate) struct StandIn {
    pid: PathBuf,
    typed: PathBuf,
    holder: PathBuf,
    holding: PathBuf,
}

impl StandIn {
    /// Every line typed back to it, in order, as it last wrote them down whole.
    pub(crate) fn typed(&self) -> Vec<String> {
        std::fs::read_to_string(&self.typed)
            .map(|typed| typed.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// The process id the last one to write one wrote, waited for until one has: a test
    /// asking before the stand-in has written it would otherwise take a stand-in that has not
    /// started yet for one that has stopped. A test that starts it again waits for the new
    /// one to say something, which it does only once it has written its own.
    fn pid(&self) -> u32 {
        written_pid(&self.pid)
    }

    /// Whether the program its first run started still holds its output open, as the shell's
    /// own `kill -0` says. Waits for its process id to have been written.
    pub(crate) fn holds_output(&self) -> bool {
        running(written_pid(&self.holder))
    }

    /// Lets the program its first run started end, and its output close with it.
    pub(crate) fn let_go_of_output(&self) {
        let _ = std::fs::remove_file(&self.holding);
    }

    /// Whether the last one to start is still running, as the shell's own `kill -0` says of
    /// the process id it wrote, which is never this test's or anybody else's to stop. Waits
    /// for that process id to have been written.
    pub(crate) fn is_running(&self) -> bool {
        running(self.pid())
    }
}

/// Nothing a stand-in started outlives its test holding its output open.
impl Drop for StandIn {
    fn drop(&mut self) {
        self.let_go_of_output();
    }
}

/// The process id a stand-in wrote whole at `path`, waited for until it has: a test asking
/// before would otherwise take a process that has not started yet for one that has stopped.
fn written_pid(path: &std::path::Path) -> u32 {
    let asked = std::time::Instant::now();
    loop {
        match std::fs::read_to_string(path) {
            Ok(pid) => {
                return pid
                    .trim()
                    .parse()
                    .unwrap_or_else(|_| panic!("a process id written whole: {pid:?}"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                assert!(
                    asked.elapsed() < Duration::from_secs(20),
                    "the stand-in never wrote its process id"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("its process id unread: {error}"),
        }
    }
}

/// Whether the process `pid` runs, as the shell's own `kill -0` says.
#[cfg(unix)]
fn running(pid: u32) -> bool {
    std::process::Command::new("/bin/sh")
        .args(["-c", &format!("kill -0 {pid} 2>/dev/null")])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn running(pid: u32) -> bool {
    pitboard_core::testing::may_be_running(pid)
}

/// Asked whether it runs before it has written its process id, the stand-in is waited for,
/// and not taken for one that has stopped: this process stands in for it here, and writes
/// its own id the way the stand-in does, whole, once the question is still unanswered.
#[test]
fn whether_the_stand_in_runs_is_answered_once_it_has_written_its_process_id() {
    let world = World::new("stand-in-pid");
    let stand_in = StandIn {
        pid: world.root.path().join("claude.pid"),
        typed: world.root.path().join("claude.typed"),
        holder: world.root.path().join("holder.pid"),
        holding: world.root.path().join("holding"),
    };
    let pid = stand_in.pid.clone();
    let (answer, answered) = channel();
    let asking = std::thread::spawn(move || {
        let _ = answer.send(stand_in.is_running());
    });
    assert_eq!(
        answered.recv_timeout(Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout),
        "no answer before its id is there"
    );
    let part = pid.with_extension("pid.part");
    std::fs::write(&part, format!("{}\n", std::process::id())).expect("written");
    std::fs::rename(&part, &pid).expect("moved into place");
    assert_eq!(
        answered.recv_timeout(Duration::from_secs(20)),
        Ok(true),
        "this process, once its id is there"
    );
    asking.join().expect("the question");
}

/// Claude Code's write lock, held until this is dropped.
pub(crate) struct Writing(PathBuf);

impl Drop for Writing {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

impl Drop for World {
    fn drop(&mut self) {
        // A model's lanes may still be writing here after it is shut down or dropped, until
        // each has ended and let go of the core it was given.
        drop(self.cores.take());
        let _ = self.cores_gone.recv_timeout(Duration::from_secs(20));
    }
}

// The compiled stand-in, which refuses to do anything were it run.
pub(crate) fn a_program_at(path: &std::path::Path) {
    use pitboard_core::testing::stand_in::{self, Script};
    let dir = path.parent().expect("a program's directory");
    std::fs::create_dir_all(dir).expect("a scratch directory");
    stand_in::install(
        path,
        &Script::refusing("a test's program, never meant to run\n"),
    )
    .unwrap_or_else(|e| panic!("no program at {}: {e}", path.display()));
}
