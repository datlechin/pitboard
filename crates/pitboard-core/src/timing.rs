// Temporary: J2's phase times on Windows CI, removed once read.
#![allow(
    clippy::disallowed_methods,
    reason = "a measurement's switches, removed with it"
)]

use std::sync::Mutex;
use std::time::Instant;

static MARKS: Mutex<Vec<(&'static str, Instant)>> = Mutex::new(Vec::new());

pub fn mark(label: &'static str) {
    if let Ok(mut marks) = MARKS.lock() {
        marks.push((label, Instant::now()));
    }
}

pub fn timed<T>(label: &'static str, f: impl FnOnce() -> T) -> T {
    let value = f();
    mark(label);
    value
}

pub fn on() -> bool {
    std::env::var_os("PITBOARD_J2").is_some()
}

pub fn flushed() -> bool {
    std::env::var_os("PITBOARD_J2_FLUSH").is_some()
}

// "J2 label=ms ...", each the time since the mark before, a repeated label numbered.
pub fn report() -> String {
    let Ok(marks) = MARKS.lock() else {
        return String::new();
    };
    let mut seen: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    let mut out = String::from("J2");
    for pair in marks.windows(2) {
        let (label, at) = pair[1];
        let n = seen.entry(label).or_insert(0);
        *n += 1;
        let ms = at.duration_since(pair[0].1).as_secs_f64() * 1000.0;
        out.push_str(&format!(" {label}.{n}={ms:.3}"));
    }
    if let (Some(first), Some(last)) = (marks.first(), marks.last()) {
        let ms = last.1.duration_since(first.1).as_secs_f64() * 1000.0;
        out.push_str(&format!(" total.1={ms:.3}"));
    }
    out
}
