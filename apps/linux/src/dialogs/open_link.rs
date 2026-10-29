//! Open Link, as an Adwaita dialog: the link waiting for somebody to say how it opens, as the
//! model's picker words it, or, while none waits, what the model says arrives here. A link
//! pasted with Paste Link or dropped on it is handed to the model as it came, and the model says
//! what it is. Until the Linux app has account windows, the one thing a link opens in is the
//! browser (docs/facts/linux-app.md, Account windows).
//!
//! Closed while a link waits, the link is put away.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, glib};
use pitboard_model::{AccountWindowsShown, Front, Intent, LinkPicker, PickerShown};

use crate::platform::links;
use crate::window::Host;

// What the dialog's one button does.
#[derive(Clone)]
enum Does {
    Open(String),
    Send(Intent),
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/open-link.ui")]
    pub struct OpenLink {
        #[template_child]
        pub(super) status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) link: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) action: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) paste: TemplateChild<gtk::Button>,
        pub(super) host: OnceCell<Rc<dyn Host>>,
        pub(super) shown: RefCell<Option<AccountWindowsShown>>,
        pub(super) does: RefCell<Option<Does>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OpenLink {
        const NAME: &'static str = "PitboardOpenLink";
        type Type = super::OpenLink;
        type ParentType = adw::Dialog;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for OpenLink {
        fn constructed(&self) {
            self.parent_constructed();
            let dialog = self.obj();
            let words = pitboard_model::app_words(Front::Gnome).window;
            dialog.set_title(&words.open_link);
            self.paste
                .set_label(&pitboard_model::gnome_words().paste_link);
            let weak = dialog.downgrade();
            self.paste.connect_clicked(move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.paste();
                }
            });
            let weak = dialog.downgrade();
            self.action.connect_clicked(move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.act();
                }
            });
        }
    }

    impl WidgetImpl for OpenLink {}

    impl AdwDialogImpl for OpenLink {
        fn closed(&self) {
            self.obj().put_away();
            self.parent_closed();
        }
    }
}

glib::wrapper! {
    pub struct OpenLink(ObjectSubclass<imp::OpenLink>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl OpenLink {
    pub fn new(host: Rc<dyn Host>) -> OpenLink {
        let dialog: OpenLink = glib::Object::new();
        let _ = dialog.imp().host.set(host.clone());
        let target = links::drop_target(move |text| {
            host.send(Intent::LinkArrived { text });
        });
        let weak = dialog.downgrade();
        target.connect_enter(move |_, _, _| {
            if let Some(dialog) = weak.upgrade() {
                dialog.dragged_over();
            }
            gdk::DragAction::COPY
        });
        let weak = dialog.downgrade();
        target.connect_leave(move |_| {
            if let Some(dialog) = weak.upgrade()
                && let Some(shown) = dialog.imp().shown.borrow().clone()
            {
                dialog.show(&shown);
            }
        });
        dialog.add_controller(target);
        dialog
    }

    // What it says while a link is dragged over it, until it is dropped or taken away.
    fn dragged_over(&self) {
        let status = &self.imp().status;
        status.set_paintable(None::<&gdk::Paintable>);
        status.set_icon_name(Some("insert-link-symbolic"));
        status.set_title(&pitboard_model::gnome_words().drop_link);
        status.set_description(None);
    }

    fn host(&self) -> Rc<dyn Host> {
        Rc::clone(self.imp().host.get().expect("made with its host"))
    }

    /// Shows the link waiting that `shown` says, or what arrives here while none waits.
    pub fn show(&self, shown: &AccountWindowsShown) {
        let imp = self.imp();
        imp.shown.replace(Some(shown.clone()));
        let status = &imp.status;
        let set = |icon: Option<&str>, title: &str, detail: Option<&str>| {
            match icon {
                Some(icon) => {
                    status.set_paintable(None::<&gdk::Paintable>);
                    status.set_icon_name(Some(icon));
                }
                None => {
                    status.set_icon_name(None);
                    status.set_paintable(Some(&adw::SpinnerPaintable::new(Some(&**status))));
                }
            }
            status.set_title(title);
            status.set_description(detail.map(glib::markup_escape_text).as_deref());
        };
        let mut link = None;
        let mut does = None;
        let mut paste = false;
        match shown.picker.as_ref().map(|picker| &picker.shown) {
            None => {
                set(
                    Some("insert-link-symbolic"),
                    &shown.no_link.title,
                    Some(&shown.no_link.detail),
                );
                paste = true;
            }
            Some(PickerShown::Reading { title }) => set(None, title, None),
            Some(PickerShown::ReadFailed {
                title,
                detail,
                retry,
            }) => {
                set(Some("dialog-warning-symbolic"), title, Some(detail));
                does = Some((
                    retry.title.clone(),
                    Does::Send(retry.intent.clone()),
                    retry.enabled,
                ));
            }
            Some(PickerShown::Refused { title, reason }) => {
                set(Some("dialog-warning-symbolic"), title, Some(reason));
                paste = true;
            }
            Some(PickerShown::NoWindows {
                title,
                link: site,
                link_text,
                detail,
                in_browser,
            })
            | Some(PickerShown::NoAccount {
                title,
                link: site,
                link_text,
                detail,
                in_browser,
                ..
            }) => {
                set(Some("web-browser-symbolic"), title, Some(detail));
                link = Some(link_text.clone());
                does = Some((in_browser.clone(), Does::Open(site.url.clone()), true));
            }
            // Offered only where the front has account windows to choose from, which the Linux
            // app has none of yet.
            Some(PickerShown::Choose {
                title, link_text, ..
            }) => {
                set(Some("web-browser-symbolic"), title, None);
                link = Some(link_text.clone());
            }
        }
        imp.link.set_visible(link.is_some());
        imp.link.set_label(link.as_deref().unwrap_or_default());
        imp.paste.set_visible(paste);
        imp.action.set_visible(does.is_some());
        if let Some((title, _, enabled)) = &does {
            imp.action.set_label(title);
            imp.action.set_sensitive(*enabled);
        }
        imp.does.replace(does.map(|(_, does, _)| does));
        let default = if imp.action.is_visible() {
            Some(&*imp.action)
        } else if paste {
            Some(&*imp.paste)
        } else {
            None
        };
        self.set_default_widget(default);
    }

    fn waiting(&self) -> Option<LinkPicker> {
        (self.imp().shown.borrow().as_ref()).and_then(|shown| shown.picker.clone())
    }

    fn paste(&self) {
        let host = self.host();
        links::paste(&self.clipboard(), move |text| {
            host.send(Intent::LinkArrived { text });
        });
    }

    // The browser opens the link, which is then put away, as the dialog closes.
    fn act(&self) {
        let Some(does) = self.imp().does.borrow().clone() else {
            return;
        };
        match does {
            Does::Open(link) => {
                if let Some(window) = self.root().and_downcast::<gtk::Window>() {
                    self.host().open(&link, &window);
                }
                self.close();
            }
            Does::Send(intent) => self.host().send(intent),
        }
    }

    fn put_away(&self) {
        if let Some(picker) = self.waiting() {
            self.host().send(Intent::DismissLink {
                arrival: picker.arrival,
            });
        }
    }
}
