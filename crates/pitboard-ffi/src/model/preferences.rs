//! The app's own preferences, which the model keeps in `app.json` in Pitboard's directory:
//! the tools somebody said "Not Now" to a second account for, whether the app has ever shown
//! anybody anything, and what it does once a limit reaches what share. The macOS app kept
//! them in UserDefaults, and hands them over once, as `AppLaunch::earlier_preferences`: where
//! the file is there, it wins.

use super::{AtLimit, EarlierPreferences, Tip};
use pitboard_core::duty::autoswitch::Threshold;
use pitboard_core::provider::ProviderId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const NOTHING: &str = "nothing";
const NOTIFY: &str = "notify";
const SWITCH: &str = "switch";

fn tip_code(tip: Tip) -> &'static str {
    match tip {
        Tip::UseCodex => "use_codex",
        Tip::OpenAtLogin => "open_at_login",
        Tip::AtLimit => "at_limit",
    }
}

/// What `app.json` holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Preferences {
    /// The tools, by code, somebody keeps one account of on purpose: the nudge to add a
    /// second is not shown for them.
    #[serde(default)]
    pub(crate) second_account_declined: BTreeSet<String>,
    /// Whether the app has ever shown anybody anything. An app with no Dock icon that
    /// launches straight into a menu bar item shows somebody who has just installed it
    /// nothing at all, so the first launch opens the window, once.
    #[serde(default)]
    pub(crate) has_been_seen: bool,
    // Written beside `at_limit` too, for a version before it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) auto_switch: bool,
    // The share it notifies or switches at. Any whole number reads, so one no version would
    // write leaves the rest of the file readable, and is taken as the nearest share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) auto_switch_at: Option<i64>,
    // Text, so a choice a later version adds leaves the rest of the file readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) at_limit: Option<String>,
    // Text, as `at_limit` is, for a tip a later version adds.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub(crate) tips_declined: BTreeSet<String>,
}

impl Preferences {
    /// The preferences kept in `file`, `app.json`'s text where there is one, and whether they
    /// came from it. Where there is none, or it does not read as preferences, they are what
    /// the app read from its earlier store, once: "Not Now" said before there was a second
    /// tool, `hideSecondAccountNudge`, was said about Claude Code, the only tool there was.
    pub(crate) fn kept(
        file: Option<&str>,
        earlier: Option<&EarlierPreferences>,
    ) -> (Preferences, bool) {
        if let Some(kept) = file.and_then(|text| serde_json::from_str::<Preferences>(text).ok()) {
            return (kept, true);
        }
        let Some(earlier) = earlier else {
            return (Preferences::default(), false);
        };
        let mut declined: BTreeSet<String> =
            earlier.second_account_declined.iter().cloned().collect();
        if earlier.second_account_nudge_hidden {
            declined.insert(ProviderId::Claude.code().to_owned());
        }
        (
            Preferences {
                second_account_declined: declined,
                has_been_seen: earlier.has_been_seen,
                ..Preferences::default()
            },
            false,
        )
    }

    pub(crate) fn threshold(&self) -> Threshold {
        self.auto_switch_at
            .map_or_else(Threshold::default, Threshold::clamped)
    }

    // Before anybody chose, switching by itself on reads as Switch and off as Notify.
    pub(crate) fn at_limit(&self) -> AtLimit {
        match self.at_limit.as_deref() {
            Some(NOTHING) => AtLimit::Nothing,
            Some(NOTIFY) => AtLimit::Notify,
            Some(SWITCH) => AtLimit::Switch,
            _ if self.auto_switch => AtLimit::Switch,
            _ => AtLimit::Notify,
        }
    }

    pub(crate) fn at_limit_chosen(&self) -> bool {
        self.at_limit.is_some() || self.auto_switch
    }

    pub(crate) fn declined(&self, tip: Tip) -> bool {
        self.tips_declined.contains(tip_code(tip))
    }

    // Whether it was not declined already.
    pub(crate) fn decline(&mut self, tip: Tip) -> bool {
        self.tips_declined.insert(tip_code(tip).to_owned())
    }

    pub(crate) fn choose(&mut self, choice: AtLimit) {
        self.auto_switch = choice == AtLimit::Switch;
        self.at_limit = Some(
            match choice {
                AtLimit::Nothing => NOTHING,
                AtLimit::Notify => NOTIFY,
                AtLimit::Switch => SWITCH,
            }
            .to_owned(),
        );
    }

    /// `app.json`'s text for these preferences.
    pub(crate) fn text(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn earlier(declined: &[&str], seen: bool, nudge: bool) -> EarlierPreferences {
        EarlierPreferences {
            second_account_declined: declined.iter().map(|&code| code.to_owned()).collect(),
            has_been_seen: seen,
            second_account_nudge_hidden: nudge,
        }
    }

    fn declined(preferences: &Preferences) -> Vec<&str> {
        preferences
            .second_account_declined
            .iter()
            .map(String::as_str)
            .collect()
    }

    /// Where the file is there it wins, whatever the app read from its earlier store: the
    /// earlier store is read once, to move what it held, and is out of date after that.
    #[test]
    fn the_file_wins_where_it_is_there() {
        let (kept, from_file) = Preferences::kept(
            Some(r#"{"second_account_declined":["codex"],"has_been_seen":true}"#),
            Some(&earlier(&["claude"], false, true)),
        );
        assert!(from_file);
        assert_eq!(declined(&kept), ["codex"]);
        assert!(kept.has_been_seen);
    }

    /// Without the file, what the app read from its earlier store is taken, once, and a "Not
    /// Now" said before there was a second tool is about Claude Code: AppModel.swift moved
    /// `hideSecondAccountNudge` the same way.
    #[test]
    fn without_the_file_the_earlier_store_is_taken() {
        let (kept, from_file) = Preferences::kept(None, Some(&earlier(&["codex"], true, true)));
        assert!(!from_file);
        assert_eq!(declined(&kept), ["claude", "codex"]);
        assert!(kept.has_been_seen);

        let (kept, _) = Preferences::kept(None, Some(&earlier(&[], false, false)));
        assert_eq!(kept, Preferences::default());
        assert_eq!(
            Preferences::kept(None, None),
            (Preferences::default(), false)
        );
    }

    /// A file that does not read as preferences is taken as none, and what the earlier store
    /// held in its place; a file missing a field takes that field's default.
    #[test]
    fn a_file_that_does_not_read_is_none() {
        let (kept, from_file) =
            Preferences::kept(Some("not json"), Some(&earlier(&["codex"], true, false)));
        assert!(!from_file);
        assert_eq!(declined(&kept), ["codex"]);
        let (kept, from_file) = Preferences::kept(Some("{}"), None);
        assert!(from_file);
        assert_eq!(kept, Preferences::default());
    }

    /// The file is what it holds, read back as it was written.
    #[test]
    fn the_file_reads_back_as_written() {
        let preferences = Preferences {
            second_account_declined: BTreeSet::from(["codex".to_owned()]),
            has_been_seen: true,
            ..Preferences::default()
        };
        let text = preferences.text().expect("text");
        assert_eq!(
            text,
            r#"{"second_account_declined":["codex"],"has_been_seen":true}"#
        );
        assert_eq!(Preferences::kept(Some(&text), None), (preferences, true));
    }

    /// Switching by itself is off unless somebody turned it on, at 95% unless they chose
    /// another share; what they chose reads back, and a share no version could keep reads as
    /// the nearest there can be.
    #[test]
    fn switching_by_itself_is_off_until_turned_on_and_keeps_its_share() {
        let none = Preferences::default();
        assert!(!none.auto_switch);
        assert_eq!(none.threshold().percent(), 95);

        let on = Preferences {
            auto_switch: true,
            auto_switch_at: Some(90),
            ..Preferences::default()
        };
        let text = on.text().expect("text");
        assert_eq!(
            text,
            r#"{"second_account_declined":[],"has_been_seen":false,"auto_switch":true,"auto_switch_at":90}"#
        );
        assert_eq!(Preferences::kept(Some(&text), None), (on, true));

        let (odd, _) =
            Preferences::kept(Some(r#"{"auto_switch":true,"auto_switch_at":120}"#), None);
        assert_eq!(odd.threshold().percent(), 99);
    }

    /// Before anybody chose what happens at the share, switching by itself turned on reads as
    /// Switch, and off, or a fresh install, as Notify. A choice is written beside whether it
    /// switches, for a version before the choice, and one a later version wrote that this one
    /// does not know reads as though none were made.
    #[test]
    fn what_happens_at_a_limit_follows_switching_by_itself_until_chosen() {
        let read = |text: &str| Preferences::kept(Some(text), None).0.at_limit();
        assert_eq!(Preferences::default().at_limit(), AtLimit::Notify);
        assert_eq!(read(r#"{"auto_switch":true}"#), AtLimit::Switch);
        assert_eq!(read(r#"{"has_been_seen":true}"#), AtLimit::Notify);
        assert_eq!(
            read(r#"{"auto_switch":true,"at_limit":"nothing"}"#),
            AtLimit::Nothing
        );
        assert_eq!(
            read(r#"{"auto_switch":true,"at_limit":"later"}"#),
            AtLimit::Switch
        );

        let mut chosen = Preferences::default();
        chosen.choose(AtLimit::Switch);
        assert_eq!(
            chosen.text().as_deref(),
            Some(
                r#"{"second_account_declined":[],"has_been_seen":false,"auto_switch":true,"at_limit":"switch"}"#
            )
        );
        chosen.choose(AtLimit::Nothing);
        assert_eq!(
            chosen.text().as_deref(),
            Some(r#"{"second_account_declined":[],"has_been_seen":false,"at_limit":"nothing"}"#)
        );
        let text = chosen.text().expect("text");
        assert_eq!(Preferences::kept(Some(&text), None), (chosen, true));
    }

    /// A share no version would keep, written by hand or by a later version, is read as the
    /// nearest there can be, and the rest of the file with it: one field out of range does
    /// not make the preferences unreadable.
    #[test]
    fn a_share_out_of_range_is_read_as_the_nearest_with_the_rest() {
        for (written, read) in [(300, 99), (-1, 50)] {
            let (kept, from_file) = Preferences::kept(
                Some(&format!(
                    r#"{{"second_account_declined":["codex"],"auto_switch":true,"auto_switch_at":{written}}}"#
                )),
                None,
            );
            assert!(from_file, "{written}");
            assert_eq!(declined(&kept), ["codex"]);
            assert!(kept.auto_switch);
            assert_eq!(kept.threshold().percent(), read);
        }
    }
}
