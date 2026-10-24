use crate::context::Context;
use crate::host::{self, Os};
use crate::provider::folder::Unfit;
use crate::provider::{ProviderError, ProviderId};
use std::path::{Path, PathBuf};

// A constant: nothing a build for tests is told lets it reach the account's own data folder.
pub(super) const REAL: bool = !cfg!(any(test, feature = "test-support"));

const NOT_NAMED: &str = "this build is for tests and was given no Claude data folder";
pub(crate) const MACOS_ONLY: &str = "Pitboard switches Claude Desktop accounts on macOS only";

pub(crate) fn root(ctx: &Context) -> Result<PathBuf, ProviderError> {
    // cfg! is a value, so this branch compiles in a release too, where scratch_claude is None.
    if !REAL {
        return ctx
            .scratch_claude()
            .map(|scratch| scratch.data.clone())
            .ok_or_else(|| unsupported(NOT_NAMED));
    }
    match host::OS {
        Os::MacOs => real(ctx),
        Os::Linux => Err(unsupported(MACOS_ONLY)),
        Os::Windows => Err(unsupported(MACOS_ONLY)),
    }
}

fn real(ctx: &Context) -> Result<PathBuf, ProviderError> {
    real_for(host::user::home(), ctx.home())
}

// The passwd home comes in as an argument, so a unit test never asks for the real one.
fn real_for(own: Option<PathBuf>, home: &Path) -> Result<PathBuf, ProviderError> {
    let own =
        own.ok_or_else(|| unsupported("this system names no home folder for this account"))?;
    if !same_folder(&own, home) {
        return Err(unsupported(
            "`HOME` names another folder than this account's own, where Claude keeps its data, \
             so Pitboard leaves Claude Desktop alone here",
        ));
    }
    Ok(own.join("Library/Application Support/Claude"))
}

fn same_folder(a: &Path, b: &Path) -> bool {
    let resolved = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    resolved(a) == resolved(b)
}

// This account's own home where `HOME` names another folder, for `doctor`'s advice.
pub(crate) fn home_elsewhere(ctx: &Context) -> Option<PathBuf> {
    if !REAL {
        return None;
    }
    match host::OS {
        Os::MacOs => elsewhere_for(host::user::home(), ctx.home()),
        Os::Linux | Os::Windows => None,
    }
}

fn elsewhere_for(own: Option<PathBuf>, home: &Path) -> Option<PathBuf> {
    own.filter(|own| !same_folder(own, home))
}

fn unsupported(reason: &str) -> ProviderError {
    ProviderError::Unsupported {
        provider: ProviderId::Desktop,
        reason: reason.into(),
    }
}

pub(super) fn refuses(root: &Path) -> Option<Unfit> {
    if std::fs::symlink_metadata(root).is_ok_and(|found| found.file_type().is_symlink()) {
        return Some(Unfit::Link);
    }
    if reset_waits(root) {
        return Some(Unfit::ResetWaits);
    }
    let mut beside = root.as_os_str().to_owned();
    beside.push("-3p");
    third_party(Path::new(&beside)).then_some(Unfit::ThirdParty("Claude-3p"))
}

// The register's `desktop_third_party_folder`: Claude has kept its settings there, and the
// person has not picked claude.ai in its chooser since.
fn third_party(beside: &Path) -> bool {
    if std::fs::symlink_metadata(beside.join("config.json")).is_err() {
        return false;
    }
    let picked = std::fs::read(beside.join("claude_desktop_config.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
        .and_then(|config| config.get("deploymentMode")?.as_str().map(str::to_owned));
    picked.as_deref() != Some("1p")
}

// The register's `desktop_reset_app_data`.
fn reset_waits(root: &Path) -> bool {
    let asked = std::fs::symlink_metadata(root.join("reset-app-data"));
    asked.is_ok_and(|found| found.is_file())
        && std::fs::symlink_metadata(root.join("reset-app-data.given-up")).is_err()
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    const _: () = assert!(!REAL, "a unit test is a build for tests");

    fn reason(error: ProviderError) -> String {
        match error {
            ProviderError::Unsupported { provider, reason } => {
                assert_eq!(provider, ProviderId::Desktop);
                reason
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_build_for_tests_cannot_name_the_accounts_own_data_folder() {
        let refused = root(&Context::for_unit_test()).expect_err("no scratch was named");
        assert_eq!(reason(refused), NOT_NAMED);
        let folder = tempfile::tempdir().expect("a scratch folder");
        let scratch = folder.path().join("claude");
        let named = Context::for_unit_test().with_scratch_claude(scratch.clone());
        assert_eq!(root(&named).unwrap(), scratch.join("data"));
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes no link"
    )]
    fn home_naming_another_folder_makes_the_tool_absent() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let base = scratch.path();
        let (own, other) = (base.join("own"), base.join("other"));
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        assert_eq!(
            real_for(Some(own.clone()), &own).unwrap(),
            own.join("Library/Application Support/Claude")
        );
        let link = base.join("link");
        host::fs::testing::link_dir(&own, &link).unwrap();
        assert!(
            real_for(Some(own.clone()), &link).is_ok(),
            "a link to it is it"
        );
        assert!(reason(real_for(Some(own.clone()), &other).unwrap_err()).starts_with("`HOME`"));
        assert_eq!(elsewhere_for(Some(own.clone()), &other), Some(own.clone()));
        assert_eq!(elsewhere_for(Some(own.clone()), &link), None);
        assert_eq!(elsewhere_for(None, &other), None);
        assert_eq!(home_elsewhere(&Context::new(other.clone())), None);
        assert_eq!(
            reason(real_for(None, &own).unwrap_err()),
            "this system names no home folder for this account"
        );
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes no link"
    )]
    fn a_link_or_a_third_party_deployment_beside_it_is_refused() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let base = scratch.path();
        let root = base.join("Claude");
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(refuses(&root), None);
        let third = base.join("Claude-3p");
        std::fs::create_dir_all(&third).unwrap();
        let chose = |config: &[u8]| {
            std::fs::write(third.join("claude_desktop_config.json"), config).unwrap();
            refuses(&root)
        };
        // Issue 271's: an earlier build's, on a Mac with no deployment.
        let left = br#"{"enterpriseConfig": {}, "_cfprefsMigrated": true}"#;
        assert_eq!(chose(left), None, "Claude never ran from it");
        assert_eq!(
            chose(br#"{"deploymentMode": "3p"}"#),
            None,
            "nor yet from this one"
        );
        std::fs::write(third.join("config.json"), b"{}").unwrap();
        assert_eq!(chose(left), Some(Unfit::ThirdParty("Claude-3p")));
        assert_eq!(
            chose(br#"{"deploymentMode": "3p"}"#),
            Some(Unfit::ThirdParty("Claude-3p"))
        );
        assert_eq!(chose(b"{"), Some(Unfit::ThirdParty("Claude-3p")));
        assert_eq!(
            chose(br#"{"deploymentMode": "1p"}"#),
            None,
            "claude.ai picked"
        );
        std::fs::remove_file(third.join("claude_desktop_config.json")).unwrap();
        let beside = refuses(&root).expect("refused");
        assert_eq!(beside, Unfit::ThirdParty("Claude-3p"));
        assert_eq!(
            beside.to_string(),
            "is not the one Claude runs from: it runs as a third-party deployment, from \
             `Claude-3p` beside it"
        );
        let link = base.join("Linked");
        host::fs::testing::link_dir(&root, &link).unwrap();
        assert_eq!(refuses(&link), Some(Unfit::Link));
        assert_eq!(Unfit::Link.to_string(), "is a link");
    }

    #[test]
    fn a_reset_of_claudes_data_waiting_is_refused_until_claude_ends_it() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        assert_eq!(refuses(root), None);
        std::fs::create_dir(root.join("reset-app-data")).unwrap();
        assert_eq!(refuses(root), None, "Claude asks for a file");
        std::fs::remove_dir(root.join("reset-app-data")).unwrap();
        std::fs::write(
            root.join("reset-app-data"),
            br#"{"choice":"restart","tries":0}"#,
        )
        .unwrap();
        assert_eq!(refuses(root), Some(Unfit::ResetWaits));
        assert_eq!(
            Unfit::ResetWaits.to_string(),
            "is waiting for Reset App Data, which deletes what it holds the next time Claude opens"
        );
        std::fs::write(root.join("reset-app-data.given-up"), b"").unwrap();
        assert_eq!(refuses(root), None, "given up, it deletes nothing");
    }
}
