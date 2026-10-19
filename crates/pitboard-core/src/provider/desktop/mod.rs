pub(crate) mod assumptions;
mod jar;
mod layout;
pub(crate) mod paths;
mod running;
mod settings;

use crate::context::Context;
use crate::holder::{AppId, Holder, Location, Noun, Remedy};
use crate::host::Listed;
use crate::provider::folder::{AppSettings, Class, Folder, Keys, SignedIn, Unfit, Unread};
use crate::provider::{ProviderError, ProviderId};
use crate::service::Permit;
use serde_json::value::RawValue;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub(crate) const BUNDLE_ID: &str = "com.anthropic.claudefordesktop";

pub(crate) const HOLDER: Holder = Holder {
    kind: "claude_desktop",
    noun: Noun::One("Claude"),
    location: Location::Anywhere,
    remedy: Remedy::ReopenApp {
        app: AppId::MacBundle(BUNDLE_ID),
        name: "Claude",
    },
};

// The jar first: it holds the session.
const ITEMS: &[&str] = &[
    "Cookies",
    "Cookies-journal",
    "Local Storage",
    "Session Storage",
    "IndexedDB/https_claude.ai_0.indexeddb.leveldb",
    "IndexedDB/https_claude.ai_0.indexeddb.blob",
    "WebStorage",
    "File System",
    "Service Worker",
    "Platform Notifications",
    "CdmStorage.db",
    "CdmStorage.db-journal",
    "declarative_performance_observer.db",
    "declarative_performance_observer.db-journal",
    "Partitions",
    "chrome-import-sites.json",
    "builtin-mcp-auth",
    "custom3p-builtin-mcp.json",
    "custom3p-builtin-mcp.json.journal",
    "session-popouts.json",
    "local-sessions.json",
];

const NAMING_KEY: &str = "lastKnownAccountUuid";
const CACHES: &[&str] = &["oauth:tokenCache", "oauth:tokenCacheV2"];
const KEYS: &[&str] = &[
    "oauth:tokenCache",
    "oauth:tokenCacheV2",
    NAMING_KEY,
    "custom3pMcpOAuth",
];

#[derive(Debug)]
pub(crate) struct Desktop;

impl Folder for Desktop {
    fn id(&self) -> ProviderId {
        ProviderId::Desktop
    }

    fn root(&self, ctx: &Context) -> Result<PathBuf, ProviderError> {
        paths::root(ctx)
    }

    fn refuses(&self, root: &Path) -> Option<Unfit> {
        paths::refuses(root)
    }

    fn items(&self) -> &'static [&'static str] {
        ITEMS
    }

    fn keys(&self) -> &'static [&'static str] {
        KEYS
    }

    fn caches(&self) -> &'static [&'static str] {
        CACHES
    }

    fn naming_key(&self) -> &'static str {
        NAMING_KEY
    }

    fn classify(&self, name: &OsStr) -> Class {
        layout::classify(name)
    }

    fn settings(&self, root: &Path) -> Result<AppSettings, Unread> {
        settings::read(root, NAMING_KEY)
    }

    fn write_settings(
        &self,
        permit: Permit,
        root: &Path,
        read: &AppSettings,
        keys: &Keys,
    ) -> Result<(), Unread> {
        settings::write(permit, root, read, &read.with(self, keys))
    }

    fn session(&self, ctx: &Context, jar: &Path) -> Result<Option<SignedIn>, Unread> {
        jar::session(ctx, jar)
    }

    fn emptied(&self, key: &str, value: &RawValue) -> bool {
        jar::emptied(key, value)
    }

    fn is_the_app(&self, ctx: &Context, root: &Path, one: &Listed, all: &[Listed]) -> bool {
        running::is_the_app(ctx, root, one, all)
    }

    fn holder(&self) -> &'static Holder {
        &HOLDER
    }

    fn app_name(&self) -> &'static str {
        "Claude"
    }
}
