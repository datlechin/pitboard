//! What a fixture's model asks of the app's own system, played by the fixture: the other
//! apps on its machine, and notifications, which it posts none of.

use crate::Front;
use crate::model::{AppId, Notice, Notifier, OtherApps, PlatformError};
use pitboard_core::host::{OS, Os};
use pitboard_core::testing::MemoryHost;
use pitboard_core::testing::desktop::FakeClaude;
use std::collections::BTreeSet;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

/// The shipping apps' ChatGPT on this system, as the core's holder detection names it.
#[cfg(test)]
pub(crate) const CHATGPT: &str = chatgpt(chatgpt_system(Front::MacMenuBar)).app;

/// The system whose ChatGPT a world of the app `front` names runs. The Linux app's is Linux's.
/// The shipping apps' bindings are tested on every system, against Windows' ChatGPT on Windows
/// and a Mac's elsewhere, as their own tests name it (`ModelTests.cs`'s `ChatGpt`).
pub(crate) const fn chatgpt_system(front: Front) -> Os {
    match front {
        Front::Gnome => Os::Linux,
        Front::MacMenuBar | Front::WindowsNotificationArea => match OS {
            Os::Windows => Os::Windows,
            Os::MacOs | Os::Linux => Os::MacOs,
        },
    }
}

pub(crate) const CLAUDE: &str = "com.anthropic.claudefordesktop";

/// ChatGPT on one system, as pitboard-core's `provider::codex::holders` tells it from the
/// process list: what tells the core the app is open.
pub(crate) struct ChatGpt {
    /// As `OtherApps` takes it: on macOS its bundle id, on Windows its AUMID. On Linux, where
    /// nothing takes it, its package.
    pub(crate) app: &'static str,
    /// Where it runs its own `codex`, two of them, as there were on the Mac.
    codex: &'static str,
    /// The Windows package's family it runs in.
    package: Option<&'static str>,
}

pub(crate) const fn chatgpt(os: Os) -> ChatGpt {
    match os {
        // W18's family and app id, which the session's step 10 measures. Where in the package
        // its codex runs waits on that step too; the core tells it by the family alone.
        Os::Windows => ChatGpt {
            app: "OpenAI.Codex_2p2nqsd0c76g0!App",
            codex: "C:/Program Files/WindowsApps/OpenAI.Codex_1.0.0.0_x64__2p2nqsd0c76g0/codex.exe",
            package: Some("OpenAI.Codex_2p2nqsd0c76g0"),
        },
        // As the process list of a Mac running ChatGPT 26.928.31416 gave it.
        Os::MacOs => ChatGpt {
            app: "com.openai.codex",
            codex: "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
            package: None,
        },
        // Where its package installs the codex it runs (codex_linux_app).
        Os::Linux => ChatGpt {
            app: "chatgpt",
            codex: "/usr/lib/chatgpt/resources/codex",
            package: None,
        },
    }
}

impl ChatGpt {
    /// As the core names it: by its AUMID where it runs in a package, else by its bundle id.
    #[cfg(test)]
    pub(crate) fn app_id(&self) -> AppId {
        let id = self.app.into();
        match self.package {
            Some(_) => AppId::WindowsAumid { id },
            None => AppId::MacBundle { id },
        }
    }

    pub(crate) fn opens(&self, host: &MemoryHost) {
        let codex = [self.codex, self.codex];
        match self.package {
            Some(family) => host.runs_in_package("codex", family, &codex),
            None => host.runs_at("codex", &codex),
        }
    }
}

// Long enough for a UI test to see the waiting page under CI's load.
const SIGNS_IN_AFTER: Duration = Duration::from_secs(5);
const QUITS_AFTER: Duration = Duration::from_secs(2);

/// The apps running on a fixture's machine. The core sees ChatGPT's `codex` in the machine's
/// process list while it runs, and the model quits and opens it through this, so the two
/// agree the way the process list and the system's list of apps agree on a real machine.
/// Nothing here touches an app of the machine running the fixture.
pub(crate) struct FixtureApps {
    chatgpt: ChatGpt,
    host: Arc<MemoryHost>,
    // played over the fixture's machine, writing its data folder as 2.7032.0 does as it quits
    claude: Arc<FakeClaude>,
    // told whenever an app opens or quits
    held: Arc<(Mutex<Held>, Condvar)>,
    // each sign-in played in Claude
    played: Mutex<Vec<JoinHandle<()>>>,
}

#[derive(Default)]
struct Held {
    running: BTreeSet<String>,
    /// Every app asked to quit and every app opened, in order: "quit com.openai.codex".
    asked: Vec<String>,
    // how many times Claude has opened or quit: a sign-in played in it ends as it quits
    claude_turns: u64,
}

fn held(held: &Mutex<Held>) -> MutexGuard<'_, Held> {
    // Every change here is whole whenever the lock is let go.
    held.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl FixtureApps {
    /// The apps of the machine `host` is, `running` among them from the start, ChatGPT as
    /// `chatgpt` is on its system.
    pub(crate) fn new(
        host: Arc<MemoryHost>,
        claude: Arc<FakeClaude>,
        chatgpt: ChatGpt,
        running: &[&str],
    ) -> Arc<FixtureApps> {
        let apps = Arc::new(FixtureApps {
            chatgpt,
            host,
            claude,
            held: Arc::default(),
            played: Mutex::default(),
        });
        for app in running {
            apps.started(app);
        }
        apps
    }

    /// Every app asked to quit and every app opened, in order.
    #[cfg(test)]
    pub(crate) fn asked(&self) -> Vec<String> {
        held(&self.held.0).asked.clone()
    }

    // Waits for every sign-in played in Claude to have come to its end.
    #[cfg(test)]
    pub(crate) fn played(&self) {
        let played = std::mem::take(&mut *self.played.lock().expect("the played sign-ins"));
        for sign_in in played {
            sign_in.join().expect("a played sign-in");
        }
    }

    /// Whether `app` is running.
    pub(crate) fn is_running(&self, app: &str) -> bool {
        held(&self.held.0).running.contains(app)
    }

    /// Where an app of the fixture's is, which is nowhere on the machine running it.
    fn copy_of(app: &str) -> String {
        format!("/fixture/{app}.app")
    }

    // The turn Claude is on once this has run.
    fn started(&self, app: &str) -> u64 {
        let mut now = held(&self.held.0);
        now.running.insert(app.to_owned());
        if app == CLAUDE {
            now.claude_turns += 1;
        }
        let turn = now.claude_turns;
        drop(now);
        self.held.1.notify_all();
        if app == self.chatgpt.app {
            self.chatgpt.opens(&self.host);
        } else if app == CLAUDE {
            self.claude.opens();
        }
        turn
    }

    fn stopped(&self, app: &str) {
        let mut now = held(&self.held.0);
        now.running.remove(app);
        if app == CLAUDE {
            now.claude_turns += 1;
        }
        drop(now);
        self.held.1.notify_all();
        if app == self.chatgpt.app {
            self.host.runs_at("codex", &[]);
        } else if app == CLAUDE {
            self.claude.quits();
        }
    }

    // The core waits for a quit app's processes to leave the process list, so with an app's
    // own OtherApps the list follows what that one says runs.
    pub(crate) fn following(self: &Arc<Self>, apps: Arc<dyn OtherApps>) -> Arc<dyn OtherApps> {
        Arc::new(Following {
            apps,
            machine: Arc::clone(self),
        })
    }
}

struct Following {
    apps: Arc<dyn OtherApps>,
    machine: Arc<FixtureApps>,
}

impl OtherApps for Following {
    fn running(&self, app: AppId) -> Result<Option<String>, PlatformError> {
        let found = self.apps.running(app.clone());
        if let Ok(copy) = &found
            && copy.is_some() != self.machine.is_running(app.id())
        {
            if copy.is_some() {
                self.machine.started(app.id());
            } else {
                self.machine.stopped(app.id());
            }
        }
        found
    }

    fn request_quit(&self, app: AppId) -> Result<(), PlatformError> {
        self.apps.request_quit(app)
    }

    fn reopen(&self, location: String, front: bool) -> Result<(), PlatformError> {
        self.apps.reopen(location, front)
    }
}

impl OtherApps for FixtureApps {
    fn running(&self, app: AppId) -> Result<Option<String>, PlatformError> {
        Ok(self
            .is_running(app.id())
            .then(|| FixtureApps::copy_of(app.id())))
    }

    /// Quits at once, as an app with no work in progress does, and its processes leave the
    /// process list with it.
    fn request_quit(&self, app: AppId) -> Result<(), PlatformError> {
        held(&self.held.0).asked.push(format!("quit {}", app.id()));
        self.stopped(app.id());
        Ok(())
    }

    // Opened in front on a folder signed out, Claude is a person signing in and quitting.
    fn reopen(&self, location: String, front: bool) -> Result<(), PlatformError> {
        let app = location
            .strip_prefix("/fixture/")
            .and_then(|rest| rest.strip_suffix(".app"))
            .unwrap_or(&location)
            .to_owned();
        let opened = if front { "open-front" } else { "open" };
        held(&self.held.0).asked.push(format!("{opened} {app}"));
        let signs_in = app == CLAUDE && front && !self.claude.holds_a_session();
        let turn = self.started(&app);
        if signs_in {
            let (claude, who, apps) = (
                Arc::clone(&self.claude),
                pitboard_core::testing::desktop::TRAVEL,
                Arc::clone(&self.held),
            );
            // Held throughout, so Pitboard's quit or open comes before or after each step, and
            // a person who sees Claude quit or open again meanwhile goes no further.
            let still =
                move |now: &mut Held| now.claude_turns == turn && now.running.contains(CLAUDE);
            let played = std::thread::spawn(move || {
                let (lock, turned) = &*apps;
                let (mut now, _) = turned
                    .wait_timeout_while(held(lock), SIGNS_IN_AFTER, still)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if !still(&mut now) {
                    return;
                }
                claude.signs_in(&who);
                claude.reports();
                let (mut now, _) = turned
                    .wait_timeout_while(now, QUITS_AFTER, still)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if still(&mut now) {
                    now.running.remove(CLAUDE);
                    now.claude_turns += 1;
                    claude.quits();
                }
            });
            self.played
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(played);
        }
        Ok(())
    }
}

/// Notifications, of which a fixture posts none: the person running the tests is asked for
/// nothing, a notification's permission included.
pub(crate) struct Unposted;

impl Notifier for Unposted {
    fn post(&self, _notice: Notice) -> Result<(), PlatformError> {
        Ok(())
    }
}
