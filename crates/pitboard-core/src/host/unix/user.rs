//! The person signed in, the POSIX way.

use crate::host::Elevation;
use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

/// The name in the passwd database for this user id, as Node's `os.userInfo()` reads it.
pub(crate) fn login_name() -> Option<String> {
    entry(|passwd| {
        text(passwd, passwd.pw_name)?
            .to_str()
            .ok()
            .map(str::to_owned)
    })
}

/// This user's home directory, as the passwd database names it: where a home is when the
/// environment names none, as Foundation finds it for an app.
///
/// Never in a build for tests, so a test that gives no home of its own reaches nothing
/// real. A unit test that asks panics, unless it says through
/// [`testing::reaching_the_real_home`] that this lookup is what it tests. Any other build
/// for tests, such as the command line the integration tests run or an app built with the
/// fixtures, is told there is none, and Pitboard refuses the empty home that leaves as a
/// home that is not a full path.
pub(crate) fn home() -> Option<PathBuf> {
    #[cfg(test)]
    assert!(
        testing::reaches_the_real_home(),
        "a unit test asked for this account's real home: give it a home of its own, as \
         Context::for_unit_test does"
    );
    if cfg!(all(feature = "test-support", not(test))) {
        return None;
    }
    passwd_home()
}

/// [`home`], asked of the passwd database whatever the build.
fn passwd_home() -> Option<PathBuf> {
    entry(|passwd| path(passwd, passwd.pw_dir))
}

#[cfg(feature = "test-support")]
pub(crate) fn accounts_own_home() -> Option<PathBuf> {
    passwd_home()
}

#[cfg(feature = "test-support")]
pub(crate) fn accounts_own_pitboard_home() -> Option<PathBuf> {
    passwd_home().map(|home| crate::host::default_pitboard_home(&home))
}

/// Whether `path` is this account's own home, as the passwd database names it, for a build
/// for tests to refuse to act on. Compared as the file system resolves both, so a link to
/// the home is the home.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_the_accounts_own_home(path: &std::path::Path) -> bool {
    let resolved = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
    passwd_home().is_some_and(|own| resolved(&own) == resolved(path))
}

/// The one way a unit test reaches this account's real home.
#[cfg(test)]
pub(crate) mod testing {
    use std::cell::Cell;

    thread_local! {
        static REACHES: Cell<bool> = const { Cell::new(false) };
    }

    /// While what this returns is kept, [`super::home`] answers this thread's test with the
    /// account's real home: for a test of the lookup itself, or of what reads it where the
    /// environment names no home. Nothing else may.
    pub(crate) fn reaching_the_real_home() -> Reaching {
        REACHES.with(|reaches| reaches.set(true));
        Reaching(())
    }

    /// What [`reaching_the_real_home`] returns.
    pub(crate) struct Reaching(());

    impl Drop for Reaching {
        fn drop(&mut self) {
            REACHES.with(|reaches| reaches.set(false));
        }
    }

    pub(super) fn reaches_the_real_home() -> bool {
        REACHES.with(Cell::get)
    }
}

/// This user's login shell, as the passwd database names it: the one to ask when the
/// environment names none.
pub(crate) fn login_shell() -> Option<PathBuf> {
    entry(|passwd| path(passwd, passwd.pw_shell))
}

/// Whether this process runs as the person themselves: not under sudo, which says so in
/// `SUDO_UID` (`sudo`, as the context read it from the environment this process was
/// started with), and not as root, an effective user id of 0. Under sudo first, since
/// `sudo pitboard` is both and "with sudo" says what to stop doing. POSIX always answers
/// both, so this is never unknown.
pub(crate) fn elevation(sudo: bool) -> Elevation {
    // SAFETY: `geteuid` cannot fail and touches no memory of this process.
    let root = unsafe { libc::geteuid() } == 0;
    elevation_of(sudo, root)
}

/// What [`elevation`] answers for each way this process can have been started.
fn elevation_of(sudo: bool, root: bool) -> Elevation {
    match (sudo, root) {
        (true, _) => Elevation::Elevated { why: "with sudo" },
        (false, true) => Elevation::Elevated { why: "as root" },
        (false, false) => Elevation::Normal,
    }
}

/// What `read` takes from this user's passwd entry, while the buffer its strings live in is
/// still there. `None` where there is no entry.
fn entry<T>(read: impl FnOnce(&libc::passwd) -> Option<T>) -> Option<T> {
    grown(
        first_size(),
        |passwd, buffer, found| {
            // SAFETY: getpwuid_r writes into buffers this call owns and reports through
            // `found`, which is null when there is no entry. The strings it points at live in
            // `buffer`, which `grown` keeps until `read` is done with the entry.
            unsafe {
                libc::getpwuid_r(
                    libc::getuid(),
                    passwd,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    found,
                )
            }
        },
        read,
    )
}

/// The size of buffer the system says an entry fits in, or 4096 bytes where it says none.
fn first_size() -> usize {
    // SAFETY: sysconf only reads a limit of the system's.
    let said = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    usize::try_from(said)
        .ok()
        .filter(|&size| size > 0)
        .unwrap_or(4096)
}

/// The largest buffer an entry is looked up with, so a lookup that never fits ends.
const LARGEST: usize = 1 << 20;

/// What `read` takes from the entry `lookup` finds, first with a buffer of `size` bytes and
/// then, each time it answers ERANGE, which says the entry did not fit, with one twice as
/// large, up to [`LARGEST`]. `None` where there is no entry, or the lookup failed.
fn grown<T>(
    mut size: usize,
    mut lookup: impl FnMut(
        &mut libc::passwd,
        &mut [libc::c_char],
        &mut *mut libc::passwd,
    ) -> libc::c_int,
    read: impl FnOnce(&libc::passwd) -> Option<T>,
) -> Option<T> {
    loop {
        let mut buffer: Vec<libc::c_char> = vec![0; size];
        // SAFETY: an all-zero `passwd` is a valid one to hand to getpwuid_r, which fills it.
        let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        match lookup(&mut passwd, &mut buffer, &mut found) {
            // The entry's strings point into `buffer`, which lives until `read` returns.
            0 if !found.is_null() => return read(&passwd),
            libc::ERANGE if size < LARGEST => size = size.saturating_mul(2).min(LARGEST),
            _ => return None,
        }
    }
}

/// One of `entry`'s strings, for as long as `entry` is borrowed, which the elided lifetime
/// ties it to. `None` where it is null.
fn text(entry: &libc::passwd, field: *const libc::c_char) -> Option<&CStr> {
    let _ = entry;
    // SAFETY: a field of an entry getpwuid_r reported is null or points into its buffer,
    // NUL-terminated, and that buffer outlives every borrow of the entry.
    (!field.is_null()).then(|| unsafe { CStr::from_ptr(field) })
}

/// One of `entry`'s strings as a path, whatever bytes it holds. `None` where it is null or
/// empty.
fn path(entry: &libc::passwd, field: *const libc::c_char) -> Option<PathBuf> {
    text(entry, field)
        .filter(|named| !named.is_empty())
        .map(|named| PathBuf::from(OsStr::from_bytes(named.to_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A lookup standing in for getpwuid_r, whose entry has `home` for its home directory and
    /// which answers ERANGE, as getpwuid_r does, while the buffer is too small for it. It
    /// writes down the size of every buffer it is given.
    fn needing<'a>(
        home: &'a str,
        sizes: &'a mut Vec<usize>,
    ) -> impl FnMut(&mut libc::passwd, &mut [libc::c_char], &mut *mut libc::passwd) -> libc::c_int + 'a
    {
        move |passwd, buffer, found| {
            sizes.push(buffer.len());
            let bytes = [home.as_bytes(), b"\0"].concat();
            if buffer.len() < bytes.len() {
                return libc::ERANGE;
            }
            for (to, from) in buffer.iter_mut().zip(&bytes) {
                *to = libc::c_char::from_ne_bytes([*from]);
            }
            passwd.pw_dir = buffer.as_mut_ptr();
            *found = passwd;
            0
        }
    }

    /// An entry too large for the first buffer is looked up again with a larger one, as
    /// getpwuid_r asks by answering ERANGE, rather than taken for no entry: a home kept in a
    /// directory service can be longer than any one size given first.
    #[test]
    fn an_entry_too_large_for_the_first_buffer_is_looked_up_again() {
        let home = format!("/Users/{}", "x".repeat(5000));
        let mut sizes = Vec::new();
        let found = grown(4096, needing(&home, &mut sizes), |passwd| {
            path(passwd, passwd.pw_dir)
        });
        assert!(found.as_deref() == Some(Path::new(&home)), "{sizes:?}");
        assert_eq!(sizes, [4096, 8192]);
    }

    /// A lookup that never fits ends, at a buffer of a mebibyte.
    #[test]
    fn a_lookup_that_never_fits_ends() {
        let home = "x".repeat(LARGEST * 2);
        let mut sizes = Vec::new();
        let found = grown(4096, needing(&home, &mut sizes), |_| Some(()));
        assert_eq!(found, None);
        assert_eq!(sizes.first(), Some(&4096));
        assert_eq!(sizes.last(), Some(&LARGEST));
        assert_eq!(sizes.len(), 9, "{sizes:?}");
    }

    /// Every account the tests run as has a name and a home named from the root.
    #[test]
    fn this_account_has_a_name_and_a_home() {
        assert!(login_name().is_some_and(|name| !name.is_empty()));
        let _real = testing::reaching_the_real_home();
        assert!(
            home().is_some_and(|home| home.is_absolute()),
            "{:?}",
            home()
        );
        assert_eq!(home(), passwd_home());
        assert!(is_the_accounts_own_home(&home().expect("a home")));
        assert!(!is_the_accounts_own_home(&std::env::temp_dir()));
    }

    /// A unit test that forgets to give a home of its own is stopped where it would have
    /// read the account's real one, rather than reading it.
    #[test]
    #[should_panic(expected = "a unit test asked for this account's real home")]
    fn a_unit_test_never_reaches_the_real_home() {
        let _ = home();
    }

    /// Reaching it is said for one test's thread and as long as it is kept, and no longer.
    #[test]
    fn reaching_the_real_home_is_said_for_one_thread_while_it_is_kept() {
        {
            let _real = testing::reaching_the_real_home();
            assert!(testing::reaches_the_real_home());
            let other = std::thread::spawn(testing::reaches_the_real_home);
            assert!(!other.join().expect("it answers"), "another thread");
        }
        assert!(!testing::reaches_the_real_home(), "once it is dropped");
    }

    /// Under sudo is said before root, since `sudo pitboard` is both and the way out is to
    /// leave sudo off; root alone is said as root; and only neither is the person.
    #[test]
    fn sudo_is_said_before_root_and_only_neither_is_the_person() {
        assert_eq!(
            elevation_of(true, true),
            Elevation::Elevated { why: "with sudo" }
        );
        assert_eq!(
            elevation_of(true, false),
            Elevation::Elevated { why: "with sudo" },
            "sudo -u someone else"
        );
        assert_eq!(
            elevation_of(false, true),
            Elevation::Elevated { why: "as root" }
        );
        assert_eq!(elevation_of(false, false), Elevation::Normal);
    }

    /// Root is read from the effective user id, which is checked here against what the
    /// system says another way: the owner of a file this process makes, which POSIX sets to
    /// the effective user id. Run as root, as in a container whose only user is root, this
    /// tells an `elevation` that reads the id from one that ignores it. The tests run as a
    /// person, never as root, and there both say the person, so this cannot catch a read
    /// that is missing; `sudo_is_said_before_root_and_only_neither_is_the_person` checks
    /// what each answer says.
    #[test]
    fn root_is_read_from_the_effective_user_id() {
        use std::os::unix::fs::MetadataExt;
        let made = std::env::temp_dir().join(format!("pitboard-euid-{}", std::process::id()));
        std::fs::write(&made, b"").expect("a file of this process's own");
        let owner = std::fs::metadata(&made).map(|made| made.uid());
        let _ = std::fs::remove_file(&made);
        let root = owner.expect("the file this process made") == 0;
        assert_eq!(elevation(false), elevation_of(false, root));
        assert_eq!(elevation(true), Elevation::Elevated { why: "with sudo" });
    }

    /// And a shell named from the root, which is what is asked for `PATH` when the
    /// environment names none.
    #[test]
    fn this_account_has_a_login_shell() {
        assert!(
            login_shell().is_some_and(|shell| shell.is_absolute()),
            "{:?}",
            login_shell()
        );
    }
}
