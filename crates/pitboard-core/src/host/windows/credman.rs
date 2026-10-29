// Credential Manager's generic items, as `Bun.secrets` keeps one for Claude Code
// (`credman_attributes`): `<service>/<account>`, the account its user name, the text's UTF-8 as
// its blob, enterprise persistence, no comment and no attributes. Names alone for the rest.

use crate::context::Context;
use crate::host::SecretStore;
use crate::permit::Permit;
use crate::provider::claude::pieces::BLOB;
use crate::store::{Backend, Cost, Error, RawStore};
use std::io;
use windows_sys::Win32::Foundation::{ERROR_NO_SUCH_LOGON_SESSION, ERROR_NOT_FOUND};
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_ENTERPRISE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredEnumerateW, CredFree,
    CredReadW, CredWriteW,
};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

// SAFETY: `text` is null or NUL-terminated, as every string CredReadW and CredEnumerateW hand
// back is.
unsafe fn text_at(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    let mut len = 0;
    // SAFETY: read up to the NUL the caller promises.
    while unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `text` holds `len` units before its NUL.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) })
}

fn not_found(failed: &io::Error) -> bool {
    failed
        .raw_os_error()
        .is_some_and(|code| code as u32 == ERROR_NOT_FOUND)
}

// `filter` is a prefix and `*`, as CredEnumerateW takes one.
pub(crate) fn names(filter: &str) -> io::Result<Vec<String>> {
    let filter = wide(filter);
    let mut count = 0u32;
    let mut found: *mut *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: a NUL-terminated filter, and two out-params of this frame; on success `found` is
    // an array of `count` items, freed below.
    if unsafe { CredEnumerateW(filter.as_ptr(), 0, &raw mut count, &raw mut found) } == 0 {
        let failed = io::Error::last_os_error();
        return if not_found(&failed) {
            Ok(Vec::new())
        } else {
            Err(failed)
        };
    }
    let names = (0..count as usize)
        // SAFETY: `found` holds `count` pointers to items, each with a NUL-terminated name.
        .map(|at| unsafe { text_at((**found.add(at)).TargetName) })
        .collect();
    // SAFETY: the array CredEnumerateW allocated, freed this once.
    unsafe { CredFree(found.cast()) };
    Ok(names)
}

// What a generic item holds, as Windows keeps it.
#[cfg_attr(not(any(test, feature = "test-support")), allow(dead_code))]
pub(crate) struct Item {
    pub(crate) kind: u32,
    pub(crate) persist: u32,
    pub(crate) user: String,
    pub(crate) comment: Option<String>,
    pub(crate) attributes: u32,
    pub(crate) blob: Vec<u8>,
}

pub(crate) fn item(target: &str) -> io::Result<Option<Item>> {
    let target = wide(target);
    let mut found: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: a NUL-terminated name, and an out-param of this frame, an item freed below.
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &raw mut found) } == 0 {
        let failed = io::Error::last_os_error();
        return if not_found(&failed) {
            Ok(None)
        } else {
            Err(failed)
        };
    }
    // SAFETY: CredReadW filled `found` with one item, which lives until the CredFree below.
    let held = unsafe { &*found };
    let blob = if held.CredentialBlob.is_null() {
        Vec::new()
    } else {
        // SAFETY: the blob holds CredentialBlobSize bytes.
        unsafe { std::slice::from_raw_parts(held.CredentialBlob, held.CredentialBlobSize as usize) }
            .to_vec()
    };
    // SAFETY: both are null or NUL-terminated.
    let (user, comment) = unsafe {
        (
            text_at(held.UserName),
            (!held.Comment.is_null()).then(|| text_at(held.Comment)),
        )
    };
    let read = Item {
        kind: held.Type,
        persist: held.Persist,
        user,
        comment,
        attributes: held.AttributeCount,
        blob,
    };
    // SAFETY: the item CredReadW allocated, freed this once.
    unsafe { CredFree(found.cast()) };
    Ok(Some(read))
}

pub(crate) fn held(target: &str) -> io::Result<bool> {
    item(target).map(|found| found.is_some())
}

pub(crate) fn write(target: &str, user: &str, blob: &[u8]) -> io::Result<()> {
    let (target, user) = (wide(target), wide(user));
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_ptr().cast_mut(),
        UserName: user.as_ptr().cast_mut(),
        CredentialBlobSize: u32::try_from(blob.len())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
        CredentialBlob: blob.as_ptr().cast_mut(),
        Persist: CRED_PERSIST_ENTERPRISE,
        ..Default::default()
    };
    // SAFETY: the item's strings and blob outlive the call, which copies them.
    if unsafe { CredWriteW(&raw const credential, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// True where there was one to delete.
pub(crate) fn delete(target: &str) -> io::Result<bool> {
    let target = wide(target);
    // SAFETY: a NUL-terminated name.
    if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        let failed = io::Error::last_os_error();
        return if not_found(&failed) {
            Ok(false)
        } else {
            Err(failed)
        };
    }
    Ok(true)
}

// What a call failed with, for a store: a network logon has no credential set, which says
// nothing of what is kept (docs/facts/windows-runners.md).
pub(super) fn failed(e: &io::Error, said: impl FnOnce(String) -> Error) -> Error {
    if e.raw_os_error()
        .is_some_and(|code| code as u32 == ERROR_NO_SUCH_LOGON_SESSION)
    {
        return Error::Locked;
    }
    said(e.to_string())
}

// Another program's items under one user name, as the host offers them (`foreign_secrets`).
pub(super) struct Generic {
    account: String,
    // A build for tests addresses only its own scratch slots (AGENTS.md, Credential Manager).
    #[cfg(any(test, feature = "test-support"))]
    own: Vec<String>,
    #[cfg(any(test, feature = "test-support"))]
    ctx: Context,
}

impl Generic {
    #[cfg_attr(not(any(test, feature = "test-support")), allow(unused_variables))]
    pub(super) fn of(ctx: &Context, account: &str) -> Generic {
        Generic {
            account: account.to_owned(),
            #[cfg(any(test, feature = "test-support"))]
            own: crate::provider::claude::credman::own_slots_for_tests(ctx),
            #[cfg(any(test, feature = "test-support"))]
            ctx: ctx.clone(),
        }
    }

    fn target(&self, service: &str) -> Result<String, Error> {
        let target = format!("{service}/{}", self.account);
        #[cfg(any(test, feature = "test-support"))]
        {
            if let Some(why) = crate::provider::names::refusal(&target, &self.own) {
                let not_own = crate::provider::claude::credman::not_own(&self.ctx);
                return Err(Error::Unreadable(format!("{why}; {not_own}")));
            }
        }
        Ok(target)
    }
}

impl RawStore for Generic {
    fn kind(&self) -> Backend {
        Backend::CredentialManager
    }

    fn contains(&self, service: &str) -> Result<bool, Error> {
        let target = self.target(service)?;
        held(&target).map_err(|e| failed(&e, |why| Error::Unreadable(format!("`{target}`: {why}"))))
    }

    fn read(&self, service: &str) -> Result<Option<String>, Error> {
        let target = self.target(service)?;
        let found = item(&target)
            .map_err(|e| failed(&e, |why| Error::Unreadable(format!("`{target}`: {why}"))))?;
        found
            .map(|held| {
                String::from_utf8(held.blob)
                    .map_err(|_| Error::Unreadable(format!("`{target}` holds no UTF-8 text")))
            })
            .transpose()
    }

    fn write(&self, _: Permit, service: &str, contents: &str) -> Result<(), Error> {
        let target = self.target(service)?;
        if contents.len() > BLOB {
            return Err(Error::Write(format!(
                "`{target}` would hold {} bytes, past the {BLOB} a Credential Manager item holds",
                contents.len()
            )));
        }
        write(&target, &self.account, contents.as_bytes())
            .map_err(|e| failed(&e, |why| Error::Write(format!("`{target}`: {why}"))))?;
        match self.read(service)? {
            Some(back) if back == contents => Ok(()),
            Some(_) => Err(Error::NotDurable(format!(
                "`{target}` holds different bytes"
            ))),
            None => Err(Error::NotDurable(format!("`{target}` reads back empty"))),
        }
    }

    fn delete(&self, _: Permit, service: &str) -> Result<(), Error> {
        let target = self.target(service)?;
        delete(&target)
            .map(drop)
            .map_err(|e| failed(&e, |why| Error::Write(format!("`{target}`: {why}"))))
    }

    fn cost(&self, _service: &str, contents: &str) -> Option<Cost> {
        Some(Cost {
            needs: contents.len(),
            limit: BLOB,
            second_route: false,
            store: SecretStore::CredentialManager,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Never a family's prefix: the account running the tests may hold real logins.
    #[test]
    fn a_prefix_nothing_has_lists_nothing() {
        assert_eq!(
            names("pitboard-citest-w21-nothing-*").expect("listed"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn an_item_nothing_has_is_not_held() {
        assert!(!held("pitboard-citest-w22-nothing/claude-code-user").expect("asked"));
    }

    // CRED_MAX_CREDENTIAL_BLOB_SIZE, which the store's own check stops at first.
    #[test]
    fn an_item_holds_2560_bytes_and_no_more() {
        let target = format!(
            "pitboard-citest-w23a-blob-{}/claude-code-user",
            std::process::id()
        );
        let _ = delete(&target);
        let fits = write(&target, "claude-code-user", &[b'x'; BLOB]);
        let past = write(&target, "claude-code-user", &[b'x'; BLOB + 1]);
        let held = item(&target).map(|found| found.map(|held| held.blob.len()));
        let _ = delete(&target);
        fits.expect("2560 bytes fit");
        assert!(past.is_err(), "2561 bytes do not");
        assert_eq!(held.expect("read"), Some(BLOB));
    }

    // RF-G3's values (run 38704825885), the counterpart of `credman_write_is_harmless`.
    #[test]
    fn an_item_is_written_as_bun_writes_one_and_read_back() {
        let target = format!(
            "pitboard-citest-w23a-{}/claude-code-user",
            std::process::id()
        );
        let _ = delete(&target);
        write(&target, "claude-code-user", "{\"Đạt\":1}".as_bytes()).expect("written");
        let found = item(&target).expect("read").expect("there");
        let gone = delete(&target).expect("deleted");
        assert!(gone);
        assert_eq!(found.kind, CRED_TYPE_GENERIC);
        assert_eq!(found.persist, CRED_PERSIST_ENTERPRISE);
        assert_eq!(found.user, "claude-code-user");
        assert_eq!(found.comment, None);
        assert_eq!(found.attributes, 0);
        assert_eq!(found.blob, "{\"Đạt\":1}".as_bytes());
        assert!(!delete(&target).expect("asked"), "deleted once");
        assert!(!held(&target).expect("asked"));
    }
}
