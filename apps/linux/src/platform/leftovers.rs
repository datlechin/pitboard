//! What Uninstall takes back of the app in the person's home once the core removed Pitboard:
//! its start at sign-in, and its data and cache folders, its id in `$XDG_DATA_HOME` and
//! `$XDG_CACHE_HOME`. Removing a package reaches no home, so the app takes these back itself
//! before it names the package's command.

use std::path::PathBuf;

use super::Failed;
use super::autostart::StartAtSignIn;
use crate::config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leftover {
    StartAtSignIn,
    Folder(PathBuf),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Removed {
    pub went: Vec<Leftover>,
    pub stayed: Vec<(Leftover, Failed)>,
}

/// The app's data and cache folders in this person's home. GLib reads `XDG_DATA_HOME` and
/// `XDG_CACHE_HOME` once for the process.
pub fn folders_of_this_user() -> [PathBuf; 2] {
    [
        glib::user_data_dir().join(config::APP_ID),
        glib::user_cache_dir().join(config::APP_ID),
    ]
}

/// Turns start at sign-in off, where the build has one, and removes each folder, saying what
/// went and what could not.
pub fn remove(start_at_sign_in: Option<&dyn StartAtSignIn>, folders: &[PathBuf]) -> Removed {
    let mut removed = Removed::default();
    match start_at_sign_in.map(StartAtSignIn::turn_off) {
        Some(Ok(true)) => removed.went.push(Leftover::StartAtSignIn),
        Some(Ok(false)) | None => {}
        Some(Err(failed)) => removed.stayed.push((Leftover::StartAtSignIn, failed)),
    }
    // A link in a folder goes, and what it names stays: remove_dir_all follows none.
    for folder in folders {
        let leftover = Leftover::Folder(folder.clone());
        match std::fs::remove_dir_all(folder) {
            Ok(()) => removed.went.push(leftover),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => removed
                .stayed
                .push((leftover, Failed(format!("{}: {e}", folder.display())))),
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::autostart::Autostart;
    use std::path::Path;

    const ID: &str = "com.usepitboard.Pitboard";

    fn holding_a_file(folder: &Path) -> PathBuf {
        std::fs::create_dir_all(folder).unwrap();
        let file = folder.join("file");
        std::fs::write(&file, "kept").unwrap();
        file
    }

    #[test]
    fn uninstall_removes_the_apps_own_entry_and_folders_and_nothing_else() {
        let scratch = tempfile::tempdir().unwrap();
        let (config, data, cache) = (
            scratch.path().join("config"),
            scratch.path().join("data"),
            scratch.path().join("cache"),
        );
        let autostart = Autostart::new(&config.join("autostart"), ID, "/usr/bin/pitboard-app");
        autostart.turn_on().unwrap();
        let theirs = config.join("autostart/org.example.Other.desktop");
        std::fs::write(
            &theirs,
            "[Desktop Entry]\nType=Application\nName=Other\nExec=other\n",
        )
        .unwrap();
        let ours = [data.join(ID), cache.join(ID)];
        for folder in &ours {
            holding_a_file(&folder.join("web/0b6e2a56-8f52-5b3e-9d3c-3f1c2c4f6a10"));
        }
        let kept: Vec<_> = [
            data.join(format!("{ID}.Devel")),
            cache.join(format!("{ID}.Devel")),
            data.join("org.example.Other"),
        ]
        .iter()
        .map(|folder| holding_a_file(folder))
        .collect();
        // A link in the data folder goes, and what it names stays.
        let named = holding_a_file(&scratch.path().join("elsewhere"));
        std::os::unix::fs::symlink(named.parent().unwrap(), ours[0].join("link")).unwrap();

        let removed = remove(Some(&autostart), &ours);

        assert_eq!(
            removed,
            Removed {
                went: vec![
                    Leftover::StartAtSignIn,
                    Leftover::Folder(ours[0].clone()),
                    Leftover::Folder(ours[1].clone()),
                ],
                stayed: vec![],
            }
        );
        assert!(!autostart.entry().exists());
        for folder in &ours {
            assert!(!folder.exists(), "{}", folder.display());
        }
        for file in kept.iter().chain([&theirs, &named]) {
            assert!(file.exists(), "{}", file.display());
        }
        assert_eq!(
            remove(Some(&autostart), &ours),
            Removed::default(),
            "nothing is left to go"
        );
    }
}
