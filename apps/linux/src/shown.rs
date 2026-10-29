//! The snapshot the app shows, kept by part, by the rule every app keeps (A-api, Change flags): a
//! snapshot not newer than the one shown is dropped; one whose `changed.since` is the one shown
//! updates only the parts it names, and any other updates every part. No part is compared here.
//!
//! A view follows the parts it shows through the store's `changed` signal, detailed by the
//! part's name, as GObject's `notify` is by a property's. The accounts and the notices are also
//! list models of rows kept by their id, so a row is the same object for as long as its id is
//! shown. As a part they name changes, each row's value is compared with the one it held, and a
//! row whose value differs says so.

use std::any::Any;
use std::cell::{Ref, RefCell};

use gio::prelude::*;
use glib::subclass::prelude::*;
use pitboard_model::{AccountItem, PanelNotice, Part, Snapshot};

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use std::cell::OnceCell;
    use std::sync::OnceLock;

    #[derive(Default)]
    pub struct Shown {
        pub(super) snapshot: OnceCell<RefCell<Snapshot>>,
        pub(super) accounts: OnceCell<gio::ListStore>,
        pub(super) notices: OnceCell<gio::ListStore>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Shown {
        const NAME: &'static str = "PitboardShown";
        type Type = super::Shown;
    }

    impl ObjectImpl for Shown {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("changed").detailed().build()])
        }
    }

    #[derive(Default)]
    pub struct Row {
        pub(super) id: OnceCell<String>,
        pub(super) value: RefCell<Option<Box<dyn Any>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Row {
        const NAME: &'static str = "PitboardShownRow";
        type Type = super::Row;
    }

    impl ObjectImpl for Row {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("changed").build()])
        }
    }
}

glib::wrapper! {
    pub struct Shown(ObjectSubclass<imp::Shown>);
}

glib::wrapper! {
    /// A row of one of the store's lists: the value of one account or notice, by its id.
    pub struct Row(ObjectSubclass<imp::Row>);
}

impl Shown {
    /// The store, showing `first` whole, as `PitboardModel::snapshot` gives it.
    pub fn new(first: Snapshot) -> Shown {
        let shown: Shown = glib::Object::new();
        let imp = shown.imp();
        let (accounts_list, notices_list) =
            (gio::ListStore::new::<Row>(), gio::ListStore::new::<Row>());
        follow(&accounts_list, &accounts(&first), |a| &a.id);
        follow(&notices_list, &notices(&first), |n| &n.id);
        let _ = imp.accounts.set(accounts_list);
        let _ = imp.notices.set(notices_list);
        let _ = imp.snapshot.set(RefCell::new(first));
        shown
    }

    pub fn snapshot(&self) -> Ref<'_, Snapshot> {
        self.imp()
            .snapshot
            .get()
            .expect("made with a snapshot")
            .borrow()
    }

    /// Every account shown, in the order of the sections and of each section's accounts: rows
    /// whose value is an `AccountItem`.
    pub fn accounts(&self) -> gio::ListStore {
        self.imp()
            .accounts
            .get()
            .expect("made with a snapshot")
            .clone()
    }

    /// The notices, the most pressing first: rows whose value is a `PanelNotice`.
    pub fn notices(&self) -> gio::ListStore {
        self.imp()
            .notices
            .get()
            .expect("made with a snapshot")
            .clone()
    }

    /// Takes `snapshot` by the rule, and says each part it updated, in the order of the
    /// snapshot's fields, once every one is: none for a snapshot it dropped.
    pub fn take(&self, snapshot: Snapshot) -> Vec<Part> {
        let parts = {
            let mut shown = self.imp().snapshot.get().expect("made").borrow_mut();
            if snapshot.revision <= shown.revision {
                return Vec::new();
            }
            let parts = if snapshot.changed.since == shown.revision {
                snapshot.changed.parts.clone()
            } else {
                pitboard_model::every_part()
            };
            for part in &parts {
                assign(&mut shown, &snapshot, *part);
            }
            shown.revision = snapshot.revision;
            shown.changed = snapshot.changed;
            parts
        };
        if parts.contains(&Part::Sections) {
            follow(&self.accounts(), &accounts(&self.snapshot()), |a| &a.id);
        }
        if parts.contains(&Part::Notices) {
            follow(&self.notices(), &notices(&self.snapshot()), |n| &n.id);
        }
        for part in &parts {
            self.emit_by_name_with_details::<()>(
                "changed",
                glib::Quark::from_str(name(*part)),
                &[],
            );
        }
        parts
    }

    /// Calls `changed` each time `part` is updated.
    pub fn connect_changed(
        &self,
        part: Part,
        changed: impl Fn(&Shown) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            &format!("changed::{}", name(part)),
            false,
            glib::closure_local!(move |shown: &Shown| changed(shown)),
        )
    }
}

impl Row {
    fn new(id: &str, value: Box<dyn Any>) -> Row {
        let row: Row = glib::Object::new();
        let _ = row.imp().id.set(id.to_owned());
        row.imp().value.replace(Some(value));
        row
    }

    pub fn id(&self) -> &str {
        self.imp().id.get().expect("made with an id")
    }

    /// Its value: an `AccountItem` in the accounts, a `PanelNotice` in the notices.
    pub fn value<T: 'static>(&self) -> Ref<'_, T> {
        Ref::map(self.imp().value.borrow(), |value| {
            value
                .as_ref()
                .and_then(|value| value.downcast_ref())
                .expect("a row holds the value of its list")
        })
    }

    pub fn account(&self) -> Ref<'_, AccountItem> {
        self.value()
    }

    pub fn notice(&self) -> Ref<'_, PanelNotice> {
        self.value()
    }

    /// Calls `changed` each time its value changes while its id is shown.
    pub fn connect_changed(&self, changed: impl Fn(&Row) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            false,
            glib::closure_local!(move |row: &Row| changed(row)),
        )
    }
}

fn accounts(snapshot: &Snapshot) -> Vec<&AccountItem> {
    (snapshot.sections.iter())
        .flat_map(|section| &section.accounts)
        .collect()
}

fn notices(snapshot: &Snapshot) -> Vec<&PanelNotice> {
    snapshot.notices.iter().collect()
}

// The rows of `list` made to hold `values` in their order, each value's row kept by its id: a
// value that changed is told to its row, and the list is changed only where its ids are.
fn follow<T: Clone + PartialEq + 'static>(
    list: &gio::ListStore,
    values: &[&T],
    id: impl Fn(&T) -> &String,
) {
    let kept: Vec<Row> = list
        .iter::<Row>()
        .map(|row| row.expect("rows alone"))
        .collect();
    let rows: Vec<Row> = values
        .iter()
        .map(|value| {
            let id = id(value);
            match kept.iter().find(|row| row.id() == id) {
                Some(row) => {
                    if *row.value::<T>() != **value {
                        row.imp().value.replace(Some(Box::new((*value).clone())));
                        row.emit_by_name::<()>("changed", &[]);
                    }
                    row.clone()
                }
                None => Row::new(id, Box::new((*value).clone())),
            }
        })
        .collect();
    // Only the stretch between what stayed at either end is spliced.
    let same = |a: &Row, b: &Row| a == b;
    let front = kept
        .iter()
        .zip(&rows)
        .take_while(|(a, b)| same(a, b))
        .count();
    let back = kept[front..]
        .iter()
        .rev()
        .zip(rows[front..].iter().rev())
        .take_while(|(a, b)| same(a, b))
        .count();
    let (removed, added) = (kept.len() - front - back, &rows[front..rows.len() - back]);
    if removed > 0 || !added.is_empty() {
        let position = u32::try_from(front).expect("a list of fewer rows than u32 counts");
        let removed = u32::try_from(removed).expect("as above");
        list.splice(position, removed, added);
    }
}

// The detail a part's change is told under: its field's name, in the case a signal's detail is.
fn name(part: Part) -> &'static str {
    match part {
        Part::Reading => "reading",
        Part::QuitQuestion => "quit-question",
        Part::ReplaceQuestion => "replace-question",
        Part::WindowRequest => "window-request",
        Part::SigningIn => "signing-in",
        Part::Sheet => "sheet",
        Part::SheetFailure => "sheet-failure",
        Part::StatusItem => "status-item",
        Part::Sections => "sections",
        Part::AccountMenu => "account-menu",
        Part::Notices => "notices",
        Part::MenuNotices => "menu-notices",
        Part::Footing => "footing",
        Part::Setup => "setup",
        Part::AccountsShown => "accounts-shown",
        Part::MenuAccountsNote => "menu-accounts-note",
        Part::UpdatedAt => "updated-at",
        Part::UpdatedMenu => "updated-menu",
        Part::UpdatedWindow => "updated-window",
        Part::SheetText => "sheet-text",
        Part::SigningInText => "signing-in-text",
        Part::StowText => "stow-text",
        Part::FailureAlert => "failure-alert",
        Part::Machine => "machine",
        Part::AccountWindows => "account-windows",
        Part::Uninstall => "uninstall",
        Part::Removal => "removal",
        Part::Welcome => "welcome",
        Part::WidgetRevision => "widget-revision",
        Part::Answers => "answers",
        Part::QuickActions => "quick-actions",
    }
}

// The one place a part's field is named, as the macOS app's `AppModel.assign` names each.
fn assign(shown: &mut Snapshot, told: &Snapshot, part: Part) {
    match part {
        Part::Reading => shown.reading = told.reading,
        Part::QuitQuestion => shown.quit_question.clone_from(&told.quit_question),
        Part::ReplaceQuestion => shown.replace_question.clone_from(&told.replace_question),
        Part::WindowRequest => shown.window_request = told.window_request,
        Part::SigningIn => shown.signing_in.clone_from(&told.signing_in),
        Part::Sheet => shown.sheet.clone_from(&told.sheet),
        Part::SheetFailure => shown.sheet_failure.clone_from(&told.sheet_failure),
        Part::StatusItem => shown.status_item.clone_from(&told.status_item),
        Part::Sections => shown.sections.clone_from(&told.sections),
        Part::AccountMenu => shown.account_menu.clone_from(&told.account_menu),
        Part::Notices => shown.notices.clone_from(&told.notices),
        Part::MenuNotices => shown.menu_notices.clone_from(&told.menu_notices),
        Part::Footing => shown.footing.clone_from(&told.footing),
        Part::Setup => shown.setup.clone_from(&told.setup),
        Part::AccountsShown => shown.accounts_shown.clone_from(&told.accounts_shown),
        Part::MenuAccountsNote => shown
            .menu_accounts_note
            .clone_from(&told.menu_accounts_note),
        Part::UpdatedAt => shown.updated_at = told.updated_at,
        Part::UpdatedMenu => shown.updated_menu.clone_from(&told.updated_menu),
        Part::UpdatedWindow => shown.updated_window.clone_from(&told.updated_window),
        Part::SheetText => shown.sheet_text.clone_from(&told.sheet_text),
        Part::SigningInText => shown.signing_in_text.clone_from(&told.signing_in_text),
        Part::StowText => shown.stow_text.clone_from(&told.stow_text),
        Part::FailureAlert => shown.failure_alert.clone_from(&told.failure_alert),
        Part::Machine => shown.machine.clone_from(&told.machine),
        Part::AccountWindows => shown.account_windows.clone_from(&told.account_windows),
        Part::Uninstall => shown.uninstall.clone_from(&told.uninstall),
        Part::Removal => shown.removal.clone_from(&told.removal),
        Part::Welcome => shown.welcome.clone_from(&told.welcome),
        Part::WidgetRevision => shown.widget_revision = told.widget_revision,
        Part::Answers => shown.answers.clone_from(&told.answers),
        Part::QuickActions => shown.quick_actions.clone_from(&told.quick_actions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::snapshot;
    use pitboard_model::{Changed, Severity};
    use std::rc::Rc;

    fn notice(id: &str, title: &str) -> PanelNotice {
        PanelNotice {
            id: id.into(),
            severity: Severity::Warning,
            spoken_severity: "Warning".into(),
            title: title.into(),
            lines: vec![],
            until: None,
            until_label: None,
            actions: vec![],
            learn: None,
        }
    }

    fn since(snapshot: Snapshot, since: u64, parts: &[Part]) -> Snapshot {
        Snapshot {
            changed: Changed {
                since,
                parts: parts.to_vec(),
            },
            ..snapshot
        }
    }

    // Each part told, by its detail, in the order told.
    fn told(shown: &Shown) -> Rc<RefCell<Vec<Part>>> {
        let told = Rc::new(RefCell::new(Vec::new()));
        for part in pitboard_model::every_part() {
            let told = told.clone();
            shown.connect_changed(part, move |_| told.borrow_mut().push(part));
        }
        told
    }

    #[test]
    fn a_snapshot_based_on_the_one_shown_updates_only_its_parts() {
        let first = snapshot(1);
        let shown = Shown::new(first.clone());
        let told = told(&shown);
        let reading = !first.reading;

        let next = Snapshot {
            reading,
            updated_window: "Updated 5:26 AM".into(),
            ..since(snapshot(2), 1, &[Part::Reading])
        };
        assert_eq!(shown.take(next), [Part::Reading]);

        assert_eq!(shown.snapshot().revision, 2);
        assert_eq!(shown.snapshot().reading, reading);
        assert_eq!(
            shown.snapshot().updated_window,
            first.updated_window,
            "a part the snapshot does not name stays as shown"
        );
        assert_eq!(*told.borrow(), [Part::Reading]);

        // Based on one never shown, so every part is shown again.
        let other = Snapshot {
            updated_window: "Updated 5:27 AM".into(),
            ..since(snapshot(4), 3, &[Part::Reading])
        };
        told.borrow_mut().clear();
        assert_eq!(shown.take(other), pitboard_model::every_part());
        assert_eq!(shown.snapshot().updated_window, "Updated 5:27 AM");
        assert_eq!(*told.borrow(), pitboard_model::every_part());
    }

    #[test]
    fn a_snapshot_not_newer_is_dropped() {
        let shown = Shown::new(snapshot(5));
        let told = told(&shown);

        for revision in [5, 4, 0] {
            let older = Snapshot {
                updated_window: "Updated 5:26 AM".into(),
                ..since(snapshot(revision), 4, &[Part::UpdatedWindow])
            };
            assert!(shown.take(older).is_empty(), "{revision}");
        }

        assert_eq!(shown.snapshot().revision, 5);
        assert_eq!(shown.snapshot().updated_window, snapshot(5).updated_window);
        assert!(told.borrow().is_empty());
    }

    #[test]
    fn a_row_keeps_its_object_across_snapshots() {
        let with = |revision, notices: Vec<PanelNotice>| Snapshot {
            notices,
            ..since(snapshot(revision), revision - 1, &[Part::Notices])
        };
        let shown = Shown::new(with(
            1,
            vec![
                notice("read", "Couldn’t read"),
                notice("switch/codex", "Switched"),
            ],
        ));
        let list = shown.notices();
        let rows = |list: &gio::ListStore| -> Vec<Row> {
            list.iter::<Row>().map(Result::unwrap).collect()
        };
        let before = rows(&list);
        let changed = Rc::new(RefCell::new(Vec::new()));
        for row in &before {
            let changed = changed.clone();
            row.connect_changed(move |row| changed.borrow_mut().push(row.id().to_owned()));
        }
        let spliced = Rc::new(RefCell::new(Vec::new()));
        list.connect_items_changed({
            let spliced = spliced.clone();
            move |_, at, removed, added| spliced.borrow_mut().push((at, removed, added))
        });

        shown.take(with(
            2,
            vec![
                notice("stuck", "Interrupted"),
                notice("read", "Couldn’t read"),
                notice("switch/codex", "Switched to work"),
            ],
        ));

        let after = rows(&list);
        assert_eq!(
            after.iter().map(Row::id).collect::<Vec<_>>(),
            ["stuck", "read", "switch/codex"]
        );
        assert_eq!(after[1], before[0], "the same object while its id is shown");
        assert_eq!(after[2], before[1]);
        assert_eq!(after[2].notice().title, "Switched to work");
        assert_eq!(
            *changed.borrow(),
            ["switch/codex"],
            "only a value that changed is told"
        );
        assert_eq!(*spliced.borrow(), [(0, 0, 1)], "only the new row is added");

        shown.take(with(3, vec![notice("switch/codex", "Switched to work")]));
        assert_eq!(rows(&list), [before[1].clone()]);
        assert_eq!(*spliced.borrow(), [(0, 0, 1), (0, 2, 0)]);
    }
}
