//! D-Bus on Linux: the session bus as sd-bus finds it at systemd 262, where the user's systemd
//! and the Secret Service answer.
//!
//! Read in systemd's source at tag `v262`. `sd_bus_open_user` in
//! `src/libsystemd/sd-bus/sd-bus.c` connects as a bus client to `DBUS_SESSION_BUS_ADDRESS`, else
//! to `unix:path=$XDG_RUNTIME_DIR/bus` (`bus_set_address_user`). The user manager joins that bus
//! the same way once its `dbus.socket` and `dbus.service` run, and takes
//! `org.freedesktop.systemd1` there (`bus_init_api` and `bus_setup_api` in `src/core/dbus.c`,
//! `manager_dbus_is_running` in `src/core/manager.c`).
//!
//! Not the manager's private socket, `systemd/private` in the runtime folder, which `systemctl
//! --user` takes: `bus_on_connection` makes `org.freedesktop.systemd1` the sender of each
//! message there, which zbus 5.19 reads only as a unique name and panics on (z-galaxy/zbus#1497),
//! and systemd/systemd#3610 says that socket is not for other programs.
//!
//! A build for tests reaches no bus but one a test started and named (`Context::with_test_bus`),
//! never one in this account's own runtime folder.

use crate::context::{Context, SessionBus};
use std::time::{Duration, Instant};
use zbus::Address;
use zbus::address::transport::{Transport, Unix, UnixSocket};
use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::names::BusName;

// sd-bus's own wait for an answer, `BUS_DEFAULT_TIMEOUT` in `bus-internal.h` at `v262`.
const ANSWER_WITHIN: Duration = Duration::from_secs(25);

const MANAGER: &str = "org.freedesktop.systemd1";

// Socket activation starts the bus before the manager joins it, and dbus-broker v37 refuses at
// once a call to a name nobody owns that no service file names (`driver_forward_unicast` in
// `src/bus/driver.c`): systemd installs none for the session bus. Not measured.
const JOINS_WITHIN: Duration = Duration::from_secs(5);
const LOOKS_EVERY: Duration = Duration::from_millis(50);

/// The session bus, once the user's systemd owns `org.freedesktop.systemd1` on it.
pub(super) fn user_manager(ctx: &Context) -> Result<Connection, String> {
    let connection = session(ctx)?;
    let bus = zbus::blocking::fdo::DBusProxy::new(&connection)
        .map_err(|e| format!("the session bus did not answer: {e}"))?;
    let name = BusName::try_from(MANAGER).map_err(|e| e.to_string())?;
    let given_up = Instant::now() + JOINS_WITHIN;
    while !bus
        .name_has_owner(name.clone())
        .map_err(|e| format!("the session bus did not answer: {e}"))?
    {
        if Instant::now() >= given_up {
            return Err(format!("nothing owns {MANAGER} on the session bus"));
        }
        std::thread::sleep(LOOKS_EVERY);
    }
    Ok(connection)
}

/// A connection to the session's bus, where the Secret Service and the user's systemd answer.
pub(super) fn session(ctx: &Context) -> Result<Connection, String> {
    #[cfg(any(test, feature = "test-support"))]
    let address = tests_own(ctx.test_bus())?;
    #[cfg(not(any(test, feature = "test-support")))]
    let address = named(ctx)?;
    Builder::address(address)
        .and_then(|builder| builder.method_timeout(ANSWER_WITHIN).build())
        .map_err(|e| format!("the session bus did not answer: {e}"))
}

// A socket a test made in a folder of its own, as `testing::SecretService` makes one.
#[cfg(any(test, feature = "test-support"))]
fn tests_own(given: Option<&str>) -> Result<Address, String> {
    let refused = |why: &str| format!("a build for tests connects to no bus {why}");
    let given = given.ok_or_else(|| refused("a test did not start and name"))?;
    let address = Address::try_from(given).map_err(|e| refused(&format!("named {given}: {e}")))?;
    let Transport::Unix(unix) = address.transport() else {
        return Err(refused("but a socket in a test's own folder"));
    };
    let UnixSocket::File(socket) = unix.path() else {
        return Err(refused("but a socket in a test's own folder"));
    };
    // SAFETY: `geteuid` cannot fail and touches no memory of this process.
    let own = std::path::PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() }));
    let resolved = std::fs::canonicalize(socket).unwrap_or_else(|_| socket.clone());
    if crate::host::lies_in(socket, &own) || crate::host::lies_in(&resolved, &own) {
        return Err(refused(&format!("in this account's own {}", own.display())));
    }
    Ok(address)
}

// The session bus `ctx` names, as a D-Bus address.
#[cfg_attr(
    any(test, feature = "test-support"),
    allow(
        dead_code,
        reason = "a build for tests reaches only the bus a test named"
    )
)]
fn named(ctx: &Context) -> Result<Address, String> {
    let bus = ctx
        .session_bus()
        .ok_or("neither DBUS_SESSION_BUS_ADDRESS nor XDG_RUNTIME_DIR is set")?;
    match bus {
        SessionBus::Address(given) => Address::try_from(given.as_str()),
        SessionBus::Socket(path) => Ok(Address::from(Transport::Unix(Unix::new(
            UnixSocket::File(path),
        )))),
    }
    .map_err(|e| format!("the session bus's address does not read: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Environment;
    use crate::host::Os;

    fn read(pairs: &[(&str, &str)]) -> Context {
        Context::read(&Environment::on(Os::Linux, pairs.iter().copied()), "cli")
    }

    // Not the session a person runs the tests in, even where its variables name the test's bus.
    #[test]
    fn a_build_for_tests_connects_to_no_bus_it_was_not_given() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let given = crate::secret_service_testing::SecretService::without_a_provider(
            &scratch.path().join("bus"),
        )
        .unwrap_or_else(|missing| panic!("{missing}"));
        let runtime = scratch
            .path()
            .join("runtime")
            .to_string_lossy()
            .into_owned();
        let desktop = read(&[
            ("HOME", "/home/x"),
            ("XDG_RUNTIME_DIR", &runtime),
            ("DBUS_SESSION_BUS_ADDRESS", given.address()),
        ]);
        for reach in [session, user_manager] {
            let refused = reach(&desktop).expect_err("only a bus a test named");
            assert!(
                refused.contains("a test did not start and name"),
                "{refused}"
            );
        }

        // SAFETY: `geteuid` cannot fail and touches no memory of this process.
        let uid = unsafe { libc::geteuid() };
        for named in [
            format!("unix:path=/run/user/{uid}/bus"),
            format!("unix:path=/run/user/{uid}/pitboard/bus"),
            "unix:abstract=pitboard-test".to_owned(),
            "tcp:host=127.0.0.1,port=9".to_owned(),
        ] {
            let ctx = read(&[("HOME", "/home/x")]).with_test_bus(named.clone());
            let refused = session(&ctx).expect_err(&named);
            assert!(
                refused.starts_with("a build for tests connects to no bus"),
                "{named}: {refused}"
            );
        }

        let ctx = read(&[("HOME", "/home/x")]).with_test_bus(given.address().to_owned());
        let reached = session(&ctx).expect("the bus the test started");
        assert!(
            reached.unique_name().is_some(),
            "a bus gives each connection a name"
        );
    }
}
