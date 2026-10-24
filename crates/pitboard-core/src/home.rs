//! Pitboard's own directory. Every directory Pitboard creates is private to its owner,
//! whatever the umask: park file names contain account identifiers, so on a shared machine a
//! listing would leak.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::host::Os;
use crate::permit::Permit;
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

pub(crate) fn dir(ctx: &Context) -> PathBuf {
    ctx.pitboard_home.clone()
}

/// Removes what an older Pitboard kept here and this one does not: `readings/`, a
/// fortnight of readings per account that how long an account lasts was once worked out
/// from, before [`crate::usage::pace`] took it from one reading. Only the files it wrote, one
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

// What stayed of a folder that could not be removed whole: each file that could not be
// removed, and each folder that could not where nothing in it stayed.
#[derive(Debug)]
pub(crate) struct Kept {
    pub(crate) left: Vec<PathBuf>,
    pub(crate) why: io::Error,
}

impl From<Kept> for io::Error {
    fn from(kept: Kept) -> io::Error {
        let left: Vec<String> = kept
            .left
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        io::Error::new(
            kept.why.kind(),
            format!("{} could not be removed: {}", left.join(", "), kept.why),
        )
    }
}

// A file another program holds open keeps its folder on exFAT and FAT32 (RF-C6x).
pub(crate) fn remove_all(permit: Permit, folder: &Path) -> std::result::Result<(), Kept> {
    let why = match crate::host::fs::remove_dir_all(permit, folder) {
        Ok(()) => return Ok(()),
        Err(_) if !folder.exists() => return Ok(()),
        Err(why) => why,
    };
    let mut left = Vec::new();
    remove_each(permit, folder, &mut left);
    if left.is_empty() {
        return Ok(());
    }
    left.sort();
    Err(Kept { left, why })
}

// std's `remove_dir_all` stops at the first file it cannot remove, so each is tried on its own.
fn remove_each(permit: Permit, folder: &Path, left: &mut Vec<PathBuf>) {
    let before = left.len();
    for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_each(permit, &path, left);
        } else if crate::host::fs::remove_file(permit, &path).is_err() {
            left.push(path);
        }
    }
    if crate::host::fs::remove_dir(permit, folder).is_err() && left.len() == before {
        left.push(folder.to_path_buf());
    }
}

pub(crate) fn ensure(ctx: &Context, permit: Permit) -> io::Result<PathBuf> {
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
/// The one gate every change passes asks this ([`crate::permit::Permit`]), and so does
/// every read of Pitboard's account list, so nothing is read or written under such a home.
pub(crate) fn check_absolute(ctx: &Context) -> Result<()> {
    let homes: [(&'static str, Option<&Path>); 5] = [
        (ctx.os.home_variable(), Some(ctx.home())),
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

// On Linux root may change things only where it owns each of these.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn written(ctx: &Context) -> Vec<PathBuf> {
    use crate::provider::{claude, codex};
    vec![
        ctx.home().to_path_buf(),
        dir(ctx),
        claude::paths::config_dir(ctx),
        claude::paths::config_file(ctx),
        PathBuf::from(claude::paths::storage_dir(ctx)),
        codex::paths::home(ctx),
    ]
}

// A sync client would copy a parked login to another machine.
pub(crate) fn check_location(ctx: &Context, path: &Path) -> Result<()> {
    check_location_with(path, &Syncing::of(ctx))
}

pub(crate) fn check_location_with(path: &Path, syncing: &Syncing) -> Result<()> {
    let resolved = path
        .canonicalize()
        .map_or_else(|_| path.to_path_buf(), crate::host::without_verbatim_prefix);
    match synced_by(&resolved).or_else(|| syncing.over(&resolved)) {
        Some(marker) => Err(Error::StateOnSyncedDrive {
            path: resolved,
            marker,
        }),
        None => Ok(()),
    }
}

// What Windows says it syncs, besides `synced_by`'s names: OneDrive's folders, a sync root
// (MC-sync), and a roaming profile but its `AppData\Local` and `LocalLow` (MC-roam).
pub(crate) struct Syncing<'a> {
    pub(crate) folders: Vec<PathBuf>,
    pub(crate) roaming: Option<PathBuf>,
    pub(crate) in_a_sync_root: &'a dyn Fn(&Path) -> bool,
}

impl Default for Syncing<'_> {
    fn default() -> Self {
        Syncing {
            folders: Vec::new(),
            roaming: None,
            in_a_sync_root: &|_| false,
        }
    }
}

impl Syncing<'static> {
    fn of(ctx: &Context) -> Syncing<'static> {
        match ctx.os {
            Os::MacOs | Os::Linux => Syncing::default(),
            Os::Windows => Syncing {
                folders: ctx.one_drive.clone(),
                roaming: crate::host::user::profile_roams()
                    .then(|| ctx.profile.clone().unwrap_or_else(|| ctx.home.clone())),
                in_a_sync_root: &crate::host::fs::in_a_sync_root,
            },
        }
    }
}

impl Syncing<'_> {
    // Compared as Windows compares a path, in any case.
    fn over(&self, path: &Path) -> Option<String> {
        let within = |folder: &Path| {
            path.ancestors()
                .any(|above| crate::host::same_path_in_any_case(above, folder))
        };
        if let Some(folder) = self.folders.iter().find(|folder| within(folder)) {
            return Some(folder.display().to_string());
        }
        if (self.in_a_sync_root)(path) {
            return Some("a cloud sync folder".into());
        }
        let profile = self.roaming.as_ref()?;
        let local = ["Local", "LocalLow"].map(|name| profile.join("AppData").join(name));
        (within(profile) && !local.iter().any(|folder| within(folder)))
            .then(|| "a roaming profile".into())
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::context::Environment;

    fn refused_over(os: Os, pairs: &[(&str, &str)]) -> Option<(&'static str, PathBuf)> {
        let env = Environment::on(os, pairs.iter().copied());
        match check_absolute(&Context::read(&env, "cli")) {
            Ok(()) => None,
            Err(Error::HomeNotAbsolute { variable, path }) => Some((variable, path)),
            Err(other) => panic!("{other}"),
        }
    }

    // Plain `su` gives root its own HOME and keeps the caller's tool homes, and a container can
    // be handed the caller's files where root's would be: each is a place a change writes in.
    #[test]
    fn a_change_writes_in_the_home_and_in_each_folder_the_environment_names() {
        let named = |pairs: &[(&str, &str)]| {
            written(&Context::read(
                &Environment::on(Os::Linux, pairs.iter().copied()),
                "cli",
            ))
        };
        assert_eq!(
            named(&[("HOME", "/nowhere/root")]),
            [
                "/nowhere/root",
                "/nowhere/root/.pitboard",
                "/nowhere/root/.claude",
                "/nowhere/root/.claude.json",
                "/nowhere/root/.claude",
                "/nowhere/root/.codex",
            ]
            .map(PathBuf::from)
        );
        assert_eq!(
            named(&[
                ("HOME", "/nowhere/root"),
                ("PITBOARD_HOME", "/nowhere/alice/.pitboard"),
                ("CLAUDE_CONFIG_DIR", "/nowhere/alice/.claude"),
                ("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/nowhere/alice/secure"),
                ("CODEX_HOME", "/nowhere/alice/.codex"),
            ]),
            [
                "/nowhere/root",
                "/nowhere/alice/.pitboard",
                "/nowhere/alice/.claude",
                "/nowhere/alice/.claude/.claude.json",
                "/nowhere/alice/secure",
                "/nowhere/alice/.codex",
            ]
            .map(PathBuf::from)
        );
    }

    /// The readings an older Pitboard kept to work out a rate from go, and nothing beside
    /// them does: not the files around them, and not a file in that folder Pitboard never
    /// wrote, which keeps the folder too. `PITBOARD_HOME` can name a folder of somebody's
    /// own.
    #[test]
    fn what_an_older_pitboard_kept_and_this_one_does_not_is_removed() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path().join("root");
        let ctx = Context::new(root.clone()).with_pitboard_home(root.join(".pitboard"));
        let readings = dir(&ctx).join("readings");
        std::fs::create_dir_all(&readings).expect("a readings folder");
        std::fs::write(readings.join("acc.ndjson"), "{}\n").expect("a reading");
        std::fs::write(dir(&ctx).join("usage.json"), "{}").expect("usage");

        remove_retired(&ctx, Permit::for_a_test());
        assert!(!readings.exists());
        assert!(
            dir(&ctx).join("usage.json").exists(),
            "only what is retired"
        );
        remove_retired(&ctx, Permit::for_a_test());

        std::fs::create_dir_all(&readings).expect("a readings folder");
        std::fs::write(readings.join("acc.ndjson"), "{}\n").expect("a reading");
        std::fs::write(readings.join("notes.txt"), "mine").expect("somebody's own");
        std::fs::write(readings.join("my data.ndjson"), "mine").expect("not an account id");
        remove_retired(&ctx, Permit::for_a_test());
        assert!(!readings.join("acc.ndjson").exists());
        assert!(readings.join("notes.txt").exists());
        assert!(readings.join("my data.ndjson").exists());
    }

    // Each file is tried on its own, so one that stays keeps no other.
    #[test]
    fn what_could_not_be_removed_is_named_alone_and_the_rest_goes() {
        use crate::host::fs::testing;
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let stuck = home.join("stuck");
        std::fs::create_dir_all(&stuck).expect("a folder");
        std::fs::write(stuck.join("kept"), "x").expect("written");
        for n in 0..20 {
            std::fs::write(home.join(format!("file-{n}")), "x").expect("written");
        }
        testing::deny_changes(&stuck).expect("read-only");
        let kept = remove_all(Permit::for_a_test(), &home);
        testing::allow_changes(&stuck).expect("changeable again");

        let kept = kept.expect_err("a file stayed");
        assert_eq!(kept.left, vec![stuck.join("kept")]);
        let mut there: Vec<PathBuf> = std::fs::read_dir(&home)
            .expect("still there")
            .map(|entry| entry.expect("listed").path())
            .collect();
        there.sort();
        assert_eq!(there, vec![stuck.clone()]);
        remove_all(Permit::for_a_test(), &home).expect("removed once it can be");
        assert!(!home.exists());
    }

    // std's remove_dir_all stops at the first file it cannot remove, which on NTFS comes first
    // by name.
    #[cfg(windows)]
    #[test]
    fn a_file_held_without_sharing_its_deletion_keeps_only_itself() {
        use crate::helper::{Helper, Job};
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        std::fs::create_dir_all(&home).expect("a folder");
        for name in ["audit.log", "state.json", "usage.json"] {
            std::fs::write(home.join(name), "x").expect("written");
        }
        let held = Helper::start(Job::HoldsWithoutDelete, &home.join("audit.log"));
        let kept = remove_all(Permit::for_a_test(), &home).expect_err("held");
        assert_eq!(kept.left, vec![home.join("audit.log")]);
        assert!(!home.join("state.json").exists());
        assert!(!home.join("usage.json").exists());
        held.stop();
        remove_all(Permit::for_a_test(), &home).expect("removed once let go of");
        assert!(!home.exists());
    }

    /// Each home the environment names is a full path or refused, by the variable that
    /// named it, and the home is asked first, so a relative one is named rather than the
    /// Pitboard directory worked out from it. Unset, a tool's variable names nothing, and an
    /// empty `CLAUDE_SECURESTORAGE_CONFIG_DIR` names Claude Code's default folder. The home
    /// is `USERPROFILE` on Windows.
    #[test]
    fn a_home_that_is_not_a_full_path_is_refused_by_the_variable_that_named_it() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let full = scratch.path().join("home");
        let at = |name: &str| full.join(name).to_string_lossy().into_owned();
        let (pitboard, claude, slot, codex) =
            (at("pitboard"), at("claude"), at("slot"), at("codex"));
        let x = full.to_string_lossy().into_owned();
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            let variable = os.home_variable();
            let home = (variable, x.as_str());
            for pairs in [
                &[home][..],
                &[home, ("PITBOARD_HOME", pitboard.as_str())],
                &[home, ("CLAUDE_CONFIG_DIR", claude.as_str())],
                &[home, ("CLAUDE_SECURESTORAGE_CONFIG_DIR", "")],
                &[home, ("CLAUDE_SECURESTORAGE_CONFIG_DIR", slot.as_str())],
                &[home, ("CODEX_HOME", codex.as_str())],
                &[home, ("CODEX_HOME", "")],
            ] {
                assert_eq!(refused_over(os, pairs), None, "{os:?} {pairs:?}");
            }
            for (pairs, refused, path) in [
                (&[(variable, "")][..], variable, ""),
                (&[(variable, "relative")], variable, "relative"),
                (
                    &[(variable, "x"), ("PITBOARD_HOME", pitboard.as_str())],
                    variable,
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
                    refused_over(os, pairs),
                    Some((refused, PathBuf::from(path))),
                    "{os:?} {pairs:?}"
                );
            }
        }
    }

    /// The refusal says which variable, what it holds, and the two ways out.
    #[test]
    fn a_home_that_is_not_a_full_path_is_said_with_the_way_out() {
        let said = |pairs: &[(&str, &str)]| {
            let env: Environment = pairs.iter().copied().collect();
            let refused = check_absolute(&Context::for_command_line(&env)).expect_err("refused");
            assert_eq!(refused.code(), "home_not_absolute");
            refused.to_string()
        };
        let variable = crate::host::OS.home_variable();
        assert_eq!(
            said(&[(variable, "")]),
            format!(
                "{variable} is empty, so the folder it names would depend on where each \
                 program runs. Set it to a full path, or unset it; Pitboard reads and changes \
                 nothing until then."
            )
        );
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let full = scratch.path().to_string_lossy().into_owned();
        assert!(
            said(&[(variable, full.as_str()), ("CODEX_HOME", "codex")])
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
        synced_on(path, &Syncing::default())
    }

    fn synced_on(path: &str, syncing: &Syncing) -> Option<String> {
        match check_location_with(Path::new(path), syncing) {
            Ok(()) => None,
            Err(Error::StateOnSyncedDrive { marker, .. }) => Some(marker),
            Err(other) => panic!("{other}"),
        }
    }

    // MC-sync and MC-roam: what Windows says of a folder, which no runner can make it say.
    #[test]
    fn a_windows_folder_that_syncs_or_roams_is_refused() {
        let roots = |path: &Path| path.starts_with("/u/Box");
        let syncing = Syncing {
            folders: vec![PathBuf::from("/u/Work Files")],
            roaming: Some(PathBuf::from("/u/profile")),
            in_a_sync_root: &roots,
        };
        for (path, marker) in [
            ("/u/Work Files/pitboard", "/u/Work Files"),
            ("/u/work files/x/pitboard", "/u/Work Files"),
            ("/u/Box/pitboard", "a cloud sync folder"),
            ("/u/profile/pitboard", "a roaming profile"),
            ("/u/profile/AppData/Roaming/Pitboard", "a roaming profile"),
            ("/u/PROFILE/AppData/LocalLowly", "a roaming profile"),
        ] {
            assert_eq!(synced_on(path, &syncing).as_deref(), Some(marker), "{path}");
        }
        for path in [
            "/u/profile/AppData/Local/Pitboard",
            "/u/profile/appdata/local",
            "/u/profile/AppData/LocalLow/Pitboard",
            "/u/elsewhere/pitboard",
            "/u/Boxes/pitboard",
        ] {
            assert_eq!(synced_on(path, &syncing), None, "{path}");
        }
        assert_eq!(
            synced_on("/u/profile/pitboard", &Syncing::default()),
            None,
            "a local profile"
        );
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
