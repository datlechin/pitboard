//! Where Claude Desktop's login is, and where Pitboard keeps what it moves out of it.
//!
//! The names here are read from Claude Desktop 2.19675.0 and dated in the register.

use crate::context::Context;
use crate::provider::TreeItem;
use std::path::{Path, PathBuf};

/// The app's bundle identifier, which is how it is asked to quit and opened again.
pub(crate) const BUNDLE_ID: &str = "com.anthropic.claudefordesktop";

/// The bundle every one of the app's processes runs from.
pub(crate) const BUNDLE_NAME: &str = "Claude.app";

/// What a sentence calls the app, and what `open -a` opens.
pub(crate) const APP_NAME: &str = "Claude";

/// Paths inside the bundle whose processes are not the app's own. `chrome-native-host` is
/// started by Chrome for the browser extension, and holds nothing of the data folder.
pub(crate) const EXCLUDED: &[&str] = &["Contents/Helpers/chrome-native-host"];

/// The items of the data folder that belong to the account signed in, relative to it.
/// Experiment E2 found the list enough to move an account whole in both directions; an
/// item may be absent, as `File System` was for one of the two accounts.
pub(crate) const ITEMS: &[TreeItem] = &[
    TreeItem { path: "Cookies" },
    TreeItem {
        path: "Cookies-journal",
    },
    TreeItem {
        path: "Local Storage",
    },
    TreeItem {
        path: "Session Storage",
    },
    TreeItem {
        path: "IndexedDB/https_claude.ai_0.indexeddb.leveldb",
    },
    TreeItem {
        path: "IndexedDB/https_claude.ai_0.indexeddb.blob",
    },
    TreeItem { path: "WebStorage" },
    TreeItem {
        path: "File System",
    },
];

/// The keys of `config.json` that belong to the account signed in. Every other key there
/// belongs to the machine and stays where it is.
pub(crate) const CONFIG_KEYS: [&str; 3] = [
    "oauth:tokenCache",
    "oauth:tokenCacheV2",
    "lastKnownAccountUuid",
];

/// Chromium's link in the data folder naming the process that has it open. Claude Desktop
/// 2.19675.0 keeps none (experiment E14): its main process holds the `LOCK` files of its
/// leveldb stores instead. It is still looked for, since a build that writes one says the
/// app is open even where the process list misses it, and its absence says nothing.
pub(crate) const SINGLETON_LOCK: &str = "SingletonLock";

/// The start of every park of a tree login's name, told apart from a credential park's.
pub(crate) const PARK_PREFIX: &str = "pitboard-tree-";

/// Where Claude Desktop keeps its data on this machine: where the context says, or the
/// platform's own place for it. Claude Desktop does not run on Linux, so there it is
/// nowhere unless somebody says.
pub(crate) fn support_dir(ctx: &Context) -> Option<PathBuf> {
    if let Some(dir) = &ctx.desktop_dir {
        return Some(dir.clone());
    }
    crate::host::OS
        .claude_desktop_data()
        .map(|data| ctx.home().join(data))
}

/// The app's settings in a data folder, three keys of which are the account's.
pub(crate) fn config_file(root: &Path) -> PathBuf {
    root.join("config.json")
}

/// Chromium's cookie database in a data folder, which says whose session it holds.
pub(crate) fn cookies_db(root: &Path) -> PathBuf {
    root.join("Cookies")
}

/// What the app itself wrote down of its plan usage, which stays with the machine.
pub(crate) fn history_file(ctx: &Context) -> Option<PathBuf> {
    support_dir(ctx).map(|dir| dir.join("plan-usage-history.json"))
}

/// Pitboard's own directory for Claude Desktop: its parks, strays and journal.
pub(crate) fn desktop_home(ctx: &Context) -> PathBuf {
    crate::home::dir(ctx).join("desktop")
}

/// Where each account's items wait while another account is signed in.
pub(crate) fn parks_dir(ctx: &Context) -> PathBuf {
    desktop_home(ctx).join("parks")
}

/// Where what was in the way of a move goes, never deleted by a switch.
pub(crate) fn strays_dir(ctx: &Context) -> PathBuf {
    desktop_home(ctx).join("strays")
}

/// The version of the app the context names, as its bundle says. `None` where there is no
/// app there or it cannot be read, and always on Linux, where Claude Desktop does not run.
pub(crate) fn installed_version(ctx: &Context) -> Option<String> {
    ctx.desktop_app
        .as_deref()
        .and_then(|app| ctx.host().bundle_version(app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitboards_own_places_are_inside_its_home() {
        let ctx = Context::new(PathBuf::from("/home/x"))
            .with_pitboard_home(PathBuf::from("/scratch/.pitboard"));
        assert_eq!(desktop_home(&ctx), Path::new("/scratch/.pitboard/desktop"));
        assert_eq!(
            parks_dir(&ctx),
            Path::new("/scratch/.pitboard/desktop/parks")
        );
        assert_eq!(
            strays_dir(&ctx),
            Path::new("/scratch/.pitboard/desktop/strays")
        );
        assert_eq!(
            config_file(Path::new("/s/Claude")),
            Path::new("/s/Claude/config.json")
        );
        assert_eq!(
            cookies_db(Path::new("/s/Claude")),
            Path::new("/s/Claude/Cookies")
        );
        let ctx = ctx.with_desktop_dir("/s/Claude".into());
        assert_eq!(
            history_file(&ctx),
            Some(PathBuf::from("/s/Claude/plan-usage-history.json"))
        );
    }
}
