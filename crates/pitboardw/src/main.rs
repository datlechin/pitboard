//! What a Windows task starts in place of `pitboard.exe`. A task gives a console program a
//! window below Windows 11 24H2, which reads no `consoleAllocationPolicy`, and the watch
//! service's would stay open all session. This is a GUI program, which Windows gives no
//! console, and it starts the `pitboard.exe` beside it with a console that has no window.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod launch;
#[cfg(any(windows, test))]
mod line;

#[cfg(windows)]
fn main() {
    launch::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("pitboardw starts pitboard.exe on Windows alone");
    std::process::exit(2);
}
