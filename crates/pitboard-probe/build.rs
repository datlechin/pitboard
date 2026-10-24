//! Embed a manifest into four of the probe's programs, on the MSVC toolchain only:
//!
//! - `pitboard-probe-detached`: `consoleAllocationPolicy=detached` in the spelling
//!   Microsoft's application manifest reference documents (`asmv3:windowsSettings` in the
//!   2024 WindowsSettings namespace);
//! - `pitboard-probe-detached-asmv1`: the same policy in an undocumented spelling (a plain
//!   `<application><windowsSettings>` in the asm.v1 namespace), so block E4 can tell whether
//!   only the documented form is honoured;
//! - `pitboard-probe-sparse`: the `<msix>` element that ties the program to the sparse probe
//!   package of block K1, so running it from the package's external location gives it that
//!   package's identity;
//! - `pitboard-probe-longpath`: `pitboard.exe`'s own manifest, so T8's long path row reads
//!   what its `longPathAware` does.
//!
//! Nothing is embedded on any other target, and the probe builds everywhere. The console
//! block reads each program's manifest back, so a build that embedded nothing says so.
//!
//! It also records which compiler built the probe, since block F3 measures that compiler's
//! standard library, and on MSVC gives every program an 8 MB main stack.

// A build script reads its configuration from the environment Cargo sets; the workspace
// rule about reading the environment through the context is for Pitboard's engine, not here.
#![allow(clippy::disallowed_methods)]

use std::path::Path;

fn main() {
    // The compiler that built the probe, whose standard library's lookup order block F3
    // measures.
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = std::process::Command::new(rustc)
        .arg("-V")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=PITBOARD_PROBE_RUSTC={version}");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    // clap's parser of every subcommand outgrows the 1 MB main stack in a debug build
    // (docs/facts/windows-runners.md).
    println!("cargo:rustc-link-arg-bins=/STACK:8388608");

    let manifests = Path::new(env!("CARGO_MANIFEST_DIR")).join("manifests");
    embed(
        "pitboard-probe-detached",
        &manifests.join("detached.manifest"),
    );
    embed(
        "pitboard-probe-detached-asmv1",
        &manifests.join("detached-asmv1.manifest"),
    );
    embed(
        "pitboard-probe-sparse",
        &manifests.join("sparse-identity.manifest"),
    );
    let pitboard =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pitboard/build/pitboard.exe.manifest");
    embed("pitboard-probe-longpath", &pitboard);
    // It carries asInvoker, as for pitboard.exe, so the linker adds no trustInfo beside it.
    println!("cargo:rustc-link-arg-bin=pitboard-probe-longpath=/MANIFESTUAC:NO");
}

/// Embed `manifest` into the binary named `bin` through the MSVC linker, which merges it
/// with the manifest it makes itself.
fn embed(bin: &str, manifest: &Path) {
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-bin={bin}=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bin={bin}=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
