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

/// What `fstatfs` says of the file system `path` is on, read from the nearest folder that
/// opens, as Codex's `uses_wsl_drvfs` reads it. `None` where none opens or it says nothing.
pub(super) fn file_system(path: &std::path::Path) -> Option<libc::statfs> {
    use std::os::fd::AsRawFd;
    let folder = path
        .ancestors()
        .find_map(|above| std::fs::File::open(above).ok())?;
    let mut stats = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: the descriptor is open for the call, which fills the buffer and keeps neither.
    if unsafe { libc::fstatfs(folder.as_raw_fd(), stats.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: fstatfs returned 0, so it filled the buffer.
    Some(unsafe { stats.assume_init() })
}

// Instant reads CLOCK_UPTIME_RAW on macOS and CLOCK_MONOTONIC on Linux, and neither counts a
// sleep (macOS's clock_gettime(3), Linux man-pages 6.19).
pub(super) fn awake() -> std::time::Duration {
    static SINCE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    SINCE.get_or_init(std::time::Instant::now).elapsed()
}
