//! Which build this is, from what Meson told Cargo, and the names each build answers to.
//!
//! Only Meson's release build is the release, since only it installs anything: whatever Cargo
//! builds alone is the debug build, `--release` or not. The debug build answers to an id, a
//! link scheme and an object path of its own, so no launcher, link, notification or search a
//! person's installed Pitboard owns ever reaches it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    Debug,
    Release,
}

impl Build {
    // `PITBOARD_BUILD` as Meson sets it, matched as bytes since a const fn cannot compare a str.
    pub const fn from_setting(setting: Option<&str>) -> Build {
        match setting {
            Some(setting) if matches!(setting.as_bytes(), b"release") => Build::Release,
            _ => Build::Debug,
        }
    }

    // GNOME's suffix for a development build's id.
    pub const fn app_id(self) -> &'static str {
        match self {
            Build::Release => "com.usepitboard.Pitboard",
            Build::Debug => "com.usepitboard.Pitboard.Devel",
        }
    }

    pub const fn link_scheme(self) -> &'static str {
        match self {
            Build::Release => "pitboard",
            Build::Debug => "pitboard-debug",
        }
    }

    // The path GApplication exports the app at, its id with each dot a slash, under which the
    // search provider and the runner are served.
    pub const fn object_path(self) -> &'static str {
        match self {
            Build::Release => "/com/usepitboard/Pitboard",
            Build::Debug => "/com/usepitboard/Pitboard/Devel",
        }
    }
}

// The package manager that installed this build, which Uninstall names the command of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Package {
    Deb,
    Rpm,
}

impl Package {
    // `PITBOARD_PACKAGE`, Meson's `package` option, where it names a package manager.
    pub const fn from_setting(setting: Option<&str>) -> Option<Package> {
        match setting {
            Some(setting) if matches!(setting.as_bytes(), b"deb") => Some(Package::Deb),
            Some(setting) if matches!(setting.as_bytes(), b"rpm") => Some(Package::Rpm),
            _ => None,
        }
    }
}

pub const BUILD: Build = Build::from_setting(option_env!("PITBOARD_BUILD"));
pub const APP_ID: &str = BUILD.app_id();
pub const LINK_SCHEME: &str = BUILD.link_scheme();
pub const OBJECT_PATH: &str = BUILD.object_path();
// Where Meson installs the program. A build Cargo made alone is installed nowhere.
pub const BINDIR: Option<&str> = option_env!("PITBOARD_BINDIR");
pub const PACKAGE: Option<Package> = Package::from_setting(option_env!("PITBOARD_PACKAGE"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_is_a_release_only_where_meson_says_so() {
        assert_eq!(Build::from_setting(Some("release")), Build::Release);
        for setting in [
            None,
            Some(""),
            Some("debug"),
            Some("Release"),
            Some("release "),
        ] {
            assert_eq!(Build::from_setting(setting), Build::Debug, "{setting:?}");
        }
        assert_eq!(BUILD, Build::Debug, "cargo test is never Meson's release");
        assert_eq!(BINDIR, None);
    }

    #[test]
    fn each_build_has_its_own_id_scheme_and_path() {
        assert_eq!(Build::Release.app_id(), "com.usepitboard.Pitboard");
        assert_eq!(Build::Release.link_scheme(), "pitboard");
        assert_eq!(Build::Release.object_path(), "/com/usepitboard/Pitboard");
        assert_eq!(Build::Debug.app_id(), "com.usepitboard.Pitboard.Devel");
        assert_eq!(Build::Debug.link_scheme(), "pitboard-debug");
        assert_eq!(
            Build::Debug.object_path(),
            "/com/usepitboard/Pitboard/Devel"
        );
        for build in [Build::Release, Build::Debug] {
            let path = build.app_id().replace('.', "/");
            assert_eq!(build.object_path(), format!("/{path}"), "{build:?}");
            assert!(gio::Application::id_is_valid(build.app_id()), "{build:?}");
        }
        assert_eq!(
            (APP_ID, LINK_SCHEME, OBJECT_PATH),
            (
                Build::Debug.app_id(),
                Build::Debug.link_scheme(),
                Build::Debug.object_path(),
            )
        );
    }

    #[test]
    fn a_package_is_named_only_where_meson_says_so() {
        assert_eq!(Package::from_setting(Some("deb")), Some(Package::Deb));
        assert_eq!(Package::from_setting(Some("rpm")), Some(Package::Rpm));
        for setting in [None, Some("none"), Some(""), Some("Deb")] {
            assert_eq!(Package::from_setting(setting), None, "{setting:?}");
        }
        assert_eq!(PACKAGE, None);
    }
}
