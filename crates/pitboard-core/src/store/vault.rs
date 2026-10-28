//! One private file per parked login, in Pitboard's own private folder: as it is on Linux,
//! and sealed to the person's account on Windows.

use super::{Backend, Error, RawStore};
use crate::atomic;
use crate::context::Context;
use crate::permit::Permit;
use std::borrow::Cow;
use std::path::PathBuf;

// What a sealed park starts with, then the version of what follows.
const SEALED: &[u8] = b"pitboard-sealed";
const VERSION: u8 = 1;
// Followed by the park's name, so a park renamed onto another's will not open.
const ENTROPY: &[u8] = b"pitboard-vault-1\0";

// DPAPI on Windows.
pub(crate) trait Seal: Sync {
    fn seal(&self, entropy: &[u8], plain: &[u8]) -> std::io::Result<Vec<u8>>;
    fn open(&self, entropy: &[u8], sealed: &[u8]) -> std::io::Result<Vec<u8>>;
}

pub(crate) struct FileVault {
    dir: PathBuf,
    seal: Option<&'static dyn Seal>,
}

impl FileVault {
    #[cfg_attr(windows, allow(dead_code, reason = "Windows' host seals every park"))]
    pub(crate) fn new(ctx: &Context) -> FileVault {
        FileVault {
            dir: super::vault_dir(ctx),
            seal: None,
        }
    }

    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "only Windows' host seals a park")
    )]
    pub(crate) fn sealed(ctx: &Context, seal: &'static dyn Seal) -> FileVault {
        FileVault {
            dir: super::vault_dir(ctx),
            seal: Some(seal),
        }
    }

    // Pitboard's names are used as they are: anything else is refused, never escaped.
    fn path(&self, service: &str) -> Result<PathBuf, Error> {
        if !storable(service) {
            return Err(Error::Write(format!(
                "{service} is not a name this vault will store"
            )));
        }
        Ok(self.dir.join(format!("{service}.json")))
    }
}

fn entropy(service: &str) -> Vec<u8> {
    [ENTROPY, service.as_bytes()].concat()
}

// Why a sealed park would not open: its shape is wrong, or the seal itself did not open, which
// carries `CryptUnprotectData`'s code so doctor can tell a logon without the key from a shape
// Pitboard planted or a stranger wrote.
enum Opened {
    Shape(String),
    Sealed { code: Option<i32>, detail: String },
}

// Whatever stops it opening, a park is there and unreadable, never absent.
fn opened(seal: &dyn Seal, service: &str, sealed: &[u8]) -> Result<String, Opened> {
    let Some((&version, blob)) = sealed.strip_prefix(SEALED).and_then(<[u8]>::split_first) else {
        return Err(Opened::Shape("it is not a park Pitboard sealed".into()));
    };
    if version != VERSION {
        return Err(Opened::Shape(format!(
            "it is sealed in form {version}, which this Pitboard does not read"
        )));
    }
    let plain = seal
        .open(&entropy(service), blob)
        .map_err(|e| Opened::Sealed {
            code: e.raw_os_error(),
            detail: format!("it does not open for this account: {e}"),
        })?;
    String::from_utf8(plain).map_err(|_| Opened::Shape("what it holds is not text".into()))
}

// Windows opens a device for CON, NUL.json or COM1.x, and cuts trailing dots and spaces.
fn storable(service: &str) -> bool {
    let stem = service.split('.').next().unwrap_or_default().as_bytes();
    let device = [b"CON", b"PRN", b"AUX", b"NUL"]
        .iter()
        .any(|name| stem.eq_ignore_ascii_case(*name))
        || (stem.len() == 4
            && [b"COM", b"LPT"]
                .iter()
                .any(|port| stem[..3].eq_ignore_ascii_case(*port))
            && stem[3].is_ascii_digit());
    !service.is_empty()
        && service
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !service.ends_with('.')
        && !device
}

impl RawStore for FileVault {
    fn kind(&self) -> Backend {
        match self.seal {
            None => Backend::File,
            Some(_) => Backend::SealedFile,
        }
    }

    fn contains(&self, service: &str) -> Result<bool, Error> {
        super::exists(&self.path(service)?)
    }

    /// A directory enumerates itself. A vault that is not there yet holds nothing, which is
    /// an answer; anything else that stops the read is not, and says so.
    fn list(&self) -> Result<Option<Vec<String>>, Error> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Some(Vec::new())),
            Err(e) => {
                return Err(Error::Unreadable(format!(
                    "cannot list {}: {e}",
                    self.dir.display()
                )));
            }
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let service = name.strip_suffix(".json")?.to_string();
                crate::park::is_park_name(&service).then_some(service)
            })
            .collect();
        names.sort();
        Ok(Some(names))
    }

    fn read(&self, service: &str) -> Result<Option<String>, Error> {
        let path = self.path(service)?;
        let read = match self.seal {
            None => std::fs::read_to_string(&path).map(Ok),
            Some(seal) => std::fs::read(&path).map(|sealed| opened(seal, service, &sealed)),
        };
        let unreadable =
            |why: String| Error::Unreadable(format!("cannot read {}: {why}", path.display()));
        match read {
            Ok(Ok(s)) => Ok(Some(s)),
            Ok(Err(Opened::Shape(why))) => Err(unreadable(why)),
            Ok(Err(Opened::Sealed { code, detail })) => Err(Error::Sealed { code, detail }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(unreadable(e.to_string())),
        }
    }

    fn write(&self, permit: Permit, service: &str, contents: &str) -> Result<(), Error> {
        let path = self.path(service)?;
        let bytes = match self.seal {
            None => Cow::Borrowed(contents.as_bytes()),
            Some(seal) => {
                let blob = seal
                    .seal(&entropy(service), contents.as_bytes())
                    .map_err(|e| Error::Write(format!("cannot seal {}: {e}", path.display())))?;
                Cow::Owned([SEALED, &[VERSION], &blob].concat())
            }
        };
        crate::host::fs::create_private_dir(permit, &self.dir)
            .map_err(|e| Error::Write(e.to_string()))?;
        atomic::write(permit, &path, &bytes, atomic::Perms::Secret)
            .map_err(|e| Error::Write(format!("cannot write {}: {e}", path.display())))?;
        match self.read(service)? {
            Some(back) if back == contents => Ok(()),
            _ => Err(Error::NotDurable(format!(
                "{} does not hold what was written",
                path.display()
            ))),
        }
    }

    fn delete(&self, permit: Permit, service: &str) -> Result<(), Error> {
        let path = self.path(service)?;
        match crate::host::fs::remove_file(permit, &path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Write(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARK: &str = "pitboard-park-1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0-1789935600123";
    const LOGIN: &str = r#"{"claudeAiOauth":{"refreshToken":"r"}}"#;

    #[cfg(windows)]
    const SEAL: &dyn Seal = &crate::host::Dpapi;
    #[cfg(not(windows))]
    const SEAL: &dyn Seal = &StandIn;

    // Off Windows: a digest of the entropy and the login, then the login turned, which a
    // rename or a flipped byte breaks as it breaks DPAPI.
    #[cfg(not(windows))]
    struct StandIn;

    #[cfg(not(windows))]
    impl StandIn {
        fn digest(entropy: &[u8], plain: &[u8]) -> Vec<u8> {
            use sha2::{Digest, Sha256};
            Sha256::new()
                .chain_update(entropy)
                .chain_update(plain)
                .finalize()
                .to_vec()
        }
    }

    #[cfg(not(windows))]
    impl Seal for StandIn {
        fn seal(&self, entropy: &[u8], plain: &[u8]) -> std::io::Result<Vec<u8>> {
            let turned = plain.iter().map(|b| b ^ 0x5a);
            Ok(StandIn::digest(entropy, plain)
                .into_iter()
                .chain(turned)
                .collect())
        }

        fn open(&self, entropy: &[u8], sealed: &[u8]) -> std::io::Result<Vec<u8>> {
            let bad = || std::io::Error::from(std::io::ErrorKind::InvalidData);
            let (digest, turned) = sealed.split_at_checked(32).ok_or_else(bad)?;
            let plain: Vec<u8> = turned.iter().map(|b| b ^ 0x5a).collect();
            (StandIn::digest(entropy, &plain) == digest)
                .then_some(plain)
                .ok_or_else(bad)
        }
    }

    fn sealed_vault() -> (FileVault, tempfile::TempDir) {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let ctx = Context::new(scratch.path().to_path_buf())
            .with_pitboard_home(scratch.path().join("pitboard"));
        (FileVault::sealed(&ctx, SEAL), scratch)
    }

    // Never absent and never a login, and still there to a look: a shape Pitboard did not
    // seal reads Unreadable, a seal that will not open reads Sealed.
    fn unopened(vault: &FileVault, service: &str) {
        match vault.read(service) {
            Err(Error::Unreadable(why)) => assert!(why.contains(service), "{why}"),
            Err(Error::Sealed { .. }) => {}
            other => panic!("{service} read as {other:?}"),
        }
        assert!(vault.contains(service).expect("a look"), "{service}");
    }

    #[test]
    fn a_sealed_park_round_trips_and_holds_no_plain_text() {
        let (vault, _scratch) = sealed_vault();
        assert_eq!(vault.kind(), Backend::SealedFile);
        assert_eq!(vault.read(PARK).expect("nothing yet"), None);
        vault
            .write(Permit::for_a_test(), PARK, LOGIN)
            .expect("sealed");
        assert_eq!(vault.read(PARK).expect("opened").as_deref(), Some(LOGIN));
        let held = std::fs::read(vault.path(PARK).unwrap()).expect("the file");
        assert!(held.starts_with(SEALED), "{held:?}");
        assert!(
            !held.windows(5).any(|bytes| bytes == b"oauth"),
            "the login is not in the file"
        );
    }

    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test damages a park as a disk or a person would"
    )]
    fn a_renamed_park_or_one_flipped_byte_does_not_open() {
        let (vault, _scratch) = sealed_vault();
        let permit = Permit::for_a_test();
        let other = "pitboard-park-0a1b2c3d-4b5a-4968-8776-a5b4c3d2e1f0-1789935600456";
        vault.write(permit, PARK, LOGIN).expect("sealed");
        std::fs::rename(vault.path(PARK).unwrap(), vault.path(other).unwrap()).expect("renamed");
        unopened(&vault, other);

        vault.write(permit, PARK, LOGIN).expect("sealed");
        let path = vault.path(PARK).unwrap();
        let sealed = std::fs::read(&path).expect("the file");
        for at in [
            0,
            SEALED.len(),
            SEALED.len() + 1,
            sealed.len() / 2,
            sealed.len() - 1,
        ] {
            let mut flipped = sealed.clone();
            flipped[at] ^= 0x01;
            std::fs::write(&path, &flipped).expect("damaged");
            unopened(&vault, PARK);
        }
    }

    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test plants a park as a person used to Linux would"
    )]
    fn plain_text_planted_in_the_vault_is_never_a_login() {
        let (vault, _scratch) = sealed_vault();
        crate::host::fs::create_private_dir(Permit::for_a_test(), &vault.dir).expect("a vault");
        std::fs::write(vault.path(PARK).unwrap(), LOGIN).expect("planted");
        unopened(&vault, PARK);

        let later = [SEALED, &[VERSION + 1], LOGIN.as_bytes()].concat();
        std::fs::write(vault.path(PARK).unwrap(), later).expect("planted");
        unopened(&vault, PARK);
    }

    // As a profile restored on another PC, whose key this account does not have.
    #[test]
    fn a_seal_whose_key_is_not_found_is_unreadable_not_absent() {
        struct Lost;
        impl Seal for Lost {
            fn seal(&self, _: &[u8], plain: &[u8]) -> std::io::Result<Vec<u8>> {
                Ok(plain.to_vec())
            }
            fn open(&self, _: &[u8], _: &[u8]) -> std::io::Result<Vec<u8>> {
                Err(std::io::ErrorKind::NotFound.into())
            }
        }
        let scratch = tempfile::tempdir().expect("a scratch home");
        let ctx = Context::new(scratch.path().to_path_buf())
            .with_pitboard_home(scratch.path().join("pitboard"));
        let vault = FileVault::sealed(&ctx, &Lost);
        match vault.write(Permit::for_a_test(), PARK, LOGIN) {
            Err(Error::Sealed { .. }) => {}
            other => panic!("a write that does not read back: {other:?}"),
        }
        unopened(&vault, PARK);
    }

    #[test]
    fn the_list_is_right_after_writes() {
        let (vault, _scratch) = sealed_vault();
        let permit = Permit::for_a_test();
        assert_eq!(vault.list().expect("listed"), Some(Vec::new()));
        let names: Vec<String> = (0..3)
            .map(|at| crate::park::service_name("acc", 1_789_935_600_000 + at))
            .collect();
        for name in names.iter().rev() {
            vault.write(permit, name, LOGIN).expect("sealed");
        }
        assert_eq!(vault.list().expect("listed"), Some(names.clone()));
        vault.delete(permit, &names[1]).expect("deleted");
        assert_eq!(
            vault.list().expect("listed"),
            Some(vec![names[0].clone(), names[2].clone()])
        );
    }

    /// A real Codex account id is the ChatGPT account and the person inside it, and a park
    /// of one has to be a name this vault stores. Joined with a colon once, every Codex
    /// park on Linux was refused, after a browser sign-in had already been finished.
    #[test]
    fn a_real_codex_park_name_is_one_the_vault_stores() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path().to_path_buf();
        let ctx = Context::new(root.clone()).with_pitboard_home(root.clone());
        let vault = FileVault::new(&ctx);
        let name = crate::park::service_name(
            "8c3f0f86-0a7c-4d52-9b0e-1f2a3b4c5d6e_user-AbC123dEf456",
            1_790_000_000_000,
        );
        assert!(vault.path(&name).is_ok(), "{name}");
        assert!(crate::park::is_park_name(&name));
    }

    // SECURITY.md says a park and its folder are private to their owner, sealed or not.
    #[test]
    fn a_parked_login_is_readable_only_by_its_owner() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path();

        let ctx = Context::new(root.to_path_buf()).with_pitboard_home(root.join(".pitboard"));
        for vault in [FileVault::new(&ctx), FileVault::sealed(&ctx, SEAL)] {
            vault
                .write(Permit::for_a_test(), PARK, r#"{"claudeAiOauth":{}}"#)
                .expect("a park");

            let shared =
                |p: &std::path::Path| crate::host::fs::access(p).expect("it exists").shared;
            assert!(!shared(&vault.dir), "the vault directory");
            assert!(!shared(&vault.path(PARK).unwrap()), "the parked login");
        }
    }

    #[test]
    fn only_names_pitboard_generates_are_accepted() {
        let vault = FileVault::new(&Context::for_unit_test());
        for good in [
            "pitboard-park-1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0-1789935600123",
            "a.b_c-1",
            "CONSOLE",
            "nul-1",
            "COM10",
            "LPT",
            "a.con",
        ] {
            assert!(vault.path(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "../escape",
            "has space",
            "a/b",
            "sl\\ash",
            "CON",
            "nul",
            "Aux.x",
            "prn.tar.gz",
            "COM1",
            "com0",
            "LPT9.json",
            "trailing.",
            "trailing ",
            "abÉ",
        ] {
            assert!(
                vault.path(bad).is_err(),
                "{bad:?} must be refused, not escaped"
            );
        }
    }
}
