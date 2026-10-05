//! Claude Desktop, the app, as one provider among several.
//!
//! Its login is not a credential. Claude Desktop is an Electron app, and the account it is
//! signed in to lives in Chromium's cookie jar, its storage folders and three keys of its
//! `config.json`, all inside one data folder beside caches and settings that belong to the
//! machine. So none of the credential operations of [`Provider`] apply: each one is an
//! error here, and the login is moved through [`TreeLogin`] instead, one folder item at a
//! time, by rename and never by copy.
//!
//! Read from Claude Desktop 2.19675.0, and dated in [`assumptions`]. Whose login a folder
//! holds is read from its cookie jar and `config.json` here; moving it is the tree
//! engine's.

pub mod assumptions;
pub(crate) mod config;
pub(crate) mod cookies;
pub(crate) mod crypto;
pub(crate) mod history;
pub(crate) mod holders;
pub(crate) mod identity;
pub(crate) mod live_usage;
pub(crate) mod paths;
pub(crate) mod safe_storage;
pub(crate) mod types;
pub(crate) mod web;

pub(crate) use holders::HOLDERS;
pub(crate) use paths::{
    APP_NAME, BUNDLE_NAME, CONFIG_KEYS, EXCLUDED, ITEMS, SINGLETON_LOCK, support_dir,
};
pub(crate) use types::TreeIdentity;

use crate::context::Context;
use crate::holder::Holder;
use crate::host::Bundle;
use crate::provider::{
    Adoption, Credential, Expiry, Identity, Isolation, LiveStore, ParkSemantics, Provider,
    ProviderError, ProviderId, SignInView, TreeItem, TreeLogin,
};
use crate::usage::Snapshot;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// What a sign-in runs instead of the app: a program that starts nothing and fails, so a
/// caller that reaches [`Provider::sign_in`] past the service's refusal opens nothing of
/// Claude's and waits on nothing.
pub(crate) const NO_SIGN_IN: &str = "/usr/bin/false";

#[derive(Debug, Clone, Copy)]
pub(crate) struct Desktop;

pub(crate) static DESKTOP: Desktop = Desktop;

/// The answer to every credential operation: there is no credential.
fn not_a_credential<T>() -> Result<T, ProviderError> {
    Err(ProviderError::NotACredential)
}

impl Provider for Desktop {
    fn id(&self) -> ProviderId {
        ProviderId::Desktop
    }

    fn live(&self, _ctx: &Context) -> Result<LiveStore, ProviderError> {
        not_a_credential()
    }

    fn read_live(&self, _ctx: &Context) -> Result<Option<Credential>, ProviderError> {
        not_a_credential()
    }

    fn identify(
        &self,
        _ctx: &Context,
        _credential: &Credential,
    ) -> Result<Identity, ProviderError> {
        not_a_credential()
    }

    fn verify(&self, _ctx: &Context, _credential: &Credential) -> Result<Identity, ProviderError> {
        not_a_credential()
    }

    fn usage(&self, _ctx: &Context, _credential: &Credential) -> Result<Snapshot, ProviderError> {
        not_a_credential()
    }

    fn renew(&self, _ctx: &Context, _credential: &Credential) -> Result<Credential, ProviderError> {
        not_a_credential()
    }

    /// The data folder, which is the one place the live login can be. A recovery record
    /// names it, so a switch interrupted with one folder is not finished against another.
    fn slot(&self, ctx: &Context) -> String {
        support_dir(ctx)
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The app takes no lock Pitboard could share. It is quit instead, which is stronger.
    fn write_lock(&self, _ctx: &Context) -> Option<PathBuf> {
        None
    }

    /// The account `config.json` says the app last signed in to. Only its uuid: the app
    /// writes no email there. `None` where it names none or cannot be read.
    fn recorded_identity(&self, ctx: &Context) -> Option<Identity> {
        let bytes = std::fs::read(paths::config_file(&support_dir(ctx)?)).ok()?;
        let config: Value = serde_json::from_slice(&bytes).ok()?;
        let uuid = config.get("lastKnownAccountUuid")?.as_str()?;
        (!uuid.is_empty()).then(|| Identity {
            account_id: uuid.to_string(),
            email: String::new(),
            group: None,
        })
    }

    /// The account's own `config.json` keys travel with its items, so there is nothing
    /// left to correct afterwards.
    fn after_switch(
        &self,
        _ctx: &Context,
        _incoming: &crate::state::Account,
        _outgoing: &Identity,
    ) -> Result<(), crate::error::Error> {
        Ok(())
    }

    /// The program inside the app bundle the context names.
    fn program(&self, ctx: &Context) -> Option<PathBuf> {
        ctx.desktop_program()?;
        crate::provider::program_of(ctx, ProviderId::Desktop)
    }

    /// Never run: Claude Desktop signs in from its own window, with the data folder it
    /// always uses, so there is no private sign-in, and the service refuses one with
    /// `sign_in_unsupported` before it gets here. Should anything ever get here anyway, the
    /// command is [`NO_SIGN_IN`] and never the app: opening Claude from a sign-in would
    /// have it read and write the live data folder.
    fn sign_in(&self, _ctx: &Context, _dir: &Path) -> std::process::Command {
        std::process::Command::new(NO_SIGN_IN)
    }

    fn read_signin(
        &self,
        _ctx: &Context,
        _dir: &Path,
    ) -> Result<Option<String>, crate::store::Error> {
        Ok(None)
    }

    fn discard_signin(&self, _ctx: &Context, _dir: &Path) {}

    /// Claude Desktop is signed in to inside the app, and Pitboard runs no sign-in of its
    /// own that could print an address or ask for a code.
    fn read_sign_in(&self, _said: &str) -> SignInView {
        SignInView::default()
    }

    fn overridden_by(&self, _ctx: &Context) -> Vec<String> {
        Vec::new()
    }

    /// The app reads its login when it opens, and is never switched while open.
    fn adoption(&self) -> Adoption {
        Adoption::NextLaunch { program: APP_NAME }
    }

    /// A park is the account's items themselves, moved, so there is never a second copy.
    fn park_semantics(&self) -> ParkSemantics {
        ParkSemantics::MoveOnly
    }

    fn private_signin_isolation(&self, _ctx: &Context) -> Isolation {
        Isolation::NotIsolated {
            reason: "Claude Desktop keeps its login in its own data folder, which Pitboard \
                     cannot point elsewhere."
                .into(),
        }
    }

    fn slice(&self, _live: &Value) -> Result<Value, ProviderError> {
        not_a_credential()
    }

    fn splice(&self, _live: &Value, _incoming: &Value) -> Result<Value, ProviderError> {
        not_a_credential()
    }

    /// No credential has a refresh token to name. Nothing calls this for a tree login.
    fn fingerprint(&self, _slice: &Value) -> String {
        String::new()
    }

    /// No credential says when it stops working. Nothing calls this for a tree login.
    fn expiry(&self, _slice: &Value) -> Expiry {
        Expiry::default()
    }

    fn tree(&self) -> Option<&'static dyn TreeLogin> {
        Some(&DESKTOP)
    }
}

impl TreeLogin for Desktop {
    fn root(&self, ctx: &Context) -> Option<PathBuf> {
        support_dir(ctx)
    }

    fn items(&self) -> &'static [TreeItem] {
        ITEMS
    }

    fn config_keys(&self) -> &'static [&'static str] {
        &CONFIG_KEYS
    }

    /// Every bundle called `Claude.app`, unless a test or an app moved both the bundle and
    /// the data folder: then only that bundle, by its path, whatever it is called. The
    /// person's own Claude runs from the real bundle and holds nothing of a folder
    /// elsewhere, so a test beside it is not stopped by it; while the folder is the real one,
    /// any Claude may hold it, wherever the bundle is said to be.
    fn bundle<'a>(&self, ctx: &'a Context) -> Bundle<'a> {
        match &ctx.desktop_app {
            Some(app) if ctx.desktop_dir.is_some() && ctx.desktop_app_moved() => Bundle::At(app),
            _ => Bundle::Named(BUNDLE_NAME),
        }
    }

    fn excluded(&self) -> &'static [&'static str] {
        EXCLUDED
    }

    fn singleton_lock(&self) -> Option<&'static str> {
        Some(SINGLETON_LOCK)
    }

    fn holders(&self) -> &'static [Holder] {
        HOLDERS
    }

    /// Whose login a folder holds: the session in its cookie jar and the uuid its
    /// `config.json` names. A jar that cannot be read, or is in a form this Pitboard was
    /// not written for, is an error, so nothing is ever moved under an account it was not
    /// confirmed to be.
    fn identify(
        &self,
        ctx: &Context,
        root: &Path,
    ) -> Result<Option<TreeIdentity>, crate::error::Error> {
        identity::identify_tree(ctx, root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every credential operation is an error, and none of them panics: a caller that
    /// forgets to ask for [`Provider::tree`] gets an answer it can report.
    #[test]
    fn every_credential_operation_says_there_is_no_credential() {
        let ctx = Context::new(PathBuf::from("/nowhere"));
        let credential = Credential::new(ProviderId::Desktop, Value::Null);
        let refused = |result: Result<(), ProviderError>| {
            assert!(
                matches!(result, Err(ProviderError::NotACredential)),
                "{result:?}"
            );
        };
        refused(DESKTOP.live(&ctx).map(drop));
        refused(DESKTOP.read_live(&ctx).map(drop));
        refused(Provider::identify(&DESKTOP, &ctx, &credential).map(drop));
        refused(DESKTOP.verify(&ctx, &credential).map(drop));
        refused(DESKTOP.usage(&ctx, &credential).map(drop));
        refused(DESKTOP.renew(&ctx, &credential).map(drop));
        refused(DESKTOP.slice(&Value::Null).map(drop));
        refused(DESKTOP.splice(&Value::Null, &Value::Null).map(drop));
        assert!(DESKTOP.tree().is_some());
        assert_eq!(
            DESKTOP.adoption(),
            Adoption::NextLaunch { program: "Claude" }
        );
        assert_eq!(DESKTOP.park_semantics(), ParkSemantics::MoveOnly);
        assert!(matches!(
            DESKTOP.private_signin_isolation(&ctx),
            Isolation::NotIsolated { .. }
        ));
    }

    /// A sign-in never opens the app, whatever reaches here: the command a caller would run
    /// starts nothing of Claude's, and fails, so nothing waits on a sign-in that is not one.
    #[test]
    fn a_sign_in_never_starts_the_app() {
        let ctx = Context::new(PathBuf::from("/nowhere"))
            .with_desktop_app("/scratch/Claude.app".into())
            .with_search_path("/nowhere/at/all".into());
        let command = DESKTOP.sign_in(&ctx, Path::new("/tmp/pitboard-signin-scratch"));
        let program = Path::new(command.get_program());
        assert!(
            !program.to_string_lossy().contains("Claude"),
            "{}",
            program.display()
        );
        assert_eq!(program, Path::new(NO_SIGN_IN));
    }

    /// The data folder is where the context says, so a test never reaches the real one.
    #[test]
    fn the_data_folder_is_where_the_context_says() {
        let ctx = Context::new(PathBuf::from("/home/x"))
            .with_desktop_dir("/scratch/claude-desktop".into());
        assert_eq!(
            DESKTOP.root(&ctx),
            Some(PathBuf::from("/scratch/claude-desktop"))
        );
        assert_eq!(DESKTOP.slot(&ctx), "/scratch/claude-desktop");
        let default = Context::new(PathBuf::from("/home/x"));
        let expected = match crate::host::OS {
            crate::host::Os::MacOs => {
                Some(PathBuf::from("/home/x/Library/Application Support/Claude"))
            }
            crate::host::Os::Linux => None,
        };
        assert_eq!(support_dir(&default), expected);
    }

    /// A scratch data folder, removed when the test ends.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "pitboard-desktop-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch dir");
        Scratch(root)
    }

    /// Whose login a folder holds is read from its jar, and a jar that cannot be read is an
    /// error rather than "nobody".
    #[test]
    fn whose_login_a_folder_holds_is_read_and_never_guessed() {
        let s = scratch("whose");
        let mem = crate::host::memory::MemoryHost::new();
        let ctx = Context::new(s.0.clone()).with_memory_stores(mem.clone());
        // No jar: signed out.
        let found = TreeLogin::identify(&DESKTOP, &ctx, &s.0);
        assert!(matches!(found, Ok(None)), "{found:?}");
        // A jar nothing can read: not signed out.
        std::fs::write(s.0.join("Cookies"), b"").unwrap();
        let found = TreeLogin::identify(&DESKTOP, &ctx, &s.0);
        assert!(
            matches!(
                found,
                Err(crate::error::Error::DesktopDataInaccessible { .. })
            ),
            "{found:?}"
        );
    }

    /// The account the app last signed in to is the one its `config.json` names, and a
    /// config that names none, or cannot be read, names nobody.
    #[test]
    fn the_recorded_account_is_the_one_config_names() {
        let s = scratch("recorded");
        let ctx = Context::new(s.0.clone()).with_desktop_dir(s.0.to_string_lossy().into());
        let config = s.0.join("config.json");
        assert_eq!(DESKTOP.recorded_identity(&ctx), None, "no config");

        std::fs::write(
            &config,
            r#"{"locale":"en-US","lastKnownAccountUuid":"11111111-1111-1111-1111-111111111111"}"#,
        )
        .unwrap();
        let found = DESKTOP.recorded_identity(&ctx).expect("named");
        assert_eq!(found.account_id, "11111111-1111-1111-1111-111111111111");
        assert_eq!(found.email, "");
        assert_eq!(found.group, None);

        for other in [
            &br#"{"locale":"en-US"}"#[..],
            br#"{"lastKnownAccountUuid":""}"#,
            br#"{"lastKnownAccountUuid":42}"#,
            b"{\"lastKnownAccountUuid\": ",
        ] {
            std::fs::write(&config, other).unwrap();
            assert_eq!(
                DESKTOP.recorded_identity(&ctx),
                None,
                "{}",
                String::from_utf8_lossy(other)
            );
        }
        let nowhere =
            Context::new(s.0.clone()).with_desktop_dir(s.0.join("gone").to_string_lossy().into());
        assert_eq!(DESKTOP.recorded_identity(&nowhere), None);
    }

    /// Only items inside the data folder, never the folder itself or anything above it.
    #[test]
    fn every_item_stays_inside_the_data_folder() {
        for item in ITEMS {
            let path = Path::new(item.path);
            assert!(path.is_relative(), "{}", item.path);
            assert!(
                path.components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))),
                "{}",
                item.path
            );
        }
        assert!(!CONFIG_KEYS.is_empty());
    }
}
