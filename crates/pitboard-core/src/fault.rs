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

/// A durable step just finished, or a wait came to nothing. In a test that armed this name,
/// the run dies here; in one that hung something on it, that runs here first.
#[cfg(test)]
pub(crate) fn point(name: &'static str) {
    TRAIL.with(|trail| {
        if let Some(reached) = trail.borrow_mut().as_mut() {
            reached.push(name);
        }
    });
    COUNTED.with(|counted| {
        if let Some((at, reached)) = counted.borrow_mut().as_mut()
            && at == name
        {
            *reached += 1;
        }
    });
    let hung = MEANWHILE.with(|meanwhile| {
        let mut meanwhile = meanwhile.borrow_mut();
        let at = meanwhile.iter().position(|(at, _)| at == name)?;
        Some(meanwhile.remove(at).1)
    });
    if let Some(run) = hung {
        run();
    }
    ARMED.with(|armed| {
        let mut armed = armed.borrow_mut();
        let Some((at, left)) = armed.as_mut() else {
            return;
        };
        if at != name {
            return;
        }
        *left -= 1;
        if *left == 0 {
            *armed = None;
            panic!("{KILLED}{name}");
        }
    });
}

/// Run `change`, and when it reaches `name`, run `elsewhere` there once, as if another
/// program had written at exactly that moment.
///
/// A kill tests what is left when a run stops. This tests what a run does when the world
/// moves under it: a tool that takes no lock can rewrite its own login between two steps
/// of a switch, and the switch has to notice rather than write over it. Called inside
/// another's `change`, each runs at its own point.
#[cfg(test)]
pub(crate) fn meanwhile<T>(
    name: &'static str,
    elsewhere: impl FnOnce() + 'static,
    change: impl FnOnce() -> T,
) -> T {
    MEANWHILE.with(|meanwhile| {
        meanwhile
            .borrow_mut()
            .push((name.to_string(), Box::new(elsewhere)));
    });
    let _hung = Hung(name);
    change()
}

// Takes down what `meanwhile` hung on its point however the change ends, a kill included.
#[cfg(test)]
struct Hung(&'static str);

#[cfg(test)]
impl Drop for Hung {
    fn drop(&mut self) {
        MEANWHILE.with(|meanwhile| meanwhile.borrow_mut().retain(|(at, _)| at != self.0));
    }
}

// How many times `change` reached `name`, with what it returned.
#[cfg(test)]
pub(crate) fn counting<T>(name: &'static str, change: impl FnOnce() -> T) -> (T, usize) {
    COUNTED.with(|counted| *counted.borrow_mut() = Some((name.to_string(), 0)));
    let outcome = change();
    let reached = COUNTED.with(|counted| counted.borrow_mut().take());
    (outcome, reached.map_or(0, |(_, reached)| reached))
}

// Every point `change` reached, in order, with what it returned, for Claude Desktop's tests.
#[cfg(all(test, not(windows)))]
pub(crate) fn trail<T>(change: impl FnOnce() -> T) -> (T, Vec<&'static str>) {
    TRAIL.with(|trail| *trail.borrow_mut() = Some(Vec::new()));
    let outcome = change();
    let reached = TRAIL.with(|trail| trail.borrow_mut().take());
    (outcome, reached.unwrap_or_default())
}

#[cfg(not(test))]
#[inline(always)]
pub(crate) fn point(_name: &'static str) {}

// Whether a test asked the step `name` to fail: an error the code handles, where a point
// is a kill it does not.
#[cfg(test)]
pub(crate) fn fails(name: &'static str) -> bool {
    FAILING.with(|failing| {
        let mut failing = failing.borrow_mut();
        let Some(asked) = failing
            .as_mut()
            .filter(|asked| asked.name == name && asked.at.is_none())
        else {
            return false;
        };
        asked.reached += 1;
        let fails = asked.nth.is_none_or(|nth| nth == asked.reached);
        asked.failed += usize::from(fails);
        fails
    })
}

#[cfg(not(test))]
#[inline(always)]
pub(crate) fn fails(_name: &'static str) -> bool {
    false
}

// `fails` for the one place `at` a step looks at.
#[cfg(test)]
fn fails_at(name: &str, at: &str) -> bool {
    FAILING.with(|failing| {
        let mut failing = failing.borrow_mut();
        let Some(asked) = failing
            .as_mut()
            .filter(|asked| asked.name == name && asked.at.as_deref() == Some(at))
        else {
            return false;
        };
        asked.failed += 1;
        true
    })
}

// The path a volume is compared by: one on another volume where a test put `path` there. Both
// sides of a comparison go through it, so a test refusal holds only for the path it named.
#[cfg(test)]
pub(crate) fn volume_of(path: &std::path::Path) -> &std::path::Path {
    if fails_at("another_volume", &path.to_string_lossy()) {
        std::path::Path::new("/dev")
    } else {
        path
    }
}

#[cfg(not(test))]
#[inline(always)]
pub(crate) fn volume_of(path: &std::path::Path) -> &std::path::Path {
    path
}

#[cfg(test)]
struct Failing {
    name: String,
    // the one place that fails, where only one does
    at: Option<String>,
    // the one time it fails, counted from 1, where only one does
    nth: Option<usize>,
    reached: usize,
    failed: usize,
}

// `change`, with every step `name` failing, and how many did.
#[cfg(test)]
fn failing_as<T>(asked: Failing, change: impl FnOnce() -> T) -> (T, usize) {
    FAILING.with(|failing| *failing.borrow_mut() = Some(asked));
    let outcome = change();
    let failed = FAILING.with(|failing| failing.borrow_mut().take());
    (outcome, failed.map_or(0, |asked| asked.failed))
}

#[cfg(test)]
pub(crate) fn failing<T>(name: &'static str, change: impl FnOnce() -> T) -> T {
    failing_counted(name, change).0
}

// `failing`, with how many steps failed.
#[cfg(test)]
pub(crate) fn failing_counted<T>(name: &'static str, change: impl FnOnce() -> T) -> (T, usize) {
    let asked = Failing {
        name: name.to_string(),
        at: None,
        nth: None,
        reached: 0,
        failed: 0,
    };
    failing_as(asked, change)
}

// `failing`, the `nth` time the change reaches the step alone, for Claude Desktop's tests.
#[cfg(all(test, not(windows)))]
pub(crate) fn failing_nth<T>(name: &'static str, nth: usize, change: impl FnOnce() -> T) -> T {
    let asked = Failing {
        name: name.to_string(),
        at: None,
        nth: Some(nth),
        reached: 0,
        failed: 0,
    };
    failing_as(asked, change).0
}

// `failing`, at the one place `at`.
#[cfg(test)]
pub(crate) fn failing_at<T>(name: &'static str, at: &str, change: impl FnOnce() -> T) -> T {
    let asked = Failing {
        name: name.to_string(),
        at: Some(at.to_string()),
        nth: None,
        reached: 0,
        failed: 0,
    };
    failing_as(asked, change).0
}

#[cfg(test)]
const KILLED: &str = "pitboard fault: killed at ";

#[cfg(test)]
type Elsewhere = Box<dyn FnOnce()>;

#[cfg(test)]
thread_local! {
    static ARMED: std::cell::RefCell<Option<(String, usize)>> =
        const { std::cell::RefCell::new(None) };
    static MEANWHILE: std::cell::RefCell<Vec<(String, Elsewhere)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static COUNTED: std::cell::RefCell<Option<(String, usize)>> =
        const { std::cell::RefCell::new(None) };
    static FAILING: std::cell::RefCell<Option<Failing>> = const { std::cell::RefCell::new(None) };
    static TRAIL: std::cell::RefCell<Option<Vec<&'static str>>> =
        const { std::cell::RefCell::new(None) };
    static KILLING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// One hook for the whole test run, set once: the hook is the process's, and tests run in
// parallel. It drops a kill inside `killing_nth` on this thread and passes on every other panic.
#[cfg(test)]
fn quiet_kills() {
    static SET: std::sync::Once = std::sync::Once::new();
    SET.call_once(|| {
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let killed = info
                .payload()
                .downcast_ref::<String>()
                .is_some_and(|said| said.starts_with(KILLED));
            if !(killed && KILLING.with(std::cell::Cell::get)) {
                before(info);
            }
        }));
    });
}

/// Run `change`, killing it at `name` if it gets there. `Ok` is what the change returned,
/// `Err(point)` is where it died.
///
/// The one difference from a real kill: a value dropped while the panic unwinds still runs
/// its destructor, so Pitboard's own exclusivity lock and Claude Code's write lock are
/// released here where a killed process would leave the lock directory for staleness to
/// reclaim. That is the gentler of the two, and the lock protocol has its own tests.
#[cfg(test)]
pub(crate) fn killing<T>(name: &'static str, change: impl FnOnce() -> T) -> Result<T, String> {
    killing_nth(name, 1, change)
}

// `killing`, at the `nth` time the change reaches `name`: a point reached once per item.
#[cfg(test)]
pub(crate) fn killing_nth<T>(
    name: &'static str,
    nth: usize,
    change: impl FnOnce() -> T,
) -> Result<T, String> {
    ARMED.with(|armed| *armed.borrow_mut() = Some((name.to_string(), nth)));
    quiet_kills();
    let was = KILLING.with(|killing| killing.replace(true));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(change));
    KILLING.with(|killing| killing.set(was));
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
    fn a_count_is_of_the_point_named_and_of_that_run() {
        let (done, reached) = counting("second", || {
            point("first");
            point("second");
            point("second");
            "finished"
        });
        assert_eq!((done, reached), ("finished", 2));
        point("second");
        assert_eq!(counting("second", || ()).1, 0);
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
