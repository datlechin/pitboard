//! The status item, as StatusNotifierItem 0.1 has an app serve one, under KDE's interface name,
//! which Plasma and the AppIndicator extension read (docs/facts/linux-app.md, The status item):
//! a name of its own on the connection it is given, the item at `/StatusNotifierItem` and its
//! menu at `/MenuBar`, and its registration with the tray's watcher.
//!
//! It registers once `org.kde.StatusNotifierWatcher` has an owner and again each time the name
//! gets a new one, as the name's own signal says, and tells whether a tray shows it. Nothing
//! polls, and nothing asks the bus to start a watcher.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use gio::prelude::*;

use super::menu::{self, Menu, Picked};
use super::{Failed, Status, StatusItem, TrayItem};
use crate::resources;

const INTERFACE: &str = "org.kde.StatusNotifierItem";
const PATH: &str = "/StatusNotifierItem";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

/// What the item hears from the tray, and whether there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    /// A tray shows the item: the watcher took it.
    Tray,
    /// No tray shows it: no watcher owns its name, the one there went or refused the item, or
    /// the item is hidden.
    NoTray,
    /// A click on the item, with the activation token the tray provided for it.
    Activated { token: Option<String> },
    /// A click on an entry of the menu, by the entry's id, with the token the tray provided.
    Clicked { id: String, token: Option<String> },
    /// The tray is about to show the menu.
    Glanced,
}

/// The desktop's status item, on a connection it is given: GApplication's, in the app.
pub struct Desktop {
    inner: Rc<Inner>,
    // gio 0.22 exports another `WatcherId` than the one its name watch returns.
    unwatch: Option<Box<dyn FnOnce()>>,
}

struct Inner {
    connection: gio::DBusConnection,
    name: String,
    id: String,
    title: String,
    told: Box<dyn Fn(Heard)>,
    state: RefCell<State>,
}

struct State {
    shown: Option<TrayItem>,
    menu: Menu,
    published: Option<Published>,
    watcher: bool,
    // raised as the watcher or the item's own name changes, so an answer to a registration
    // asked before says nothing
    asked: u64,
    tray: Option<bool>,
    // the last the tray provided, which goes with the next click
    token: Option<String>,
}

// The item on the bus.
struct Published {
    item: gio::RegistrationId,
    menu: gio::RegistrationId,
    owner: gio::OwnerId,
    owned: bool,
}

impl Desktop {
    /// An item that `id` names, which StatusNotifierItem asks to stay the same from one session
    /// to the next, titled `title`. It is on the bus from its first [`StatusItem::show`].
    pub fn new(
        connection: &gio::DBusConnection,
        id: &str,
        title: &str,
        told: impl Fn(Heard) + 'static,
    ) -> Desktop {
        let inner = Rc::new(Inner {
            connection: connection.clone(),
            // The specification's form, the process's id and one for its only item.
            name: format!("{INTERFACE}-{}-1", std::process::id()),
            id: id.to_owned(),
            title: title.to_owned(),
            told: Box::new(told),
            state: RefCell::new(State {
                shown: None,
                menu: Menu::new(),
                published: None,
                watcher: false,
                asked: 0,
                tray: None,
                token: None,
            }),
        });
        let (came, went) = (Rc::downgrade(&inner), Rc::downgrade(&inner));
        let watcher = gio::bus_watch_name_on_connection(
            connection,
            WATCHER,
            gio::BusNameWatcherFlags::NONE,
            move |_, _, _| alive(&came, Inner::watcher_came),
            move |_, _| alive(&went, Inner::watcher_went),
        );
        Desktop {
            inner,
            unwatch: Some(Box::new(move || gio::bus_unwatch_name(watcher))),
        }
    }

    /// The name the item owns on the bus while shown, which it registers by.
    pub fn name(&self) -> &str {
        &self.inner.name
    }
}

impl StatusItem for Desktop {
    fn show(&self, item: &TrayItem) -> Result<(), Failed> {
        let inner = &self.inner;
        let (news, changed, published) = {
            let mut state = inner.state.borrow_mut();
            let news = state
                .shown
                .as_ref()
                .map(|was| news(was, item))
                .unwrap_or_default();
            let changed = state.menu.set(&item.menu);
            state.shown = Some(item.clone());
            (news, changed, state.published.is_some())
        };
        // A tray reads all of an item it is told of.
        if !published {
            return inner.publish().map_err(failed);
        }
        for (signal, parameters) in news {
            inner
                .connection
                .emit_signal(None, PATH, INTERFACE, signal, parameters.as_ref())
                .map_err(failed)?;
        }
        changed.say(&inner.connection).map_err(failed)
    }

    fn hide(&self) -> Result<(), Failed> {
        let taken = self.inner.take_off();
        self.inner.say(false);
        taken.map_err(failed)
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        let _ = self.inner.take_off();
        if let Some(unwatch) = self.unwatch.take() {
            unwatch();
        }
    }
}

impl Inner {
    fn publish(self: &Rc<Self>) -> Result<(), glib::Error> {
        // Each holds the item until GDBus lets go of it, which it does once the object is
        // unregistered, and calls none after.
        let item = self
            .connection
            .register_object(PATH, &resources::interface(INTERFACE))
            .typed_method_call::<Call>()
            .invoke({
                let inner = self.clone();
                move |_, _, call, invocation| inner.item_called(call, invocation)
            })
            .property({
                let inner = self.clone();
                move |_, _, _, _, name| inner.item_property(name)
            })
            .build()?;
        let menu = self
            .connection
            .register_object(menu::PATH, &resources::interface(menu::INTERFACE))
            .typed_method_call::<menu::Call>()
            .invoke({
                let inner = self.clone();
                move |_, _, call, invocation| inner.menu_called(call, invocation)
            })
            .property({
                let inner = self.clone();
                move |_, _, _, _, name| inner.state.borrow().menu.property(name)
            })
            .build();
        let menu = match menu {
            Ok(menu) => menu,
            Err(e) => {
                let _ = self.connection.unregister_object(item);
                return Err(e);
            }
        };
        let (acquired, lost) = (Rc::downgrade(self), Rc::downgrade(self));
        let owner = gio::bus_own_name_on_connection(
            &self.connection,
            &self.name,
            gio::BusNameOwnerFlags::NONE,
            move |_, _| alive(&acquired, Inner::name_acquired),
            move |_, _| alive(&lost, Inner::name_lost),
        );
        self.state.borrow_mut().published = Some(Published {
            item,
            menu,
            owner,
            owned: false,
        });
        Ok(())
    }

    // Off the bus, the name first, so the watcher lets go of the item before its objects go.
    fn take_off(&self) -> Result<(), glib::BoolError> {
        let published = {
            let mut state = self.state.borrow_mut();
            state.asked += 1;
            state.published.take()
        };
        let Some(published) = published else {
            return Ok(());
        };
        gio::bus_unown_name(published.owner);
        let item = self.connection.unregister_object(published.item);
        let menu = self.connection.unregister_object(published.menu);
        item.and(menu)
    }

    fn watcher_came(self: &Rc<Self>) {
        let asked = {
            let mut state = self.state.borrow_mut();
            state.watcher = true;
            state.asked += 1;
            let owned = state
                .published
                .as_ref()
                .is_some_and(|published| published.owned);
            owned.then_some(state.asked)
        };
        if let Some(asked) = asked {
            self.register(asked);
        }
    }

    fn watcher_went(self: &Rc<Self>) {
        {
            let mut state = self.state.borrow_mut();
            state.watcher = false;
            state.asked += 1;
        }
        self.say(false);
    }

    fn name_acquired(self: &Rc<Self>) {
        let asked = {
            let mut state = self.state.borrow_mut();
            if let Some(published) = state.published.as_mut() {
                published.owned = true;
            }
            state.watcher.then_some(state.asked)
        };
        if let Some(asked) = asked {
            self.register(asked);
        }
    }

    fn name_lost(self: &Rc<Self>) {
        {
            let mut state = self.state.borrow_mut();
            if let Some(published) = state.published.as_mut() {
                published.owned = false;
            }
            state.asked += 1;
        }
        self.say(false);
    }

    fn register(self: &Rc<Self>, asked: u64) {
        let inner = Rc::downgrade(self);
        self.connection.call(
            Some(WATCHER),
            WATCHER_PATH,
            WATCHER,
            "RegisterStatusNotifierItem",
            Some(&(self.name.as_str(),).to_variant()),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            -1,
            gio::Cancellable::NONE,
            move |answer| {
                let Some(inner) = inner.upgrade() else {
                    return;
                };
                if inner.state.borrow().asked == asked {
                    inner.say(answer.is_ok());
                }
            },
        );
    }

    // Told only as it changes.
    fn say(&self, tray: bool) {
        let changed = self.state.borrow_mut().tray.replace(tray) != Some(tray);
        if changed {
            (self.told)(if tray { Heard::Tray } else { Heard::NoTray });
        }
    }

    fn item_called(&self, call: Call, invocation: gio::DBusMethodInvocation) {
        let heard = {
            let mut state = self.state.borrow_mut();
            match call {
                Call::ProvideXdgActivationToken(token) => {
                    state.token = Some(token);
                    None
                }
                Call::Activate => Some(Heard::Activated {
                    token: state.token.take(),
                }),
                // The tray draws the menu the item names, and Pitboard has nothing for a
                // middle click or a scroll.
                Call::ContextMenu | Call::SecondaryActivate | Call::Scroll => None,
            }
        };
        invocation.return_value(None);
        if let Some(heard) = heard {
            (self.told)(heard);
        }
    }

    // Answered before the app hears of it, as libdbusmenu answers first.
    fn menu_called(&self, call: menu::Call, invocation: gio::DBusMethodInvocation) {
        let (reply, heard) = {
            let mut state = self.state.borrow_mut();
            let menu::Answer { reply, picked } = state.menu.answer(call);
            let heard: Vec<Heard> = picked
                .into_iter()
                .map(|picked| match picked {
                    Picked::Clicked(id) => Heard::Clicked {
                        id,
                        token: state.token.take(),
                    },
                    Picked::Glanced => Heard::Glanced,
                })
                .collect();
            (reply, heard)
        };
        invocation.return_result(reply);
        for heard in heard {
            (self.told)(heard);
        }
    }

    fn item_property(&self, name: &str) -> glib::Variant {
        let state = self.state.borrow();
        let shown = state.shown.as_ref();
        let named = |name: fn(&TrayItem) -> &str| shown.map_or("", name).to_variant();
        match name {
            "Category" => "ApplicationStatus".to_variant(),
            "Id" => self.id.to_variant(),
            "Title" => self.title.to_variant(),
            "Status" => shown
                .map_or(Status::Passive, |shown| shown.status)
                .as_str()
                .to_variant(),
            "WindowId" => 0i32.to_variant(),
            "IconThemePath" | "OverlayIconName" | "AttentionMovieName" => "".to_variant(),
            "Menu" => glib::variant::ObjectPath::try_from(menu::PATH)
                .expect("the menu's path is an object path")
                .to_variant(),
            "ItemIsMenu" => false.to_variant(),
            "IconName" => named(|shown| &shown.icon_name),
            "AttentionIconName" => named(|shown| &shown.attention_icon_name),
            "IconPixmap" | "OverlayIconPixmap" | "AttentionIconPixmap" => no_pixmaps().to_variant(),
            // The text as written: Plasma 6.3.6 shows it as rich text only where it reads as
            // markup, and the AppIndicator extension shows no tool tip.
            "ToolTip" => (
                "",
                no_pixmaps(),
                shown.map_or("", |shown| &shown.tool_tip.title),
                shown.map_or("", |shown| &shown.tool_tip.text),
            )
                .to_variant(),
            _ => unreachable!("{INTERFACE} has no property {name}"),
        }
    }
}

/// The item's methods, as its interface declares them.
enum Call {
    ProvideXdgActivationToken(String),
    ContextMenu,
    Activate,
    SecondaryActivate,
    Scroll,
}

impl DBusMethodCall for Call {
    // GDBus has checked the parameters against the interface before a call reaches here.
    fn parse_call(
        _: &str,
        _: Option<&str>,
        method: &str,
        parameters: glib::Variant,
    ) -> Result<Call, glib::Error> {
        match method {
            "ProvideXdgActivationToken" => parameters
                .get::<(String,)>()
                .map(|(token,)| Call::ProvideXdgActivationToken(token)),
            "ContextMenu" => Some(Call::ContextMenu),
            "Activate" => Some(Call::Activate),
            "SecondaryActivate" => Some(Call::SecondaryActivate),
            "Scroll" => Some(Call::Scroll),
            _ => None,
        }
        .ok_or_else(|| {
            glib::Error::new(
                gio::DBusError::UnknownMethod,
                &format!("{INTERFACE} has no {method}"),
            )
        })
    }
}

// The signals for what changed of the item's face, each with its parameters.
fn news(was: &TrayItem, now: &TrayItem) -> Vec<(&'static str, Option<glib::Variant>)> {
    let mut news = Vec::new();
    if was.status != now.status {
        news.push(("NewStatus", Some((now.status.as_str(),).to_variant())));
    }
    if was.icon_name != now.icon_name {
        news.push(("NewIcon", None));
    }
    if was.attention_icon_name != now.attention_icon_name {
        news.push(("NewAttentionIcon", None));
    }
    if was.tool_tip != now.tool_tip {
        news.push(("NewToolTip", None));
    }
    news
}

// The item's icons are names in the theme, never drawn by the app.
fn no_pixmaps() -> Vec<(i32, i32, Vec<u8>)> {
    Vec::new()
}

// What the bus says reaches the item only while it lives.
fn alive(inner: &Weak<Inner>, act: fn(&Rc<Inner>)) {
    if let Some(inner) = inner.upgrade() {
        act(&inner);
    }
}

fn failed(e: impl std::fmt::Display) -> Failed {
    Failed(format!("the status item: {e}"))
}
