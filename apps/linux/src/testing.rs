//! What the app's own tests make the model of: one over homes of the test's own, never started,
//! which reads and writes nothing until it is sent `Intent::Start`.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use pitboard_model::{AppLaunch, Front, PitboardModel, PlatformError, Snapshot};

struct Unheard;

impl pitboard_model::Listener for Unheard {
    fn changed(&self, _: Snapshot) -> Result<(), PlatformError> {
        Ok(())
    }
}

impl pitboard_model::Notifier for Unheard {
    fn post(&self, _: pitboard_model::Notice) -> Result<(), PlatformError> {
        Ok(())
    }
}

/// The model, and the folder its homes are in, which goes with it.
pub(crate) fn unstarted() -> (Arc<PitboardModel>, tempfile::TempDir) {
    let scratch = tempfile::tempdir().unwrap();
    let environment: HashMap<String, String> =
        ["HOME", "PITBOARD_HOME", "CLAUDE_CONFIG_DIR", "CODEX_HOME"]
            .into_iter()
            .map(|name| {
                let folder = scratch.path().join(name);
                (name.to_owned(), folder.to_string_lossy().into_owned())
            })
            .collect();
    let launch = AppLaunch {
        environment,
        app_location: None,
        front: Front::Gnome,
        account_windows: None,
        handover: None,
        widget_feed: None,
    };
    let clock = crate::local_time::DesktopClock::following(None);
    let model = PitboardModel::new(
        launch,
        Arc::new(Unheard),
        None,
        Arc::new(Unheard),
        clock.local_time(),
    );
    (model, scratch)
}

/// What that model shows before it is started, numbered `revision`.
pub(crate) fn snapshot(revision: u64) -> Snapshot {
    static FIRST: OnceLock<Snapshot> = OnceLock::new();
    let first = FIRST.get_or_init(|| {
        let (model, _scratch) = unstarted();
        let first = model.snapshot();
        model.shutdown();
        first
    });
    Snapshot {
        revision,
        ..first.clone()
    }
}
