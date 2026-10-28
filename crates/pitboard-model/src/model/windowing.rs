//! The account windows' bookkeeping, driven through `State::apply` by hand: the macOS app's
//! AccountPickerTests.swift, the decision cases of StoreJanitorTests.swift and the decision
//! halves of AccountWindowsTests.swift, as they were at a3e5ce0, each under its own name in
//! snake case, and what those never tested: the records the macOS app kept, taken in their
//! own case, the wait before Open answers, a download from start to end, and what quitting
//! asks while downloads run.

use super::state::Job;
use super::testing::machine::{Hand, Stub};
use super::testing::records::{DANA, refusal, status};
use super::windows::{LINK_SCHEME, WEB_SCHEME};
use super::{DownloadEnd, EarlierWindowRecords, Intent};
use crate::account_windows::records::{Entry, Records};
use crate::account_windows::{WindowNoteKind, stores::derive};
use crate::present::testing::account;
use crate::present::words;
use crate::{
    Account, Choice, DownloadState, JumpAsk, LinkPicker, OpenWindow, PickerShown, Question,
    SiteMenu, WaitingShown, WindowWaiting, downloads_quit_question,
};
use pitboard_core::host::{OS, Os};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

/// Another Pitboard directory on the same machine, such as a release build's run with a
/// scratch home, which shares the app's web stores.
const SCRATCH: &str = "/tmp/test/.pitboard";

/// The accounts of the fixture both tools are in, as a read lists them: Claude Code's work in
/// use and personal, Codex's main in use and spare.
fn two_tools() -> Vec<Account> {
    vec![
        account(Some("work")).signed_in().uuid("w").build(),
        account(Some("personal")).uuid("p").build(),
        account(Some("main"))
            .of("codex")
            .signed_in()
            .uuid("m")
            .build(),
        account(Some("spare")).of("codex").uuid("s").build(),
    ]
}

fn without(label: &str) -> Vec<Account> {
    two_tools()
        .into_iter()
        .filter(|account| account.label.as_deref() != Some(label))
        .collect()
}

/// The store of the claude.ai window of the account `uuid`.
fn claude(uuid: &str) -> String {
    derive(pitboard_sites::CLAUDE.store_name, uuid)
}

/// The store of the chatgpt.com window of the account `uuid`.
fn chatgpt(uuid: &str) -> String {
    derive(pitboard_sites::CHATGPT.store_name, uuid)
}

/// `link` handed over as this build's Share extension writes it.
fn shared(link: &str, scheme: &str) -> Intent {
    let link = pitboard_sites::SiteLink::parse(link).expect("a link of one of the sites");
    Intent::LinkArrived {
        text: pitboard_sites::pitboard_link(&link, scheme),
    }
}

/// A model as the app starts it over `machine`, what it keeps read and its first read in.
fn started(machine: &mut Stub) -> Hand {
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run(machine);
    model
}

/// The same, its windows on a fixture's stand-in pages, as the Swift tests' were.
fn started_on_stand_ins(machine: &mut Stub) -> Hand {
    let mut model = Hand::new();
    model
        .state
        .windows
        .on(Os::MacOs, "pitboard-debug", "pitboard-fixture");
    model.send(Intent::Start);
    model.run(machine);
    model
}

fn started_on(os: Os, machine: &mut Stub) -> Hand {
    let mut model = Hand::new();
    model.state.windows.on(os, LINK_SCHEME, WEB_SCHEME);
    model.send(Intent::Start);
    model.run(machine);
    model
}

fn machine() -> Stub {
    Stub::reading(Ok(status(two_tools())))
}

fn picker(model: &Hand) -> LinkPicker {
    model
        .shown()
        .account_windows
        .picker
        .expect("a link waiting")
}

fn open_window(model: &Hand, store: &str) -> Option<OpenWindow> {
    model
        .shown()
        .account_windows
        .open
        .into_iter()
        .find(|window| window.store == store)
}

/// Opens the window of `store`, as the app does once it has, and everything that leads to.
fn open(model: &mut Hand, machine: &mut Stub, store: &str) -> OpenWindow {
    model.send(Intent::WindowOpened {
        store: store.into(),
    });
    model.run(machine);
    open_window(model, store).expect("the window opens")
}

fn close(model: &mut Hand, store: &str) {
    model.send(Intent::WindowClosed {
        store: store.into(),
    });
}

/// The wait before Open answers, over.
fn armed(model: &mut Hand) {
    model.later(Duration::from_millis(750));
    model.tick();
    assert!(picker(model).armed);
}

fn choose(model: &mut Hand, store: &str) {
    let arrival = picker(model).arrival;
    model.send(Intent::OpenLink {
        arrival,
        store: store.into(),
    });
}

fn chosen(model: &Hand) -> String {
    match picker(model).shown {
        PickerShown::Choose { chosen, .. } => chosen,
        other => panic!("accounts to choose from: {other:?}"),
    }
}

/// The stores the app is asked to delete, in the order asked.
fn deleting(model: &Hand) -> Vec<String> {
    model
        .shown()
        .account_windows
        .deleting
        .into_iter()
        .map(|asked| asked.store)
        .collect()
}

fn checks(job: &Job) -> bool {
    matches!(job, Job::CheckShared { .. })
}

/// The records file holding `stores` for each directory, as the model writes it.
fn file(stores: &[(&str, &[&str])]) -> String {
    let mut records = Records::default();
    for (key, stores) in stores {
        records.set(
            key,
            &Entry {
                stores: stores.iter().map(|&store| store.to_owned()).collect(),
                ..Entry::default()
            },
        );
    }
    records.text().expect("text")
}

// MARK: - The picker

/// Every link from outside waits for a choice, and the choice offered is the account chosen
/// last for the site, else the one in use, else the first. An account no longer offered for
/// the site is never chosen.
///
/// AccountPickerTests.swift's thePickerOffersTheSitesAccountsWithOneChosen.
#[test]
fn the_picker_offers_the_sites_accounts_with_one_chosen() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    model.send(shared("https://claude.ai/chat/x", "pitboard"));
    let PickerShown::Choose {
        title,
        link,
        link_text,
        accounts,
        chosen: first,
        open,
    } = picker(&model).shown
    else {
        panic!("accounts to choose from");
    };
    assert_eq!(title, words::open_link_as("claude.ai"));
    assert_eq!(link.url, "https://claude.ai/chat/x");
    assert_eq!(link_text, "claude.ai/chat/x");
    assert_eq!(open, words::OPEN);
    let labels: Vec<&str> = accounts.iter().map(|a| a.window.label.as_str()).collect();
    assert_eq!(labels, ["work", "personal"], "only claude.ai's accounts");
    assert_eq!(accounts[0].detail, "work@example.com");
    assert_eq!(first, claude("w"), "the account in use");

    armed(&mut model);
    choose(&mut model, &claude("p"));
    model.send(shared("https://claude.ai/new", "pitboard"));
    assert_eq!(chosen(&model), claude("p"), "chosen last for the site");

    machine.answer = Ok(status(without("personal")));
    model.refresh(&mut machine);
    assert_eq!(
        chosen(&model),
        claude("w"),
        "an account no longer offered is never chosen"
    );
}

// Anything on the machine can open a Pitboard link, so none switches, whatever it names.
#[test]
fn a_pitboard_link_never_switches_an_account() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    let personal = crate::present::glance_id("claude:p");
    for text in [
        format!("pitboard://switch?account={personal}"),
        "pitboard://switch?to=claude%2Fpersonal".into(),
        format!("pitboard://open?account={personal}"),
        "pitboard://open?url=https%3A%2F%2Fclaude.ai%2F&switch=claude%2Fpersonal".into(),
    ] {
        model.send(Intent::LinkArrived { text: text.clone() });
        model.run(&mut machine);
        assert!(
            matches!(picker(&model).shown, PickerShown::Refused { .. }),
            "{text}"
        );
    }
    model.send(shared("https://claude.ai/new", "pitboard"));
    armed(&mut model);
    choose(&mut model, &claude("p"));
    model.run(&mut machine);
    assert!(model.state.switch_under_way().is_none());
    assert_eq!(
        model.count(|job| matches!(job, Job::Holding { .. } | Job::Switch { .. })),
        0
    );
    assert!(machine.switched_to.is_empty());
}

/// AccountPickerTests.swift's thePickerSaysWhatStandsInTheWay: a link of another site is
/// refused, saying why; before any read the picker waits for one, and says why one failed;
/// and a site no account opens offers to add one.
#[test]
fn the_picker_says_what_stands_in_the_way() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    model.send(Intent::LinkArrived {
        text: "pitboard://open?url=https%3A%2F%2Fexample.com%2F".into(),
    });
    assert_eq!(
        picker(&model).shown,
        PickerShown::Refused {
            title: words::cannot_open_link(OS),
            reason: "Pitboard opens claude.ai and chatgpt.com links only. This link is on \
                     example.com."
                .into(),
        }
    );

    let mut unread = Hand::new();
    unread.send(shared("https://claude.ai/", "pitboard"));
    assert_eq!(
        picker(&unread).shown,
        PickerShown::Reading {
            title: words::READING_ACCOUNTS.into()
        }
    );

    let mut failing = Stub::reading(Err(refusal("unreachable", "No network.", Vec::new())));
    failing.offline = Err(refusal("state_unreadable", "No network.", Vec::new()));
    let mut failed = started(&mut failing);
    failed.send(shared("https://claude.ai/", "pitboard"));
    let PickerShown::ReadFailed {
        title,
        detail,
        retry,
    } = picker(&failed).shown
    else {
        panic!("the read that failed");
    };
    assert_eq!(title, words::could_not_read_accounts(OS));
    assert_eq!(detail, "No network.");
    assert_eq!(retry.title, words::try_again(OS));
    assert_eq!(retry.intent, Intent::Refresh { asked: true });

    let mut codex_only = Stub::reading(Ok(status(vec![account(Some("main")).of("codex").build()])));
    let mut model = started(&mut codex_only);
    model.send(shared("https://claude.ai/", "pitboard"));
    let PickerShown::NoAccount {
        title,
        link_text,
        detail,
        in_browser,
        add,
        ..
    } = picker(&model).shown
    else {
        panic!("no account");
    };
    assert_eq!((title, detail), words::no_site_account(OS, "claude.ai"));
    assert_eq!(link_text, "claude.ai/");
    assert_eq!(in_browser, words::open_in_browser(OS));
    assert_eq!(add.title, words::add_account(OS, true));
    assert_eq!(
        add.intent,
        Intent::PresentSheet {
            sheet: super::Sheet::Add {
                provider: Some("claude".into())
            }
        }
    );
}

/// A link is read as strictly as a stranger's, and a second one replaces the first, shown as
/// new. Closing the picker on a link that was replaced closes nothing.
///
/// AccountPickerTests.swift's theInboxTakesPitboardLinksOfItsOwnScheme.
#[test]
fn the_inbox_takes_pitboard_links_of_its_own_scheme() {
    let mut machine = machine();
    let mut model = started_on_stand_ins(&mut machine);
    assert_eq!(model.shown().account_windows.picker, None);
    let (title, detail) = words::no_link(OS);
    assert_eq!(
        model.shown().account_windows.no_link,
        crate::present::EmptyList { title, detail },
        "what the Open Link window says meanwhile"
    );

    model.send(shared("https://claude.ai/new", "pitboard-debug"));
    let first = picker(&model);
    let PickerShown::Choose { link, .. } = &first.shown else {
        panic!("accounts to choose from");
    };
    assert_eq!(link.url, "https://claude.ai/new");

    model.send(shared("https://claude.ai/new", "pitboard"));
    let second = picker(&model);
    assert!(second.arrival > first.arrival, "shown as new");
    assert_eq!(
        second.shown,
        PickerShown::Refused {
            title: words::cannot_open_link(OS),
            reason: pitboard_sites::LinkRefusal::Unreadable.to_string(),
        },
        "another build's scheme"
    );

    model.send(Intent::DismissLink {
        arrival: first.arrival,
    });
    assert_eq!(picker(&model).arrival, second.arrival, "not the one shown");
    model.send(Intent::DismissLink {
        arrival: second.arrival,
    });
    assert_eq!(model.shown().account_windows.picker, None);
}

/// Choosing an account takes the link, remembers the account for the site, and the next
/// link of the site offers it first, ahead of the account in use.
///
/// AccountPickerTests.swift's choosingAnAccountRemembersItForTheSite.
#[test]
fn choosing_an_account_remembers_it_for_the_site() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    model.send(shared("https://chatgpt.com/", "pitboard"));
    assert_eq!(chosen(&model), chatgpt("m"), "the account in use");
    armed(&mut model);
    choose(&mut model, &chatgpt("s").to_uppercase());
    assert_eq!(model.shown().account_windows.picker, None);
    assert_eq!(
        model
            .state
            .windows
            .waiting()
            .get(&chatgpt("s"))
            .map(String::as_str),
        Some("https://chatgpt.com/"),
        "the first page of the window about to open"
    );
    model.send(shared("https://chatgpt.com/c/1", "pitboard"));
    assert_eq!(chosen(&model), chatgpt("s"));
}

/// Open answers only once a link has waited three quarters of a second from when the
/// accounts to choose from appear, so a Return typed for another app as the picker came
/// forward opens nothing: from the link's arrival, from the read that lists the accounts
/// where the link came first, and again for each link that arrives. AccountPicker.swift's
/// arming, which the Swift tested nowhere.
#[test]
fn a_link_waits_before_it_can_be_opened() {
    let mut machine = machine();
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.send(shared("https://claude.ai/chat/1", "pitboard"));
    assert!(matches!(picker(&model).shown, PickerShown::Reading { .. }));
    model.later(Duration::from_secs(1));
    model.tick();
    assert!(!picker(&model).armed, "nothing to choose from yet");

    model.run(&mut machine);
    assert!(matches!(picker(&model).shown, PickerShown::Choose { .. }));
    assert!(!picker(&model).armed, "from when the accounts appear");
    choose(&mut model, &claude("w"));
    assert!(model.shown().account_windows.picker.is_some(), "too soon");
    assert!(model.state.windows.waiting().is_empty());

    model.later(Duration::from_millis(749));
    model.tick();
    assert!(!picker(&model).armed);
    model.later(Duration::from_millis(1));
    assert_eq!(
        model.state.next_due(),
        Some(model.now.running),
        "the actor wakes for it"
    );
    model.tick();
    assert!(picker(&model).armed);

    model.send(shared("https://claude.ai/chat/2", "pitboard"));
    assert!(!picker(&model).armed, "each link waits");
    armed(&mut model);
    choose(&mut model, &claude("w"));
    assert_eq!(model.shown().account_windows.picker, None);
}

// MARK: - Windows and their records

/// The windows are the enrolled accounts' of each site, and the menus offer them: an item
/// for a site's only account, a submenu for several.
///
/// AccountWindowsTests.swift's theWindowsAreTheEnrolledAccountsOfEachSite.
#[test]
fn the_windows_are_the_enrolled_accounts_of_each_site() {
    let mut machine = machine();
    machine.answer = Ok(status(
        two_tools()
            .into_iter()
            .chain([account(None).uuid("u").signed_in().build()])
            .collect(),
    ));
    let model = started(&mut machine);
    let shown = model.shown().account_windows;
    let labels: Vec<&str> = shown.accounts.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(labels, ["work", "personal", "main", "spare"]);
    let titles: Vec<&str> = shown
        .menus
        .iter()
        .map(|menu| match menu {
            SiteMenu::One { title, .. } | SiteMenu::Several { title, .. } => title.as_str(),
        })
        .collect();
    assert_eq!(
        titles,
        [
            words::open_site("claude.ai"),
            words::open_site("chatgpt.com")
        ]
    );
}

/// A window that cannot show its page yet says what it waits for in the model's words, as
/// the Open Link window does: the accounts being read, or, where they could not be and
/// nothing is known, why, with a way to try again that reads them again. Once they are read,
/// a window still waiting, for its store to be recorded, says they are being read, as the
/// Swift said. AccountWindowView.swift's own words, as they were at a3e5ce0, which the Swift
/// tested nowhere.
#[test]
fn a_window_that_cannot_show_its_page_says_why_in_the_models_words() {
    let reading = WindowWaiting {
        window_title: words::ACCOUNT.into(),
        shown: WaitingShown::Reading {
            title: words::READING_ACCOUNTS.into(),
        },
    };
    assert_eq!(Hand::new().shown().account_windows.waiting, reading);

    let unreadable = || {
        Err(refusal(
            "state_unreadable",
            "could not read Pitboard's account list",
            Vec::new(),
        ))
    };
    let mut machine = Stub::reading(unreadable());
    machine.offline = unreadable();
    let mut model = started(&mut machine);
    assert_eq!(
        model.shown().account_windows.waiting,
        WindowWaiting {
            window_title: words::ACCOUNT.into(),
            shown: WaitingShown::ReadFailed {
                title: words::could_not_read_accounts(OS),
                detail: "could not read Pitboard's account list".into(),
                retry: Choice {
                    title: words::try_again(OS),
                    intent: Intent::Refresh { asked: true },
                    enabled: true,
                },
            },
        }
    );

    machine.answer = Ok(status(two_tools()));
    model.refresh(&mut machine);
    assert_eq!(model.shown().account_windows.waiting, reading);
}

/// A store WebKit made and nobody recorded would never be deleted, so a window waits until
/// its store is recorded, and only then shows its page.
///
/// StoreJanitorTests.swift's aStoreIsRecordedBeforeItIsMade.
#[test]
fn a_store_is_recorded_before_it_is_made() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    model.send(Intent::WindowOpened {
        store: claude("w").to_uppercase(),
    });
    let write = model.take(|job| matches!(job, Job::KeepWindows { .. }));
    let Job::KeepWindows { entry, .. } = &write else {
        unreachable!()
    };
    assert!(entry.stores.contains(&claude("w")));
    assert_eq!(
        open_window(&model, &claude("w")),
        None,
        "not before it is recorded"
    );
    model.give(machine.answer(write));
    let window = open_window(&model, &claude("w")).expect("open once recorded");
    assert_eq!(window.account.label, "work");
    assert_eq!(window.load.url, "https://claude.ai/");
    assert!(machine.windows_records().stores.contains(&claude("w")));
}

/// A window that has never opened for this Pitboard directory says how to sign in; one
/// that has does not, and a window is one per account.
///
/// AccountWindowsTests.swift's onlyAWindowsFirstOpeningSaysHowToSignIn.
#[test]
fn only_a_windows_first_opening_says_how_to_sign_in() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    let first = open(&mut model, &mut machine, &chatgpt("m"));
    assert_eq!(first.note, Some(WindowNoteKind::SignIn));
    model.send(Intent::WindowOpened {
        store: chatgpt("m"),
    });
    assert_eq!(model.state.windows.open.len(), 1, "one window per account");
    close(&mut model, &chatgpt("m"));
    assert_eq!(open_window(&model, &chatgpt("m")), None);
    assert_eq!(open(&mut model, &mut machine, &chatgpt("m")).note, None);
}

/// A window starts at a link chosen for it, else at the page it was last on where that is
/// one of the site's own, else at the site's home: after it is closed as well as after a
/// relaunch.
///
/// AccountWindowsTests.swift's aWindowStartsAtItsLinkItsLastPageOrItsHome.
#[test]
fn a_window_starts_at_its_link_its_last_page_or_its_home() {
    let mut machine = machine();
    let mut model = started_on_stand_ins(&mut machine);
    let work = claude("w");
    let home = open(&mut model, &mut machine, &work);
    assert_eq!(home.load.url, "pitboard-fixture://claude.ai/");
    model.send(Intent::PageShown {
        store: work.clone(),
        url: "pitboard-fixture://claude.ai/chat/old".into(),
    });
    model.run(&mut machine);
    close(&mut model, &work);
    assert_eq!(
        open(&mut model, &mut machine, &work).load.url,
        "pitboard-fixture://claude.ai/chat/old"
    );
    model.send(Intent::PageShown {
        store: work.clone(),
        url: "https://example.com/somewhere".into(),
    });
    close(&mut model, &work);
    assert_eq!(
        open(&mut model, &mut machine, &work).load.url,
        "pitboard-fixture://claude.ai/chat/old",
        "a page off the site is never kept"
    );
    close(&mut model, &work);

    let mut relaunched = started_on_stand_ins(&mut machine);
    assert_eq!(
        open(&mut relaunched, &mut machine, &work).load.url,
        "pitboard-fixture://claude.ai/chat/old",
        "and after a relaunch"
    );
    close(&mut relaunched, &work);
    relaunched.send(shared("https://claude.ai/chat/shared", "pitboard-debug"));
    armed(&mut relaunched);
    choose(&mut relaunched, &work);
    assert_eq!(
        open(&mut relaunched, &mut machine, &work).load.url,
        "pitboard-fixture://claude.ai/chat/shared"
    );
}

/// A link chosen for an account whose window is open loads in that window, where Back
/// returns to what it showed: its page is asked for again, under a new number.
///
/// AccountWindowsTests.swift's aLinkForAnOpenWindowLoadsInIt.
#[test]
fn a_link_for_an_open_window_loads_in_it() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    let main = chatgpt("m");
    let before = open(&mut model, &mut machine, &main).load;
    model.send(shared("https://chatgpt.com/c/abc", "pitboard"));
    let PickerShown::Choose { accounts, .. } = picker(&model).shown else {
        panic!("accounts to choose from");
    };
    assert_eq!(accounts[0].detail, words::window_open("main@example.com"));
    armed(&mut model);
    choose(&mut model, &main);
    let after = open_window(&model, &main).expect("still open").load;
    assert_eq!(after.url, "https://chatgpt.com/c/abc");
    assert!(after.serial > before.serial);
    assert!(model.state.windows.waiting().is_empty());
}

/// Only a site's own page is kept as a window's last, and never its sign-in, which would
/// sign the window in again with whatever it carried: a window quit on chatgpt.com's sign-in
/// callback opens on the page before it, and one recorded on a sign-in page opens at home.
/// The Swift kept any page on the site, sign-in included.
#[test]
fn a_window_never_opens_on_its_sign_in_again() {
    let main = chatgpt("m");
    let mut machine = machine();
    machine.windows_earlier = Some(Records::earlier(&EarlierWindowRecords {
        stores: HashMap::from([(DANA.to_owned(), vec![main.to_uppercase()])]),
        pages: HashMap::from([(
            DANA.to_owned(),
            HashMap::from([(
                main.to_uppercase(),
                "https://chatgpt.com/api/auth/callback/openai?code=old".to_owned(),
            )]),
        )]),
    }));
    let mut model = started(&mut machine);
    assert_eq!(
        open(&mut model, &mut machine, &main).load.url,
        "https://chatgpt.com/",
        "a sign-in recorded before"
    );
    for url in [
        "https://chatgpt.com/c/1",
        "https://chatgpt.com/api/auth/callback/openai?x",
    ] {
        model.send(Intent::PageShown {
            store: main.clone(),
            url: url.into(),
        });
    }
    model.run(&mut machine);
    close(&mut model, &main);
    assert_eq!(
        open(&mut model, &mut machine, &main).load.url,
        "https://chatgpt.com/c/1"
    );
    model.send(Intent::PageShown {
        store: claude("w"),
        url: "https://claude.ai/magic-link#someone".into(),
    });
    assert!(!machine.windows_records().pages.contains_key(&claude("w")));
}

/// A window's last page is kept for this Pitboard directory alone, and goes with what the
/// window keeps when that is removed, another directory's pages staying as they were.
///
/// StoreJanitorTests.swift's aWindowsLastPageIsKeptPerDirectoryUntilItsStoreGoes.
#[test]
fn a_windows_last_page_is_kept_per_directory_until_its_store_goes() {
    let work = claude("w");
    let mut machine = machine();
    let mut theirs = Records::default();
    theirs.set(
        SCRATCH,
        &Entry {
            stores: BTreeSet::from([work.clone()]),
            pages: BTreeMap::from([(work.clone(), "https://claude.ai/chat/theirs".to_owned())]),
            ..Entry::default()
        },
    );
    machine.windows_file = theirs.text();
    let mut model = started(&mut machine);
    open(&mut model, &mut machine, &work);
    model.send(Intent::PageShown {
        store: work.clone(),
        url: "https://claude.ai/chat/1".into(),
    });
    model.run(&mut machine);
    assert_eq!(
        machine
            .windows_records()
            .pages
            .get(&work)
            .map(String::as_str),
        Some("https://claude.ai/chat/1")
    );
    model.send(Intent::WebsiteDataRemoved {
        store: work.to_uppercase(),
    });
    model.run(&mut machine);
    assert!(machine.windows_records().pages.is_empty());
    let file = Records::read(machine.windows_file.as_deref().expect("written")).expect("read");
    assert_eq!(
        file.entry(SCRATCH).pages.get(&work).map(String::as_str),
        Some("https://claude.ai/chat/theirs"),
        "another directory's pages stay"
    );
}

// MARK: - After a read

/// A read that no longer lists an account, as the read after forgetting it in the app or
/// with `pitboard forget` in a terminal, closes its window and has its store deleted, and
/// nobody else's, and its last page goes with it. The store stays recorded until the app has
/// deleted it.
///
/// AccountWindowsTests.swift's forgettingAnAccountClosesItsWindowAndDeletesItsStore.
#[test]
fn forgetting_an_account_closes_its_window_and_deletes_its_store() {
    let (personal, spare) = (claude("p"), chatgpt("s"));
    let mut machine = machine();
    let mut model = started(&mut machine);
    open(&mut model, &mut machine, &personal);
    open(&mut model, &mut machine, &spare);
    model.send(Intent::PageShown {
        store: personal.clone(),
        url: "https://claude.ai/chat/x".into(),
    });
    model.run(&mut machine);

    machine.answer = Ok(status(without("personal")));
    model.refresh(&mut machine);
    let shown = model.shown().account_windows;
    assert_eq!(shown.closing, vec![personal.clone()]);
    let open: Vec<&str> = shown.open.iter().map(|w| w.store.as_str()).collect();
    assert_eq!(open, [spare.as_str()]);
    assert_eq!(deleting(&model), vec![personal.clone()]);
    let records = machine.windows_records();
    assert!(records.pages.is_empty(), "its last page goes with it");
    assert!(records.stores.contains(&personal), "recorded until deleted");

    close(&mut model, &personal);
    assert!(model.shown().account_windows.closing.is_empty());
    model.send(Intent::StoreDeleted {
        store: personal.to_uppercase(),
    });
    model.run(&mut machine);
    assert!(deleting(&model).is_empty());
    assert_eq!(
        machine.windows_records().stores,
        BTreeSet::from([spare.clone()])
    );
}

/// A read that failed says nothing about who was forgotten, and nor does what stands in for
/// it, the last numbers measured: nothing is deleted, not even a store no account derives,
/// and no window closes.
///
/// AccountWindowsTests.swift's aFailedReadDeletesNothing.
#[test]
fn a_failed_read_deletes_nothing() {
    let orphan = "00000000-0000-4000-8000-000000000001";
    let mut machine = Stub::reading(Err(refusal(
        "unreachable",
        "Anthropic could not be reached",
        Vec::new(),
    )));
    machine.offline = Ok(status(two_tools()[..2].to_vec()));
    machine.windows_file = Some(file(&[(DANA, &[orphan])]));
    let mut model = started(&mut machine);
    open(&mut model, &mut machine, &claude("w"));
    model.refresh(&mut machine);
    assert_eq!(model.count(checks), 0);
    assert!(deleting(&model).is_empty());
    assert!(model.shown().account_windows.closing.is_empty());
    assert!(machine.windows_records().stores.contains(orphan));
}

/// What the poll reads once the account index changes says who is enrolled even while the
/// reads that ask a service fail: `pitboard forget` in a terminal, on a Mac that cannot
/// reach Anthropic, closes the account's window and deletes its store all the same.
///
/// AccountWindowsTests.swift's forgettingOnTheCommandLineWhileReadsFailClosesTheWindow.
#[test]
fn forgetting_on_the_command_line_while_reads_fail_closes_the_window() {
    let personal = claude("p");
    let mut machine = machine();
    let mut model = started(&mut machine);
    open(&mut model, &mut machine, &personal);
    machine.answer = Err(refusal(
        "unreachable",
        "Anthropic could not be reached",
        Vec::new(),
    ));
    model.refresh(&mut machine);
    assert!(
        model.shown().account_windows.closing.is_empty(),
        "a failed read changes nothing"
    );

    machine.offline = Ok(status(without("personal")));
    machine.changed += 5;
    model.notice(&mut machine);
    assert_eq!(
        model.shown().account_windows.closing,
        vec![personal.clone()]
    );
    assert_eq!(deleting(&model), [personal]);
}

/// The same on a Mac that has not reached Anthropic since Pitboard opened, whose accounts
/// stand in for a read that failed: what the poll reads once the account index changes says
/// who is enrolled, and `pitboard forget` closes the window. What stands in deletes nothing.
///
/// AccountWindowsTests.swift's forgettingOnTheCommandLineWhileEveryReadHasFailedClosesTheWindow.
#[test]
fn forgetting_on_the_command_line_while_every_read_has_failed_closes_the_window() {
    let personal = claude("p");
    let mut machine = Stub::reading(Err(refusal(
        "unreachable",
        "Anthropic could not be reached",
        Vec::new(),
    )));
    machine.offline = Ok(status(two_tools()));
    let mut model = started(&mut machine);
    assert!(model.state.status.is_some(), "what stands in");
    open(&mut model, &mut machine, &personal);
    assert_eq!(model.count(checks), 0);

    machine.offline = Ok(status(without("personal")));
    machine.changed += 5;
    model.notice(&mut machine);
    assert_eq!(
        model.shown().account_windows.closing,
        vec![personal.clone()]
    );
    assert_eq!(deleting(&model), [personal]);
}

/// Only a read that says afresh who is enrolled puts anything away: newer numbers a session
/// recorded, taken onto what is shown, keep its accounts and delete nothing.
///
/// PitboardKitTests' aReadThatSaysWhoIsEnrolledIsToldOnce, which held AppModel.swift's
/// rule for which snapshots it told the windows of.
#[test]
fn only_a_read_that_says_who_is_enrolled_puts_anything_away() {
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[&claude("w"), &claude("gone")])]));
    let mut model = started(&mut machine);
    assert_eq!(model.count(checks), 1, "the first read puts away gone's");
    machine.offline = Ok(status(without("personal")));
    machine.readings += 5;
    model.notice(&mut machine);
    assert_eq!(model.count(checks), 1, "numbers alone");
    assert_eq!(
        model.state.status.as_ref().map(|s| s.accounts.len()),
        Some(4)
    );
}

/// What the poll reads once the account index changes says who is enrolled where nothing was
/// shown before it, every read having failed and nothing known to stand in: it is the same
/// index every read lists the accounts from. AppModel.swift's rule told the windows of no
/// snapshot before one that answered, so this read put nothing away there; the model acts on
/// it, as it acts on each read that says who is enrolled, which changes nothing where the
/// read before it said the same.
#[test]
fn the_polls_read_says_who_is_enrolled_where_nothing_was_shown_before_it() {
    let gone = claude("gone");
    let unreadable = || {
        Err(refusal(
            "state_unreadable",
            "could not read Pitboard's account list",
            Vec::new(),
        ))
    };
    let mut machine = Stub::reading(unreadable());
    machine.offline = unreadable();
    machine.windows_file = Some(file(&[(DANA, &[&claude("w"), &gone])]));
    let mut model = started(&mut machine);
    model.notice(&mut machine);
    assert!(model.state.status.is_none(), "nothing shown");
    assert_eq!(model.count(checks), 0);

    machine.offline = Ok(status(two_tools()));
    machine.changed += 5;
    model.notice(&mut machine);
    assert_eq!(deleting(&model), [gone]);
    model.notice(&mut machine);
    model.refresh(&mut machine);
    assert_eq!(model.count(checks), 1, "said again, nothing more is asked");
}

/// Only stores this Pitboard directory recorded, and no enrolled account derives, are
/// deleted: a store nobody recorded is never asked about.
///
/// StoreJanitorTests.swift's aSweepDeletesOnlyRecordedStoresNoAccountHas.
#[test]
fn a_sweep_deletes_only_recorded_stores_no_account_has() {
    let one = "00000000-0000-4000-8000-000000000001";
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[one, &claude("w")])]));
    let model = started(&mut machine);
    assert_eq!(
        model
            .asked
            .iter()
            .filter(|job| checks(job))
            .collect::<Vec<_>>(),
        [&Job::CheckShared {
            stores: vec![one.into()]
        }]
    );
    assert_eq!(deleting(&model), [one]);
}

/// A store something keeps holding is left recorded, and the next read asks for it again.
///
/// StoreJanitorTests.swift's aStoreStillHeldIsLeftForTheNextSweep, whose tries and pauses
/// stay the app's.
#[test]
fn a_store_still_held_is_left_for_the_next_sweep() {
    let one = "00000000-0000-4000-8000-000000000001";
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[one])]));
    let mut model = started(&mut machine);
    assert_eq!(deleting(&model), [one]);
    let first = model.shown().account_windows.deleting[0].ask;
    model.send(Intent::StoreHeld {
        store: one.to_uppercase(),
    });
    assert!(deleting(&model).is_empty());
    assert!(machine.windows_records().stores.contains(one));

    model.refresh(&mut machine);
    assert_eq!(deleting(&model), [one], "asked for again");
    assert!(
        model.shown().account_windows.deleting[0].ask > first,
        "as a new ask"
    );
    model.send(Intent::StoreDeleted { store: one.into() });
    model.run(&mut machine);
    assert!(machine.windows_records().stores.is_empty());
}

/// Two reads at once ask for a store once: a sweep claims what it puts away before it asks
/// anything, and a sweep meanwhile leaves it.
///
/// StoreJanitorTests.swift's twoSweepsAtOnceDeleteAStoreOnce.
#[test]
fn two_sweeps_at_once_delete_a_store_once() {
    let one = "00000000-0000-4000-8000-000000000001";
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[one])]));
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run_but(&mut machine, checks);
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, checks);
    assert_eq!(model.count(checks), 1);
    model.run(&mut machine);
    model.refresh(&mut machine);
    assert_eq!(model.count(checks), 1);
    assert_eq!(deleting(&model), [one]);
}

/// A release build run with another home shares the app's stores with the copy installed.
/// A store both directories recorded is the other one's to keep while it is there: a sweep
/// here only takes it off this directory's record.
///
/// StoreJanitorTests.swift's aStoreAnotherDirectoryRecordedIsNotDeleted.
#[test]
fn a_store_another_directory_recorded_is_not_deleted() {
    let one = "00000000-0000-4000-8000-000000000001";
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[one]), (SCRATCH, &[one])]));
    machine.directories = vec![DANA.into(), SCRATCH.into()];
    let model = started(&mut machine);
    assert!(deleting(&model).is_empty());
    assert!(machine.windows_records().stores.is_empty());
    let file = Records::read(machine.windows_file.as_deref().expect("written")).expect("read");
    assert_eq!(file.entry(SCRATCH).stores, BTreeSet::from([one.to_owned()]));
}

/// A directory that is gone, such as a test's scratch home, has no account using the store,
/// so its stale record does not keep the store on disk.
///
/// StoreJanitorTests.swift's aStoreRecordedOnlyByADirectoryThatIsGoneIsDeleted.
#[test]
fn a_store_recorded_only_by_a_directory_that_is_gone_is_deleted() {
    let one = "00000000-0000-4000-8000-000000000001";
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[one]), (SCRATCH, &[one])]));
    let model = started(&mut machine);
    assert_eq!(deleting(&model), [one]);
}

/// A window of an account enrolled again while its store is being deleted waits until the
/// app has said how that went, then opens on a store recorded afresh.
#[test]
fn a_window_waits_while_its_store_is_deleted() {
    let personal = claude("p");
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[&personal])]));
    machine.answer = Ok(status(without("personal")));
    let mut model = started(&mut machine);
    assert_eq!(deleting(&model), vec![personal.clone()]);
    machine.answer = Ok(status(two_tools()));
    model.refresh(&mut machine);
    model.send(Intent::WindowOpened {
        store: personal.clone(),
    });
    model.run(&mut machine);
    assert_eq!(open_window(&model, &personal), None);
    model.send(Intent::StoreDeleted {
        store: personal.clone(),
    });
    model.run(&mut machine);
    let window = open_window(&model, &personal).expect("open once deleted");
    assert_eq!(window.note, Some(WindowNoteKind::SignIn));
    assert!(machine.windows_records().stores.contains(&personal));
}

// WebView2 refuses a profile of a deleted one's name until its folder goes, as the browser
// process exits, so the app answers `StoreHeld` until then.
#[test]
fn a_window_waits_while_its_old_store_is_held() {
    let personal = claude("p");
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[&personal])]));
    machine.answer = Ok(status(without("personal")));
    let mut model = started_on(Os::Windows, &mut machine);
    assert_eq!(deleting(&model), vec![personal.clone()]);
    model.send(Intent::StoreHeld {
        store: personal.to_uppercase(),
    });
    machine.answer = Ok(status(two_tools()));
    model.refresh(&mut machine);
    assert_eq!(
        deleting(&model),
        vec![personal.clone()],
        "asked again, though enrolled again"
    );
    model.send(Intent::StoreHeld {
        store: personal.clone(),
    });
    model.send(Intent::WindowOpened {
        store: personal.clone(),
    });
    model.run(&mut machine);
    assert_eq!(open_window(&model, &personal), None);
    let shown = model.shown().account_windows;
    assert_eq!(shown.held, vec![personal.clone()]);
    assert!(shown.menus.iter().any(|menu| match menu {
        SiteMenu::One { window, .. } => window.store == personal,
        SiteMenu::Several { windows, .. } => windows.iter().any(|w| w.store == personal),
    }));
    let opens = JumpAsk::OpenWindow {
        store: personal.clone(),
    };
    let jumps = |model: &Hand| model.on_windows().jump_list.expect("read");
    assert!(
        jumps(&model).iter().any(|task| task.ask == opens),
        "the Jump List stays as it was"
    );

    model.refresh(&mut machine);
    assert_eq!(open_window(&model, &personal), None);
    model.send(Intent::StoreDeleted {
        store: personal.clone(),
    });
    model.run(&mut machine);
    assert!(model.shown().account_windows.held.is_empty());
    let window = open_window(&model, &personal).expect("open once deleted");
    assert_eq!(window.note, Some(WindowNoteKind::SignIn));
    assert!(jumps(&model).iter().any(|task| task.ask == opens));
}

#[test]
fn a_held_store_is_still_held_after_a_restart() {
    let personal = claude("p");
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[&personal])]));
    machine.answer = Ok(status(without("personal")));
    let mut model = started_on(Os::Windows, &mut machine);
    model.send(Intent::StoreHeld {
        store: personal.clone(),
    });
    model.run(&mut machine);

    machine.answer = Ok(status(two_tools()));
    let mut model = started_on(Os::Windows, &mut machine);
    assert_eq!(deleting(&model), vec![personal.clone()], "asked again");
    assert_eq!(model.shown().account_windows.held, vec![personal.clone()]);
    model.send(Intent::WindowOpened {
        store: personal.clone(),
    });
    model.run(&mut machine);
    assert_eq!(open_window(&model, &personal), None);
    model.send(Intent::StoreDeleted {
        store: personal.clone(),
    });
    model.run(&mut machine);
    let window = open_window(&model, &personal).expect("open once deleted");
    assert_eq!(window.note, Some(WindowNoteKind::SignIn));
    assert!(machine.windows_records().held.is_empty());
}

// WebView2 deletes a profile whoever recorded it, so that cannot keep the window waiting forever.
#[test]
fn a_held_store_another_directory_recorded_meanwhile_is_still_deleted() {
    let personal = claude("p");
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[&personal])]));
    machine.answer = Ok(status(without("personal")));
    let mut model = started_on(Os::Windows, &mut machine);
    model.send(Intent::StoreHeld {
        store: personal.clone(),
    });
    model.run(&mut machine);
    let mut records =
        Records::read(machine.windows_file.as_deref().expect("written")).expect("records");
    records.set(
        SCRATCH,
        &Entry {
            stores: BTreeSet::from([personal.clone()]),
            ..Entry::default()
        },
    );
    machine.windows_file = records.text();
    machine.directories = vec![DANA.into(), SCRATCH.into()];

    machine.answer = Ok(status(two_tools()));
    model.refresh(&mut machine);
    assert_eq!(deleting(&model), vec![personal.clone()]);
    model.send(Intent::WindowOpened {
        store: personal.clone(),
    });
    model.run(&mut machine);
    assert_eq!(open_window(&model, &personal), None);
    model.send(Intent::StoreDeleted {
        store: personal.clone(),
    });
    model.run(&mut machine);
    let window = open_window(&model, &personal).expect("open once deleted");
    assert_eq!(window.note, Some(WindowNoteKind::SignIn));
    assert!(model.shown().account_windows.held.is_empty());
}

// WebKit gave up deleting it, so the store still holds the sign-in its window opens on.
#[test]
fn a_mac_keeps_a_held_store_whose_account_is_enrolled_again() {
    let personal = claude("p");
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[&personal])]));
    machine.answer = Ok(status(without("personal")));
    let mut model = started_on(Os::MacOs, &mut machine);
    assert_eq!(deleting(&model), vec![personal.clone()]);
    model.send(Intent::StoreHeld {
        store: personal.to_uppercase(),
    });
    machine.answer = Ok(status(two_tools()));
    model.refresh(&mut machine);
    assert!(deleting(&model).is_empty(), "not asked again");
    let window = open(&mut model, &mut machine, &personal);
    assert_eq!(window.note, None);
    assert!(model.shown().account_windows.held.is_empty());
    assert!(machine.windows_records().stores.contains(&personal));
}

/// Nothing is put away before the records are read, and a read that lands first is acted on
/// once they are in: a store nobody derives goes then, and none sooner.
#[test]
fn a_read_before_the_records_are_in_is_acted_on_once_they_are() {
    let one = "00000000-0000-4000-8000-000000000001";
    let mut machine = machine();
    machine.windows_file = Some(file(&[(DANA, &[one, &claude("w")])]));
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run_but(&mut machine, |job| matches!(job, Job::LoadKept));
    assert!(model.state.status.is_some(), "the read landed first");
    assert_eq!(model.count(checks), 0);
    model.send(Intent::WindowOpened { store: claude("w") });
    assert_eq!(
        open_window(&model, &claude("w")),
        None,
        "nor opens a window"
    );

    model.run(&mut machine);
    assert_eq!(deleting(&model), [one]);
    assert_eq!(
        open_window(&model, &claude("w")).map(|window| window.note),
        Some(None),
        "recorded before"
    );
}

// MARK: - The records the macOS app kept

/// What the macOS app kept in UserDefaults, exactly as its `StoreRecord` and `PageRecord`
/// wrote it, each store in upper case as Foundation writes a UUID, is taken once and kept in
/// the app's file at once: no store an enrolled account derives is deleted, a window opens
/// where it was last and does not say how to sign in, and only a store whose account a read
/// no longer lists is deleted.
#[test]
fn todays_records_are_taken_once_and_nothing_enrolled_is_deleted() {
    let upper = |store: String| store.to_uppercase();
    let mut machine = machine();
    machine.windows_earlier = Some(Records::earlier(&EarlierWindowRecords {
        stores: HashMap::from([
            (
                DANA.to_owned(),
                vec![
                    upper(claude("p")),
                    upper(chatgpt("m")),
                    upper(claude("w")),
                    upper(chatgpt("s")),
                ],
            ),
            (SCRATCH.to_owned(), vec![upper(claude("w"))]),
        ]),
        pages: HashMap::from([(
            DANA.to_owned(),
            HashMap::from([(upper(claude("w")), "https://claude.ai/chat/1".to_owned())]),
        )]),
    }));
    let mut model = started(&mut machine);
    assert_eq!(
        model.count(checks),
        0,
        "every store is an enrolled account's"
    );
    assert!(deleting(&model).is_empty());
    assert_eq!(machine.kept_windows.len(), 1, "kept at once");
    let file = Records::read(machine.windows_file.as_deref().expect("written")).expect("read");
    assert_eq!(
        file.entry(DANA).stores,
        BTreeSet::from([claude("p"), chatgpt("m"), claude("w"), chatgpt("s")])
    );
    assert_eq!(
        file.entry(SCRATCH).stores,
        BTreeSet::from([claude("w")]),
        "the other directory's too"
    );

    let work = open(&mut model, &mut machine, &claude("w"));
    assert_eq!(work.load.url, "https://claude.ai/chat/1");
    assert_eq!(work.note, None, "its sign-in is there");

    machine.answer = Ok(status(without("spare")));
    model.refresh(&mut machine);
    assert_eq!(deleting(&model), [chatgpt("s")]);

    machine.windows_earlier = None;
    let relaunched = started(&mut machine);
    assert!(
        deleting(&relaunched).contains(&chatgpt("s")),
        "from the file now"
    );
    assert_eq!(machine.kept_windows.len(), 1, "not kept again");
}

/// A records file that is there and cannot be read is never written over, and nothing is
/// deleted by it; a window opens all the same, its store held for this launch.
#[test]
fn records_that_cannot_be_read_are_never_written_and_delete_nothing() {
    let mut machine = machine();
    machine.windows_unreadable = true;
    machine.answer = Ok(status(without("personal")));
    let mut model = started(&mut machine);
    let work = open(&mut model, &mut machine, &claude("w"));
    assert_eq!(work.note, Some(WindowNoteKind::SignIn));
    model.send(Intent::PageShown {
        store: claude("w"),
        url: "https://claude.ai/chat/1".into(),
    });
    model.refresh(&mut machine);
    assert!(machine.kept_windows.is_empty());
    assert_eq!(model.count(checks), 0);
    assert!(deleting(&model).is_empty());

    let mut nowhere = machine_kept_nowhere();
    let mut model = started(&mut nowhere);
    assert!(open(&mut model, &mut nowhere, &claude("w")).note.is_some());
    assert!(nowhere.kept_windows.is_empty());
}

/// A records file that is there and does not read as records, as a later version, a hand
/// edit or damage may leave it, is held as one that cannot be read. Taken as no file, the
/// first write left this directory's records alone in it, and the read after forgetting an
/// account deleted a store that another Pitboard directory, still there, recorded too, which
/// signed that directory's window out.
#[test]
fn records_that_do_not_read_as_records_are_never_written_and_delete_nothing() {
    let personal = claude("p");
    let damaged = format!(
        r#"{{"stores":{{"{DANA}":["{personal}"],"{SCRATCH}":["{personal}"]}},"pages":"a later format"}}"#
    );
    let mut machine = machine();
    machine.directories.push(SCRATCH.into());
    machine.windows_file = Some(damaged.clone());
    let mut model = started(&mut machine);
    let window = open(&mut model, &mut machine, &personal);
    assert_eq!(
        window.note,
        Some(WindowNoteKind::SignIn),
        "held for this launch"
    );
    machine.answer = Ok(status(without("personal")));
    model.refresh(&mut machine);
    assert_eq!(machine.windows_file.as_deref(), Some(damaged.as_str()));
    assert!(machine.kept_windows.is_empty());
    assert_eq!(model.count(checks), 0);
    assert!(deleting(&model).is_empty());
}

/// An app that names nowhere to keep the records.
fn machine_kept_nowhere() -> Stub {
    let mut machine = machine();
    machine.windows_nowhere = true;
    machine
}

/// What the model holds of the records is what it writes, and reads back the same after a
/// relaunch.
#[test]
fn what_is_kept_is_what_is_held() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    open(&mut model, &mut machine, &claude("w"));
    let held = model.state.windows.records().cloned();
    assert_eq!(held.as_ref(), Some(&machine.windows_records()));
    let relaunched = started(&mut machine);
    assert_eq!(relaunched.state.windows.records().cloned(), held);
}

// MARK: - Downloads

fn downloads(model: &Hand) -> Vec<crate::DownloadShown> {
    model.shown().account_windows.downloads
}

/// A download starts, is given its file, and ends once, as a browser's does: named by its
/// address until it has a file, then by the file; a second ending, or a file given once it
/// has ended, changes nothing. One that finished before it had a file is left as it was, as
/// the Swift left it. Downloads.swift's `Transfer`, which the Swift tested nowhere.
#[test]
fn a_download_goes_from_starting_to_its_end_once() {
    let mut model = Hand::new();
    let main = chatgpt("m");
    model.send(Intent::DownloadStarted {
        id: "1".into(),
        store: main.to_uppercase(),
        name: Some("notes.txt".into()),
    });
    let [one] = &downloads(&model)[..] else {
        panic!("one download");
    };
    assert_eq!(
        (one.name.as_str(), &one.state, one.running, &one.said),
        ("notes.txt", &DownloadState::Starting, true, &None)
    );
    assert_eq!(one.store, main);

    let file = "/Users/dana/Downloads/notes 2.txt";
    model.send(Intent::DownloadSaving {
        id: "1".into(),
        file: file.into(),
    });
    model.send(Intent::DownloadSaving {
        id: "1".into(),
        file: "/elsewhere".into(),
    });
    let one = &downloads(&model)[0];
    assert_eq!(one.name, "notes 2.txt");
    assert_eq!(one.state, DownloadState::Running { file: file.into() });

    model.send(Intent::DownloadEnded {
        id: "1".into(),
        end: DownloadEnd::Finished,
    });
    model.send(Intent::DownloadEnded {
        id: "1".into(),
        end: DownloadEnd::Cancelled,
    });
    let one = &downloads(&model)[0];
    assert_eq!(one.state, DownloadState::Finished { file: file.into() });
    assert_eq!(one.said.as_deref(), Some(words::DOWNLOADED));
    assert!(!one.running);

    model.send(Intent::DownloadStarted {
        id: "2".into(),
        store: main.clone(),
        name: None,
    });
    model.send(Intent::DownloadEnded {
        id: "2".into(),
        end: DownloadEnd::Finished,
    });
    let two = &downloads(&model)[0];
    assert_eq!(two.name, "Download", "newest first, named by default");
    assert_eq!(
        two.state,
        DownloadState::Starting,
        "no file to have finished in"
    );
    model.send(Intent::DownloadEnded {
        id: "2".into(),
        end: DownloadEnd::Cancelled,
    });
    model.send(Intent::DownloadSaving {
        id: "2".into(),
        file: file.into(),
    });
    let two = &downloads(&model)[0];
    assert_eq!(two.state, DownloadState::Cancelled);
    assert_eq!(two.said.as_deref(), Some(words::CANCELLED));

    model.send(Intent::DownloadStarted {
        id: "3".into(),
        store: claude("w"),
        name: Some("a".into()),
    });
    model.send(Intent::DownloadEnded {
        id: "3".into(),
        end: DownloadEnd::Failed {
            reason: "The network connection was lost.".into(),
        },
    });
    model.send(Intent::DownloadStarted {
        id: "4".into(),
        store: main.clone(),
        name: Some("b".into()),
    });
    assert_eq!(
        downloads(&model)[1].said.as_deref(),
        Some("The network connection was lost.")
    );

    model.send(Intent::ClearDownloads { store: main });
    let left: Vec<String> = downloads(&model)
        .into_iter()
        .map(|download| download.id)
        .collect();
    assert_eq!(left, ["4", "3"], "the running one, and another window's");
}

/// Each open window's list of downloads clears the ones that have ended with a button the
/// model words beside the intent it sends, held back until one of the window's own has ended,
/// since before then there is nothing for it to clear. The Swift worded it in
/// AccountWindowView.swift, and held it back there while every download in the list ran.
#[test]
fn an_open_window_offers_to_clear_its_ended_downloads() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    let work = claude("w");
    open(&mut model, &mut machine, &work);
    let clear = |model: &Hand| {
        model
            .on_a_mac()
            .account_windows
            .open
            .into_iter()
            .find(|window| window.store == work)
            .expect("the window open")
            .clear_downloads
    };
    assert_eq!(
        clear(&model),
        Some(Choice {
            title: words::CLEAR.into(),
            intent: Intent::ClearDownloads {
                store: work.clone()
            },
            enabled: false,
        }),
        "nothing in its list"
    );
    let clears = |model: &Hand| clear(model).expect("a Clear").enabled;
    for (id, store) in [("1", work.to_uppercase()), ("2", chatgpt("m"))] {
        model.send(Intent::DownloadStarted {
            id: id.into(),
            store,
            name: Some(format!("{id}.txt")),
        });
    }
    assert!(!clears(&model), "its one download under way");
    model.send(Intent::DownloadEnded {
        id: "2".into(),
        end: DownloadEnd::Cancelled,
    });
    assert!(!clears(&model), "another window's ended");
    model.send(Intent::DownloadEnded {
        id: "1".into(),
        end: DownloadEnd::Failed {
            reason: "The network connection was lost.".into(),
        },
    });
    assert!(clears(&model), "its own ended");
    model.send(Intent::ClearDownloads {
        store: work.clone(),
    });
    assert!(!clears(&model), "cleared");
}

// WebView2's own download list has no Clear for the Windows app to send.
#[test]
fn a_window_on_windows_offers_no_clear_of_its_downloads() {
    let mut machine = machine();
    let mut model = started(&mut machine);
    let work = claude("w");
    open(&mut model, &mut machine, &work);
    model.send(Intent::DownloadStarted {
        id: "1".into(),
        store: work.clone(),
        name: Some("1.txt".into()),
    });
    model.send(Intent::DownloadEnded {
        id: "1".into(),
        end: DownloadEnd::Cancelled,
    });

    let windows = model.on_windows().account_windows;

    assert_eq!(windows.downloads.len(), 1, "its list still has it");
    assert_eq!(windows.open[0].clear_downloads, None);
}

/// Quitting stops every download under way, so it asks first, as Safari does, saying how
/// many, and nothing while none is. The app counts them itself and asks for the words: one
/// started a moment before Quit is under way before any snapshot lists it, and a question in
/// the snapshot would let Pitboard quit without asking. AppDelegate.swift's alert, which the
/// Swift tested nowhere.
#[test]
fn quitting_while_downloads_run_asks_first() {
    let question = |running| {
        let (title, message, confirm) = words::quit_with_downloads(OS, running);
        Question::asking(title, message.into(), confirm.into())
    };
    assert_eq!(downloads_quit_question(0), None);
    assert_eq!(downloads_quit_question(1), Some(question(1)));
    assert_eq!(downloads_quit_question(2), Some(question(2)));
}
