//! Whose login a Claude Desktop data folder holds, and whether Pitboard knows the account.
//!
//! The session is told by the hash of its cookie's ciphertext and the account by the uuid
//! `config.json` names. Neither alone is enough: the uuid is what the app last saw, which
//! Log out leaves behind (experiment E1b), and the session says nothing of whose it is. A
//! jar with no session is signed out whatever the uuid says. Where no uuid was named, the
//! app writes the new account's before its session reaches the jar (E4), and the bundle
//! replaces a uuid Log out left behind by the same write, so a session Pitboard has not
//! seen under a uuid it knows is that account's: the app renews sessions in place.

use super::cookies;
use super::paths::{config_file, cookies_db};
use super::types::TreeIdentity;
use crate::context::Context;
use crate::error::Error;
use crate::provider::ProviderId;
use crate::state::{Detail, Key, State};
use serde_json::Value;
use std::path::Path;

/// The register's entry for whether a sign-in replaces a `lastKnownAccountUuid` that Log
/// out left behind.
pub(crate) const UUID_TRACKS_SIGNIN: &str = "desktop_uuid_tracks_signin";

/// The session the cookie jar in `root` holds, told by its fingerprint and expiry. `None`
/// where there is none: no jar, or a jar with no session in it. A jar that cannot be read
/// is never read as signed out.
///
/// Apart from [`identify_tree`] because a park keeps no `config.json`: the account a park
/// holds is written in its manifest, and its session is checked against that.
pub(crate) fn session_of(ctx: &Context, root: &Path) -> Result<Option<cookies::Session>, Error> {
    let db = cookies_db(root);
    let table = match ctx.host().cookie_table(&db) {
        Ok(table) => table,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        // The app writing its jar as it was read, or `sqlite3` waiting on its lock: gone
        // once Claude is quit, so said as that rather than as a file nobody may read.
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ResourceBusy | std::io::ErrorKind::TimedOut
            ) =>
        {
            return Err(Error::DesktopDataBusy { path: db });
        }
        // A file on this Mac that could not be read: nothing was asked of anybody, so it is
        // not a question Anthropic failed to answer.
        Err(source) => {
            return Err(Error::DesktopDataInaccessible { path: db, source });
        }
    };
    cookies::session(&table).map_err(|e| Error::DesktopFormatUnknown {
        what: db.display().to_string(),
        found: e.to_string(),
    })
}

/// Whose login the folder at `root` holds. `None` where it holds none: no jar, or a jar
/// with no session in it. A jar that cannot be read is never read as signed out.
pub(crate) fn identify_tree(ctx: &Context, root: &Path) -> Result<Option<TreeIdentity>, Error> {
    let Some(session) = session_of(ctx, root)? else {
        return Ok(None);
    };
    let keys = super::config::read_keys(&config_file(root))?;
    // A session with no account named for it is a sign-in the app has not finished
    // writing down. Nothing was asked of anybody, so it is not Anthropic's to answer.
    let uuid = keys
        .0
        .get("lastKnownAccountUuid")
        .and_then(Value::as_str)
        .filter(|uuid| !uuid.is_empty())
        .ok_or(Error::DesktopSignInIncomplete)?;
    Ok(Some(TreeIdentity {
        account_uuid: uuid.to_string(),
        fingerprint: session.fingerprint,
        expires_at: session.expires_at,
    }))
}

/// Who the live login is, as far as Pitboard's accounts say.
#[derive(Debug)]
pub(crate) enum LiveOwner {
    /// Signed out.
    Nobody,
    /// An account Pitboard has, whose session it is.
    Enrolled(Key),
    /// An account Pitboard does not have.
    NotEnrolled(TreeIdentity),
}

/// Whose `live` is among `state`'s Claude Desktop accounts. A known uuid under a session
/// Pitboard has not seen is that account's while the register's
/// `desktop_uuid_tracks_signin` holds: another account's sign-in would have replaced the
/// uuid Log out left behind (E1b), and the app renews a session in place. Were it dated
/// unverified, such a session would be refused, naming the uuid's label, and enrolling that
/// label would be the confirmation. A session Pitboard knows as another account's is always
/// refused.
pub(crate) fn whose(state: &State, live: Option<TreeIdentity>) -> Result<LiveOwner, Error> {
    let Some(live) = live else {
        return Ok(LiveOwner::Nobody);
    };
    // A session Pitboard has seen as another account's, under this uuid: the uuid is stale
    // or the session moved, and taking either at its word would file one account's login
    // under another's name.
    if let Some(owner) = state.accounts.iter().find(|a| {
        a.provider() == ProviderId::Desktop
            && a.account_uuid != live.account_uuid
            && matches!(
                &a.detail,
                Detail::Desktop { session_fingerprint, .. }
                    if *session_fingerprint == live.fingerprint
            )
    }) {
        return Err(Error::DesktopIdentityUnconfirmed {
            label: owner.label.clone(),
        });
    }
    let Some(account) = state.by_uuid(ProviderId::Desktop, &live.account_uuid) else {
        return Ok(LiveOwner::NotEnrolled(live));
    };
    let seen = matches!(
        &account.detail,
        Detail::Desktop { session_fingerprint, .. } if *session_fingerprint == live.fingerprint
    );
    if seen || crate::assumptions::verified(ProviderId::Desktop, UUID_TRACKS_SIGNIN) {
        Ok(LiveOwner::Enrolled(account.key()))
    } else {
        Err(Error::DesktopIdentityUnconfirmed {
            label: account.label.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::memory::MemoryHost;
    use crate::provider::desktop::types::{CookieRow, CookieTable};
    use crate::state::{Account, Detail, State};
    use std::path::PathBuf;

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "pitboard-desktop-identity-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch dir");
        Scratch(root)
    }

    const UUID: &str = "11111111-1111-1111-1111-111111111111";

    fn signed_in(value: &[u8]) -> CookieTable {
        CookieTable {
            meta_version: 24,
            rows: vec![CookieRow {
                host_key: ".claude.ai".into(),
                name: "sessionKey".into(),
                encrypted_value: value.to_vec(),
                expires_utc: (1_793_318_400 + 11_644_473_600) * 1_000_000,
            }],
        }
    }

    /// A data folder in `root` with a cookie jar planted as `jar` and a config naming `uuid`.
    fn folder(root: &Path, jar: Option<CookieTable>, uuid: Option<&str>) -> (Context, PathBuf) {
        let mem = MemoryHost::new();
        let support = root.join("Claude");
        std::fs::create_dir_all(&support).unwrap();
        if let Some(jar) = jar {
            std::fs::write(support.join("Cookies"), b"").unwrap();
            mem.plant_cookies(&support.join("Cookies"), jar);
        }
        let mut config = serde_json::json!({ "locale": "en-US" });
        if let Some(uuid) = uuid {
            config["lastKnownAccountUuid"] = uuid.into();
        }
        std::fs::write(support.join("config.json"), config.to_string()).unwrap();
        let ctx = Context::new(root.to_path_buf())
            .with_pitboard_home(root.join(".pitboard"))
            .with_desktop_dir(support.to_string_lossy().into())
            .with_memory_stores(mem);
        (ctx, support)
    }

    #[test]
    fn a_folder_is_identified_by_its_jar_and_its_config() {
        let s = scratch("signed-in");
        let (ctx, support) = folder(&s.0, Some(signed_in(b"v10session")), Some(UUID));
        let found = identify_tree(&ctx, &support).unwrap().expect("signed in");
        assert_eq!(found.account_uuid, UUID);
        use sha2::{Digest, Sha256};
        assert_eq!(
            found.fingerprint,
            hex::encode(Sha256::digest(b"v10session"))
        );
        assert_eq!(found.expires_at, Some(1_793_318_400));
    }

    #[test]
    fn a_folder_with_no_session_is_signed_out() {
        let s = scratch("signed-out");
        // No jar at all, as before the first launch.
        let (ctx, support) = folder(&s.0, None, None);
        assert_eq!(identify_tree(&ctx, &support).unwrap(), None);
        // A jar with no session in it, as after Log out; experiment E1b found the uuid is
        // left behind naming the account that left, and it names nobody.
        let (ctx, support) = folder(&s.0, Some(signed_in(b"v10x")), Some(UUID));
        let mut empty = signed_in(b"v10x");
        empty.rows.clear();
        let mem = MemoryHost::new();
        mem.plant_cookies(&support.join("Cookies"), empty);
        let ctx = ctx.with_memory_stores(mem);
        assert_eq!(identify_tree(&ctx, &support).unwrap(), None);
    }

    /// A jar that cannot be read is never read as signed out, and is said to be a file
    /// Pitboard could not read, not a question Anthropic did not answer: nothing was asked.
    #[test]
    fn an_unreadable_jar_is_not_signed_out() {
        let s = scratch("unreadable");
        let (ctx, support) = folder(&s.0, None, Some(UUID));
        // There, with nothing planted, which the memory host answers as a failed read.
        std::fs::write(support.join("Cookies"), b"").unwrap();
        let err = identify_tree(&ctx, &support).unwrap_err();
        match &err {
            Error::DesktopDataInaccessible { path, .. } => {
                assert_eq!(path, &support.join("Cookies"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(err.cause(), None);
        let said = err.to_string();
        assert!(!said.contains("Anthropic"), "{said}");
        assert!(!said.contains("connection"), "{said}");
    }

    /// A session whose account the app has not written down yet is a sign-in still
    /// finishing on this Mac, and is said as that: nothing was asked of Anthropic, so
    /// neither it nor the network is blamed.
    #[test]
    fn a_session_without_a_uuid_is_not_identified() {
        let s = scratch("no-uuid");
        let (ctx, support) = folder(&s.0, Some(signed_in(b"v10session")), None);
        let err = identify_tree(&ctx, &support).unwrap_err();
        assert!(matches!(err, Error::DesktopSignInIncomplete), "{err:?}");
        assert_eq!(err.cause(), None);
        let said = err.to_string();
        assert!(
            said.contains("Open Claude, let it finish signing in, quit it, then try again"),
            "{said}"
        );
        assert!(!said.contains("Anthropic"), "{said}");
        assert!(!said.contains("connection"), "{said}");
    }

    /// A jar the app is writing as it is read, or one `sqlite3` gave up waiting on, is
    /// busy and not unreadable: quitting Claude is all it takes, and the error says so.
    #[test]
    fn a_busy_jar_says_to_quit_claude() {
        let s = scratch("busy");
        for kind in [
            std::io::ErrorKind::ResourceBusy,
            std::io::ErrorKind::TimedOut,
        ] {
            let (ctx, support) = folder(&s.0, Some(signed_in(b"v10session")), Some(UUID));
            let mem = MemoryHost::new();
            mem.jar_fails(kind);
            let ctx = ctx.with_memory_stores(mem);
            let err = identify_tree(&ctx, &support).unwrap_err();
            match &err {
                Error::DesktopDataBusy { path } => assert_eq!(path, &support.join("Cookies")),
                other => panic!("{kind:?}: {other:?}"),
            }
            assert_eq!(err.exit_code(), 1, "{kind:?}: worth trying again");
            let said = err.to_string();
            assert!(said.contains("Quit Claude"), "{said}");
            assert!(!said.contains("Anthropic"), "{said}");
        }
    }

    #[test]
    fn a_jar_of_another_format_is_refused() {
        let s = scratch("format");
        let mut jar = signed_in(b"v10session");
        jar.meta_version = 25;
        let (ctx, support) = folder(&s.0, Some(jar), Some(UUID));
        assert!(matches!(
            identify_tree(&ctx, &support),
            Err(Error::DesktopFormatUnknown { .. })
        ));
        let (ctx, support) = folder(&s.0, Some(signed_in(b"v11session")), Some(UUID));
        assert!(matches!(
            identify_tree(&ctx, &support),
            Err(Error::DesktopFormatUnknown { .. })
        ));
    }

    fn enrolled(label: &str, uuid: &str, fingerprint: &str) -> Account {
        Account {
            last_used_at: None,
            label: label.into(),
            account_uuid: uuid.into(),
            email: String::new(),
            detail: Detail::Desktop {
                organization_uuid: None,
                session_fingerprint: fingerprint.into(),
                session_expires_at: None,
            },
            parked: None,
        }
    }

    fn live(uuid: &str, fingerprint: &str) -> TreeIdentity {
        TreeIdentity {
            account_uuid: uuid.into(),
            fingerprint: fingerprint.into(),
            expires_at: None,
        }
    }

    #[test]
    fn whose_login_is_live_is_told_by_uuid_and_fingerprint() {
        let mut state = State::default();
        state.upsert(enrolled("work", UUID, "aaaa"));
        // Claude Code's account of the same uuid is never the Desktop one.
        state.upsert(Account {
            detail: Detail::Codex {
                workspace_id: None,
                plan: None,
            },
            ..enrolled("other", "22222222-2222-2222-2222-222222222222", "")
        });

        assert!(matches!(whose(&state, None), Ok(LiveOwner::Nobody)));
        match whose(&state, Some(live(UUID, "aaaa"))) {
            Ok(LiveOwner::Enrolled(key)) => {
                assert_eq!(key, Key::new(ProviderId::Desktop, "work"));
            }
            other => panic!("{other:?}"),
        }
        let stranger = live("22222222-2222-2222-2222-222222222222", "bbbb");
        match whose(&state, Some(stranger.clone())) {
            Ok(LiveOwner::NotEnrolled(found)) => assert_eq!(found, stranger),
            other => panic!("{other:?}"),
        }
    }

    /// A session Pitboard knows as one account's, under another account's uuid, is the
    /// uuid going stale or the session moving: either way neither name can be trusted, and
    /// filing it under the uuid would put one account's login under another's name.
    #[test]
    fn a_known_session_under_another_uuid_is_refused() {
        const OTHER: &str = "33333333-3333-3333-3333-333333333333";
        let mut state = State::default();
        state.upsert(enrolled("work", UUID, "aaaa"));
        state.upsert(enrolled("home", OTHER, "bbbb"));
        let before = serde_json::to_string(&state).unwrap();

        // The uuid is work's, the session is home's.
        match whose(&state, Some(live(UUID, "bbbb"))) {
            Err(Error::DesktopIdentityUnconfirmed { label }) => assert_eq!(label, "home"),
            other => panic!("{other:?}"),
        }
        // And under a uuid Pitboard has never seen, it is not a stranger either.
        let stranger = "44444444-4444-4444-4444-444444444444";
        match whose(&state, Some(live(stranger, "bbbb"))) {
            Err(Error::DesktopIdentityUnconfirmed { label }) => assert_eq!(label, "home"),
            other => panic!("{other:?}"),
        }
        assert_eq!(serde_json::to_string(&state).unwrap(), before);
    }

    /// The refusal is met two ways, and its message says both, so neither reads as wrong:
    /// a session Pitboard has not seen under the label's account, or the label's session
    /// under another account.
    #[test]
    fn the_refusal_says_both_ways_it_is_met() {
        let said = Error::DesktopIdentityUnconfirmed {
            label: "work".into(),
        }
        .to_string();
        assert!(said.contains("a session Pitboard has not seen"), "{said}");
        assert!(
            said.contains("`work`'s session under another account"),
            "{said}"
        );
        assert!(!said.contains("changed since"), "{said}");
        assert!(
            said.contains("Open Claude, check which account it shows, quit it"),
            "{said}"
        );
        assert!(said.contains("pitboard enroll desktop/work"), "{said}");
    }

    /// Claude replaces an account's session without signing it out: on 5 October 2026 a
    /// `session_stale_relogin` was met with a new `sessionKey` for the same uuid. The
    /// register's `desktop_uuid_tracks_signin` says a sign-in by anybody else would have
    /// replaced the uuid too, so a session Pitboard has not seen under a known uuid is that
    /// account's, and nothing is written by asking.
    #[test]
    fn a_renewed_session_under_a_known_uuid_is_that_account() {
        assert!(crate::assumptions::verified(
            ProviderId::Desktop,
            UUID_TRACKS_SIGNIN
        ));
        let mut state = State::default();
        state.upsert(enrolled("work", UUID, "aaaa"));
        let before = serde_json::to_string(&state).unwrap();
        match whose(&state, Some(live(UUID, "cccc"))) {
            Ok(LiveOwner::Enrolled(key)) => {
                assert_eq!(key, Key::new(ProviderId::Desktop, "work"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(serde_json::to_string(&state).unwrap(), before);
    }
}
