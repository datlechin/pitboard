//! The libraries the macOS and Windows apps link, the static and the dynamic one, holding
//! `pitboard-model` and everything it exports.
//!
//! A crate of its own because Cargo builds a dependency as every crate type it names
//! (rust-lang/cargo#11232), and the Linux app links the model as a Rust library, which needs
//! neither of these (docs/facts/bindings.md).

pub use pitboard_model::*;
