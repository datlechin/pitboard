//! The sites Pitboard opens in an account's window, and which links from outside it opens.
//!
//! - [`Site`]: each site as values, in [`ALL`]: its hosts, the tool whose accounts it serves,
//!   where its sign-in goes and what its window says about signing in.
//! - [`SiteLink`]: a link from outside checked, or the [`LinkRefusal`] saying why it is not
//!   opened. Only a site's own page over `https`, never its sign-in.
//! - [`pitboard_link`] and [`read_pitboard_link`]: the Pitboard link,
//!   `<scheme>://open?url=<link>`, in which a link reaches the app.
//! - [`WebAddress`]: a page's address as an account window's rules read it, to tell the
//!   site's own pages from the rest.
//!
//! A leaf, with no I/O and nothing of the core: both apps reach it through pitboard-model, and
//! the macOS Share extension, which is sandboxed and links nothing of the core, through
//! pitboard-share-ffi. Each reads a link by the same rule, as the macOS app read one in Swift
//! before: see the `address` module for how and why.

mod address;
mod handoff;
mod link;
mod site;
mod web;

pub use handoff::{pitboard_link, read_pitboard_link};
pub use link::{LinkRefusal, SiteLink, trimmed};
pub use site::{ALL, CHATGPT, CLAUDE, Conjunction, Site, listed};
pub use web::WebAddress;
