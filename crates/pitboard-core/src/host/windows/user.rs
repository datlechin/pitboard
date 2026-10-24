use crate::host::token::{self, Token};
use crate::host::{Elevation, Floor};
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, IsWellKnownSid, PSID, TOKEN_ELEVATION, TOKEN_ELEVATION_TYPE, TOKEN_QUERY,
    TOKEN_USER, TokenElevation, TokenElevationType, TokenUser, WinLocalServiceSid,
    WinLocalSystemSid, WinNetworkServiceSid,
};
use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

// Windows needs none: Claude Code's Windows build names its item `claude-code-user` whoever
// runs it (`credman_account_name`).
pub(crate) fn login_name() -> Option<String> {
    None
}

// Where libuv's `uv_os_homedir` looks once `USERPROFILE` is unset.
pub(crate) fn home() -> Option<PathBuf> {
    real_lookups().then(profile_directory).flatten()
}

// The token's TokenUser, which owns and is granted what Pitboard makes, and a report hides.
pub(crate) fn sid() -> Option<String> {
    Opened::this_process()?.user_sid()
}

// The user another process runs as, where its token may be read.
pub(crate) fn token_user(process: HANDLE) -> Option<String> {
    let mut handle: HANDLE = std::ptr::null_mut();
    // SAFETY: an open process handle; `handle` is written only on success.
    let opened = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut handle) };
    (opened != 0).then(|| Opened(handle).user_sid()).flatten()
}

// `DOMAIN\name`'s SID, as WTS names the user of a session.
pub(crate) fn account_sid(account: &str) -> Option<String> {
    use windows_sys::Win32::Security::{LookupAccountNameW, SID_NAME_USE};
    let name: Vec<u16> = account.encode_utf16().chain([0]).collect();
    let (mut sid_length, mut domain_length) = (0u32, 0u32);
    let mut kind: SID_NAME_USE = 0;
    // SAFETY: with no buffers it writes only the lengths it needs.
    unsafe {
        LookupAccountNameW(
            std::ptr::null(),
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut sid_length,
            std::ptr::null_mut(),
            &mut domain_length,
            &mut kind,
        );
    }
    if sid_length == 0 {
        return None;
    }
    // Whole u64s, so the SID at its start is aligned.
    let mut sid = vec![0u64; (sid_length as usize).div_ceil(size_of::<u64>())];
    let mut domain = vec![0u16; domain_length as usize];
    // SAFETY: both buffers hold at least the lengths given.
    let found = unsafe {
        LookupAccountNameW(
            std::ptr::null(),
            name.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_length,
            domain.as_mut_ptr(),
            &mut domain_length,
            &mut kind,
        )
    };
    (found != 0)
        .then(|| sid_text(sid.as_mut_ptr().cast()))
        .flatten()
}

pub(crate) fn sid_text(sid: PSID) -> Option<String> {
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    let mut text: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: a valid SID, and `text` is LocalAlloc'd on success and freed below.
    if unsafe { ConvertSidToStringSidW(sid, &raw mut text) } == 0 || text.is_null() {
        return None;
    }
    let mut len = 0;
    // SAFETY: on success `text` is a NUL-terminated string, read up to its NUL.
    while unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `text` holds `len` units before its NUL.
    let said = String::from_utf16(unsafe { std::slice::from_raw_parts(text, len) }).ok();
    // SAFETY: the LocalAlloc'd string ConvertSidToStringSidW gave, freed this once.
    unsafe { windows_sys::Win32::Foundation::LocalFree(text.cast()) };
    said
}

// `FOLDERID_Profile` ignores `USERPROFILE` and `HOME` (docs/facts/windows-runners.md). Codex's
// default home is in it.
pub(crate) fn profile() -> Option<PathBuf> {
    real_lookups()
        .then(|| known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile))
        .flatten()
}

pub(crate) fn local_app_data() -> Option<PathBuf> {
    real_lookups()
        .then(|| known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData))
        .flatten()
}

// The machine's, not the account's, so a build for tests reads it too.
pub(crate) fn program_data() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_ProgramData)
}

// The runners' profiles are local; `PT_ROAMING` is documented, not measured
// (docs/facts/windows-documented.md).
pub(crate) fn profile_roams() -> bool {
    use windows_sys::Win32::System::GroupPolicy::PT_ROAMING;
    use windows_sys::Win32::UI::Shell::GetProfileType;
    let mut flags = 0u32;
    // SAFETY: a u32 of this frame, which the call writes and does not keep.
    let read = real_lookups() && unsafe { GetProfileType(&raw mut flags) } != 0;
    read && flags & PT_ROAMING != 0
}

#[cfg(test)]
fn real_lookups() -> bool {
    testing::reaches_the_real_home()
}

#[cfg(not(test))]
fn real_lookups() -> bool {
    !cfg!(feature = "test-support")
}

fn profile_directory() -> Option<PathBuf> {
    use windows_sys::Win32::UI::Shell::GetUserProfileDirectoryW;
    let token = Opened::this_process()?;
    let mut size = 0u32;
    // SAFETY: with no buffer it writes only the length it needs, NUL included, to `size`.
    unsafe { GetUserProfileDirectoryW(token.0, std::ptr::null_mut(), &raw mut size) };
    let mut wide = vec![0u16; size as usize];
    // SAFETY: an open token, and a buffer of the `size` units it says.
    let read = unsafe { GetUserProfileDirectoryW(token.0, wide.as_mut_ptr(), &raw mut size) };
    (read != 0).then(|| path_of(&wide))
}

fn path_of(wide: &[u16]) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    let len = wide
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(wide.len());
    PathBuf::from(std::ffi::OsString::from_wide(&wide[..len]))
}

// Resolved first, so a spelling with `\\?\` or a short name is the home too.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_the_accounts_own_home(path: &std::path::Path) -> bool {
    let resolved = |p: &std::path::Path| {
        crate::host::without_verbatim_prefix(
            std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()),
        )
    };
    [
        known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile),
        profile_directory(),
    ]
    .into_iter()
    .flatten()
    .any(|own| crate::host::same_path_in_any_case(&resolved(&own), &resolved(path)))
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn lies_in_the_accounts_own(path: &std::path::Path, below: &str) -> bool {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile)
        .is_some_and(|home| crate::host::lies_in(path, &home.join(below)))
}

// The account's own, and with `as_started` the known folders and the ones this process was
// started with, which can be others: a test's child is started with them inside its own folder.
#[cfg(any(test, feature = "test-support"))]
#[allow(
    clippy::disallowed_methods,
    reason = "the folders this process was started with, which a test's guard refuses too"
)]
pub(crate) fn app_data_folders(as_started: bool) -> (Vec<PathBuf>, Vec<PathBuf>) {
    use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, FOLDERID_RoamingAppData};
    let started = |name: &str| {
        std::env::var_os(name)
            .filter(|_| as_started)
            .map(PathBuf::from)
    };
    let known = |id| as_started.then(|| known_folder(id)).flatten();
    let roaming = [
        accounts_own_app_data(ROAMING),
        known(&FOLDERID_RoamingAppData),
        started("APPDATA"),
    ];
    let local = [
        accounts_own_app_data(LOCAL),
        known(&FOLDERID_LocalAppData),
        started("LOCALAPPDATA"),
    ];
    (
        roaming.into_iter().flatten().collect(),
        local.into_iter().flatten().collect(),
    )
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn accounts_own_home() -> Option<PathBuf> {
    profile_directory().or_else(|| known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile))
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn accounts_own_pitboard_home() -> Option<PathBuf> {
    accounts_own_app_data(LOCAL).map(|data| data.join("Pitboard"))
}

// Each shell folder's value, and where the profile keeps it unless that names another.
#[cfg(any(test, feature = "test-support"))]
const ROAMING: (&str, &str) = ("AppData", r"AppData\Roaming");
#[cfg(any(test, feature = "test-support"))]
const LOCAL: (&str, &str) = ("Local AppData", r"AppData\Local");

// As the account's own environment expands its shell folder, which no variable this process was
// started with moves.
#[cfg(any(test, feature = "test-support"))]
fn accounts_own_app_data((name, in_profile): (&str, &str)) -> Option<PathBuf> {
    let named = || {
        let key = windows_registry::CURRENT_USER
            .open(r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders")
            .ok()?;
        expanded_for_this_account(&key.get_hstring(name).ok()?.to_string_lossy())
    };
    named().or_else(|| accounts_own_home().map(|home| home.join(in_profile)))
}

#[cfg(any(test, feature = "test-support"))]
fn expanded_for_this_account(text: &str) -> Option<PathBuf> {
    use windows_sys::Win32::Security::{TOKEN_DUPLICATE, TOKEN_IMPERSONATE};
    use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsForUserW;
    let token = Opened::this_process_with(TOKEN_QUERY | TOKEN_IMPERSONATE | TOKEN_DUPLICATE)?;
    let source: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let mut out = vec![0u16; 32_768];
    // SAFETY: an open token with the access the call needs, a NUL-terminated source, and a
    // buffer of the length given.
    let done = unsafe {
        ExpandEnvironmentStringsForUserW(
            token.0,
            source.as_ptr(),
            out.as_mut_ptr(),
            out.len() as u32,
        )
    };
    let path = (done != 0).then(|| path_of(&out))?;
    (path.is_absolute() && !path.to_string_lossy().contains('%')).then_some(path)
}

// KF_FLAG_DONT_VERIFY: a folder not made yet is still where a login would go.
fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::UI::Shell::{KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath};

    let mut out: windows_sys::core::PWSTR = std::ptr::null_mut();
    let flags = KF_FLAG_DONT_VERIFY as u32;
    // SAFETY: `id` is a GUID, the null token asks for this user, and `out` is freed below.
    let hr = unsafe { SHGetKnownFolderPath(id, flags, std::ptr::null_mut(), &raw mut out) };
    let path = (hr >= 0 && !out.is_null()).then(|| {
        let mut len = 0;
        // SAFETY: on success `out` is a NUL-terminated string, read up to its NUL.
        while unsafe { *out.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: `out` holds `len` units before its NUL.
        let wide = unsafe { std::slice::from_raw_parts(out, len) };
        PathBuf::from(std::ffi::OsString::from_wide(wide))
    });
    // SAFETY: `out` is null or the shell's allocation, and CoTaskMemFree takes either.
    unsafe { windows_sys::Win32::System::Com::CoTaskMemFree(out.cast()) };
    path
}

// `_sudo` is unread: Windows' own `sudo` elevates the token of the program it starts.
pub(crate) fn elevation(_sudo: bool) -> Elevation {
    token::elevation(this_processs_token())
}

fn this_processs_token() -> Token {
    let Some(opened) = Opened::this_process() else {
        return Token::default();
    };
    Token {
        elevated: opened.elevated(),
        elevation_type: opened.elevation_type(),
        service_account: opened.service_account(),
    }
}

struct Opened(HANDLE);

impl Opened {
    fn this_process() -> Option<Opened> {
        Opened::this_process_with(TOKEN_QUERY)
    }

    fn this_process_with(access: u32) -> Option<Opened> {
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: a pseudo-handle needs no closing; `handle` is written only on success.
        let opened = unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut handle) };
        (opened != 0).then_some(Opened(handle))
    }

    fn elevated(&self) -> Option<bool> {
        let mut elevation = TOKEN_ELEVATION::default();
        let mut written = 0u32;
        // SAFETY: an open token, and a TOKEN_ELEVATION buffer of exactly the length given.
        let read = unsafe {
            GetTokenInformation(
                self.0,
                TokenElevation,
                (&raw mut elevation).cast(),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut written,
            )
        };
        (read != 0).then_some(elevation.TokenIsElevated != 0)
    }

    fn elevation_type(&self) -> Option<u32> {
        let mut kind: TOKEN_ELEVATION_TYPE = 0;
        let mut written = 0u32;
        // SAFETY: an open token, and a TOKEN_ELEVATION_TYPE buffer of exactly the length given.
        let read = unsafe {
            GetTokenInformation(
                self.0,
                TokenElevationType,
                (&raw mut kind).cast(),
                size_of::<TOKEN_ELEVATION_TYPE>() as u32,
                &mut written,
            )
        };
        (read != 0).then(|| u32::try_from(kind).unwrap_or(0))
    }

    fn service_account(&self) -> Option<bool> {
        self.with_user(|user| {
            let is = |kind| {
                // SAFETY: `user` is the valid SID Windows just gave, still in its buffer.
                unsafe { IsWellKnownSid(user, kind) != 0 }
            };
            is(WinLocalSystemSid) || is(WinLocalServiceSid) || is(WinNetworkServiceSid)
        })
    }

    fn user_sid(&self) -> Option<String> {
        self.with_user(sid_text).flatten()
    }

    fn with_user<T>(&self, then: impl FnOnce(PSID) -> T) -> Option<T> {
        let mut needed = 0u32;
        // SAFETY: with no buffer it writes only the length it needs, to `needed`.
        unsafe {
            GetTokenInformation(self.0, TokenUser, std::ptr::null_mut(), 0, &mut needed);
        }
        if (needed as usize) < size_of::<TOKEN_USER>() {
            return None;
        }
        // Whole u64s, so the buffer is aligned for the TOKEN_USER at its start.
        let mut buffer = vec![0u64; (needed as usize).div_ceil(size_of::<u64>())];
        let mut written = 0u32;
        // SAFETY: the buffer holds at least `needed` bytes, the length given.
        let read = unsafe {
            GetTokenInformation(
                self.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut written,
            )
        };
        if read == 0 {
            return None;
        }
        // SAFETY: an aligned TOKEN_USER starts the buffer, and its SID points into the buffer.
        let user = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        Some(then(user))
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        // SAFETY: a token handle OpenProcessToken gave, closed this once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(crate) fn floor() -> Floor {
    crate::host::windows_floor(version())
}

// Not GetVersionEx, which answers what the manifest asks for, nor the registry's ProductName.
pub(crate) fn version() -> Option<(u32, u32, u32)> {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..OSVERSIONINFOW::default()
    };
    // SAFETY: an OSVERSIONINFOW whose size field says so, which RtlGetVersion fills in place.
    let status = unsafe { windows_sys::Wdk::System::SystemServices::RtlGetVersion(&raw mut info) };
    (status == 0).then_some((info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber))
}

// The one way a unit test reaches this account's real home.
#[cfg(test)]
pub(crate) mod testing {
    use std::cell::Cell;

    thread_local! {
        static REACHES: Cell<bool> = const { Cell::new(false) };
    }

    // For this thread's test, while what it returns is kept.
    pub(crate) fn reaching_the_real_home() -> Reaching {
        REACHES.with(|reaches| reaches.set(true));
        Reaching(())
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    // RF-B1 and RF-B3, run 38693947329: on a runner's local profile they agree.
    #[test]
    fn the_accounts_folders_are_read_only_where_a_test_reaches_for_them() {
        let unread = (home(), profile(), local_app_data(), profile_roams());
        assert_eq!(
            unread,
            (None, None, None, false),
            "a test that does not reach"
        );
        let _real = testing::reaching_the_real_home();
        let own = home().expect("the profile folder");
        assert!(own.is_absolute(), "{}", own.display());
        let profile = profile().expect("the profile");
        assert!(crate::host::same_path_in_any_case(&own, &profile));
        let local = local_app_data().expect("local app data");
        let measured = profile.join("AppData").join("Local");
        assert!(crate::host::same_path_in_any_case(&local, &measured));
        assert!(!profile_roams());
    }

    #[test]
    fn the_machines_program_data_is_read_in_a_test_too() {
        let data = program_data().expect("FOLDERID_ProgramData");
        assert!(data.is_absolute() && data.is_dir(), "{}", data.display());
        assert_eq!(super::super::host().program_data(), Some(data));
    }

    #[test]
    fn the_accounts_own_home_is_its_profile_however_it_is_spelled() {
        let profile =
            known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile).expect("the profile");
        let text = profile.to_string_lossy().into_owned();
        for spelling in [
            text.clone(),
            text.to_uppercase(),
            text.to_lowercase(),
            format!(r"\\?\{text}"),
            format!(r"{text}\"),
        ] {
            assert!(
                is_the_accounts_own_home(std::path::Path::new(&spelling)),
                "{spelling}"
            );
        }
        if let Some(short) = crate::host::fs::short_path(&profile) {
            assert!(is_the_accounts_own_home(&short), "{}", short.display());
        }
        assert!(!is_the_accounts_own_home(&std::env::temp_dir()));
        assert!(!is_the_accounts_own_home(&profile.join("AppData")));
    }

    #[test]
    fn the_profile_and_local_app_data_are_named() {
        use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, FOLDERID_Profile};
        for id in [FOLDERID_Profile, FOLDERID_LocalAppData] {
            let folder = known_folder(&id).expect("a known folder");
            assert!(folder.is_absolute(), "{}", folder.display());
        }
    }

    // `whoami /user` prints the user's name and SID, which is this token's TokenUser.
    #[test]
    fn the_sid_is_the_one_windows_shows_for_this_user() {
        let sid = sid().expect("TokenUser");
        assert!(sid.starts_with("S-1-"), "{sid}");
        let said = std::process::Command::new("whoami.exe")
            .args(["/user", "/fo", "csv", "/nh"])
            .output()
            .expect("whoami runs");
        let said = String::from_utf8_lossy(&said.stdout);
        assert!(said.trim_end().ends_with(&format!("\"{sid}\"")), "{said}");
    }

    #[test]
    fn the_token_and_the_version_are_read() {
        let token = this_processs_token();
        assert!(token.elevated.is_some(), "{token:?}");
        assert!(token.elevation_type.is_some(), "{token:?}");
        assert_eq!(token.service_account, Some(false), "{token:?}");
        assert_ne!(elevation(false), Elevation::Unknown);
        let (major, _, build) = version().expect("RtlGetVersion answers");
        assert_eq!(major, 10);
        assert!(build > 0);
    }

    // `ver` prints `Microsoft Windows [Version 10.0.26100.33438]`.
    #[test]
    fn the_build_is_the_one_windows_shows() {
        let said = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "ver"])
            .output()
            .expect("cmd runs");
        let said = String::from_utf8_lossy(&said.stdout);
        let shown = said
            .split(['[', ']'])
            .nth(1)
            .and_then(|inside| inside.split_whitespace().last())
            .and_then(|numbers| numbers.split('.').nth(2))
            .and_then(|build| build.parse::<u32>().ok())
            .unwrap_or_else(|| panic!("no build in {said:?}"));
        assert_eq!(version().map(|(_, _, build)| build), Some(shown));
    }
}
