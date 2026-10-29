//! The words the UI tests look for, each the model's, and the launcher's, read as the desktop
//! reads its file.

use std::path::Path;

use pitboard_model::{AppWords, Front};

pub(crate) fn app() -> AppWords {
    pitboard_model::app_words(Front::Gnome)
}

/// Each action the launcher offers, by the app's action it activates, with its name there.
pub(crate) fn launcher_actions() -> Vec<(String, String)> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("data/com.usepitboard.Pitboard.desktop.in");
    let launcher = glib::KeyFile::new();
    launcher
        .load_from_file(&path, glib::KeyFileFlags::NONE)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let actions = launcher.string_list("Desktop Entry", "Actions").unwrap();
    (actions.iter())
        .map(|action| {
            let name = launcher
                .string(&format!("Desktop Action {action}"), "Name")
                .unwrap();
            (action.to_string(), name.to_string())
        })
        .collect()
}
