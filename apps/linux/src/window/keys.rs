//! The keys the window answers, which the keyboard shortcuts dialog lists: the window's own,
//! bound on its class to its actions, and the two the toolkit answers for it. GtkWindow pops up
//! a primary `GtkMenuButton`'s menu on F10, and `AdwNavigationView` goes back on Alt+Left
//! (docs/facts/linux-app.md, The keyboard shortcuts).

use gtk::gdk;
use pitboard_model::GnomeShortcutWords;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    AddAccount,
    Refresh,
    Preferences,
    KeyboardShortcuts,
    MainMenu,
    Back,
    CloseWindow,
    Quit,
}

impl Shortcut {
    /// Every one, in the order the dialog lists them.
    pub const ALL: [Shortcut; 8] = [
        Shortcut::AddAccount,
        Shortcut::Refresh,
        Shortcut::Preferences,
        Shortcut::KeyboardShortcuts,
        Shortcut::MainMenu,
        Shortcut::Back,
        Shortcut::CloseWindow,
        Shortcut::Quit,
    ];

    pub fn keys(self) -> (gdk::Key, gdk::ModifierType) {
        let control = gdk::ModifierType::CONTROL_MASK;
        match self {
            Shortcut::AddAccount => (gdk::Key::n, control),
            Shortcut::Refresh => (gdk::Key::r, control),
            Shortcut::Preferences => (gdk::Key::comma, control),
            Shortcut::KeyboardShortcuts => (gdk::Key::question, control),
            Shortcut::MainMenu => (gdk::Key::F10, gdk::ModifierType::empty()),
            Shortcut::Back => (gdk::Key::Left, gdk::ModifierType::ALT_MASK),
            Shortcut::CloseWindow => (gdk::Key::w, control),
            Shortcut::Quit => (gdk::Key::q, control),
        }
    }

    /// The window's action its key is bound to, or none where the toolkit answers it.
    pub fn action(self) -> Option<&'static str> {
        match self {
            Shortcut::AddAccount => Some("pitboard.add-account"),
            Shortcut::Refresh => Some("pitboard.refresh"),
            Shortcut::Preferences => Some("pitboard.preferences"),
            Shortcut::KeyboardShortcuts => Some("pitboard.shortcuts"),
            Shortcut::CloseWindow => Some("window.close"),
            Shortcut::Quit => Some("pitboard.quit"),
            Shortcut::MainMenu | Shortcut::Back => None,
        }
    }

    pub fn title(self, words: &GnomeShortcutWords) -> &str {
        match self {
            Shortcut::AddAccount => &words.add_account,
            Shortcut::Refresh => &words.refresh,
            Shortcut::Preferences => &words.preferences,
            Shortcut::KeyboardShortcuts => &words.keyboard_shortcuts,
            Shortcut::MainMenu => &words.main_menu,
            Shortcut::Back => &words.back,
            Shortcut::CloseWindow => &words.close_window,
            Shortcut::Quit => &words.quit,
        }
    }

    /// Its keys as GTK says them on this system: "Ctrl+N".
    pub fn label(self) -> String {
        let (key, modifiers) = self.keys();
        gtk::accelerator_get_label(key, modifiers).into()
    }
}
