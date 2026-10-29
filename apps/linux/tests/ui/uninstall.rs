//! Uninstall: the model's question, then the model removes Pitboard, and the app takes back what
//! is its own in the person's home, its start at sign-in and its folders, and says how the
//! package manager removes the app in an alert whose one button quits it.

use adw::prelude::*;
use pitboard_linux::config;
use pitboard_linux::dialogs::question::CANCEL;
use pitboard_linux::platform::autostart::StartAtSignIn;
use pitboard_linux::platform::stand_in;
use pitboard_linux::preferences::Preferences;
use pitboard_linux::preferences::advanced::{QUIT, removal_alert};
use pitboard_model::{Intent, Package, PackageRemoval, RemovalShown};

use super::views::{Viewed, button, every};

#[gtk::test]
fn uninstall_removes_the_apps_leftovers_and_names_the_package_command() {
    let viewed = Viewed::of("oneTool");
    stand_in::Autostart::new(&viewed.folder).turn_on().unwrap();
    let own = [viewed.folder.join("data"), viewed.folder.join("cache")];
    for folder in &own {
        std::fs::create_dir_all(folder).unwrap();
        std::fs::write(folder.join("kept"), "kept").unwrap();
    }
    viewed
        .window
        .activate_action("pitboard.preferences", None)
        .unwrap();
    let preferences = viewed.dialog().downcast::<Preferences>().unwrap();
    preferences.set_visible_page_name("advanced");
    let offer = viewed.shown.snapshot().uninstall.clone();
    let question = offer.confirm.clone().expect("Uninstall asks first");
    let uninstall = (every::<adw::ButtonRow>(&preferences).into_iter())
        .find(|row| row.title() == offer.title)
        .expect("Uninstall Pitboard…");
    viewed.sent_since();

    uninstall.emit_by_name::<()>("activated", &[]);
    let asked = viewed
        .dialog()
        .downcast::<adw::AlertDialog>()
        .expect("its question");
    assert_eq!(asked.heading().as_deref(), Some(question.title.as_str()));
    assert_eq!(
        asked.default_response().as_deref(),
        Some(CANCEL),
        "read before it is answered"
    );
    button(&asked, &question.confirm).emit_clicked();
    viewed.until("Pitboard removed", |shown| {
        matches!(
            shown.removal,
            Some(RemovalShown::Done | RemovalShown::Stopped { .. })
        )
    });
    assert_eq!(viewed.shown.snapshot().removal, Some(RemovalShown::Done));
    assert_eq!(viewed.sent_since(), [Intent::Uninstall]);

    let said = viewed
        .dialog()
        .downcast::<adw::AlertDialog>()
        .expect("what the package manager removes");
    let removal = pitboard_model::package_removal(config::PACKAGE, &[]);
    assert_eq!(said.heading().as_deref(), Some(removal.title.as_str()));
    assert_eq!(said.body(), removal.message);
    assert_eq!(
        viewed.started_at_sign_in().last(),
        Some(&serde_json::json!({ "on": false }))
    );
    for folder in &own {
        assert!(!folder.exists(), "{}", folder.display());
    }
    assert!(!viewed.asked.quit.get());

    said.activate_action("default.activate", None).unwrap();
    viewed.until("its one button quits the app", |_| viewed.asked.quit.get());
}

// A Cargo build names no package, so a package's alert is drawn from its own removal.
#[gtk::test]
fn the_package_command_is_shown_beside_its_copy_button() {
    adw::init().unwrap();
    let removal: PackageRemoval = pitboard_model::package_removal(Package::Rpm, &[]);

    let alert = removal_alert(&removal);

    let commands = alert.extra_child().expect("the command, under the message");
    let rows = every::<adw::ActionRow>(&commands);
    let titles: Vec<String> = rows.iter().map(|row| row.title().to_string()).collect();
    assert_eq!(titles, removal.command.into_iter().collect::<Vec<_>>());
    let copy = pitboard_model::gnome_words().copy_command;
    let copies = every::<gtk::Button>(&rows[0])
        .into_iter()
        .find(|button| button.tooltip_text().as_deref() == Some(copy.as_str()))
        .expect("its copy button");
    copies.emit_clicked();
    let copied = glib::MainContext::default()
        .block_on(copies.clipboard().read_text_future())
        .unwrap();
    assert_eq!(copied.as_deref(), Some(rows[0].title().as_str()));
    assert_eq!(alert.default_response().as_deref(), Some(QUIT));
    assert_eq!(alert.close_response(), QUIT);
}
