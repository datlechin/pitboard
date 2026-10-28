//! The program's GResource, which `build.rs` compiles in: its icons, its metainfo, and the D-Bus
//! interfaces it serves, each written from its published definition in `data/dbus`.

use std::sync::Once;

/// Where the resource keeps its files, for both builds.
pub const BASE: &str = "/com/usepitboard/Pitboard";

/// Registers the resource for the process, once whoever asks.
pub fn register() {
    static REGISTERED: Once = Once::new();
    REGISTERED.call_once(|| {
        gio::resources_register_include!("pitboard.gresource")
            .expect("build.rs compiles the resource into the program");
    });
}

/// The interface `name`, as `data/dbus/<name>.xml` declares it.
pub fn interface(name: &str) -> gio::DBusInterfaceInfo {
    register();
    let path = format!("{BASE}/dbus/{name}.xml");
    let xml = gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE)
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    let xml = std::str::from_utf8(&xml).unwrap_or_else(|e| panic!("{path}: {e}"));
    gio::DBusNodeInfo::for_xml(xml)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .lookup_interface(name)
        .unwrap_or_else(|| panic!("{path} declares no {name}"))
}
