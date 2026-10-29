//! Windows, while its face is being written: every answer not written yet refuses, through an
//! error or an answer its callers already have, so that nothing of Windows is read or changed
//! by a part that is not built yet.
//!
//! A Windows build of a 0.x release refuses before it gets here ([`crate::release`]). A build
//! opened to Windows, for working on Pitboard and for CI, reaches this face.
//!
//! Which store a tool keeps its login in is the tool's to say, in its provider's code.

pub(crate) mod credman;
#[allow(
    clippy::disallowed_methods,
    reason = "the face every file and folder Pitboard makes is made through"
)]
pub(crate) mod fs;
mod job;
pub(crate) mod proc;
pub(super) mod seal;
mod settings;
mod tasks;
pub(crate) mod user;
pub(super) mod watch;

use super::administered::{Administered, Hive};
use super::{Elevation, Floor, Host, Jar, JarError, Listed, LoginPath, Os, Scheduler};
use crate::context::{Context, Environment};
use crate::state::VaultForm;
use crate::store::vault::FileVault;
use crate::store::{PlainFile, RawStore};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) const OS: Os = Os::Windows;

// Instant reads QueryPerformanceCounter here, which counts a sleep; unbiased interrupt time
// does not.
pub(super) fn awake() -> std::time::Duration {
    let mut units = 0u64;
    // SAFETY: the pointer is to a u64 of this frame, which the call writes and does not keep.
    unsafe {
        windows_sys::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTimePrecise(
            &raw mut units,
        );
    }
    super::from_interrupt_time(units)
}

// What a program started now is given, which one started before a change was not.
pub(super) fn login_path(_env: &Environment) -> LoginPath {
    if cfg!(any(test, feature = "test-support")) {
        return LoginPath::Unknown;
    }
    registry_path()
}

const MACHINE_ENVIRONMENT: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
const USER_ENVIRONMENT: &str = "Environment";

// What the account's own environment, then the machine's, names `name`, read whatever kind of
// build this is: a program started from now on is given it, whatever this one was started with.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn lasting_environment(name: &str) -> Vec<String> {
    [
        (windows_registry::CURRENT_USER, USER_ENVIRONMENT),
        (windows_registry::LOCAL_MACHINE, MACHINE_ENVIRONMENT),
    ]
    .into_iter()
    .filter_map(|(hive, key)| expanded(hive.open(key).ok()?, name))
    .collect()
}

fn environment(hive: &windows_registry::Key, key: &str, name: &str) -> Administered {
    if !super::administered::READ_BY_REAL_HOSTS {
        return Administered::Unset;
    }
    hive.open(key)
        .ok()
        .and_then(|key| expanded(key, name))
        .map_or(Administered::Unset, Administered::Set)
}

fn registry_path() -> LoginPath {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};
    let parts: Vec<String> = [
        (LOCAL_MACHINE, MACHINE_ENVIRONMENT),
        (CURRENT_USER, USER_ENVIRONMENT),
    ]
    .into_iter()
    .filter_map(|(hive, key)| expanded(hive.open(key).ok()?, "Path"))
    .filter(|part| !part.is_empty())
    .collect();
    if parts.is_empty() {
        return LoginPath::Unknown;
    }
    LoginPath::Said(parts.join(";"))
}

fn expanded(key: windows_registry::Key, name: &str) -> Option<String> {
    use windows_registry::Type;
    let kind = key.get_type(name).ok()?;
    if !matches!(kind, Type::String | Type::ExpandString) {
        return None;
    }
    let text = key.get_hstring(name).ok()?;
    if kind == Type::String {
        return Some(text.to_string_lossy());
    }
    let unexpanded: Vec<u16> = text.iter().copied().chain([0]).collect();
    let mut out = vec![0u16; 1024];
    loop {
        // SAFETY: a NUL-terminated string, and a buffer of the length given.
        let needed = unsafe {
            windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW(
                unexpanded.as_ptr(),
                out.as_mut_ptr(),
                out.len() as u32,
            )
        } as usize;
        if needed == 0 {
            return None;
        }
        if needed <= out.len() {
            return Some(String::from_utf16_lossy(&out[..needed - 1]));
        }
        out.resize(needed, 0);
    }
}

// As `reg.exe query` prints it for Claude Code: unexpanded, trimmed, and only a string.
fn policy(hive: Hive, key: &str, value: &str) -> Administered {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE, Type};
    let root = match hive {
        Hive::Machine => LOCAL_MACHINE,
        Hive::User => CURRENT_USER,
    };
    let read = root
        .open(key)
        .and_then(|opened| match opened.get_type(value)? {
            Type::String | Type::ExpandString => opened.get_hstring(value).map(Some),
            _ => Ok(None),
        });
    match read {
        Ok(Some(text)) => Administered::Set(text.to_string_lossy().trim_end().to_owned()),
        Ok(None) => Administered::Unset,
        // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND): no such key or value.
        Err(e) if e.code().0 as u32 == 0x8007_0002 => Administered::Unset,
        Err(e) => Administered::Unreadable(format!("cannot be read: {e}")),
    }
}

#[derive(Debug)]
struct Windows {
    scheduler: Option<super::task::Tasks>,
}

impl Host for Windows {
    fn foreign_secrets(&self, ctx: &Context, account: &str) -> Option<Box<dyn RawStore>> {
        Some(Box::new(credman::Generic::of(ctx, account)))
    }

    fn file(&self, path: PathBuf) -> Box<dyn RawStore> {
        Box::new(PlainFile::at(path))
    }

    fn vault(&self, ctx: &Context, _form: Option<VaultForm>) -> Box<dyn RawStore> {
        Box::new(FileVault::sealed(ctx, &seal::Dpapi))
    }

    // Inside this home.
    fn vault_is_shared(&self, _form: Option<VaultForm>) -> bool {
        false
    }

    fn listing(&self) -> Result<Vec<Listed>, String> {
        proc::listing()
    }

    fn cookies(&self, _jar: &Path, _hosts: &[&str], _names: &[&str]) -> Result<Jar, JarError> {
        Err(JarError::Unsupported)
    }

    fn app_version(&self, _bundle: &Path) -> Option<String> {
        None
    }

    fn app_id(&self, _bundle: &Path) -> Option<String> {
        None
    }

    fn scheduler(&self) -> Option<&dyn Scheduler> {
        self.scheduler.as_ref().map(|tasks| tasks as &dyn Scheduler)
    }

    fn elevation(&self, ctx: &Context) -> Elevation {
        user::elevation(ctx.sudo())
    }

    fn floor(&self) -> Floor {
        user::floor()
    }

    // Windows has none: what an administrator sets for Codex there is in files.
    fn managed_preference(&self, _domain: &str, _key: &str) -> Administered {
        Administered::Unset
    }

    fn policy(&self, hive: Hive, key: &str, value: &str) -> Administered {
        if super::administered::READ_BY_REAL_HOSTS {
            policy(hive, key, value)
        } else {
            Administered::Unset
        }
    }

    fn program_data(&self) -> Option<PathBuf> {
        user::program_data()
    }

    fn fixed_drives(&self) -> Vec<char> {
        fs::fixed_drives()
    }

    // The account's own items: no test's business, as what an administrator set is not.
    fn credential_names(&self, prefix: &str) -> Result<Vec<String>, String> {
        if !super::administered::READ_BY_REAL_HOSTS {
            return Ok(Vec::new());
        }
        credman::names(&format!("{prefix}*")).map_err(|e| e.to_string())
    }

    fn user_environment(&self, name: &str) -> Administered {
        environment(windows_registry::CURRENT_USER, USER_ENVIRONMENT, name)
    }

    fn machine_environment(&self, name: &str) -> Administered {
        environment(windows_registry::LOCAL_MACHINE, MACHINE_ENVIRONMENT, name)
    }

    #[cfg_attr(not(any(test, feature = "test-support")), allow(unused_variables))]
    fn credential_held(&self, ctx: &Context, target: &str) -> Result<bool, crate::store::Error> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(why) =
            crate::provider::claude::credman::refused_in_a_build_for_tests(ctx, target)
        {
            return Err(crate::store::Error::Unreadable(why));
        }
        credman::held(target).map_err(|e| credman::failed(&e, crate::store::Error::Unreadable))
    }

    fn windows_settings(&self) -> super::WindowsSettings {
        if super::administered::READ_BY_REAL_HOSTS {
            settings::read()
        } else {
            super::WindowsSettings::default()
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn write_jar(_path: &Path, _cookies: &[super::Cookie]) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

pub(super) fn host() -> Arc<dyn Host> {
    Arc::new(Windows {
        scheduler: user::sid().map(|sid| super::task::Tasks::new(sid, Arc::new(tasks::System))),
    })
}

// The path it was started by, through Scoop's junction or winget's link, which an upgrade keeps
// (docs/facts/windows-runners.md).
pub(super) fn current_program() -> std::io::Result<PathBuf> {
    let same = |a: &Path, b: &Path| {
        std::fs::canonicalize(a).is_ok_and(|a| std::fs::canonicalize(b).is_ok_and(|b| a == b))
    };
    Ok(super::program::lasting(std::env::current_exe()?, same))
}

// Task Scheduler as a machine in memory has it, keeping its task in the test's own home.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn pretend_scheduler(
    refuse_start: Arc<std::sync::atomic::AtomicBool>,
) -> Option<Box<dyn Scheduler>> {
    let pretend = Arc::new(super::task::Pretend { refuse_start });
    user::sid().map(|sid| Box::new(super::task::Tasks::new(sid, pretend)) as Box<dyn Scheduler>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderId;

    /// What a unit test's context reaches here: this face, with every home its own.
    fn ctx() -> Context {
        Context::for_unit_test()
    }

    // Nothing turns Credential Manager on for a test, and it holds nothing at a test's slot.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test writes a login as Claude Code does"
    )]
    fn claude_codes_chain_is_its_file_where_credential_manager_holds_nothing() {
        let home = tempfile::tempdir().expect("a scratch home");
        let config = home.path().join(".claude");
        std::fs::create_dir_all(&config).expect("a config folder");
        let ctx = Context::new(home.path().to_path_buf())
            .with_claude_config_dir(config.to_string_lossy().into_owned());
        let live = crate::provider::document(ProviderId::Claude)
            .live(&ctx)
            .expect("a chain");
        assert_eq!(
            crate::store::read_raw(&live.chain, &live.service).expect("readable"),
            None
        );
        std::fs::write(config.join(".credentials.json"), r#"{"claudeAiOauth":{}}"#)
            .expect("a login");
        assert_eq!(
            crate::store::holder(&live.chain, &live.service)
                .expect("readable")
                .map(|held| held.kind()),
            Some(crate::store::Backend::File)
        );

        let signin = crate::switch::enroll::signin_dir(&ctx);
        std::fs::create_dir_all(&signin).expect("a sign-in folder");
        std::fs::write(signin.join(".credentials.json"), r#"{"claudeAiOauth":{}}"#)
            .expect("a sign-in's login");
        assert_eq!(
            crate::provider::claude::live::login_from_sign_in(&ctx, &signin)
                .expect("readable")
                .as_deref(),
            Some(r#"{"claudeAiOauth":{}}"#)
        );
        crate::provider::claude::live::discard_signin(
            &ctx,
            crate::permit::Permit::for_a_test(),
            &signin,
        )
        .expect("discarded");
        assert!(!signin.join(".credentials.json").exists());
    }

    // Refused before `CredReadW`: a build for tests never reads a real login's blob into memory.
    #[test]
    fn a_build_for_tests_looks_at_its_own_slot_alone() {
        let home = tempfile::tempdir().expect("a scratch home");
        let config = home.path().join(".claude");
        let ctx = Context::new(home.path().to_path_buf())
            .with_claude_config_dir(config.to_string_lossy().into_owned());
        let own = crate::provider::claude::slot::service_for_dir(&config.to_string_lossy());
        for piece in ["", "#m"] {
            let held = host().credential_held(&ctx, &format!("{own}/claude-code-user{piece}"));
            assert!(matches!(held, Ok(false)), "{piece}: {held:?}");
        }
        for live in [
            "Claude Code-credentials/claude-code-user",
            "Claude Code-credentials/claude-code-user#m",
        ] {
            let held = host().credential_held(&Context::new(home.path().to_path_buf()), live);
            let Err(crate::store::Error::Unreadable(refused)) = held else {
                panic!("{live}: {held:?}");
            };
            assert!(
                refused.starts_with("a test must never address"),
                "{refused}"
            );
        }
    }

    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test writes a login as Codex does"
    )]
    fn codexs_live_login_is_auth_json_in_its_home() {
        let home = tempfile::tempdir().expect("a scratch Codex home");
        let ctx = ctx().with_codex_home(home.path().to_string_lossy().into());
        let live = crate::provider::document(ProviderId::Codex)
            .live(&ctx)
            .expect("the file store");
        assert_eq!(
            crate::store::read_raw(&live.chain, &live.service).expect("readable"),
            None
        );
        std::fs::write(home.path().join("auth.json"), r#"{"tokens":{}}"#).expect("a login");
        assert_eq!(
            crate::store::read_raw(&live.chain, &live.service)
                .expect("readable")
                .as_deref(),
            Some(r#"{"tokens":{}}"#)
        );
    }

    #[test]
    fn the_vault_seals_each_park_in_this_home_alone() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let ctx = Context::new(scratch.path().to_path_buf())
            .with_pitboard_home(scratch.path().join("pitboard"));
        let vault = ctx.host().vault(&ctx, None);
        let (park, login) = (
            "pitboard-park-1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0-1789935600123",
            r#"{"claudeAiOauth":{"refreshToken":"r"}}"#,
        );
        assert_eq!(vault.kind(), crate::store::Backend::SealedFile);
        assert!(!host().vault_is_shared(None));
        vault
            .write(crate::permit::Permit::for_a_test(), park, login)
            .expect("sealed");
        assert_eq!(vault.read(park).expect("opened").as_deref(), Some(login));
        assert_eq!(vault.list().expect("listed"), Some(vec![park.to_string()]));
        let held = std::fs::read(crate::store::vault_dir(&ctx).join(format!("{park}.json")))
            .expect("the file");
        assert!(
            !held
                .windows(login.len())
                .any(|bytes| bytes == login.as_bytes())
        );
    }

    // Copies of this test's program, each kept running by a helper until it is let go.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test puts its stand-ins where a person would install a program"
    )]
    fn a_codex_this_user_runs_is_found_where_it_runs_from_in_any_case() {
        use crate::helper::{Helper, Job};
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let programs = [
            dir.join("a").join("codex.exe"),
            dir.join("b").join("Codex.EXE"),
        ];
        let helpers: Vec<Helper> = programs
            .iter()
            .enumerate()
            .map(|(index, program)| {
                std::fs::create_dir_all(program.parent().expect("its folder")).expect("a folder");
                std::fs::copy(std::env::current_exe().expect("this test"), program)
                    .expect("a stand-in");
                Helper::start_as(program, Job::Locks, &dir.join(format!("{index}.lock")))
            })
            .collect();
        let found = host().processes("codex").expect("the process list reads");
        for (program, helper) in programs.iter().zip(&helpers) {
            let one = found
                .iter()
                .find(|one| one.pid == helper.pid())
                .unwrap_or_else(|| panic!("{} runs: {found:?}", program.display()));
            assert_eq!(
                std::fs::canonicalize(&one.path).expect("the path it runs from"),
                std::fs::canonicalize(program).expect("the stand-in"),
                "{}",
                one.path.display()
            );
            assert_eq!(one.package, None);
        }
        let listed = host().listing().expect("the process list reads");
        let me = listed
            .iter()
            .find(|one| one.pid == std::process::id())
            .expect("this test is listed");
        assert!(me.placed);
        for helper in helpers {
            assert_eq!(
                listed
                    .iter()
                    .find(|one| one.pid == helper.pid())
                    .map(|one| one.parent),
                Some(std::process::id()),
                "a helper's parent is this test"
            );
            helper.stop();
        }
    }

    #[test]
    fn no_cookie_jar_or_app_is_read() {
        assert!(matches!(
            host().cookies(Path::new("C:/x/Cookies"), &["claude.ai"], &["sessionKey"]),
            Err(JarError::Unsupported)
        ));
        assert_eq!(host().app_version(Path::new("C:/x")), None);
    }

    // Asked nothing: a test's task would be the person's own, whatever home the test gives.
    #[test]
    fn the_real_task_scheduler_refuses_a_build_for_tests() {
        use crate::schedule::{Job, Location};
        let ctx = ctx();
        let real = host();
        let tasks = real.scheduler().expect("Task Scheduler");
        let sid = user::sid().expect("this account's SID");
        for (job, named) in [(Job::Renew, "Renew"), (Job::Watch, "Watch")] {
            assert_eq!(
                tasks.location(&ctx, job),
                Location::Task(format!(r"\Pitboard\{named} {sid}"))
            );
            assert!(!tasks.installed(&ctx, job));
            assert_eq!(tasks.task(&ctx, job), None);
            let refused = tasks
                .put(
                    &ctx,
                    crate::permit::Permit::for_a_test(),
                    job,
                    Path::new(r"C:\Program Files\Pitboard\bin\pitboard.exe"),
                    &[],
                    &[],
                )
                .expect_err("refused");
            assert_eq!(refused.code(), "schedule_refused");
            assert!(
                refused.to_string().contains("a build for tests"),
                "{refused}"
            );
            assert!(
                !tasks
                    .remove(&ctx, crate::permit::Permit::for_a_test(), job)
                    .expect("nothing there")
            );
        }
        assert!(matches!(
            crate::schedule::status(&ctx),
            crate::schedule::Installed::No
        ));
        assert!(
            crate::host::memory::MemoryHost::new()
                .scheduler()
                .is_some_and(|pretend| {
                    pretend.location(&ctx, Job::Renew) == tasks.location(&ctx, Job::Renew)
                })
        );
    }

    #[test]
    fn as_the_person_the_gate_lets_a_change_through() {
        let ctx = ctx();
        assert_eq!(
            ctx.host().elevation(&ctx),
            Elevation::Normal,
            "Pitboard's tests run as the person: run them from a terminal that is not elevated"
        );
        assert_eq!(user::elevation(true), Elevation::Normal);
        assert_eq!(ctx.host().floor(), Floor::Met);
        assert!(crate::permit::gate(&ctx).is_ok());
    }

    // Claude Desktop's folders are never moved here.
    #[test]
    fn no_apps_folder_is_moved() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().join("dir");
        let permit = crate::permit::Permit::for_a_test();
        let unsupported = |made: std::io::Result<()>| {
            assert_eq!(
                made.map_err(|e| e.kind()),
                Err(std::io::ErrorKind::Unsupported)
            );
        };
        unsupported(fs::private_dir(&dir).map(drop));
        unsupported(fs::lock_dir(&dir).map(drop));
        unsupported(fs::move_new(permit, &dir, &dir.join("x")));
        unsupported(fs::same_volume(&dir, &dir).map(drop));
        unsupported(write_jar(&dir.join("Cookies"), &[]));
        assert!(!dir.exists(), "nothing was made");
        assert_eq!(user::login_name(), None);
    }

    #[test]
    fn a_policy_is_read_as_reg_exe_prints_it() {
        let current = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let (_, _, build) = user::version().expect("RtlGetVersion");
        assert_eq!(
            policy(Hive::Machine, current, "CurrentBuild"),
            Administered::Set(build.to_string())
        );
        assert_eq!(
            policy(Hive::Machine, current, "pitboard-absent"),
            Administered::Unset
        );
        for hive in [Hive::Machine, Hive::User] {
            assert_eq!(
                policy(hive, r"SOFTWARE\Policies\pitboard-absent", "Settings"),
                Administered::Unset
            );
        }
        assert_eq!(
            host().policy(Hive::Machine, current, "CurrentBuild"),
            Administered::Unset
        );
    }

    // The machine's `Path` holds `%SystemRoot%\System32`, expanded.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "where Windows is, as it names it"
    )]
    fn the_path_a_program_started_now_gets_is_read_from_the_registry() {
        assert_eq!(login_path(&Environment::default()), LoginPath::Unknown);
        let LoginPath::Said(path) = registry_path() else {
            panic!("every Windows has a Path");
        };
        let system = std::env::var_os("SystemRoot")
            .map(|windows| PathBuf::from(windows).join("System32"))
            .expect("SystemRoot");
        assert!(
            std::env::split_paths(&path)
                .any(|dir| crate::host::same_path_in_any_case(&dir, &system)),
            "{path}"
        );
    }
}
