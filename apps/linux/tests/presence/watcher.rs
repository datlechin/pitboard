//! A stand-in for a tray's StatusNotifierWatcher, on a connection of its own. It takes an item
//! only where the item's name has an owner, as Plasma 6.3.6's watcher does
//! (`statusnotifierwatcher/statusnotifierwatcher.cpp:39-68`), and comes and goes with its name.

use std::cell::RefCell;
use std::rc::Rc;

use gio::prelude::*;

use crate::bus;

pub(crate) const NAME: &str = "org.kde.StatusNotifierWatcher";

// The one method an item calls, from KStatusNotifierItem 6.13.0's
// src/org.kde.StatusNotifierWatcher.xml.
const XML: &str = r#"
<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <method name="RegisterStatusNotifierItem">
      <arg name="service" type="s" direction="in"/>
    </method>
  </interface>
</node>"#;

pub(crate) struct Watcher {
    connection: gio::DBusConnection,
    registered: Rc<RefCell<Vec<String>>>,
    registration: Option<gio::RegistrationId>,
}

impl Watcher {
    pub(crate) fn new(connection: gio::DBusConnection) -> Watcher {
        let interface = gio::DBusNodeInfo::for_xml(XML)
            .unwrap()
            .lookup_interface(NAME)
            .unwrap();
        let registered = Rc::new(RefCell::new(Vec::new()));
        let registration = connection
            .register_object("/StatusNotifierWatcher", &interface)
            .method_call({
                let registered = registered.clone();
                move |connection, _, _, _, _, parameters, invocation| {
                    let (service,) = parameters.get::<(String,)>().unwrap();
                    if bus(
                        &connection,
                        "NameHasOwner",
                        (service.as_str(),).to_variant(),
                    ) == (true,).to_variant()
                    {
                        registered.borrow_mut().push(service);
                    }
                    invocation.return_value(None);
                }
            })
            .build()
            .unwrap();
        Watcher {
            connection,
            registered,
            registration: Some(registration),
        }
    }

    pub(crate) fn appear(&self) {
        let reply = bus(&self.connection, "RequestName", (NAME, 4u32).to_variant());
        assert_eq!(reply, (1u32,).to_variant(), "{NAME} was not owned");
    }

    pub(crate) fn leave(&self) {
        let reply = bus(&self.connection, "ReleaseName", (NAME,).to_variant());
        assert_eq!(reply, (1u32,).to_variant(), "{NAME} was not released");
    }

    /// Each name an item registered by, in order.
    pub(crate) fn registered(&self) -> Vec<String> {
        self.registered.borrow().clone()
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if let Some(registration) = self.registration.take() {
            self.connection.unregister_object(registration).unwrap();
        }
    }
}
