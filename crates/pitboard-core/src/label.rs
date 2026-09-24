//! Turning what somebody typed into one account.
//!
//! A label was a name unique across the whole machine, because there was one tool and one
//! set of accounts. With three, `work` is a name somebody will want for their work account
//! on each of them, and refusing the second one would be pitboard imposing a namespace
//! nobody asked for.
//!
//! So a label is unique within a provider, and `codex/work` says which. A bare `work` still
//! works and still means what it always did, as long as it names one account; where it
//! names two, pitboard says so and lists them rather than picking.

use crate::error::{Enrolled, Error, Result};
use crate::provider::ProviderId;
use crate::state::{Account, Key, State};

/// The separator between a provider and a label. A label may not contain one.
pub const SEPARATOR: char = '/';

/// What somebody typed, before it is looked up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec<'a> {
    /// `work`: whichever provider has it, if only one does.
    Bare(&'a str),
    /// `codex/work`: this provider's, and no other's.
    Qualified(ProviderId, &'a str),
}

impl Spec<'_> {
    pub fn label(&self) -> &str {
        match self {
            Spec::Bare(label) | Spec::Qualified(_, label) => label,
        }
    }
}

/// Read `provider/label`, or a bare label.
///
/// A prefix that is not a provider is an error rather than a label containing a slash,
/// because the alternative is `pitboard use codx/work` quietly looking for an account
/// literally called `codx/work` and reporting it missing.
pub fn parse(typed: &str) -> Result<Spec<'_>> {
    match typed.split_once(SEPARATOR) {
        None => Ok(Spec::Bare(typed)),
        Some((prefix, label)) => match ProviderId::parse(prefix) {
            Some(provider) => Ok(Spec::Qualified(provider, label)),
            None => Err(Error::ProviderUnknown {
                typed: prefix.to_string(),
                known: ProviderId::ALL
                    .iter()
                    .map(|p| p.code().to_string())
                    .collect(),
            }),
        },
    }
}

/// The one account this names.
///
/// A label written by 0.1.x could contain a slash, because nothing then separated a tool
/// from a name. Such an account is still found by what it is called, whole: `team/a` finds
/// the Claude Code account literally labelled `team/a` when no tool is called `team`, and
/// when one is and has no such account. Otherwise the app, which passes the label it was
/// given, could neither switch to it nor forget it.
pub fn resolve<'a>(state: &'a State, typed: &str) -> Result<&'a Account> {
    let literal = || state.accounts.iter().find(|a| a.label == typed);
    let spec = match parse(typed) {
        Ok(spec) => spec,
        Err(unknown) => return literal().ok_or(unknown),
    };
    if let Spec::Qualified(provider, label) = spec
        && state.get(&Key::new(provider, label)).is_none()
        && let Some(account) = literal()
    {
        return Ok(account);
    }
    match spec {
        Spec::Qualified(provider, label) => {
            state
                .get(&Key::new(provider, label))
                .ok_or_else(|| Error::AccountUnknown {
                    label: typed.to_string(),
                    enrolled: state.labels(provider),
                })
        }
        Spec::Bare(label) => {
            let mut found = state.accounts.iter().filter(|a| a.label == label);
            let first = found.next().ok_or_else(|| Error::AccountUnknown {
                label: label.to_string(),
                enrolled: qualified(state),
            })?;
            match found.next() {
                None => Ok(first),
                // Every one of them, not just the two that clash, so the next attempt can
                // be typed from what is on the screen.
                Some(_) => Err(Error::LabelAmbiguous {
                    label: label.to_string(),
                    matches: state
                        .accounts
                        .iter()
                        .filter(|a| a.label == label)
                        .map(qualify)
                        .collect(),
                }),
            }
        }
    }
}

/// `claude/work`, as a person would type it back.
pub fn qualify(account: &Account) -> String {
    account.key().qualified()
}

/// Every enrolled label, for a bare label that found nothing.
///
/// Qualified only when more than one tool has accounts, because then the reason the label
/// missed may be that it needs a prefix. With one tool a prefix is noise on every line, and
/// this message is one people read often.
fn qualified(state: &State) -> Enrolled {
    let mut providers = state.accounts.iter().map(Account::provider);
    let first = providers.next();
    let one_tool = providers.all(|p| Some(p) == first);
    Enrolled(
        state
            .accounts
            .iter()
            .map(|a| {
                if one_tool {
                    a.label.clone()
                } else {
                    qualify(a)
                }
            })
            .collect(),
    )
}

/// A name somebody is choosing for a new account, with the tool it is for.
///
/// `pitboard enroll codex/personal --sign-in` says both at once. A bare name means the
/// default tool, which keeps every command anybody has typed before working unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub provider: ProviderId,
    pub label: String,
}

/// The tool a bare name is for.
///
/// Claude Code, because a bare name is what every pitboard command before this took and it
/// meant Claude Code. Adding a second tool must not change what somebody's existing script
/// does.
pub const DEFAULT: ProviderId = ProviderId::Claude;

/// Read `[provider/]label` as a name for an account that does not exist yet.
///
/// Unlike [`resolve`], nothing is enrolled yet to disambiguate against, so a bare name
/// cannot mean "whichever tool has it". It means [`DEFAULT`].
pub fn choose(typed: &str) -> std::result::Result<Chosen, String> {
    let (provider, label) = match typed.split_once(SEPARATOR) {
        None => (DEFAULT, typed),
        Some((prefix, label)) => {
            let provider = ProviderId::parse(prefix).ok_or_else(|| {
                let known: Vec<&str> = ProviderId::ALL.iter().map(|p| p.code()).collect();
                format!(
                    "`{prefix}` is not a tool pitboard knows. It knows: {}",
                    known.join(", ")
                )
            })?;
            (provider, label)
        }
    };
    if label.is_empty() {
        return Err(format!(
            "`{typed}` names a tool but no account. Try `{}{SEPARATOR}work`",
            provider.code()
        ));
    }
    if label.contains(SEPARATOR) {
        return Err(format!(
            "a label cannot contain `{SEPARATOR}`: it separates the tool from the name, so \
             at most one belongs in `{typed}`"
        ));
    }
    Ok(Chosen {
        provider,
        label: label.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Detail;

    fn account(provider: ProviderId, label: &str) -> Account {
        let detail = match provider {
            ProviderId::Claude => Detail::Claude {
                organization_uuid: String::new(),
                oauth_account: serde_json::Value::Null,
            },
            ProviderId::Codex => Detail::Codex {
                workspace_id: None,
                plan: None,
            },
        };
        Account {
            label: label.into(),
            account_uuid: format!("{}-{label}", provider.code()),
            email: format!("{label}@example.com"),
            parked: None,
            last_used_at: None,
            detail,
        }
    }

    fn state(labels: &[(ProviderId, &str)]) -> State {
        let mut state = State::default();
        for (provider, label) in labels {
            state.accounts.push(account(*provider, label));
        }
        state
    }

    #[test]
    fn a_prefix_says_which_provider() {
        assert_eq!(
            parse("claude/work").unwrap(),
            Spec::Qualified(ProviderId::Claude, "work")
        );
        assert_eq!(parse("work").unwrap(), Spec::Bare("work"));
    }

    /// Without this, `pitboard use codx/work` looks for an account literally called
    /// `codx/work`, fails to find one, and says the label is not enrolled. The person then
    /// checks their labels, sees `work` is right there, and has no way to tell what
    /// happened.
    #[test]
    fn a_prefix_that_is_not_a_provider_says_so() {
        let err = parse("codx/work").unwrap_err();
        assert_eq!(err.code(), "provider_unknown");
        assert!(err.to_string().contains("codx"), "{err}");
        assert!(err.to_string().contains("claude"), "{err}");
    }

    #[test]
    fn a_bare_label_still_works_when_it_names_one_account() {
        let state = state(&[(ProviderId::Claude, "work")]);
        assert_eq!(resolve(&state, "work").unwrap().label, "work");
        assert_eq!(resolve(&state, "claude/work").unwrap().label, "work");
    }

    #[test]
    fn a_label_nothing_has_lists_what_is_enrolled_qualified() {
        let state = state(&[(ProviderId::Claude, "work")]);
        let err = resolve(&state, "personal").unwrap_err();
        assert_eq!(err.code(), "account_unknown");
        assert!(
            err.to_string().contains("`work`") && !err.to_string().contains("claude/work"),
            "one tool enrolled, so a prefix on every line would be noise: {err}"
        );
    }

    /// Once two tools have accounts, the prefix is the point: a bare label that missed may
    /// have missed because it needed one.
    #[test]
    fn a_miss_lists_labels_qualified_once_two_tools_have_accounts() {
        let one = state(&[
            (ProviderId::Claude, "work"),
            (ProviderId::Claude, "personal"),
        ]);
        assert!(
            !resolve(&one, "nobody")
                .unwrap_err()
                .to_string()
                .contains("claude/"),
            "one tool stays bare"
        );

        let two = state(&[
            (ProviderId::Claude, "work"),
            (ProviderId::Codex, "personal"),
        ]);
        let err = resolve(&two, "nobody").unwrap_err().to_string();
        assert!(
            err.contains("claude/work") && err.contains("codex/personal"),
            "{err}"
        );
    }

    /// A label from before there was a tool prefix is still found by its whole name.
    #[test]
    fn a_label_written_with_a_slash_before_prefixes_existed_is_still_found() {
        let state = state(&[(ProviderId::Claude, "team/a"), (ProviderId::Codex, "a")]);
        assert_eq!(resolve(&state, "team/a").unwrap().label, "team/a");
        assert_eq!(
            resolve(&state, "codex/a").unwrap().provider(),
            ProviderId::Codex,
            "a prefix that names a real account of that tool still wins"
        );
        assert_eq!(
            resolve(&state, "codx/a").unwrap_err().code(),
            "provider_unknown",
            "and a mistyped prefix is still said to be one"
        );
    }

    /// pitboard picking one would switch an account the person did not name.
    #[test]
    fn a_bare_label_two_providers_share_is_refused_and_both_are_named() {
        let state = state(&[(ProviderId::Claude, "work"), (ProviderId::Codex, "work")]);

        let err = resolve(&state, "work").unwrap_err();
        assert_eq!(err.code(), "label_ambiguous");
        assert!(err.to_string().contains("claude/work"), "{err}");
        assert!(err.to_string().contains("codex/work"), "{err}");

        assert_eq!(
            resolve(&state, "codex/work").unwrap().provider(),
            ProviderId::Codex,
            "and a prefix picks exactly the one it names"
        );
    }

    #[test]
    fn a_qualified_miss_lists_only_that_providers_labels() {
        let state = state(&[(ProviderId::Claude, "work")]);
        let err = resolve(&state, "claude/personal").unwrap_err();
        assert!(err.to_string().contains("`work`"), "{err}");
        assert!(
            !err.to_string().contains("claude/work"),
            "inside one provider the prefix is noise: {err}"
        );
    }

    /// The syntax for adding an account, which has to carry the tool because nothing is
    /// enrolled yet to work it out from.
    #[test]
    fn a_new_account_can_name_its_tool() {
        assert_eq!(
            choose("work").unwrap(),
            Chosen {
                provider: DEFAULT,
                label: "work".into()
            }
        );
        assert_eq!(
            choose("claude/work").unwrap(),
            Chosen {
                provider: ProviderId::Claude,
                label: "work".into()
            }
        );
    }

    /// A bare name meant Claude Code in every pitboard anybody has run. A second tool must
    /// not change what a script somebody already wrote does.
    #[test]
    fn a_bare_name_still_means_what_it_always_did() {
        assert_eq!(DEFAULT, ProviderId::Claude);
        assert_eq!(choose("work").unwrap().provider, ProviderId::Claude);
    }

    #[test]
    fn a_new_name_is_refused_when_it_is_not_one() {
        for (typed, says) in [
            ("codx/work", "not a tool"),
            ("claude/", "no account"),
            ("claude/b/c", "cannot contain"),
        ] {
            let refused = choose(typed).unwrap_err();
            assert!(refused.contains(says), "{typed}: {refused}");
        }
    }
}
