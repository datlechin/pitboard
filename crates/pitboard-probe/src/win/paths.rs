// T8's file system rows: FILE_ID_INFO, reserved names and long paths. Each block works in a
// folder of its own it makes under a scratch folder that passed the write guard, and removes
// it, so every name inside it is one the probe made.

use super::ffi::{self, Owned};
use super::{io_code, logon_now};
use crate::PROBE_PREFIX;
use crate::report::Report;
use serde_json::{Value, json};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdInfo, GetFileAttributesW,
    GetFileInformationByHandleEx, GetFullPathNameW, INVALID_FILE_ATTRIBUTES, OPEN_EXISTING,
    WriteFile,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW};

// A folder of the block's own, which must not be there yet.
fn own_folder(block: &'static str, scratch: &Path, stem: &str) -> Result<PathBuf, Report> {
    if let Some(reason) = ffi::refuse_write(scratch) {
        return Err(Report::refused(block, logon_now(), reason));
    }
    let folder = scratch.join(format!("{PROBE_PREFIX}{stem}-{}", std::process::id()));
    std::fs::create_dir_all(scratch)
        .and_then(|()| std::fs::create_dir(&folder))
        .map_err(|e| {
            Report::refused(
                block,
                logon_now(),
                format!("cannot make a folder of its own under the scratch folder: {e}"),
            )
        })?;
    Ok(folder)
}

fn opened(path: &Path, access: u32, disposition: u32, flags: u32) -> Result<Owned, u32> {
    let w = ffi::wide(path);
    // SAFETY: `w` is NUL-terminated; the handle is owned by the result.
    let h = unsafe {
        CreateFileW(
            w.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            disposition,
            flags,
            std::ptr::null_mut(),
        )
    };
    Owned::new(h).ok_or_else(ffi::last_error)
}

// The volume's serial number and the file's id on it.
type Id = (u64, u128);

// As `pitboard_core::host::windows::fs::file_id` reads it.
fn file_id(path: &Path, own: bool) -> Result<Id, u32> {
    let flags = FILE_FLAG_BACKUP_SEMANTICS | if own { FILE_FLAG_OPEN_REPARSE_POINT } else { 0 };
    let file = opened(path, FILE_READ_ATTRIBUTES, OPEN_EXISTING, flags)?;
    let mut info = FILE_ID_INFO::default();
    // SAFETY: an open handle, and a FILE_ID_INFO of exactly the length given.
    let read = unsafe {
        GetFileInformationByHandleEx(
            file.raw(),
            FileIdInfo,
            (&raw mut info).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if read == 0 {
        return Err(ffi::last_error());
    }
    Ok((
        info.VolumeSerialNumber,
        u128::from_le_bytes(info.FileId.Identifier),
    ))
}

// cmd's mklink needs no right of its own for a junction, unlike a symbolic link.
fn junction(at: &Path, target: &Path) -> Result<(), Value> {
    let made = std::process::Command::new("cmd.exe")
        .raw_arg(format!(
            "/d /c mklink /J \"{}\" \"{}\"",
            at.display(),
            target.display()
        ))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match made {
        Ok(status) if status.success() && at.is_dir() => Ok(()),
        Ok(status) => Err(json!({ "mklink_exit": status.code() })),
        Err(e) => Err(io_code(&e)),
    }
}

pub(super) fn file_id_block(scratch: &Path) -> Report {
    let root = match own_folder("file-id", scratch, "fid") {
        Ok(root) => root,
        Err(refused) => return refused,
    };
    let a = root.join("a");
    let b = root.join("b");
    let j = root.join("j");
    let file = a.join(format!("{PROBE_PREFIX}f"));
    let link = a.join(format!("{PROBE_PREFIX}h"));
    let other = b.join(format!("{PROBE_PREFIX}f"));
    let renamed = a.join(format!("{PROBE_PREFIX}g"));
    let mut steps = serde_json::Map::new();
    let made = std::fs::create_dir(&a)
        .and_then(|()| std::fs::create_dir(&b))
        .and_then(|()| std::fs::write(&file, b"x"))
        .and_then(|()| std::fs::write(&other, b"x"));
    if let Err(e) = made {
        let _ = std::fs::remove_dir_all(&root);
        return Report::refused(
            "file-id",
            logon_now(),
            format!("cannot make the folders and files: {e}"),
        );
    }
    let hard_link = std::fs::hard_link(&file, &link);
    steps.insert(
        "hard_link_error".into(),
        json!(hard_link.as_ref().map_err(io_code).err()),
    );
    steps.insert("junction_error".into(), json!(junction(&j, &a).err()));

    let ids: Vec<(&str, Result<Id, u32>)> = vec![
        ("folder", file_id(&a, false)),
        ("junction", file_id(&j, false)),
        ("junction_itself", file_id(&j, true)),
        ("file", file_id(&file, false)),
        ("hard_link", file_id(&link, false)),
        (
            "file_through_junction",
            file_id(&j.join(format!("{PROBE_PREFIX}f")), false),
        ),
        ("other_folder", file_id(&b, false)),
        ("other_file", file_id(&other, false)),
    ];
    let before_rename = file_id(&file, false);
    let rename = std::fs::rename(&file, &renamed);
    let after_rename = file_id(&renamed, false);
    let _ = std::fs::remove_dir(&j);
    let removed = std::fs::remove_dir_all(&root);

    let id = |name: &str| {
        ids.iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, r)| r.as_ref().ok().copied())
    };
    let same = |x: &str, y: &str| match (id(x), id(y)) {
        (Some(p), Some(q)) => json!(p == q),
        _ => Value::Null,
    };
    let errors: serde_json::Map<String, Value> = ids
        .iter()
        .filter_map(|(n, r)| r.as_ref().err().map(|e| ((*n).to_string(), json!(e))))
        .collect();
    let read_ids: Vec<Id> = ids
        .iter()
        .filter_map(|(_, r)| r.as_ref().ok().copied())
        .collect();
    Report::ok(
        "file-id",
        logon_now(),
        json!({
            "read": read_ids.len(),
            "errors": errors,
            "steps": steps,
            "one_volume_serial": read_ids.windows(2).all(|w| w[0].0 == w[1].0),
            "widest_id_bits": read_ids.iter().map(|(_, i)| 128 - i.leading_zeros()).max(),
            "junction_is_its_folder": same("junction", "folder"),
            "junction_itself_differs": same("junction_itself", "folder").as_bool().map(|s| !s),
            "hard_link_is_its_file": same("hard_link", "file"),
            "file_through_junction_is_the_file": same("file_through_junction", "file"),
            "other_folder_differs": same("other_folder", "folder").as_bool().map(|s| !s),
            "other_file_differs": same("other_file", "file").as_bool().map(|s| !s),
            "rename_error": rename.as_ref().map_err(io_code).err(),
            "kept_across_a_rename": match (before_rename, after_rename) {
                (Ok(p), Ok(q)) => json!(p == q),
                _ => Value::Null,
            },
            "removed": removed.is_ok(),
        }),
    )
}

// Every reserved name Microsoft's naming page lists in some spelling, some it does not, and
// trailing dots and spaces.
const NAMES: &[&str] = &[
    "CON",
    "con",
    "CON.json",
    "con.txt",
    "CON .json",
    "NUL",
    "NUL.json",
    "nul.tar.gz",
    "AUX.json",
    "PRN.json",
    "COM1",
    "COM1.json",
    "com9.json",
    "COM0.json",
    "LPT1.json",
    "LPT0.json",
    "COM\u{b9}.json",
    "LPT\u{b2}.json",
    "CONIN$",
    "CONOUT$",
    "CLOCK$.json",
    "CONX.json",
    "COM10.json",
    "work.",
    "work..",
    "work ",
    "work. .",
    "work.json.",
    "work.json ",
];

fn full_path(path: &Path) -> Result<String, u32> {
    let w = ffi::wide(path);
    let mut buf = vec![0u16; 1024];
    // SAFETY: `w` is NUL-terminated; `buf` holds `buf.len()` units; no file part is asked.
    let n = unsafe {
        GetFullPathNameW(
            w.as_ptr(),
            buf.len() as u32,
            buf.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    } as usize;
    if n == 0 || n >= buf.len() {
        return Err(ffi::last_error());
    }
    Ok(ffi::from_wide_buf(&buf[..n]))
}

pub(super) fn reserved_names(scratch: &Path) -> Report {
    let root = match own_folder("reserved-names", scratch, "names") {
        Ok(root) => root,
        Err(refused) => return refused,
    };
    let base = full_path(&root).unwrap_or_default();
    let mut rows = Vec::new();
    for name in NAMES {
        let path = root.join(name);
        let mut row = serde_json::Map::new();
        row.insert("name".into(), json!(name));
        match full_path(&path) {
            Ok(full) if full.starts_with(r"\\.\") => {
                row.insert("device".into(), json!(full));
            }
            Ok(full) => {
                let tail = full
                    .strip_prefix(&base)
                    .map(|t| t.trim_start_matches('\\').to_string());
                row.insert("full_path_names".into(), json!(tail));
                // Never opened where Windows takes it for a device.
                let made = std::fs::write(&path, b"x");
                row.insert("made".into(), json!(made.as_ref().map_err(io_code).err()));
                let stored: Vec<String> = std::fs::read_dir(&root)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect();
                row.insert("stored_as".into(), json!(stored));
                for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
            Err(code) => {
                row.insert("full_path_error".into(), json!(code));
            }
        }
        rows.push(Value::Object(row));
    }
    let removed = std::fs::remove_dir_all(&root);
    Report::ok(
        "reserved-names",
        logon_now(),
        json!({ "names": rows, "removed": removed.is_ok() }),
    )
}

fn long_paths_enabled_in_the_registry() -> Value {
    let key = ffi::wide(r"SYSTEM\CurrentControlSet\Control\FileSystem");
    let value = ffi::wide("LongPathsEnabled");
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: NUL-terminated names; a u32 of exactly the size given.
    let read = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut data).cast(),
            &raw mut size,
        )
    };
    if read == ERROR_SUCCESS {
        json!(data)
    } else {
        json!({ "error": read })
    }
}

// ntdll's, which windows-sys does not carry: whether both the system and this program's
// manifest let long paths through.
fn long_paths_on_for_this_process() -> Option<bool> {
    let ntdll = ffi::wide("ntdll.dll");
    // SAFETY: a NUL-terminated name of a module every process has loaded.
    let module = unsafe { GetModuleHandleW(ntdll.as_ptr()) };
    if module.is_null() {
        return None;
    }
    // SAFETY: a loaded module and a NUL-terminated export name.
    let found = unsafe { GetProcAddress(module, c"RtlAreLongPathsEnabled".as_ptr().cast()) }?;
    // SAFETY: RtlAreLongPathsEnabled takes nothing and returns a BOOLEAN.
    let call: extern "system" fn() -> u8 = unsafe { std::mem::transmute(found) };
    Some(call() != 0)
}

fn win32<T>(ok: bool, value: T) -> Value
where
    T: Into<Value>,
{
    if ok {
        value.into()
    } else {
        json!({ "error": ffi::last_error() })
    }
}

pub(super) fn long_path(scratch: &Path) -> Report {
    let root = match own_folder("long-path", scratch, "long") {
        Ok(root) => root,
        Err(refused) => return refused,
    };
    let root = std::path::absolute(&root).unwrap_or(root);
    let mut deep = root.clone();
    while deep.as_os_str().len() <= 300 {
        deep.push("d".repeat(40));
    }
    // std adds `\\?\` past MAX_PATH by itself, so the folders are there whatever the rest finds.
    let made = std::fs::create_dir_all(&deep);
    let file = deep.join(format!("{PROBE_PREFIX}long.txt"));
    let sub = deep.join(format!("{PROBE_PREFIX}sub"));
    let attributes = {
        let w = ffi::wide(&deep);
        // SAFETY: a NUL-terminated name.
        let found = unsafe { GetFileAttributesW(w.as_ptr()) };
        win32(found != INVALID_FILE_ATTRIBUTES, "found")
    };
    let created = match opened(
        &file,
        GENERIC_READ | GENERIC_WRITE,
        CREATE_NEW,
        FILE_ATTRIBUTE_NORMAL,
    ) {
        Ok(handle) => {
            let mut wrote = 0u32;
            // SAFETY: an open handle and one byte of this frame.
            let ok = unsafe {
                WriteFile(
                    handle.raw(),
                    b"x".as_ptr(),
                    1,
                    &raw mut wrote,
                    std::ptr::null_mut(),
                )
            };
            win32(ok != 0 && wrote == 1, "made")
        }
        Err(code) => json!({ "error": code }),
    };
    let reopened = match opened(&file, GENERIC_READ, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL) {
        Ok(_) => json!("opened"),
        Err(code) => json!({ "error": code }),
    };
    let directory = {
        let w = ffi::wide(&sub);
        // SAFETY: a NUL-terminated name and no security attributes.
        let ok = unsafe { CreateDirectoryW(w.as_ptr(), std::ptr::null()) };
        win32(ok != 0, "made")
    };
    let read_by_std = std::fs::metadata(&deep).map(|m| m.is_dir());
    let removed = std::fs::remove_dir_all(&root);
    Report::ok(
        "long-path",
        logon_now(),
        json!({
            "long_paths_enabled": long_paths_enabled_in_the_registry(),
            "on_for_this_process": long_paths_on_for_this_process(),
            "length": deep.as_os_str().len(),
            "folders_made_by_std": made.as_ref().map_err(io_code).err(),
            "std_reads_the_folder": read_by_std.as_ref().map_err(io_code).ok(),
            "get_file_attributes": attributes,
            "create_file": created,
            "open_again": reopened,
            "create_directory": directory,
            "removed": removed.is_ok(),
        }),
    )
}
