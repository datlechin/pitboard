//! A2 and B1-B4: the build, every home a tool or the standard library may use, the profile
//! type, facts about the account name, the spellings a tool may hash a home from, and the
//! search-path variable's spellings. Every path passes through the account's redactor, so
//! the profile reads `<profile>` and the account name `<user>`; what differs between the
//! name's spellings is reported as facts.

use super::ffi::{self, Token};
use super::{logon_now, take_child_report};
use crate::hashes;
use crate::report::Report;
use serde_json::{Map, Value, json};
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::UI::Shell::GetProfileType;

const TAG_A: &str = "pitboard-probe-path-a";
const TAG_B: &str = "pitboard-probe-path-b";

pub(super) fn homes(codex_home: Option<&Path>, claude_config_dir: Option<&Path>) -> Report {
    let logon = logon_now();
    let r = ffi::redactor();
    let red = |p: &Path| r.redact(&p.display().to_string());
    let env = |k: &str| std::env::var_os(k).map(|v| r.redact(&v.to_string_lossy()));
    #[allow(
        deprecated,
        reason = "std::env::home_dir is one of the homes B1 compares"
    )]
    let std_home = std::env::home_dir();
    let profile = ffi::folder_profile();
    let local_app_data = ffi::folder_local_app_data();
    let temp = std::env::temp_dir();
    let local_app_data_in_profile = profile
        .as_ref()
        .zip(local_app_data.as_ref())
        .map(|(p, l)| crate::guard::same_path(&p.join(r"AppData\Local"), l));

    let mut data = json!({
        "build": os_build(),
        "std_home_dir": std_home.as_deref().map(red),
        "folderid_profile": profile.as_deref().map(red),
        "folderid_local_app_data": local_app_data.as_deref().map(red),
        "folderid_local_app_data_is_profile_appdata_local": local_app_data_in_profile,
        "folderid_roaming_app_data": ffi::folder_roaming_app_data().as_deref().map(red),
        "folderid_program_data": ffi::folder_program_data().as_deref().map(red),
        "get_user_profile_directory": user_profile_directory().as_deref().map(|s| r.redact(s)),
        "std_temp_dir": red(&temp),
        "env": {
            "USERPROFILE": env("USERPROFILE"),
            "HOME": env("HOME"),
            "HOMEDRIVE": env("HOMEDRIVE"),
            "HOMEPATH": env("HOMEPATH"),
            "APPDATA": env("APPDATA"),
            "LOCALAPPDATA": env("LOCALAPPDATA"),
            "TEMP": env("TEMP"),
            "TMP": env("TMP"),
            "CODEX_HOME": env("CODEX_HOME"),
            "CLAUDE_CONFIG_DIR": env("CLAUDE_CONFIG_DIR"),
            "PITBOARD_HOME": env("PITBOARD_HOME"),
            "PATHEXT": env("PATHEXT"),
        },
        "profile_type": profile_type(),
        "path_variable_spellings": path_spellings(),
    });
    if let Some(p) = &profile {
        data["profile_name"] = p
            .file_name()
            .map(|n| hashes::name_facts(&n.to_string_lossy()))
            .unwrap_or(Value::Null);
        data["profile_forms"] = forms(p, &r);
    }
    data["temp_forms"] = forms(&temp, &r);

    let mut hashed = Map::new();
    for (label, dir) in [
        ("codex_home", codex_home),
        ("claude_config_dir", claude_config_dir),
    ] {
        if let Some(dir) = dir {
            hashed.insert(label.into(), hash_candidates(dir, &r));
        }
    }
    data["hash_candidates"] = Value::Object(hashed);
    Report::ok("homes", logon, data)
}

/// The spellings a folder may be named by, redacted, and what differs between them.
fn forms(path: &Path, r: &crate::redact::Redactor) -> Value {
    let long = ffi::long_path(path);
    let short = ffi::short_path(path);
    let canonical = std::fs::canonicalize(path)
        .ok()
        .map(|c| c.display().to_string());
    let leaf = |s: &str| {
        Path::new(s)
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let given_parts = leaf(&path.display().to_string());
    let short_parts = short.as_deref().map(leaf);
    json!({
        "given": r.redact(&path.display().to_string()),
        "long": long.as_deref().map(|s| r.redact(s)),
        "short": short.as_deref().map(|s| r.redact(s)),
        "canonical": canonical.as_deref().map(|s| r.redact(s)),
        "short_differs": short.as_deref().map(|s| !s.eq_ignore_ascii_case(&path.display().to_string())),
        // Which components have an 8.3 alias that differs from their long name, by index,
        // and whether each alias is ASCII; never the alias itself.
        "short_components": short_parts.map(|parts| {
            parts
                .iter()
                .zip(given_parts.iter())
                .enumerate()
                .filter(|(_, (s, g))| !s.eq_ignore_ascii_case(g))
                .map(|(i, (s, _))| json!({ "index": i, "alias_is_ascii": s.is_ascii(), "alias_has_tilde": s.contains('~') }))
                .collect::<Vec<_>>()
        }),
    })
}

/// Every candidate spelling of a home folder and its SHA-256, the spelling redacted.
fn hash_candidates(dir: &Path, r: &crate::redact::Redactor) -> Value {
    let given = dir.display().to_string();
    let canonical = std::fs::canonicalize(dir)
        .ok()
        .map(|c| c.display().to_string());
    let list: Vec<Value> = hashes::candidates(&given, canonical.as_deref())
        .into_iter()
        .map(|c| {
            json!({
                "label": c.label,
                "spelling": r.redact(&c.text),
                "sha256": hashes::sha256_hex(&c.text),
            })
        })
        .collect();
    json!({ "exists": dir.exists(), "candidates": list })
}

/// `RtlGetVersion`'s build, the way the floor check will read it. The UBR is not read: it
/// lives in a registry value the probe did not make, so the runner-facts workflow and the
/// runbook read it beside this.
fn os_build() -> Value {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        dwMajorVersion: 0,
        dwMinorVersion: 0,
        dwBuildNumber: 0,
        dwPlatformId: 0,
        szCSDVersion: [0; 128],
    };
    // SAFETY: `info` is sized and zeroed; RtlGetVersion fills it and always succeeds.
    unsafe {
        windows_sys::Wdk::System::SystemServices::RtlGetVersion(
            (&mut info as *mut OSVERSIONINFOW).cast(),
        );
    }
    json!({
        "major": info.dwMajorVersion,
        "minor": info.dwMinorVersion,
        "build": info.dwBuildNumber,
        "meets_floor_19041": info.dwBuildNumber >= 19041,
        "ubr": "read beside this, from the registry, by the workflow or the owner",
    })
}

fn user_profile_directory() -> Option<String> {
    let token = Token::current().ok()?;
    ffi::user_profile_directory(&token)
        .ok()
        .map(|p| p.display().to_string())
}

/// B3: `GetProfileType`'s flags.
fn profile_type() -> Value {
    let mut flags = 0u32;
    // SAFETY: `flags` is a valid out-param.
    let ok = unsafe { GetProfileType(&mut flags) };
    if ok == 0 {
        return json!({ "error": ffi::last_error() });
    }
    json!({
        "flags": flags,
        "temporary": flags & 1 != 0,
        "roaming": flags & 2 != 0,
        "mandatory": flags & 4 != 0,
        "roaming_preexisting": flags & 8 != 0,
    })
}

/// B4: how this process's environment spells the search-path variable.
fn path_spellings() -> Vec<String> {
    std::env::vars_os()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .filter(|k| k.eq_ignore_ascii_case("path"))
        .collect()
}

/// Which of the two tags a value carries.
fn tag_of(value: Option<OsString>) -> &'static str {
    match value {
        None => "absent",
        Some(v) => {
            let v = v.to_string_lossy();
            if v.ends_with(TAG_A) {
                "a"
            } else if v.ends_with(TAG_B) {
                "b"
            } else {
                "other"
            }
        }
    }
}

/// The child B4 starts: which spellings it got, and which value each lookup returns. Keys
/// never differ only in case (a report may not hold such keys), so the all-capitals `PATH`
/// is `all_caps` and the title-case `Path` is `title_case`.
pub(super) fn env_child() -> Report {
    let logon = logon_now();
    let spellings: Vec<Value> = std::env::vars_os()
        .filter(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case("path"))
        .map(|(k, v)| json!({ "spelling": k.to_string_lossy(), "value": tag_of(Some(v)) }))
        .collect();
    Report::ok(
        "env-child",
        logon,
        json!({
            "spellings": spellings,
            "var_os_all_caps": tag_of(std::env::var_os("PATH")),
            "var_os_title_case": tag_of(std::env::var_os("Path")),
        }),
    )
}

/// B4. `PATH` carries tag `a` and `Path` tag `b` wherever both are set, so a child's answer
/// says which one it got.
pub(super) fn path_vars(scratch: &Path) -> Report {
    let logon = logon_now();
    if let Some(reason) = ffi::refuse_write(scratch) {
        return Report::refused("path-vars", logon, reason);
    }
    let _ = std::fs::create_dir_all(scratch);
    let a = scratch.join(TAG_A);
    let b = scratch.join(TAG_B);
    let exe = std::env::current_exe().unwrap_or_default();
    let pid = std::process::id();
    let mut data = json!({
        "this_process": path_spellings(),
        "tags": { "a": "PATH", "b": "Path" },
    });

    // std's Command, told PATH then Path.
    let out = scratch.join(format!("{}env-std-{pid}.json", crate::PROBE_PREFIX));
    let status = std::process::Command::new(&exe)
        .arg("--out")
        .arg(&out)
        .arg("env-child")
        .env("PATH", &a)
        .env("Path", &b)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    data["std_command_all_caps_then_title_case"] = json!({
        "exit": status.ok().and_then(|s| s.code()),
        "child": take_child_report(&out),
    });

    // A raw environment block holding both names, in each order.
    let raw = |first: (&str, &PathBuf), second: (&str, &PathBuf), stem: &str| -> Value {
        let out = scratch.join(format!("{}env-{stem}-{pid}.json", crate::PROBE_PREFIX));
        let block = env_block(&[first, second]);
        let args: [&OsStr; 3] = [
            OsStr::new("--out"),
            out.as_os_str(),
            OsStr::new("env-child"),
        ];
        match ffi::spawn(None, &exe, &args, CREATE_NO_WINDOW, Some(&block), None) {
            Ok(child) => json!({ "exit": child.wait_ms(30_000), "child": take_child_report(&out) }),
            Err(code) => json!({ "spawn_error": code }),
        }
    };
    data["raw_block_all_caps_first"] = raw(("PATH", &a), ("Path", &b), "raw-caps-first");
    data["raw_block_title_case_first"] = raw(("Path", &b), ("PATH", &a), "raw-title-first");
    Report::ok("path-vars", logon, data)
}

/// This process's environment without any spelling of PATH, then `extra` in order, as a
/// double-NUL-terminated UTF-16 block.
fn env_block(extra: &[(&str, &PathBuf)]) -> Vec<u16> {
    let mut block = Vec::new();
    for (k, v) in std::env::vars_os() {
        if k.to_string_lossy().eq_ignore_ascii_case("path") {
            continue;
        }
        block.extend(k.encode_wide());
        block.push(u16::from(b'='));
        block.extend(v.encode_wide());
        block.push(0);
    }
    for (k, v) in extra {
        block.extend(OsStr::new(k).encode_wide());
        block.push(u16::from(b'='));
        block.extend(v.as_os_str().encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}
