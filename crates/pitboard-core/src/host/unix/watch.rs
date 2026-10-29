//! A folder watched on macOS and Linux: FSEvents and inotify through `notify`, each of which
//! tells the folder's own removal and every event it dropped as an event of its own.

use std::path::Path;

// Dropped, it stops the watch.
#[derive(Debug)]
pub(crate) struct Watcher {
    _watcher: notify::RecommendedWatcher,
}

impl Watcher {
    /// Watches `dir`, not what is below it, telling `handler` each event. `ended` is never
    /// called: an end comes as an event.
    pub(crate) fn on(
        dir: &Path,
        handler: impl notify::EventHandler,
        _ended: impl Fn() + Send + 'static,
    ) -> notify::Result<Watcher> {
        use notify::Watcher as _;
        let mut watcher = notify::RecommendedWatcher::new(handler, notify::Config::default())?;
        watcher.watch(dir, notify::RecursiveMode::NonRecursive)?;
        Ok(Watcher { _watcher: watcher })
    }
}
