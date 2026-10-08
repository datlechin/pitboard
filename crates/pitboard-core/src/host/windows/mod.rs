//! Windows, while its face is being written: every answer not written yet refuses, through an
//! error or an answer its callers already have, so that nothing of Windows is read or changed
//! by a part that is not built yet.
//!
//! A Windows build of a 0.x release refuses before it gets here ([`crate::release`]). A build
//! opened to Windows, for working on Pitboard and for CI, reaches this face, and finds:
//!
//! - no store of secrets for a tool (W23 offers Credential Manager), and a vault whose every
//!   call cannot be read, of a kind of its own, [`Backend::Unknown`] (W20 seals parks to the
//!   person);
//! - no process list (W18), so a switch says nobody can tell what still runs a tool, and
//!   every process may still be running;
//! - no scheduler (W25);
//! - no home of the account's own (W14), which is refused as a home that is not a full path;
//! - no file made, private or not, and no access read (W15, W16);
//! - no program found (W17), and no `PATH` of a login shell, which Windows does not have.
//!
//! Each tool's own live chain refuses on Windows in its provider's code, not here: which
//! store a tool keeps its login in is the tool's to say (W21, W22, W23).

pub(crate) mod fs;
pub(crate) mod proc;
pub(crate) mod user;

use super::administered::Administered;
use super::{Elevation, Floor, Host, LoginPath, Os, Process, Scheduler};
use crate::context::{Context, Environment};
use crate::store::{Backend, PlainFile, RawStore, Unbuilt};
use std::path::PathBuf;
use std::sync::Arc;

pub(super) const OS: Os = Os::Windows;

/// Windows has no login shell whose startup files build a `PATH`: a program's environment
/// comes from the registry, through whatever started it. Which `PATH` an app reads tools on
/// is W17's to say; until then nobody can tell.
pub(super) fn login_path(_env: &Environment) -> LoginPath {
    LoginPath::Unknown
}

#[derive(Debug)]
struct Windows;

impl Host for Windows {
    /// Credential Manager, where Claude Code may keep its login, is offered by W23. Until
    /// then there is no store of secrets here, and each tool's chain refuses on Windows in
    /// its own provider's code, so this answer builds no chain.
    fn foreign_secrets(&self, _ctx: &Context, _account: &str) -> Option<Box<dyn RawStore>> {
        None
    }

    /// A file is read as on every system. Nothing writes one through this face: every way to
    /// write it needs a file made, which [`fs`] refuses.
    fn file(&self, path: PathBuf) -> Box<dyn RawStore> {
        Box::new(PlainFile::at(path))
    }

    /// Pitboard parks no login on Windows until W20 seals each one to the person's Windows
    /// account: every call says the vault cannot be read, which is never taken for an empty
    /// one.
    fn vault(&self, _ctx: &Context) -> Box<dyn RawStore> {
        Box::new(Unbuilt::new(
            Backend::Unknown,
            "Pitboard does not park logins on Windows yet",
        ))
    }

    /// The answer that removes nothing: a park nobody accounts for is left alone, as
    /// another home's would be, until W20 says where Windows parks are kept.
    fn vault_is_shared(&self) -> bool {
        true
    }

    /// W18 reads which processes run a tool, in every session of the person's. Until then
    /// the list cannot be read, which is not the same as nothing running.
    fn processes(&self, _program: &str) -> Option<Vec<Process>> {
        None
    }

    /// Task Scheduler is written for by W25.
    fn scheduler(&self) -> Option<&dyn Scheduler> {
        None
    }

    fn elevation(&self, ctx: &Context) -> Elevation {
        user::elevation(ctx.sudo())
    }

    fn floor(&self) -> Floor {
        user::floor()
    }

    /// Windows has no managed preferences: what an administrator sets for Codex there is in
    /// files, which W21 reads (the register's `codex_managed_preferences`).
    fn managed_preference(&self, _domain: &str, _key: &str) -> Administered {
        Administered::Unset
    }
}

pub(super) fn host() -> Arc<dyn Host> {
    Arc::new(Windows)
}

/// The path this program was started by, as Windows says it. Whether a link or a shim
/// started it is W17's to read.
pub(super) fn current_program() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}

/// Task Scheduler as a machine in memory has it, which W25 writes with the scheduler itself.
/// Until then a machine in memory on Windows has no scheduler, as this machine has none.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn pretend_scheduler(
    _refuse_start: Arc<std::sync::atomic::AtomicBool>,
) -> Option<Box<dyn Scheduler>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderId;
    use crate::store::Error;

    /// What a unit test's context reaches here: this face, with every home its own.
    fn ctx() -> Context {
        Context::for_unit_test()
    }

    fn unreadable<T: std::fmt::Debug>(read: Result<T, Error>, why: &str) {
        match read {
            Err(Error::Unreadable(said)) => assert_eq!(said, why),
            other => panic!("expected the store to be unreadable, {why}: {other:?}"),
        }
    }

    /// Claude Code's chain on Windows is a store nobody can read yet, never the file alone:
    /// Claude Code may keep its login in Credential Manager there, and a chain of the file
    /// alone would read a login that is not the one in use, and write one nothing reads.
    /// A sign-in's login is refused the same way, and so is taking one away.
    #[test]
    fn claude_codes_chain_is_unreadable_and_never_the_file() {
        let ctx = ctx();
        let why = "Pitboard does not switch Claude Code on Windows yet";
        let live = crate::provider::of(ProviderId::Claude)
            .live(&ctx)
            .expect("a chain");
        assert_eq!(
            crate::store::resolve(&live.chain, &live.service).map_err(|e| e.to_string()),
            Err(format!("the credential store could not be read: {why}"))
        );
        unreadable(crate::store::read_raw(&live.chain, &live.service), why);
        let dir = std::env::temp_dir();
        unreadable(crate::provider::claude::live::read_signin(&ctx, &dir), why);
        unreadable(
            crate::provider::claude::live::discard_signin(
                &ctx,
                crate::service::Permit::for_a_test(),
                &dir,
            ),
            why,
        );
    }

    /// Codex's live store on Windows is one nobody can tell yet, so it is refused before any
    /// chain is reached, saying Pitboard does not read Codex's configuration here, until W21
    /// reads which store Codex keeps its login in on Windows.
    #[test]
    fn codexs_store_is_refused_until_its_configuration_is_read() {
        let ctx = ctx();
        match crate::provider::of(ProviderId::Codex).live(&ctx) {
            Err(crate::provider::ProviderError::Unsupported {
                provider: ProviderId::Codex,
                reason,
            }) => assert!(
                reason.contains("Pitboard does not read Codex's configuration on this system yet"),
                "{reason}"
            ),
            Err(other) => panic!("refused otherwise: {other:?}"),
            Ok(_) => panic!("a live store Pitboard cannot tell is refused"),
        }
    }

    /// The vault answers every call as one that cannot be read, of a kind of its own, and
    /// lists nothing: an empty list would say every parked login is gone.
    #[test]
    fn the_vault_is_unreadable() {
        let ctx = ctx();
        let vault = ctx.host().vault(&ctx);
        let why = "Pitboard does not park logins on Windows yet";
        assert_eq!(vault.kind(), Backend::Unknown);
        assert_eq!(Backend::Unknown.name(), "unknown");
        unreadable(vault.contains("pitboard-park"), why);
        unreadable(vault.read("pitboard-park"), why);
        unreadable(vault.list(), why);
        let permit = crate::service::Permit::for_a_test();
        unreadable(vault.write(permit, "pitboard-park", "{}"), why);
        unreadable(vault.delete(permit, "pitboard-park"), why);
    }

    /// Nobody can tell what runs a tool here, which a switch says, and every process may
    /// still be running.
    #[test]
    fn holders_are_unknown() {
        let ctx = ctx();
        for tool in ["claude", "codex"] {
            assert_eq!(ctx.host().processes(tool), None, "{tool}");
        }
        assert!(super::proc::may_be_running(std::process::id()));
        assert!(super::proc::may_be_running(u32::MAX));
    }

    /// There is no scheduler, on this machine or on one in memory.
    #[test]
    fn there_is_no_scheduler() {
        assert!(host().scheduler().is_none());
        let memory = crate::host::memory::MemoryHost::new();
        assert!(memory.scheduler().is_none());
        assert!(matches!(
            crate::schedule::status(&ctx()),
            crate::schedule::Installed::Unsupported
        ));
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
        assert!(crate::service::gate(&ctx).is_ok());
    }

    /// No file is made, private or not, and no access is read, until W15 and W16; and no
    /// program is found until W17.
    #[test]
    fn no_file_is_made_and_no_program_found() {
        let dir = std::env::temp_dir().join(format!("pitboard-windows-{}", std::process::id()));
        let permit = crate::service::Permit::for_a_test();
        let unsupported = |made: std::io::Result<()>| {
            assert_eq!(
                made.map_err(|e| e.kind()),
                Err(std::io::ErrorKind::Unsupported)
            );
        };
        unsupported(fs::create_private_dir(permit, &dir));
        unsupported(fs::create_private(permit, &dir.join("a")).map(drop));
        unsupported(fs::open_private_append(permit, &dir.join("b")).map(drop));
        unsupported(fs::open_private_lock(permit, &dir.join("c")).map(drop));
        unsupported(fs::create_dir_all(permit, &dir));
        unsupported(fs::create_dir(permit, &dir));
        assert!(!dir.exists(), "nothing was made");
        assert_eq!(fs::access(&std::env::temp_dir()), None);
        assert!(!proc::can_run(&std::env::current_exe().expect("this test")));
        assert_eq!(login_path(&Environment::default()), LoginPath::Unknown);
        assert_eq!(user::login_name(), None);
    }
}
