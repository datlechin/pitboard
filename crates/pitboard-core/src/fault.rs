//! Named points where a change can be killed partway.
//!
//! Recovery from an interrupted change is this project's central safety claim, and the way
//! it was tested was to plant a journal file and a vault item describing a crash that never
//! happened, then check that reconcile made sense of them. That tests reconcile. It does
//! not test the sequence that produced the state reconcile is given, which is where the
//! mistakes are: the orphan windows in enrolling and renewing were exactly this shape, and
//! they survived every one of those tests.
//!
//! So each durable step is named, and a test can say "die here". Dying is a panic rather
//! than an error, on purpose: an error would run the compensation the code has for it, and
//! the thing being tested is what is left when nothing gets to run. The harness catches the
//! panic the way the operating system would catch the process.
//!
//! In any build that is not a test this is an empty inline function. There is no feature to
//! set wrongly and nothing to strip from a release: `cfg(test)` cannot be on in one.

/// A durable step just finished. In a test that armed this name, the run dies here; in one
/// that hung something on it, that runs here first.
#[cfg(test)]
pub(crate) fn point(name: &'static str) {
    let hung = MEANWHILE.with(|meanwhile| {
        let mut meanwhile = meanwhile.borrow_mut();
        match meanwhile.as_ref() {
            Some((at, _)) if at == name => meanwhile.take().map(|(_, run)| run),
            _ => None,
        }
    });
    if let Some(run) = hung {
        run();
    }
    ARMED.with(|armed| {
        if armed.borrow().as_deref() == Some(name) {
            *armed.borrow_mut() = None;
            panic!("{KILLED}{name}");
        }
    });
}

/// Run `change`, and when it reaches `name`, run `elsewhere` there once, as if another
/// program had written at exactly that moment.
///
/// A kill tests what is left when a run stops. This tests what a run does when the world
/// moves under it: a tool that takes no lock can rewrite its own login between two steps
/// of a switch, and the switch has to notice rather than write over it.
#[cfg(test)]
pub(crate) fn meanwhile<T>(
    name: &'static str,
    elsewhere: impl FnOnce() + 'static,
    change: impl FnOnce() -> T,
) -> T {
    MEANWHILE
        .with(|meanwhile| *meanwhile.borrow_mut() = Some((name.to_string(), Box::new(elsewhere))));
    let outcome = change();
    MEANWHILE.with(|meanwhile| *meanwhile.borrow_mut() = None);
    outcome
}

#[cfg(not(test))]
#[inline(always)]
pub(crate) fn point(_name: &'static str) {}

#[cfg(test)]
const KILLED: &str = "pitboard fault: killed at ";

#[cfg(test)]
type Elsewhere = Box<dyn FnOnce()>;

#[cfg(test)]
thread_local! {
    static ARMED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    static MEANWHILE: std::cell::RefCell<Option<(String, Elsewhere)>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `change`, killing it at `name` if it gets there. `Ok` is what the change returned,
/// `Err(point)` is where it died.
///
/// The one difference from a real kill: a value dropped while the panic unwinds still runs
/// its destructor, so pitboard's own exclusivity lock and Claude Code's write lock are
/// released here where a killed process would leave the lock directory for staleness to
/// reclaim. That is the gentler of the two, and the lock protocol has its own tests.
#[cfg(test)]
pub(crate) fn killing<T>(name: &'static str, change: impl FnOnce() -> T) -> Result<T, String> {
    ARMED.with(|armed| *armed.borrow_mut() = Some(name.to_string()));
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(change));
    std::panic::set_hook(quiet);
    ARMED.with(|armed| *armed.borrow_mut() = None);
    outcome.map_err(|panic| {
        let said = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_default();
        match said.strip_prefix(KILLED) {
            Some(point) => point.to_string(),
            // Not our panic. A real failure inside the change must not read as a kill.
            None => panic!("{said}"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_dies_at_the_point_it_was_armed_for() {
        let reached = std::cell::Cell::new(0);
        let died = killing("second", || {
            point("first");
            reached.set(1);
            point("second");
            reached.set(2);
        });
        assert_eq!(died.unwrap_err(), "second");
        assert_eq!(reached.get(), 1, "it stopped where it was told to");
    }

    #[test]
    fn a_point_nobody_armed_is_nothing_at_all() {
        let done = killing("never reached", || {
            point("first");
            point("second");
            "finished"
        });
        assert_eq!(done.expect("no kill"), "finished");
    }

    #[test]
    fn the_arming_does_not_survive_the_run() {
        let _ = killing("first", || point("first"));
        let done = killing("nothing", || {
            point("first");
            "finished"
        });
        assert_eq!(done.expect("no kill"), "finished");
    }
}
