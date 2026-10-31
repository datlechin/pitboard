// How Windows replaces a file, chosen apart from the system so every system's tests read the
// rules: the route a volume's flags allow, which refusals are waited out, and for how long.

use std::time::Duration;

// FILE_SUPPORTS_POSIX_UNLINK_RENAME. NTFS and ReFS report it; exFAT, FAT32 and an SMB share
// mapped as a drive do not (docs/facts/windows-runners.md).
pub(crate) const POSIX_UNLINK_RENAME: u32 = 0x400;
// FILE_PERSISTENT_ACLS: a volume that keeps an access list on each file. exFAT and FAT32 keep
// none.
pub(crate) const PERSISTENT_ACLS: u32 = 0x8;

const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_SHARING_VIOLATION: i32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    // SetFileInformationByHandle(FileRenameInfoEx, REPLACE_IF_EXISTS | POSIX_SEMANTICS), which
    // lands over a file another program holds open sharing delete.
    Posix,
    // MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH), which every volume takes and any open
    // handle on the target refuses with 5.
    MoveFile,
}

// A volume whose flags cannot be read takes the route every volume takes.
pub(crate) fn route(flags: Option<u32>) -> Route {
    match flags {
        Some(flags) if flags & POSIX_UNLINK_RENAME != 0 => Route::Posix,
        _ => Route::MoveFile,
    }
}

// A reader or a scanner that holds the file for a moment. Anything else fails at once.
pub(crate) fn waited_out(error: Option<i32>) -> bool {
    matches!(error, Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION))
}

// A file missing from a folder that is there, as a reader can find one NTFS replaces. A
// folder that is not there, 3, is no replace.
pub(crate) fn read_again(error: Option<i32>) -> bool {
    error == Some(ERROR_FILE_NOT_FOUND)
}

// 50 waits doubling from 1 ms to 50 ms, about 2.3 s in all, for a writer and for a reader that
// finds the file missing under another run's lock. Beside readers that reopen the file without a
// pause, `MoveFileExW` needed at most 33 attempts on windows-11-arm, and on windows-2025 ran out
// in 30 of 184 rounds, which no longer wait helps (docs/facts/windows-runners.md).
pub(crate) fn waits() -> impl Iterator<Item = Duration> {
    (0..50u32).map(|attempt| Duration::from_millis((1u64 << attempt.min(6)).min(50)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each volume's flags as GetVolumeInformationByHandleW gave them on both runner images
    // (run 38693947329).
    #[test]
    fn each_kind_of_volume_takes_the_route_its_flags_allow() {
        for (volume, flags, wanted) in [
            ("NTFS", 0x03e7_2eff, Route::Posix),
            ("ReFS and a Dev Drive", 0x5fc6_36cf, Route::Posix),
            ("exFAT and FAT32", 0x0002_0206, Route::MoveFile),
            (
                "an SMB share mapped as a drive",
                0x00c7_00fe,
                Route::MoveFile,
            ),
        ] {
            assert_eq!(route(Some(flags)), wanted, "{volume}");
        }
        assert_eq!(route(None), Route::MoveFile, "flags not read");
        assert_eq!(0x03e7_2eff & PERSISTENT_ACLS, PERSISTENT_ACLS, "NTFS");
        assert_eq!(0x0002_0206 & PERSISTENT_ACLS, 0, "exFAT");
    }

    #[test]
    fn only_a_sharing_violation_and_access_denied_are_waited_out() {
        assert!(waited_out(Some(32)));
        assert!(waited_out(Some(5)));
        for other in [None, Some(2), Some(3), Some(87), Some(145), Some(183)] {
            assert!(!waited_out(other), "{other:?}");
        }
    }

    #[test]
    fn a_reader_looks_again_only_for_a_file_missing_from_a_folder_that_is_there() {
        assert!(read_again(Some(2)));
        for other in [None, Some(3), Some(5), Some(32)] {
            assert!(!read_again(other), "{other:?}");
        }
    }

    #[test]
    fn the_waits_double_to_fifty_milliseconds_and_end() {
        let waits: Vec<u128> = waits().map(|wait| wait.as_millis()).collect();
        assert_eq!(waits[..8], [1, 2, 4, 8, 16, 32, 50, 50]);
        assert_eq!(waits.len(), 50);
        assert_eq!(waits.iter().sum::<u128>(), 2263);
    }
}
