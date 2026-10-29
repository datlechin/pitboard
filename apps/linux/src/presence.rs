//! What the app shows outside its window, from what the model shows: the status item and its
//! menu, where a tray shows one, and what GNOME's search and KRunner find, from the model's quick
//! actions. Whether a tray shows the item is the model's to hear, as `SurfacesChanged`, so it
//! words a status item only while one is shown.
//!
//! The item is shown from the first snapshot, since a tray finds it only once it is on the bus:
//! until the model words it, it shows the app's mark and its menu.

use std::cell::RefCell;
use std::collections::HashMap;

use pitboard_model::{
    Front, Intent, MenuEntry, MenuWords, Place, QuickAction, QuickAsk, SettingsSection, Snapshot,
    StatusItemShown, UsageLevel,
};

use crate::config;
use crate::platform::Failed;
use crate::platform::search::Found;
use crate::platform::tray::{Status, StatusItem, ToolTip, TrayEntry, TrayItem};

/// Pitboard's documentation, which Help opens, as the Windows app's does.
pub const HELP: &str = "https://docs.usepitboard.com";

/// What choosing an entry of the tray's menu or a search's result does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    Send(Intent),
    /// A switch asked from outside, by the account's id in the widget feed.
    Switch(String),
    AddAccount,
    /// The browser opens it.
    Link(String),
    Quit,
}

pub struct Presence {
    item: Box<dyn StatusItem>,
    words: MenuWords,
    picked: RefCell<HashMap<String, Picked>>,
    // the last list the model gave, kept while it gives none
    quick: RefCell<Vec<QuickAction>>,
}

impl Presence {
    pub fn new(item: Box<dyn StatusItem>) -> Presence {
        Presence {
            item,
            words: pitboard_model::app_words(Front::Gnome).menu,
            picked: RefCell::default(),
            quick: RefCell::default(),
        }
    }

    /// Shows the item as `snapshot` has it: what it says of itself, and its menu.
    pub fn show(&self, snapshot: &Snapshot) -> Result<(), Failed> {
        let (item, picked) = tray(snapshot, &self.words);
        self.picked.replace(picked.into_iter().collect());
        self.item.show(&item)
    }

    /// What the menu's entry `id` does, for an entry the item last showed.
    pub fn picked(&self, id: &str) -> Option<Picked> {
        self.picked.borrow().get(id).cloned()
    }

    pub fn follow_quick_actions(&self, snapshot: &Snapshot) {
        if let Some(actions) = &snapshot.quick_actions {
            self.quick.replace(actions.clone());
        }
    }

    /// What a search of `terms` finds.
    pub fn found(&self, terms: &[String]) -> Vec<Found> {
        found(&self.quick.borrow(), terms)
    }
}

/// The item for `snapshot`, and what each entry of its menu does, by the entry's id.
pub fn tray(snapshot: &Snapshot, words: &MenuWords) -> (TrayItem, Vec<(String, Picked)>) {
    let mut picked = Vec::new();
    let mut item = |id: String, label: &str, enabled: bool, radio: Option<bool>, does| {
        if let Some(does) = does {
            picked.push((id.clone(), does));
        }
        TrayEntry::Item {
            id,
            label: label.to_owned(),
            enabled,
            radio,
        }
    };
    let mut groups: Vec<Vec<TrayEntry>> = Vec::new();
    groups.push(vec![item(
        "open".into(),
        &words.open_pitboard,
        true,
        None,
        Some(Picked::Send(Intent::Show { place: None })),
    )]);
    let notices = &snapshot.menu_notices;
    groups.push(
        (notices.install.iter())
            .chain(&notices.switches)
            .chain(&notices.sign_ins)
            .chain(&notices.others)
            .enumerate()
            .map(|(n, entry)| {
                let does = entry_does(entry);
                item(
                    format!("notice/{n}"),
                    &entry.title,
                    entry.enabled && does.is_some(),
                    None,
                    does,
                )
            })
            .collect(),
    );
    let mut accounts = Vec::new();
    for section in &snapshot.sections {
        if let Some(heading) = &section.heading {
            accounts.push(item(
                format!("tool/{}", section.id),
                heading,
                false,
                None,
                None,
            ));
        }
        for account in &section.accounts {
            let does = (account.action.as_ref()).map(|action| Picked::Send(action.intent.clone()));
            let enabled = account.in_use || does.is_some();
            accounts.push(item(
                format!("account/{}", account.id),
                &account.menu_title,
                enabled,
                Some(account.in_use),
                does,
            ));
        }
    }
    if let Some(note) = &snapshot.menu_accounts_note {
        accounts.push(item("accounts-note".into(), note, false, None, None));
    }
    groups.push(accounts);
    groups.push(vec![
        item(
            "add-account".into(),
            &words.add_account,
            true,
            None,
            Some(Picked::AddAccount),
        ),
        item(
            "refresh".into(),
            &words.refresh,
            !snapshot.reading,
            None,
            Some(Picked::Send(Intent::Refresh { asked: true })),
        ),
    ]);
    groups.push(vec![
        item(
            "preferences".into(),
            &words.settings,
            true,
            None,
            Some(Picked::Send(Intent::Show {
                place: Some(Place::Settings {
                    section: SettingsSection::General,
                }),
            })),
        ),
        item(
            "help".into(),
            &words.help,
            true,
            None,
            Some(Picked::Link(HELP.into())),
        ),
    ]);
    groups.push(vec![item(
        "quit".into(),
        &words.quit,
        true,
        None,
        Some(Picked::Quit),
    )]);

    let mut menu = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !menu.is_empty() {
            menu.push(TrayEntry::Separator);
        }
        menu.extend(group);
    }
    let (status, level, tool_tip) = face(snapshot.status_item.as_ref());
    let icon = format!("com.usepitboard.Pitboard-{level}");
    let item = TrayItem {
        status,
        icon_name: format!("{icon}-symbolic"),
        attention_icon_name: format!("{icon}-attention-symbolic"),
        tool_tip,
        menu,
    };
    (item, picked)
}

// What a menu's entry about a notice does: what it sends, or the page it opens.
fn entry_does(entry: &MenuEntry) -> Option<Picked> {
    (entry.intent.clone().map(Picked::Send)).or_else(|| entry.link.clone().map(Picked::Link))
}

// The item's status, the level its icon shows, and its tool tip: the tip's first line, the app's
// name, as its title, and the rest as its text.
fn face(shown: Option<&StatusItemShown>) -> (Status, &'static str, ToolTip) {
    let Some(shown) = shown else {
        let words = pitboard_model::app_words(Front::Gnome);
        let tool_tip = ToolTip {
            title: words.app_name,
            text: String::new(),
        };
        return (Status::Active, "none", tool_tip);
    };
    let level = match shown.level {
        Some(UsageLevel::Plenty) => "plenty",
        Some(UsageLevel::Low) => "low",
        Some(UsageLevel::Out) => "out",
        None => "none",
    };
    let status = if shown.attention {
        Status::NeedsAttention
    } else {
        Status::Active
    };
    let (title, text) = shown.tip.split_once('\n').unwrap_or((&shown.tip, ""));
    let tool_tip = ToolTip {
        title: title.into(),
        text: text.into(),
    };
    (status, level, tool_tip)
}

/// What a search of `terms` finds among `quick`, the model's quick actions, as GNOME Shell and
/// KRunner show a result: its name, the line under it, and the app's icon.
pub fn found(quick: &[QuickAction], terms: &[String]) -> Vec<Found> {
    (pitboard_model::search(Front::Gnome, quick, terms).into_iter())
        .filter_map(|action| {
            let id = match &action.ask {
                QuickAsk::Switch { account } => format!("switch/{account}"),
                QuickAsk::AddAccount => "add-account".into(),
                // No front without account windows is given one (`Front::offers_account_windows`).
                QuickAsk::OpenWindow { .. } => return None,
            };
            Some(Found {
                id,
                name: action.title,
                description: action.detail.unwrap_or_default(),
                icon: config::APP_ID.into(),
            })
        })
        .collect()
}

/// What choosing the result `id`, one `found` gave, does.
pub fn result_picked(id: &str) -> Option<Picked> {
    match id.strip_prefix("switch/") {
        Some(account) => Some(Picked::Switch(account.into())),
        None => (id == "add-account").then_some(Picked::AddAccount),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::snapshot;
    use pitboard_model::{QuickAction, Severity};

    fn words() -> MenuWords {
        pitboard_model::app_words(Front::Gnome).menu
    }

    fn labels(item: &TrayItem) -> Vec<String> {
        (item.menu.iter())
            .map(|entry| match entry {
                TrayEntry::Item { label, .. } => label.clone(),
                TrayEntry::Separator => "-".into(),
            })
            .collect()
    }

    #[test]
    fn until_the_model_words_the_item_it_shows_the_apps_mark_and_its_menu() {
        let shown = snapshot(1);
        assert_eq!(
            shown.status_item, None,
            "a GNOME front has no status item before one is heard"
        );

        let (item, picked) = tray(&shown, &words());

        assert_eq!(item.status, Status::Active);
        assert_eq!(item.icon_name, "com.usepitboard.Pitboard-none-symbolic");
        assert_eq!(item.tool_tip.title, "Pitboard");
        let words = words();
        let mut expected = vec![words.open_pitboard.clone(), "-".into()];
        expected.extend(
            shown
                .menu_accounts_note
                .clone()
                .into_iter()
                .chain(["-".into()]),
        );
        expected.extend([
            words.add_account,
            words.refresh,
            "-".into(),
            words.settings,
            words.help,
            "-".into(),
            words.quit,
        ]);
        assert_eq!(labels(&item), expected);
        let picked: HashMap<_, _> = picked.into_iter().collect();
        assert_eq!(picked["open"], Picked::Send(Intent::Show { place: None }));
        assert_eq!(picked["add-account"], Picked::AddAccount);
        assert_eq!(picked["help"], Picked::Link(HELP.into()));
        assert_eq!(picked["quit"], Picked::Quit);
    }

    #[test]
    fn the_item_says_the_level_and_the_attention_the_model_words() {
        let shown = Snapshot {
            status_item: Some(StatusItemShown {
                name_and_usage: "work 92%".into(),
                usage: "92%".into(),
                spoken: "Pitboard, work 92%, needs attention".into(),
                level: Some(UsageLevel::Out),
                attention: true,
                tip: "Pitboard\nNeeds attention\nwork: 5-hour 92%".into(),
            }),
            ..snapshot(2)
        };

        let (item, _) = tray(&shown, &words());

        assert_eq!(item.status, Status::NeedsAttention);
        assert_eq!(item.icon_name, "com.usepitboard.Pitboard-out-symbolic");
        assert_eq!(
            item.attention_icon_name,
            "com.usepitboard.Pitboard-out-attention-symbolic"
        );
        assert_eq!(
            item.tool_tip,
            ToolTip {
                title: "Pitboard".into(),
                text: "Needs attention\nwork: 5-hour 92%".into()
            }
        );
    }

    #[test]
    fn a_notice_in_the_menu_sends_its_intent_or_opens_its_page() {
        let entry = |title: &str, intent: Option<Intent>, link: Option<&str>| MenuEntry {
            title: title.into(),
            subtitle: None,
            help: None,
            severity: Some(Severity::Warning),
            intent,
            link: link.map(str::to_owned),
            enabled: true,
        };
        let mut shown = snapshot(3);
        shown.menu_notices.install = Some(entry(
            "Install Claude Code…",
            None,
            Some("https://claude.com/product/claude-code"),
        ));
        shown.menu_notices.others = Some(entry(
            "2 Problems…",
            Some(Intent::Show {
                place: Some(Place::Accounts),
            }),
            None,
        ));

        let (item, picked) = tray(&shown, &words());

        assert_eq!(labels(&item)[2..4], ["Install Claude Code…", "2 Problems…"]);
        let picked: HashMap<_, _> = picked.into_iter().collect();
        assert_eq!(
            picked["notice/0"],
            Picked::Link("https://claude.com/product/claude-code".into())
        );
        assert_eq!(
            picked["notice/1"],
            Picked::Send(Intent::Show {
                place: Some(Place::Accounts)
            })
        );
    }

    #[test]
    fn a_search_finds_a_switch_and_add_account_by_the_ids_they_are_chosen_by() {
        let quick = [
            QuickAction {
                title: "Switch to personal".into(),
                ask: QuickAsk::Switch {
                    account: "35c0d1c9".into(),
                },
                detail: Some("5-hour 12%".into()),
            },
            QuickAction {
                title: "Add Account…".into(),
                ask: QuickAsk::AddAccount,
                detail: None,
            },
        ];

        let switch = found(&quick, &["pers".into()]);
        let add = found(&quick, &["add".into(), "acc".into()]);

        assert_eq!(
            switch,
            [Found {
                id: "switch/35c0d1c9".into(),
                name: "Switch to personal".into(),
                description: "5-hour 12%".into(),
                icon: config::APP_ID.into(),
            }]
        );
        assert_eq!(add[0].id, "add-account");
        assert!(found(&quick, &[]).is_empty());
        assert_eq!(
            result_picked(&switch[0].id),
            Some(Picked::Switch("35c0d1c9".into()))
        );
        assert_eq!(result_picked(&add[0].id), Some(Picked::AddAccount));
    }
}
