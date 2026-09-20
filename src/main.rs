//! pitboard — placeholder. The real surface lands in M1.
//!
//! Every user-facing string interpolates the binary name rather than hard-coding it,
//! so a rename never leaves a stale string behind.

fn main() {
    let name = env!("CARGO_BIN_NAME");
    let version = env!("CARGO_PKG_VERSION");
    println!("{name} {version} — not implemented yet");
}
