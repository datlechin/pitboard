//! Which account to switch to once the one in use has run out, from Advice.swift, and the
//! record of what has been told about, once for each reset of a limit, from Notifier.swift.
//!
//! Pure: what advice there is follows from a read, the tools in their order and what has been
//! told, and the model's state keeps both and says what to post and what to keep.

use crate::{Account, Limit, Status, Tool, Usage};
use pitboard_core::usage::{self, same_reset, whole};
use std::collections::BTreeMap;

/// The share, as drawn, an account to offer must be below in every limit: one the app draws
/// at 100% of any has nothing left to offer.
const FULL: u8 = 100;

/// What has been told about, by `Advice::key`: the reset of the limit last told, in epoch
/// seconds, or 0 where none was known. Kept in Pitboard's directory between launches.
pub(crate) type Told = BTreeMap<String, i64>;

/// An account in use has run out of a limit, and another account of the same tool has room
/// for a switch away from it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Advice {
    /// The tool both accounts are for, as a `Tool`'s code.
    pub(crate) provider: String,
    /// The tool's name, when accounts of more than one tool were read and a sentence has to
    /// say which it is about. `None` otherwise, so a machine with one tool reads as before.
    pub(crate) tool: Option<String>,
    /// The account that ran out, and the limit it ran out of.
    pub(crate) ran: String,
    pub(crate) window: Limit,
    /// The account offered instead, and what it has left of the same kind of limit: `None`
    /// where it has no such limit.
    pub(crate) instead: String,
    pub(crate) left: Option<i64>,
    /// What to switch to: `instead` with its tool, which names one account whatever else is
    /// enrolled.
    pub(crate) switch_to: String,
}

fn window(limit: &Limit) -> usage::Window {
    usage::Window {
        kind: limit.kind.clone(),
        scope: limit.scope.clone(),
        percent: limit.percent,
        resets_at: limit.resets_at,
        is_active: limit.is_active,
        severity: limit.severity.clone(),
        length_seconds: limit.length_seconds,
    }
}

fn windows_of(measured: &Usage) -> Vec<usage::Window> {
    measured.windows.iter().map(window).collect()
}

/// The account of the same tool that can be switched to at `now` with the most room for a
/// switch away from `ran` of `current`, by the rule the automatic switch keeps, and what it
/// has left of that kind of limit: none when none has room.
fn spare(
    ran: &Limit,
    current: &Account,
    mine: &[&Account],
    now: i64,
) -> Option<(String, Option<i64>, String)> {
    let ran = window(ran);
    let in_use = current.usage.as_ref().map(windows_of).unwrap_or_default();
    let (spare, room) = usage::roomiest(
        mine.iter()
            .filter(|account| account.switchable && account.qualified.is_some())
            .filter_map(|account| {
                let theirs = account.usage.as_ref()?;
                let room = usage::room(
                    &windows_of(theirs),
                    theirs.lists_every_limit,
                    &in_use,
                    &ran,
                    FULL,
                    now,
                )?;
                Some((account, room))
            }),
    )?;
    Some((
        spare.label.clone()?,
        room.used.map(|used| 100 - whole(used)),
        spare.qualified.clone()?,
    ))
}

impl Advice {
    /// What has been told is kept under the tool, the account and the limit. Keyed by the
    /// limit alone, one tool's exhausted five hours would silence another's.
    pub(crate) fn key(provider: &str, label: &str, window: &Limit) -> String {
        [
            provider,
            label,
            window.kind.as_str(),
            window.scope.as_deref().unwrap_or_default(),
        ]
        .join("/")
    }

    pub(crate) fn key_of(&self) -> String {
        Advice::key(&self.provider, &self.ran, &self.window)
    }

    /// Nothing to say unless an account in use has exhausted a limit not yet told about, and
    /// an account of the same tool that can be switched to now has room for a switch away
    /// from it. At most one piece of advice per tool, in the order `tools` lists them.
    ///
    /// Only ever the same tool: a Codex account with room left is no help to somebody whose
    /// Claude Code account has run out, and a switch between them is not a switch at all.
    /// A limit told about at a reset one minute from this one's, by the core's rule for one
    /// reset, has been told.
    pub(crate) fn about(status: &Status, tools: &[Tool], told: &Told) -> Vec<Advice> {
        let codes: Vec<&str> = status
            .accounts
            .iter()
            .map(|account| account.provider.as_str())
            .collect();
        let providers = crate::present::in_order(&codes, tools);
        providers
            .iter()
            .filter_map(|provider| {
                let mine: Vec<&Account> = status
                    .accounts
                    .iter()
                    .filter(|account| &account.provider == provider)
                    .collect();
                let current = mine
                    .iter()
                    .find(|account| account.signed_in && account.label.is_some())?;
                let ran = current.label.as_deref()?;
                let windows = current.usage.iter().flat_map(|usage| &usage.windows);
                for window in windows.filter(|window| window.percent >= 100.0) {
                    if let Some(&at) = told.get(&Advice::key(provider, ran, window))
                        && same_reset(at, window.resets_at.unwrap_or(0))
                    {
                        continue;
                    }
                    let Some((instead, left, switch_to)) =
                        spare(window, current, &mine, status.now)
                    else {
                        continue;
                    };
                    return Some(Advice {
                        provider: provider.clone(),
                        tool: (providers.len() > 1).then(|| {
                            tools
                                .iter()
                                .find(|tool| &tool.code == provider)
                                .map_or_else(|| provider.clone(), |tool| tool.name.clone())
                        }),
                        ran: ran.to_owned(),
                        window: window.clone(),
                        instead,
                        left,
                        switch_to,
                    });
                }
                None
            })
            .collect()
    }

    /// This advice as `status` bears it out now: `None` once the account that ran out has
    /// room again or is no longer in use, and otherwise offering the best account there is
    /// now. The one offered before may have been forgotten, renamed, expired or run out
    /// itself, and a menu item offering it would fail when chosen.
    pub(crate) fn renewed(&self, status: &Status) -> Option<Advice> {
        let current = self.still_out(status)?;
        let mine: Vec<&Account> = status
            .accounts
            .iter()
            .filter(|account| account.provider == self.provider)
            .collect();
        let (instead, left, switch_to) = spare(&self.window, current, &mine, status.now)?;
        Some(Advice {
            instead,
            left,
            switch_to,
            ..self.clone()
        })
    }

    /// The account that ran out, where `status` still bears this out: it is still the one in
    /// use, and the same limit of it is still spent, at the same reset.
    fn still_out<'a>(&self, status: &'a Status) -> Option<&'a Account> {
        status.accounts.iter().find(|account| {
            account.provider == self.provider
                && account.label.as_deref() == Some(self.ran.as_str())
                && account.signed_in
                && account.usage.iter().flat_map(|u| &u.windows).any(|limit| {
                    limit.kind == self.window.kind
                        && limit.scope == self.window.scope
                        && limit.percent >= 100.0
                        && same_reset(
                            limit.resets_at.unwrap_or(0),
                            self.window.resets_at.unwrap_or(0),
                        )
                })
        })
    }

    /// This advice about the account `label` of its tool, as it reads once that account is
    /// called `to`.
    pub(crate) fn renaming(&self, label: &str, to: &str) -> Advice {
        let rename = |name: &String| {
            if name == label {
                to.to_owned()
            } else {
                name.clone()
            }
        };
        let old = format!("{}/{label}", self.provider);
        Advice {
            ran: rename(&self.ran),
            instead: rename(&self.instead),
            switch_to: if self.switch_to == old {
                format!("{}/{to}", self.provider)
            } else {
                self.switch_to.clone()
            },
            ..self.clone()
        }
    }
}

/// What was told about the account `label` of `provider`'s tool, kept as told about it as
/// `to`, so a rename does not make an account that ran out read as one that has just run out.
/// That account alone: not the same name in another tool, and not a name that only starts
/// with it.
pub(crate) fn rename_told(told: &mut Told, provider: &str, label: &str, to: &str) {
    let old = format!("{provider}/{label}/");
    let moved: Vec<(String, i64)> = told
        .iter()
        .filter(|(key, _)| key.starts_with(&old))
        .map(|(key, &at)| (key.clone(), at))
        .collect();
    for (key, at) in moved {
        told.remove(&key);
        told.insert(format!("{provider}/{to}/{}", &key[old.len()..]), at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::testing::{LimitExt, account, both_tools, window};

    fn status(accounts: Vec<Account>) -> Status {
        Status {
            now: 0,
            accounts,
            warnings: Vec::new(),
        }
    }

    fn about(status: &Status, told: &Told) -> Vec<Advice> {
        Advice::about(status, &[], told)
    }

    /// MenuTests.swift's theAccountWithTheMostLeftIsOffered.
    #[test]
    fn the_account_with_the_most_left_is_offered() {
        let read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 100.0)])
                .build(),
            account(Some("spare"))
                .limits(vec![window("session", 80.0)])
                .build(),
            account(Some("fresh"))
                .limits(vec![window("session", 5.0)])
                .build(),
        ]);
        let advice = about(&read, &Told::new()).remove(0);
        assert_eq!(advice.ran, "work");
        assert_eq!(advice.instead, "fresh");
        assert_eq!(advice.left, Some(95));
        assert_eq!(
            advice.switch_to, "claude/fresh",
            "the switch names the account with its tool"
        );
        assert_eq!(advice.tool, None, "one tool, so nothing says which");
    }

    /// Of two with as much left, the one least full in its other limits, as the automatic
    /// switch chooses.
    #[test]
    fn of_two_with_as_much_left_the_least_full_in_its_other_limits_is_offered() {
        let read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 100.0)])
                .build(),
            account(Some("busy"))
                .limits(vec![window("session", 20.0), window("weekly_all", 80.0)])
                .build(),
            account(Some("idle"))
                .limits(vec![window("session", 20.0), window("weekly_all", 10.0)])
                .build(),
        ]);
        assert_eq!(
            about(&read, &Told::new())
                .first()
                .map(|advice| advice.instead.as_str()),
            Some("idle")
        );
    }

    /// MenuTests.swift's anAccountThatCannotBeSwitchedToIsNotOffered.
    #[test]
    fn an_account_that_cannot_be_switched_to_is_not_offered() {
        let read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 100.0)])
                .build(),
            account(Some("parked"))
                .switchable(false)
                .limits(vec![window("session", 0.0)])
                .build(),
        ]);
        assert!(about(&read, &Told::new()).is_empty());
    }

    /// MenuTests.swift's nothingIsSaidWhenEveryAccountIsSpent.
    #[test]
    fn nothing_is_said_when_every_account_is_spent() {
        let read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 100.0)])
                .build(),
            account(Some("spare"))
                .limits(vec![window("session", 100.0)])
                .build(),
        ]);
        assert!(about(&read, &Told::new()).is_empty());
    }

    fn spent_at(resets: i64) -> Status {
        status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 100.0).resets(Some(resets))])
                .build(),
            account(Some("spare"))
                .limits(vec![window("session", 0.0)])
                .build(),
        ])
    }

    /// MenuTests.swift's oneExhaustedWindowIsMentionedOnce. The next window is its own; what
    /// was said about the last one does not carry over.
    #[test]
    fn one_exhausted_window_is_mentioned_once() {
        let read = spent_at(42);
        let told = Advice::key("claude", "work", &window("session", 100.0));
        assert!(about(&read, &Told::from([(told.clone(), 42)])).is_empty());
        assert_eq!(
            about(&read, &Told::from([(told, 42 - 18_000)]))
                .first()
                .map(|advice| advice.instead.as_str()),
            Some("spare")
        );
    }

    /// A session is given a reset in whole seconds, and Anthropic's answer gives a fraction
    /// that is dropped, so one window can come back a second apart from what was told about
    /// it. The core counts resets a minute apart as one, and so does what is told.
    ///
    /// MenuTests.swift's aWindowToldAboutIsNotToldAgainWithItsResetRoundedOtherwise.
    #[test]
    fn a_window_told_about_is_not_told_again_with_its_reset_rounded_otherwise() {
        let told = Advice::key("claude", "work", &window("session", 100.0));
        assert!(about(&spent_at(43), &Told::from([(told, 42)])).is_empty());
    }

    /// Advice is about one window. The window after it, spent as well, is advice of its own
    /// to tell, and not this advice still holding.
    ///
    /// MenuTests.swift's adviceHoldsForTheWindowItIsAboutAndNotTheOneAfter.
    #[test]
    fn advice_holds_for_the_window_it_is_about_and_not_the_one_after() {
        let advice = about(&spent_at(7_200), &Told::new()).remove(0);
        assert!(advice.still_out(&spent_at(7_200)).is_some());
        assert!(
            advice.still_out(&spent_at(7_201)).is_some(),
            "one reset, as another source rounds it"
        );
        assert!(advice.still_out(&spent_at(25_200)).is_none());
    }

    /// An account whose limits are unknown is not one to recommend.
    #[test]
    fn an_account_with_no_reading_is_not_offered() {
        let read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 100.0)])
                .build(),
            account(Some("spare")).unmeasured().build(),
        ]);
        assert!(about(&read, &Told::new()).is_empty());
    }

    /// Anthropic's answer lists every limit an account has, and a Team seat's has no weekly
    /// limit for all models. One that has none of the limit that ran out is a place to go,
    /// and is not said to have all of it left.
    #[test]
    fn an_account_on_a_plan_without_that_limit_is_offered() {
        let read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 20.0), window("weekly_all", 100.0)])
                .build(),
            account(Some("seat"))
                .limits(vec![
                    window("session", 10.0),
                    window("weekly_scoped", 0.0).scope("Fable"),
                ])
                .build(),
        ]);
        assert_eq!(
            about(&read, &Told::new())
                .first()
                .map(|advice| (advice.instead.as_str(), advice.left)),
            Some(("seat", None))
        );
    }

    /// The automatic switch's rule, for a tool it never switches. OpenAI's answer may leave
    /// a window out, so a Codex account whose reading leaves out a limit the account in use
    /// has is not offered, whichever limit ran out. Nor is one shown at 100% of any limit.
    #[test]
    fn a_codex_account_is_offered_by_the_same_rule() {
        let five_hours = |percent| window("five_hour", percent).length(18_000);
        let week = |percent| window("seven_day", percent).length(604_800);
        let codex = |label, limits| account(Some(label)).of("codex").limits(limits).build();
        let work = account(Some("work"))
            .of("codex")
            .signed_in()
            .limits(vec![five_hours(100.0), week(30.0)])
            .build();
        let not_offered = vec![
            work,
            codex("full", vec![five_hours(99.6), week(10.0)]),
            codex("weekly-only", vec![week(0.0)]),
            codex("five-hour-only", vec![five_hours(10.0)]),
        ];
        assert!(
            Advice::about(&status(not_offered.clone()), &both_tools(), &Told::new()).is_empty()
        );
        let spare = codex("spare", vec![five_hours(40.0), week(50.0)]);
        let read = status([not_offered, vec![spare]].concat());
        let advice = Advice::about(&read, &both_tools(), &Told::new());
        assert_eq!(
            advice
                .first()
                .map(|advice| (advice.switch_to.as_str(), advice.left)),
            Some(("codex/spare", Some(60)))
        );
    }

    /// MenuTests.swift's aWeeklyLimitIsComparedWithWeeklyLimits.
    #[test]
    fn a_weekly_limit_is_compared_with_weekly_limits() {
        let read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 10.0), window("weekly_all", 100.0)])
                .build(),
            account(Some("spare"))
                .limits(vec![window("session", 90.0), window("weekly_all", 20.0)])
                .build(),
            account(Some("other"))
                .limits(vec![window("session", 5.0), window("weekly_all", 60.0)])
                .build(),
        ]);
        let advice = about(&read, &Told::new()).remove(0);
        assert_eq!(advice.window.kind, "weekly_all");
        assert_eq!(advice.instead, "spare");
    }

    /// A limit whose reset has passed counts as reset, though the last reading of it, taken
    /// before, said it was spent: an account parked since is a place to go.
    #[test]
    fn a_limit_whose_reset_has_passed_is_not_counted_as_spent() {
        let mut read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("weekly_all", 100.0).resets(Some(90_000))])
                .build(),
            account(Some("spare"))
                .limits(vec![
                    window("session", 100.0).resets(Some(500)),
                    window("weekly_all", 30.0).resets(Some(90_000)),
                ])
                .build(),
        ]);
        read.now = 1_000;
        let advice = about(&read, &Told::new()).remove(0);
        assert_eq!((advice.instead.as_str(), advice.left), ("spare", Some(70)));
    }

    /// An account that has run out of another limit is no place to go, whichever limit sent
    /// somebody looking: switched to, it stops at once. One with room in every limit is
    /// offered instead, though it has less of the limit that ran out.
    #[test]
    fn an_account_with_another_limit_spent_is_not_offered() {
        let spent_weekly = account(Some("spare"))
            .limits(vec![window("session", 0.0), window("weekly_all", 100.0)])
            .build();
        let work = account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 100.0), window("weekly_all", 30.0)])
            .build();
        let read = status(vec![work.clone(), spent_weekly.clone()]);
        assert!(about(&read, &Told::new()).is_empty());

        let read = status(vec![
            work,
            spent_weekly,
            account(Some("other"))
                .limits(vec![window("session", 40.0), window("weekly_all", 10.0)])
                .build(),
        ]);
        let advice = about(&read, &Told::new()).remove(0);
        assert_eq!((advice.instead.as_str(), advice.left), ("other", Some(60)));
    }

    /// A Codex account with room is no help to somebody whose Claude Code account has run
    /// out: a switch between them is not a switch at all.
    ///
    /// MenuTests.swift's adviceNeverCrossesTools.
    #[test]
    fn advice_never_crosses_tools() {
        let read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 100.0)])
                .build(),
            account(Some("job"))
                .of("codex")
                .signed_in()
                .limits(vec![window("five_hour", 10.0).length(18_000)])
                .build(),
            account(Some("spare"))
                .of("codex")
                .limits(vec![window("five_hour", 0.0).length(18_000)])
                .build(),
        ]);
        assert!(Advice::about(&read, &both_tools(), &Told::new()).is_empty());
    }

    /// Each tool is advised about from its own accounts, and says which tool it is about.
    ///
    /// MenuTests.swift's eachToolIsAdvisedFromItsOwnAccounts. Its `said`, which only tests
    /// spoke, words nothing the panel or a notification says; their words are
    /// presenting's.
    #[test]
    fn each_tool_is_advised_from_its_own_accounts() {
        let read = status(vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 100.0)])
                .build(),
            account(Some("personal"))
                .limits(vec![window("session", 30.0)])
                .build(),
            account(Some("work"))
                .of("codex")
                .signed_in()
                .limits(vec![window("seven_day", 100.0).length(604_800)])
                .build(),
            account(Some("spare"))
                .of("codex")
                .limits(vec![window("seven_day", 60.0).length(604_800)])
                .build(),
        ]);
        let advice = Advice::about(&read, &both_tools(), &Told::new());
        let to: Vec<&str> = advice.iter().map(|a| a.switch_to.as_str()).collect();
        assert_eq!(to, ["claude/personal", "codex/spare"]);
        let tools: Vec<Option<&str>> = advice.iter().map(|a| a.tool.as_deref()).collect();
        assert_eq!(tools, [Some("Claude Code"), Some("Codex")]);
        assert_eq!(advice[1].left, Some(40));
    }

    /// What was said about one tool's `work` says nothing about another's.
    ///
    /// MenuTests.swift's whatWasToldIsKeptApartByTool.
    #[test]
    fn what_was_told_is_kept_apart_by_tool() {
        let exhausted = window("session", 100.0).resets(Some(7));
        let read = status(vec![
            account(Some("work"))
                .of("codex")
                .signed_in()
                .limits(vec![exhausted.clone()])
                .build(),
            account(Some("spare"))
                .of("codex")
                .limits(vec![window("session", 0.0)])
                .build(),
        ]);
        let claude = Told::from([(Advice::key("claude", "work", &exhausted), 7)]);
        assert_eq!(
            about(&read, &claude).first().map(|a| a.switch_to.as_str()),
            Some("codex/spare")
        );
        let codex = Told::from([(Advice::key("codex", "work", &exhausted), 7)]);
        assert!(about(&read, &codex).is_empty());
    }

    /// What has been told is kept per tool and account. A rename moves what was told about
    /// that one account, every limit of it, and nothing else: not the same name in another
    /// tool, and not a name that only starts with it.
    ///
    /// AppModelTests.swift's aRenameMovesWhatWasToldAboutThatAccountAlone.
    #[test]
    fn a_rename_moves_what_was_told_about_that_account_alone() {
        let mut told = Told::from([
            ("claude/work/session/".to_owned(), 1),
            ("claude/work/weekly_all/".to_owned(), 2),
            ("codex/work/primary/".to_owned(), 3),
            ("claude/workshop/session/".to_owned(), 4),
        ]);
        rename_told(&mut told, "claude", "work", "office");
        assert_eq!(
            told,
            Told::from([
                ("claude/office/session/".to_owned(), 1),
                ("claude/office/weekly_all/".to_owned(), 2),
                ("codex/work/primary/".to_owned(), 3),
                ("claude/workshop/session/".to_owned(), 4),
            ])
        );
    }

    /// Advice renamed reads as the account is called now, of its own tool alone.
    #[test]
    fn advice_renamed_reads_as_the_account_is_called_now() {
        let advice = about(&spent_at(7_200), &Told::new()).remove(0);
        let spare = advice.renaming("spare", "home");
        assert_eq!(
            (spare.instead.as_str(), spare.switch_to.as_str()),
            ("home", "claude/home")
        );
        let work = advice.renaming("work", "office");
        assert_eq!(work.ran, "office");
        assert_eq!(work.key_of(), "claude/office/session/");
    }
}
