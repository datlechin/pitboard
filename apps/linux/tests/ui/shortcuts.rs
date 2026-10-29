//! Keyboard Shortcuts lists every key the window answers, as GTK reads them off the window: the
//! keys bound on its class to its actions, F10 for its primary menu, and Alt+Left for its
//! navigation view, each with the model's words and its keys as GTK says them.

use adw::prelude::*;
use pitboard_linux::dialogs::shortcuts::Shortcuts;
use pitboard_linux::window::Shortcut;

use super::views::{Viewed, every};

// Every key a shortcut controller of `widget` answers with a keyval, as GTK labels it, and the
// window's action it activates, where it names one.
fn answered(widget: &impl IsA<gtk::Widget>) -> Vec<(String, Option<String>)> {
    let controllers = widget.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|i| {
            controllers
                .item(i)
                .and_downcast::<gtk::ShortcutController>()
        })
        .flat_map(|shortcuts| {
            (0..shortcuts.n_items())
                .filter_map(|i| shortcuts.item(i).and_downcast::<gtk::Shortcut>())
                .collect::<Vec<_>>()
        })
        .filter_map(|shortcut| {
            let trigger = shortcut.trigger().and_downcast::<gtk::KeyvalTrigger>()?;
            let action = (shortcut.action().and_downcast::<gtk::NamedAction>())
                .map(|action| action.action_name().to_string());
            Some((
                gtk::accelerator_get_label(trigger.keyval(), trigger.modifiers()).to_string(),
                action,
            ))
        })
        .collect()
}

#[gtk::test]
fn every_shortcut_the_window_answers_is_listed_with_its_keys() {
    let viewed = Viewed::of("oneTool");
    let window = &viewed.window;
    // The window's own: each of its class's keys that activates one of its actions.
    let mut keys: Vec<String> = (answered(window).into_iter())
        .filter(|(_, action)| {
            action
                .as_deref()
                .is_some_and(|action| action.starts_with("pitboard.") || action == "window.close")
        })
        .map(|(keys, _)| keys)
        .collect();
    // GtkWindow's F10, which pops up a primary menu button's menu.
    let f10 = gtk::accelerator_get_label(gtk::gdk::Key::F10, gtk::gdk::ModifierType::empty());
    assert!(answered(window).iter().any(|(keys, _)| *keys == f10));
    assert!(
        every::<gtk::MenuButton>(window)
            .iter()
            .any(|menu| menu.is_primary())
    );
    keys.push(f10.into());
    // The navigation view's Alt+Left, which goes back.
    let navigation = every::<adw::NavigationView>(window).remove(0);
    let back = gtk::accelerator_get_label(gtk::gdk::Key::Left, gtk::gdk::ModifierType::ALT_MASK);
    assert!(answered(&navigation).iter().any(|(keys, _)| *keys == back));
    keys.push(back.into());

    window
        .activate_action("pitboard.shortcuts", None)
        .expect("the main menu's Keyboard Shortcuts");
    let dialog = viewed
        .dialog()
        .downcast::<Shortcuts>()
        .expect("Keyboard Shortcuts");

    let words = pitboard_model::gnome_words().shortcuts;
    let listed: Vec<(String, String)> = (every::<adw::ActionRow>(&dialog).iter())
        .map(|row| {
            let said = every::<gtk::Label>(row)
                .into_iter()
                .map(|label| label.label().to_string())
                .find(|said| keys.contains(said))
                .unwrap_or_default();
            (row.title().to_string(), said)
        })
        .collect();
    let expected: Vec<(String, String)> = (Shortcut::ALL.iter())
        .map(|shortcut| (shortcut.title(&words).to_owned(), shortcut.label()))
        .collect();
    assert_eq!(listed, expected);
    let mut said: Vec<&String> = listed.iter().map(|(_, keys)| keys).collect();
    let mut answered: Vec<&String> = keys.iter().collect();
    said.sort();
    answered.sort();
    assert_eq!(said, answered, "each key the window answers, and no other");
}
