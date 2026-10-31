//! What Meson installs for the desktop, read as the desktop reads it, through `glib::KeyFile`,
//! and the GResource `build.rs` compiles for the program.
#![cfg(target_os = "linux")]

use std::collections::BTreeSet;
use std::path::Path;

use pitboard_linux::config::{self, Build};
use pitboard_linux::platform::{krunner, search};
use pitboard_linux::resources;

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
    let provider = read(&format!("{id}.search-provider.ini"));
    let group = "Shell Search Provider";
    assert_eq!(
        string(&provider, group, "DesktopId"),
        format!("{id}.desktop")
    );
    assert_eq!(string(&provider, group, "BusName"), id);
    assert_eq!(string(&provider, group, "ObjectPath"), search::path(path));
    assert_eq!(string(&provider, group, "Version"), "2");
    assert!(!provider.boolean(group, "AutoStart").unwrap());

    let runner = read(&format!("krunner/{id}.desktop"));
    let entry = "Desktop Entry";
    assert_eq!(string(&runner, entry, "X-Plasma-API"), "DBus");
    assert_eq!(
        string(&runner, entry, "X-Plasma-DBusRunner-Service"),
        format!("{id}*")
    );
    assert_eq!(
        string(&runner, entry, "X-Plasma-DBusRunner-Path"),
        krunner::path(path)
    );
}

// GtkApplication looks for its icons under the resource path its id names, in the hicolor
// theme's folders (docs/facts/linux-app.md, The icons).
#[test]
fn the_resource_holds_the_icons_and_the_metainfo_with_its_releases() {
    resources::register();
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

// A theme without a name leaves hicolor, which GTK's, libadwaita's and the app's resources join
// (docs/facts/linux-app.md, The icons).
#[gtk::test]
fn every_icon_the_app_names_is_found_in_adwaita_and_in_hicolor_alone() {
    adw::init().unwrap();
    resources::register();
    let display = gtk::gdk::Display::default().unwrap();
    let shown = gtk::IconTheme::for_display(&display);
    // As GtkApplication does at startup, for its resource base path.
    shown.add_resource_path(&format!("{}/icons", resources::BASE));
    let carried = shown.resource_path();
    let carried: Vec<&str> = carried.iter().map(|path| path.as_str()).collect();
    let names = icons_named();
    assert!(names.len() > 15, "the scan found {names:?}");
    for (theme, system_folders) in [("Adwaita", true), ("hicolor", false)] {
        let icons = gtk::IconTheme::new();
        icons.set_resource_path(&carried);
        if !system_folders {
            icons.set_search_path(&[]);
        }
        icons.set_theme_name(Some(theme));
        let missing: Vec<_> = names.iter().filter(|name| !icons.has_icon(name)).collect();
        assert!(missing.is_empty(), "{theme} lacks {missing:?}");
    }
}

// The icons the views the program carries and its code name, and its own, which the welcome and
// About show.
fn icons_named() -> BTreeSet<String> {
    let mut names = BTreeSet::from([config::APP_ID.to_owned()]);
    let views = format!("{}/ui", resources::BASE);
    let none = gio::ResourceLookupFlags::NONE;
    for view in gio::resources_enumerate_children(&views, none).unwrap() {
        let xml = gio::resources_lookup_data(&format!("{views}/{view}"), none).unwrap();
        let xml = std::str::from_utf8(&xml).unwrap();
        for after in xml.split("icon-name\">").skip(1) {
            names.insert(after[..after.find('<').unwrap()].to_owned());
        }
    }
    let mut folders = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                // A file's tests come last, and are not the app's.
                let code = text.split("#[cfg(test)]").next().unwrap_or_default();
                names.extend(symbolic_literals(code));
            }
        }
    }
    names
}

fn symbolic_literals(code: &str) -> impl Iterator<Item = String> + '_ {
    code.match_indices("-symbolic\"").filter_map(|(end, _)| {
        let head = &code[..end];
        let start = head.rfind(|c: char| !matches!(c, 'a'..='z' | '0'..='9' | '-'))?;
        head[start..]
            .starts_with('"')
            .then(|| format!("{}-symbolic", &head[start + 1..]))
    })
}
