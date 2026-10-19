//! What an administrator set on this machine for a program, outside every person's home: a
//! file only an administrator writes, such as Codex's `/etc/codex/requirements.toml`, or a
//! managed preference a configuration profile forces on macOS.
//!
//! A tool reads these as well as the person's own settings, and an administrator's beat the
//! person's. So where a tool keeps its login is not always what the person's own file says,
//! and Pitboard reads what the tool reads.
//!
//! The real hosts read none of it in a build for tests ([`READ_BY_REAL_HOSTS`]): what an
//! administrator set on the machine running the tests is no test's business, and a test that
//! needs a setting says it through [`super::memory::MemoryHost`].

use std::path::Path;

/// What one place an administrator sets things in holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Administered {
    /// Nothing is set there.
    Unset,
    /// This, as text.
    Set(String),
    /// Something is there that could not be read, and why, in words that follow its name.
    Unreadable(String),
}

// `HKLM`, an administrator's, or `HKCU`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Hive {
    Machine,
    User,
}

impl Hive {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Hive::Machine => "HKLM",
            Hive::User => "HKCU",
        }
    }
}

/// Whether the real hosts read what an administrator set. Not in a build for tests: a unit
/// test, the command line the integration tests run, or an app built with the fixtures.
pub(crate) const READ_BY_REAL_HOSTS: bool = !cfg!(any(test, feature = "test-support"));

pub(crate) fn listed(dir: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect()
}

/// The file at `path`, as text. Absent is unset; there and unreadable, or not text, says why.
pub(crate) fn read_text(path: &Path) -> Administered {
    std::fs::read(path).map_or_else(|e| unread(&e), as_text)
}

// As Claude Code reads its remote token files, which any account may put at a Windows drive's
// root: a regular file alone, of at most `limit` bytes.
pub(crate) fn read_up_to(path: &Path, limit: u64) -> Administered {
    use std::io::Read;
    // Asked before it is opened too, since opening a FIFO waits for a writer.
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return not_a_file(),
        Err(e) => return unread(&e),
    }
    let file = match std::fs::File::open(path) {
        Ok(file) if file.metadata().is_ok_and(|meta| meta.is_file()) => file,
        Ok(_) => return not_a_file(),
        Err(e) => return unread(&e),
    };
    let mut bytes = Vec::new();
    match file.take(limit + 1).read_to_end(&mut bytes) {
        Ok(_) if bytes.len() as u64 > limit => larger_than(limit),
        Ok(_) => as_text(bytes),
        Err(e) => unread(&e),
    }
}

pub(crate) fn larger_than(limit: u64) -> Administered {
    Administered::Unreadable(format!("is larger than {limit} bytes"))
}

fn not_a_file() -> Administered {
    Administered::Unreadable("is not a file".into())
}

fn as_text(bytes: Vec<u8>) -> Administered {
    match String::from_utf8(bytes) {
        Ok(text) => Administered::Set(text),
        Err(_) => Administered::Unreadable("is not UTF-8 text".into()),
    }
}

fn unread(e: &std::io::Error) -> Administered {
    if e.kind() == std::io::ErrorKind::NotFound {
        Administered::Unset
    } else {
        Administered::Unreadable(format!("cannot be read: {e}"))
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-administered-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        dir
    }

    /// A file that is not there sets nothing. One that is there is read whole, and one that
    /// cannot be read, or is not text, says so rather than reading as unset: a tool that
    /// cannot read it does not start, which is not the same as nothing being set.
    #[test]
    fn a_file_is_unset_set_or_unreadable() {
        let dir = scratch("file");
        assert_eq!(read_text(&dir.join("absent.toml")), Administered::Unset);
        std::fs::write(dir.join("set.toml"), "a = 1\n").expect("written");
        assert_eq!(
            read_text(&dir.join("set.toml")),
            Administered::Set("a = 1\n".into())
        );
        std::fs::write(dir.join("bytes.toml"), [0xff, 0xfe, 0x00]).expect("written");
        assert_eq!(
            read_text(&dir.join("bytes.toml")),
            Administered::Unreadable("is not UTF-8 text".into())
        );
        let folder = read_text(&dir);
        assert!(
            matches!(&folder, Administered::Unreadable(why) if why.starts_with("cannot be read")),
            "a folder where the file should be cannot be read as one: {folder:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bounded_read_takes_a_file_up_to_its_limit_alone() {
        let dir = scratch("bounded");
        let file = dir.join(".api_key");
        assert_eq!(read_up_to(&file, 8), Administered::Unset);
        std::fs::write(&file, "12345678").expect("written");
        assert_eq!(read_up_to(&file, 8), Administered::Set("12345678".into()));
        std::fs::write(&file, "123456789").expect("written");
        assert_eq!(read_up_to(&file, 8), larger_than(8));
        assert_eq!(read_up_to(&dir, 8), not_a_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No build for tests reads what this machine's administrator set.
    #[test]
    fn a_build_for_tests_reads_nothing_an_administrator_set() {
        const { assert!(!READ_BY_REAL_HOSTS) };
        let ctx = crate::context::Context::for_unit_test();
        assert_eq!(
            ctx.host()
                .administered_file(Path::new("/etc/codex/requirements.toml")),
            Administered::Unset
        );
        assert_eq!(
            ctx.host()
                .managed_preference("com.openai.codex", "requirements_toml_base64"),
            Administered::Unset
        );
        assert_eq!(
            ctx.host()
                .administered_folder(Path::new("/etc/claude-code/managed-settings.d")),
            Vec::<std::path::PathBuf>::new()
        );
        for hive in [Hive::Machine, Hive::User] {
            assert_eq!(
                ctx.host()
                    .policy(hive, r"SOFTWARE\Policies\ClaudeCode", "Settings"),
                Administered::Unset
            );
        }
    }
}
