//! `pitboard-probe-longpath`: the probe's command line with `pitboard.exe`'s manifest embedded
//! by `build.rs`, for T8's long path row.

fn main() -> std::process::ExitCode {
    pitboard_probe::cli::main()
}
