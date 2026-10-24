use super::{Check, Facts, ok, warn};
use crate::context::Context;
use crate::host::Os;
use crate::provider::claude::paths as claude;
use crate::provider::codex::paths as codex;
use crate::proxy::{SystemProxy, system_proxies};
use crate::words;
use std::path::PathBuf;

pub(crate) struct WslFact {
    pub(crate) distribution: Option<String>,
    // Codex's home, where it is on a drive of Windows'.
    pub(crate) codex_home_on_windows: Option<PathBuf>,
}

// The shell Claude Code runs the status line in, and the pitboard the PATH it hands that shell
// finds. Both shells take `pitboard.exe` for a bare `pitboard`.
pub(crate) struct StatusLineFact {
    pub(crate) git_bash: claude::GitBash,
    pub(crate) powershell: Option<claude::PowerShell>,
    pub(crate) pitboard: Option<PathBuf>,
    // Whether the settings Pitboard reads run a `pitboard` the shell has to find.
    pub(crate) runs_pitboard: bool,
}

// What Windows says of the account's folders, beside the home Pitboard reads them from.
pub(crate) struct WindowsHomes {
    pub(crate) home: PathBuf,
    pub(crate) default_pitboard_home: PathBuf,
    pub(crate) local_app_data: Option<PathBuf>,
    pub(crate) profile: Option<PathBuf>,
    pub(crate) codex_home_set: bool,
    pub(crate) roams: bool,
}

// As Codex 0.163.0's Linux build tells WSL (`codex_wsl`).
pub(super) fn wsl_fact(ctx: &Context) -> Option<WslFact> {
    let inside = ctx.os == Os::Linux && (ctx.wsl_distro.is_some() || ctx.host().kernel_is_wsl());
    let codex_home = codex::home(ctx);
    inside.then(|| WslFact {
        distribution: ctx.wsl_distro.clone().filter(|name| !name.is_empty()),
        codex_home_on_windows: ctx
            .host()
            .on_a_windows_drive(&codex_home)
            .then_some(codex_home),
    })
}

// One file two folders lead to, such as winget's link and its package, is one copy.
pub(super) fn pitboards(ctx: &Context) -> Vec<PathBuf> {
    let lookup = crate::host::program::Lookup::new(Os::Windows, Some(".EXE".as_ref()));
    let mut files: Vec<PathBuf> = Vec::new();
    let mut copies = Vec::new();
    for found in crate::host::program::every_among(
        std::path::Path::new("pitboard"),
        std::env::split_paths(&ctx.search_path()),
        &lookup,
        crate::host::proc::can_run,
    ) {
        let file = std::fs::canonicalize(&found).unwrap_or_else(|_| found.clone());
        if !files.iter().any(|seen| Os::Windows.same_path(seen, &file)) {
            files.push(file);
            copies.push(found);
        }
    }
    copies
}

// Each only where it is so, as a person on Windows would not otherwise find out.
pub(super) fn judge_windows_homes(facts: &Facts) -> Vec<Check> {
    let Some(windows) = &facts.windows else {
        return Vec::new();
    };
    let same = |a: &std::path::Path, b: &std::path::Path| Os::Windows.same_path(a, b);
    let mut checks = Vec::new();
    if let Some(local) = &windows.local_app_data
        && !same(&local.join("Pitboard"), &windows.default_pitboard_home)
    {
        checks.push(warn(
            "local_app_data",
            "local app data",
            format!(
                "Windows keeps this account's local app data in {}",
                local.display()
            ),
            format!(
                "Pitboard keeps its folder in {} whatever Windows says, so a run of the \
                 renewal schedule and one by hand use the same folder.",
                windows.default_pitboard_home.display()
            ),
        ));
    }
    if windows.roams {
        checks.push(warn(
            "roaming_profile",
            "roaming profile",
            "this Windows profile roams",
            "Claude Code's and Codex's logins in it go to every computer this account signs in \
             to, as their Credential Manager items do. Pitboard's own folder, in AppData\\Local, \
             stays on this one.",
        ));
    }
    checks
}

// What changes what Pitboard or a tool does on Windows, each only where it is so.
pub(super) fn judge_windows_settings(facts: &Facts) -> Vec<Check> {
    let mut checks = Vec::new();
    let settings = facts.windows_settings.clone().unwrap_or_default();
    if !settings.wsl.is_empty() {
        // The ChatGPT app's WSL mode can hand a Linux codex this home (H9), not yet measured.
        checks.push(Check {
            advice: format!(
                "Claude Code and Codex installed there are installs of their own, which Pitboard \
                 on Windows does not switch. Claude Code there keeps its own login. Where Codex \
                 there uses {}, Codex's home here, a switch here changes its login too. WSL 2 \
                 runs them in a virtual machine, so Windows' list of processes does not show \
                 them running.",
                facts.codex.home.display()
            ),
            ..ok(
                "wsl",
                "WSL",
                format!("WSL holds {}", words::listed(settings.wsl)),
            )
        });
    }
    if let [first, rest @ ..] = facts.pitboards.as_slice()
        && !rest.is_empty()
    {
        let rest = rest.iter().map(|copy| copy.display().to_string());
        checks.push(Check {
            advice: "A shell runs the first, so updating another changes nothing there. Uninstall \
                     the copies you do not use."
                .into(),
            ..ok(
                "pitboard_copies",
                "copies of pitboard",
                format!("{} on PATH, then {}", first.display(), words::listed(rest)),
            )
        });
    }
    // hyper-util fills Codex's https proxy in from Windows' settings wherever HTTPS_PROXY is
    // unset or empty, whatever HTTP_PROXY and ALL_PROXY say (`codex_windows_system_proxy`).
    if let Some(server) = &settings.proxy
        && facts
            .network
            .sets(&["HTTPS_PROXY", "https_proxy"])
            .is_none()
    {
        let entries = system_proxies(server);
        let shown = entries
            .iter()
            .map(SystemProxy::masked)
            .collect::<Vec<_>>()
            .join(";");
        let codex = match entries.as_slice() {
            [one] if one.protocol.is_none() => "Codex sends its requests through it".to_string(),
            [one] => format!(
                "Codex reads the setting as one address, so it sends its requests to a proxy \
                 named {}, and they fail",
                one.masked()
            ),
            _ => format!(
                "Codex reads the setting as one address, which a proxy for each protocol is not, \
                 so its requests go {}",
                way(facts.network.sets(&["ALL_PROXY", "all_proxy"]))
            ),
        };
        // Claude Code reads no ALL_PROXY (`proxy_from_variables`).
        let claude = facts.network.sets(&["http_proxy", "HTTP_PROXY"]);
        let pitboard = facts.network.proxy.as_ref().map(|proxy| proxy.variable);
        let others = if claude.is_none() && pitboard.is_none() {
            "Claude Code and Pitboard follow only the proxy variables, so theirs go out directly"
                .to_string()
        } else {
            format!(
                "Claude Code's go {}, and Pitboard's {}",
                way(claude),
                way(pitboard)
            )
        };
        checks.push(Check {
            advice: format!(
                "{codex}. {others}: set HTTPS_PROXY where this network needs the proxy."
            ),
            ..ok(
                "system_proxy",
                "Windows proxy",
                format!("Windows' proxy settings name {shown}, and HTTPS_PROXY is not set"),
            )
        });
    }
    checks
}

pub(super) fn way(variable: Option<&str>) -> String {
    match variable {
        Some(variable) => format!("through the one {variable} names"),
        None => "out directly".to_string(),
    }
}

pub(super) fn judge_wsl(wsl: Option<&WslFact>) -> Option<Check> {
    let wsl = wsl?;
    let detail = match &wsl.distribution {
        Some(name) => format!("inside WSL, in {name}"),
        None => "inside WSL".to_string(),
    };
    let advice = match &wsl.codex_home_on_windows {
        None => "Pitboard here switches what is installed in this Linux. Claude Code and Codex \
                 installed on Windows keep their own logins, which Pitboard for Windows switches."
            .to_string(),
        // The ChatGPT app's WSL mode shares that folder with Codex on Windows (H9).
        Some(home) => {
            let (there, uses) = match on_windows(home) {
                Some(path) => (format!("{path} on Windows"), path),
                None => ("on a Windows drive".to_string(), "that folder".to_string()),
            };
            format!(
                "Pitboard here switches what is installed in this Linux. Claude Code installed \
                 on Windows keeps its own login, which Pitboard for Windows switches. Codex's \
                 home, {}, is {there}, where Codex here starts no shared background server. \
                 Where Codex on Windows uses {uses}, a switch here changes its login too.",
                home.display()
            )
        }
    };
    Some(Check {
        advice,
        ..ok("wsl", "WSL", detail)
    })
}

// `/mnt/c/x` is `C:\x`, as Codex's `win_path_to_wsl` maps the other way.
pub(super) fn on_windows(path: &std::path::Path) -> Option<String> {
    let rest = path.to_str()?.strip_prefix("/mnt/")?;
    let mut chars = rest.chars();
    let drive = chars.next().filter(char::is_ascii_alphabetic)?;
    let tail = chars.as_str();
    if !(tail.is_empty() || tail.starts_with('/')) {
        return None;
    }
    Some(format!(
        "{}:\\{}",
        drive.to_ascii_uppercase(),
        tail.trim_start_matches('/').replace('/', "\\")
    ))
}

// A path to pitboard.exe, or a script of the person's own, needs no PATH of Pitboard's.
pub(super) fn runs_bare_pitboard(command: &str) -> bool {
    command
        .split_whitespace()
        .next()
        .map(|program| program.trim_matches(['"', '\'']).to_ascii_lowercase())
        .is_some_and(|program| program == "pitboard" || program == "pitboard.exe")
}

// Claude Code's PATH is the one this runs with, as the shell it starts takes it.
pub(super) fn judge_status_line(line: Option<&StatusLineFact>) -> Option<Check> {
    let line = line?;
    let runs = match (&line.git_bash.found, &line.powershell) {
        (Some(bash), _) => format!("runs the status line in Git Bash, {}", bash.display()),
        (None, Some(claude::PowerShell::Seven(found))) => format!(
            "runs the status line in PowerShell 7, {}, finding no Git Bash",
            found.display()
        ),
        (None, Some(claude::PowerShell::Windows(found))) => format!(
            "runs the status line in Windows PowerShell, {}, finding no Git Bash",
            found.display()
        ),
        (None, None) => "finds neither Git Bash nor PowerShell to run the status line in".into(),
    };
    let passed_over = line
        .git_bash
        .passed_over
        .as_ref()
        .map_or_else(String::new, |named| {
            format!(
                "; it passes over {}, which CLAUDE_CODE_GIT_BASH_PATH names",
                named.display()
            )
        });
    Some(match &line.pitboard {
        Some(found) => ok(
            "status_line",
            "status line",
            format!(
                "Claude Code {runs}; `pitboard` there is {}{passed_over}",
                found.display()
            ),
        ),
        None if line.runs_pitboard => warn(
            "status_line",
            "status line",
            format!("Claude Code {runs}; no `pitboard` is on its PATH{passed_over}"),
            "Add the folder pitboard.exe is in to your PATH, then start Claude Code again.",
        ),
        None => ok(
            "status_line",
            "status line",
            format!("Claude Code {runs}; no `pitboard` is on its PATH{passed_over}"),
        ),
    })
}
