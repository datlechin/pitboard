//! The status item's menu, as `com.canonical.dbusmenu` serves it to the tray that draws it
//! (docs/facts/linux-app.md, The status item): a root, number 0, over the entries, each with
//! only the properties that are not the protocol's defaults.
//!
//! An entry keeps its number for as long as it stays in the menu, as libdbusmenu's and KDE's
//! menus keep theirs, and a new menu says what changed in the way each tray reads it again: a
//! change in which entries there are raises the revision with `LayoutUpdated`, and a change
//! in an entry that stays is said by its properties with `ItemsPropertiesUpdated`. Plasma
//! 6.3.6 reads again only the properties a layout holds, and the AppIndicator extension none
//! of an entry it knows.

use std::collections::{BTreeMap, HashMap};

use gio::prelude::*;

use super::TrayEntry;

pub(super) const INTERFACE: &str = "com.canonical.dbusmenu";
pub(super) const PATH: &str = "/MenuBar";

const ROOT: i32 = 0;
// The protocol's version libdbusmenu 18.10 serves (`libdbusmenu-glib/server.c:45`).
const VERSION: u32 = 3;

type Properties = BTreeMap<String, glib::Variant>;

// What an entry is across menus, with its place among the entries of its kind: an item by its
// id and whether it is a radio item, which a tray reads once, and a separator by its place alone.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Kind {
    Item { id: String, radio: bool },
    Separator,
}

struct Numbered {
    number: i32,
    // what a click tells, where one does
    clicked: Option<String>,
    properties: Properties,
}

pub(super) struct Menu {
    revision: u32,
    root: Properties,
    entries: Vec<Numbered>,
    numbers: HashMap<(Kind, usize), i32>,
    next: i32,
}

/// What a call on the menu tells the app.
pub(super) enum Picked {
    Clicked(String),
    Glanced,
}

pub(super) struct Answer {
    pub(super) reply: Result<Option<glib::Variant>, glib::Error>,
    pub(super) picked: Vec<Picked>,
}

/// What a new menu changed, to be said on the bus.
pub(super) struct Changed {
    revision: Option<u32>,
    updated: Vec<(i32, Properties)>,
    removed: Vec<(i32, Vec<String>)>,
}

impl Changed {
    pub(super) fn say(&self, connection: &gio::DBusConnection) -> Result<(), glib::Error> {
        if !self.updated.is_empty() || !self.removed.is_empty() {
            let parameters = (&self.updated, &self.removed).to_variant();
            connection.emit_signal(
                None,
                PATH,
                INTERFACE,
                "ItemsPropertiesUpdated",
                Some(&parameters),
            )?;
        }
        if let Some(revision) = self.revision {
            let parameters = (revision, ROOT).to_variant();
            connection.emit_signal(None, PATH, INTERFACE, "LayoutUpdated", Some(&parameters))?;
        }
        Ok(())
    }
}

impl Menu {
    pub(super) fn new() -> Menu {
        Menu {
            revision: 1,
            root: Properties::new(),
            entries: Vec::new(),
            numbers: HashMap::new(),
            next: ROOT + 1,
        }
    }

    pub(super) fn set(&mut self, menu: &[TrayEntry]) -> Changed {
        let mut numbers = HashMap::new();
        let mut seen: HashMap<Kind, usize> = HashMap::new();
        let entries: Vec<Numbered> = menu
            .iter()
            .map(|entry| {
                let kind = match entry {
                    TrayEntry::Item { id, radio, .. } => Kind::Item {
                        id: id.clone(),
                        radio: radio.is_some(),
                    },
                    TrayEntry::Separator => Kind::Separator,
                };
                let nth = seen.entry(kind.clone()).or_default();
                let key = (kind, *nth);
                *nth += 1;
                let number = match self.numbers.get(&key) {
                    Some(number) => *number,
                    None => {
                        let number = self.next;
                        self.next += 1;
                        number
                    }
                };
                numbers.insert(key, number);
                numbered(number, entry)
            })
            .collect();

        let root = if entries.is_empty() {
            Properties::new()
        } else {
            Properties::from([("children-display".into(), "submenu".to_variant())])
        };
        let was: HashMap<i32, &Properties> = std::iter::once((ROOT, &self.root))
            .chain(
                self.entries
                    .iter()
                    .map(|entry| (entry.number, &entry.properties)),
            )
            .collect();
        let now = std::iter::once((ROOT, &root)).chain(
            entries
                .iter()
                .map(|entry| (entry.number, &entry.properties)),
        );
        let mut changed = Changed {
            revision: None,
            updated: Vec::new(),
            removed: Vec::new(),
        };
        for (number, properties) in now {
            let Some(before) = was.get(&number) else {
                continue;
            };
            let updated: Properties = properties
                .iter()
                .filter(|(name, value)| before.get(*name) != Some(*value))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            let removed: Vec<String> = before
                .keys()
                .filter(|name| !properties.contains_key(*name))
                .cloned()
                .collect();
            if !updated.is_empty() {
                changed.updated.push((number, updated));
            }
            if !removed.is_empty() {
                changed.removed.push((number, removed));
            }
        }
        if order(&self.entries) != order(&entries) {
            self.revision += 1;
            changed.revision = Some(self.revision);
        }
        self.root = root;
        self.entries = entries;
        self.numbers = numbers;
        changed
    }

    pub(super) fn property(&self, name: &str) -> glib::Variant {
        match name {
            "Version" => VERSION.to_variant(),
            // Pitboard's words are English.
            "TextDirection" => "ltr".to_variant(),
            "Status" => "normal".to_variant(),
            "IconThemePath" => Vec::<String>::new().to_variant(),
            _ => unreachable!("{INTERFACE} has no property {name}"),
        }
    }

    pub(super) fn answer(&self, call: Call) -> Answer {
        let mut picked = Vec::new();
        let reply = match call {
            Call::GetLayout {
                parent,
                depth,
                names,
            } => self.layout(parent, depth, &names),
            Call::GetGroupProperties { ids, names } => {
                let ids = if ids.is_empty() { self.numbers() } else { ids };
                let found: Vec<(i32, Properties)> = ids
                    .iter()
                    .filter_map(|id| Some((*id, only(self.properties(*id)?, &names))))
                    .collect();
                Ok(Some(found.to_variant()))
            }
            Call::GetProperty { id, name } => match self.properties(id) {
                None => Err(no_entry(id)),
                Some(properties) => match properties.get(&name) {
                    Some(value) => Ok(Some(glib::Variant::from_variant(value))),
                    None => Err(glib::Error::new(
                        gio::DBusError::InvalidArgs,
                        &format!("Entry {id} has no {name}"),
                    )),
                },
            },
            Call::Event { id, event } => self.clicked(id, &event).map(|clicked| {
                picked.extend(clicked.map(Picked::Clicked));
                None
            }),
            Call::EventGroup { events } => {
                let mut unknown = Vec::new();
                for (id, event) in &events {
                    match self.clicked(*id, event) {
                        Ok(clicked) => picked.extend(clicked.map(Picked::Clicked)),
                        Err(_) => unknown.push(*id),
                    }
                }
                none_found(&unknown, events.len()).map(|()| Some(unknown.to_variant()))
            }
            Call::AboutToShow { id } => match self.properties(id) {
                None => Err(no_entry(id)),
                Some(_) => {
                    if id == ROOT {
                        picked.push(Picked::Glanced);
                    }
                    Ok(Some(false.to_variant()))
                }
            },
            Call::AboutToShowGroup { ids } => {
                let unknown: Vec<i32> = ids
                    .iter()
                    .copied()
                    .filter(|id| self.properties(*id).is_none())
                    .collect();
                if ids.contains(&ROOT) {
                    picked.push(Picked::Glanced);
                }
                none_found(&unknown, ids.len())
                    .map(|()| Some((Vec::<i32>::new(), unknown).to_variant()))
            }
        };
        Answer { reply, picked }
    }

    fn layout(
        &self,
        parent: i32,
        depth: i32,
        names: &[String],
    ) -> Result<Option<glib::Variant>, glib::Error> {
        let properties = self.properties(parent).ok_or_else(|| no_entry(parent))?;
        // Entries have no children, so any depth but none reaches them all.
        let children: Vec<glib::Variant> = if parent == ROOT && depth != 0 {
            self.entries
                .iter()
                // Not boxed here: the layout's `to_variant` puts each in the variant `av` holds.
                .map(|entry| {
                    (entry.number, only(&entry.properties, names), no_children()).to_variant()
                })
                .collect()
        } else {
            no_children()
        };
        let layout = (parent, only(properties, names), children);
        Ok(Some((self.revision, layout).to_variant()))
    }

    // What `event` on `id` tells: a click on an item that takes one tells its id.
    fn clicked(&self, id: i32, event: &str) -> Result<Option<String>, glib::Error> {
        self.properties(id).ok_or_else(|| no_entry(id))?;
        Ok((event == "clicked")
            .then(|| self.entries.iter().find(|entry| entry.number == id))
            .flatten()
            .and_then(|entry| entry.clicked.clone()))
    }

    fn numbers(&self) -> Vec<i32> {
        std::iter::once(ROOT)
            .chain(self.entries.iter().map(|entry| entry.number))
            .collect()
    }

    fn properties(&self, id: i32) -> Option<&Properties> {
        if id == ROOT {
            return Some(&self.root);
        }
        self.entries
            .iter()
            .find(|entry| entry.number == id)
            .map(|entry| &entry.properties)
    }
}

/// The menu's methods, as libdbusmenu 18.10's interface declares them.
pub(super) enum Call {
    GetLayout {
        parent: i32,
        depth: i32,
        names: Vec<String>,
    },
    GetGroupProperties {
        ids: Vec<i32>,
        names: Vec<String>,
    },
    GetProperty {
        id: i32,
        name: String,
    },
    Event {
        id: i32,
        event: String,
    },
    EventGroup {
        events: Vec<(i32, String)>,
    },
    AboutToShow {
        id: i32,
    },
    AboutToShowGroup {
        ids: Vec<i32>,
    },
}

impl DBusMethodCall for Call {
    // GDBus has checked the parameters against the interface before a call reaches here.
    fn parse_call(
        _: &str,
        _: Option<&str>,
        method: &str,
        parameters: glib::Variant,
    ) -> Result<Call, glib::Error> {
        let call = match method {
            "GetLayout" => {
                parameters
                    .get::<(i32, i32, Vec<String>)>()
                    .map(|(parent, depth, names)| Call::GetLayout {
                        parent,
                        depth,
                        names,
                    })
            }
            "GetGroupProperties" => parameters
                .get::<(Vec<i32>, Vec<String>)>()
                .map(|(ids, names)| Call::GetGroupProperties { ids, names }),
            "GetProperty" => parameters
                .get::<(i32, String)>()
                .map(|(id, name)| Call::GetProperty { id, name }),
            "Event" => parameters
                .get::<(i32, String, glib::Variant, u32)>()
                .map(|(id, event, _, _)| Call::Event { id, event }),
            "EventGroup" => parameters
                .get::<(Vec<(i32, String, glib::Variant, u32)>,)>()
                .map(|(events,)| Call::EventGroup {
                    events: events
                        .into_iter()
                        .map(|(id, event, _, _)| (id, event))
                        .collect(),
                }),
            "AboutToShow" => parameters
                .get::<(i32,)>()
                .map(|(id,)| Call::AboutToShow { id }),
            "AboutToShowGroup" => parameters
                .get::<(Vec<i32>,)>()
                .map(|(ids,)| Call::AboutToShowGroup { ids }),
            _ => None,
        };
        call.ok_or_else(|| {
            glib::Error::new(
                gio::DBusError::UnknownMethod,
                &format!("{INTERFACE} has no {method}"),
            )
        })
    }
}

fn numbered(number: i32, entry: &TrayEntry) -> Numbered {
    let mut properties = Properties::new();
    let clicked = match entry {
        TrayEntry::Separator => {
            properties.insert("type".into(), "separator".to_variant());
            None
        }
        TrayEntry::Item {
            id,
            label,
            enabled,
            radio,
        } => {
            // An underscore marks the key that picks the entry, and two stand for one.
            properties.insert("label".into(), label.replace('_', "__").to_variant());
            if !enabled {
                properties.insert("enabled".into(), false.to_variant());
            }
            if let Some(chosen) = radio {
                properties.insert("toggle-type".into(), "radio".to_variant());
                properties.insert("toggle-state".into(), i32::from(*chosen).to_variant());
            }
            enabled.then(|| id.clone())
        }
    };
    Numbered {
        number,
        clicked,
        properties,
    }
}

fn order(entries: &[Numbered]) -> Vec<i32> {
    entries.iter().map(|entry| entry.number).collect()
}

fn only(properties: &Properties, names: &[String]) -> Properties {
    properties
        .iter()
        .filter(|(name, _)| names.is_empty() || names.contains(name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn no_children() -> Vec<glib::Variant> {
    Vec::new()
}

fn no_entry(id: i32) -> glib::Error {
    glib::Error::new(
        gio::DBusError::InvalidArgs,
        &format!("The menu has no entry {id}"),
    )
}

// A group of events or ids none of which is in the menu is refused, as libdbusmenu does.
fn none_found(unknown: &[i32], asked: usize) -> Result<(), glib::Error> {
    if unknown.len() < asked {
        return Ok(());
    }
    Err(glib::Error::new(
        gio::DBusError::InvalidArgs,
        &format!("The menu has none of the entries {unknown:?}"),
    ))
}
