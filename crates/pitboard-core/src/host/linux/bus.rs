//! D-Bus on Linux, reached as systemd 262's own tools reach it: the user's systemd as
//! `systemctl --user` does, and the session bus as sd-bus finds it.
//!
//! Read in systemd's `src/shared/bus-util.c` at tag `v262`. `bus_connect_transport_systemd`
//! takes the user manager's private socket, `systemd/private` in `XDG_RUNTIME_DIR`, peer to
//! peer with no `Hello`, and accepts it only where the process listening is this user's or
//! root's (`bus_check_peercred`). It falls back to the session bus only where `XDG_RUNTIME_DIR`
//! is unset and `DBUS_SESSION_BUS_ADDRESS` is set, and a private socket that does not answer
//! is the answer.
//!
//! A build for tests reaches neither: the user's systemd is asked nothing
//! (`systemd::UserManager`), and the only session bus is one a test started and named
//! (`Context::with_test_bus`), never one in this account's own runtime folder.

use crate::context::{Context, SessionBus};
use std::path::{Path, PathBuf};
use zbus::Address;
use zbus::address::transport::{Transport, Unix, UnixSocket};
use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;

// sd-bus's own wait for an answer, `BUS_DEFAULT_TIMEOUT` in `bus-internal.h` at `v262`.
const ANSWER_WITHIN: std::time::Duration = std::time::Duration::from_secs(25);

// How `systemctl --user` reaches the user's systemd.
#[derive(Debug, PartialEq, Eq)]
enum Way {
    Private(PathBuf),
    Session(SessionBus),
}

fn way(ctx: &Context) -> Option<Way> {
    match (ctx.runtime_dir(), ctx.session_bus()) {
        (Some(runtime), _) => Some(Way::Private(runtime.join("systemd").join("private"))),
        (None, Some(bus)) => Some(Way::Session(bus)),
        (None, None) => None,
    }
}

/// A connection to the user's systemd, by the runtime folder and the bus `ctx` names.
pub(super) fn user_manager(ctx: &Context) -> Result<Connection, String> {
    match way(ctx) {
        Some(Way::Private(socket)) => private(&socket),
        Some(Way::Session(bus)) => address(&bus)
            .and_then(Builder::address)
            .and_then(|builder| builder.method_timeout(ANSWER_WITHIN).build())
            .map_err(|e| format!("the session bus did not answer: {e}")),
        None => Err("neither XDG_RUNTIME_DIR nor DBUS_SESSION_BUS_ADDRESS is set".into()),
    }
}

/// A connection to the session's bus, where the Secret Service answers.
pub(super) fn session(ctx: &Context) -> Result<Connection, String> {
    #[cfg(any(test, feature = "test-support"))]
    let address = tests_own(ctx.test_bus())?;
    #[cfg(not(any(test, feature = "test-support")))]
    let address = {
        let bus = ctx
            .session_bus()
            .ok_or("neither DBUS_SESSION_BUS_ADDRESS nor XDG_RUNTIME_DIR is set")?;
        address(&bus).map_err(|e| format!("the session bus's address does not read: {e}"))?
    };
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
    let own = PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() }));
    let resolved = std::fs::canonicalize(socket).unwrap_or_else(|_| socket.clone());
    if crate::host::lies_in(socket, &own) || crate::host::lies_in(&resolved, &own) {
        return Err(refused(&format!("in this account's own {}", own.display())));
    }
    Ok(address)
}

// The session bus as a D-Bus address names it.
fn address(bus: &SessionBus) -> zbus::Result<Address> {
    match bus {
        SessionBus::Address(given) => Address::try_from(given.as_str()),
        SessionBus::Socket(path) => Ok(socket(path)),
    }
}

fn socket(path: &Path) -> Address {
    Address::from(Transport::Unix(Unix::new(UnixSocket::File(
        path.to_path_buf(),
    ))))
}

fn private(path: &Path) -> Result<Connection, String> {
    let connection = Builder::address(socket(path))
        .and_then(|builder| builder.p2p().method_timeout(ANSWER_WITHIN).build())
        .map_err(|e| format!("{} did not answer: {e}", path.display()))?;
    #[allow(
        deprecated,
        reason = "zbus 5.19's blocking `peer_creds` is an async fn; this is its blocking form"
    )]
    let peer = connection
        .peer_credentials()
        .map_err(|e| format!("{} would not say who answers it: {e}", path.display()))?;
    // SAFETY: `geteuid` cannot fail and touches no memory of this process.
    let me = unsafe { libc::geteuid() };
    match peer.unix_user_id() {
        Some(uid) if uid == 0 || uid == me => Ok(connection),
        _ => Err(format!(
            "{} is answered by another user's process",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Environment;
    use crate::host::Os;

    fn read(pairs: &[(&str, &str)]) -> Context {
        Context::read(&Environment::on(Os::Linux, pairs.iter().copied()), "cli")
    }

    // As `bus_connect_transport_systemd` at v262: the session bus only without a runtime folder.
    #[test]
    fn the_user_manager_is_reached_as_systemctl_reaches_it() {
        let address = "unix:path=/tmp/session-bus";
        let both = read(&[
            ("HOME", "/home/x"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("DBUS_SESSION_BUS_ADDRESS", address),
        ]);
        assert_eq!(
            way(&both),
            Some(Way::Private(PathBuf::from(
                "/run/user/1000/systemd/private"
            )))
        );
        let session = read(&[("HOME", "/home/x"), ("DBUS_SESSION_BUS_ADDRESS", address)]);
        assert_eq!(
            way(&session),
            Some(Way::Session(SessionBus::Address(address.into())))
        );
        let neither = read(&[("HOME", "/home/x")]);
        assert_eq!(way(&neither), None);
        let refused = user_manager(&neither).expect_err("nowhere to ask");
        assert!(refused.contains("XDG_RUNTIME_DIR"), "{refused}");
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
        let refused = session(&desktop).expect_err("only a bus a test named");
        assert!(
            refused.contains("a test did not start and name"),
            "{refused}"
        );

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
