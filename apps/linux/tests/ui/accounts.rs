//! The accounts page: each account's group with its limits, what it says of the account in use
//! and the button another offers, its menu, and the notices and the tip above them.

use adw::prelude::*;
use atspi::Role;
use pitboard_linux::window::{AccountGroup, LimitRow, NoticeRow, level_name};
use pitboard_model::{ActionKind, RowEnd, UsageLevel};

use super::harness::{App, PATIENCE, Screen, eventually, owned, session};
use super::views::{self, Viewed, button, every};
use super::words;

// Each account's group, by its title, which is markup.
fn group(viewed: &Viewed, title: &str) -> AccountGroup {
    let escaped = glib::markup_escape_text(title);
    (every::<AccountGroup>(&viewed.window).into_iter())
        .find(|group| group.title() == escaped)
        .unwrap_or_else(|| panic!("no group {title:?}"))
}

#[gtk::test]
fn each_account_shows_its_limits_with_their_level() {
    let viewed = Viewed::of("twoTools");
    let items = viewed.items();
    assert_eq!(
        every::<AccountGroup>(&viewed.window).len(),
        items.len(),
        "a group each"
    );
    let mut levels = Vec::new();
    for item in &items {
        let rows = every::<LimitRow>(&group(&viewed, &item.title));
        assert_eq!(rows.len(), item.limits.len(), "{}", item.title);
        for (row, limit) in rows.iter().zip(&item.limits) {
            assert_eq!(row.title(), limit.name);
            assert_eq!(row.subtitle().as_deref(), Some(limit.resets.as_str()));
            let bar = every::<gtk::LevelBar>(row).remove(0);
            assert_eq!(
                bar.value(),
                limit.percent.clamp(0.0, 100.0),
                "{}",
                limit.name
            );
            let offsets: Vec<_> = ["plenty", "low", "out", "high", "full"]
                .into_iter()
                .filter(|name| bar.offset_value(Some(name)).is_some())
                .collect();
            assert_eq!(offsets, [level_name(limit.level)], "{}", limit.name);
            assert_eq!(bar.offset_value(Some(level_name(limit.level))), Some(100.0));
            let figures: Vec<_> = (every::<gtk::Label>(row).iter())
                .map(|label| label.label().to_string())
                .collect();
            assert!(figures.contains(&limit.figure), "{figures:?}");
            levels.push(limit.level);
        }
    }
    assert!(levels.contains(&UsageLevel::Plenty), "{levels:?}");
}

// Over AT-SPI, as a screen reader's user finds them: the label and the button GTK names by
// the model's words, with the roles it gives them.
#[test]
fn the_account_in_use_says_so_and_another_offers_its_button() {
    let items = views::items_of("oneTool");
    let in_use = (items.iter())
        .find_map(|item| match &item.end {
            Some(RowEnd::InUse { title }) => Some(title.clone()),
            _ => None,
        })
        .expect("an account in use");
    let offered = (items.iter())
        .find_map(|item| match &item.end {
            Some(RowEnd::Button { action }) => Some(action.clone()),
            _ => None,
        })
        .expect("another's button");
    let bus = session();
    let _app = App::launch("oneTool", &[]);
    owned(&bus);
    let screen = Screen::new();
    screen.window();

    screen.waits_for("the account in use", |seen| {
        seen.role == Role::Label && seen.name == in_use && seen.showing
    });
    let switch = screen.waits_for("another's button", |seen| {
        seen.role == Role::Button && seen.name == offered.spoken && seen.showing
    });
    screen.press(&switch, "click");

    eventually("the button gone once switched to", || {
        let app = screen.app()?;
        screen
            .find(&app, |seen| {
                seen.role == Role::Button && seen.name == offered.spoken
            })
            .is_none()
            .then_some(())
    });
}

// GTK's popover menu, opened and chosen from over AT-SPI as a person would (M12): the account's
// actions but the one its button offers, then copying its address, then removing it.
#[test]
fn an_accounts_other_actions_are_in_its_menu() {
    let items = views::items_of("oneTool");
    let item = (items.iter())
        .find(|item| matches!(item.end, Some(RowEnd::Button { .. })))
        .expect("an account not in use")
        .clone();
    let offered = match &item.end {
        Some(RowEnd::Button { action }) => action.intent.clone(),
        _ => unreachable!(),
    };
    let mut expected: Vec<String> = (item.actions.iter())
        .filter(|action| {
            action.kind != ActionKind::Remove && action.intent != Some(offered.clone())
        })
        .map(|action| action.title.clone())
        .collect();
    expected.push(words::app().menu.copy_email);
    expected.extend(
        (item.actions.iter())
            .filter(|action| action.kind == ActionKind::Remove)
            .map(|action| action.title.clone()),
    );
    let bus = session();
    let _app = App::launch("oneTool", &[]);
    owned(&bus);
    let screen = Screen::new();
    screen.window();
    let group = screen.waits_for("its group", |seen| {
        seen.role == Role::Grouping && seen.name == item.title
    });
    let menu = screen
        .find(&group, |seen| seen.role == Role::ToggleButton)
        .expect("its menu's button");

    screen.press(&menu, "click");

    let shown = || -> Vec<String> {
        (screen.every(|seen| seen.role == Role::MenuItem && seen.showing))
            .into_iter()
            .map(|seen| seen.name)
            .collect()
    };
    let deadline = std::time::Instant::now() + PATIENCE;
    let offered = loop {
        let offered = shown();
        if offered == expected || std::time::Instant::now() > deadline {
            break offered;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(offered, expected);
}

// The stuck world's notice offers to give up on the switch, which asks first.
#[gtk::test]
fn a_notices_buttons_send_their_intents_asking_first_where_it_says() {
    let viewed = Viewed::of("stuck");
    let notices = viewed.shown.snapshot().notices.clone();
    let rows = every::<NoticeRow>(&viewed.window);
    let action = (notices.iter())
        .flat_map(|notice| &notice.actions)
        .find(|action| !action.dismisses && action.confirm.is_some())
        .unwrap_or_else(|| panic!("a button that asks first: {notices:#?}"))
        .clone();
    let question = action.confirm.clone().expect("asked");
    let row = (rows.iter())
        .find(|row| {
            every::<gtk::Button>(*row)
                .iter()
                .any(|b| b.label().as_deref() == Some(&action.title))
        })
        .expect("its row");

    button(row, &action.title).emit_clicked();

    let alert = viewed
        .dialog()
        .downcast::<adw::AlertDialog>()
        .expect("an alert");
    assert_eq!(alert.heading().as_deref(), Some(question.title.as_str()));
    assert!(viewed.sent().is_empty(), "sent before it was answered");
    button(&alert, &question.confirm).emit_clicked();
    viewed.until("answered", |_| viewed.window.visible_dialog().is_none());
    assert_eq!(viewed.sent(), std::slice::from_ref(&action.intent));
    for notice in &notices {
        let shown = (rows.iter()).find(|row| row.title() == notice.title);
        assert!(shown.is_some(), "{} has no row", notice.title);
    }
}

// The setup step's tip: its first choice is what it is for, drawn as the suggested action, and
// each sends what the model says.
#[gtk::test]
fn the_tip_shows_its_choices() {
    let viewed = Viewed::of("onlyOne");
    let step = viewed.shown.snapshot().setup.clone().expect("a setup step");
    let tip = (every::<NoticeRow>(&viewed.window).into_iter())
        .find(|row| row.title() == step.title)
        .expect("the tip's row");
    assert!(tip.is_visible() && tip.is_mapped());
    assert_eq!(tip.subtitle().as_deref(), Some(step.detail.as_str()));
    let buttons: Vec<gtk::Button> = every(&tip);
    let shown: Vec<_> = (buttons.iter())
        .filter(|button| button.label().is_some())
        .map(|button| {
            (
                button.label().unwrap().to_string(),
                button.has_css_class("suggested-action"),
            )
        })
        .collect();
    let choices: Vec<_> = (step.actions.iter())
        .enumerate()
        .map(|(at, choice)| (choice.title.clone(), at == 0))
        .collect();
    assert_eq!(shown, choices);

    let declined = step.actions.last().expect("a choice").clone();
    button(&tip, &declined.title).emit_clicked();
    assert_eq!(viewed.sent(), [declined.intent]);
}
