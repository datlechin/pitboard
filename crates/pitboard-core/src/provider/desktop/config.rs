//! The three keys of Claude Desktop's `config.json` that Pitboard moves with the account.
//!
//! Everything else in the file belongs to the machine: its window layout, its theme, its
//! caches. A switch reads the three, and writes them back in place, leaving every other
//! key as it was and where it was.

use super::paths::CONFIG_KEYS;
use crate::error::Error;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// The account's keys of a `config.json`, any of which may be missing.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct ConfigKeys(pub BTreeMap<String, Value>);

/// Says which keys there are and how long each value is, and nothing of what any value is:
/// two of the three are the app's token cache, which a failed assertion or a debug log
/// must never carry.
impl std::fmt::Debug for ConfigKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConfigKeys ")?;
        let mut map = f.debug_map();
        for (key, value) in &self.0 {
            map.entry(key, &format_args!("<{} bytes>", value.to_string().len()));
        }
        map.finish()
    }
}

/// The account's keys in the config at `path`. A folder Claude has never written a config
/// into holds none.
pub(crate) fn read_keys(path: &Path) -> Result<ConfigKeys, Error> {
    let config = read(path)?;
    Ok(ConfigKeys(
        CONFIG_KEYS
            .iter()
            .filter_map(|&key| Some((key.to_string(), config.get(key)?.clone())))
            .collect(),
    ))
}

/// Writes `incoming`'s keys into the config at `path`: each of the three it has is set,
/// each it lacks is removed, and every other key stays as it was, in its place. A key of
/// `incoming` that is not one of the three is never written. The file keeps its mode, and
/// one made here is private, since what it holds is a login.
pub(crate) fn splice(path: &Path, incoming: &ConfigKeys) -> Result<(), Error> {
    let mut config = read(path)?;
    for key in CONFIG_KEYS {
        match (incoming.0.get(key), config.get_mut(key)) {
            (Some(value), Some(there)) => *there = value.clone(),
            (Some(value), None) => {
                config.insert(key.to_string(), value.clone());
            }
            (None, _) => {
                config.shift_remove(key);
            }
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&Value::Object(config)).map_err(|e| {
        Error::DesktopFormatUnknown {
            what: "config.json".into(),
            found: e.to_string(),
        }
    })?;
    bytes.push(b'\n');
    crate::atomic::write(path, &bytes, crate::atomic::Perms::MatchExisting).map_err(|source| {
        Error::DesktopDataInaccessible {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// The config at `path` as an object. Missing is empty; anything but an object is refused.
fn read(path: &Path) -> Result<Map<String, Value>, Error> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(source) => {
            return Err(Error::DesktopDataInaccessible {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let unknown = |found: String| Error::DesktopFormatUnknown {
        what: "config.json".into(),
        found,
    };
    // The parser's message says where it stopped, and holds nothing of the file.
    match serde_json::from_slice(&bytes).map_err(|e| unknown(e.to_string()))? {
        Value::Object(config) => Ok(config),
        _ => Err(unknown("not an object".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "pitboard-desktop-config-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch dir");
        Scratch(root)
    }

    const LIVE: &str = r#"{
  "locale": "en-US",
  "oauth:tokenCache": "djEwAAAA",
  "bootFrameLayout.sidebar": 240,
  "oauth:tokenCacheV2": "djEwBBBB",
  "lastKnownAccountUuid": "11111111-1111-1111-1111-111111111111",
  "dxt:allowlistCache:org-a": {"x": 1},
  "userThemeMode": "dark"
}"#;

    fn keys(pairs: &[(&str, Value)]) -> ConfigKeys {
        ConfigKeys(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        )
    }

    fn key_order(path: &Path) -> Vec<String> {
        let found: serde_json::Map<String, Value> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        found.keys().cloned().collect()
    }

    #[test]
    fn splice_touches_only_three_keys() {
        let s = scratch("three");
        let config = s.0.join("config.json");
        std::fs::write(&config, LIVE).unwrap();

        let read = read_keys(&config).unwrap();
        assert_eq!(
            read,
            keys(&[
                ("oauth:tokenCache", json!("djEwAAAA")),
                ("oauth:tokenCacheV2", json!("djEwBBBB")),
                (
                    "lastKnownAccountUuid",
                    json!("11111111-1111-1111-1111-111111111111")
                ),
            ])
        );

        let incoming = keys(&[
            ("oauth:tokenCache", json!("djEwCCCC")),
            ("oauth:tokenCacheV2", json!("djEwDDDD")),
            (
                "lastKnownAccountUuid",
                json!("22222222-2222-2222-2222-222222222222"),
            ),
        ]);
        splice(&config, &incoming).unwrap();

        assert_eq!(read_keys(&config).unwrap(), incoming);
        let after: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
        let before: Value = serde_json::from_str(LIVE).unwrap();
        for key in [
            "locale",
            "bootFrameLayout.sidebar",
            "dxt:allowlistCache:org-a",
            "userThemeMode",
        ] {
            assert_eq!(after[key], before[key], "{key}");
        }
        assert_eq!(
            key_order(&config),
            [
                "locale",
                "oauth:tokenCache",
                "bootFrameLayout.sidebar",
                "oauth:tokenCacheV2",
                "lastKnownAccountUuid",
                "dxt:allowlistCache:org-a",
                "userThemeMode"
            ],
            "every key stays where it was"
        );

        // A key that is not one of the three is never written, whatever is handed in.
        let mut sneaky = incoming.clone();
        sneaky.0.insert("locale".into(), json!("fr-FR"));
        splice(&config, &sneaky).unwrap();
        let after: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
        assert_eq!(after["locale"], "en-US");
    }

    #[test]
    fn splice_removes_keys_the_incoming_account_lacks() {
        let s = scratch("remove");
        let config = s.0.join("config.json");
        std::fs::write(&config, LIVE).unwrap();

        // An account that signed in before the second cache existed.
        let incoming = keys(&[
            ("oauth:tokenCache", json!("djEwCCCC")),
            (
                "lastKnownAccountUuid",
                json!("22222222-2222-2222-2222-222222222222"),
            ),
        ]);
        splice(&config, &incoming).unwrap();
        assert_eq!(read_keys(&config).unwrap(), incoming);
        assert!(!key_order(&config).contains(&"oauth:tokenCacheV2".to_string()));

        // Signed out: none of the three.
        splice(&config, &ConfigKeys::default()).unwrap();
        assert_eq!(read_keys(&config).unwrap(), ConfigKeys::default());
        assert_eq!(
            key_order(&config),
            [
                "locale",
                "bootFrameLayout.sidebar",
                "dxt:allowlistCache:org-a",
                "userThemeMode"
            ]
        );

        // And back, the keys taking their place at the end.
        splice(&config, &incoming).unwrap();
        assert_eq!(read_keys(&config).unwrap(), incoming);
    }

    #[test]
    fn splice_keeps_the_mode() {
        let s = scratch("mode");
        let config = s.0.join("config.json");
        std::fs::write(&config, LIVE).unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();

        splice(&config, &ConfigKeys::default()).unwrap();

        let mode = std::fs::metadata(&config).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }

    /// A folder Claude has never written a config into holds none of the three keys, and a
    /// config Pitboard cannot read is refused rather than read as one with none of them.
    #[test]
    fn a_missing_config_is_empty_and_a_broken_one_is_refused() {
        let s = scratch("broken");
        let config = s.0.join("config.json");
        assert_eq!(read_keys(&config).unwrap(), ConfigKeys::default());

        std::fs::write(&config, b"{\"locale\": ").unwrap();
        assert!(matches!(
            read_keys(&config),
            Err(Error::DesktopFormatUnknown { .. })
        ));
        assert!(matches!(
            splice(&config, &ConfigKeys::default()),
            Err(Error::DesktopFormatUnknown { .. })
        ));
        assert_eq!(std::fs::read(&config).unwrap(), b"{\"locale\": ");

        std::fs::write(&config, b"[1, 2]").unwrap();
        assert!(matches!(
            read_keys(&config),
            Err(Error::DesktopFormatUnknown { .. })
        ));
    }

    /// Splicing into a folder with no config yet writes one with the three keys alone,
    /// private, since what it holds is a login.
    #[test]
    fn splice_into_a_folder_with_no_config_writes_one() {
        let s = scratch("fresh");
        let config = s.0.join("config.json");
        let incoming = keys(&[(
            "lastKnownAccountUuid",
            json!("22222222-2222-2222-2222-222222222222"),
        )]);
        splice(&config, &incoming).unwrap();
        assert_eq!(read_keys(&config).unwrap(), incoming);
        let mode = std::fs::metadata(&config).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    /// Two of the keys are the app's token cache, and printing them never shows it.
    #[test]
    fn printing_the_keys_never_shows_a_value() {
        let keys = ConfigKeys(
            [
                ("oauth:tokenCache".to_string(), json!("djEwSECRETCACHE")),
                ("lastKnownAccountUuid".to_string(), json!("uuid-of-someone")),
            ]
            .into_iter()
            .collect(),
        );
        let printed = format!("{keys:?}");
        assert!(!printed.contains("SECRET"), "{printed}");
        assert!(!printed.contains("uuid-of-someone"), "{printed}");
        assert!(printed.contains("oauth:tokenCache"), "{printed}");
        assert!(printed.contains("<17 bytes>"), "{printed}");
        let pretty = format!("{keys:#?}");
        assert!(!pretty.contains("SECRET"), "{pretty}");
    }
}
