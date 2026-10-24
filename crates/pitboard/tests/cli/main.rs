//! The command line's tests, run as a person or a tool runs `pitboard`: one test binary, so
//! the harness in `common` and the core it stands on are compiled and linked once.

mod codex_round_trip;
mod common;
mod contract;
mod crash_recovery;
mod credman_write_is_harmless;
mod desktop_round_trip;
mod environment_semantics;
mod harness;
mod inherited_environment;
mod keychain_write_is_harmless;
mod proxy;
mod switch_round_trip;
mod version_resource;
mod warm;
mod watch;
mod windows_refuses;
mod windows_terminal;
