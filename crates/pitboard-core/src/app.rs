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
//! An app also says which `pitboard` a terminal would run, and whether it is the app's own.

use crate::context::{Context, Environment};
use crate::host::{self, LoginPath, OS};
use crate::provider::ProviderId;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// An app's context, and what building it found.
#[derive(Debug)]
#[non_exhaustive]
pub struct AppContext {
    pub context: Context,
    /// The tools whose program was named outright or found, in [`ProviderId::ALL`]'s order.
    /// A tool missing here may still be on some `PATH`, so this narrows what an app offers
    /// and never forbids anything.
    pub found: Vec<ProviderId>,
    /// The login shell's `PATH`, as far as it is looked in: where each tool's program is
    /// looked for, and what a sign-in is given. `None` where the shell could not be asked,
    /// and the app looks on its own `PATH`, as it always did.
    pub search_path: Option<String>,
    /// The login shell had not answered in time. What was found stands, and asking again a
    /// while later may find more.
    pub late: bool,
}

impl AppContext {
    /// The context of an app started with `env`, which is at `app`.
    ///
    /// Asks the person's login shell for its `PATH`, which can take seconds, so this is never
    /// called on an app's main thread.
    pub fn discover(env: &Environment, app: Option<&Path>) -> AppContext {
        AppContext::read(env, app, host::login_path(env), host::proc::can_run)
    }

    /// `discover` with what the login shell said and whether a file can be run handed in, so
    /// a test can say both without a shell or a program.
    ///
    /// Each tool's program is the one its variable names outright, or else the first found
    /// on the login shell's `PATH`, where a version manager or an npm prefix puts it, and
    /// then where the tool's own installers put it, which is all there is when the shell
    /// could not be asked. Claude Desktop is found where its bundle is, and never on a path.
    pub(crate) fn read(
        env: &Environment,
        app: Option<&Path>,
        said: LoginPath,
        runnable: impl Fn(&Path) -> bool,
    ) -> AppContext {
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
                    .filter(|dir| dir.is_absolute() && !guarded(dir, &context.home))
                    .collect()
            })
            .unwrap_or_default();
        let mut found = Vec::new();
        for &tool in ProviderId::ALL {
            // An app counts only where its bundle has its program, named outright or not:
            // a bundle that is not there is not an installed app, and a system that runs no
            // such app has none to look for.
            if !tool.on_path() {
                if context.desktop_program().is_some_and(&runnable) {
                    found.push(tool);
                }
                continue;
            }
            if env
                .path(tool.program_variable())
                .is_some_and(|named| !named.is_empty())
            {
                found.push(tool);
                continue;
            }
            let places = looked
                .iter()
                .cloned()
                .chain(tool.install_places(&context.home));
            if let Some(program) =
                host::program::find_among(Path::new(tool.program()), places, &runnable)
            {
                context.set_program(tool, program);
                found.push(tool);
            }
        }
        let search_path = shell.map(|_| {
            std::env::join_paths(&looked)
                .map(|joined| joined.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        if let Some(path) = &search_path {
            context.search_path = Some(path.into());
        }
        context.schedule_program = app.and_then(|app| OS.app_command_line(app));
        AppContext {
            context,
            found,
            search_path,
            late,
        }
    }
}

/// Whether `dir` is inside a folder of `home` that the system asks the person about before
/// an app may read it.
fn guarded(dir: &Path, home: &Path) -> bool {
    OS.guarded_folders()
        .iter()
        .any(|folder| dir.starts_with(home.join(folder)))
}

/// The command line an app at `app` comes with, which its renewal schedule runs. `None` for
/// anything that is not an app, such as a test or a build directory.
pub fn app_command_line(app: &Path) -> Option<PathBuf> {
    OS.app_command_line(app)
}

/// Whether `path` is a program this user may run, as the core judges every program it finds
/// and the system judges one it starts: a regular file, once every link is followed, that
/// this user may execute. An app asks this of anything it would run or link to, so it has
/// no rule of its own.
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

/// Where each way of installing Pitboard puts `pitboard`, looked in after the login shell's
/// `PATH`: `cargo install` and a copy from a release under `home`, then where the system's
/// package managers put programs, which on a Mac is where Homebrew links the app's own and
/// where the app links it itself, `/usr/local/bin`.
pub fn command_line_places(home: &Path) -> Vec<PathBuf> {
    [home.join(".cargo/bin"), home.join(".local/bin")]
        .into_iter()
        .chain(OS.package_bins().iter().map(PathBuf::from))
        .collect()
}

/// The first `pitboard` a terminal would run, as a terminal finds it: the first file of
/// that name that can be run on `search_path`, in `PATH`'s form, and then in `places`. It is
/// the app's own when, every link on the way followed, it is the command line at `helper`.
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
    let Some(found) = host::program::find_among(Path::new("pitboard"), dirs, host::proc::can_run)
    else {
        return CommandLine::Nowhere;
    };
    let own = helper.and_then(|helper| std::fs::canonicalize(helper).ok());
    match own {
        Some(own) if std::fs::canonicalize(&found).is_ok_and(|resolved| resolved == own) => {
            CommandLine::Bundled(found)
        }
        _ => CommandLine::Another(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Os;
    use std::cell::RefCell;

    fn env(pairs: &[(&str, &str)]) -> Environment {
        pairs.iter().copied().collect()
    }

    fn said(path: &str) -> LoginPath {
        LoginPath::Said(path.into())
    }

    /// A program is found where the variable naming it outright says, then on the login
    /// shell's `PATH`, where a version manager or an npm prefix puts it, then where its
    /// installer does.
    #[test]
    fn a_program_is_found_by_its_variable_then_the_login_shell_then_its_installer() {
        let home = env(&[("HOME", "/Users/x")]);
        let here: Vec<PathBuf> = [
            "/Users/x/.nvm/versions/node/v22.1.0/bin/codex",
            "/Users/x/.volta/bin/claude",
            "/Users/x/.local/bin/codex",
            "/Users/x/.local/bin/claude",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        let runnable = |path: &Path| here.iter().any(|there| there == path);
        let login = "/usr/bin:bin:/Users/x/.volta/bin:/Users/x/.nvm/versions/node/v22.1.0/bin";

        let shell = AppContext::read(&home, None, said(login), runnable);
        assert_eq!(
            shell.context.codex_program(),
            Path::new("/Users/x/.nvm/versions/node/v22.1.0/bin/codex")
        );
        assert_eq!(
            shell.context.claude_program(),
            Path::new("/Users/x/.volta/bin/claude")
        );
        assert_eq!(shell.found, [ProviderId::Claude, ProviderId::Codex]);
        let looked = "/usr/bin:/Users/x/.volta/bin:/Users/x/.nvm/versions/node/v22.1.0/bin";
        assert_eq!(
            shell.search_path.as_deref(),
            Some(looked),
            "the core looks where the app looks"
        );
        assert_eq!(
            shell.context.search_path(),
            looked,
            "and a sign-in is given it"
        );

        let named = AppContext::read(
            &env(&[("HOME", "/Users/x"), ("PITBOARD_CODEX", "/elsewhere/codex")]),
            None,
            said(login),
            runnable,
        );
        assert_eq!(named.context.codex_program(), Path::new("/elsewhere/codex"));
        assert_eq!(
            named.context.claude_program(),
            Path::new("/Users/x/.volta/bin/claude")
        );
        assert_eq!(
            named.found,
            [ProviderId::Claude, ProviderId::Codex],
            "a program named outright counts as found"
        );

        let empty = AppContext::read(
            &env(&[("HOME", "/Users/x"), ("PITBOARD_CODEX", "")]),
            None,
            said("/opt/tools/bin"),
            |path| path == Path::new("/opt/tools/bin/codex"),
        );
        assert_eq!(
            empty.context.codex_program(),
            Path::new("/opt/tools/bin/codex"),
            "an empty name names nothing, and the app looks as it does without one"
        );

        let unknown = AppContext::read(&home, None, LoginPath::Unknown, runnable);
        assert_eq!(
            unknown.context.codex_program(),
            Path::new("/Users/x/.local/bin/codex")
        );
        assert_eq!(
            unknown.context.claude_program(),
            Path::new("/Users/x/.local/bin/claude")
        );
        assert_eq!(
            unknown.search_path, None,
            "the app looks where it always did"
        );
        assert!(!unknown.late);

        let nowhere = AppContext::read(&home, None, said(login), |_| false);
        assert_eq!(nowhere.context.codex_program(), Path::new("codex"));
        assert_eq!(nowhere.context.claude_program(), Path::new("claude"));
        assert!(nowhere.found.is_empty());

        let late = AppContext::read(&home, None, LoginPath::Late, |_| false);
        assert!(late.late, "a late shell says so, for the app to ask again");
        assert_eq!(late.search_path, None);
    }

    /// Claude Desktop is found in its bundle, where the context says it is, and never on a
    /// search path: on a volume that ignores case, `Claude` there would be Claude Code.
    #[cfg(target_os = "macos")]
    #[test]
    fn claude_desktop_is_found_in_its_bundle_and_never_on_a_path() {
        let home = env(&[("HOME", "/Users/x")]);
        let installed =
            |path: &Path| path == Path::new("/Applications/Claude.app/Contents/MacOS/Claude");
        let app = AppContext::read(&home, None, said("/usr/bin"), installed);
        assert_eq!(app.found, [ProviderId::Desktop]);

        let on_path = |path: &Path| path.ends_with("Claude") || path.ends_with("claude");
        let looked = AppContext::read(&home, None, said("/Users/x/bin"), |path: &Path| {
            path.starts_with("/Users/x/bin") && on_path(path)
        });
        assert_eq!(
            looked.found,
            [ProviderId::Claude],
            "a `Claude` on the path is not the app"
        );
        assert_eq!(
            looked.context.program_for(ProviderId::Desktop),
            Path::new("/Applications/Claude.app/Contents/MacOS/Claude")
        );

        let moved_env = env(&[
            ("HOME", "/Users/x"),
            ("PITBOARD_CLAUDE_DESKTOP_APP", "/scratch/Claude.app"),
        ]);
        let absent = AppContext::read(&moved_env, None, said("/usr/bin"), |_| false);
        assert!(
            absent.found.is_empty(),
            "a bundle named outright counts only where its program is"
        );
        let moved = AppContext::read(&moved_env, None, said("/usr/bin"), |path: &Path| {
            path == Path::new("/scratch/Claude.app/Contents/MacOS/Claude")
        });
        assert_eq!(moved.found, [ProviderId::Desktop]);
        assert_eq!(
            moved.context.program_for(ProviderId::Desktop),
            Path::new("/scratch/Claude.app/Contents/MacOS/Claude")
        );
    }

    /// The installers' places come after the login shell's `PATH`, in the order each tool
    /// says, and every tool's own installer goes first.
    #[test]
    fn each_tool_is_looked_for_where_its_installers_put_it() {
        let home = Path::new("/Users/x");
        for &tool in ProviderId::ALL.iter().filter(|tool| tool.on_path()) {
            let places = tool.install_places(home);
            assert_eq!(places.first(), Some(&home.join(".local/bin")), "{tool}");
            let after: Vec<&Path> = places[1..].iter().map(PathBuf::as_path).collect();
            let expected: Vec<&Path> = OS.package_bins().iter().map(Path::new).collect();
            assert_eq!(after, expected, "{tool}");
        }
        match OS {
            Os::MacOs => assert_eq!(
                OS.package_bins(),
                ["/opt/homebrew/bin", "/usr/local/bin"],
                "Homebrew on Apple silicon and on Intel"
            ),
            Os::Linux => assert!(OS.package_bins().is_empty()),
        }
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
            ("PITBOARD_API_BASE", "http://127.0.0.1:8080"),
            ("PITBOARD_NO_ARGV", "1"),
        ]);
        let app = AppContext::read(&given, None, LoginPath::Unknown, |_| false).context;
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
            AppContext::read(&env(&pairs), None, LoginPath::Unknown, |_| false)
                .context
                .argv_fallback()
        };
        assert!(!allowed(Some("1")));
        assert!(allowed(None));
        assert!(allowed(Some("0")));
    }

    /// A relative entry on the login shell's `PATH` names a directory relative to wherever
    /// the shell was, which is not where this app is, so it is not looked in.
    #[test]
    fn a_relative_entry_is_not_looked_in() {
        let looked = RefCell::new(Vec::new());
        let _ = AppContext::read(
            &env(&[("HOME", "/Users/x")]),
            None,
            said("bin:./node_modules/.bin:/usr/bin"),
            |path| {
                looked.borrow_mut().push(path.to_path_buf());
                false
            },
        );
        let looked = looked.into_inner();
        assert!(looked.iter().all(|path| path.is_absolute()), "{looked:?}");
        assert_eq!(looked.first(), Some(&PathBuf::from("/usr/bin/claude")));
    }

    /// A folder macOS guards, such as Documents or iCloud Drive, is not looked in, and is not
    /// where the core looks or what a sign-in is given: looking there asks the person whether
    /// Pitboard may, and a program started from there would have it asked on its behalf.
    /// Linux guards none.
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
        let looked = RefCell::new(Vec::new());
        let read = AppContext::read(
            &env(&[("HOME", "/Users/x")]),
            None,
            said(&entries.join(":")),
            |path| {
                looked.borrow_mut().push(path.to_path_buf());
                false
            },
        );
        let looked = looked.into_inner();
        let kept: &[&str] = match OS {
            Os::MacOs => &["/Users/x/Documentsbin", "/usr/bin"],
            Os::Linux => &entries,
        };
        let places = ProviderId::Claude.install_places(Path::new("/Users/x"));
        let claude: Vec<PathBuf> = kept
            .iter()
            .map(PathBuf::from)
            .chain(places)
            .map(|dir| dir.join("claude"))
            .collect();
        assert_eq!(looked[..claude.len()], claude[..], "{looked:?}");
        // Claude Desktop is looked for only in its own bundle, never on the path.
        let on_path: Vec<&PathBuf> = looked
            .iter()
            .filter(|path| !path.starts_with("/Applications/Claude.app"))
            .collect();
        assert_eq!(
            on_path.len(),
            2 * claude.len(),
            "the same places for each tool: {looked:?}"
        );
        assert_eq!(read.search_path.as_deref(), Some(kept.join(":").as_str()));
    }

    /// The renewal schedule runs the command line inside the app, since the app has no
    /// renewal of its own. Anything not running from an app bundle names none, which means it
    /// cannot schedule renewal: the only other thing to schedule is the app itself.
    #[test]
    fn the_schedule_runs_the_command_line_inside_the_app() {
        let scheduled = |app: Option<&str>| {
            AppContext::read(
                &env(&[("HOME", "/Users/x")]),
                app.map(Path::new),
                LoginPath::Unknown,
                |_| false,
            )
            .context
            .schedule_program()
            .map(Path::to_path_buf)
        };
        let inside = |app: &str| match OS {
            Os::MacOs => Some(PathBuf::from(format!("{app}/Contents/Helpers/pitboard"))),
            Os::Linux => None,
        };
        for app in [
            "/Applications/Pitboard.app",
            "/Users/x/My Apps/Pitboard.app",
        ] {
            assert_eq!(scheduled(Some(app)), inside(app), "{app}");
            assert_eq!(app_command_line(Path::new(app)), inside(app), "{app}");
        }
        assert_eq!(
            scheduled(Some("/Users/x/pitboard/apps/macos/.build/debug")),
            None
        );
        assert_eq!(scheduled(None), None);
    }

    /// A scratch directory with an app bundle carrying a command line, and whatever a test
    /// puts beside it. Its name has a space and a quote in it, as a folder somebody made
    /// might. Nothing in it is ever run.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Scratch {
            let root = std::env::temp_dir().join(format!(
                "pitboard's tool {}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            let scratch = Scratch(root);
            scratch.program(&scratch.helper(), true);
            scratch
        }

        fn helper(&self) -> PathBuf {
            self.0.join("Fake.app/Contents/Helpers/pitboard")
        }

        /// A directory under the root, made if it is not there.
        fn dir(&self, name: &str) -> PathBuf {
            let made = self.0.join(name);
            std::fs::create_dir_all(&made).expect("a scratch directory");
            made
        }

        fn program(&self, at: &Path, runnable: bool) {
            std::fs::create_dir_all(at.parent().expect("its directory")).expect("a directory");
            std::fs::write(at, "#!/bin/sh\n").expect("a program");
            if runnable {
                crate::host::fs::testing::make_runnable(at);
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The first `pitboard` on the path is the one a terminal runs, and a link to the app's
    /// own, the way Homebrew makes one, is the app's own however many links it takes to get
    /// there. A directory, a file nobody can run and a link to nothing are not a `pitboard`.
    #[test]
    fn the_first_pitboard_found_says_whose_it_is() {
        let scratch = Scratch::new();
        let empty = scratch.dir("empty");
        let folder = scratch.dir("folder");
        scratch.dir("folder/pitboard");
        let plain = scratch.dir("plain");
        scratch.program(&plain.join("pitboard"), false);
        let dangling = scratch.dir("dangling");
        crate::host::fs::testing::link(
            &scratch.0.join("gone/pitboard"),
            &dangling.join("pitboard"),
        );
        let brew = scratch.dir("brew/bin");
        crate::host::fs::testing::link(
            Path::new("../../Fake.app/Contents/Helpers/pitboard"),
            &brew.join("pitboard"),
        );
        let cargo = scratch.dir("cargo/bin");
        scratch.program(&cargo.join("pitboard"), true);
        let helpers = scratch.0.join("helpers");
        crate::host::fs::testing::link(scratch.helper().parent().expect("Helpers"), &helpers);

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
            CommandLine::Bundled(brew.join("pitboard"))
        );
        assert_eq!(
            find(&[&helpers], Some(&own)),
            CommandLine::Bundled(helpers.join("pitboard"))
        );
        assert_eq!(
            find(&[&cargo, &brew], Some(&own)),
            CommandLine::Another(cargo.join("pitboard"))
        );
        assert_eq!(
            find(&[&empty, &folder, &plain, &dangling], Some(&own)),
            CommandLine::Nowhere
        );
        assert_eq!(find(&[], Some(&own)), CommandLine::Nowhere);

        let other = scratch.0.join("Other.app/Contents/Helpers/pitboard");
        assert_eq!(
            find(&[&brew], Some(&other)),
            CommandLine::Another(brew.join("pitboard")),
            "another copy's"
        );
        assert_eq!(
            find(&[&brew], None),
            CommandLine::Another(brew.join("pitboard")),
            "not run from an app"
        );

        let path = std::env::join_paths([Path::new("/nowhere/bin"), Path::new("bin"), &brew])
            .expect("a search path");
        assert_eq!(
            find_command_line(Some(&path), std::slice::from_ref(&cargo), Some(&own)),
            CommandLine::Bundled(brew.join("pitboard")),
            "the login shell's PATH first, ahead of the places installs go"
        );
    }

    /// Pitboard's own installs go where cargo and a copy from a release put it under the
    /// home, then where the system's package managers put programs.
    #[test]
    fn the_command_line_is_looked_for_where_each_way_of_installing_it_puts_it() {
        let home = Path::new("/Users/x");
        let places = command_line_places(home);
        assert_eq!(
            places[..2],
            [home.join(".cargo/bin"), home.join(".local/bin")]
        );
        let after: Vec<&Path> = places[2..].iter().map(PathBuf::as_path).collect();
        let expected: Vec<&Path> = OS.package_bins().iter().map(Path::new).collect();
        assert_eq!(after, expected);
    }
}
