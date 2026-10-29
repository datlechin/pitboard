//! The model's notices, posted as the desktop's notifications, or written by a fixture's
//! stand-in. A notification has a title and a body alone, so a notice's subtitle, the tool beside
//! another tool's accounts, is the body's first line. Each is posted at GNOME's normal priority:
//! none is urgent enough to show over a full-screen window or through Do Not Disturb
//! (docs/facts/linux-app.md, Notifications).

use std::sync::Arc;

use pitboard_model::PlatformError;

use crate::platform::notifications::{self, Button, Notifier, Priority};

/// What the model posts through, and the app with it.
pub struct Notices(pub Arc<dyn Notifier>);

impl pitboard_model::Notifier for Notices {
    fn post(&self, notice: pitboard_model::Notice) -> Result<(), PlatformError> {
        self.0
            .post(posted(notice))
            .map_err(|failed| PlatformError::Failed { reason: failed.0 })
    }
}

/// How the desktop is handed `notice`.
pub(crate) fn posted(notice: pitboard_model::Notice) -> notifications::Notice {
    let pitboard_model::Notice {
        id,
        title,
        subtitle,
        body,
        clicked,
        button,
    } = notice;
    notifications::Notice {
        id,
        title,
        body: match subtitle {
            Some(subtitle) => format!("{subtitle}\n{body}"),
            None => body,
        },
        priority: Priority::Normal,
        clicked,
        buttons: (button.into_iter())
            .map(|button| Button {
                title: button.title,
                clicked: button.clicked,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Failed;
    use pitboard_model::Notifier as _;
    use pitboard_model::{NoticeButton, NoticeButtonKind};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Posted(Mutex<Vec<notifications::Notice>>);

    impl Notifier for Posted {
        fn post(&self, notice: notifications::Notice) -> Result<(), Failed> {
            self.0.lock().unwrap().push(notice);
            Ok(())
        }

        fn withdraw(&self, _id: String) -> Result<(), Failed> {
            Ok(())
        }
    }

    fn notice(subtitle: Option<&str>) -> pitboard_model::Notice {
        pitboard_model::Notice {
            id: "run-out/claude/work".into(),
            title: "work is out until 3:45 PM".into(),
            subtitle: subtitle.map(str::to_owned),
            body: "Switch to personal, which has 88% left.".into(),
            clicked: "accounts".into(),
            button: Some(NoticeButton {
                kind: NoticeButtonKind::SwitchTo,
                title: "Switch to personal".into(),
                clicked: "switch/claude/personal".into(),
            }),
        }
    }

    #[test]
    fn a_notices_subtitle_is_the_first_line_of_its_body() {
        let posted = Arc::new(Posted::default());
        let notices = Notices(posted.clone());

        notices.post(notice(Some("Claude Code"))).unwrap();
        notices.post(notice(None)).unwrap();

        let posted = posted.0.lock().unwrap();
        assert_eq!(
            posted[0].body,
            "Claude Code\nSwitch to personal, which has 88% left."
        );
        assert_eq!(posted[1].body, "Switch to personal, which has 88% left.");
    }

    #[test]
    fn a_notice_is_posted_at_normal_priority_with_its_button_and_what_each_click_hands_back() {
        let posted = Arc::new(Posted::default());

        Notices(posted.clone()).post(notice(None)).unwrap();

        assert_eq!(
            posted.0.lock().unwrap()[0],
            notifications::Notice {
                id: "run-out/claude/work".into(),
                title: "work is out until 3:45 PM".into(),
                body: "Switch to personal, which has 88% left.".into(),
                priority: Priority::Normal,
                clicked: "accounts".into(),
                buttons: vec![Button {
                    title: "Switch to personal".into(),
                    clicked: "switch/claude/personal".into(),
                }],
            }
        );
    }
}
