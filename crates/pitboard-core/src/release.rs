//! Whether this build of Pitboard may do anything on the system it was built for.
//!
//! Pitboard for Windows reaches people as 1.0.0, once its command line, its app, their
//! installers and their docs are done. Until then every release is a 0.x one, and from the
//! first of them that compiles on Windows, every one publishes Windows code to crates.io that
//! is not finished. So a Windows build of a 0.x release refuses every command but
//! `--version`, `--help`, `completions` and `manpage`, says that Pitboard for Windows is not
//! released yet, and changes nothing. macOS and Linux are never refused here.
//!
//! Every change asks it before anything else, in the one gate every change passes
//! ([`crate::permit::Permit`]), so no file is written, no store of logins changed and no
//! token renewed by a caller that forgot to ask. Every read of Pitboard's account list asks
//! it too, so an app on a refused build is refused at its first read. The command line asks
//! it before every command but those four.

use crate::error::{Error, Result};
use crate::host::{OS, Os};
use std::sync::OnceLock;

/// Whether this build was opened to Windows before Windows is released: a build compiled
/// with `--cfg pitboard_unreleased_windows` in `RUSTFLAGS`, which CI's Windows jobs set for
/// their lints and tests and a person working on Pitboard on Windows sets the same way, or a
/// unit test of this crate, which is never installed.
///
/// Not a Cargo feature. crates.io lists a published crate's features, and anybody could turn
/// one on from there, as `cargo install pitboard --features pitboard-core/<feature>`, and
/// build a Windows Pitboard that is half written. Nor does `test-support` open it, for the
/// same reason: it is a feature of the published crate too. A `cfg` comes from whoever runs
/// the compiler, as a change to the source would, and no index offers one.
pub(crate) const OPENED: bool = cfg!(any(test, pitboard_unreleased_windows));

// pitboard-core has a version line of its own, so the gate reads Pitboard's.
static VERSION: OnceLock<&'static str> = OnceLock::new();

pub fn set_version(version: &'static str) {
    let _ = VERSION.set(version);
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn version() -> Option<&'static str> {
    VERSION.get().copied()
}

pub(crate) fn pitboard_version() -> &'static str {
    VERSION.get().copied().unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// What the gate decides of a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// It may do what it is asked.
    Runs,
    /// It is a Windows build of a release before Pitboard for Windows is released, and does
    /// nothing.
    NotReleased,
}

/// What the gate decides of a build for `os`, of `version`, opened to Windows before its
/// release or not ([`OPENED`]). A version whose major number cannot be read is taken for a
/// 0.x one, which refuses on Windows.
pub(crate) fn decision(os: Os, version: &str, opened: bool) -> Decision {
    let major = version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u64>().ok())
        .unwrap_or(0);
    match os {
        Os::MacOs | Os::Linux => Decision::Runs,
        Os::Windows if major >= 1 || opened => Decision::Runs,
        Os::Windows => Decision::NotReleased,
    }
}

// A process that names no version is taken for a 0.x one.
fn decided(os: Os, opened: bool) -> Decision {
    decision(os, VERSION.get().copied().unwrap_or_default(), opened)
}

/// Whether this build may do anything here: `windows_not_released` where it is a Windows
/// build of a 0.x release that was not opened to Windows.
pub fn check() -> Result<()> {
    match decided(OS, OPENED) {
        Decision::Runs => Ok(()),
        Decision::NotReleased => Err(Error::WindowsNotReleased),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Windows build of a 0.x release refuses unless it was opened to Windows. Version 1,
    /// its betas included, runs, and so does every build for macOS and Linux, whatever its
    /// version.
    #[test]
    fn only_a_windows_build_of_a_release_before_1_0_refuses() {
        for version in ["0.7.0", "0.8.0", "0.99.1", "not a version", ""] {
            assert_eq!(
                decision(Os::Windows, version, false),
                Decision::NotReleased,
                "{version}"
            );
            assert_eq!(
                decision(Os::Windows, version, true),
                Decision::Runs,
                "{version}"
            );
        }
        for version in ["1.0.0-beta.1", "1.0.0-beta.12", "1.0.0", "1.4.2", "2.0.0"] {
            for opened in [false, true] {
                assert_eq!(
                    decision(Os::Windows, version, opened),
                    Decision::Runs,
                    "{version}, opened {opened}"
                );
            }
        }
        for os in [Os::MacOs, Os::Linux] {
            for version in ["0.7.0", "0.8.0", "1.0.0-beta.1", "1.0.0", "not a version"] {
                for opened in [false, true] {
                    assert_eq!(
                        decision(os, version, opened),
                        Decision::Runs,
                        "{os:?}, {version}, opened {opened}"
                    );
                }
            }
        }
    }

    /// What this build decides is the decision for its own system and version, and a unit
    /// test of the core is opened to Windows: it is never installed, and the Windows face's
    /// own tests run through the gate. Refused, it is said in the words and the code that
    /// stay after 1.0.0, where they never occur.
    #[test]
    fn this_build_answers_for_its_own_system_and_version() {
        const { assert!(OPENED) };
        assert_eq!(check().is_ok(), decided(OS, OPENED) == Decision::Runs);
        assert!(check().is_ok());
        let refused = Error::WindowsNotReleased;
        assert_eq!(refused.code(), "windows_not_released");
        assert_eq!(
            refused.to_string(),
            "Pitboard for Windows is not released yet. This build changes nothing."
        );
        assert_eq!(refused.exit_code(), 1);
    }

    fn user_agent_sent() -> String {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port on loopback");
        let url = format!("http://{}/", listener.local_addr().expect("its address"));
        let host = std::thread::spawn(move || {
            let (mut client, _) = listener.accept().expect("a request");
            let head = BufReader::new(client.try_clone().expect("a second handle"));
            let agent = head
                .lines()
                .map_while(std::result::Result::ok)
                .take_while(|line| !line.is_empty())
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("user-agent")
                        .then(|| value.trim().to_owned())
                });
            let _ = client.write_all(
                b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            agent
        });
        let ctx = crate::context::Context::for_unit_test();
        crate::api::agent(&ctx).get(&url).call().expect("an answer");
        host.join().expect("the host").expect("a User-Agent")
    }

    // The version is set once per process, so this is the one test that sets it.
    #[test]
    fn the_gate_and_every_request_read_the_version_the_front_end_names() {
        assert_eq!(decided(Os::Windows, false), Decision::NotReleased);
        assert_eq!(
            user_agent_sent(),
            concat!("pitboard/", env!("CARGO_PKG_VERSION"))
        );
        set_version("1.0.0-beta.1");
        set_version("0.15.0");
        assert_eq!(version(), Some("1.0.0-beta.1"));
        assert_eq!(decided(Os::Windows, false), Decision::Runs);
        assert_eq!(user_agent_sent(), "pitboard/1.0.0-beta.1");
    }
}
