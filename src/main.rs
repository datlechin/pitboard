//! pitboard — park and restore your own Claude Code logins, and see what each has left.
//!
//! Everything in this build is read-only. There is no write path yet.

mod claude;
mod doctor;
mod slot;
mod status;
mod store;
mod time;
mod usage;

const USAGE: &str = "\
pitboard — park and restore your own Claude Code logins

  pitboard status        what is signed in, and how much of it is left  (default)
  pitboard doctor        check that this tool's model of Claude Code still holds
  pitboard version

Options
  --json                 machine-readable output

Everything this build does is read-only.
";

/// 0 success · 1 failure · 2 wrong usage · 3 an assumption about Claude Code broke.
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let verb = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("status");

    match verb {
        "status" | "ls" | "list" => {
            let r = status::gather();
            if json {
                println!("{}", status::render_json(&r));
            } else {
                print!("\n{}\n", status::render_human(&r));
            }
            std::process::ExitCode::SUCCESS
        }
        "doctor" => {
            let checks = doctor::run();
            if json {
                println!("{}", doctor::render_json(&checks));
            } else {
                print!("\n{}\n", doctor::render_human(&checks));
            }
            if checks.iter().any(|c| c.level == doctor::Level::Fail) {
                std::process::ExitCode::from(3)
            } else {
                std::process::ExitCode::SUCCESS
            }
        }
        "version" | "--version" | "-V" => {
            println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
            std::process::ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            std::process::ExitCode::SUCCESS
        }
        other => {
            eprintln!("{}: unknown command `{other}`\n", env!("CARGO_BIN_NAME"));
            eprint!("{USAGE}");
            std::process::ExitCode::from(2)
        }
    }
}
