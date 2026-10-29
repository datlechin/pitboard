//! A folder watched on Windows: ReadDirectoryChangesW through `notify`.
//!
//! notify 8.2.1 ends a watch without an event where the folder is deleted, and where
//! ReadDirectoryChangesW answers with any other error (`src/windows.rs:339-368`). Each end is
//! told by the meta event `SingleWatchComplete`, which its server thread sends once it has
//! closed the folder's handle, so it is read here on a thread of its own. Dropped, the watch
//! waits for that thread, which ends once the server has closed every handle it held: a drop
//! has let go of the folder by the time it returns.

use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub(crate) struct Watcher {
    watcher: Option<notify::ReadDirectoryChangesWatcher>,
    ends: Option<std::thread::JoinHandle<()>>,
}

impl Watcher {
    /// Watches `dir`, not what is below it, telling `handler` each event and calling `ended`
    /// each time a watch ends, its own drop's included.
    pub(crate) fn on(
        dir: &Path,
        handler: impl notify::EventHandler,
        ended: impl Fn() + Send + 'static,
    ) -> notify::Result<Watcher> {
        use notify::Watcher as _;
        let (meta, metas) = std::sync::mpsc::channel();
        let mut watcher =
            notify::ReadDirectoryChangesWatcher::create(Arc::new(Mutex::new(handler)), meta)?;
        watcher.watch(dir, notify::RecursiveMode::NonRecursive)?;
        // An end before this thread starts waits in the channel.
        let ends = std::thread::Builder::new()
            .name("pitboard-watch-ends".into())
            .spawn(move || {
                for meta in metas {
                    if meta == notify::windows::MetaEvent::SingleWatchComplete {
                        ended();
                    }
                }
            })
            .map_err(notify::Error::io)?;
        Ok(Watcher {
            watcher: Some(watcher),
            ends: Some(ends),
        })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        drop(self.watcher.take());
        if let Some(ends) = self.ends.take() {
            let _ = ends.join();
        }
    }
}
