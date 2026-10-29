//! The model's listener: each snapshot, told on the model's own thread, goes to the main loop
//! through one unbounded channel, and one task there takes them in the order they were told.
//! The model's thread never waits on the main loop.

use std::sync::Arc;

use pitboard_model::{PlatformError, Snapshot};

pub struct Listener(async_channel::Sender<Snapshot>);

impl pitboard_model::Listener for Listener {
    fn changed(&self, snapshot: Snapshot) -> Result<(), PlatformError> {
        // Unbounded, so a send waits on nothing: it fails only once the task has gone.
        self.0
            .send_blocking(snapshot)
            .map_err(|_| PlatformError::Failed {
                reason: "the app takes no more snapshots".into(),
            })
    }
}

/// A listener, and the task on the thread-default main context that hands `take` each snapshot
/// it is told, until the task is aborted or every listener is gone.
pub fn listening(take: impl Fn(Snapshot) + 'static) -> (Arc<Listener>, glib::JoinHandle<()>) {
    let (told, heard) = async_channel::unbounded();
    let task = glib::spawn_future_local(async move {
        while let Ok(snapshot) = heard.recv().await {
            take(snapshot);
        }
    });
    (Arc::new(Listener(told)), task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_model::Listener as _;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn each_snapshot_told_on_another_thread_is_taken_on_the_main_loop_in_order() {
        let context = glib::MainContext::new();
        context
            .with_thread_default(|| {
                let main = std::thread::current().id();
                let taken = Rc::new(RefCell::new(Vec::new()));
                let (listener, _task) = listening({
                    let taken = taken.clone();
                    move |snapshot: Snapshot| {
                        assert_eq!(std::thread::current().id(), main);
                        taken.borrow_mut().push(snapshot.revision);
                    }
                });
                let first = crate::testing::snapshot(1);
                std::thread::spawn(move || {
                    for revision in 1..=3 {
                        let snapshot = Snapshot {
                            revision,
                            ..first.clone()
                        };
                        listener.changed(snapshot).unwrap();
                    }
                })
                .join()
                .unwrap();

                while taken.borrow().len() < 3 {
                    context.iteration(true);
                }
                assert_eq!(*taken.borrow(), [1, 2, 3]);
            })
            .unwrap();
    }
}
