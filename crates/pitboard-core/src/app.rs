//! What an app has to find for itself that a command typed at a prompt is given.
//!
//! The command line is started from a shell, so its environment has the person's `PATH`, and
//! each tool's program is looked for there when it runs. An app the system started has
//! launchd's environment instead, whose `PATH` holds the system's directories and nothing a
//! version manager or an npm prefix added. So an app asks the person's login shell for its
//! `PATH`, which is the host's to ask, looks for each tool's program there and then where
//! the tool's own installers put it, and hands the shell's `PATH` to every sign-in. The rest
//! of its environment is read as the command line reads its own, by the same code.
//!
//! An app also says which `pitboard` a terminal would run, and whether it is the app's own,
//! and keeps files of its own in Pitboard's directory, beside the core's, written as the core
//! writes its own.

use crate::context::{Context, Environment};
use crate::error::{Error, Result};
use crate::host::{self, LoginPath, OS, Os};
use crate::provider::ProviderId;
use crate::service::Permit;
use crate::{atomic, home};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// An app's context, and what building it found.
#[derive(Debug)]
#[non_exhaustive]
pub struct AppContext {
    pub context: Context,
    /// The login shell's `PATH`, as far as it is looked in: where each tool's program is
    /// looked for, and what a sign-in is given. `None` where the shell could not be asked,
    /// and the app looks on its own `PATH`, as it always did.
    pub search_path: Option<String>,
    /// The login shell had not answered in time, so tools are looked for on this app's own
    /// `PATH` until it is asked again.
    pub late: bool,
    // Looked in after the login shell's `PATH` for the `pitboard` a terminal would run.
    pub command_line_places: Vec<PathBuf>,
}

impl AppContext {
    /// The context of an app started with `env`, which is at `app`.
    ///
    /// Asks the person's login shell for its `PATH`, which can take seconds, so this is never
    /// called on an app's main thread.
    pub fn discover(env: &Environment, app: Option<&Path>) -> AppContext {
        AppContext::read(env, app, host::login_path(env))
    }

    // A tool's program is looked for when it is needed: the one its variable names, or the
    // first on the login shell's `PATH`, then where the tool's installers put it.
    pub(crate) fn read(env: &Environment, app: Option<&Path>, said: LoginPath) -> AppContext {
        let mut context = Context::read(env, "app");
        let (shell, late) = match said {
            LoginPath::Said(path) => (Some(path), false),
            LoginPath::Unknown => (None, false),
            LoginPath::Late => (None, true),
        };
        // A relative entry would be looked in wherever this app happens to be running, and
        // looking inside a folder the system guards asks the person whether Pitboard may, for
        // something it never needed to read.
        let looked: Vec<PathBuf> = shell
            .as_deref()
            .map(|path| {
                std::env::split_paths(path)
                    .filter(|dir| dir.is_absolute() && !guarded(env.os(), dir, &context.home))
                    .collect()
            })
            .unwrap_or_default();
        let search_path = shell.map(|_| {
            std::env::join_paths(&looked)
                .map(|joined| joined.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        if let Some(path) = &search_path {
            context.search_path = Some(path.into());
        }
        context.installer_places = ProviderId::ALL
            .iter()
            .filter_map(|&tool| Some((tool, (tool.cli()?.install_places)(env))))
            .collect();
        context.schedule_program = app.and_then(|app| env.os().app_command_line(app));
        let command_line_places = command_line_places(env, app);
        AppContext {
            context,
            search_path,
            late,
            command_line_places,
        }
    }
}

fn guarded(os: Os, dir: &Path, home: &Path) -> bool {
    os.guarded_folders()
        .iter()
        .any(|folder| dir.starts_with(home.join(folder)))
}

/// The command line an app at `app` comes with, which its renewal schedule runs. `None` for
/// anything that is not an app, such as a test or a build directory.
pub fn app_command_line(app: &Path) -> Option<PathBuf> {
    OS.app_command_line(app)
}

// So an app has no rule of its own for what it would run or link to.
pub fn can_run(path: &Path) -> bool {
    host::proc::can_run(path)
}

/// The `pitboard` a terminal would run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLine {
    /// The app's own, at this path or linked to from it.
    Bundled(PathBuf),
    /// Another install, at this path.
    Another(PathBuf),
    /// None anywhere a terminal would look.
    Nowhere,
}

// On a Mac `/usr/local/bin` is also where Homebrew and the app itself link the app's own.
pub fn command_line_places(env: &Environment, app: Option<&Path>) -> Vec<PathBuf> {
    let os = env.os();
    let own: Vec<PathBuf> = match os {
        Os::MacOs | Os::Linux => {
            let home = env.home();
            vec![
                home.join(".cargo").join("bin"),
                home.join(".local").join("bin"),
            ]
        }
        Os::Windows => env
            .named_folder("USERPROFILE")
            .map(|home| home.join(".cargo").join("bin"))
            .into_iter()
            .chain(
                app.and_then(|app| os.app_command_line(app))
                    .and_then(|helper| helper.parent().map(Path::to_path_buf)),
            )
            .collect(),
    };
    own.into_iter().chain(os.package_bins(env)).collect()
}

// The app's own where, every link followed, it is the command line at `helper`.
pub fn find_command_line(
    search_path: Option<&OsStr>,
    places: &[PathBuf],
    helper: Option<&Path>,
) -> CommandLine {
    let dirs = search_path
        .map(|path| std::env::split_paths(path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(places.iter().cloned());
    // Pitboard installs only as `pitboard.exe` on Windows, so `PATHEXT` cannot change which.
    let lookup = host::program::Lookup::here();
    let Some(found) =
        host::program::find_among(Path::new("pitboard"), dirs, &lookup, host::proc::can_run)
    else {
        return CommandLine::Nowhere;
    };
    let own = helper.and_then(|helper| std::fs::canonicalize(helper).ok());
    match own {
        Some(own)
            if std::fs::canonicalize(&found)
                .is_ok_and(|resolved| OS.same_path(&resolved, &own)) =>
        {
            CommandLine::Bundled(found)
        }
        _ => CommandLine::Another(found),
    }
}

/// A file an app keeps of its own in Pitboard's directory: what it remembers between launches
/// that is the app's and no command's. The core reads none of them for itself, and nothing in
/// one is secret, but each is written private to its owner, as everything there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppFile {
    /// The app's own preferences, such as which tools somebody keeps one account of.
    Preferences,
    /// What the app has told a person about, once for each reset of a limit, so a run-out
    /// told before the app was quit is not told again after it is opened.
    Told,
}

impl AppFile {
    /// Its name in Pitboard's directory.
    pub(crate) fn name(self) -> &'static str {
        match self {
            AppFile::Preferences => "app.json",
            AppFile::Told => "told.json",
        }
    }
}

// Held by an app for as long as it runs, as the warm-up holds `warm.lock`: the app writes
// Pitboard's directory again after `uninstall` took it away, so `uninstall` refuses while one
// holds it.
#[derive(Debug)]
pub struct Open {
    _lock: std::fs::File,
}

impl Open {
    fn path(ctx: &Context) -> PathBuf {
        home::dir(ctx).join("app.lock")
    }

    // None where it cannot be held: another app holds it, or the directory cannot be made.
    pub(crate) fn hold(ctx: &Context, permit: Permit) -> Option<Open> {
        home::ensure(ctx, permit).ok()?;
        let lock = host::fs::open_private_lock(permit, &Open::path(ctx)).ok()?;
        lock.try_lock().ok()?;
        Some(Open { _lock: lock })
    }

    pub(crate) fn is_held(ctx: &Context) -> bool {
        std::fs::File::open(Open::path(ctx))
            .is_ok_and(|lock| matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock)))
    }
}

/// What the app keeps in `file`, or `None` where it keeps nothing there. A file that is there
/// and cannot be read, because it is not this user's to read, a disk failed, another program
/// holds it or it is not text, is an error and not nothing: an app that took it as nothing
/// would write what it has over what it never read.
pub(crate) fn read_app_file(ctx: &Context, file: AppFile) -> std::io::Result<Option<String>> {
    read_file(&home::dir(ctx).join(file.name()))
}

/// What an app keeps in the file at `path`, wherever that is, or `None` where nothing is
/// there, by the rule of `read_app_file`: a file that is there and cannot be read is an
/// error, not nothing.
pub fn read_file(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Keeps `body` in the file at `path`, outside Pitboard's directory, as the core writes its
/// own: the directories it needs made private first where they are not there yet, and the
/// file written whole or not at all, private to its owner. For what an app keeps of its own
/// that is the app's whichever Pitboard directory it serves, such as the account windows'
/// records, which belong with the app's web stores.
///
/// It takes the [`Permit`] only the one gate every change passes makes, which an app asks for
/// with [`crate::service::Pitboard::permit`], so an app writes nothing of its own either where
/// it runs as root or under sudo.
pub fn write_file(permit: Permit, path: &Path, body: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        host::fs::create_private_dir(permit, dir)?;
    }
    atomic::write(permit, path, body.as_bytes(), atomic::Perms::Secret)
}

/// Keeps `body` in `file`, as the core writes its own: Pitboard's directory made private
/// first where it is not there yet, and the file written whole or not at all, private to its
/// owner.
pub(crate) fn write_app_file(
    ctx: &Context,
    permit: Permit,
    file: AppFile,
    body: &str,
) -> Result<()> {
    let path = home::dir(ctx).join(file.name());
    let unwritable = |source| Error::HomeUnwritable {
        path: path.clone(),
        source,
    };
    home::ensure(ctx, permit).map_err(unwritable)?;
    atomic::write(permit, &path, body.as_bytes(), atomic::Perms::Secret).map_err(unwritable)
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::host::Os;
    use crate::host::fs::testing;
    use crate::provider::{ProviderId, installed_among, program_among};
    use std::cell::RefCell;

    fn env(pairs: &[(&str, &str)]) -> Environment {
        pairs.iter().copied().collect()
    }

    fn said(path: &str) -> LoginPath {
        LoginPath::Said(path.into())
    }

    fn full(path: &str) -> PathBuf {
        match OS {
            Os::MacOs | Os::Linux => PathBuf::from(path),
            Os::Windows => PathBuf::from(format!("C:{}", path.replace('/', "\\"))),
        }
    }

    fn file_of(program: &str) -> String {
        format!("{program}{}", std::env::consts::EXE_SUFFIX)
    }

    fn program(dir: &str, name: &str) -> PathBuf {
        full(dir).join(file_of(name))
    }

    fn search(entries: &[&str]) -> String {
        let entries = entries.iter().map(|entry| {
            if entry.starts_with('/') {
                full(entry)
            } else {
                PathBuf::from(entry)
            }
        });
        let joined = std::env::join_paths(entries).expect("a search path");
        joined.to_string_lossy().into_owned()
    }

    // Home is `/Users/x`, with its local app data on Windows.
    fn at_home(more: &[(&str, &str)]) -> Environment {
        let home = full("/Users/x").to_string_lossy().into_owned();
        let local = full("/Users/x/AppData/Local")
            .to_string_lossy()
            .into_owned();
        let mut pairs = vec![("HOME", home.as_str())];
        if OS == Os::Windows {
            pairs.extend([
                ("USERPROFILE", home.as_str()),
                ("LOCALAPPDATA", local.as_str()),
            ]);
        }
        pairs.extend(more.iter().copied());
        env(&pairs)
    }

    fn installed(env: &Environment, tool: ProviderId) -> PathBuf {
        let cli = tool.cli().expect("a command line");
        (cli.install_places)(env)[0].join(file_of(tool.program()))
    }

    #[test]
    fn a_program_is_found_by_its_variable_then_the_login_shell_then_its_installer() {
        let home = at_home(&[]);
        let elsewhere = program("/elsewhere", "codex");
        let here: Vec<PathBuf> = vec![
            program("/Users/x/.nvm/versions/node/v22.1.0/bin", "codex"),
            program("/Users/x/.volta/bin", "claude"),
            installed(&home, ProviderId::Codex),
            installed(&home, ProviderId::Claude),
            elsewhere.clone(),
        ];
        let runnable = |path: &Path| here.iter().any(|there| there == path);
        let found = |read: &AppContext, tool| program_among(&read.context, tool, runnable);
        let login = search(&[
            "/usr/bin",
            "bin",
            "/Users/x/.volta/bin",
            "/Users/x/.nvm/versions/node/v22.1.0/bin",
        ]);

        let shell = AppContext::read(&home, None, said(&login));
        assert_eq!(
            found(&shell, ProviderId::Codex),
            Some(program("/Users/x/.nvm/versions/node/v22.1.0/bin", "codex"))
        );
        assert_eq!(
            found(&shell, ProviderId::Claude),
            Some(program("/Users/x/.volta/bin", "claude"))
        );
        assert_eq!(
            installed_among(&shell.context, runnable),
            [ProviderId::Claude, ProviderId::Codex]
        );
        assert_eq!(
            shell.command_line_places,
            command_line_places(&home, None),
            "a terminal's installs are looked for under the app's own home"
        );
        let looked = search(&[
            "/usr/bin",
            "/Users/x/.volta/bin",
            "/Users/x/.nvm/versions/node/v22.1.0/bin",
        ]);
        assert_eq!(
            shell.search_path.as_deref(),
            Some(looked.as_str()),
            "the core looks where the app looks"
        );
        assert_eq!(
            shell.context.search_path(),
            looked.as_str(),
            "and a sign-in is given it"
        );

        let named = |codex: &Path| {
            let codex = codex.to_string_lossy();
            AppContext::read(&at_home(&[("PITBOARD_CODEX", &codex)]), None, said(&login))
        };
        assert_eq!(
            found(&named(&elsewhere), ProviderId::Codex),
            Some(elsewhere.clone())
        );
        assert_eq!(
            found(&named(&program("/gone", "codex")), ProviderId::Codex),
            None,
            "a program named outright is found only where it is"
        );

        let empty = AppContext::read(
            &at_home(&[("PITBOARD_CODEX", "")]),
            None,
            said(&search(&["/opt/tools/bin"])),
        );
        assert_eq!(
            program_among(&empty.context, ProviderId::Codex, |path| {
                path == program("/opt/tools/bin", "codex")
            }),
            Some(program("/opt/tools/bin", "codex")),
            "an empty name names nothing, and the app looks as it does without one"
        );

        let unknown = AppContext::read(&home, None, LoginPath::Unknown);
        assert_eq!(
            found(&unknown, ProviderId::Codex),
            Some(installed(&home, ProviderId::Codex))
        );
        assert_eq!(
            found(&unknown, ProviderId::Claude),
            Some(installed(&home, ProviderId::Claude))
        );
        assert_eq!(
            unknown.search_path, None,
            "the app looks where it always did"
        );
        assert!(!unknown.late);

        assert!(installed_among(&shell.context, |_| false).is_empty());

        let late = AppContext::read(&home, None, LoginPath::Late);
        assert!(late.late, "a late shell says so, for the app to ask again");
        assert_eq!(late.search_path, None);
    }

    // A program installed after the app started is found the next time it is looked for.
    #[test]
    fn a_program_installed_since_the_app_started_is_found() {
        let folder = tempfile::tempdir().expect("a scratch folder");
        let scratch = folder.path().join("scratch");
        let home = scratch.to_string_lossy().into_owned();
        let given = env(&[
            ("HOME", &home),
            ("USERPROFILE", &home),
            ("LOCALAPPDATA", &home),
        ]);
        let read = AppContext::read(&given, None, said(&search(&["/nowhere"])));
        let pitboard = crate::service::Pitboard::new(read.context);
        assert!(!pitboard.installed().contains(&ProviderId::Codex));

        let codex = installed(&given, ProviderId::Codex);
        std::fs::create_dir_all(codex.parent().expect("its folder")).unwrap();
        std::fs::write(&codex, "#!/bin/sh\n").unwrap();
        testing::make_runnable(&codex).expect("runnable");
        assert!(pitboard.installed().contains(&ProviderId::Codex));
    }

    // On a volume that ignores case `Claude` would find Claude Code's `claude`.
    #[test]
    fn a_tool_with_no_command_line_is_never_looked_for_on_the_search_path() {
        let looked = RefCell::new(Vec::new());
        let stem = |path: &Path| path.file_stem().and_then(OsStr::to_str).map(str::to_owned);
        let read = AppContext::read(
            &at_home(&[]),
            None,
            said(&search(&["/usr/bin", "/Users/x/bin"])),
        );
        let found = installed_among(&read.context, |path| {
            looked.borrow_mut().push(path.to_path_buf());
            stem(path).is_some_and(|name| name.eq_ignore_ascii_case("claude"))
        });
        assert!(
            looked
                .borrow()
                .iter()
                .all(|path| stem(path).as_deref() != Some("Claude")),
            "{looked:?}"
        );
        assert_eq!(found, [ProviderId::Claude]);
        assert_eq!(read.context.program_for(ProviderId::Desktop), None);
    }

    #[test]
    fn its_data_folder_makes_it_found() {
        let folder = tempfile::tempdir().expect("a scratch folder");
        let scratch = folder.path().join("scratch");
        let named = scratch.to_string_lossy().into_owned();
        let read = AppContext::read(
            &env(&[
                ("HOME", "/Users/x"),
                ("PITBOARD_TEST_CLAUDE_DESKTOP", &named),
            ]),
            None,
            LoginPath::Unknown,
        );
        assert!(
            installed_among(&read.context, |_| false).is_empty(),
            "no data folder yet"
        );
        std::fs::create_dir_all(scratch.join("data")).unwrap();
        assert_eq!(
            installed_among(&read.context, |_| false),
            [ProviderId::Desktop]
        );
    }

    #[test]
    fn each_tool_is_looked_for_where_its_installers_put_it() {
        let places = |os: Os, pairs: &[(&str, &str)], tool: ProviderId| {
            let cli = tool.cli().expect("a command line");
            (cli.install_places)(&Environment::on(os, pairs.iter().copied()))
        };
        let under = |root: &str, parts: &[&str]| {
            let mut place = PathBuf::from(root);
            place.extend(parts);
            place
        };
        let mac = [("HOME", "/Users/x")];
        for tool in [ProviderId::Claude, ProviderId::Codex] {
            assert_eq!(
                places(Os::MacOs, &mac, tool),
                [
                    under("/Users/x", &[".local", "bin"]),
                    "/opt/homebrew/bin".into(),
                    "/usr/local/bin".into()
                ],
                "Homebrew on Apple silicon and on Intel: {tool}"
            );
            assert_eq!(
                places(Os::Linux, &[("HOME", "/home/x")], tool),
                [under("/home/x", &[".local", "bin"])],
                "{tool}"
            );
        }

        let windows = [
            ("USERPROFILE", "C:/Users/x"),
            ("APPDATA", "C:/Users/x/AppData/Roaming"),
            ("LOCALAPPDATA", "C:/Users/x/AppData/Local"),
        ];
        let managers = [
            under("C:/Users/x/AppData/Roaming", &["npm"]),
            under(
                "C:/Users/x/AppData/Local",
                &["Microsoft", "WinGet", "Links"],
            ),
            under("C:/Users/x", &["scoop", "shims"]),
        ];
        let claude_launcher = under("C:/Users/x", &[".local", "bin"]);
        let mut expected = vec![claude_launcher];
        expected.extend(managers.clone());
        assert_eq!(places(Os::Windows, &windows, ProviderId::Claude), expected);
        let git_bash = [&windows[..], &[("HOME", "C:/Users/x/home")]].concat();
        assert_eq!(
            places(Os::Windows, &git_bash, ProviderId::Claude)[0],
            under("C:/Users/x/home", &[".local", "bin"]),
            "Claude Code's launcher is in `HOME ?? USERPROFILE`"
        );
        let mut expected = vec![under(
            "C:/Users/x/AppData/Local",
            &["Programs", "OpenAI", "Codex", "bin"],
        )];
        expected.extend(managers.clone());
        assert_eq!(places(Os::Windows, &windows, ProviderId::Codex), expected);
        let moved = [&windows[..], &[("CODEX_INSTALL_DIR", "D:/tools/codex")]].concat();
        assert_eq!(
            places(Os::Windows, &moved, ProviderId::Codex)[0],
            PathBuf::from("D:/tools/codex")
        );
        let blank = [&windows[..], &[("CODEX_INSTALL_DIR", "  ")]].concat();
        assert_eq!(places(Os::Windows, &blank, ProviderId::Codex), expected);

        for tool in [ProviderId::Claude, ProviderId::Codex] {
            assert_eq!(
                places(Os::Windows, &[], tool),
                Vec::<PathBuf>::new(),
                "a place whose variable is unset is no place: {tool}"
            );
        }
        assert_eq!(
            places(
                Os::Windows,
                &[("APPDATA", ""), ("LOCALAPPDATA", "C:/L")],
                ProviderId::Claude
            ),
            [under("C:/L", &["Microsoft", "WinGet", "Links"])],
            "nor is one whose variable is empty"
        );
    }

    /// An app reads every variable the command line reads, the same way, so a custom OAuth
    /// endpoint, a variable that signs Claude Code in some other way, the successor
    /// credential backend and a test's own address for the services reach it too. It used to
    /// read only the variables that move where things are kept.
    #[test]
    fn an_app_reads_its_environment_as_the_command_line_does() {
        let given = env(&[
            ("HOME", "/Users/x"),
            ("PITBOARD_HOME", "/Users/x/elsewhere"),
            ("CLAUDE_CONFIG_DIR", "/Users/x/claude"),
            ("CLAUDE_SECURESTORAGE_CONFIG_DIR", ""),
            ("USER", "x"),
            ("CODEX_HOME", "/Users/x/codex"),
            ("CLAUDE_CODE_CUSTOM_OAUTH_URL", "https://oauth.example"),
            ("ANTHROPIC_API_KEY", "not-a-key"),
            ("CLAUDE_CODE_USE_BEDROCK", "1"),
            ("CLAUDE_CODE_HOVER_REST", "true"),
            ("CLAUDE_CODE_FORCE_WINDOWS_CREDMAN", "1"),
            ("PITBOARD_API_BASE", "http://127.0.0.1:8080"),
            ("PITBOARD_NO_ARGV", "1"),
        ]);
        let app = AppContext::read(&given, None, LoginPath::Unknown).context;
        let cli = Context::for_command_line(&given);
        assert_eq!(app.home, cli.home);
        assert_eq!(app.pitboard_home, cli.pitboard_home);
        assert_eq!(app.claude_config_dir, cli.claude_config_dir);
        assert_eq!(app.secure_storage_dir.as_deref(), Some(""));
        assert_eq!(app.user, cli.user);
        assert_eq!(app.codex_home, cli.codex_home);
        assert!(app.custom_oauth() && cli.custom_oauth());
        assert_eq!(
            app.overriding_auth(),
            ["ANTHROPIC_API_KEY", "CLAUDE_CODE_USE_BEDROCK"]
        );
        assert_eq!(app.overriding_auth(), cli.overriding_auth());
        assert!(app.hover_rest && cli.hover_rest);
        assert!(app.credential_manager_forced && cli.credential_manager_forced);
        assert_eq!(app.api_base.as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(app.api_base, cli.api_base);
        assert!(!app.argv_fallback() && !cli.argv_fallback());
        assert_eq!((app.caller.as_str(), cli.caller.as_str()), ("app", "cli"));
    }

    /// `PITBOARD_NO_ARGV=1` in the app's environment refuses the argument line, as it does
    /// for the command line, and anything else leaves it allowed.
    #[test]
    fn the_argument_line_is_refused_where_the_environment_says_so() {
        let allowed = |value: Option<&str>| {
            let mut pairs = vec![("HOME", "/Users/x")];
            pairs.extend(value.map(|v| ("PITBOARD_NO_ARGV", v)));
            AppContext::read(&env(&pairs), None, LoginPath::Unknown)
                .context
                .argv_fallback()
        };
        assert!(!allowed(Some("1")));
        assert!(allowed(None));
        assert!(allowed(Some("0")));
    }

    // It names a folder relative to wherever the shell was, which is not where this app is.
    #[test]
    fn a_relative_entry_is_not_looked_in() {
        let looked = RefCell::new(Vec::new());
        let read = AppContext::read(
            &at_home(&[]),
            None,
            said(&search(&["bin", "./node_modules/.bin", "/usr/bin"])),
        );
        installed_among(&read.context, |path| {
            looked.borrow_mut().push(path.to_path_buf());
            false
        });
        let looked = looked.into_inner();
        assert!(looked.iter().all(|path| path.is_absolute()), "{looked:?}");
        assert_eq!(looked.first(), Some(&program("/usr/bin", "claude")));
    }

    // Looking in one asks the person whether Pitboard may, as a program started there would.
    #[test]
    fn a_guarded_folder_on_the_path_is_not_looked_in() {
        let entries = [
            "/Users/x/Documents/bin",
            "/Users/x/Library/Mobile Documents/com~apple~CloudDocs/bin",
            "/Users/x/Desktop",
            "/Users/x/Downloads/tools/bin",
            "/Users/x/Library/CloudStorage/bin",
            "/Users/x/Documentsbin",
            "/usr/bin",
        ];
        let home = at_home(&[]);
        let looked = RefCell::new(Vec::new());
        let read = AppContext::read(&home, None, said(&search(&entries)));
        installed_among(&read.context, |path| {
            looked.borrow_mut().push(path.to_path_buf());
            false
        });
        let looked = looked.into_inner();
        let kept: &[&str] = match OS {
            Os::MacOs => &["/Users/x/Documentsbin", "/usr/bin"],
            Os::Linux | Os::Windows => &entries,
        };
        let lookup = read.context.lookup();
        let each = |tool: ProviderId| -> Vec<PathBuf> {
            let cli = tool.cli().expect("a command line");
            kept.iter()
                .map(|entry| full(entry))
                .chain((cli.install_places)(&home))
                .flat_map(|dir| lookup.spellings(&dir.join(tool.program())))
                .collect()
        };
        let (claude, codex) = (each(ProviderId::Claude), each(ProviderId::Codex));
        assert_eq!(looked[..claude.len()], claude[..], "{looked:?}");
        assert_eq!(looked[claude.len()..], codex[..], "{looked:?}");
        assert_eq!(read.search_path.as_deref(), Some(search(kept).as_str()));
    }

    // The app has no renewal of its own, so anything else names none and cannot schedule it.
    #[test]
    fn the_schedule_runs_the_command_line_inside_the_app() {
        let scheduled = |app: Option<&Path>| {
            AppContext::read(&at_home(&[]), app, LoginPath::Unknown)
                .context
                .schedule_program()
                .map(Path::to_path_buf)
        };
        let mac = |app: &str| Some(Path::new(app).join("Contents/Helpers/pitboard"));
        let windows = |install: &str| Some(full(install).join("bin").join("pitboard.exe"));
        for (app, on_a_mac, on_windows) in [
            (
                full("/Applications/Pitboard.app"),
                mac("/Applications/Pitboard.app"),
                None,
            ),
            (
                full("/Users/x/My Apps/Pitboard.app"),
                mac("/Users/x/My Apps/Pitboard.app"),
                None,
            ),
            (
                full("/Program Files/Pitboard/Pitboard.exe"),
                None,
                windows("/Program Files/Pitboard"),
            ),
            (
                full("/Users/x/Programs/My Pitboard/PITBOARD.EXE"),
                None,
                windows("/Users/x/Programs/My Pitboard"),
            ),
            (
                full("/Users/x/Programs/Pitboard/pitboard.exe.lnk"),
                None,
                None,
            ),
            (
                full("/Users/x/pitboard/apps/macos/.build/debug"),
                None,
                None,
            ),
        ] {
            let inside = match OS {
                Os::MacOs => on_a_mac,
                Os::Linux => None,
                Os::Windows => on_windows,
            };
            assert_eq!(scheduled(Some(&app)), inside, "{}", app.display());
            assert_eq!(app_command_line(&app), inside, "{}", app.display());
        }
        assert_eq!(scheduled(None), None);
    }

    // Its name has a space and a quote in it, as a folder somebody made might.
    struct Scratch(tempfile::TempDir);

    impl Scratch {
        fn new() -> Scratch {
            let scratch = Scratch(
                tempfile::Builder::new()
                    .prefix("pitboard's tool ")
                    .tempdir()
                    .expect("a scratch directory"),
            );
            scratch.program(&scratch.helper(), true);
            scratch
        }

        fn path(&self) -> &Path {
            self.0.path()
        }

        // The command line inside the app `name`, relative to the root.
        fn inside(name: &str) -> PathBuf {
            match OS {
                Os::MacOs | Os::Linux => Path::new(&format!("{name}.app"))
                    .join("Contents")
                    .join("Helpers")
                    .join("pitboard"),
                Os::Windows => Path::new(name).join("bin").join("pitboard.exe"),
            }
        }

        fn helper(&self) -> PathBuf {
            self.path().join(Scratch::inside("Fake"))
        }

        /// A directory under the root, made if it is not there.
        fn dir(&self, name: &str) -> PathBuf {
            let made = self.path().join(name);
            std::fs::create_dir_all(&made).expect("a scratch directory");
            made
        }

        fn program(&self, at: &Path, runnable: bool) {
            std::fs::create_dir_all(at.parent().expect("its directory")).expect("a directory");
            std::fs::write(at, "#!/bin/sh\n").expect("a program");
            if runnable {
                testing::make_runnable(at).expect("runnable");
            } else {
                testing::deny_running(at).expect("nobody may run it");
            }
        }
    }

    // A link to the app's own, as Homebrew makes one, is the app's own through every link.
    #[test]
    fn the_first_pitboard_found_says_whose_it_is() {
        let pitboard = file_of("pitboard");
        let scratch = Scratch::new();
        let empty = scratch.dir("empty");
        let folder = scratch.dir("folder");
        scratch.dir(&format!("folder/{pitboard}"));
        let plain = scratch.dir("plain");
        scratch.program(&plain.join(&pitboard), false);
        let dangling = scratch.dir("dangling");
        testing::link(
            &scratch.path().join("gone").join(&pitboard),
            &dangling.join(&pitboard),
        )
        .expect("a link to nothing");
        let brew = scratch.dir("brew/bin");
        testing::link(
            &Path::new("..").join("..").join(Scratch::inside("Fake")),
            &brew.join(&pitboard),
        )
        .expect("a link as Homebrew makes one");
        let cargo = scratch.dir("cargo/bin");
        scratch.program(&cargo.join(&pitboard), true);
        let helpers = scratch.path().join("helpers");
        testing::link_dir(scratch.helper().parent().expect("its folder"), &helpers)
            .expect("a link to a directory");

        let own = scratch.helper();
        let find = |dirs: &[&PathBuf], helper: Option<&Path>| {
            let places: Vec<PathBuf> = dirs.iter().map(|dir| (*dir).clone()).collect();
            find_command_line(None, &places, helper)
        };
        assert_eq!(
            find(
                &[&empty, &folder, &plain, &dangling, &brew, &cargo],
                Some(&own)
            ),
            CommandLine::Bundled(brew.join(&pitboard))
        );
        assert_eq!(
            find(&[&helpers], Some(&own)),
            CommandLine::Bundled(helpers.join(&pitboard))
        );
        assert_eq!(
            find(&[&cargo, &brew], Some(&own)),
            CommandLine::Another(cargo.join(&pitboard))
        );
        assert_eq!(
            find(&[&empty, &folder, &plain, &dangling], Some(&own)),
            CommandLine::Nowhere
        );
        assert_eq!(find(&[], Some(&own)), CommandLine::Nowhere);

        let other = scratch.path().join(Scratch::inside("Other"));
        assert_eq!(
            find(&[&brew], Some(&other)),
            CommandLine::Another(brew.join(&pitboard)),
            "another copy's"
        );
        assert_eq!(
            find(&[&brew], None),
            CommandLine::Another(brew.join(&pitboard)),
            "not run from an app"
        );

        let path = std::env::join_paths([full("/nowhere/bin"), PathBuf::from("bin"), brew.clone()])
            .expect("a search path");
        assert_eq!(
            find_command_line(Some(&path), std::slice::from_ref(&cargo), Some(&own)),
            CommandLine::Bundled(brew.join(&pitboard)),
            "the login shell's PATH first, ahead of the places installs go"
        );
    }

    #[test]
    fn the_command_line_is_looked_for_where_each_way_of_installing_it_puts_it() {
        let under = |root: &str, parts: &[&str]| {
            let mut place = PathBuf::from(root);
            place.extend(parts);
            place
        };
        let places = |os: Os, pairs: &[(&str, &str)], app: Option<&str>| {
            command_line_places(
                &Environment::on(os, pairs.iter().copied()),
                app.map(Path::new),
            )
        };
        assert_eq!(
            places(
                Os::MacOs,
                &[("HOME", "/Users/x")],
                Some("/Applications/Pitboard.app")
            ),
            [
                under("/Users/x", &[".cargo", "bin"]),
                under("/Users/x", &[".local", "bin"]),
                "/opt/homebrew/bin".into(),
                "/usr/local/bin".into()
            ]
        );
        assert_eq!(
            places(Os::Linux, &[("HOME", "/home/x")], None),
            [
                under("/home/x", &[".cargo", "bin"]),
                under("/home/x", &[".local", "bin"])
            ]
        );
        let windows = [
            ("USERPROFILE", "C:/Users/x"),
            ("APPDATA", "C:/Users/x/AppData/Roaming"),
            ("LOCALAPPDATA", "C:/Users/x/AppData/Local"),
        ];
        let app = "C:/Users/x/AppData/Local/Programs/Pitboard/Pitboard.exe";
        assert_eq!(
            places(Os::Windows, &windows, Some(app)),
            [
                under("C:/Users/x", &[".cargo", "bin"]),
                under("C:/Users/x/AppData/Local/Programs/Pitboard", &["bin"]),
                under("C:/Users/x/AppData/Roaming", &["npm"]),
                under(
                    "C:/Users/x/AppData/Local",
                    &["Microsoft", "WinGet", "Links"]
                ),
                under("C:/Users/x", &["scoop", "shims"]),
            ]
        );
        assert_eq!(
            places(Os::Windows, &[], Some(app)),
            [under(
                "C:/Users/x/AppData/Local/Programs/Pitboard",
                &["bin"]
            )],
            "a place whose variable is unset is no place"
        );
        assert_eq!(places(Os::Windows, &[], None), Vec::<PathBuf>::new());
    }

    /// An app's file is kept in Pitboard's directory, which is made private where it is not
    /// there yet, and the file private to its owner, whole: read back as it was written, and
    /// in its place what was written last. Nothing kept reads as nothing.
    #[test]
    fn an_apps_file_is_kept_private_in_pitboards_directory() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path().join("root");
        let ctx = Context::new(root.clone()).with_pitboard_home(root.join(".pitboard"));
        let read = |file| read_app_file(&ctx, file).expect("read");
        assert_eq!(read(AppFile::Told), None);

        write_app_file(
            &ctx,
            Permit::for_a_test(),
            AppFile::Told,
            r#"{"claude/work/session/":7200}"#,
        )
        .expect("kept");
        write_app_file(&ctx, Permit::for_a_test(), AppFile::Preferences, "{}").expect("kept");
        assert_eq!(
            read(AppFile::Told).as_deref(),
            Some(r#"{"claude/work/session/":7200}"#)
        );
        write_app_file(&ctx, Permit::for_a_test(), AppFile::Told, "{}").expect("kept again");
        assert_eq!(read(AppFile::Told).as_deref(), Some("{}"));
        assert_eq!(read(AppFile::Preferences).as_deref(), Some("{}"));
        for kept in [
            root.join(".pitboard"),
            root.join(".pitboard").join("told.json"),
            root.join(".pitboard").join("app.json"),
        ] {
            assert!(
                testing::is_private(&kept).expect("there"),
                "{}",
                kept.display()
            );
        }
    }

    /// A file that is there and cannot be read is an error, not nothing kept: here a
    /// directory where the file goes, which no user can read as a file, and a file that is
    /// not text. An app that took either as nothing would write over what it never read.
    #[test]
    fn an_apps_file_that_cannot_be_read_is_not_nothing() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path().join("root");
        let dir = root.join(".pitboard");
        let ctx = Context::new(root.clone()).with_pitboard_home(dir.clone());
        std::fs::create_dir_all(dir.join(AppFile::Preferences.name())).expect("a directory");
        std::fs::write(dir.join(AppFile::Told.name()), [0xff, 0xfe, 0x00]).expect("a file");
        assert!(read_app_file(&ctx, AppFile::Preferences).is_err());
        assert!(read_app_file(&ctx, AppFile::Told).is_err());
    }

    /// A file an app keeps outside Pitboard's directory is kept as one inside it: its
    /// directories made private where they are not there, the file private and whole, and
    /// nothing kept read as nothing, while a file that cannot be read is an error.
    #[test]
    fn a_file_kept_elsewhere_is_kept_as_pitboards_own() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path().join("root");
        let file = root
            .join("Application Support")
            .join("app")
            .join("windows.json");
        assert_eq!(read_file(&file).expect("read"), None);
        write_file(Permit::for_a_test(), &file, "{}").expect("kept");
        write_file(Permit::for_a_test(), &file, r#"{"stores":{}}"#).expect("kept again");
        assert_eq!(
            read_file(&file).expect("read").as_deref(),
            Some(r#"{"stores":{}}"#)
        );
        for kept in [file.parent().expect("its directory"), file.as_path()] {
            assert!(
                testing::is_private(kept).expect("there"),
                "{}",
                kept.display()
            );
        }
        std::fs::write(&file, [0xff, 0xfe, 0x00]).expect("a file that is not text");
        assert!(read_file(&file).is_err());
    }

    /// Each file has a name of its own, apart from the core's.
    #[test]
    fn each_apps_file_has_a_name_of_its_own() {
        assert_eq!(AppFile::Preferences.name(), "app.json");
        assert_eq!(AppFile::Told.name(), "told.json");
    }
}
