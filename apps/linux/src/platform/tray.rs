//! The status item where the desktop shows a tray: its face, in StatusNotifierItem 0.1's terms,
//! and its menu, as values of the glue's own.

use serde::Serialize;

use super::Failed;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrayItem {
    pub status: Status,
    // names in the icon theme, the app's status icons
    pub icon_name: String,
    pub attention_icon_name: String,
    pub tool_tip: ToolTip,
    pub menu: Vec<TrayEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Status {
    Passive,
    Active,
    NeedsAttention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolTip {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayEntry {
    Item {
        // what a click on it is told by
        id: String,
        label: String,
        enabled: bool,
        // whether it is one of a group of radio items, and is the one chosen
        radio: Option<bool>,
    },
    Separator,
}

/// The tray's item: the desktop's, or a fixture's stand-in.
pub trait StatusItem {
    fn show(&self, item: &TrayItem) -> Result<(), Failed>;
    fn hide(&self) -> Result<(), Failed>;
}
