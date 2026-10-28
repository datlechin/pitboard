//! Where a home keeps its parked logins of Claude Code and Codex, as `pitboard vault` says it.

use crate::context::Context;
use crate::error::Result;
use crate::host::Vault;
use crate::state::{self, VaultForm};
use crate::store;
use std::path::PathBuf;

/// Where a home keeps its parked logins of Claude Code and Codex, from its files and from what
/// answers on this machine. It may say more in a later release, so it cannot be built outside
/// this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Keeping {
    /// `None` where the vault this home opens is none a person looks in.
    pub place: Option<Vault>,
    /// Whether every Pitboard directory of the Mac, or of the session on Linux, parks there.
    pub shared: bool,
    /// The folder of files this home keeps them in, or may move them to.
    pub folder: Option<PathBuf>,
    /// Whether a Secret Service answers on this session's bus, where this system parks in one.
    pub secret_service: Option<bool>,
}

pub(crate) fn keeping(ctx: &Context) -> Result<Keeping> {
    state::load(ctx)?;
    let host = ctx.host();
    let form = store::home_form(ctx)?;
    let place = store::vault_place(ctx);
    let in_files = matches!(place, Some(Vault::Files | Vault::SealedFiles))
        || host.vault_forms().contains(&VaultForm::Files);
    Ok(Keeping {
        place,
        shared: host.vault_is_shared(form),
        folder: in_files.then(|| store::vault_dir(ctx)),
        secret_service: host
            .vault_forms()
            .contains(&VaultForm::SecretService)
            .then(|| host.answers(ctx, VaultForm::SecretService)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ScratchMachine;
    use crate::host::memory::MemoryHost;
    use std::sync::Arc;

    fn kept_on(host: Arc<MemoryHost>) -> (Keeping, PathBuf) {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let machine = ScratchMachine::on(host, &root, |ctx| {
            ctx.with_pitboard_home(root.join(".pitboard"))
        });
        let kept = keeping(&machine.ctx).expect("read");
        (kept, root.join(".pitboard/vault"))
    }

    // As macOS keeps them, as Windows does, and as Linux does each way, a Secret Service there
    // or not.
    #[test]
    fn the_vault_says_where_a_home_keeps_its_parks_and_who_else_parks_there() {
        let (kept, _) = kept_on(MemoryHost::new());
        assert_eq!(
            kept,
            Keeping {
                place: Some(Vault::Keychain),
                shared: true,
                folder: None,
                secret_service: None,
            }
        );

        let (kept, folder) = kept_on(MemoryHost::sealed());
        assert_eq!(
            kept,
            Keeping {
                place: Some(Vault::SealedFiles),
                shared: false,
                folder: Some(folder),
                secret_service: None,
            }
        );

        let (kept, folder) = kept_on(MemoryHost::by_form());
        assert_eq!(
            kept,
            Keeping {
                place: Some(Vault::SecretService),
                shared: true,
                folder: Some(folder),
                secret_service: Some(true),
            }
        );

        let host = MemoryHost::by_form();
        host.secret_service_answers(false);
        let (kept, folder) = kept_on(host);
        assert_eq!(
            kept,
            Keeping {
                place: Some(Vault::Files),
                shared: false,
                folder: Some(folder),
                secret_service: Some(false),
            }
        );
    }
}
