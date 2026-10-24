//! Where a running `codex` can be, and how each is started again to take a switch
//! (`codex_never_follows_a_switch`).

// Each place is a register fact: codex_runs_inside_the_chatgpt_app, codex_app_server_daemon,
// codex_windows_app_server, codex_windows_app_identity and codex_linux_app. An editor's
// extension is not measured (docs/facts/not-measured.md).

use crate::context::Context;
use crate::holder::{AppId, Holder, Location, Noun, Remedy};
use crate::host::{OS, Os};

pub(crate) const HOLDERS: &[Holder] = &[
    // `/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex`,
    // two of them, children of the app. Closing the app's windows leaves it running. The
    // app's bundle id is `com.openai.codex`.
    Holder {
        kind: "chatgpt_app",
        noun: Noun::One("the ChatGPT app"),
        location: Location::Within("ChatGPT.app"),
        remedy: Remedy::ReopenApp {
            app: AppId::MacBundle("com.openai.codex"),
            name: "ChatGPT",
        },
    },
    // Its family and app id as Codex names them (codex_windows_app_identity).
    Holder {
        kind: "chatgpt_app",
        noun: Noun::One("the ChatGPT app"),
        location: Location::Package("OpenAI.Codex_2p2nqsd0c76g0"),
        remedy: Remedy::ReopenApp {
            app: AppId::WindowsAumid("OpenAI.Codex_2p2nqsd0c76g0!App"),
            name: "ChatGPT",
        },
    },
    // `/usr/lib/chatgpt/resources/codex`, which the app runs as `codex app-server` with its own
    // environment (codex_linux_app). It stays open after its windows close while its icon is up.
    Holder {
        kind: "chatgpt_app",
        noun: Noun::One("the ChatGPT app"),
        location: Location::Within("chatgpt"),
        remedy: Remedy::Do("quit ChatGPT and open it again"),
    },
    // `releases/<version>/bin/codex` in Codex's package, a child of launchd, which terminal
    // sessions can share (`codex_app_server_daemon`, `codex_windows_app_server`).
    Holder {
        kind: "app_server_daemon",
        noun: Noun::One("Codex's background app server"),
        location: Location::Within("app-server-daemon"),
        remedy: restart_the_app_server(OS),
    },
    // Where an older Codex left a daemon's launch state, the app server runs from the standalone
    // package's release, as a terminal session does, so it is told by the pid Codex records.
    Holder {
        kind: "app_server_daemon",
        noun: Noun::One("Codex's background app server"),
        location: Location::Recorded,
        remedy: restart_the_app_server(OS),
    },
    // `<extensions>/openai.chatgpt-<version>-<platform>/bin/<platform>/codex`.
    Holder {
        kind: "editor_extension",
        noun: Noun::One("Codex in an editor"),
        location: Location::WithinPrefixed("openai.chatgpt-"),
        remedy: Remedy::Do("run Developer: Reload Window in each editor window that uses it"),
    },
    // Started from a shell by its bare name, or by any other path.
    Holder {
        kind: "session",
        noun: Noun::Counted {
            one: "`codex` session",
            many: "`codex` sessions",
        },
        location: Location::Anywhere,
        remedy: Remedy::Restart,
    },
];

// The app server and its updater, each as `{"pid": ...}`, in the names for the package each runs
// from (codex_app_server_daemon).
const PID_FILES: [&str; 4] = [
    "daemon.pid",
    "daemon-updater.pid",
    "app-server.pid",
    "app-server-updater.pid",
];

// What a stopped daemon left names a process no longer running, or one that took its id since.
pub(crate) fn recorded(ctx: &Context) -> Vec<u32> {
    let folder = super::paths::home(ctx).join("app-server-daemon");
    PID_FILES
        .iter()
        .filter_map(|name| {
            let text = std::fs::read_to_string(folder.join(name)).ok()?;
            let record: serde_json::Value = serde_json::from_str(&text).ok()?;
            u32::try_from(record.get("pid")?.as_u64()?).ok()
        })
        .collect()
}

// Codex refuses to start its Windows daemon from an elevated terminal (codex_windows_app_server).
const fn restart_the_app_server(os: Os) -> Remedy {
    Remedy::Run {
        arguments: "app-server daemon restart",
        not_elevated: matches!(os, Os::Windows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holder::{Holding, classify};
    use crate::host::Process;
    use std::path::PathBuf;

    // No editor with the extension was running, so its path is the packaged layout's.
    #[test]
    fn every_measured_codex_is_its_own_kind() {
        let listed = [
            "codex",
            "/Users/a/.codex/packages/standalone/releases/0.154.0-aarch64-apple-darwin/bin/codex",
            "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
            "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
            "/Users/a/.codex/packages/app-server-daemon/releases/0.159.3-aarch64-apple-darwin/bin/codex",
            "/Users/a/.vscode/extensions/openai.chatgpt-26.5.0-darwin-arm64/bin/macos-aarch64/codex",
        ];
        let processes: Vec<Process> = (1..)
            .zip(listed)
            .map(|(pid, path)| Process {
                pid,
                path: PathBuf::from(path),
                package: None,
            })
            .collect();
        let holding = classify(&processes, HOLDERS, "codex", &[]);
        let kinds: Vec<(&str, Vec<u32>)> = holding
            .iter()
            .map(|h| (h.holder.kind, h.pids.clone()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("chatgpt_app", vec![3, 4]),
                ("app_server_daemon", vec![5]),
                ("editor_extension", vec![6]),
                ("session", vec![1, 2]),
            ]
        );
    }

    // ChatGPT by its package's family wherever it is installed, not by its folder.
    #[test]
    fn every_codex_on_windows_is_its_own_kind() {
        let listed = [
            (
                "C:/Users/a/AppData/Local/Programs/OpenAI/Codex/bin/codex.exe",
                None,
            ),
            (
                "C:/Program Files/WindowsApps/OpenAI.Codex_1.0.0.0_x64__2p2nqsd0c76g0/codex.exe",
                Some("OpenAI.Codex_2p2nqsd0c76g0"),
            ),
            (
                "C:/Users/a/.codex/packages/app-server-daemon/releases/0.164.0-x86_64-pc-windows-msvc/bin/codex.exe",
                None,
            ),
            (
                "C:/Users/a/.vscode/extensions/openai.chatgpt-26.5.0-win32-x64/bin/windows-x86_64/codex.exe",
                None,
            ),
        ];
        let processes: Vec<Process> = (1..)
            .zip(listed)
            .map(|(pid, (path, package))| Process {
                pid,
                path: PathBuf::from(path),
                package: package.map(str::to_owned),
            })
            .collect();
        let holding = classify(&processes, HOLDERS, "codex", &[]);
        let kinds: Vec<(&str, Vec<u32>)> = holding
            .iter()
            .map(|h| (h.holder.kind, h.pids.clone()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("chatgpt_app", vec![2]),
                ("app_server_daemon", vec![3]),
                ("editor_extension", vec![4]),
                ("session", vec![1]),
            ]
        );
        assert_eq!(
            holding[0].holder.remedy,
            Remedy::ReopenApp {
                app: AppId::WindowsAumid("OpenAI.Codex_2p2nqsd0c76g0!App"),
                name: "ChatGPT",
            }
        );
    }

    // Codex 0.164.0 starts its app server from the release its package's `current` resolves to,
    // which for the standalone package an older Codex's launch state leaves is a terminal
    // session's (codex_app_server_daemon).
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test lays out the pid files Codex's app server leaves"
    )]
    fn a_daemon_left_in_the_standalone_package_is_told_by_the_pid_codex_records() {
        let release = "/home/a/.codex/packages/standalone/releases/0.164.0-x86_64-unknown-linux-musl/bin/codex";
        let mem = crate::host::memory::MemoryHost::new();
        mem.runs_at("codex", &[release, release]);
        let scratch = tempfile::tempdir().expect("a scratch home");
        let home = scratch.path().join("codex");
        let folder = home.join("app-server-daemon");
        std::fs::create_dir_all(&folder).expect("a folder");
        let ctx = Context::for_command_line(&crate::context::Environment::on(
            OS,
            [
                ("HOME", scratch.path().as_os_str()),
                ("USERPROFILE", scratch.path().as_os_str()),
                ("CODEX_HOME", home.as_os_str()),
            ],
        ))
        .with_memory_stores(std::sync::Arc::clone(&mem));
        let found = || {
            crate::holder::find(&ctx, crate::provider::ProviderId::Codex, "codex", HOLDERS)
                .expect("a process list")
                .iter()
                .map(|h| (h.holder.kind, h.pids.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(found(), [("session", vec![1, 2])]);

        std::fs::write(
            folder.join("app-server.pid"),
            r#"{"pid":2,"processStartTime":"Fri Oct 23 10:00:00 2026"}"#,
        )
        .expect("written");
        std::fs::write(folder.join("app-server-updater.pid"), "").expect("written");
        std::fs::write(folder.join("daemon.pid"), "not a record").expect("written");
        assert_eq!(recorded(&ctx), [2]);
        assert_eq!(
            found(),
            [("app_server_daemon", vec![2]), ("session", vec![1])]
        );
    }

    // ChatGPT for Linux 26.1021.22027 runs `resources/codex` under `/usr/lib/chatgpt`, from its
    // `.deb`, `.rpm` or Arch package alike (codex_linux_app).
    #[test]
    fn every_codex_on_linux_is_its_own_kind() {
        let listed = [
            "codex",
            "/usr/lib/chatgpt/resources/codex",
            "/home/a/.codex/packages/app-server-daemon/releases/0.164.0-x86_64-unknown-linux-musl/bin/codex",
            "/home/a/.vscode/extensions/openai.chatgpt-26.5.0-linux-x64/bin/linux-x86_64/codex",
            "/home/a/.local/bin/codex",
        ];
        let processes: Vec<Process> = (1..)
            .zip(listed)
            .map(|(pid, path)| Process {
                pid,
                path: PathBuf::from(path),
                package: None,
            })
            .collect();
        let holding = classify(&processes, HOLDERS, "codex", &[]);
        let kinds: Vec<(&str, Vec<u32>)> = holding
            .iter()
            .map(|h| (h.holder.kind, h.pids.clone()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("chatgpt_app", vec![2]),
                ("app_server_daemon", vec![3]),
                ("editor_extension", vec![4]),
                ("session", vec![1, 5]),
            ]
        );
        assert_eq!(
            crate::holder::remedies(&holding[..1], "to take a switch"),
            "Quit ChatGPT and open it again to take a switch."
        );
    }

    #[test]
    fn the_app_server_is_restarted_from_a_terminal_codex_starts_it_from() {
        let server = *HOLDERS
            .iter()
            .find(|h| h.kind == "app_server_daemon")
            .expect("the app server");
        assert_eq!(server.remedy, restart_the_app_server(OS));
        let said = |os, program| {
            let holding = [Holding {
                holder: Holder {
                    remedy: restart_the_app_server(os),
                    ..server
                },
                pids: vec![7],
                program,
            }];
            crate::holder::remedies(&holding, "to take a switch")
        };
        for os in [Os::MacOs, Os::Linux] {
            assert_eq!(
                said(os, "codex"),
                "Run `codex app-server daemon restart` to take a switch."
            );
        }
        assert_eq!(
            said(Os::Windows, "codex"),
            "From a terminal that is not elevated, run `codex app-server daemon restart` to take \
             a switch."
        );
        assert_eq!(
            said(Os::Windows, "codex.cmd"),
            "From a terminal that is not elevated, run `codex.cmd app-server daemon restart` to \
             take a switch."
        );
    }

    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test lays out an npm prefix as `npm install -g` leaves one"
    )]
    fn the_app_server_is_restarted_by_the_codex_cmd_pitboard_found_on_windows() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let npm = scratch.path().join("npm");
        let native = scratch.path().join("bin");
        for (folder, name) in [
            (&npm, "codex"),
            (&npm, "codex.cmd"),
            (&npm, "codex.ps1"),
            (&native, "codex.exe"),
        ] {
            std::fs::create_dir_all(folder).expect("a folder");
            std::fs::write(folder.join(name), "").expect("a program");
            crate::host::fs::testing::make_runnable(&folder.join(name)).expect("runnable");
        }
        let mem = crate::host::memory::MemoryHost::new();
        mem.runs_at(
            "codex",
            &["C:/Users/a/.codex/packages/app-server-daemon/current/bin/codex.exe"],
        );
        let home = scratch.path().join("home");
        for (os, folder, run) in [
            (Os::Windows, &npm, "codex.cmd"),
            (Os::Windows, &native, "codex"),
            (Os::MacOs, &npm, "codex"),
        ] {
            let env = crate::context::Environment::on(
                os,
                [
                    ("HOME", home.as_os_str()),
                    ("USERPROFILE", home.as_os_str()),
                    ("CODEX_HOME", home.join("codex").as_os_str()),
                    ("PATH", folder.as_os_str()),
                ],
            );
            let ctx =
                Context::for_command_line(&env).with_memory_stores(std::sync::Arc::clone(&mem));
            let holding =
                crate::holder::find(&ctx, crate::provider::ProviderId::Codex, "codex", HOLDERS)
                    .expect("a process list");
            let restart = format!("`{run} app-server daemon restart` to take a switch.");
            assert_eq!(
                crate::holder::remedies(&holding, "to take a switch"),
                match OS {
                    Os::Windows => format!("From a terminal that is not elevated, run {restart}"),
                    Os::MacOs | Os::Linux => format!("Run {restart}"),
                },
                "{os:?} {}",
                folder.display()
            );
        }
    }

    /// Every list ends with a kind that is anywhere, so no `codex` goes unsaid.
    #[test]
    fn the_last_kind_is_anywhere() {
        assert_eq!(HOLDERS.last().map(|h| h.location), Some(Location::Anywhere));
        assert!(
            HOLDERS[..HOLDERS.len() - 1]
                .iter()
                .all(|h| h.location != Location::Anywhere),
            "and nothing before it claims everything"
        );
    }
}
