//! The window's pages beside the accounts, each pushed over them with a header bar of its own:
//! Diagnostics, doctor's checks ([`diagnostics`]), and Activity, every change Pitboard made
//! ([`activity`]). Each asks the model to read what it shows again every time it is shown, and as
//! its own button asks, since an app runs for days and a terminal changes things meanwhile.

pub mod activity;
pub mod diagnostics;

pub use activity::ActivityPage;
pub use diagnostics::DiagnosticsPage;
