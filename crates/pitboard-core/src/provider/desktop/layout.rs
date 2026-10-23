use super::ITEMS;

// SQLite and Claude's stores keep these beside a file, and each goes where the file goes.
const BESIDE: &[&str] = &["-journal", "-wal", "-shm", ".journal"];

// Chromium names a site's IndexedDB stores after its origin.
const CLAUDE_AI_INDEXEDDB: &str = "IndexedDB/https_claude.ai_0.";

// Where Chromium keeps the jar once it moves its network service's files.
const NETWORK: &str = "Network";

// The register's `desktop_names_stay` and `desktop_store_parts`: every name but the items
// stays, so a move stops only for a part of an item's store that no item names.
pub(super) fn unread_part(path: &str) -> bool {
    !ITEMS.contains(&path)
        && (path == NETWORK
            || path.starts_with(CLAUDE_AI_INDEXEDDB)
            || ITEMS.iter().any(|item| {
                path.strip_prefix(item)
                    .is_some_and(|beside| BESIDE.contains(&beside))
            }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_part_of_an_items_store_that_no_item_names_is_unread() {
        for path in [
            "Network",
            "Cookies-wal",
            "Cookies-shm",
            "Cookies.journal",
            "CdmStorage.db-wal",
            "session-popouts.json.journal",
            "IndexedDB/https_claude.ai_0.indexeddb.sqlite",
            "IndexedDB/https_claude.ai_0.indexeddb.leveldb-journal",
        ] {
            assert!(unread_part(path), "{path}");
        }
        for path in [
            "Cookies",
            "Cookies-journal",
            "SharedStorage",
            "SharedStorage-wal",
            "custom3p-builtin-mcp.json.journal",
            "IndexedDB/https_claude.ai_0.indexeddb.leveldb",
            "IndexedDB/https_claude.ai_0.indexeddb.blob",
            "IndexedDB/https_www.example.com_0.indexeddb.leveldb",
            "IndexedDB",
            "config.json.journal",
            "session-popouts.json.tmp",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.Cookies.a1B2c3",
            "SingletonLock",
            "design",
            "something-new",
        ] {
            assert!(!unread_part(path), "{path}");
        }
    }
}
