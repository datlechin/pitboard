//! What macOS and Linux do the same way, because both are POSIX: modes, signals, the
//! passwd database, a login shell, and a service manager asked from the command line.

#[allow(
    clippy::disallowed_methods,
    reason = "the face every file and folder Pitboard makes is made through"
)]
pub(crate) mod fs;
pub(crate) mod proc;
pub(crate) mod service;
pub(crate) mod shell;
pub(crate) mod user;
pub(crate) mod watch;

// Instant reads CLOCK_UPTIME_RAW on macOS and CLOCK_MONOTONIC on Linux, and neither counts a
// sleep (macOS's clock_gettime(3), Linux man-pages 6.19).
pub(super) fn awake() -> std::time::Duration {
    static SINCE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    SINCE.get_or_init(std::time::Instant::now).elapsed()
}
