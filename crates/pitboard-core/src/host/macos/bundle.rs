use core_foundation::bundle::CFBundle;
use core_foundation::string::CFString;
use core_foundation::url::CFURL;
use std::path::Path;

pub(super) fn identifier(bundle: &Path) -> Option<String> {
    said(bundle, "CFBundleIdentifier")
}

pub(super) fn version(bundle: &Path) -> Option<String> {
    said(bundle, "CFBundleShortVersionString")
}

fn said(bundle: &Path, key: &'static str) -> Option<String> {
    let url = CFURL::from_path(bundle, true)?;
    let info = CFBundle::new(url)?.info_dictionary();
    info.find(CFString::from_static_string(key))?
        .downcast::<CFString>()
        .map(|value| value.to_string())
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    fn scratch_app() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().expect("a scratch folder");
        let app = root.path().join("Scratch.app");
        std::fs::create_dir_all(app.join("Contents")).expect("a scratch bundle");
        std::fs::write(
            app.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.example.scratch</string>
<key>CFBundleShortVersionString</key><string>2.31226.1</string>
</dict></plist>"#,
        )
        .expect("a plist");
        (root, app)
    }

    #[test]
    fn a_bundle_says_its_identifier_and_version_and_a_folder_that_is_none_says_nothing() {
        let (root, app) = scratch_app();
        assert_eq!(identifier(&app).as_deref(), Some("com.example.scratch"));
        assert_eq!(version(&app).as_deref(), Some("2.31226.1"));
        assert_eq!(identifier(&root.path().join("Missing.app")), None);
        assert_eq!(version(&root.path().join("Missing.app")), None);
    }

    // So no test reads the Claude installed on the machine running it.
    #[test]
    fn a_build_for_tests_reads_no_app_the_machine_holds() {
        let (_root, app) = scratch_app();
        let host = crate::host::current();
        let (id, built) = (host.app_id(&app), host.app_version(&app));
        let readable = identifier(&app);
        assert_eq!(readable.as_deref(), Some("com.example.scratch"));
        assert_eq!(id, None);
        assert_eq!(built, None);
    }
}
