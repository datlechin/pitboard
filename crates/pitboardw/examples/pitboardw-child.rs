//! What `pitboardw`'s tests start in place of `pitboard.exe`. It prints its process id, whether
//! its console has a window, and each argument, then reads its input to the end when the first
//! argument is `wait`, and exits with the code an `exit=<code>` argument names.

use std::io::{Read, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    writeln!(out, "pid {}", std::process::id()).expect("printed");
    writeln!(out, "console {}", console()).expect("printed");
    for arg in &args {
        writeln!(out, "arg {arg}").expect("printed");
    }
    out.flush().expect("printed");
    if args.first().is_some_and(|first| first == "wait") {
        let _ = std::io::stdin().read_to_end(&mut Vec::new());
    }
    let code = args
        .iter()
        .find_map(|arg| arg.strip_prefix("exit=")?.parse().ok())
        .unwrap_or(0);
    std::process::exit(code);
}

#[cfg(windows)]
fn console() -> &'static str {
    use windows_sys::Win32::System::Console::GetConsoleWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    // SAFETY: neither takes a pointer, and the window is only asked whether it is visible.
    unsafe {
        let window = GetConsoleWindow();
        if window.is_null() {
            "none"
        } else if IsWindowVisible(window) == 0 {
            "hidden"
        } else {
            "shown"
        }
    }
}

#[cfg(not(windows))]
fn console() -> &'static str {
    "none"
}
