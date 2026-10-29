//! Pitboard's app for GNOME and KDE Plasma: GTK 4 and libadwaita glue over the app model.
//!
//! The glue holds no rule of the model's. It shows what a snapshot says, sends what was asked
//! of it, and speaks the desktop's services for the model. What a build is, and the names it
//! answers to, are [`config`]'s; whether a launch starts anything is [`launch`]'s; the one
//! GApplication the primary instance runs is [`application`]'s; the desktop's services, and a
//! fixture's stand-ins for them, are [`platform`]'s; what the program carries in its GResource
//! is [`resources`]'s.
//!
//! The model's snapshots reach the main loop through [`listener`] and are kept by part in
//! [`shown`], which [`window`], its [`pages`] and its [`dialogs`], [`preferences`] among them,
//! [`presence`], the status item and the searches, and [`widget_feed`], what a Plasma widget
//! reads, follow. The app's only actions on the bus are [`outside`]'s. The model's notices are
//! posted by [`notices`], its clock is [`local_time`]'s, and the app's own folder is
//! [`data_folder`]'s.
#![cfg(target_os = "linux")]

pub mod application;
pub mod config;
pub mod data_folder;
pub mod dialogs;
pub mod launch;
pub mod listener;
pub mod local_time;
pub mod notices;
pub mod outside;
pub mod pages;
pub mod platform;
pub mod preferences;
pub mod presence;
pub mod resources;
pub mod shown;
#[cfg(test)]
mod testing;
pub mod widget_feed;
pub mod window;

pub use launch::run;
