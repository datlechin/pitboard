//! One account, as a group of the accounts page: its name, the tool it is for and its plan, and
//! at the end of its header that it is in use, that a switch to it is under way, or its button,
//! and a menu of what else it offers. Its rows are each of its limits, its email address, what
//! needs doing about it, its sign-in and its warm-up.
//!
//! What the menu offers is the group's own actions under `account.`, one for each kind of
//! action the model offers, dimmed while the model holds it back. An action the model asks a
//! question for asks it first. A row is made again only when what the rows are changes, so a
//! button in one keeps the focus while its words move.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use pitboard_model::{
    AccountAction, AccountDetail, AccountItem, AccountNote, AccountSection, ActionKind, Front,
    Intent, RowEnd,
};

use super::Host;
use super::limit_row::LimitRow;
use crate::dialogs::question;

const COPY_EMAIL: &str = "account.copy-email";

// The group's action for each kind of action, where the Linux app offers it.
fn action_name(kind: ActionKind) -> Option<&'static str> {
    match kind {
        ActionKind::Switch => Some("account.switch"),
        ActionKind::SignInAgain => Some("account.sign-in-again"),
        ActionKind::Add => Some("account.add"),
        ActionKind::Rename => Some("account.rename"),
        ActionKind::WarmUpNow => Some("account.warm-up-now"),
        ActionKind::Remove => Some("account.remove"),
        // No account windows until WebKitGTK 2.56 (docs/facts/linux-app.md, Account windows),
        // and the model offers none.
        ActionKind::OpenWindow => None,
    }
}

const KINDS: [ActionKind; 7] = [
    ActionKind::Switch,
    ActionKind::SignInAgain,
    ActionKind::Add,
    ActionKind::Rename,
    ActionKind::WarmUpNow,
    ActionKind::OpenWindow,
    ActionKind::Remove,
];

// The menu's entries, each its words and its action, in its three sections.
type Menu = [Vec<(String, &'static str)>; 3];

// What the rows under the header are made of, which they are made again for once it changes.
#[derive(Clone, PartialEq)]
struct Details {
    email: Option<String>,
    notes: Vec<AccountNote>,
    sign_in: Option<String>,
    warm: Option<(String, pitboard_model::Choice)>,
    offered: Option<Intent>,
}

impl Details {
    fn of(item: &AccountItem) -> Details {
        let AccountDetail {
            email,
            sign_in,
            warm,
            ..
        } = &item.detail;
        Details {
            email: email.clone(),
            notes: item.notes.clone(),
            sign_in: sign_in.clone(),
            warm: (warm.as_ref()).map(|warm| (warm.said.clone(), warm.settings.clone())),
            offered: offered(item),
        }
    }
}

// What the header's own button sends, where it shows one.
fn offered(item: &AccountItem) -> Option<Intent> {
    match &item.end {
        Some(RowEnd::Button { action }) => Some(action.intent.clone()),
        _ => None,
    }
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/account-group.ui")]
    pub struct AccountGroup {
        #[template_child]
        pub(super) in_use: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) in_use_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) switching: TemplateChild<adw::Spinner>,
        #[template_child]
        pub(super) action: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) more: TemplateChild<gtk::MenuButton>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) item: RefCell<Option<AccountItem>>,
        pub(super) limits: RefCell<Vec<LimitRow>>,
        pub(super) rows: RefCell<Vec<gtk::Widget>>,
        pub(super) details: RefCell<Option<Details>>,
        pub(super) menu: RefCell<Option<Menu>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AccountGroup {
        const NAME: &'static str = "PitboardAccountGroup";
        type Type = super::AccountGroup;
        type ParentType = adw::PreferencesGroup;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
            for kind in KINDS {
                if let Some(name) = action_name(kind) {
                    class.install_action(name, None, move |group, _, _| group.act(kind));
                }
            }
            class.install_action(COPY_EMAIL, None, |group, _, _| group.copy_email());
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AccountGroup {
        fn constructed(&self) {
            self.parent_constructed();
            let words = pitboard_model::app_words(Front::Gnome).menu;
            self.more.set_tooltip_text(Some(&words.account));
            self.more
                .update_property(&[gtk::accessible::Property::Label(&words.account)]);
            let group = self.obj().downgrade();
            self.action.connect_clicked(move |_| {
                if let Some(group) = group.upgrade()
                    && let (Some(host), Some(intent)) = (group.host(), group.offered())
                {
                    host.send(intent);
                }
            });
        }
    }

    impl WidgetImpl for AccountGroup {}
    impl PreferencesGroupImpl for AccountGroup {}
}

glib::wrapper! {
    pub struct AccountGroup(ObjectSubclass<imp::AccountGroup>)
        @extends adw::PreferencesGroup, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AccountGroup {
    pub fn new(host: Rc<dyn Host>) -> AccountGroup {
        let group: AccountGroup = glib::Object::new();
        let _ = group.imp().host.set(host);
        group
    }

    fn host(&self) -> Option<Rc<dyn Host>> {
        self.imp().host.get().cloned()
    }

    fn offered(&self) -> Option<Intent> {
        self.imp().item.borrow().as_ref().and_then(offered)
    }

    /// Shows `item`, one of `section`'s accounts.
    pub fn show(&self, section: &AccountSection, item: &AccountItem) {
        let imp = self.imp();
        // A group's title and description are markup.
        self.set_title(&glib::markup_escape_text(&item.title));
        let description = pitboard_model::account_description(section, item);
        self.set_description(
            description
                .map(|said| glib::markup_escape_text(&said))
                .as_deref(),
        );
        imp.in_use.set_visible(false);
        imp.switching.set_visible(false);
        imp.action.set_visible(false);
        match &item.end {
            Some(RowEnd::InUse { title }) => {
                imp.in_use_label.set_label(title);
                imp.in_use.set_visible(true);
            }
            Some(RowEnd::Switching { spoken }) => {
                // libadwaita's binding names no Accessible for its spinner, which a widget is.
                (imp.switching.upcast_ref::<gtk::Widget>())
                    .update_property(&[gtk::accessible::Property::Label(spoken)]);
                imp.switching.set_visible(true);
            }
            Some(RowEnd::Button { action }) => {
                imp.action.set_label(&action.title);
                // A button is labelled by its label, which would name it "Switch" alone.
                imp.action
                    .reset_relation(gtk::AccessibleRelation::LabelledBy);
                imp.action
                    .update_property(&[gtk::accessible::Property::Label(&action.spoken)]);
                imp.action.set_visible(true);
            }
            None => {}
        }
        imp.item.replace(Some(item.clone()));
        self.offer(item);
        self.fill(item);
    }

    // The menu of what it offers but the header's button, each dimmed while held back: what it
    // does to the account, copying its address, then removing it. Made again only when what it
    // lists changes, since a menu given anew closes the one open.
    fn offer(&self, item: &AccountItem) {
        let words = pitboard_model::app_words(Front::Gnome).menu;
        let mut sections: [Vec<(String, &'static str)>; 3] = Default::default();
        let offered = offered(item);
        for kind in KINDS {
            let Some(name) = action_name(kind) else {
                continue;
            };
            let action = self.action_of(item, kind);
            self.action_set_enabled(name, action.is_some_and(|action| action.enabled));
            let Some(action) = action else {
                continue;
            };
            if action.intent.is_some() && action.intent == offered {
                continue;
            }
            let section = if kind == ActionKind::Remove { 2 } else { 0 };
            sections[section].push((action.title.clone(), name));
        }
        self.action_set_enabled(COPY_EMAIL, !item.email.is_empty());
        if !item.email.is_empty() {
            sections[1].push((words.copy_email, COPY_EMAIL));
        }
        let imp = self.imp();
        if imp.menu.borrow().as_ref() == Some(&sections) {
            return;
        }
        let menu = gio::Menu::new();
        for entries in sections.iter().filter(|entries| !entries.is_empty()) {
            let section = gio::Menu::new();
            for (title, name) in entries {
                section.append(Some(title), Some(name));
            }
            menu.append_section(None, &section);
        }
        imp.more.set_visible(menu.n_items() > 0);
        imp.more.set_menu_model(Some(&menu));
        imp.menu.replace(Some(sections));
    }

    fn action_of<'a>(&self, item: &'a AccountItem, kind: ActionKind) -> Option<&'a AccountAction> {
        item.actions.iter().find(|action| action.kind == kind)
    }

    fn act(&self, kind: ActionKind) {
        let item = self.imp().item.borrow().clone();
        let Some(host) = self.host() else {
            return;
        };
        let Some(action) = item.as_ref().and_then(|item| self.action_of(item, kind)) else {
            return;
        };
        if let Some(intent) = action.intent.clone() {
            question::asking_first(self, action.confirm.as_ref(), move || {
                host.send(intent.clone());
            });
        }
    }

    fn copy_email(&self) {
        if let Some(item) = self.imp().item.borrow().as_ref() {
            self.clipboard().set_text(&item.email);
        }
    }

    // Each limit's row, then the details' rows. The limits' rows are shown again in place while
    // there are as many, and the details' are made again only once they change.
    fn fill(&self, item: &AccountItem) {
        let imp = self.imp();
        let details = Details::of(item);
        let same_limits = imp.limits.borrow().len() == item.limits.len();
        let same_details = imp.details.borrow().as_ref() == Some(&details);
        if !same_limits || !same_details {
            for row in imp.limits.take() {
                self.remove(&row);
            }
            for row in imp.rows.take() {
                self.remove(&row);
            }
            let limits: Vec<LimitRow> = item.limits.iter().map(|_| LimitRow::default()).collect();
            for row in &limits {
                self.add(row);
            }
            imp.limits.replace(limits);
            let rows = self.detail_rows(&details);
            for row in &rows {
                self.add(row);
            }
            imp.rows.replace(rows);
            imp.details.replace(Some(details));
        }
        for (row, limit) in imp.limits.borrow().iter().zip(&item.limits) {
            row.show(limit);
        }
    }

    fn detail_rows(&self, details: &Details) -> Vec<gtk::Widget> {
        let words = pitboard_model::app_words(Front::Gnome).window;
        let mut rows = Vec::new();
        let property = |title: &str, said: &str| {
            let row = adw::ActionRow::builder()
                .title(title)
                .subtitle(said)
                .use_markup(false)
                .build();
            row.add_css_class("property");
            row
        };
        if let Some(email) = &details.email {
            rows.push(property(&words.email, email).upcast());
        }
        for note in &details.notes {
            rows.push(self.note_row(note, details.offered.as_ref()).upcast());
        }
        if let Some(sign_in) = &details.sign_in {
            rows.push(property(&words.sign_in, sign_in).upcast());
        }
        if let Some((said, settings)) = &details.warm {
            let row = property(&words.warm_up, said);
            let button = gtk::Button::builder()
                .label(&settings.title)
                .valign(gtk::Align::Center)
                .sensitive(settings.enabled)
                .css_classes(["flat"])
                .build();
            let (host, intent) = (self.host(), settings.intent.clone());
            button.connect_clicked(move |_| {
                if let Some(host) = &host {
                    host.send(intent.clone());
                }
            });
            row.add_suffix(&button);
            rows.push(row.upcast());
        }
        rows
    }

    // What needs doing about it, with the button that does it, but where the header's button
    // does it already, or the docs about it where nothing in the app does.
    fn note_row(&self, note: &AccountNote, offered: Option<&Intent>) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(&note.text)
            .use_markup(false)
            .build();
        let action = (note.action.as_ref())
            .filter(|action| action.intent.is_some() && action.intent.as_ref() != offered);
        if let Some(action) = action {
            let button = gtk::Button::builder()
                .label(&action.title)
                .valign(gtk::Align::Center)
                .sensitive(action.enabled)
                .build();
            let (group, action) = (self.downgrade(), action.clone());
            button.connect_clicked(move |_| {
                if let (Some(group), Some(intent)) = (group.upgrade(), action.intent.clone()) {
                    let host = group.host();
                    question::asking_first(&group, action.confirm.as_ref(), move || {
                        if let Some(host) = &host {
                            host.send(intent.clone());
                        }
                    });
                }
            });
            row.add_suffix(&button);
        } else if let Some(learn) = &note.learn {
            let button = gtk::Button::builder()
                .label(&learn.title)
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            let (group, link) = (self.downgrade(), learn.link.clone());
            button.connect_clicked(move |_| {
                if let Some(group) = group.upgrade()
                    && let (Some(host), Some(window)) =
                        (group.host(), group.root().and_downcast::<gtk::Window>())
                {
                    host.open(&link, &window);
                }
            });
            row.add_suffix(&button);
        }
        row
    }
}
