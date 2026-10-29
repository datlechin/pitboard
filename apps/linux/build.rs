// The program's GResource: its icons, the D-Bus interfaces it serves, its views and its metainfo,
// with the releases CHANGELOG.md holds, written by the script Meson installs the metainfo with;
// and in the debug build alone, its own icon, so a release names nothing of the debug build's.
// Only a build for Linux on Linux makes them, the one build that links GTK.

#[cfg(target_os = "linux")]
fn main() {
    use std::path::PathBuf;
    use std::process::Command;

    // A build script reads Cargo's variables from its environment: there is no other way in.
    #[allow(clippy::disallowed_methods)]
    let (target, out, build) = (
        std::env::var("CARGO_CFG_TARGET_OS"),
        std::env::var_os("OUT_DIR"),
        std::env::var("PITBOARD_BUILD"),
    );
    println!("cargo::rustc-check-cfg=cfg(pitboard_debug_build)");
    println!("cargo::rerun-if-env-changed=PITBOARD_BUILD");
    if target.as_deref() != Ok("linux") {
        return;
    }
    let out = PathBuf::from(out.expect("Cargo names OUT_DIR"));
    let (script, template, changelog) = (
        "data/metainfo-releases.py",
        "data/com.usepitboard.Pitboard.metainfo.xml.in",
        "../../CHANGELOG.md",
    );
    let made = Command::new("python3")
        .args([script, template, changelog])
        .output()
        .expect("python3 runs data/metainfo-releases.py");
    assert!(
        made.status.success(),
        "{script} refused {changelog}: {}",
        String::from_utf8_lossy(&made.stderr)
    );
    // Written only when it changed: Cargo takes a file newer than this script's last run as a
    // reason to run it again, and compile_resources names it.
    let metainfo = out.join("com.usepitboard.Pitboard.metainfo.xml");
    if std::fs::read(&metainfo).ok().as_ref() != Some(&made.stdout) {
        std::fs::write(&metainfo, &made.stdout).expect("OUT_DIR takes the metainfo");
    }
    for read in [script, template, changelog] {
        println!("cargo::rerun-if-changed={read}");
    }
    glib_build_tools::compile_resources(
        &[PathBuf::from("data"), out],
        "data/resources.gresource.xml",
        "pitboard.gresource",
    );
    // As `config::Build` reads it: only Meson's release is the release.
    if build.as_deref() != Ok("release") {
        glib_build_tools::compile_resources(
            &["data"],
            "data/devel.gresource.xml",
            "devel.gresource",
        );
        println!("cargo::rustc-cfg=pitboard_debug_build");
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}
