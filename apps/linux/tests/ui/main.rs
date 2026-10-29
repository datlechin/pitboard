//! The UI tests: the built program, launched into a fixture with homes of its own on the private
//! bus and the headless display `headless.sh` gives, driven over D-Bus as the desktop drives it
//! and over AT-SPI as a screen reader's user would. One at a time (`.config/nextest.toml`), since
//! each owns the debug build's name on the one bus. GTK 4.22 gives AT-SPI the ids they find a
//! widget by, so they run on Ubuntu 26.04 (`check.sh ui`).
#![cfg(target_os = "linux")]

mod accounts;
mod actions;
mod harness;
mod launch;
mod lifecycle;
mod open_link;
mod preferences;
mod questions;
mod sign_in;
mod uninstall;
mod views;
mod words;
