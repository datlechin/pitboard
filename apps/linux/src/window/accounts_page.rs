//! The accounts page: what the model's `AccountsShown` says stands in place of the list, each on
//! a status page with its one button, and the list, which [`super::accounts`] keeps. While
//! nothing has been read, the status page's spinner says so.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pitboard_model::{AccountsShown, Choice, Part};

use super::Host;
use super::accounts::Accounts;
use crate::shown::Shown;

// What the status page's button does.
#[derive(Clone)]
enum Does {
    Choose(Choice),
    Open(String),
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/accounts-page.ui")]
    pub struct AccountsPage {
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) status_action: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) list: TemplateChild<adw::PreferencesPage>,
        #[template_child]
        pub(super) notices_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) notices: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub(super) tip: TemplateChild<adw::PreferencesGroup>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) accounts: OnceCell<Rc<Accounts>>,
        pub(super) does: RefCell<Option<Does>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AccountsPage {
        const NAME: &'static str = "PitboardAccountsPage";
        type Type = super::AccountsPage;
        type ParentType = adw::Bin;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AccountsPage {
        fn constructed(&self) {
            self.parent_constructed();
            let page = self.obj().downgrade();
            self.status_action.connect_clicked(move |button| {
                if let Some(page) = page.upgrade() {
                    page.act(button);
                }
            });
        }
    }

    impl WidgetImpl for AccountsPage {}
    impl BinImpl for AccountsPage {}
}

glib::wrapper! {
    pub struct AccountsPage(ObjectSubclass<imp::AccountsPage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AccountsPage {
    /// Which page it shows: `status`, or `list`.
    pub fn visible_child_name(&self) -> Option<glib::GString> {
        self.imp().stack.visible_child_name()
    }

    pub(super) fn follow(&self, shown: &Shown, host: Rc<dyn Host>) {
        let imp = self.imp();
        let _ = imp.host.set(host.clone());
        let accounts = Accounts::follow(
            &imp.list,
            (&imp.notices_group, &imp.notices),
            &imp.tip,
            shown,
            host,
        );
        let _ = imp.accounts.set(accounts);
        self.show(&shown.snapshot().accounts_shown);
        let page = self.downgrade();
        shown.connect_changed(Part::AccountsShown, move |shown| {
            if let Some(page) = page.upgrade() {
                page.show(&shown.snapshot().accounts_shown);
            }
        });
    }

    fn show(&self, shown: &AccountsShown) {
        let imp = self.imp();
        let (status, action) = (&imp.status, &imp.status_action);
        let (title, detail, icon, does) = match shown {
            AccountsShown::List => {
                imp.does.replace(None);
                imp.stack.set_visible_child_name("list");
                return;
            }
            AccountsShown::Reading { title } => (title, None, None, None),
            AccountsShown::NoTool {
                title,
                detail,
                link_title,
                link,
            } => (
                title,
                Some(detail),
                Some("package-x-generic-symbolic"),
                Some((link_title.clone(), Does::Open(link.clone()), true)),
            ),
            AccountsShown::ReadFailed {
                title,
                detail,
                retry,
            } => (
                title,
                Some(detail),
                Some("dialog-warning-symbolic"),
                Some((
                    retry.title.clone(),
                    Does::Choose(retry.clone()),
                    retry.enabled,
                )),
            ),
            AccountsShown::NoAccounts { title, detail, add } => (
                title,
                Some(detail),
                Some("system-users-symbolic"),
                Some((add.title.clone(), Does::Choose(add.clone()), add.enabled)),
            ),
        };
        status.set_title(title);
        status.set_description(
            detail
                .map(|detail| glib::markup_escape_text(detail))
                .as_deref(),
        );
        match icon {
            Some(icon) => {
                status.set_paintable(None::<&gtk::gdk::Paintable>);
                status.set_icon_name(Some(icon));
            }
            None => {
                status.set_icon_name(None);
                status.set_paintable(Some(&adw::SpinnerPaintable::new(Some(&**status))));
            }
        }
        match &does {
            Some((label, _, enabled)) => {
                action.set_label(label);
                action.set_sensitive(*enabled);
                action.set_visible(true);
            }
            None => action.set_visible(false),
        }
        imp.does.replace(does.map(|(_, does, _)| does));
        imp.stack.set_visible_child_name("status");
    }

    fn act(&self, button: &gtk::Button) {
        let (Some(host), Some(does)) = (self.imp().host.get(), self.imp().does.borrow().clone())
        else {
            return;
        };
        match does {
            Does::Choose(choice) => host.send(choice.intent),
            Does::Open(link) => {
                if let Some(window) = button.root().and_downcast::<gtk::Window>() {
                    host.open(&link, &window);
                }
            }
        }
    }
}
