#[path = "../pitboard/build/versioninfo.rs"]
mod versioninfo;

fn main() {
    versioninfo::manifest(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("build/pitboardw.exe.manifest"),
    );
    versioninfo::embed(
        versioninfo::Kind::Program,
        "pitboardw",
        "Pitboard command line for scheduled tasks",
    );
}
