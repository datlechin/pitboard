//! Where Claude Code keeps things, and who it currently thinks is signed in. Read-only.

use crate::context::{Context, Environment};
use crate::error::{Error, Result};
use crate::host::Os;
use crate::host::program::{Lookup, find_among};
use crate::provider::claude::daemon;
use crate::provider::claude::slot;
use serde_json::Value;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// `CLAUDE_CONFIG_DIR ?? ~/.claude`, as Claude Code 2.1.289 reads it: set, even empty, it
/// is the directory, so an empty one is the empty path, which is refused before anything
/// is read under it ([`crate::home::check_absolute`]).
pub(crate) fn config_dir(ctx: &Context) -> PathBuf {
    ctx.claude_config_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".claude"))
}

// As Claude Code chooses it: the register's `config_file_location`.
pub(crate) fn config_file(ctx: &Context) -> PathBuf {
    let file = ctx
        .claude_config_dir
        .as_deref()
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| ctx.home.clone(), PathBuf::from)
        .join(".claude.json");
    let legacy = config_dir(ctx).join(".config.json");
    match std::fs::metadata(&legacy) {
        Ok(meta)
            if meta.is_file()
                && (meta.len() > 0 || (!meta.permissions().readonly() && !file.exists())) =>
        {
            legacy
        }
        _ => file,
    }
}

/// Where the `claude` that signs someone in actually is, if it is anywhere. A bare name is
/// looked up on the context's search path the way a shell would.
pub(crate) fn program(ctx: &Context) -> Option<PathBuf> {
    crate::provider::program_of(ctx, crate::provider::ProviderId::Claude)
}

// The launcher's home is `HOME ?? USERPROFILE` on Windows (`windows_install_layout`).
pub(crate) fn install_places(env: &Environment) -> Vec<PathBuf> {
    let home = match env.os() {
        Os::MacOs | Os::Linux => Some(env.home()),
        Os::Windows => env
            .path("HOME")
            .or_else(|| env.path("USERPROFILE"))
            .map(PathBuf::from),
    };
    home.map(|home| home.join(".local").join("bin"))
        .into_iter()
        .chain(env.os().package_bins(env))
        .collect()
}

// Never by running it: `claude --version` starts a daemon, which writes.
pub(crate) fn installed_version(ctx: &Context) -> Option<String> {
    let found =
        program(ctx).map(|found| crate::host::program::npm_shim_target(&found).unwrap_or(found));
    // Only a `claude.exe` whose version resource names the product `Claude Code`: a shim such
    // as Scoop's shim.exe or a version manager's launcher may carry its own `x.y.z.0` version.
    let is_claude_code = found
        .as_deref()
        .and_then(crate::host::proc::product_name)
        .is_some_and(|name| name == "Claude Code");
    if is_claude_code
        && let Some(release) = found
            .as_deref()
            .and_then(crate::host::proc::product_version)
            .and_then(release_of)
    {
        return Some(release);
    }
    let resolved = found.and_then(|p| std::fs::canonicalize(p).ok());
    if let Some(path) = &resolved
        && let Some(name) = path.file_name().and_then(|n| n.to_str())
        && looks_like_a_version(name)
    {
        return Some(name.to_string());
    }
    // Its parent, not `..`, which a canonical Windows path, `\\?\` and all, does not undo.
    if let Some(dir) = resolved.as_ref().and_then(|p| p.parent())
        && let Some(version) = [Some(dir), dir.parent()]
            .into_iter()
            .flatten()
            .find_map(|dir| version_in_package_json(&dir.join("package.json")))
    {
        return Some(version);
    }
    daemon::read(ctx).and_then(|d| d.version)
}

// Claude Code's reads `<release>.0` (`windows_version_resource`).
fn release_of([major, minor, patch, fourth]: [u16; 4]) -> Option<String> {
    (fourth == 0).then(|| format!("{major}.{minor}.{patch}"))
}

fn looks_like_a_version(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn version_in_package_json(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let name = json.get("name")?.as_str()?;
    if name != "@anthropic-ai/claude-code" {
        return None;
    }
    Some(json.get("version")?.as_str()?.to_string())
}

// The Git Bash Claude Code runs a hook and the status line in on Windows, found as it finds one
// (`windows_statusline_shell`), and `CLAUDE_CODE_GIT_BASH_PATH` where Claude Code passes it over.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GitBash {
    pub(crate) found: Option<PathBuf>,
    pub(crate) passed_over: Option<PathBuf>,
}

const GIT_BASH_INSTALLS: [&str; 2] = [
    r"C:\Program Files\Git\bin\bash.exe",
    r"C:\Program Files (x86)\Git\bin\bash.exe",
];

pub(crate) fn git_bash(ctx: &Context) -> GitBash {
    git_bash_among(
        ctx.git_bash.as_deref(),
        &ctx.search_path(),
        &ctx.lookup(),
        |path| path.exists(),
    )
}

fn git_bash_among(
    named: Option<&Path>,
    search: &OsStr,
    lookup: &Lookup,
    exists: impl Fn(&Path) -> bool,
) -> GitBash {
    let takes = |named: &Path| {
        let name = named.to_string_lossy().to_lowercase();
        let base = name
            .trim_end_matches(['\\', '/'])
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or_default();
        ["bash.exe", "sh.exe", "bash", "sh"].contains(&base) && exists(named)
    };
    if let Some(named) = named.filter(|named| takes(named)) {
        return GitBash {
            found: Some(named.to_path_buf()),
            passed_over: None,
        };
    }
    // `where.exe git`'s first, its folder's parent's `bin`.
    let beside_git = || {
        let git = find_among(
            Path::new("git"),
            std::env::split_paths(search),
            lookup,
            &exists,
        )?;
        let bash = git.parent()?.parent()?.join("bin").join("bash.exe");
        exists(&bash).then_some(bash)
    };
    GitBash {
        found: GIT_BASH_INSTALLS
            .into_iter()
            .map(PathBuf::from)
            .find(|bash| exists(bash))
            .or_else(beside_git),
        passed_over: named.map(Path::to_path_buf),
    }
}

// The PowerShell Claude Code runs a hook and the status line in on Windows where it finds no
// Git Bash, found as it finds one (`windows_statusline_powershell`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PowerShell {
    Seven(PathBuf),
    Windows(PathBuf),
}

// Where Claude Code looks past `PATH`, as its environment names them.
pub(crate) struct PowerShellPlaces {
    pub(crate) program_files: Option<PathBuf>,
    pub(crate) local_app_data: Option<PathBuf>,
    pub(crate) user_profile: Option<PathBuf>,
    pub(crate) system_root: Option<PathBuf>,
}

pub(crate) fn powershell(ctx: &Context) -> Option<PowerShell> {
    let places = PowerShellPlaces {
        program_files: ctx.program_files.clone(),
        local_app_data: ctx.local_app_data.clone(),
        user_profile: Some(ctx.home.clone()),
        system_root: ctx.system_root.clone(),
    };
    powershell_among(
        &ctx.search_path(),
        &ctx.lookup(),
        &places,
        crate::host::proc::can_run,
    )
}

fn powershell_among(
    search: &OsStr,
    lookup: &Lookup,
    places: &PowerShellPlaces,
    exists: impl Fn(&Path) -> bool,
) -> Option<PowerShell> {
    let on_path = |name: &str| {
        find_among(
            Path::new(name),
            std::env::split_paths(search),
            lookup,
            &exists,
        )
    };
    let under = |folder: &Option<PathBuf>, parts: &[&str]| {
        let mut path = folder.clone()?;
        path.extend(parts);
        exists(&path).then_some(path)
    };
    let system_root = Some(
        places
            .system_root
            .clone()
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows")),
    );
    let found = on_path("pwsh")
        .or_else(|| under(&places.program_files, &["PowerShell", "7", "pwsh.exe"]))
        .or_else(|| {
            under(
                &places.local_app_data,
                &["Microsoft", "WindowsApps", "pwsh.exe"],
            )
        })
        .or_else(|| under(&places.user_profile, &[".dotnet", "tools", "pwsh.exe"]))
        .or_else(|| on_path("powershell"))
        .or_else(|| {
            under(
                &system_root,
                &["System32", "WindowsPowerShell", "v1.0", "powershell.exe"],
            )
        })?;
    let seven = found
        .file_stem()
        .is_some_and(|stem| stem.eq_ignore_ascii_case("pwsh"));
    Some(if seven {
        PowerShell::Seven(found)
    } else {
        PowerShell::Windows(found)
    })
}

/// The directory whose path string selects the credential slot.
pub(crate) fn storage_dir(ctx: &Context) -> String {
    storage_dir_from(
        ctx.secure_storage_dir.as_deref(),
        &ctx.home,
        &config_dir(ctx).to_string_lossy(),
    )
}

/// What the lock a token refresh of the login takes first guards, as [`crate::lock::acquire`]
/// takes it: `.oauth_refresh` in the storage directory, locked as `.oauth_refresh.lock`
/// (the register's `refresh_lock`).
pub(crate) fn refresh_lock(ctx: &Context) -> PathBuf {
    PathBuf::from(storage_dir(ctx)).join(".oauth_refresh")
}

// The second lock a refresh takes, on the storage directory as Bun's realpath spells it.
pub(crate) fn legacy_refresh_lock(ctx: &Context) -> PathBuf {
    let dir = PathBuf::from(storage_dir(ctx));
    std::fs::canonicalize(&dir).map_or(dir, crate::host::without_verbatim_prefix)
}

// `CLAUDE_SECURESTORAGE_CONFIG_DIR`, empty as the home's `.claude`, else the config folder,
// each in NFC as Claude Code's `Lx` and `Pe` normalise them.
fn storage_dir_from(secure: Option<&str>, home: &Path, config_dir: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    match secure {
        Some("") => home.join(".claude").to_string_lossy().nfc().collect(),
        Some(v) => v.nfc().collect(),
        None => config_dir.nfc().collect(),
    }
}

/// Whether this process reads the unsuffixed slot. An empty
/// `CLAUDE_SECURESTORAGE_CONFIG_DIR` pins it even when `CLAUDE_CONFIG_DIR` is set, and
/// without one, an empty `CLAUDE_CONFIG_DIR` names it, since Claude Code tests
/// `!CLAUDE_CONFIG_DIR`.
pub(crate) fn is_default_slot(ctx: &Context) -> bool {
    is_default_slot_from(
        ctx.secure_storage_dir.as_deref(),
        ctx.claude_config_dir.as_deref(),
    )
}

fn is_default_slot_from(secure: Option<&str>, config_dir: Option<&str>) -> bool {
    match secure {
        Some(v) => v.is_empty(),
        None => config_dir.is_none_or(str::is_empty),
    }
}

/// The keychain service this process would read.
#[cfg_attr(not(feature = "test-support"), allow(unreachable_pub))]
pub fn live_service(ctx: &Context) -> String {
    if is_default_slot(ctx) {
        slot::LIVE_SERVICE.to_string()
    } else {
        slot::service_for_dir(&storage_dir(ctx))
    }
}

// As Claude Code parses its config and its settings: one U+FEFF at the start dropped
// (`settings_byte_order_mark`).
pub(crate) fn without_byte_order_mark(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

pub(crate) fn load_config(ctx: &Context) -> Result<Value> {
    let path = config_file(ctx);
    let raw = std::fs::read_to_string(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::ClaudeConfigMissing {
                path: path.clone(),
                spelling: crate::words::spelled(ctx, crate::provider::ProviderId::Claude),
            }
        } else {
            Error::ClaudeConfigUnreadable {
                path: path.clone(),
                source,
            }
        }
    })?;
    serde_json::from_str(without_byte_order_mark(&raw))
        .map_err(|source| Error::ClaudeConfigNotJson { path, source })
}

/// The account Claude Code's config names, which can be another than the login stored's: a
/// Claude Code process started or signed in on another login can write its own there (the
/// register's `config_identity_is_the_last_writers`). Only Anthropic says whose a login is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConfigAccount {
    pub(crate) email: String,
    pub(crate) account_uuid: String,
    pub(crate) organization_uuid: String,
    pub(crate) organization_name: Option<String>,
}

/// The three organization fields are written by separate fetches, not by the profile save,
/// so a config that has never made them is normal and they stay `None`.
pub(crate) fn config_account(config: &Value) -> Option<ConfigAccount> {
    let o = config.get("oauthAccount")?.as_object()?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_owned);
    Some(ConfigAccount {
        email: s("emailAddress")?,
        account_uuid: s("accountUuid")?,
        organization_uuid: s("organizationUuid").unwrap_or_default(),
        organization_name: s("organizationName"),
    })
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    // Bun 1.4.3's realpath is libuv's, which takes `\\?\` off what Windows gives.
    #[test]
    fn the_legacy_lock_is_named_as_bun_spells_its_folder() {
        let root =
            std::env::temp_dir().join(format!("pitboard-legacy-lock-{}", std::process::id()));
        std::fs::create_dir_all(root.join("claude")).expect("a scratch config dir");
        let ctx = Context::new(root.clone())
            .with_claude_config_dir(root.join(".").join("claude").to_string_lossy().into());
        let lock = legacy_refresh_lock(&ctx);
        let canonical = std::fs::canonicalize(root.join("claude")).expect("there");
        assert_eq!(lock, crate::host::without_verbatim_prefix(canonical));
        assert!(
            !lock.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            lock.display()
        );
        assert!(lock.is_absolute());
        let missing = root.join("never-made");
        let ctx = ctx.with_claude_config_dir(missing.to_string_lossy().into());
        assert_eq!(
            legacy_refresh_lock(&ctx),
            missing,
            "as given where nothing is there"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_program_is_found_on_path_and_a_missing_one_is_not() {
        let ctx = Context::new(PathBuf::from("/home/x"));
        let (everywhere, file) = match crate::host::OS {
            Os::MacOs | Os::Linux => ("ls", "ls"),
            Os::Windows => ("cmd", "cmd.exe"),
        };
        let found = program(&ctx.clone().with_claude_program(everywhere.into()))
            .expect("every machine that runs these tests has it on PATH");
        assert!(found.ends_with(file), "{found:?}");
        assert!(found.is_absolute(), "a shell would get an absolute path");
        assert!(std::fs::metadata(&found).is_ok());
        assert_eq!(
            program(
                &ctx.clone()
                    .with_claude_program("no-such-program-anywhere".into())
            ),
            None
        );
        assert_eq!(
            program(&ctx.with_claude_program("/nowhere/at/all/claude".into())),
            None
        );
    }

    #[test]
    fn a_version_resource_names_the_release_it_ends_in_zero() {
        assert_eq!(release_of([2, 1, 300, 0]).as_deref(), Some("2.1.300"));
        assert_eq!(release_of([10, 0, 26100, 1]), None);
    }

    #[test]
    fn npms_shim_is_followed_to_the_program_it_starts() {
        let prefix = tempfile::tempdir().expect("a scratch npm prefix");
        let package = prefix
            .path()
            .join("node_modules")
            .join("@anthropic-ai")
            .join("claude-code");
        std::fs::create_dir_all(package.join("bin")).expect("its bin");
        std::fs::write(package.join("bin").join("claude.exe"), "").expect("a program");
        crate::host::fs::testing::make_runnable(&package.join("bin").join("claude.exe"))
            .expect("runnable");
        std::fs::write(
            package.join("package.json"),
            r#"{"name": "@anthropic-ai/claude-code", "version": "9.9.7"}"#,
        )
        .expect("its package.json");
        let shim = prefix.path().join("claude.cmd");
        std::fs::write(
            &shim,
            "@ECHO off\r\nCALL :find_dp0\r\n\
             \"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe\"   %*\r\n",
        )
        .expect("a shim");
        crate::host::fs::testing::make_runnable(&shim).expect("runnable");
        let ctx = Context::new(prefix.path().to_path_buf()).with_claude_program(shim);
        assert_eq!(installed_version(&ctx).as_deref(), Some("9.9.7"));
    }

    #[test]
    fn an_empty_storage_dir_pins_the_default_slot_even_with_a_config_dir() {
        assert!(is_default_slot_from(None, None));
        assert!(!is_default_slot_from(None, Some("/cfg")));
        assert!(is_default_slot_from(Some(""), Some("/cfg")));
        assert!(!is_default_slot_from(Some("/elsewhere"), None));
        assert!(
            is_default_slot_from(None, Some("")),
            "Claude Code tests !CLAUDE_CONFIG_DIR"
        );
    }

    /// Claude Code 2.1.289 reads `CLAUDE_CONFIG_DIR` two ways: `??` for its config dir, so
    /// an empty one is the empty path, and `||` for its config file's base, so an empty one
    /// is the home. Both are read here as it reads them, though a context holding an empty
    /// one is refused before either is used.
    #[test]
    fn an_empty_config_dir_is_read_as_claude_code_reads_it() {
        let ctx =
            Context::new(PathBuf::from("/nowhere/home")).with_claude_config_dir(String::new());
        assert_eq!(config_dir(&ctx), PathBuf::new());
        assert_eq!(config_file(&ctx), Path::new("/nowhere/home/.claude.json"));
        assert!(is_default_slot(&ctx));
    }

    fn scratch_home() -> (tempfile::TempDir, Context, PathBuf) {
        let home = tempfile::tempdir().expect("a scratch home");
        std::fs::create_dir(home.path().join(".claude")).expect("a config dir");
        let ctx = Context::new(home.path().to_path_buf());
        let legacy = home.path().join(".claude/.config.json");
        (home, ctx, legacy)
    }

    #[test]
    fn an_empty_config_json_gives_way_to_claude_json() {
        let (home, ctx, legacy) = scratch_home();
        std::fs::write(&legacy, "").unwrap();
        std::fs::write(home.path().join(".claude.json"), "{}").unwrap();
        assert_eq!(config_file(&ctx), home.path().join(".claude.json"));
    }

    #[test]
    fn an_empty_config_json_is_taken_only_while_writable_and_claude_json_is_missing() {
        let (home, ctx, legacy) = scratch_home();
        std::fs::write(&legacy, "").unwrap();
        assert_eq!(config_file(&ctx), legacy);

        let mut read_only = std::fs::metadata(&legacy).unwrap().permissions();
        read_only.set_readonly(true);
        std::fs::set_permissions(&legacy, read_only).unwrap();
        assert_eq!(config_file(&ctx), home.path().join(".claude.json"));
    }

    #[test]
    fn a_config_json_wins_when_it_is_a_file_that_holds_anything() {
        let (home, ctx, legacy) = scratch_home();
        std::fs::write(home.path().join(".claude.json"), "{}").unwrap();
        std::fs::write(&legacy, "{}").unwrap();
        assert_eq!(config_file(&ctx), legacy);

        std::fs::remove_file(&legacy).unwrap();
        std::fs::create_dir(&legacy).unwrap();
        assert_eq!(config_file(&ctx), home.path().join(".claude.json"));
    }

    // Claude Code: `if (n !== undefined) return (n || join(homedir(), ".claude"))`. The empty
    // string would make every credential and lock path relative to the working directory.
    #[test]
    fn an_empty_storage_dir_means_the_default_directory_not_the_current_one() {
        let home = Path::new(match crate::host::OS {
            Os::MacOs | Os::Linux => "/home/x",
            Os::Windows => r"C:\Users\x",
        });
        assert_eq!(
            storage_dir_from(Some(""), home, "/cfg"),
            home.join(".claude").to_string_lossy()
        );
        assert_eq!(
            storage_dir_from(Some("/elsewhere"), home, "/cfg"),
            "/elsewhere"
        );
        assert_eq!(storage_dir_from(None, home, "/cfg"), "/cfg");
    }

    #[test]
    fn the_storage_folder_is_the_config_folder_in_nfc() {
        let home = Path::new("/home/x");
        assert_eq!(storage_dir_from(None, home, "/Cafe\u{301}"), "/Caf\u{e9}");
    }

    #[test]
    fn a_config_that_starts_with_a_byte_order_mark_is_read() {
        let root = tempfile::tempdir().expect("a scratch home");
        let ctx = Context::new(root.path().to_path_buf());
        let mut bytes = "\u{feff}".as_bytes().to_vec();
        bytes.extend(br#"{"oauthAccount":{"accountUuid":"a","emailAddress":"a@b.c"}}"#);
        std::fs::write(root.path().join(".claude.json"), bytes).expect("written");
        let config = load_config(&ctx).expect("read past the mark");
        assert_eq!(
            config_account(&config).map(|account| account.email),
            Some("a@b.c".to_string())
        );
    }

    #[test]
    fn the_config_account_needs_an_email_and_an_account() {
        let full = serde_json::json!({"oauthAccount": {
            "emailAddress": "a@b.c", "accountUuid": "u", "organizationUuid": "o",
            "organizationName": "Org", "organizationType": "claude_max",
            "organizationRateLimitTier": "default_claude_max_20x"}});
        let id = config_account(&full).expect("should parse");
        assert_eq!(id.email, "a@b.c");
        assert_eq!(id.organization_name.as_deref(), Some("Org"));

        assert!(config_account(&serde_json::json!({})).is_none());
        assert!(
            config_account(&serde_json::json!({"oauthAccount": {"accountUuid": "u"}})).is_none()
        );
    }

    /// What 2.1.278 actually writes when it saves a profile: the organization fields come
    /// from other fetches and are often absent. An account is still identified.
    #[test]
    fn the_config_account_survives_a_config_with_no_organization_fields() {
        let written = serde_json::json!({"oauthAccount": {
            "accountUuid": "u", "emailAddress": "a@b.c", "organizationUuid": "o",
            "billingType": "subscription", "seatTier": "max_20x"}});
        let id = config_account(&written).expect("should parse");
        assert_eq!(id.organization_uuid, "o");
        assert_eq!(id.organization_name, None);
    }

    // As 2.1.301's `x()` finds it, the variable's file taken only where its name is bash or sh.
    #[test]
    fn git_bash_is_found_where_claude_code_finds_it() {
        let windows = Lookup::new(Os::Windows, None);
        let git = std::env::temp_dir().join("Git");
        let search = std::env::join_paths([git.join("cmd")]).expect("a search path");
        let beside_git = git.join("bin").join("bash.exe");
        let named = |path: &str| Some(PathBuf::from(path));
        let find = |variable: Option<PathBuf>, there: &[&Path]| {
            git_bash_among(variable.as_deref(), &search, &windows, |path| {
                there.contains(&path)
            })
        };
        let installed = Path::new(GIT_BASH_INSTALLS[0]);
        let x86 = Path::new(GIT_BASH_INSTALLS[1]);
        let mine = Path::new(r"D:\tools\Bash.EXE");
        assert_eq!(
            find(named(r"D:\tools\Bash.EXE"), &[mine, installed]),
            GitBash {
                found: Some(mine.to_path_buf()),
                passed_over: None
            }
        );
        for (variable, there) in [
            (
                r"D:\tools\git.exe",
                vec![Path::new(r"D:\tools\git.exe"), installed],
            ),
            (r"D:\tools\bash.exe", vec![installed]),
        ] {
            assert_eq!(
                find(named(variable), &there),
                GitBash {
                    found: Some(installed.to_path_buf()),
                    passed_over: named(variable)
                },
                "{variable}"
            );
        }
        assert_eq!(find(None, &[x86]).found.as_deref(), Some(x86));
        let git_exe = git.join("cmd").join("git.exe");
        assert_eq!(
            find(None, &[&git_exe, &beside_git]).found,
            Some(beside_git.clone())
        );
        assert_eq!(find(None, &[&git_exe]).found, None, "no bash beside git");
        assert_eq!(find(None, &[]).found, None);
    }

    // As 2.1.302's `pw()` finds it. On the session's PC (2026-10-18) Claude Code ran the status
    // line in powershell.exe, then in pwsh.exe once PowerShell 7 was installed.
    #[test]
    fn powershell_is_found_where_claude_code_finds_it() {
        let windows = Lookup::new(Os::Windows, None);
        let bin = std::env::temp_dir().join("bin");
        let search = std::env::join_paths([bin.clone()]).expect("a search path");
        let (program_files, local, profile) = (
            PathBuf::from(r"C:\Program Files"),
            PathBuf::from(r"C:\Users\x\AppData\Local"),
            PathBuf::from(r"C:\Users\x"),
        );
        let places = PowerShellPlaces {
            program_files: Some(program_files.clone()),
            local_app_data: Some(local.clone()),
            user_profile: Some(profile.clone()),
            system_root: None,
        };
        let find = |there: &[&PathBuf]| {
            powershell_among(&search, &windows, &places, |path| {
                there.iter().any(|one| one.as_path() == path)
            })
        };
        let under = |folder: &PathBuf, parts: &[&str]| {
            let mut path = folder.clone();
            path.extend(parts);
            path
        };
        let on_path = bin.join("pwsh.exe");
        let seven = under(&program_files, &["PowerShell", "7", "pwsh.exe"]);
        let alias = under(&local, &["Microsoft", "WindowsApps", "pwsh.exe"]);
        let dotnet = under(&profile, &[".dotnet", "tools", "pwsh.exe"]);
        let five_on_path = bin.join("powershell.exe");
        let five = under(
            &PathBuf::from(r"C:\Windows"),
            &["System32", "WindowsPowerShell", "v1.0", "powershell.exe"],
        );
        assert_eq!(
            find(&[&on_path, &seven, &five]),
            Some(PowerShell::Seven(on_path.clone()))
        );
        assert_eq!(
            find(&[&seven, &alias, &five_on_path]),
            Some(PowerShell::Seven(seven.clone()))
        );
        assert_eq!(
            find(&[&alias, &dotnet]),
            Some(PowerShell::Seven(alias.clone()))
        );
        assert_eq!(
            find(&[&dotnet, &five]),
            Some(PowerShell::Seven(dotnet.clone()))
        );
        assert_eq!(
            find(&[&five_on_path, &five]),
            Some(PowerShell::Windows(five_on_path.clone()))
        );
        assert_eq!(find(&[&five]), Some(PowerShell::Windows(five)));
        assert_eq!(find(&[]), None);
    }
}
