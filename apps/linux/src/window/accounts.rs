//! The accounts page's list: the notices and the setup step's tip above a group per account, in
//! the order of the sections and of their accounts. A group is kept for as long as its account
//! is shown, so its buttons and its menu keep their place. An `AdwPreferencesPage` adds a group
//! only at its end (libadwaita 1.7), so where an account comes in between others, the groups
//! after it are added again behind it.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
use pitboard_model::{AccountSection, Part, Snapshot};

use super::Host;
use super::account_group::AccountGroup;
use super::notices::{self, NoticeRow};
use crate::shown::{Row, Shown};

pub(super) struct Accounts {
    page: adw::PreferencesPage,
    host: Rc<dyn Host>,
    tip: NoticeRow,
    tip_group: adw::PreferencesGroup,
    groups: RefCell<Vec<(Row, AccountGroup)>>,
}

impl Accounts {
    /// The list on `page`, its notices in the list of their group, shown while there are
    /// any, and its tip in `tip_group`, following what `shown` shows.
    pub(super) fn follow(
        page: &adw::PreferencesPage,
        (notices_group, notices): (&adw::PreferencesGroup, &gtk::ListBox),
        tip_group: &adw::PreferencesGroup,
        shown: &Shown,
        host: Rc<dyn Host>,
    ) -> Rc<Accounts> {
        let store = shown.notices();
        notices::bind(notices, &store, host.clone());
        let group = notices_group.clone();
        let listed = move |store: &gio::ListStore| group.set_visible(store.n_items() > 0);
        listed(&store);
        store.connect_items_changed(move |store, _, _, _| listed(store));
        let tip = NoticeRow::new(host.clone());
        tip_group.add(&tip);
        let accounts = Rc::new(Accounts {
            page: page.clone(),
            host,
            tip,
            tip_group: tip_group.clone(),
            groups: RefCell::default(),
        });
        accounts.list(shown);
        accounts.show_tip(&shown.snapshot());
        let weak = Rc::downgrade(&accounts);
        shown.connect_changed(Part::Sections, move |shown| {
            if let Some(accounts) = weak.upgrade() {
                accounts.list(shown);
            }
        });
        let weak = Rc::downgrade(&accounts);
        shown.connect_changed(Part::Setup, move |shown| {
            if let Some(accounts) = weak.upgrade() {
                accounts.show_tip(&shown.snapshot());
            }
        });
        accounts
    }

    fn show_tip(&self, snapshot: &Snapshot) {
        match &snapshot.setup {
            Some(step) => {
                self.tip.show_tip(step);
                self.tip_group.set_visible(true);
            }
            None => self.tip_group.set_visible(false),
        }
    }

    fn list(&self, shown: &Shown) {
        let rows: Vec<Row> = (shown.accounts().iter::<Row>())
            .map(|row| row.expect("rows alone"))
            .collect();
        let kept = self.groups.take();
        let groups: Vec<(Row, AccountGroup)> = (rows.into_iter())
            .map(|row| {
                let group = (kept.iter())
                    .find(|(kept, _)| *kept == row)
                    .map(|(_, group)| group.clone())
                    .unwrap_or_else(|| AccountGroup::new(self.host.clone()));
                (row, group)
            })
            .collect();
        let same = |a: &[(Row, AccountGroup)], b: &[(Row, AccountGroup)]| {
            a.iter()
                .map(|(row, _)| row)
                .eq(b.iter().map(|(row, _)| row))
        };
        if !same(&kept, &groups) {
            let stays = (kept.iter())
                .zip(&groups)
                .take_while(|((a, _), (b, _))| a == b)
                .count();
            for (_, group) in &kept[stays..] {
                self.page.remove(group);
            }
            for (_, group) in &groups[stays..] {
                self.page.add(group);
            }
        }
        let snapshot = shown.snapshot();
        for (row, group) in &groups {
            let item = row.account();
            if let Some(section) = section_of(&snapshot.sections, &item.id) {
                group.show(section, &item);
            }
        }
        self.groups.replace(groups);
    }
}

fn section_of<'a>(sections: &'a [AccountSection], id: &str) -> Option<&'a AccountSection> {
    (sections.iter()).find(|section| section.accounts.iter().any(|item| item.id == id))
}
