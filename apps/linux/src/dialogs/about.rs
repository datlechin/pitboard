//! About Pitboard, libadwaita's about dialog made from the app's metainfo, as libadwaita reads
//! it: the name, the developer, the licence, and the website, the docs and where to report an
//! issue (docs/facts/linux-app.md, About). The icon and the version are this build's, and the
//! source code's link and the copyright are the model's words.

use crate::{config, resources};

/// Where the source code is, as the metainfo's `vcs-browser` names it.
pub const SOURCE_CODE: &str = "https://github.com/datlechin/pitboard";

pub fn about() -> adw::AboutDialog {
    resources::register();
    let words = pitboard_model::app_words(pitboard_model::Front::Gnome).settings;
    let about = adw::AboutDialog::from_appdata(
        &format!("{}/com.usepitboard.Pitboard.metainfo.xml", resources::BASE),
        None,
    );
    about.set_application_icon(config::APP_ID);
    about.set_version(env!("CARGO_PKG_VERSION"));
    about.set_copyright(&words.copyright);
    about.add_link(&words.source_code, SOURCE_CODE);
    about
}
