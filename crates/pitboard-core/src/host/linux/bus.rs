//! D-Bus on Linux, reached as systemd 262's own tools reach it: the user's systemd as
//! `systemctl --user` does, and the session bus as sd-bus finds it.
//!
//! Read in systemd's `src/shared/bus-util.c` at tag `v262`. `bus_connect_transport_systemd`
//! takes the user manager's private socket, `systemd/private` in `XDG_RUNTIME_DIR`, peer to
//! peer with no `Hello`, and accepts it only where the process listening is this user's or
//! root's (`bus_check_peercred`). It falls back to the session bus only where `XDG_RUNTIME_DIR`
//! is unset and `DBUS_SESSION_BUS_ADDRESS` is set, and a private socket that does not answer
//! is the answer.

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
}
