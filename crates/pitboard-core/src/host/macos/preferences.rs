//! Managed preferences: what a configuration profile an administrator installed forces for a
//! program, which macOS keeps apart from the person's own defaults.
//!
//! Read the way Codex 0.160.0 reads its own, in `codex-rs/config/src/loader/macos.rs`
//! (`codex_managed_preferences` in its register): a value counts only while
//! `CFPreferencesAppValueIsForced` says a profile forces it, and must be a string. Claude
//! Code reads the profile's plist whole instead (`macos_managed_preferences`).

use crate::host::Input;
use crate::host::administered::{self, Administered};
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::CFString;
use core_foundation_sys::preferences::{CFPreferencesAppValueIsForced, CFPreferencesCopyAppValue};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// The text a configuration profile forces `key` of `domain` to: unset where nothing forces
/// it, whatever the person's own defaults hold, and unreadable where what is forced is not a
/// string.
///
/// Whether it is forced is asked before the value is copied and again after, as Codex asks:
/// `CFPreferencesCopyAppValue` also searches the person's own defaults, and the calls are not
/// one snapshot, so a profile removed in between would hand back the person's value.
pub(super) fn forced(domain: &str, key: &str) -> Administered {
    let (key, application) = (CFString::new(key), CFString::new(domain));
    let is_forced = || {
        // SAFETY: both are CFStrings that live past the call, which only reads them.
        unsafe {
            CFPreferencesAppValueIsForced(
                key.as_concrete_TypeRef(),
                application.as_concrete_TypeRef(),
            ) != 0
        }
    };
    if !is_forced() {
        return Administered::Unset;
    }
    // SAFETY: both are CFStrings that live past the call, which only reads them.
    let copied = unsafe {
        CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), application.as_concrete_TypeRef())
    };
    if copied.is_null() {
        return Administered::Unset;
    }
    // SAFETY: CFPreferencesCopyAppValue follows the create rule, so the value it returned is
    // this caller's to release, which a CFType made under the create rule does when dropped.
    let value = unsafe { CFType::wrap_under_create_rule(copied) };
    if !is_forced() {
        return Administered::Unset;
    }
    match value.downcast::<CFString>() {
        Some(text) => Administered::Set(text.to_string()),
        None => Administered::Unreadable("is forced, and is not a string".into()),
    }
}

// As Claude Code 2.1.309 converts it with `plutil` (`macos_managed_preferences`).
pub(super) fn as_json(path: &Path) -> Administered {
    if let Err(e) = std::fs::File::open(path) {
        return administered::unread(&e);
    }
    let plutil = |args: &[&str]| {
        let mut plutil = Command::new("/usr/bin/plutil");
        plutil.args(args).arg("--").arg(path);
        super::proc::run_within(plutil, Input::Nothing, Duration::from_secs(5))
    };
    let unfinished =
        |e: std::io::Error| Administered::Unreadable(format!("cannot be read by `plutil`: {e}"));
    match plutil(&["-convert", "json", "-o", "-"]) {
        Ok(out) if out.status.success() => administered::as_text(out.stdout),
        Ok(_) => match plutil(&["-lint", "-s"]) {
            Ok(lint) if lint.status.success() => Administered::Unreadable(
                "holds a value `plutil` cannot convert to JSON, a date or data".into(),
            ),
            Ok(_) => Administered::Set(String::new()),
            Err(e) => unfinished(e),
        },
        Err(e) => unfinished(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A domain no profile forces anything in reads as unset, from the real preferences: the
    /// person's own defaults never count as an administrator's.
    #[test]
    fn nothing_forced_is_unset() {
        assert_eq!(
            forced(
                "com.usepitboard.Pitboard.never-managed",
                "config_toml_base64"
            ),
            Administered::Unset
        );
    }

    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test sets up its scratch files as a person or a tool would"
    )]
    fn a_plist_is_read_as_plutil_converts_it_for_claude_code() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let plist = |name: &str, body: &str| {
            let path = scratch.path().join(name);
            std::fs::write(
                &path,
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\
                     <dict>{body}</dict></plist>\n"
                ),
            )
            .expect("written");
            path
        };
        let pinned = plist(
            "pinned.plist",
            "<key>forceLoginOrgUUID</key><array><string>org-a</string></array>",
        );
        let Administered::Set(json) = as_json(&pinned) else {
            panic!("{:?}", as_json(&pinned));
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).expect("JSON"),
            serde_json::json!({"forceLoginOrgUUID": ["org-a"]})
        );

        let dated = plist(
            "dated.plist",
            "<key>since</key><date>2026-10-23T00:00:00Z</date>",
        );
        assert!(matches!(as_json(&dated), Administered::Unreadable(_)));

        let not_a_plist = scratch.path().join("not.plist");
        std::fs::write(&not_a_plist, "not a plist").expect("written");
        assert_eq!(as_json(&not_a_plist), Administered::Set(String::new()));

        assert_eq!(
            as_json(&scratch.path().join("absent.plist")),
            Administered::Unset
        );
    }
}
