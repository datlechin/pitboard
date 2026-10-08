//! The ten worlds, each made on the real core, as the macOS app's `Fixture` named them.
//!
//! Each is a machine of its own: a home in a folder of the fixture's, Claude Code's keychain,
//! Pitboard's vault, the process list and the scheduler in memory, and Anthropic and OpenAI
//! answering from a script. Its accounts are put there the way a person puts them there, by
//! the core: a tool signed in, enrolled from a terminal or the app, another account signed
//! in privately and parked, a switch one way and back, each on a clock set to when it would
//! have happened, so the activity log has a history. What a world then shows is whatever the
//! core and the model make of that machine.

use super::FixtureError;
use super::apps::{CHATGPT, FixtureApps, Unposted};
use super::tools::Browser;
use crate::model::preferences::Preferences;
use crate::model::state::Cadence;
use crate::model::{
    LocalTime, ModelListener, PitboardModel, Platform, WindowsLaunch, WindowsPlace,
};
use crate::{AppCore, Made};
use pitboard_core::api::Owner;
use pitboard_core::app::AppFile;
use pitboard_core::context::Context;
use pitboard_core::host::{OS, Os};
use pitboard_core::provider::ProviderId;
use pitboard_core::service;
use pitboard_core::state::Key;
use pitboard_core::testing::fs as files;
use pitboard_core::testing::{Fault, FixedClock, MemoryHost, ScriptedApi, live_service};
use pitboard_core::usage;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

/// The Pitboard link scheme a fixture answers: a debug build's, whichever build it is, so a UI
/// test's link never reaches a copy installed.
pub(crate) const LINK_SCHEME: &str = "pitboard-debug";

const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;

/// A machine in a known state, as a debug build knows it from `PITBOARD_FIXTURE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum World {
    /// Claude Code and Codex, each with an account in use and one to switch to, and a
    /// Claude Code account whose parked login has expired and needs a sign-in.
    TwoTools,
    /// Claude Code alone, with two accounts.
    OneTool,
    /// Claude Code is installed and nobody is signed in to it.
    Empty,
    /// `Empty`, opened for the first time.
    FirstLaunch,
    /// Neither Claude Code nor Codex is on this machine, and nothing is signed in.
    NoClaudeCode,
    /// Somebody is signed in to Claude Code and Pitboard has no name for them.
    Unnamed,
    /// One Claude Code account, so nothing to switch to.
    OnlyOne,
    /// `OneTool` with an account index the core cannot read, so every read of the accounts
    /// fails, `state_unreadable`, and nothing is known to list.
    ReadFailure,
    /// `OneTool` with a switch to personal interrupted, which nothing can finish: Claude
    /// Code's keychain locked as it was written, and Claude Code has since renewed its login
    /// and its session has expired, so neither the record nor Anthropic says whose it is. The
    /// read that asks Anthropic says so, `recovery_undetermined`, so the app offers Give Up…
    /// as it starts.
    Stuck,
    /// `TwoTools`, with ChatGPT open and running Codex's login.
    ChatGptOpen,
}

impl World {
    /// Every world, in the order the macOS app's `Fixture` listed them.
    pub(crate) const ALL: [World; 10] = [
        World::TwoTools,
        World::OneTool,
        World::Empty,
        World::FirstLaunch,
        World::NoClaudeCode,
        World::Unnamed,
        World::OnlyOne,
        World::ReadFailure,
        World::Stuck,
        World::ChatGptOpen,
    ];

    /// Its name, as `PITBOARD_FIXTURE` gives it and the UI tests launch it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            World::TwoTools => "twoTools",
            World::OneTool => "oneTool",
            World::Empty => "empty",
            World::FirstLaunch => "firstLaunch",
            World::NoClaudeCode => "noClaudeCode",
            World::Unnamed => "unnamed",
            World::OnlyOne => "onlyOne",
            World::ReadFailure => "readFailure",
            World::Stuck => "stuck",
            World::ChatGptOpen => "chatGPTOpen",
        }
    }

    /// The world called `name`, or why there is none.
    pub(crate) fn named(name: &str) -> Result<World, FixtureError> {
        World::ALL
            .into_iter()
            .find(|world| world.name() == name)
            .ok_or_else(|| FixtureError::Unknown {
                reason: format!(
                    "No fixture is called {name:?}. There are: {}.",
                    World::ALL.map(World::name).join(", ")
                ),
            })
    }

    /// The tools whose program is on the machine.
    fn installed(self) -> &'static [ProviderId] {
        match self {
            World::NoClaudeCode => &[],
            World::TwoTools
            | World::OneTool
            | World::Empty
            | World::FirstLaunch
            | World::Unnamed
            | World::OnlyOne
            | World::ReadFailure
            | World::Stuck
            | World::ChatGptOpen => &[ProviderId::Claude, ProviderId::Codex],
        }
    }
}

/// Where a world's machine is kept: a folder of its own, emptied before it is made.
pub(crate) struct Folder {
    path: PathBuf,
    /// Whether it goes once the world is done with: a test's does. The one an app launches
    /// into stays until the next launch empties it, as the macOS app's fixture folder did.
    removed_after: bool,
}

impl Folder {
    /// `pitboard-fixture` in this process's temporary directory, emptied: where an app's
    /// fixture is kept, one at a time.
    pub(crate) fn shared() -> std::io::Result<Folder> {
        Folder::within(&std::env::temp_dir())
    }

    /// `pitboard-fixture` in `directory`, emptied, as `shared` is in the temporary directory,
    /// and left there as that one is.
    pub(crate) fn within(directory: &Path) -> std::io::Result<Folder> {
        Folder::emptied(directory.join("pitboard-fixture"), false)
    }

    /// A folder of a test's own, named `name`, which goes once the world is done with.
    #[cfg(test)]
    pub(crate) fn own(name: &str) -> std::io::Result<Folder> {
        Folder::emptied(
            std::env::temp_dir().join(format!(
                "pitboard-fixture-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            )),
            true,
        )
    }

    fn emptied(path: PathBuf, removed_after: bool) -> std::io::Result<Folder> {
        match std::fs::remove_dir_all(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        std::fs::create_dir_all(&path)?;
        Ok(Folder {
            path,
            removed_after,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        if self.removed_after {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Why a world could not be made.
#[derive(Debug)]
pub(crate) struct Unmade(String);

impl std::fmt::Display for Unmade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<std::io::Error> for Unmade {
    fn from(error: std::io::Error) -> Unmade {
        Unmade(error.to_string())
    }
}

/// A step of making a world that the core refused, said with what it was.
fn refused(step: &str) -> impl FnOnce(service::Failed) -> Unmade + '_ {
    move |failed| Unmade(format!("{step}: {}", failed.error))
}

/// Somebody with an account of a tool: whom its service says a login is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Person {
    pub(crate) tool: ProviderId,
    /// What the tool's service knows them by: Anthropic's account id, ChatGPT's.
    pub(crate) id: String,
    pub(crate) email: String,
}

impl Person {
    fn new(tool: ProviderId, id: &str, email: &str) -> Person {
        Person {
            tool,
            id: id.into(),
            email: email.into(),
        }
    }
}

/// A login as its tool stores it, and the token its service is asked with.
pub(crate) struct Login {
    pub(crate) document: String,
    pub(crate) access: String,
}

/// A world's machine: everything the core reaches on it, and what the fixture plays on it.
pub(crate) struct Machine {
    /// The fixture's own folder, which holds everything on disk.
    folder: Folder,
    pub(crate) host: Arc<MemoryHost>,
    pub(crate) api: Arc<ScriptedApi>,
    /// The machine as every front end on it has it, with no caller, clock or browser yet.
    base: Context,
    /// When the world was made, which everything in it is put relative to.
    pub(crate) now: i64,
    /// What each person has used of their limits, by their id, which every login of theirs is
    /// answered with.
    usage: Mutex<HashMap<String, Vec<usage::Window>>>,
    /// How many logins have been made, so each has tokens of its own.
    logins: AtomicUsize,
    /// Every access token each person's logins have had, by their id: what a service is
    /// asked about them with, which a test puts out of reach.
    #[cfg(test)]
    tokens: Mutex<HashMap<String, Vec<String>>>,
}

impl Machine {
    fn new(folder: Folder) -> Result<Machine, Unmade> {
        let root = folder.path().to_path_buf();
        let home = root.join("home");
        std::fs::create_dir_all(&home)?;
        let host = MemoryHost::new();
        let api = ScriptedApi::new();
        let base = Context::new(home.clone())
            .with_pitboard_home(home.join(".pitboard"))
            .with_codex_home(home.join(".codex").to_string_lossy().into_owned())
            .with_user("dana".into())
            // Nothing on this machine's own `PATH` is ever found.
            .with_search_path(String::new())
            .with_claude_program(Machine::program_in(&root, ProviderId::Claude))
            .with_codex_program(Machine::program_in(&root, ProviderId::Codex))
            .with_schedule_program(Machine::helper_in(&root))
            .with_memory_stores(Arc::clone(&host))
            .with_scripted_api(Arc::clone(&api));
        // Claude Code's login goes in its default slot, under the name the real one has on
        // this machine, so nothing is put anywhere until the machine is known to be the one
        // in memory.
        if !base.reaches_only(&host, &api) {
            return Err(Unmade(
                "the fixture's machine reaches this one's keychain or the network".into(),
            ));
        }
        Ok(Machine {
            folder,
            host,
            api,
            base,
            now: epoch_now(),
            usage: Mutex::new(HashMap::new()),
            logins: AtomicUsize::new(0),
            #[cfg(test)]
            tokens: Mutex::new(HashMap::new()),
        })
    }

    /// The fixture's own folder.
    pub(crate) fn root(&self) -> &Path {
        self.folder.path()
    }

    /// The machine's home.
    pub(crate) fn home(&self) -> PathBuf {
        self.root().join("home")
    }

    // Windows runs a file by its extension, so this matches what `refusing_program` writes.
    fn program_in(root: &Path, tool: ProviderId) -> PathBuf {
        let name = tool.program();
        root.join("tools").join(match OS {
            Os::MacOs | Os::Linux => name.to_owned(),
            Os::Windows if cfg!(test) => format!("{name}.exe"),
            Os::Windows => format!("{name}.cmd"),
        })
    }

    /// The command line inside a stand-in app in the fixture's folder, which the schedule
    /// runs and a link reaches: where the macOS app's fixture put its own.
    fn helper_in(root: &Path) -> PathBuf {
        root.join("Pitboard.app/Contents/Helpers/pitboard")
    }

    pub(crate) fn helper(&self) -> PathBuf {
        Machine::helper_in(self.root())
    }

    /// Where a terminal looks for `pitboard` on this machine, and where the macOS app's
    /// fixture links its command line: nothing is there until somebody links it.
    pub(crate) fn bin(&self) -> PathBuf {
        self.root().join("bin")
    }

    /// The machine as the app has it, signing in through `browser`.
    pub(crate) fn app_context(&self, browser: Arc<Browser>) -> Context {
        self.base
            .clone()
            .with_caller("app".into())
            .with_sign_in_script(browser)
    }

    /// The machine as `caller` had it `ago` seconds before the world was made.
    fn front_end(&self, caller: &str, ago: i64) -> (Context, service::Pitboard) {
        let ctx = self
            .base
            .clone()
            .with_caller(caller.into())
            .with_clock(Arc::new(FixedClock::at(self.now - ago)));
        (ctx.clone(), service::Pitboard::new(ctx))
    }

    fn usage(&self) -> MutexGuard<'_, HashMap<String, Vec<usage::Window>>> {
        self.usage
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// `person` has used `windows` of their limits: `(kind, percent, how long it runs, how
    /// long until it resets)`, in seconds from when the world was made.
    fn has_used(&self, person: &Person, windows: &[(&str, f64, i64, i64)]) {
        let windows = windows
            .iter()
            .map(|&(kind, percent, length, resets_in)| usage::Window {
                kind: kind.into(),
                scope: None,
                percent,
                resets_at: Some(self.now + resets_in),
                is_active: true,
                severity: None,
                length_seconds: Some(length),
            })
            .collect();
        self.usage().insert(person.id.clone(), windows);
    }

    /// What `person`'s service says they have used, whichever login of theirs it is asked
    /// with: a five-hour limit barely touched, for somebody new.
    fn used_by(&self, person: &Person) -> usage::Snapshot {
        let windows = self.usage().get(&person.id).cloned().unwrap_or_else(|| {
            let kind = match person.tool {
                ProviderId::Codex => "five_hour",
                _ => "session",
            };
            vec![usage::Window {
                kind: kind.into(),
                scope: None,
                percent: 3.0,
                resets_at: Some(self.now + 5 * HOUR),
                is_active: true,
                severity: None,
                length_seconds: Some(5 * HOUR),
            }]
        });
        usage::Snapshot {
            windows,
            observed_at: Some(self.now),
            account_uuid: None,
            source: usage::Source::Live,
        }
    }

    /// A new login of `person`'s, in the shape their tool stores one, with their service
    /// scripted to answer for it: whose it is, and what they have used. Its access token
    /// lapses at `access_until` and its refresh token at `refresh_until`, in epoch seconds.
    pub(crate) fn login(&self, person: &Person, access_until: i64, refresh_until: i64) -> Login {
        let n = self.logins.fetch_add(1, Ordering::SeqCst);
        let refresh = format!("{}-refresh-{n}", person.id);
        match person.tool {
            ProviderId::Codex => {
                let id_token = unsigned_token(&json!({
                    "email": person.email,
                    "exp": access_until,
                    "https://api.openai.com/auth": {
                        "chatgpt_account_id": person.id,
                        "chatgpt_user_id": format!("user-{}", person.id),
                        "chatgpt_plan_type": "pro",
                    },
                }));
                let access = unsigned_token(&json!({ "exp": access_until, "for": refresh }));
                self.answers_for(person, &access);
                let document = json!({
                    "auth_mode": "chatgpt",
                    "OPENAI_API_KEY": null,
                    "tokens": {
                        "id_token": id_token,
                        "access_token": access,
                        "refresh_token": refresh,
                        "account_id": person.id,
                    },
                    "last_refresh": "2026-10-01T08:00:00Z",
                });
                Login {
                    document: document.to_string(),
                    access,
                }
            }
            // Claude Code's, and its shape for any tool the fixture has none of its own for.
            _ => {
                let access = format!("{}-access-{n}", person.id);
                self.answers_for(person, &access);
                // Anthropic renews it once, for a parked login renewed before it lapses,
                // as Renew Now and the schedule renew one.
                let renewed = format!("{access}-renewed");
                self.answers_for(person, &renewed);
                self.api.renews(
                    &refresh,
                    pitboard_core::api::Renewed {
                        access_token: renewed,
                        refresh_token: Some(format!("{refresh}-renewed")),
                        expires_in: DAY,
                        refresh_token_expires_in: Some(30 * DAY),
                        scopes: None,
                        at: None,
                    },
                );
                let document = json!({
                    "claudeAiOauth": {
                        "accessToken": access,
                        "refreshToken": refresh,
                        "expiresAt": access_until * 1_000,
                        "refreshTokenExpiresAt": refresh_until * 1_000,
                        "scopes": ["user:profile", "user:inference"],
                    }
                });
                Login {
                    document: document.to_string(),
                    access,
                }
            }
        }
    }

    /// `person`'s service answers for the access token `access`: whose it is, where it asks,
    /// and what they have used.
    fn answers_for(&self, person: &Person, access: &str) {
        if person.tool != ProviderId::Codex {
            self.api.owned_by(
                access,
                Owner {
                    account_uuid: person.id.clone(),
                    email: person.email.clone(),
                    organization_uuid: format!("org-{}", person.id),
                },
            );
        }
        self.api.using(access, self.used_by(person));
        #[cfg(test)]
        self.tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(person.id.clone())
            .or_default()
            .push(access.to_owned());
    }

    /// Every access token `person`'s logins have had.
    #[cfg(test)]
    fn tokens_of(&self, person: &Person) -> Vec<String> {
        self.tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&person.id)
            .cloned()
            .unwrap_or_default()
    }

    /// `login` is the one `person`'s tool has signed in, as signing in with the tool itself
    /// leaves it.
    fn signs_in(&self, person: &Person, login: &Login) -> Result<(), Unmade> {
        match person.tool {
            ProviderId::Claude => {
                self.host
                    .live()
                    .plant(&live_service(&self.base), &login.document);
                let config = json!({
                    "oauthAccount": {
                        "accountUuid": person.id,
                        "emailAddress": person.email,
                        "organizationUuid": format!("org-{}", person.id),
                    }
                });
                std::fs::write(self.home().join(".claude.json"), config.to_string())?;
            }
            ProviderId::Codex => {
                let home = self.home().join(".codex");
                std::fs::create_dir_all(&home)?;
                self.host
                    .file_at(home.join("auth.json"))
                    .plant("auth.json", &login.document);
                // The core reads and writes the login through the machine in memory, and
                // doctor asks the disk who may read the file, so it is there too, private,
                // as Codex makes it.
                private_file(&home.join("auth.json"), &login.document)?;
            }
            other => return Err(Unmade(format!("no fixture signs in to {}", other.name()))),
        }
        Ok(())
    }

    /// `person` signs in to their tool, and `caller` enrols them as `label`, `ago` seconds
    /// before the world was made: `pitboard enroll <label>` in a terminal, or Name in the app.
    fn enrolled(
        &self,
        person: &Person,
        label: &str,
        login: &Login,
        caller: &str,
        ago: i64,
    ) -> Result<(), Unmade> {
        self.signs_in(person, login)?;
        let typed = Key::new(person.tool, label).qualified();
        self.front_end(caller, ago)
            .1
            .enroll_current(&typed)
            .map(drop)
            .map_err(refused("enrolling the account signed in"))
    }

    /// `person` signs in privately and `caller` enrols them as `label`, `ago` seconds before
    /// the world was made, which parks their login beside the one in use: `pitboard enroll
    /// <label> --sign-in`, or Add Account in the app. No tool runs: the sign-in is put there
    /// as one that finished.
    fn parked(
        &self,
        person: &Person,
        label: &str,
        login: &Login,
        caller: &str,
        ago: i64,
    ) -> Result<(), Unmade> {
        let (ctx, front_end) = self.front_end(caller, ago);
        let signed_in = pitboard_core::testing::signed_in(&ctx, person.tool, &login.document)
            .map_err(|error| Unmade(format!("signing in privately: {error}")))?;
        let typed = Key::new(person.tool, label).qualified();
        front_end
            .enroll_signed_in(&typed, signed_in)
            .map(drop)
            .map_err(refused("enrolling an account signed in privately"))
    }

    /// The person enrolled as `label` of `which`'s tool, as the account index has them now.
    pub(crate) fn enrolled_as(&self, which: ProviderId, label: &str) -> Option<Person> {
        let account = service::Pitboard::new(self.base.clone())
            .account(&Key::new(which, label).qualified())?;
        Some(Person {
            tool: which,
            id: account.account_uuid,
            email: account.email,
        })
    }

    /// Now, by the clock the app's core reads: the machine's own.
    pub(crate) fn now_on_its_clock(&self) -> i64 {
        epoch_now()
    }

    /// `caller` switches to `qualified`, `ago` seconds before the world was made.
    fn switched(&self, qualified: &str, caller: &str, ago: i64) -> Result<(), service::Failed> {
        self.front_end(caller, ago).1.switch_to(qualified).map(drop)
    }
}

/// The people each world's accounts belong to.
fn work() -> Person {
    Person::new(ProviderId::Claude, "dana-work", "dana@work.example")
}

fn personal() -> Person {
    Person::new(ProviderId::Claude, "dana-home", "dana@home.example")
}

fn old() -> Person {
    Person::new(ProviderId::Claude, "dana-old", "dana@old.example")
}

fn codex_main() -> Person {
    Person::new(ProviderId::Codex, "chatgpt-work", "dana@work.example")
}

fn codex_spare() -> Person {
    Person::new(ProviderId::Codex, "chatgpt-home", "dana@home.example")
}

/// A world made, and what the app's model is made over.
pub(crate) struct Launched {
    pub(crate) core: Arc<AppCore>,
    pub(crate) apps: Arc<FixtureApps>,
    /// Where the account windows' records are kept: in the fixture's own folder, made again
    /// at each launch, never in the app's own directory.
    pub(crate) windows: WindowsLaunch,
    /// The machine, for a test to look at. The core keeps it as long as it needs it.
    #[cfg(test)]
    pub(crate) machine: Arc<Machine>,
}

/// The model of `world`, made in `folder`, telling `listener` and saying clock times as
/// `local_time` does.
pub(crate) fn launch(
    world: World,
    folder: Folder,
    listener: Arc<dyn ModelListener>,
    local_time: Arc<dyn LocalTime>,
) -> Result<Arc<PitboardModel>, Unmade> {
    Ok(make(world, folder)?.model(listener, local_time))
}

impl Launched {
    /// The app's model over this world, as an app has it: at the app's own pace, with the
    /// world's apps, posting nothing.
    pub(crate) fn model(
        &self,
        listener: Arc<dyn ModelListener>,
        local_time: Arc<dyn LocalTime>,
    ) -> Arc<PitboardModel> {
        PitboardModel::over(
            Arc::clone(&self.core),
            listener,
            Platform {
                apps: Arc::clone(&self.apps) as Arc<dyn crate::model::AppControl>,
                notifications: Arc::new(Unposted),
                local_time,
                earlier: None,
                windows: Some(WindowsPlace {
                    launch: self.windows.clone(),
                    web_scheme: super::pages::SCHEME.into(),
                }),
            },
            Cadence::APP,
        )
    }
}

/// `world`, made in `folder`, and the app's core over it.
pub(crate) fn make(world: World, folder: Folder) -> Result<Launched, Unmade> {
    let machine = Arc::new(Machine::new(folder)?);
    install(&machine, world.installed())?;
    seed(world, &machine)?;
    if world != World::FirstLaunch {
        let seen = Preferences {
            has_been_seen: true,
            ..Preferences::default()
        };
        if let Some(text) = seen.text() {
            service::Pitboard::new(machine.base.clone())
                .keep_app_file(AppFile::Preferences, &text)
                .map_err(|error| Unmade(format!("keeping the app's preferences: {error}")))?;
        }
    }
    let apps = FixtureApps::new(
        Arc::clone(&machine.host),
        if world == World::ChatGptOpen {
            &[CHATGPT]
        } else {
            &[]
        },
    );
    let ctx = machine.app_context(Arc::new(Browser::new(Arc::clone(&machine))));
    let found = world.installed().to_vec();
    let (helper, places) = (machine.helper(), vec![machine.bin()]);
    let core = Arc::new(AppCore::asking(
        move || {
            let made = Made {
                core: service::Pitboard::new(ctx.clone()),
                found: found.clone(),
                search_path: None,
                helper: Some(helper.clone()),
                command_line_places: places.clone(),
            };
            (made, false)
        },
        crate::ASK_AGAIN_AFTER,
    ));
    let windows = WindowsLaunch {
        directory: machine.root().to_string_lossy().into_owned(),
        key: machine
            .home()
            .join(".pitboard")
            .to_string_lossy()
            .into_owned(),
        link_scheme: LINK_SCHEME.into(),
        earlier: None,
    };
    Ok(Launched {
        core,
        apps,
        windows,
        #[cfg(test)]
        machine,
    })
}

/// The programs of `tools` and the command line inside the stand-in app, each a file that is
/// there to be found and is never run: a sign-in is played by the fixture's browser, and the
/// schedule's scheduler starts nothing. Were one run, it would refuse and do nothing.
fn install(machine: &Machine, tools: &[ProviderId]) -> Result<(), Unmade> {
    for &tool in tools {
        never_run(&Machine::program_in(machine.root(), tool))?;
    }
    never_run(&machine.helper())?;
    std::fs::create_dir_all(machine.bin())?;
    Ok(())
}

/// A file at `path` holding `contents` that only its owner can read or write, as Codex leaves
/// its login.
fn private_file(path: &Path, contents: &str) -> Result<(), Unmade> {
    std::fs::write(path, contents)?;
    Ok(files::make_private(path)?)
}

/// A program at `path` that refuses to do anything.
fn never_run(path: &Path) -> Result<(), Unmade> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    refusing_program(path)
}

/// In this crate's own tests, the compiled stand-in the tests start in place of every
/// program, which refuses the same way on every system, and is found as a program on each.
#[cfg(test)]
fn refusing_program(path: &Path) -> Result<(), Unmade> {
    use pitboard_core::testing::stand_in::{self, Script};
    let never = Script::refusing("a Pitboard fixture's program, never meant to run\n");
    stand_in::install(path, &never).map_err(|error| Unmade(format!("{error}")))
}

/// In an app's debug build, which no compiled stand-in comes with, a script that refuses
/// where it is run.
#[cfg(not(test))]
fn refusing_program(path: &Path) -> Result<(), Unmade> {
    let script = match OS {
        Os::MacOs | Os::Linux => "#!/bin/sh\nexit 64\n",
        Os::Windows => "@exit /b 64\r\n",
    };
    std::fs::write(path, script)?;
    Ok(files::make_runnable(path)?)
}

/// Puts `world`'s accounts on `machine`, each as a person would have put it there.
fn seed(world: World, machine: &Machine) -> Result<(), Unmade> {
    match world {
        // In the order it happened, which is the order the log keeps.
        World::TwoTools | World::ChatGptOpen => {
            expired(machine)?;
            codex_in_use(machine)?;
            claude_accounts(machine)?;
            codex_parked(machine)?;
            claude_switches(machine)
        }
        World::OneTool => {
            claude_accounts(machine)?;
            claude_switches(machine)
        }
        World::ReadFailure => {
            claude_accounts(machine)?;
            claude_switches(machine)?;
            unreadable_index(machine)
        }
        World::Stuck => {
            claude_accounts(machine)?;
            claude_switches(machine)?;
            interrupted(machine)
        }
        World::OnlyOne => {
            let work = work();
            uses(machine, &work);
            let login = machine.login(&work, machine.now + DAY, machine.now + 30 * DAY);
            machine.enrolled(&work, "work", &login, "cli", 3 * DAY)
        }
        World::Unnamed => {
            let work = work();
            uses(machine, &work);
            let login = machine.login(&work, machine.now + DAY, machine.now + 30 * DAY);
            machine.signs_in(&work, &login)
        }
        World::Empty | World::FirstLaunch | World::NoClaudeCode => Ok(()),
    }
}

/// What `person` has used of their limits.
fn uses(machine: &Machine, person: &Person) {
    let session = 5 * HOUR;
    let week = 7 * DAY;
    if *person == work() {
        machine.has_used(
            person,
            &[
                ("session", 42.0, session, 7_800),
                ("weekly_all", 12.0, week, 356_000),
            ],
        );
    } else if *person == personal() {
        machine.has_used(
            person,
            &[
                ("session", 100.0, session, 4_800),
                ("weekly_all", 61.0, week, 190_000),
            ],
        );
    } else if *person == codex_main() {
        machine.has_used(
            person,
            &[
                ("five_hour", 18.0, session, 12_000),
                ("seven_day", 7.0, week, 500_000),
            ],
        );
    } else if *person == codex_spare() {
        machine.has_used(person, &[("five_hour", 0.0, session, session)]);
    }
}

/// Claude Code's two accounts: work signed in and enrolled from a terminal three days ago,
/// and personal signed in privately and parked from the app two days ago. Personal's parked
/// login lasts two days more, so it is due to be renewed, which Renew Now does, and doctor
/// warns of it, as it does of every parked login that is due.
fn claude_accounts(machine: &Machine) -> Result<(), Unmade> {
    let (work, personal) = (work(), personal());
    uses(machine, &work);
    uses(machine, &personal);
    let now = machine.now;
    let login = machine.login(&work, now + DAY, now + 30 * DAY);
    machine.enrolled(&work, "work", &login, "cli", 3 * DAY)?;
    let login = machine.login(&personal, now + DAY, now + 2 * DAY);
    machine.parked(&personal, "personal", &login, "app", 2 * DAY)
}

/// A switch to personal from a terminal two hours ago, and one back to work from the app an
/// hour ago, which leave work in use and personal parked.
fn claude_switches(machine: &Machine) -> Result<(), Unmade> {
    machine
        .switched("claude/personal", "cli", 2 * HOUR)
        .map_err(refused("switching to personal"))?;
    machine
        .switched("claude/work", "app", HOUR)
        .map_err(refused("switching back to work"))
}

/// A Claude Code account parked a month ago and never switched to since: its access token
/// lapsed thirty days ago and its refresh token yesterday, so it needs a sign-in.
fn expired(machine: &Machine) -> Result<(), Unmade> {
    let old = old();
    let now = machine.now;
    let login = machine.login(&old, now - 30 * DAY, now - DAY);
    machine.parked(&old, "old", &login, "cli", 31 * DAY)
}

/// Codex signed in as main and enrolled from a terminal four days ago.
fn codex_in_use(machine: &Machine) -> Result<(), Unmade> {
    let main = codex_main();
    uses(machine, &main);
    let now = machine.now;
    let login = machine.login(&main, now + 10 * DAY, now + 60 * DAY);
    machine.enrolled(&main, "main", &login, "cli", 4 * DAY)
}

/// Codex's spare signed in privately and parked from the app a day ago.
fn codex_parked(machine: &Machine) -> Result<(), Unmade> {
    let spare = codex_spare();
    uses(machine, &spare);
    let now = machine.now;
    let login = machine.login(&spare, now + 10 * DAY, now + 60 * DAY);
    machine.parked(&spare, "spare", &login, "app", DAY)
}

/// Pitboard's account index, `state.json` in the fixture's own Pitboard directory, made one
/// nobody may read once everything in it was put there: what the person whose home it is in
/// meets where a `pitboard` run as somebody else wrote it, since the core writes it private
/// to whoever writes it. So every read of the accounts fails as it does on such a machine,
/// `state_unreadable`, the read of what is known with it. Checked, since the system's
/// administrator reads a file whatever its mode says, and a system without Unix modes has
/// none to give it: there, the world is not made, rather than made otherwise than its name
/// says.
fn unreadable_index(machine: &Machine) -> Result<(), Unmade> {
    files::deny_reading(&machine.home().join(".pitboard").join("state.json"))?;
    match service::Pitboard::new(machine.base.clone()).status_offline() {
        Err(error) if error.code() == "state_unreadable" => Ok(()),
        Err(error) => Err(Unmade(format!(
            "reading the unreadable account index: {error}"
        ))),
        Ok(_) => Err(Unmade(
            "the account index can still be read: this user reads a file whatever its mode \
             says, or this system gives files no mode"
                .into(),
        )),
    }
}

/// Anthropic out of reach from now on, once the accounts have been read: what was measured
/// is remembered, and nothing newer can be. No world is made this way; a test makes oneTool
/// so, as readFailure was before it was given an account index the core cannot read.
#[cfg(test)]
pub(crate) fn out_of_reach(machine: &Machine) -> Result<(), Unmade> {
    let (_, front_end) = machine.front_end("app", 10 * 60);
    front_end
        .status(true)
        .map(drop)
        .map_err(|error| Unmade(format!("reading the accounts: {error}")))?;
    for person in [work(), personal()] {
        for access in machine.tokens_of(&person) {
            machine
                .api
                .token_trouble(&access, pitboard_core::testing::Trouble::Offline);
        }
    }
    Ok(())
}

/// A switch to personal from the app ten minutes ago that nothing can finish. Claude Code's
/// keychain locked as the switch wrote personal's login, so it could not tell what it had
/// written and kept its record of the switch, and every copy. Claude Code has since renewed
/// work's login in the keychain, so it matches neither side the record kept, and its session
/// has expired, so Anthropic will not say whose it is either.
fn interrupted(machine: &Machine) -> Result<(), Unmade> {
    let live = live_service(&machine.base);
    machine.host.live().fault(&live, Fault::LocksOnWrite);
    let locked = machine.switched("claude/personal", "app", 10 * 60);
    machine.host.live().heal_all();
    match locked {
        Err(failed) if failed.error.code() == "switch_unverified" => {}
        Err(failed) => return Err(refused("interrupting a switch")(failed)),
        Ok(()) => return Err(Unmade("the switch was not interrupted".into())),
    }
    let renewed = machine.login(&work(), machine.now + DAY, machine.now + 30 * DAY);
    // Anthropic no longer accepts it: whose it is cannot be asked.
    machine.api.token_trouble(
        &renewed.access,
        pitboard_core::testing::Trouble::Unauthorized,
    );
    machine.host.live().plant(&live, &renewed.document);
    Ok(())
}

fn epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// A token in the shape a real one has, with a signature nothing checks, as Codex's login
/// carries its claims: three parts of unpadded base64url.
fn unsigned_token(payload: &serde_json::Value) -> String {
    [
        r#"{"alg":"RS256"}"#.as_bytes(),
        payload.to_string().as_bytes(),
        b"not a real signature",
    ]
    .map(base64url)
    .join(".")
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let held = chunk.iter().enumerate().fold(0u32, |held, (at, &byte)| {
            held | (u32::from(byte) << (16 - 8 * at))
        });
        for at in 0..(chunk.len() * 8).div_ceil(6) {
            let sextet = (held >> (18 - 6 * at)) & 0x3f;
            out.push(char::from(ALPHABET[sextet as usize]));
        }
    }
    out
}
