// One store, as 2.7032.0 reads it (`index.chunk-D3OyLXgG.js` @615321, @615852), never run.

use crate::error::Then;
use crate::provider::folder::{AppSettings, Sidecar, Unread};
use serde::de::{Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::path::Path;

const FILE: &str = "config.json";
const SIDECAR: &str = "config.json.journal";

pub(super) fn read(root: &Path, naming_key: &str) -> Result<AppSettings, Unread> {
    // Where a test sees that a read opened Claude's settings at all.
    crate::fault::point("settings.read");
    let on_disk = match std::fs::read(root.join(FILE)) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(Unread::Unreadable(format!("`{FILE}`: {e}"))),
    };
    let file = on_disk.as_deref().map(digest);
    let (body, sidecar) = match std::fs::read(root.join(SIDECAR)) {
        Ok(written) => match valid_body(&written, file.as_deref())? {
            Some(body) => (pairs(body, true)?, Sidecar::Valid),
            None => (store(on_disk.as_deref())?, Sidecar::Stale),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (store(on_disk.as_deref())?, Sidecar::None)
        }
        Err(e) => return Err(Unread::Unreadable(format!("`{SIDECAR}`: {e}"))),
    };
    if let Some((_, named)) = body.iter().find(|(key, _)| key == naming_key) {
        let named = serde_json::from_str::<String>(named.get()).ok();
        if !named.is_some_and(|name| name.is_empty() || account_id(&name)) {
            return Err(unknown(
                format!(
                    "has a settings file, `config.json`, whose `{naming_key}` is not an account id"
                ),
                Then::OpenAndQuit,
            ));
        }
    }
    Ok(AppSettings {
        body,
        file,
        sidecar,
    })
}

// Only `config.json` is written, whole and by a rename. The sidecar stays: it no longer names
// the file, so the app drops it at its next start.
pub(super) fn write(
    permit: crate::permit::Permit,
    root: &Path,
    read: &AppSettings,
    body: &[u8],
) -> Result<(), Unread> {
    let path = root.join(FILE);
    let on_disk = match std::fs::read(&path) {
        Ok(bytes) => Some(digest(&bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(Unread::Unreadable(format!("`{FILE}`: {e}"))),
    };
    if on_disk != read.file {
        return Err(Unread::Unreadable(format!(
            "`{FILE}` changed while Pitboard read it"
        )));
    }
    crate::atomic::write_at(
        permit,
        &path,
        body,
        crate::atomic::Perms::MatchExisting,
        "settings.temp_written",
    )
    .map_err(|e| Unread::Unreadable(format!("`{FILE}` could not be written: {e}")))
}

// The name becomes the account's id and part of its park's folder name: one word, no path.
fn account_id(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn unknown(what: String, then: Then) -> Unread {
    Unread::Unknown { what, then }
}

fn unreadable_sidecar() -> Unread {
    unknown(
        "holds a settings write Pitboard cannot read, `config.json.journal`".into(),
        Then::OpenAndQuit,
    )
}

fn store(on_disk: Option<&[u8]>) -> Result<Vec<(String, Box<RawValue>)>, Unread> {
    on_disk.map_or(Ok(Vec::new()), |bytes| pairs(bytes, false))
}

// Valid by the app's own rule: field 1 is the body's digest, field 2 or 3 the file's on disk.
fn valid_body<'a>(written: &'a [u8], file: Option<&str>) -> Result<Option<&'a [u8]>, Unread> {
    let at = written
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(unreadable_sidecar)?;
    let (header, body) = (&written[..at], &written[at + 1..]);
    let header = std::str::from_utf8(header).map_err(|_| unreadable_sidecar())?;
    let fields: Vec<&str> = header.split(' ').collect();
    let field = |f: &str| {
        f == "-"
            || f == "."
            || (f.len() == 64
                && f.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    };
    if fields.len() != 3 || !fields.iter().all(|f| field(f)) {
        return Err(unreadable_sidecar());
    }
    let file = file.unwrap_or("-");
    Ok((fields[0] == digest(body) && (fields[1] == file || fields[2] == file)).then_some(body))
}

fn pairs(bytes: &[u8], sidecar: bool) -> Result<Vec<(String, Box<RawValue>)>, Unread> {
    let not_json = || {
        if sidecar {
            unreadable_sidecar()
        } else {
            unknown(
                "has a settings file, `config.json`, that is not valid JSON".into(),
                Then::CopyAside,
            )
        }
    };
    let Pairs(pairs) = serde_json::from_slice(bytes).map_err(|_| not_json())?;
    for (at, (key, _)) in pairs.iter().enumerate() {
        if pairs[..at].iter().any(|(earlier, _)| earlier == key) {
            return Err(unknown(
                format!("has a settings file, `config.json`, that names `{key}` twice"),
                Then::OpenAndQuit,
            ));
        }
    }
    Ok(pairs)
}

struct Pairs(Vec<(String, Box<RawValue>)>);

impl<'de> serde::Deserialize<'de> for Pairs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Pairs, D::Error> {
        struct InOrder;
        impl<'de> Visitor<'de> for InOrder {
            type Value = Pairs;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Pairs, A::Error> {
                let mut pairs = Vec::new();
                while let Some(pair) = map.next_entry::<String, Box<RawValue>>()? {
                    pairs.push(pair);
                }
                Ok(Pairs(pairs))
            }
        }
        deserializer.deserialize_map(InOrder)
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("a scratch folder")
    }

    const LIVE: &str = "{\n\t\"locale\": \"en-US\",\n\t\"oauth:tokenCache\": \"djEwAAAA\",\n\t\"lastKnownAccountUuid\": \"11111111-1111-1111-1111-111111111111\",\n\t\"userThemeMode\": \"dark\"\n}";

    fn sidecar(body: &str, over: &str) -> String {
        format!("{} {over} .\n{body}", digest(body.as_bytes()))
    }

    fn keys(read: &AppSettings) -> Vec<&str> {
        read.body.iter().map(|(key, _)| key.as_str()).collect()
    }

    #[test]
    fn a_settings_file_is_read_in_order_with_each_value_as_it_was() {
        let root = scratch();
        let read = read(root.path(), "lastKnownAccountUuid").expect("no file is an empty store");
        assert!(read.body.is_empty());
        assert_eq!(read.file, None);
        std::fs::write(root.path().join(FILE), LIVE).unwrap();
        let read = read_again(root.path());
        assert_eq!(
            keys(&read),
            [
                "locale",
                "oauth:tokenCache",
                "lastKnownAccountUuid",
                "userThemeMode"
            ]
        );
        assert_eq!(read.body[1].1.get(), "\"djEwAAAA\"");
        assert_eq!(read.file.as_deref(), Some(digest(LIVE.as_bytes()).as_str()));
        assert_eq!(read.sidecar, Sidecar::None);
    }

    fn read_again(root: &Path) -> AppSettings {
        read(root, "lastKnownAccountUuid").expect("read")
    }

    fn refusal(root: &Path) -> (String, Then) {
        match read(root, "lastKnownAccountUuid") {
            Err(Unread::Unknown { what, then }) => (what, then),
            other => panic!("{other:?}"),
        }
    }

    fn desktop_keys(json: &str) -> crate::provider::folder::Keys {
        crate::provider::folder::Keys::from_json(json.as_bytes()).unwrap()
    }

    fn put(root: &Path, keys: &crate::provider::folder::Keys) -> Result<(), Unread> {
        use crate::provider::folder::Folder;
        let app = super::super::Desktop;
        app.write_settings(
            crate::permit::Permit::for_a_test(),
            root,
            &read_again(root),
            keys,
        )
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses the write's file calls"
    )]
    fn a_splice_touches_only_the_account_keys() {
        use crate::provider::folder::Keys;
        let s = scratch();
        let file = s.path().join(FILE);
        std::fs::write(&file, LIVE).unwrap();
        crate::host::fs::testing::open_to_others(&file).unwrap();

        put(s.path(), &Keys::none()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "{\n\t\"locale\": \"en-US\",\n\t\"userThemeMode\": \"dark\"\n}",
            "the account's keys out, every other key in order, its value byte for byte"
        );
        assert!(
            !crate::host::fs::testing::is_private(&file).unwrap(),
            "the file keeps the mode it had"
        );

        let keys = desktop_keys(
            r#"{"lastKnownAccountUuid": "22222222-2222-2222-2222-222222222222", "oauth:tokenCache":   "djEw"}"#,
        );
        put(s.path(), &keys).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "{\n\t\"locale\": \"en-US\",\n\t\"userThemeMode\": \"dark\",\n\t\"oauth:tokenCache\": \"djEw\",\n\t\"lastKnownAccountUuid\": \"22222222-2222-2222-2222-222222222222\"\n}",
            "put in after the rest, in the order the register names them"
        );

        // A file changed since it was read is refused, not written over.
        let stale = read_again(s.path());
        std::fs::write(&file, "{\"locale\": \"fr-FR\"}").unwrap();
        use crate::provider::folder::Folder;
        let refused = super::super::Desktop.write_settings(
            crate::permit::Permit::for_a_test(),
            s.path(),
            &stale,
            &Keys::none(),
        );
        assert!(matches!(refused, Err(Unread::Unreadable(_))), "{refused:?}");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "{\"locale\": \"fr-FR\"}"
        );

        // No file yet: one is written.
        std::fs::remove_file(&file).unwrap();
        put(s.path(), &keys).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("djEw"));
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses the write's file calls"
    )]
    fn a_settings_write_deletes_nothing_of_claudes() {
        use crate::provider::folder::Keys;
        let s = scratch();
        std::fs::write(s.path().join(FILE), LIVE).unwrap();
        let pending = "{\n\t\"locale\": \"de-DE\",\n\t\"oauth:tokenCache\": \"djEwAAAA\"\n}";
        let left = sidecar(pending, &digest(LIVE.as_bytes()));
        std::fs::write(s.path().join(SIDECAR), &left).unwrap();
        assert_eq!(read_again(s.path()).sidecar, Sidecar::Valid);

        put(s.path(), &Keys::none()).unwrap();
        assert_eq!(
            std::fs::read_to_string(s.path().join(FILE)).unwrap(),
            "{\n\t\"locale\": \"de-DE\"\n}",
            "the write Claude left is the settings Pitboard writes"
        );
        assert_eq!(
            std::fs::read_to_string(s.path().join(SIDECAR)).unwrap(),
            left
        );
        assert_eq!(
            read_again(s.path()).sidecar,
            Sidecar::Stale,
            "which Claude drops"
        );
        let names: Vec<_> = std::fs::read_dir(s.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 2, "no temporary is left: {names:?}");
    }

    #[test]
    fn a_broken_file_or_a_key_named_twice_is_refused_and_left_as_it_was() {
        let root = scratch();
        std::fs::write(root.path().join(FILE), "{\"a\": ").unwrap();
        let (what, then) = refusal(root.path());
        assert!(what.contains("not valid JSON"), "{what}");
        assert_eq!(then, Then::CopyAside);
        std::fs::write(root.path().join(FILE), "[1]").unwrap();
        assert_eq!(refusal(root.path()).1, Then::CopyAside);
        assert_eq!(
            std::fs::read_to_string(root.path().join(FILE)).unwrap(),
            "[1]"
        );
    }

    #[test]
    fn a_settings_file_naming_a_key_twice_is_refused() {
        let root = scratch();
        std::fs::write(
            root.path().join(FILE),
            "{\"lastKnownAccountUuid\": \"a\", \"lastKnownAccountUuid\": \"b\"}",
        )
        .unwrap();
        let (what, then) = refusal(root.path());
        assert!(
            what.contains("names `lastKnownAccountUuid` twice"),
            "{what}"
        );
        assert_eq!(then, Then::OpenAndQuit);
    }

    #[test]
    fn a_name_that_is_not_an_account_id_is_refused_and_an_empty_one_names_nobody() {
        let root = scratch();
        let named = |name: &str| format!("{{\"lastKnownAccountUuid\": {name}}}");
        let longest = "a".repeat(64);
        for odd in [
            "7".to_string(),
            "null".to_string(),
            "\"../x\"".to_string(),
            "\"a/b\"".to_string(),
            "\"a b\"".to_string(),
            "\"a\\nb\"".to_string(),
            "\"a_b\"".to_string(),
            format!("\"{longest}a\""),
        ] {
            std::fs::write(root.path().join(FILE), named(&odd)).unwrap();
            let (what, then) = refusal(root.path());
            assert!(
                what.contains("whose `lastKnownAccountUuid` is not an account id"),
                "{odd}: {what}"
            );
            assert_eq!(then, Then::OpenAndQuit, "{odd}");
        }
        // A valid sidecar is the store, so its name is the one judged.
        std::fs::write(root.path().join(FILE), LIVE).unwrap();
        let written = named("\"../x\"");
        std::fs::write(
            root.path().join(SIDECAR),
            sidecar(&written, &digest(LIVE.as_bytes())),
        )
        .unwrap();
        assert!(refusal(root.path()).0.contains("is not an account id"));
        std::fs::remove_file(root.path().join(SIDECAR)).unwrap();

        let uuid = "11111111-1111-1111-1111-111111111111";
        for (taken, names) in [
            (String::new(), None),
            (uuid.to_string(), Some(uuid)),
            (longest.clone(), Some(longest.as_str())),
        ] {
            std::fs::write(root.path().join(FILE), named(&format!("\"{taken}\""))).unwrap();
            let read = read_again(root.path());
            assert_eq!(keys(&read), ["lastKnownAccountUuid"], "{taken}");
            assert_eq!(
                read.named(&super::super::Desktop).as_deref(),
                names,
                "{taken}"
            );
        }
    }

    #[test]
    fn a_valid_sidecar_is_the_settings_and_a_stale_one_is_ignored() {
        let root = scratch();
        std::fs::write(root.path().join(FILE), LIVE).unwrap();
        let written = "{\n\t\"locale\": \"fr-FR\"\n}";
        std::fs::write(
            root.path().join(SIDECAR),
            sidecar(written, &digest(LIVE.as_bytes())),
        )
        .unwrap();
        let read = read_again(root.path());
        assert_eq!(read.sidecar, Sidecar::Valid);
        assert_eq!(keys(&read), ["locale"]);
        assert_eq!(read.file.as_deref(), Some(digest(LIVE.as_bytes()).as_str()));

        std::fs::write(root.path().join(SIDECAR), sidecar(written, &"0".repeat(64))).unwrap();
        let read = read_again(root.path());
        assert_eq!(read.sidecar, Sidecar::Stale);
        assert_eq!(keys(&read).len(), 4, "the file is the store");

        std::fs::remove_file(root.path().join(FILE)).unwrap();
        std::fs::write(root.path().join(SIDECAR), sidecar(written, "-")).unwrap();
        let read = read_again(root.path());
        assert_eq!((read.sidecar, read.file), (Sidecar::Valid, None));

        std::fs::write(
            root.path().join(SIDECAR),
            format!("{} - .\n{written}", "1".repeat(64)),
        )
        .unwrap();
        assert_eq!(read_again(root.path()).sidecar, Sidecar::Stale, "torn");
        assert!(root.path().join(SIDECAR).exists(), "never removed");
    }

    #[test]
    fn a_sidecar_pitboard_cannot_read_refuses() {
        let root = scratch();
        std::fs::write(root.path().join(FILE), LIVE).unwrap();
        for written in [
            "no header line".to_string(),
            "a b c\n{}".to_string(),
            format!("{} {}\n{{}}", "a".repeat(64), "b".repeat(64)),
            format!("{} - .  \n{{}}", "a".repeat(64)),
            sidecar("not json", "-").replace(" - ", &format!(" {} ", digest(LIVE.as_bytes()))),
        ] {
            std::fs::write(root.path().join(SIDECAR), &written).unwrap();
            let (what, then) = refusal(root.path());
            assert!(what.contains("config.json.journal"), "{written:?}: {what}");
            assert_eq!(then, Then::OpenAndQuit);
        }
    }
}
