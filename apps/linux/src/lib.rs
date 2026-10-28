//! Pitboard's app for GNOME and KDE Plasma: GTK 4 and libadwaita glue over the app model.
//!
//! The glue holds no rule of the model's. It shows what a snapshot says, sends what was asked
//! of it, and speaks the desktop's services for the model. What a build is, and the names it
//! answers to, are [`config`]'s; whether a launch starts anything is [`launch`]'s; the
//! desktop's services, and a fixture's stand-ins for them, are [`platform`]'s.
#![cfg(target_os = "linux")]

pub mod config;
pub mod launch;
pub mod platform;

pub use launch::run;
