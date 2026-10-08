use crate::host::token::{self, Token};
use crate::host::{Elevation, Floor};
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, IsWellKnownSid, TOKEN_ELEVATION, TOKEN_ELEVATION_TYPE, TOKEN_QUERY,
    TOKEN_USER, TokenElevation, TokenElevationType, TokenUser, WinLocalServiceSid,
    WinLocalSystemSid, WinNetworkServiceSid,
};
use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The person's account name, as Claude Code reads it where `USER` is unset: not read until
/// W22, which names Claude Code's store on Windows by it.
pub(crate) fn login_name() -> Option<String> {
    None
}

/// This account's own home, where the environment names none: not read until W14 asks
/// Windows for the profile folder. Without one, Pitboard refuses the empty home as one that
/// is not a full path.
pub(crate) fn home() -> Option<PathBuf> {
    None
}

/// Whether `path` is this account's own home, for a build for tests to refuse to run as the
/// daily renewal schedule there. Nobody can tell until W14 reads the profile folder, and
/// nothing needs to: such a run renews Pitboard's default home, which on Windows is none
/// until W14 finds it too, and is refused as a home that is not a full path
/// ([`crate::host::default_pitboard_home`]).
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_the_accounts_own_home(_path: &std::path::Path) -> bool {
    false
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
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: a pseudo-handle needs no closing; `handle` is written only on success.
        let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) };
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
        let is = |kind| {
            // SAFETY: `user` is the valid SID Windows just gave, still in its buffer.
            unsafe { IsWellKnownSid(user, kind) != 0 }
        };
        Some(is(WinLocalSystemSid) || is(WinLocalServiceSid) || is(WinNetworkServiceSid))
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
fn version() -> Option<(u32, u32, u32)> {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..OSVERSIONINFOW::default()
    };
    // SAFETY: an OSVERSIONINFOW whose size field says so, which RtlGetVersion fills in place.
    let status = unsafe { windows_sys::Wdk::System::SystemServices::RtlGetVersion(&raw mut info) };
    (status == 0).then_some((info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber))
}

/// The one way a unit test reaches this account's real home, which on Windows reaches
/// nothing until W14.
#[cfg(test)]
pub(crate) mod testing {
    /// What a unit test keeps while it reaches the real home.
    pub(crate) fn reaching_the_real_home() -> Reaching {
        Reaching(())
    }

    /// What [`reaching_the_real_home`] returns.
    pub(crate) struct Reaching(());
}

#[cfg(test)]
mod tests {
    use super::*;

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
