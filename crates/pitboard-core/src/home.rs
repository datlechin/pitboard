//! Pitboard's own directory. Every directory Pitboard creates is private to its owner,
//! whatever the umask: park file names contain account identifiers, so on a shared machine a
//! listing would leak.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::service::Permit;
use std::io;
use std::path::{Path, PathBuf};

/// Text that, anywhere in a path and as written, has always meant a sync client would copy
/// it to another machine, where a parked login must never go. It catches the clients'
/// folders, such as `Dropbox (Team)`, `Resilio Sync`, `Box Sync` and `Syncthing`, and with
/// them some folders that sync nothing, such as `SyncAdmin`. Every path it refuses stays
/// refused: a home in a synced folder is worse to let through than one in a folder that
/// only looks like one is to refuse.
const SYNCED: [&str; 5] = [
    "Dropbox",
    "Google Drive",
    "OneDrive",
    "com~apple~CloudDocs",
    "Sync",
];

/// What a sync client names a folder of its own, compared one folder of the path at a time
/// and in any case, since the file systems macOS and Windows make by default do not tell
/// `Dropbox` from `dropbox`. Each is the whole name, or the start of one: OneDrive for
/// Business's `OneDrive - <organisation>` and the `OneDrive-<kind>` macOS's File Provider
/// names, Dropbox's `Dropbox (<team>)`, and Google Drive for desktop's
/// `GoogleDrive-<account>`. A folder whose name only holds one of these, such as
/// `syncthing`, is not refused for it here; [`SYNCED`] refuses `Syncthing` as written.
fn a_sync_clients_folder(name: &str) -> bool {
    let name = name.to_lowercase();
    [
        "onedrive",
        "dropbox",
        "google drive",
        "com~apple~clouddocs",
        "sync",
    ]
    .contains(&name.as_str())
        || name.starts_with("onedrive - ")
        || name.starts_with("onedrive-")
        || (name.starts_with("dropbox (") && name.ends_with(')'))
        || name.starts_with("googledrive-")
}

/// Folders every folder inside of which is a sync client's, in any case: macOS keeps each
/// File Provider's folder in `~/Library/CloudStorage`, Box's, Dropbox's, Google Drive's and
/// OneDrive's among them, and every iCloud Drive folder, each app's as well as
/// `com~apple~CloudDocs`, in `~/Library/Mobile Documents`.
const SYNCED_INSIDE: [[&str; 2]; 2] =
    [["library", "cloudstorage"], ["library", "mobile documents"]];

/// The name, or the folders, that make `path` one a sync client copies elsewhere, if any:
/// [`SYNCED`] anywhere in it as written, then each folder of it by [`a_sync_clients_folder`]
/// and [`SYNCED_INSIDE`].
fn synced_by(path: &Path) -> Option<String> {
    let text = path.to_string_lossy();
    if let Some(marker) = SYNCED.iter().find(|marker| text.contains(**marker)) {
        return Some((*marker).to_string());
    }
    let names: Vec<String> = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    names.iter().enumerate().find_map(|(at, name)| {
        if a_sync_clients_folder(name) {
            return Some(name.clone());
        }
        let parent = at.checked_sub(1).map(|before| names[before].as_str())?;
        SYNCED_INSIDE
            .iter()
            .any(|[outer, inner]| parent.to_lowercase() == *outer && name.to_lowercase() == *inner)
            .then(|| format!("{parent}/{name}"))
    })
}

pub fn dir(ctx: &Context) -> PathBuf {
    ctx.pitboard_home.clone()
}

/// Removes what an older Pitboard kept here and this one does not: `readings/`, a
/// fortnight of readings per account that how long an account lasts was once worked out
/// from, before [`crate::pace`] took it from one reading. Only the files it wrote, one
/// `<account id>.ndjson` each, and then the folder where nothing else is in it:
/// `PITBOARD_HOME` can name a folder of somebody's own. Gone already is the ordinary answer.
pub(crate) fn remove_retired(ctx: &Context, permit: Permit) {
    let readings = dir(ctx).join("readings");
    let Ok(entries) = std::fs::read_dir(&readings) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        let written_by_pitboard = path.extension().is_some_and(|e| e == "ndjson")
            && path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| {
                    !stem.is_empty()
                        && stem
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                });
        if written_by_pitboard {
            let _ = crate::host::fs::remove_file(permit, &path);
        }
    }
    let _ = crate::host::fs::remove_dir(permit, &readings);
}

pub fn ensure(ctx: &Context, permit: Permit) -> io::Result<PathBuf> {
    let path = dir(ctx);
    crate::host::fs::create_private_dir(permit, &path)?;
    Ok(path)
}

/// Refuses a home this context names that is empty or relative, with `home_not_absolute`
/// and the variable that named it.
///
/// Every path Pitboard reads or writes is under one of these: the person's home, which
/// holds the scheduler's files and each tool's default folder; Pitboard's own directory;
/// Claude Code's config directory and the one its credential slot is named from; and
/// Codex's home. A relative one leads somewhere else from every folder a program runs in.
/// Claude Code uses `CLAUDE_CONFIG_DIR` and `CLAUDE_SECURESTORAGE_CONFIG_DIR` as they are
/// given, and Codex canonicalises `CODEX_HOME` against the folder it runs in, so a relative
/// one names a different login in each, and Pitboard would act on the one under the folder
/// it was run from. An empty `HOME` or `PITBOARD_HOME` left Pitboard's files in that folder.
/// An empty `CLAUDE_CONFIG_DIR` is Claude Code's config dir as the empty path, so whatever
/// folder it runs in, while its config file and its credential slot read it as unset
/// (`config_file_location` in its register).
///
/// The home is asked first, so a relative home is named rather than the Pitboard directory
/// worked out from it. Unset, each tool's variable names nothing; an empty `CODEX_HOME` is
/// unset to Codex, and an empty `CLAUDE_SECURESTORAGE_CONFIG_DIR` names Claude Code's
/// default folder.
///
/// The one gate every change passes asks this ([`crate::service::Permit`]), and so does
/// every read of Pitboard's account list, so nothing is read or written under such a home.
pub fn check_absolute(ctx: &Context) -> Result<()> {
    let homes: [(&'static str, Option<&Path>); 5] = [
        ("HOME", Some(ctx.home())),
        ("PITBOARD_HOME", Some(&ctx.pitboard_home)),
        (
            "CLAUDE_CONFIG_DIR",
            ctx.claude_config_dir.as_deref().map(Path::new),
        ),
        (
            "CLAUDE_SECURESTORAGE_CONFIG_DIR",
            ctx.secure_storage_dir
                .as_deref()
                .filter(|dir| !dir.is_empty())
                .map(Path::new),
        ),
        ("CODEX_HOME", ctx.codex_home().map(Path::new)),
    ];
    match homes
        .into_iter()
        .find_map(|(variable, path)| path.filter(|p| !p.is_absolute()).map(|p| (variable, p)))
    {
        Some((variable, path)) => Err(Error::HomeNotAbsolute {
            variable,
            path: path.to_path_buf(),
        }),
        None => Ok(()),
    }
}

/// Refuses a Pitboard directory a sync client would copy to another machine, by the names
/// in [`synced_by`], read off the path as the file system resolves it where it is there.
pub fn check_location(path: &Path) -> Result<()> {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    match synced_by(&resolved) {
        Some(marker) => Err(Error::StateOnSyncedDrive {
            path: resolved,
            marker,
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Environment;

    /// What `check_absolute` says of the context `pairs` make: the variable it names, or
    /// `None` where every home is a full path.
    fn refused_over(pairs: &[(&str, &str)]) -> Option<(&'static str, PathBuf)> {
        let env: Environment = pairs.iter().copied().collect();
        match check_absolute(&Context::for_command_line(&env)) {
            Ok(()) => None,
            Err(Error::HomeNotAbsolute { variable, path }) => Some((variable, path)),
            Err(other) => panic!("{other}"),
        }
    }

    /// The readings an older Pitboard kept to work out a rate from go, and nothing beside
    /// them does: not the files around them, and not a file in that folder Pitboard never
    /// wrote, which keeps the folder too. `PITBOARD_HOME` can name a folder of somebody's
    /// own.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn what_an_older_pitboard_kept_and_this_one_does_not_is_removed() {
        let root = std::env::temp_dir().join(format!(
            "pitboard-home-retired-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let ctx = Context::new(root.clone()).with_pitboard_home(root.join(".pitboard"));
        let readings = dir(&ctx).join("readings");
        std::fs::create_dir_all(&readings).expect("a readings folder");
        std::fs::write(readings.join("acc.ndjson"), "{}\n").expect("a reading");
        std::fs::write(dir(&ctx).join("usage.json"), "{}").expect("usage");

        remove_retired(&ctx, crate::service::Permit::for_a_test());
        assert!(!readings.exists());
        assert!(
            dir(&ctx).join("usage.json").exists(),
            "only what is retired"
        );
        remove_retired(&ctx, crate::service::Permit::for_a_test());

        std::fs::create_dir_all(&readings).expect("a readings folder");
        std::fs::write(readings.join("acc.ndjson"), "{}\n").expect("a reading");
        std::fs::write(readings.join("notes.txt"), "mine").expect("somebody's own");
        std::fs::write(readings.join("my data.ndjson"), "mine").expect("not an account id");
        remove_retired(&ctx, crate::service::Permit::for_a_test());
        assert!(!readings.join("acc.ndjson").exists());
        assert!(readings.join("notes.txt").exists());
        assert!(readings.join("my data.ndjson").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Each home the environment names is a full path or refused, by the variable that
    /// named it, and the home is asked first, so a relative one is named rather than the
    /// Pitboard directory worked out from it. Unset, a tool's variable names nothing, and an
    /// empty `CLAUDE_SECURESTORAGE_CONFIG_DIR` names Claude Code's default folder.
    #[test]
    #[cfg_attr(windows, ignore = "W14: Pitboard's own folder on Windows")]
    fn a_home_that_is_not_a_full_path_is_refused_by_the_variable_that_named_it() {
        let home = ("HOME", "/Users/x");
        for pairs in [
            &[home][..],
            &[home, ("PITBOARD_HOME", "/elsewhere/./pitboard/")],
            &[home, ("CLAUDE_CONFIG_DIR", "/Users/x/claude")],
            &[home, ("CLAUDE_SECURESTORAGE_CONFIG_DIR", "")],
            &[home, ("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/Users/x/slot")],
            &[home, ("CODEX_HOME", "/Users/x/codex")],
            &[home, ("CODEX_HOME", "")],
        ] {
            assert_eq!(refused_over(pairs), None, "{pairs:?}");
        }
        for (pairs, variable, path) in [
            (&[("HOME", "")][..], "HOME", ""),
            (&[("HOME", "relative")], "HOME", "relative"),
            (
                &[("HOME", "x"), ("PITBOARD_HOME", "/Users/x/.pitboard")],
                "HOME",
                "x",
            ),
            (&[home, ("PITBOARD_HOME", "")], "PITBOARD_HOME", ""),
            (
                &[home, ("PITBOARD_HOME", "pitboard")],
                "PITBOARD_HOME",
                "pitboard",
            ),
            (
                &[home, ("CLAUDE_CONFIG_DIR", "claude")],
                "CLAUDE_CONFIG_DIR",
                "claude",
            ),
            (&[home, ("CLAUDE_CONFIG_DIR", "")], "CLAUDE_CONFIG_DIR", ""),
            (
                &[home, ("CLAUDE_SECURESTORAGE_CONFIG_DIR", "./slot")],
                "CLAUDE_SECURESTORAGE_CONFIG_DIR",
                "./slot",
            ),
            (&[home, ("CODEX_HOME", "codex")], "CODEX_HOME", "codex"),
        ] {
            assert_eq!(
                refused_over(pairs),
                Some((variable, PathBuf::from(path))),
                "{pairs:?}"
            );
        }
    }

    /// The refusal says which variable, what it holds, and the two ways out.
    #[test]
    #[cfg_attr(windows, ignore = "W14: Pitboard's own folder on Windows")]
    fn a_home_that_is_not_a_full_path_is_said_with_the_way_out() {
        let said = |pairs: &[(&str, &str)]| {
            let env: Environment = pairs.iter().copied().collect();
            let refused = check_absolute(&Context::for_command_line(&env)).expect_err("refused");
            assert_eq!(refused.code(), "home_not_absolute");
            refused.to_string()
        };
        assert_eq!(
            said(&[("HOME", "")]),
            "HOME is empty, so the folder it names would depend on where each program runs. \
             Set it to a full path, or unset it; Pitboard reads and changes nothing until \
             then."
        );
        assert!(
            said(&[("HOME", "/Users/x"), ("CODEX_HOME", "codex")])
                .starts_with("CODEX_HOME is `codex`, which is not a full path, so ")
        );
    }

    /// What `check_location` refused before it read folder by folder: any of [`SYNCED`]
    /// anywhere in the path, as written.
    fn refused_before(path: &str) -> bool {
        SYNCED.iter().any(|marker| path.contains(marker))
    }

    /// The name `check_location` refuses `path` over, if it does.
    fn synced_over(path: &str) -> Option<String> {
        match check_location(Path::new(path)) {
            Ok(()) => None,
            Err(Error::StateOnSyncedDrive { marker, .. }) => Some(marker),
            Err(other) => panic!("{other}"),
        }
    }

    /// Real folder names of the sync clients, in the case each writes them and in others,
    /// on every system, with the name each is refused over. A folder inside macOS's
    /// `Library/CloudStorage` or `Library/Mobile Documents` is refused whatever it is
    /// called. Read as text anywhere in the path, as before, `onedrive`, `GoogleDrive-` and
    /// both of macOS's folders went through.
    #[test]
    fn a_sync_clients_folder_is_refused_by_its_name_in_any_case() {
        for (path, marker) in [
            ("/home/x/OneDrive/pitboard", "OneDrive"),
            ("/home/x/onedrive/pitboard", "onedrive"),
            ("/home/x/OneDrive - Contoso/pitboard", "OneDrive"),
            ("/home/x/onedrive - contoso/pitboard", "onedrive - contoso"),
            ("/home/x/OneDrive-Personal/pitboard", "OneDrive"),
            ("/home/x/ONEDRIVE-PERSONAL/pitboard", "ONEDRIVE-PERSONAL"),
            ("/home/x/Dropbox/pitboard", "Dropbox"),
            ("/home/x/dropbox/pitboard", "dropbox"),
            ("/home/x/Dropbox (Personal)/pitboard", "Dropbox"),
            ("/home/x/Dropbox (Business)/pitboard", "Dropbox"),
            ("/home/x/dropbox (team)/pitboard", "dropbox (team)"),
            (
                "/home/x/GoogleDrive-a@b.example/pitboard",
                "GoogleDrive-a@b.example",
            ),
            (
                "/home/x/googledrive-A@B.example/pitboard",
                "googledrive-A@B.example",
            ),
            ("/home/x/Google Drive/pitboard", "Google Drive"),
            ("/home/x/google drive/pitboard", "google drive"),
            (
                "/Users/x/Library/CloudStorage/Box-Box/pitboard",
                "Library/CloudStorage",
            ),
            ("/Users/x/library/cloudstorage/x", "library/cloudstorage"),
            (
                "/Users/x/Library/Mobile Documents/iCloud~md~obsidian/pitboard",
                "Library/Mobile Documents",
            ),
            (
                "/Users/x/Library/Mobile Documents/com~apple~CloudDocs/pitboard",
                "com~apple~CloudDocs",
            ),
            (
                "/Users/x/COM~APPLE~CLOUDDOCS/pitboard",
                "COM~APPLE~CLOUDDOCS",
            ),
            ("/home/x/Sync/pitboard", "Sync"),
            ("/home/x/sync/pitboard", "sync"),
        ] {
            assert_eq!(synced_over(path).as_deref(), Some(marker), "{path}");
        }
    }

    /// A sync client's folder name counts wherever it is in the path, the home's own
    /// folders included, so an account whose home is `/home/sync` or `/home/dropbox` has
    /// its `~/.pitboard` refused, though nothing syncs it. That is the stricter rule, as
    /// the CHANGELOG says: such a home went through before, being neither `Sync` nor
    /// `Dropbox` as written, and `PITBOARD_HOME` can name a folder outside it.
    #[test]
    fn a_home_named_as_a_sync_client_names_its_folder_is_refused_too() {
        for (path, marker) in [
            ("/home/sync/.pitboard", "sync"),
            ("/home/dropbox/.pitboard", "dropbox"),
            ("/home/OneDrive/.pitboard", "OneDrive"),
        ] {
            assert_eq!(synced_over(path).as_deref(), Some(marker), "{path}");
        }
        assert_eq!(synced_over("/home/syncer/.pitboard"), None);
    }

    /// Everything refused as text before is refused still, the sync clients' folders it
    /// caught by that text among them, and so are the folders that only look like one,
    /// such as `SyncAdmin`. A folder of no sync client, in any case, is not.
    #[test]
    fn every_folder_refused_before_is_refused_still() {
        let before = [
            "/home/x/Resilio Sync/pitboard",
            "/home/x/Box Sync/pitboard",
            "/home/x/Syncthing/pitboard",
            "/home/x/SyncAdmin/pitboard",
            "/home/x/OneDriveTools/pitboard",
            "/home/x/MyDropboxBackup/pitboard",
            "/home/x/Google Drive Stream/pitboard",
            "/Users/Synchro/.pitboard",
            "/home/x/Dropbox (Personal)/pitboard",
            "/home/x/OneDrive - Contoso/pitboard",
        ];
        for path in before {
            assert!(refused_before(path), "{path}");
            assert!(synced_over(path).is_some(), "{path}");
        }
        let neither = [
            "/Users/x/.pitboard",
            "/home/x/.pitboard",
            "/home/x/projects/synchronous/.pitboard",
            "/home/x/syncthing/pitboard",
            "/home/x/dropboxes/pitboard",
            "/home/x/onedrivetools/pitboard",
            "/Users/x/Library/Application Support/Pitboard",
            "/Users/x/Library/Caches/CloudStorage/pitboard",
            "/Users/x/Documents/Library/Mobile/pitboard",
        ];
        for path in neither {
            assert!(!refused_before(path), "{path}");
            assert_eq!(synced_over(path), None, "{path}");
        }
    }
}
