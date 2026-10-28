//! What Meson installs for the desktop, read as the desktop reads it, through `glib::KeyFile`,
//! and the GResource `build.rs` compiles for the program.
#![cfg(target_os = "linux")]

use std::path::Path;

use pitboard_linux::config::Build;

const RELEASE: Build = Build::Release;

fn read(name: &str) -> glib::KeyFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join(name);
    let file = glib::KeyFile::new();
    file.load_from_file(&path, glib::KeyFileFlags::NONE)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    file
}

fn string(file: &glib::KeyFile, group: &str, key: &str) -> String {
    file.string(group, key)
        .unwrap_or_else(|e| panic!("[{group}] {key}: {e}"))
        .into()
}

// D-Bus activation finds the launcher by the bus name, so the file is named after the id.
#[test]
fn the_launcher_is_dbus_activatable_and_claims_the_release_scheme() {
    let launcher = read(&format!("{}.desktop.in", RELEASE.app_id()));
    let entry = "Desktop Entry";
    assert_eq!(string(&launcher, entry, "Type"), "Application");
    assert!(launcher.boolean(entry, "DBusActivatable").unwrap());
    assert_eq!(string(&launcher, entry, "Icon"), RELEASE.app_id());
    assert_eq!(string(&launcher, entry, "Exec"), "@BINDIR@/pitboard-app %u");
    assert_eq!(string(&launcher, entry, "TryExec"), "@BINDIR@/pitboard-app");
    let schemes = launcher.string_list(entry, "MimeType").unwrap();
    let release_scheme = format!("x-scheme-handler/{}", RELEASE.link_scheme());
    assert_eq!(
        schemes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        [release_scheme]
    );
    let actions = launcher.string_list(entry, "Actions").unwrap();
    assert_eq!(
        actions.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        ["add-account", "refresh"]
    );
    for action in ["add-account", "refresh"] {
        let exec = string(&launcher, &format!("Desktop Action {action}"), "Exec");
        assert_eq!(exec, format!("@BINDIR@/pitboard-app --{action}"));
    }
}

#[test]
fn the_service_starts_the_program_as_a_service() {
    let service = read(&format!("{}.service.in", RELEASE.app_id()));
    let group = "D-BUS Service";
    assert_eq!(string(&service, group, "Name"), RELEASE.app_id());
    assert_eq!(
        string(&service, group, "Exec"),
        "@BINDIR@/pitboard-app --gapplication-service"
    );
}

// Each would otherwise start Pitboard for a keystroke, and Pitboard asks Anthropic and OpenAI
// as it starts (docs/facts/linux-app.md, Search and KRunner).
#[test]
fn neither_search_starts_pitboard() {
    let (id, path) = (RELEASE.app_id(), RELEASE.object_path());
    let search = read(&format!("{id}.search-provider.ini"));
    let group = "Shell Search Provider";
    assert_eq!(string(&search, group, "DesktopId"), format!("{id}.desktop"));
    assert_eq!(string(&search, group, "BusName"), id);
    assert_eq!(
        string(&search, group, "ObjectPath"),
        format!("{path}/SearchProvider")
    );
    assert_eq!(string(&search, group, "Version"), "2");
    assert!(!search.boolean(group, "AutoStart").unwrap());

    let runner = read(&format!("krunner/{id}.desktop"));
    let entry = "Desktop Entry";
    assert_eq!(string(&runner, entry, "X-Plasma-API"), "DBus");
    assert_eq!(
        string(&runner, entry, "X-Plasma-DBusRunner-Service"),
        format!("{id}*")
    );
    assert_eq!(
        string(&runner, entry, "X-Plasma-DBusRunner-Path"),
        format!("{path}/Runner")
    );
}

// GtkApplication looks for its icons under the resource path its id names, in the hicolor
// theme's folders (docs/facts/linux-app.md, The icons).
#[test]
fn the_resource_holds_the_icons_and_the_metainfo_with_its_releases() {
    gio::resources_register_include!("pitboard.gresource").unwrap();
    let base = RELEASE.object_path();
    for icon in [
        "scalable/apps/com.usepitboard.Pitboard.svg",
        "scalable/apps/com.usepitboard.Pitboard.Devel.svg",
        "symbolic/apps/com.usepitboard.Pitboard-symbolic.svg",
    ] {
        let path = format!("{base}/icons/{icon}");
        gio::resources_get_info(&path, gio::ResourceLookupFlags::NONE)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
    }
    let path = format!("{base}/{}.metainfo.xml", RELEASE.app_id());
    let metainfo = gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE)
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    let metainfo = std::str::from_utf8(&metainfo).unwrap();
    assert!(metainfo.contains(&format!("<id>{}</id>", RELEASE.app_id())));
    assert!(metainfo.contains("<release version="));
    assert!(!metainfo.contains("@RELEASES@"));
}
