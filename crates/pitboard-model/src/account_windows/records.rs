//! The account windows' records: the stores each Pitboard directory made, and the page each of
//! its windows was last on. The model keeps them in `windows.json`, in a directory of the
//! app's own that the app names in `AppLaunch::account_windows`, since the web stores they
//! describe are the app's whichever Pitboard directory it serves: WebKit keeps every store of
//! one app under the person's own Library, whatever `HOME` says. Inside, each directory's
//! records are kept apart, under its `key`, so a copy of the app run with another home never
//! takes the installed copy's stores for its own.
//!
//! The macOS app kept these in UserDefaults, as `webStores` and `windowPages`, by the same
//! keys, with each store id in upper case, as Foundation writes a UUID: WebsiteData.swift's
//! `StoreRecord` and `PageRecord`, as they were at a3e5ce0. It hands what it held over once,
//! in `EarlierHandover::windows`, the way it hands over its earlier preferences.
//! A store id is compared without regard to case wherever it comes from: read as written,
//! every store the macOS app recorded would be taken for an orphan, and deleted with the
//! sign-in it holds.
//!
//! What is here is pure: it reads the file's text as the lane read it, and says what to write.

use crate::Front;
use crate::model::EarlierWindowRecords;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// The file's name, in the directory the app names.
pub(crate) const FILE: &str = "windows.json";

/// What `front`'s app calls the Pitboard directory `directory`, as the environment gives it,
/// in the records: the key each app kept its records under before the model kept them, so
/// every store and page recorded since is still that directory's. macOS's is Foundation's
/// standardised file path, which reads the file system and Apple's own Unicode tables, so the
/// app says it, as `given` (docs/facts/foundation-urls.md); without it, as for a test or a
/// fixture, the directory as given. Windows' compares as Windows compares paths, as the app
/// keyed its records before. The Linux app kept none before, so its key is the directory the
/// core reaches, made absolute from the working directory and taken by its components; a `..`
/// stays, since only the file system says where one leads past a link.
pub(crate) fn key(front: Front, directory: &str, given: Option<String>) -> String {
    match front {
        Front::MacMenuBar => given.unwrap_or_else(|| directory.to_owned()),
        Front::WindowsNotificationArea => pitboard_core::host::path_key_in_any_case(directory),
        Front::Gnome => std::path::absolute(directory)
            .ok()
            .and_then(|path| {
                let path: PathBuf = path.components().collect();
                path.into_os_string().into_string().ok()
            })
            .unwrap_or_else(|| directory.to_owned()),
    }
}

/// A store id as the records keep and compare it: in lower case, as `stores::derive` writes
/// one, so one Foundation wrote in upper case is the same store.
pub(crate) fn store_key(store: &str) -> String {
    store.to_ascii_lowercase()
}

/// Whether `store` is written as a UUID is, `8-4-4-4-12` hexadecimal digits in either case,
/// as Foundation's `UUID(uuidString:)` reads one: the only id an app can make or delete a
/// store by.
fn is_store_id(store: &str) -> bool {
    store.len() == 36
        && store.char_indices().all(|(at, c)| match at {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// One Pitboard directory's records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The stores its windows were made with, recorded before each was made: a store made and
    /// never recorded would never be deleted.
    pub(crate) stores: BTreeSet<String>,
    /// The page each window was last on, by its store: one of the site's own pages, so the
    /// window opens there again.
    pub(crate) pages: BTreeMap<String, String>,
    // On Windows, recorded stores answered `StoreHeld`, held across a restart until deleted.
    pub(crate) held: BTreeSet<String>,
}

/// What `windows.json` holds: each directory's records, by its key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Records {
    #[serde(default)]
    stores: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    pages: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    held: BTreeMap<String, BTreeSet<String>>,
}

impl Records {
    /// `windows.json`'s text as records, or `None` where it does not read as them.
    pub(crate) fn read(text: &str) -> Option<Records> {
        serde_json::from_str::<Records>(text)
            .ok()
            .map(Records::normalised)
    }

    /// What the app's earlier store held, as records.
    pub(crate) fn earlier(earlier: &EarlierWindowRecords) -> Records {
        Records {
            stores: earlier
                .stores
                .iter()
                .map(|(key, stores)| (key.clone(), stores.iter().cloned().collect()))
                .collect(),
            pages: earlier
                .pages
                .iter()
                .map(|(key, pages)| {
                    let pages = pages
                        .iter()
                        .map(|(store, page)| (store.clone(), page.clone()))
                        .collect();
                    (key.clone(), pages)
                })
                .collect(),
            held: BTreeMap::new(),
        }
        .normalised()
    }

    /// Every store id in lower case, none that is not a UUID, and no directory that records
    /// nothing.
    fn normalised(self) -> Records {
        let ids = |by_key: BTreeMap<String, BTreeSet<String>>| {
            by_key
                .into_iter()
                .map(|(key, stores)| {
                    let stores = stores
                        .iter()
                        .filter(|store| is_store_id(store))
                        .map(|store| store_key(store))
                        .collect();
                    (key, stores)
                })
                .filter(|(_, stores): &(String, BTreeSet<String>)| !stores.is_empty())
                .collect()
        };
        Records {
            stores: ids(self.stores),
            held: ids(self.held),
            pages: self
                .pages
                .into_iter()
                .map(|(key, pages)| {
                    let pages = pages
                        .into_iter()
                        .filter(|(store, _)| is_store_id(store))
                        .map(|(store, page)| (store_key(&store), page))
                        .collect();
                    (key, pages)
                })
                .filter(|(_, pages): &(String, BTreeMap<String, String>)| !pages.is_empty())
                .collect(),
        }
    }

    /// The records of the directory `key` names.
    pub(crate) fn entry(&self, key: &str) -> Entry {
        let stores = self.stores.get(key).cloned().unwrap_or_default();
        let held = self.held.get(key).map_or_else(BTreeSet::new, |held| {
            held.intersection(&stores).cloned().collect()
        });
        Entry {
            stores,
            pages: self.pages.get(key).cloned().unwrap_or_default(),
            held,
        }
    }

    /// The directory `key` names recording `entry`, and every other as it was.
    pub(crate) fn set(&mut self, key: &str, entry: &Entry) {
        if entry.stores.is_empty() {
            self.stores.remove(key);
        } else {
            self.stores.insert(key.to_owned(), entry.stores.clone());
        }
        if entry.pages.is_empty() {
            self.pages.remove(key);
        } else {
            self.pages.insert(key.to_owned(), entry.pages.clone());
        }
        if entry.held.is_empty() {
            self.held.remove(key);
        } else {
            self.held.insert(key.to_owned(), entry.held.clone());
        }
    }

    /// `windows.json`'s text for these records.
    pub(crate) fn text(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    /// Which of `stores` a directory other than `key`'s recorded too, while `exists` says it
    /// is still there. The same account enrolled in both derives the same store, so it is
    /// that one's to keep: deleting it would sign its window out. A directory that is gone,
    /// such as a test's scratch home, has no account using it any more.
    pub(crate) fn shared(
        &self,
        key: &str,
        stores: &[String],
        exists: impl Fn(&str) -> bool,
    ) -> Vec<String> {
        stores
            .iter()
            .filter(|store| {
                let store = store_key(store);
                self.stores.iter().any(|(other, recorded)| {
                    other != key && recorded.contains(&store) && exists(other)
                })
            })
            .cloned()
            .collect()
    }
}

/// What the model has of its records as it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Loaded {
    /// This directory's records.
    pub(crate) entry: Entry,
    /// They came from `windows.json`. Where it was not there, they came from what the app's
    /// earlier store held, or are none.
    pub(crate) from_file: bool,
    /// The app handed its earlier store over, to be kept in the file at once so that store is
    /// read once.
    pub(crate) handed_over: bool,
}

/// What the file is taken to hold, as the lane read it: the records in it, or where it is not
/// there, what the app's earlier store held, or none. `None` where it is there and cannot be
/// read, or its text does not read as records, as a later version, a hand edit or damage may
/// leave it: neither is the same as no file, and written over, it would lose every other
/// directory's records, and with them what keeps a store another directory's account still
/// uses from being deleted.
fn base(file: &std::io::Result<Option<String>>, earlier: Option<&Records>) -> Option<Records> {
    match file {
        Ok(Some(text)) => Records::read(text),
        Ok(None) => Some(earlier.cloned().unwrap_or_default()),
        Err(_) => None,
    }
}

/// The records of the directory `key` names as the model starts, from `file` as the lane read
/// it, or from `earlier`, what the app's earlier store held, where the file is not there.
/// `None` where the file is there and cannot be read as records.
pub(crate) fn load(
    file: &std::io::Result<Option<String>>,
    key: &str,
    earlier: Option<&Records>,
) -> Option<Loaded> {
    let records = base(file, earlier)?;
    let from_file = matches!(file, Ok(Some(_)));
    Some(Loaded {
        entry: records.entry(key),
        from_file,
        handed_over: !from_file && earlier.is_some(),
    })
}

/// `windows.json`'s text with the directory `key` names recording `entry`, over what `file`
/// held as the lane read it, every other directory's records as they were, or `None` where
/// the file is there and cannot be read as records: nothing is written over records nobody
/// read.
pub(crate) fn kept(
    file: &std::io::Result<Option<String>>,
    key: &str,
    earlier: Option<&Records>,
    entry: &Entry,
) -> Option<String> {
    let mut records = base(file, earlier)?;
    records.set(key, entry);
    records.text()
}

/// Which of `stores` another Pitboard directory that is still there recorded too, from `file`
/// as the lane read it, or `None` where it is there and cannot be read as records: then
/// nobody can say.
pub(crate) fn shared(
    file: &std::io::Result<Option<String>>,
    key: &str,
    earlier: Option<&Records>,
    stores: &[String],
    exists: impl Fn(&str) -> bool,
) -> Option<Vec<String>> {
    Some(base(file, earlier)?.shared(key, stores, exists))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Each app's records were kept under its own key before the model kept them: macOS's as
    /// Foundation standardises the directory, which the app gives; Windows' as
    /// `windows_records_key` keyed them for the app.
    #[test]
    fn each_front_keys_its_records_as_its_app_did() {
        let mac = |directory: &str, given: Option<&str>| {
            key(Front::MacMenuBar, directory, given.map(str::to_owned))
        };
        assert_eq!(
            mac(
                "/Users/dana/./work/../pitboard/",
                Some("/Users/dana/pitboard")
            ),
            "/Users/dana/pitboard",
            "Foundation's, as the app gives it"
        );
        assert_eq!(
            mac("/tmp/f/./x/", None),
            "/tmp/f/./x/",
            "a fixture's, as given"
        );

        let windows = |directory: &str| key(Front::WindowsNotificationArea, directory, None);
        assert_eq!(
            windows(r"c:\users\x\appdata\local\pitboard\"),
            windows(r"\\?\C:\Users\X\AppData\Local\Pitboard")
        );
        assert_eq!(
            windows("C:/Users/X/AppData/Local/Pitboard"),
            r"C:\USERS\X\APPDATA\LOCAL\PITBOARD"
        );
        assert_eq!(
            key(
                Front::WindowsNotificationArea,
                r"C:\Users\X\.\AppData\..\Pitboard",
                Some("ignored".into())
            ),
            r"C:\USERS\X\PITBOARD",
            "only macOS's app gives its key"
        );
    }

    /// The Linux app kept no records before the model, so its key is the directory the core
    /// reaches: made absolute from the working directory, a `.`, a doubled `/` and a trailing
    /// `/` keying alike, and a `..` kept, since only the file system says where one leads past a
    /// link.
    // windows: the Linux app runs on Linux alone, and Windows makes a path absolute on a drive
    #[cfg(unix)]
    #[test]
    fn the_linux_app_keys_its_records_by_the_directory_the_core_reaches() {
        let gnome = |directory: &str| key(Front::Gnome, directory, None);
        assert_eq!(
            gnome("/home/dana/./work/../pitboard/"),
            "/home/dana/work/../pitboard"
        );
        assert_eq!(gnome("//home//dana/.pitboard"), "/home/dana/.pitboard");
        assert_eq!(gnome("/home/dana/.pitboard/.."), "/home/dana/.pitboard/..");
        assert_eq!(gnome("/"), "/");
        assert_eq!(gnome("/a/..b/.../c"), "/a/..b/.../c");
        assert_eq!(
            gnome("/home/Dana/Báo cáo"),
            "/home/Dana/Báo cáo",
            "names as they are"
        );
        let here = std::env::current_dir().expect("the tests' working directory");
        assert_eq!(
            gnome("./a/./b/../"),
            here.join("a/b/..")
                .to_str()
                .expect("a working directory in UTF-8"),
            "a relative one from the working directory, as the core reaches it"
        );
    }

    const DANA: &str = "/Users/dana/.pitboard";
    const SCRATCH: &str = "/tmp/test/.pitboard";

    /// A store as Foundation wrote it into UserDefaults, and as `stores::derive` writes it.
    const WORK_UPPER: &str = "7E15C34F-69EC-55B4-9542-F1C1FE3D7085";
    const WORK: &str = "7e15c34f-69ec-55b4-9542-f1c1fe3d7085";

    fn there(text: &str) -> std::io::Result<Option<String>> {
        Ok(Some(text.to_owned()))
    }

    fn unreadable() -> std::io::Result<Option<String>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "the stand-in refuses",
        ))
    }

    /// Exactly what the macOS app's `StoreRecord` and `PageRecord` wrote into UserDefaults,
    /// as `webStores` and `windowPages`, and its `EarlierStore` hands over: each Pitboard
    /// directory by its standardised path, each store as `UUID.uuidString` writes it, in
    /// upper case, sorted, and each page as `URL.absoluteString`.
    fn todays_format() -> EarlierWindowRecords {
        EarlierWindowRecords {
            stores: HashMap::from([
                (
                    DANA.to_owned(),
                    vec![
                        "323D12FB-2C52-55B5-BAEC-DF74FB60BC24".to_owned(),
                        WORK_UPPER.to_owned(),
                    ],
                ),
                (
                    SCRATCH.to_owned(),
                    vec!["8FF9E0E6-A7E2-53E2-A594-6E53DDADD38A".to_owned()],
                ),
            ]),
            pages: HashMap::from([(
                DANA.to_owned(),
                HashMap::from([(WORK_UPPER.to_owned(), "https://claude.ai/chat/1".to_owned())]),
            )]),
        }
    }

    /// What the macOS app's records hold is taken in its own case, by its own keys, and every
    /// store id is then the one `stores::derive` writes: a store compared as written would be
    /// nobody's, and deleted.
    #[test]
    fn todays_records_are_read_in_any_case_under_their_own_keys() {
        let earlier = Records::earlier(&todays_format());
        let dana = earlier.entry(DANA);
        assert_eq!(
            dana.stores,
            BTreeSet::from([
                "323d12fb-2c52-55b5-baec-df74fb60bc24".to_owned(),
                WORK.to_owned()
            ])
        );
        assert_eq!(
            dana.pages,
            BTreeMap::from([(WORK.to_owned(), "https://claude.ai/chat/1".to_owned())])
        );
        assert_eq!(
            earlier.entry(SCRATCH).stores,
            BTreeSet::from(["8ff9e0e6-a7e2-53e2-a594-6e53ddadd38a".to_owned()])
        );
        assert_eq!(earlier.entry("/Users/dana/.pitboard/"), Entry::default());
        assert_eq!(
            store_key(WORK_UPPER),
            crate::account_windows::stores::derive(
                pitboard_sites::CLAUDE.store_name,
                "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f"
            )
        );
    }

    /// Without the file, the earlier store is what there is, and it is to be kept at once;
    /// with it, the file wins and the earlier store is not read.
    #[test]
    fn the_file_wins_and_without_it_the_earlier_store_is_taken() {
        let earlier = Records::earlier(&todays_format());
        let loaded = load(&Ok(None), DANA, Some(&earlier)).expect("readable");
        assert!(!loaded.from_file);
        assert!(loaded.handed_over);
        assert!(loaded.entry.stores.contains(WORK));

        let file = format!(r#"{{"stores":{{"{DANA}":["{WORK}"]}}}}"#);
        let loaded = load(&there(&file), DANA, Some(&earlier)).expect("readable");
        assert!(loaded.from_file);
        assert!(!loaded.handed_over);
        assert_eq!(loaded.entry.stores, BTreeSet::from([WORK.to_owned()]));
        assert!(loaded.entry.pages.is_empty());

        let none = load(&Ok(None), DANA, None).expect("readable");
        assert_eq!(none.entry, Entry::default());
        assert!(!none.handed_over);
    }

    /// A file that is there and cannot be read, or whose text does not read as records, as a
    /// later version, a hand edit or damage may leave it, is not none: nothing is written over
    /// it, the earlier store is not taken in its place, and it says nothing of which stores
    /// another directory recorded. Taken as none, the first write would leave this
    /// directory's records alone in it, and a store another directory's account still uses
    /// would be deleted.
    #[test]
    fn a_file_that_cannot_be_read_is_never_written_over() {
        let earlier = Records::earlier(&todays_format());
        let damaged = format!(
            r#"{{"stores":{{"{DANA}":["{WORK}"],"{SCRATCH}":["{WORK}"]}},"pages":"a later format"}}"#
        );
        for file in [
            unreadable(),
            there("not json"),
            there(""),
            there("null"),
            there(&damaged),
        ] {
            let text = format!("{file:?}");
            assert_eq!(load(&file, DANA, Some(&earlier)), None, "{text}");
            assert_eq!(
                kept(&file, DANA, Some(&earlier), &Entry::default()),
                None,
                "{text}"
            );
            assert_eq!(
                shared(&file, DANA, Some(&earlier), &[WORK.into()], |_| true),
                None,
                "{text}"
            );
        }
    }

    /// A store id that is not a UUID, as a hand edit may leave one, names no store an app can
    /// make or delete, so it is dropped as the records are read, from the file and from the
    /// earlier store alike, its page with it, as the macOS app's `StoreRecord` and
    /// `PageRecord` dropped one. Kept, it would be asked of the app, which cannot name it, and
    /// stay asked for the whole launch.
    #[test]
    fn a_store_id_that_is_not_a_uuid_is_dropped() {
        let file = format!(
            r#"{{"stores":{{"{DANA}":["{WORK}","not a store",""],"{SCRATCH}":["{{{WORK}}}"]}},"pages":{{"{DANA}":{{"not a store":"https://claude.ai/chat/1","{WORK_UPPER}":"https://claude.ai/chat/2"}}}}}}"#
        );
        let loaded = load(&there(&file), DANA, None).expect("records");
        assert_eq!(loaded.entry.stores, BTreeSet::from([WORK.to_owned()]));
        assert_eq!(
            loaded.entry.pages,
            BTreeMap::from([(WORK.to_owned(), "https://claude.ai/chat/2".to_owned())])
        );
        let kept = kept(&there(&file), DANA, None, &loaded.entry).expect("written");
        assert!(!kept.contains("not a store"), "{kept}");
        assert_eq!(
            Records::read(&kept).expect("records").entry(SCRATCH),
            Entry::default()
        );

        let earlier = Records::earlier(&EarlierWindowRecords {
            stores: HashMap::from([(
                DANA.to_owned(),
                vec![
                    WORK_UPPER.to_owned(),
                    "7E15C34F69EC55B49542F1C1FE3D7085".to_owned(),
                ],
            )]),
            pages: HashMap::from([(
                DANA.to_owned(),
                HashMap::from([("7E15C34F".to_owned(), "https://claude.ai/".to_owned())]),
            )]),
        });
        assert_eq!(
            earlier.entry(DANA),
            Entry {
                stores: BTreeSet::from([WORK.to_owned()]),
                ..Entry::default()
            }
        );
    }

    /// A write sets this directory's records and keeps every other's, as the file holds them
    /// then, or as the earlier store held them where there is no file yet. A directory that
    /// records nothing is left out.
    #[test]
    fn a_write_keeps_every_other_directorys_records() {
        let earlier = Records::earlier(&todays_format());
        let mine = Entry {
            stores: BTreeSet::from([WORK.to_owned()]),
            ..Entry::default()
        };
        let text = kept(&Ok(None), DANA, Some(&earlier), &mine).expect("written");
        let written = Records::read(&text).expect("records");
        assert_eq!(written.entry(DANA), mine);
        assert_eq!(written.entry(SCRATCH), earlier.entry(SCRATCH));
        assert_eq!(
            text,
            format!(
                r#"{{"stores":{{"{DANA}":["{WORK}"],"{SCRATCH}":["8ff9e0e6-a7e2-53e2-a594-6e53ddadd38a"]}},"pages":{{}}}}"#
            )
        );

        let emptied = kept(&there(&text), DANA, None, &Entry::default()).expect("written");
        let emptied = Records::read(&emptied).expect("records");
        assert_eq!(emptied.entry(DANA), Entry::default());
        assert_eq!(emptied.entry(SCRATCH), earlier.entry(SCRATCH));
    }

    #[test]
    fn a_held_store_is_written_only_while_it_is_recorded() {
        let held = Entry {
            stores: BTreeSet::from([WORK.to_owned()]),
            held: BTreeSet::from([WORK.to_owned()]),
            ..Entry::default()
        };
        let text = kept(&Ok(None), DANA, None, &held).expect("written");
        assert_eq!(
            text,
            format!(
                r#"{{"stores":{{"{DANA}":["{WORK}"]}},"pages":{{}},"held":{{"{DANA}":["{WORK}"]}}}}"#
            )
        );
        assert_eq!(Records::read(&text).expect("records").entry(DANA), held);
        let upper = format!(
            r#"{{"stores":{{"{DANA}":["{WORK}"]}},"held":{{"{DANA}":["{WORK_UPPER}","not a store"]}}}}"#
        );
        assert_eq!(
            load(&there(&upper), DANA, None).expect("records").entry,
            held
        );
        let unrecorded = format!(r#"{{"stores":{{}},"held":{{"{DANA}":["{WORK}"]}}}}"#);
        assert_eq!(
            load(&there(&unrecorded), DANA, None)
                .expect("records")
                .entry,
            Entry::default()
        );
    }

    /// The record is kept per Pitboard directory: a copy of the app run with another home
    /// shares the web stores with the copy installed and none of its accounts.
    #[test]
    fn each_pitboard_directory_keeps_its_own_record() {
        let one = "00000000-0000-4000-8000-000000000001";
        let two = "00000000-0000-4000-8000-000000000002";
        let mut records = Records::default();
        let entry = |store: &str| Entry {
            stores: BTreeSet::from([store.to_owned()]),
            ..Entry::default()
        };
        records.set(DANA, &entry(one));
        records.set(SCRATCH, &entry(two));
        assert_eq!(records.entry(DANA), entry(one));
        assert_eq!(records.entry(SCRATCH), entry(two));
        records.set(DANA, &Entry::default());
        assert_eq!(records.entry(DANA), Entry::default());
        assert_eq!(records.entry(SCRATCH), entry(two));
        assert_eq!(
            records.text().as_deref(),
            Some(format!(r#"{{"stores":{{"{SCRATCH}":["{two}"]}},"pages":{{}}}}"#).as_str())
        );
    }

    /// A store another directory recorded is that one's while the directory is there, in
    /// whatever case it was written; once the directory is gone, the store is nobody's.
    #[test]
    fn a_store_another_directory_still_there_recorded_is_its_own() {
        let records = Records::earlier(&EarlierWindowRecords {
            stores: HashMap::from([
                (DANA.to_owned(), vec![WORK_UPPER.to_owned()]),
                (SCRATCH.to_owned(), vec![WORK_UPPER.to_owned()]),
            ]),
            pages: HashMap::new(),
        });
        assert_eq!(
            records.shared(SCRATCH, &[WORK.to_owned()], |_| true),
            [WORK]
        );
        assert_eq!(
            records.shared(SCRATCH, &[WORK.to_owned()], |dir| dir == SCRATCH),
            Vec::<String>::new(),
            "the other directory is gone"
        );
        let mine_only = Records::earlier(&EarlierWindowRecords {
            stores: HashMap::from([(SCRATCH.to_owned(), vec![WORK.to_owned()])]),
            pages: HashMap::new(),
        });
        assert!(
            mine_only
                .shared(SCRATCH, &[WORK.to_owned()], |_| true)
                .is_empty()
        );
    }
}
