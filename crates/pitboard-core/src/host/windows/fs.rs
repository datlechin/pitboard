// Files on Windows. What Pitboard makes is private through its access list from the moment it
// is made, and what replaces another program's file takes that file's list.

use crate::host::acl::{self, Ace};
use crate::host::replace::{self, Route};
use crate::host::{Access, Private};
use crate::service::Permit;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::Path;
use std::time::SystemTime;
use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW, GetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT, SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, GetAce,
    GetSecurityDescriptorControl, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
    UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateDirectoryW, CreateFileW, DELETE, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE,
    FILE_READ_ATTRIBUTES, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FileRenameInfoEx, GetVolumeInformationByHandleW,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_ALWAYS, READ_CONTROL,
    SYNCHRONIZE, SetFileInformationByHandle, WRITE_DAC,
};

const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
// FILE_RENAME_FLAG_REPLACE_IF_EXISTS and FILE_RENAME_FLAG_POSIX_SEMANTICS (MC-rename), which
// windows-sys does not name.
const REPLACE_IF_EXISTS: u32 = 0x1;
const POSIX_SEMANTICS: u32 = 0x2;

fn macos_only() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Claude Desktop is macOS only, so Pitboard moves no app's folder on Windows",
    )
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

// Each missing parent is made the same way. A folder already there keeps its own access list,
// which `pitboard doctor` reads.
pub(crate) fn create_private_dir(_: Permit, path: &Path) -> io::Result<()> {
    make_all(path, &Descriptor::private(true)?)
}

// CREATE_NEW refuses whatever is there, and a link is not followed to find out.
pub(crate) fn create_private(_: Permit, path: &Path) -> io::Result<File> {
    let made = Descriptor::private(false)?;
    open(
        path,
        GENERIC_WRITE,
        CREATE_NEW,
        FILE_FLAG_OPEN_REPARSE_POINT,
        &made,
    )
}

// The descriptor is read only to make the file: one already there keeps its own access list.
pub(crate) fn open_private_append(_: Permit, path: &Path) -> io::Result<File> {
    let made = Descriptor::private(false)?;
    open(
        path,
        FILE_GENERIC_WRITE & !FILE_WRITE_DATA,
        OPEN_ALWAYS,
        0,
        &made,
    )
}

pub(crate) fn open_private_lock(_: Permit, path: &Path) -> io::Result<File> {
    let made = Descriptor::private(false)?;
    open(path, GENERIC_WRITE, OPEN_ALWAYS, 0, &made)
}

// As std's `OpenOptions` opens a file, sharing all three ways, but made with `made`.
fn open(
    path: &Path,
    access: u32,
    disposition: u32,
    flags: u32,
    made: &Descriptor,
) -> io::Result<File> {
    let name = wide(path);
    let attributes = made.attributes();
    // SAFETY: a NUL-terminated name, and a descriptor that outlives the call.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            SHARE_ALL,
            &raw const attributes,
            disposition,
            flags,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a handle CreateFileW just opened, which the File owns from here.
    Ok(unsafe { File::from_raw_handle(handle) })
}

// As std's `DirBuilder` makes a folder and its parents, each with `made`.
fn make_all(path: &Path, made: &Descriptor) -> io::Result<()> {
    if path == Path::new("") {
        return Ok(());
    }
    match make(path, made) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(_) if path.is_dir() => return Ok(()),
        Err(e) => return Err(e),
    }
    match path.parent() {
        Some(parent) => make_all(parent, made)?,
        None => return Err(io::Error::other("failed to create whole tree")),
    }
    match make(path, made) {
        Ok(()) => Ok(()),
        Err(_) if path.is_dir() => Ok(()),
        Err(e) => Err(e),
    }
}

fn make(path: &Path, made: &Descriptor) -> io::Result<()> {
    let name = wide(path);
    let attributes = made.attributes();
    // SAFETY: a NUL-terminated name, and a descriptor that outlives the call.
    if unsafe { CreateDirectoryW(name.as_ptr(), &raw const attributes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// A security descriptor made from SDDL, freed when dropped.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn from_sddl(sddl: &str) -> io::Result<Descriptor> {
        let text: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
        let mut made: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: a NUL-terminated SDDL, and `made` is LocalAlloc'd on success, freed on drop.
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &raw mut made,
                std::ptr::null_mut(),
            )
        };
        if converted == 0 || made.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Descriptor(made))
    }

    fn private(folder: bool) -> io::Result<Descriptor> {
        Descriptor::from_sddl(&acl::private(&you()?, folder))
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: the LocalAlloc'd descriptor the conversion gave, freed this once.
        unsafe { LocalFree(self.0) };
    }
}

fn you() -> io::Result<String> {
    crate::host::user::sid()
        .ok_or_else(|| io::Error::other("this Windows account's SID could not be read"))
}

// A path's owner and access list as Windows reads them, freed when dropped.
struct Security {
    descriptor: PSECURITY_DESCRIPTOR,
    owner: PSID,
    dacl: *mut ACL,
}

impl Security {
    fn of(path: &Path) -> io::Result<Security> {
        let name = wide(path);
        let mut read = Security {
            descriptor: std::ptr::null_mut(),
            owner: std::ptr::null_mut(),
            dacl: std::ptr::null_mut(),
        };
        // SAFETY: a NUL-terminated name; the owner and the list point into the descriptor,
        // which is freed when `read` drops.
        let failed = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &raw mut read.owner,
                std::ptr::null_mut(),
                &raw mut read.dacl,
                std::ptr::null_mut(),
                &raw mut read.descriptor,
            )
        };
        if failed != 0 {
            return Err(io::Error::from_raw_os_error(failed as i32));
        }
        Ok(read)
    }

    // `None` for a NULL DACL, which lets everyone in.
    fn aces(&self) -> Option<Vec<Ace>> {
        if self.dacl.is_null() {
            return None;
        }
        // SAFETY: the ACL GetNamedSecurityInfoW gave, inside the descriptor.
        let count = u32::from(unsafe { (*self.dacl).AceCount });
        Some((0..count).filter_map(|at| self.ace(at)).collect())
    }

    fn ace(&self, at: u32) -> Option<Ace> {
        let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
        // SAFETY: a valid ACL, and an index below its count.
        if unsafe { GetAce(self.dacl, at, &raw mut ace) } == 0 || ace.is_null() {
            return None;
        }
        // SAFETY: every ACE starts with its header.
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        // A conditional allow is taken as met, and a conditional deny as not.
        let allows = match header.AceType {
            0 | 9 => true,
            1 => false,
            _ => return None,
        };
        // SAFETY: an allow, a deny and a conditional allow share this layout, the SID at
        // SidStart.
        let body = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let sid: PSID = (&raw const body.SidStart).cast_mut().cast();
        Some(Ace {
            allows,
            sid: crate::host::user::sid_text(sid).unwrap_or_default(),
            mask: body.Mask,
            flags: header.AceFlags,
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    fn protected(&self) -> bool {
        let (mut control, mut revision) = (0u16, 0u32);
        // SAFETY: the descriptor GetNamedSecurityInfoW gave.
        let read = unsafe {
            GetSecurityDescriptorControl(self.descriptor, &raw mut control, &raw mut revision)
        };
        read != 0 && control & SE_DACL_PROTECTED != 0
    }

    #[cfg(test)]
    fn owner(&self) -> Option<String> {
        (!self.owner.is_null())
            .then(|| crate::host::user::sid_text(self.owner))
            .flatten()
    }
}

impl Drop for Security {
    fn drop(&mut self) {
        // SAFETY: null, or the LocalAlloc'd descriptor GetNamedSecurityInfoW gave.
        unsafe { LocalFree(self.descriptor) };
    }
}

// Who besides the person reaches `path`, from its access list. `None` where it cannot be read.
pub(crate) fn access(path: &Path) -> Option<Access> {
    let read = Security::of(path).ok()?;
    let you = crate::host::user::sid();
    Some(acl::access(read.aces().as_deref(), you.as_deref()))
}

// As std opens a file, sharing all three ways, with `access` alone.
fn opened(path: &Path, access: u32, flags: u32) -> io::Result<File> {
    std::fs::OpenOptions::new()
        .access_mode(access)
        .share_mode(SHARE_ALL)
        .custom_flags(flags)
        .open(path)
}

fn volume_flags(file: &File) -> Option<u32> {
    let mut flags = 0u32;
    // SAFETY: an open handle, and no buffer but the flags this frame owns.
    let read = unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut flags,
            std::ptr::null_mut(),
            0,
        )
    };
    (read != 0).then_some(flags)
}

// The list goes over protected or inherited as it is. Nothing is copied from a reparse point,
// which the rename replaces rather than follows, nor where nothing is there or the volume keeps
// no lists, as exFAT and FAT32 keep none.
pub(crate) fn copy_access(_: Permit, existing: &Path, temp: &Path) -> io::Result<()> {
    let from = match opened(
        existing,
        READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_FLAG_OPEN_REPARSE_POINT,
    ) {
        Ok(from) => from,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let link = from.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    let lists = volume_flags(&from).is_none_or(|flags| flags & replace::PERSISTENT_ACLS != 0);
    if link || !lists {
        return Ok(());
    }
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    // SAFETY: an open handle; the list points into the descriptor, freed below.
    let failed = unsafe {
        GetSecurityInfo(
            from.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut dacl,
            std::ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if failed != 0 {
        return Err(io::Error::from_raw_os_error(failed as i32));
    }
    let set = (|| {
        let (mut control, mut revision) = (0u16, 0u32);
        // SAFETY: the descriptor GetSecurityInfo gave.
        if unsafe { GetSecurityDescriptorControl(descriptor, &raw mut control, &raw mut revision) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        let kept = if control & SE_DACL_PROTECTED != 0 {
            PROTECTED_DACL_SECURITY_INFORMATION
        } else {
            UNPROTECTED_DACL_SECURITY_INFORMATION
        };
        let to = opened(temp, WRITE_DAC | READ_CONTROL, FILE_FLAG_OPEN_REPARSE_POINT)?;
        // SAFETY: an open handle, and a list inside the descriptor, which outlives the call.
        let failed = unsafe {
            SetSecurityInfo(
                to.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | kept,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null(),
            )
        };
        if failed != 0 {
            return Err(io::Error::from_raw_os_error(failed as i32));
        }
        Ok(())
    })();
    // SAFETY: the LocalAlloc'd descriptor GetSecurityInfo gave, freed this once.
    unsafe { LocalFree(descriptor) };
    set
}

// A rename lasts once its folder is flushed, which NTFS, ReFS, exFAT and FAT32 let a standard
// user do (RF-C5, RF-C5j). An SMB share refuses with 1, and is replaced by `MoveFileExW`, which
// writes through.
pub(crate) fn sync_dir(dir: &Path) {
    let _ = flush_dir(dir);
}

pub(crate) fn flush_dir(dir: &Path) -> io::Result<()> {
    opened(dir, GENERIC_WRITE, FILE_FLAG_BACKUP_SEMANTICS)?.sync_all()
}

pub(crate) fn touch_dir(_: Permit, path: &Path, at: SystemTime) -> io::Result<()> {
    opened(path, FILE_WRITE_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS)?.set_modified(at)
}

// A folder another program keeps takes its parent's list, as one it made would.
pub(crate) fn create_dir_all(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

pub(crate) fn create_dir(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::create_dir(path)
}

pub(crate) fn copy(_: Permit, from: &Path, to: &Path) -> io::Result<u64> {
    std::fs::copy(from, to)
}

// Never over a read-only file, whose owner said it is not to change, and never by clearing the
// attribute. A reader or a scanner holding `to` is waited out for RF-C4's budget.
pub(crate) fn rename(_: Permit, from: &Path, to: &Path) -> io::Result<()> {
    let name = to.file_name().unwrap_or(to.as_os_str()).to_string_lossy();
    if std::fs::symlink_metadata(to).is_ok_and(|found| found.permissions().readonly()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{name} is read-only, and Pitboard does not replace a read-only file"),
        ));
    }
    let mut waits = replace::waits();
    loop {
        let refused = match replaced(from, to) {
            Ok(()) => return Ok(()),
            Err(refused) => refused,
        };
        if !replace::waited_out(refused.raw_os_error()) {
            return Err(io::Error::new(
                refused.kind(),
                format!("{name} could not be replaced: {refused}"),
            ));
        }
        match waits.next() {
            Some(wait) => std::thread::sleep(wait),
            None => {
                return Err(io::Error::new(
                    refused.kind(),
                    format!(
                        "{name} stayed open in another program that does not let it be replaced"
                    ),
                ));
            }
        }
    }
}

fn replaced(from: &Path, to: &Path) -> io::Result<()> {
    let source = opened(
        from,
        DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_FLAG_OPEN_REPARSE_POINT,
    )?;
    match replace::route(volume_flags(&source)) {
        Route::Posix => posix_rename(&source, to),
        Route::MoveFile => {
            drop(source);
            let (from, to) = (verbatim(from)?, verbatim(to)?);
            // SAFETY: two NUL-terminated names.
            let moved = unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            };
            if moved == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }
}

// Laid out as std's own rename lays it: the name at FileName's offset, its length in bytes
// without the NUL, and room for the NUL.
fn posix_rename(source: &File, to: &Path) -> io::Result<()> {
    let mut name = verbatim(to)?;
    name.pop();
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let bytes = name.len() * 2;
    let size = (offset + bytes + 2).max(size_of::<FILE_RENAME_INFO>());
    // u64s, so the buffer is aligned for FILE_RENAME_INFO.
    let mut buffer = vec![0u64; size.div_ceil(8)];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: `buffer` holds `size` zeroed bytes aligned for FILE_RENAME_INFO; the header is
    // written in place and the name copied to FileName's offset, within `size`.
    unsafe {
        (*info).Anonymous.Flags = REPLACE_IF_EXISTS | POSIX_SEMANTICS;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = bytes as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            info.cast::<u8>().add(offset).cast::<u16>(),
            name.len(),
        );
    }
    // SAFETY: a handle open for DELETE, and a FILE_RENAME_INFO of `size` bytes.
    let renamed = unsafe {
        SetFileInformationByHandle(
            source.as_raw_handle(),
            FileRenameInfoEx,
            info.cast(),
            size as u32,
        )
    };
    if renamed == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// `path` whole, in the `\\?\` form, NUL-terminated.
fn verbatim(path: &Path) -> io::Result<Vec<u16>> {
    let full = std::path::absolute(path)?;
    let text = full.as_os_str();
    let lead: &str = match text.to_string_lossy() {
        spelled if spelled.starts_with(r"\\?\") || spelled.starts_with(r"\\.\") => "",
        spelled if spelled.starts_with(r"\\") => r"\\?\UNC",
        _ => r"\\?\",
    };
    let rest: Vec<u16> = text.encode_wide().collect();
    let rest = if lead == r"\\?\UNC" {
        &rest[1..]
    } else {
        &rest[..]
    };
    Ok(lead
        .encode_utf16()
        .chain(rest.iter().copied())
        .chain([0])
        .collect())
}

pub(crate) fn remove_file(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

pub(crate) fn remove_dir(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::remove_dir(path)
}

// On exFAT and FAT32 a file another program holds open keeps its folder, which fails 145
// (RF-C6x); NTFS and ReFS let it go.
pub(crate) fn remove_dir_all(_: Permit, path: &Path) -> io::Result<()> {
    std::fs::remove_dir_all(path)
}

// Claude Desktop is macOS only: a release refuses its folder first, and a build for tests
// stops here once the process list says its scratch Claude is closed.
pub(crate) fn private_dir(_path: &Path) -> io::Result<Private> {
    Err(macos_only())
}

pub(crate) fn lock_dir(_path: &Path) -> io::Result<File> {
    Err(macos_only())
}

pub(crate) fn move_new(_: Permit, _from: &Path, _to: &Path) -> io::Result<()> {
    Err(macos_only())
}

pub(crate) fn same_volume(_a: &Path, _b: &Path) -> io::Result<bool> {
    Err(macos_only())
}

// The 8.3 name Windows keeps beside a long one, where the volume keeps them (RF-B2).
pub(crate) fn short_path(path: &Path) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    let wide = wide(path);
    // SAFETY: a NUL-terminated name; with no buffer it answers the length it needs.
    let needed = unsafe { GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return None;
    }
    let mut short = vec![0u16; needed as usize];
    // SAFETY: a buffer of the `needed` units it asked for.
    let len = unsafe { GetShortPathNameW(wide.as_ptr(), short.as_mut_ptr(), needed) };
    (len != 0 && len < needed)
        .then(|| std::path::PathBuf::from(std::ffi::OsString::from_wide(&short[..len as usize])))
}

// MC-sync: asked of the deepest folder there, since a folder not made yet is in no root.
pub(crate) fn in_a_sync_root(path: &Path) -> bool {
    use windows_sys::Win32::Storage::CloudFilters::{
        CF_SYNC_ROOT_BASIC_INFO, CF_SYNC_ROOT_INFO_BASIC, CfGetSyncRootInfoByPath,
    };
    let Some(there) = path.ancestors().find(|above| above.exists()) else {
        return false;
    };
    let wide = wide(there);
    let mut info = CF_SYNC_ROOT_BASIC_INFO::default();
    let mut written = 0u32;
    // SAFETY: a NUL-terminated name, and a buffer of exactly the length given.
    let answer = unsafe {
        CfGetSyncRootInfoByPath(
            wide.as_ptr(),
            CF_SYNC_ROOT_INFO_BASIC,
            (&raw mut info).cast(),
            size_of::<CF_SYNC_ROOT_BASIC_INFO>() as u32,
            &raw mut written,
        )
    };
    answer >= 0
}

// The machine's, so a build for tests reads them too: what lies at their roots is
// `administered_file`'s to read.
pub(crate) fn fixed_drives() -> Vec<char> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;
    // SAFETY: no arguments.
    let present = unsafe { GetLogicalDrives() };
    ('A'..='Z')
        .zip(0..26)
        .filter(|(_, bit)| present & (1 << bit) != 0)
        .map(|(letter, _)| letter)
        .filter(|letter| {
            let root = wide(Path::new(&format!(r"{letter}:\")));
            // SAFETY: a NUL-terminated name.
            unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_FIXED }
        })
        .collect()
}

// The volume and the file's id on it (MC-fileid), a folder's included, through a junction.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn file_id(path: &Path) -> Option<(u64, u128)> {
    id(path, 0).ok()
}

// `FILE_FLAG_OPEN_REPARSE_POINT` in `flags` reads a link's own.
#[cfg(any(test, feature = "test-support"))]
fn id(path: &Path, flags: u32) -> io::Result<(u64, u128)> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };
    let file = opened(
        path,
        FILE_READ_ATTRIBUTES,
        FILE_FLAG_BACKUP_SEMANTICS | flags,
    )?;
    let mut info = FILE_ID_INFO::default();
    // SAFETY: an open handle, and a FILE_ID_INFO buffer of exactly the length given.
    let read = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&raw mut info).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        info.VolumeSerialNumber,
        u128::from_le_bytes(info.FileId.Identifier),
    ))
}

// A junction needs no right of its own, unlike a symbolic link, and leads to a full path on a
// local volume: std's own maker is not stable yet, so this writes the mount point as it does.
#[cfg(any(test, feature = "test-support"))]
fn junction(target: &Path, at: &Path) -> io::Result<()> {
    use std::path::{Component, Prefix};
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;
    use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;

    let refused = |why: &str| io::Error::new(io::ErrorKind::InvalidInput, why);
    let full = std::path::absolute(at.parent().unwrap_or(Path::new("")).join(target))?;
    let full = crate::host::without_verbatim_prefix(full);
    let local = match full.components().next() {
        Some(Component::Prefix(prefix)) => matches!(prefix.kind(), Prefix::Disk(_)),
        _ => false,
    };
    if !local {
        return Err(refused("a junction leads to a folder on a local volume"));
    }
    let name: Vec<u16> = r"\??\"
        .encode_utf16()
        .chain(full.as_os_str().encode_wide())
        .collect();
    let bytes = u16::try_from(name.len() * 2)
        .ok()
        .filter(|bytes| *bytes <= u16::MAX - 12)
        .ok_or_else(|| refused("the junction's target is too long"))?;
    let mut data: Vec<u8> = Vec::new();
    data.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    data.extend((bytes + 12).to_le_bytes());
    data.extend(0u16.to_le_bytes());
    // The substitute name first, then an empty print name, each ending in a NUL.
    for field in [0, bytes, bytes + 2, 0] {
        data.extend(field.to_le_bytes());
    }
    data.extend(name.iter().flat_map(|unit| unit.to_le_bytes()));
    data.extend([0u8; 4]);

    std::fs::create_dir(at)?;
    let set = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(at)
        .and_then(|folder| {
            let mut returned = 0u32;
            // SAFETY: an open folder, and a mount point's reparse data of exactly the length
            // given.
            let set = unsafe {
                DeviceIoControl(
                    folder.as_raw_handle(),
                    FSCTL_SET_REPARSE_POINT,
                    data.as_ptr().cast(),
                    data.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    &raw mut returned,
                    std::ptr::null_mut(),
                )
            };
            if set == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    if set.is_err() {
        let _ = std::fs::remove_dir(at);
    }
    set
}

// Each sets a whole protected access list, as `chmod` sets a whole mode, granting the person,
// SYSTEM and Administrators what the mode gives its owner.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(feature = "test-support"), allow(unreachable_pub))]
pub mod testing {
    use super::{Descriptor, Security, you};
    use crate::host::acl;
    use std::io;
    use std::path::Path;
    use windows_sys::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    };

    // Read and run, as a folder's 5 or a program's 755 give others.
    const READ_AND_RUN: &str = "0x1200a9";
    const EXECUTE: u32 = 0x20;

    pub(super) fn set(path: &Path, dacl: &str) -> io::Result<()> {
        let made = Descriptor::from_sddl(dacl)?;
        let (mut present, mut defaulted) = (0, 0);
        let mut list = std::ptr::null_mut();
        // SAFETY: a descriptor just made, whose list stays in it while `made` lives.
        let read = unsafe {
            GetSecurityDescriptorDacl(made.0, &raw mut present, &raw mut list, &raw mut defaulted)
        };
        if read == 0 {
            return Err(io::Error::last_os_error());
        }
        let name = super::wide(path);
        // SAFETY: a NUL-terminated name, and a list that outlives the call.
        let failed = unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                list,
                std::ptr::null(),
            )
        };
        if failed != 0 {
            return Err(io::Error::from_raw_os_error(failed as i32));
        }
        Ok(())
    }

    fn folder(path: &Path) -> io::Result<bool> {
        Ok(std::fs::metadata(path)?.is_dir())
    }

    fn passed_on(folder: bool) -> &'static str {
        if folder { "OICI" } else { "" }
    }

    // Users may read it.
    pub fn open_to_others(path: &Path) -> io::Result<()> {
        let folder = folder(path)?;
        let on = passed_on(folder);
        let you = you()?;
        set(
            path,
            &format!("D:P{}(A;{on};FR;;;BU)", acl::private_aces(&you, folder)),
        )
    }

    pub fn make_private(path: &Path) -> io::Result<()> {
        let folder = folder(path)?;
        set(path, &format!("D:P{}", acl::private_aces(&you()?, folder)))
    }

    pub fn is_private(path: &Path) -> io::Result<bool> {
        let folder = folder(path)?;
        let read = Security::of(path)?;
        let you = you()?;
        Ok(read
            .aces()
            .is_some_and(|aces| acl::is_private(read.protected(), &aces, &you, folder)))
    }

    pub fn read_only_for_owner(path: &Path) -> io::Result<()> {
        let on = passed_on(folder(path)?);
        set(path, &format!("D:P(A;{on};FR;;;{})", you()?))
    }

    // Its owner included.
    pub fn deny_reading(path: &Path) -> io::Result<()> {
        let folder = folder(path)?;
        let you = you()?;
        set(
            path,
            &format!("D:P(D;;0x7;;;WD){}", acl::private_aces(&you, folder)),
        )
    }

    // Windows has no run bit, so only what `deny_running` set is undone.
    pub fn make_runnable(path: &Path) -> io::Result<()> {
        folder(path)?;
        let denied = Security::of(path)?.aces().is_some_and(|aces| {
            aces.iter()
                .any(|ace| !ace.allows && ace.mask & EXECUTE != 0)
        });
        if !denied {
            return Ok(());
        }
        let you = you()?;
        set(
            path,
            &format!(
                "D:P{}(A;;{READ_AND_RUN};;;BU)",
                acl::private_aces(&you, false)
            ),
        )
    }

    // Anybody may still read it.
    pub fn deny_running(path: &Path) -> io::Result<()> {
        folder(path)?;
        let you = you()?;
        set(
            path,
            &format!(
                "D:P(D;;0x20;;;WD){}(A;;FR;;;BU)",
                acl::private_aces(&you, false)
            ),
        )
    }

    // A file in it with an access list of its own, as Pitboard makes one, can still go.
    pub fn deny_changes(dir: &Path) -> io::Result<()> {
        let you = you()?;
        set(
            dir,
            &format!(
                "D:P(D;;0x46;;;WD)(D;OICIIO;SD;;;WD){}(A;OICI;{READ_AND_RUN};;;BU)",
                acl::private_aces(&you, true)
            ),
        )
    }

    pub fn allow_changes(dir: &Path) -> io::Result<()> {
        let you = you()?;
        set(
            dir,
            &format!(
                "D:P{}(A;OICI;{READ_AND_RUN};;;BU)",
                acl::private_aces(&you, true)
            ),
        )
    }

    // Developer Mode or the symbolic-link right, which CI's runners have (AGENTS.md).
    pub fn link(target: &Path, at: &Path) -> io::Result<()> {
        std::os::windows::fs::symlink_file(target, at)
    }

    // A junction, which needs no right of its own.
    pub fn link_dir(target: &Path, at: &Path) -> io::Result<()> {
        super::junction(target, at)
    }

    // A link's own, folded to 64 bits: how a test tells a move from a copy.
    pub fn inode(path: &Path) -> io::Result<u64> {
        let (_, id) = super::id(path, super::FILE_FLAG_OPEN_REPARSE_POINT)?;
        Ok((id as u64) ^ ((id >> 64) as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{Kind, Os};
    use std::io::Write;

    // `name`, not made yet, in a folder of the test's own.
    fn scratch(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let folder = tempfile::tempdir().expect("a scratch folder");
        let dir = folder.path().join(name);
        (folder, dir)
    }

    #[test]
    fn the_fixed_drives_hold_the_one_this_test_keeps_its_temporaries_on() {
        use std::path::{Component, Prefix};
        let drives = fixed_drives();
        println!("fixed drives: {drives:?}");
        let temp = std::env::temp_dir();
        let Some(Component::Prefix(prefix)) = temp.components().next() else {
            panic!("{} names no drive", temp.display());
        };
        let (Prefix::Disk(letter) | Prefix::VerbatimDisk(letter)) = prefix.kind() else {
            panic!("{} names no drive letter", temp.display());
        };
        assert!(drives.contains(&char::from(letter).to_ascii_uppercase()));
        assert!(drives.iter().all(char::is_ascii_uppercase));
        assert!(drives.is_sorted_by(|a, b| a < b), "each once");
    }

    fn private_to_you(path: &Path, folder: bool) {
        let you = crate::host::user::sid().expect("this account's SID");
        let read = Security::of(path).expect("its security");
        assert!(read.protected(), "{}", path.display());
        assert_eq!(read.owner(), Some(you.clone()), "{}", path.display());
        let aces = read.aces().expect("a list");
        assert!(
            acl::is_private(true, &aces, &you, folder),
            "{}: {aces:?}",
            path.display()
        );
    }

    #[test]
    fn what_pitboard_makes_is_the_persons_alone_and_what_is_made_inside_takes_it() {
        let (_folder, dir) = scratch("private");
        let permit = Permit::for_a_test();
        let nested = dir.join("a").join("b");
        create_private_dir(permit, &nested).expect("made");
        for folder in [&dir, &dir.join("a"), &nested] {
            private_to_you(folder, true);
        }
        let (fresh, history, lock) = (
            nested.join(".state.json.1.0.pitboard"),
            nested.join("history.jsonl"),
            nested.join("state.lock"),
        );
        writeln!(create_private(permit, &fresh).expect("made"), "{{}}").expect("written");
        writeln!(open_private_append(permit, &history).expect("made"), "one").expect("written");
        open_private_lock(permit, &lock).expect("made");
        for made in [&fresh, &history, &lock] {
            private_to_you(made, false);
            assert_eq!(
                access(made),
                Some(Access {
                    shared: false,
                    described: "open to you, SYSTEM and Administrators".into(),
                }),
                "{}",
                made.display()
            );
        }

        let inside = nested.join("inside");
        std::fs::write(&inside, "x").expect("written");
        let read = Security::of(&inside).expect("its security");
        assert!(!read.protected());
        let you = crate::host::user::sid().expect("this account's SID");
        let mut inherited: Vec<(String, u32, u8)> = read
            .aces()
            .expect("a list")
            .into_iter()
            .map(|ace| (ace.sid, ace.mask, ace.flags))
            .collect();
        inherited.sort();
        let mut wanted: Vec<(String, u32, u8)> = [you.as_str(), acl::SYSTEM, acl::ADMINISTRATORS]
            .map(|who| (who.to_owned(), acl::ALL, 0x10))
            .to_vec();
        wanted.sort();
        assert_eq!(inherited, wanted);
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    #[test]
    fn what_is_there_already_keeps_its_access_and_its_contents() {
        let (_folder, dir) = scratch("there");
        let permit = Permit::for_a_test();
        std::fs::create_dir_all(&dir).expect("a folder");
        testing::open_to_others(&dir).expect("opened");
        create_private_dir(permit, &dir).expect("there already");
        assert!(
            access(&dir).expect("read").shared,
            "a folder keeps its list"
        );

        let (history, lock) = (dir.join("history.jsonl"), dir.join("state.lock"));
        for there in [&history, &lock] {
            std::fs::write(there, "one\n").expect("written");
            testing::open_to_others(there).expect("opened");
        }
        writeln!(
            open_private_append(permit, &history).expect("opened"),
            "two"
        )
        .expect("added");
        open_private_lock(permit, &lock).expect("opened");
        assert_eq!(
            std::fs::read_to_string(&history).expect("read"),
            "one\ntwo\n"
        );
        assert_eq!(std::fs::read_to_string(&lock).expect("read"), "one\n");
        for there in [&history, &lock] {
            assert!(access(there).expect("read").shared, "{}", there.display());
        }

        let fresh = dir.join(".state.json.1.0.pitboard");
        create_private(permit, &fresh).expect("made");
        let refused = create_private(permit, &fresh).expect_err("a file is there");
        assert_eq!(refused.kind(), io::ErrorKind::AlreadyExists);
        let planted = dir.join(".usage.json.1.0.pitboard");
        testing::link(&dir.join("elsewhere"), &planted).expect("a link");
        assert!(create_private(permit, &planted).is_err(), "a link is there");
        assert!(
            !dir.join("elsewhere").exists(),
            "and nothing is written through it"
        );
        testing::make_private(&dir).expect("private");
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    #[test]
    fn each_way_a_test_sets_a_file_up_does_what_it_says() {
        let (_folder, dir) = scratch("testing");
        std::fs::create_dir_all(&dir).expect("a folder");
        let file = dir.join("file");
        std::fs::write(&file, "x").expect("written");
        let described = |path: &Path| access(path).expect("read").described;

        testing::open_to_others(&file).expect("opened");
        assert!(access(&file).expect("read").shared);
        assert_eq!(
            described(&file),
            "open to Users, you, SYSTEM and Administrators"
        );
        assert!(!testing::is_private(&file).expect("read"));
        testing::make_private(&file).expect("private");
        assert!(testing::is_private(&file).expect("read"));
        assert_eq!(std::fs::read(&file).expect("its owner reads it"), b"x");

        testing::read_only_for_owner(&file).expect("read only");
        assert_eq!(described(&file), "open to you");
        assert_eq!(std::fs::read(&file).expect("read"), b"x");
        let refused = std::fs::write(&file, "y").expect_err("nobody changes it");
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);

        testing::deny_reading(&file).expect("denied");
        let refused = std::fs::read(&file).expect_err("nobody reads it");
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        testing::make_private(&file).expect("private again");
        assert_eq!(std::fs::read(&file).expect("read again"), b"x");

        let program = dir.join("program.exe");
        std::fs::copy(std::env::current_exe().expect("this test"), &program).expect("copied");
        testing::deny_running(&program).expect("denied");
        let refused = std::process::Command::new(&program)
            .arg("--list")
            .output()
            .expect_err("nobody runs it");
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        assert!(std::fs::read(&program).is_ok(), "anybody still reads it");
        testing::make_runnable(&program).expect("runnable");
        let ran = std::process::Command::new(&program)
            .args(["--list", "--format=terse"])
            .output()
            .expect("it runs again");
        assert!(ran.status.success(), "{ran:?}");

        let folder = dir.join("folder");
        std::fs::create_dir(&folder).expect("a folder");
        std::fs::write(folder.join("inner"), "x").expect("written");
        testing::make_private(&folder).expect("private");
        assert!(testing::is_private(&folder).expect("read"));
        testing::deny_changes(&folder).expect("read-only");
        let nothing_added = std::fs::write(folder.join("new"), "x").expect_err("added");
        assert_eq!(nothing_added.kind(), io::ErrorKind::PermissionDenied);
        assert!(
            std::fs::remove_file(folder.join("inner")).is_err(),
            "removed"
        );
        let moved = std::fs::rename(folder.join("inner"), dir.join("inner"));
        assert!(moved.is_err(), "moved");
        assert_eq!(std::fs::read(folder.join("inner")).expect("read"), b"x");
        testing::allow_changes(&folder).expect("changeable again");
        std::fs::write(folder.join("new"), "x").expect("added");
        std::fs::remove_file(folder.join("inner")).expect("removed");
        assert!(access(&folder).expect("read").shared, "as 755 is");

        testing::link(Path::new("file"), &dir.join("to-file")).expect("a link");
        assert_eq!(
            std::fs::read_link(dir.join("to-file")).expect("a link"),
            Path::new("file")
        );
        assert!(
            dir.join("to-file").is_file(),
            "read from the link's own folder"
        );
        testing::link_dir(&folder, &dir.join("to-folder")).expect("a junction");
        assert!(dir.join("to-folder").is_dir());
        assert!(dir.join("to-folder").join("new").is_file());
        let id = file_id(&folder).expect("an id");
        assert_eq!(file_id(&dir.join("to-folder")), Some(id));
        testing::link_dir(Path::new("folder"), &dir.join("relative")).expect("a junction");
        assert_eq!(file_id(&dir.join("relative")), Some(id));
        assert_ne!(file_id(&dir), Some(id), "another folder's");
        assert_eq!(file_id(&dir.join("never-made")), None);

        let inode = testing::inode(&file).expect("there");
        assert_ne!(
            testing::inode(&dir.join("to-folder")).expect("there"),
            testing::inode(&folder).expect("there"),
            "a junction's own"
        );
        std::fs::rename(&file, dir.join("moved")).expect("moved");
        assert_eq!(
            testing::inode(&dir.join("moved")).expect("there"),
            inode,
            "a move keeps it"
        );
        std::fs::copy(dir.join("moved"), &file).expect("copied");
        assert_ne!(
            testing::inode(&file).expect("there"),
            inode,
            "a copy does not"
        );
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    #[test]
    fn access_names_who_else_reads_a_file_in_windows_words() {
        let (_folder, dir) = scratch("access");
        let folder = dir.join("folder");
        std::fs::create_dir_all(&folder).expect("a folder");
        let file = dir.join("file");
        std::fs::write(&file, "x").expect("written");
        let you = crate::host::user::sid().expect("this account's SID");
        let private = acl::private_aces(&you, false);
        let found = |path: &Path, dacl: &str| {
            testing::set(path, dacl).expect(dacl);
            access(path).expect("read")
        };
        for (sid, name) in [
            ("WD", "Everyone"),
            ("BU", "Users"),
            ("AU", "Authenticated Users"),
            (
                "S-1-5-21-1004336348-1177238915-682003330-4242",
                "another account",
            ),
        ] {
            let read = found(&file, &format!("D:P{private}(A;;FR;;;{sid})"));
            assert!(read.shared, "{sid}");
            assert_eq!(
                read.described,
                format!("open to {name}, you, SYSTEM and Administrators")
            );
            assert!(!read.described.contains("S-1-"), "{}", read.described);
        }
        let folders = acl::private_aces(&you, true);
        let below = found(&folder, &format!("D:P{folders}(A;OICIIO;FA;;;WD)"));
        assert!(!below.shared, "inherit-only");
        let users = "(A;;FR;;;BU)";
        assert!(
            found(&file, &format!("D:P(D;;0x1;;;WD){private}{users}")).shared,
            "Everyone denied"
        );
        assert!(
            !found(&file, &format!("D:P(D;;0x1;;;BU){private}{users}")).shared,
            "Users denied"
        );
        let open = found(&file, "D:NO_ACCESS_CONTROL");
        assert!(open.shared, "a NULL DACL");
        assert_eq!(open.described, "open to everyone, with no access list");
        testing::make_private(&file).expect("private");
        assert_eq!(access(&dir.join("never-made")), None);
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    // PowerShell's -EncodedCommand takes the command as base64 of its UTF-16, so nothing in it
    // is quoted again on the way.
    fn encoded(command: &str) -> String {
        use base64::prelude::{BASE64_STANDARD, Engine as _};
        let bytes: Vec<u8> = command.encode_utf16().flat_map(u16::to_le_bytes).collect();
        BASE64_STANDARD.encode(&bytes)
    }

    // Each line as a person pastes it, into cmd or into PowerShell.
    fn run_advice(shell: &str, kind: Kind, path: &Path) {
        use std::os::windows::process::CommandExt;
        let you = crate::host::user::sid().expect("this account's SID");
        let named = path.display().to_string();
        let lines = Os::Windows.make_private_commands(kind, &[&named], Some(&you));
        assert_eq!(lines.len(), 2, "{lines:?}");
        for line in lines {
            let ran = match shell {
                "cmd" => std::process::Command::new("cmd.exe")
                    .args(["/d", "/c"])
                    .raw_arg(&line)
                    .output(),
                _ => std::process::Command::new("powershell.exe")
                    .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
                    .arg(encoded(&line))
                    .output(),
            }
            .expect(shell);
            assert!(ran.status.success(), "{shell}: {line}: {ran:?}");
        }
    }

    #[test]
    fn the_icacls_advice_closes_a_login_to_others_and_its_owner_still_reads_it() {
        let (_folder, dir) = scratch("icacls Đạt");
        std::fs::create_dir_all(&dir).expect("a folder");
        testing::open_to_others(&dir).expect("opened");
        // As a folder made under one others read is open: through what it inherits.
        let shared = dir.join("shared folder");
        std::fs::create_dir(&shared).expect("a folder");
        assert!(access(&shared).expect("read").shared, "its parent's list");
        let private = Access {
            shared: false,
            described: "open to you, SYSTEM and Administrators".into(),
        };
        for shell in ["cmd", "powershell"] {
            let login = shared.join(format!("{shell} Đạt's login.json"));
            std::fs::write(&login, "x").expect("written");
            assert!(access(&login).expect("read").shared, "its folder's list");
            run_advice(shell, Kind::File, &login);
            assert_eq!(access(&login), Some(private.clone()), "{shell}");
            assert_eq!(std::fs::read(&login).expect("its owner reads it"), b"x");
            // As `icacls /grant Users:R` opens the file itself.
            testing::open_to_others(&login).expect("opened");
            run_advice(shell, Kind::File, &login);
            assert_eq!(
                access(&login),
                Some(private.clone()),
                "{shell}: its own grant"
            );
        }
        let inside = shared.join("inside");
        std::fs::write(&inside, "x").expect("written");
        testing::open_to_others(&shared).expect("opened");
        run_advice("powershell", Kind::Directory, &shared);
        assert_eq!(access(&shared), Some(private.clone()));
        assert_eq!(
            access(&inside),
            Some(private.clone()),
            "what is inside takes it"
        );
        assert_eq!(std::fs::read(&inside).expect("its owner reads it"), b"x");
        // A trailing separator, whose `\"` the C runtime would read as a quote.
        testing::open_to_others(&shared).expect("opened");
        let spelled = std::path::PathBuf::from(format!("{}\\", shared.display()));
        run_advice("cmd", Kind::Directory, &spelled);
        assert_eq!(access(&shared), Some(private));
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    #[test]
    fn a_path_with_a_space_has_a_short_name_that_leads_to_it() {
        let (_folder, long) = scratch("pitboard short");
        std::fs::create_dir_all(&long).expect("a scratch folder");
        let short = short_path(&long).expect("a short name");
        assert!(
            !short.to_string_lossy().contains(' '),
            "{}",
            short.display()
        );
        assert_eq!(
            std::fs::canonicalize(&short).expect("there"),
            std::fs::canonicalize(&long).expect("there")
        );
        assert_eq!(short_path(&long.join("never-made")), None);
        let _ = std::fs::remove_dir_all(&long);
    }

    fn listed(path: &Path) -> (bool, Vec<(String, u32, u8, bool)>) {
        let read = Security::of(path).expect("its security");
        let mut aces: Vec<_> = read
            .aces()
            .expect("a list")
            .into_iter()
            .map(|ace| (ace.sid, ace.mask, ace.flags, ace.allows))
            .collect();
        aces.sort();
        aces.dedup();
        (read.protected(), aces)
    }

    #[test]
    fn what_replaces_a_tools_file_keeps_its_list_inherited_or_protected() {
        use crate::atomic::{Perms, write};
        let (_folder, dir) = scratch("keeps");
        std::fs::create_dir_all(&dir).expect("a folder");
        let permit = Permit::for_a_test();
        let (inherited, protected) = (dir.join(".claude.json"), dir.join(".credentials.json"));
        for made in [&inherited, &protected] {
            std::fs::write(made, "{}").expect("written");
        }
        testing::open_to_others(&protected).expect("opened");
        for path in [&inherited, &protected] {
            let before = listed(path);
            write(permit, path, b"{\"new\":1}", Perms::MatchExisting).expect("replaced");
            assert_eq!(listed(path), before, "{}", path.display());
            assert_eq!(std::fs::read(path).expect("read"), b"{\"new\":1}");
        }
        assert!(!listed(&inherited).0, "it inherits still");
        assert!(listed(&protected).0);
        assert!(access(&protected).expect("read").shared);

        write(permit, &protected, b"{}", Perms::Secret).expect("replaced");
        private_to_you(&protected, false);
        assert_eq!(std::fs::read_dir(&dir).expect("listed").count(), 2);
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    #[test]
    fn a_read_only_file_is_refused_and_left_as_it_was() {
        use crate::atomic::{Perms, write};
        let (_folder, dir) = scratch("read-only");
        std::fs::create_dir_all(&dir).expect("a folder");
        let path = dir.join(".claude.json");
        std::fs::write(&path, "{}").expect("written");
        let writable = std::fs::metadata(&path).expect("there").permissions();
        let mut read_only = writable.clone();
        read_only.set_readonly(true);
        std::fs::set_permissions(&path, read_only).expect("read-only");
        for perms in [Perms::Secret, Perms::MatchExisting] {
            let refused = write(Permit::for_a_test(), &path, b"new", perms).expect_err("refused");
            assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(
                refused.to_string(),
                ".claude.json is read-only, and Pitboard does not replace a read-only file"
            );
        }
        assert_eq!(std::fs::read(&path).expect("read"), b"{}");
        assert!(
            std::fs::metadata(&path)
                .expect("there")
                .permissions()
                .readonly()
        );
        assert_eq!(std::fs::read_dir(&dir).expect("listed").count(), 1);
        std::fs::set_permissions(&path, writable).expect("writable again");
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    // As proper-lockfile keeps a lock: the time set is the time read back.
    #[test]
    fn a_folders_time_is_set_flushed_and_read_back() {
        let (_folder, dir) = scratch("touch");
        let lock = dir.join("target.lock");
        std::fs::create_dir_all(&lock).expect("a lock");
        let at = SystemTime::now() - std::time::Duration::from_secs(3600);
        touch_dir(Permit::for_a_test(), &lock, at).expect("touched");
        assert_eq!(
            std::fs::metadata(&lock)
                .expect("there")
                .modified()
                .expect("its time"),
            at
        );
        flush_dir(&dir).expect("flushed");
        assert!(flush_dir(&dir.join("missing")).is_err());
        assert!(touch_dir(Permit::for_a_test(), &dir.join("missing"), at).is_err());
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    // RF-C4's 10,000 replaces, through Pitboard's own write, beside another process reading the
    // file again and again through Pitboard's own read; then beside one holding it without
    // sharing its deletion.
    #[test]
    fn ten_thousand_replaces_never_leave_a_reader_without_the_whole_file() {
        use crate::atomic::{Perms, write};
        use crate::helper::{Helper, Job, counts, record};
        let (_folder, dir) = scratch("replaces");
        std::fs::create_dir_all(&dir).expect("a folder");
        let path = dir.join("state.json");
        let permit = Permit::for_a_test();
        write(permit, &path, &record(0), Perms::Secret).expect("written");
        let reader = Helper::start(Job::Reads, &path);
        for round in 1..=10_000 {
            write(permit, &path, &record(round), Perms::Secret)
                .unwrap_or_else(|e| panic!("round {round}: {e}"));
        }
        let said = reader.stop();
        let [reads, whole, torn, missing, failed] = counts(&said);
        assert!(reads > 0, "{said}");
        assert_eq!((whole, torn, missing, failed), (reads, 0, 0, 0), "{said}");

        let holder = Helper::start(Job::HoldsWithoutDelete, &path);
        let started = std::time::Instant::now();
        let refused = write(permit, &path, &record(0), Perms::Secret).expect_err("held");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            refused.to_string(),
            "state.json stayed open in another program that does not let it be replaced"
        );
        holder.stop();
        assert_eq!(std::fs::read(&path).expect("read"), record(10_000));
        assert_eq!(std::fs::read_dir(&dir).expect("listed").count(), 1);
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    // exFAT keeps no POSIX rename and no access list, so `MoveFileExW` replaces there, Pitboard's
    // own files and a tool's, and any program holding the file is waited out, then refused.
    #[test]
    fn a_volume_without_posix_renames_is_written_through_move_file() {
        use crate::atomic::{Perms, write};
        use crate::helper::{Helper, Job, exfat};
        let dir = exfat("pitboard-move-file");
        std::fs::create_dir_all(&dir).expect("a folder on the volume");
        let flags = volume_flags(
            &opened(&dir, FILE_READ_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS)
                .expect("the folder opened"),
        );
        assert_eq!(replace::route(flags), Route::MoveFile, "{flags:x?}");
        let permit = Permit::for_a_test();
        let (own, tools) = (dir.join("state.json"), dir.join(".claude.json"));
        std::fs::write(&tools, "{}").expect("written");
        for round in 0..3 {
            let body = format!("{{\"round\":{round}}}");
            write(permit, &own, body.as_bytes(), Perms::Secret).expect("replaced");
            write(permit, &tools, body.as_bytes(), Perms::MatchExisting).expect("replaced");
            for path in [&own, &tools] {
                assert_eq!(std::fs::read_to_string(path).expect("read"), body);
            }
        }
        assert_eq!(std::fs::read_dir(&dir).expect("listed").count(), 2);

        let holder = Helper::start(Job::Locks, &own);
        let started = std::time::Instant::now();
        let refused = write(permit, &own, b"{}", Perms::Secret).expect_err("held");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert_eq!(
            refused.to_string(),
            "state.json stayed open in another program that does not let it be replaced"
        );
        holder.stop();
        assert_eq!(
            std::fs::read_to_string(&own).expect("read"),
            "{\"round\":2}"
        );
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    #[test]
    fn a_file_a_test_wrote_is_runnable_as_it_is() {
        let (_folder, dir) = scratch("runnable");
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        let program = dir.join("program.exe");
        std::fs::copy(std::env::current_exe().expect("this test"), &program).expect("copied");
        let before = std::fs::read(&program).expect("read");
        testing::make_runnable(&program).expect("runnable as it is");
        assert_eq!(std::fs::read(&program).expect("read"), before, "unchanged");
        let missing = testing::make_runnable(&dir.join("absent.exe")).expect_err("absent");
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        let ran = std::process::Command::new(&program)
            .args(["--list", "--format=terse"])
            .output()
            .expect("the copy runs");
        assert!(ran.status.success(), "{ran:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
