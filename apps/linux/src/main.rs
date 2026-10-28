#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    pitboard_linux::run()
}

// The app is Linux's alone: `--workspace` on another system builds it as a program that does
// nothing.
#[cfg(not(target_os = "linux"))]
fn main() {}
