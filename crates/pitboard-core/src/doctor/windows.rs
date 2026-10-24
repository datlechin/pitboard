use super::{Check, Facts, ok, warn, words};
use crate::context::Context;
use crate::host::Os;
use crate::provider::claude::paths as claude;
use crate::provider::codex::paths as codex;
use crate::proxy::{SystemProxy, system_proxies};
use std::path::PathBuf;

pub(crate) struct WslFact {
    pub(crate) distribution: Option<String>,
    // Codex's home, where it is on a drive of Windows'.
    pub(crate) codex_home_on_windows: Option<PathBuf>,
}

// Both shells Claude Code runs the status line in take `pitboard.exe` for a bare `pitboard`.
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

// As Codex's Linux build tells WSL (`codex_wsl`).
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
            words::local_app_data_in(local.display()),
            words::kept_in_one_folder(windows.default_pitboard_home.display()),
        ));
    }
    if windows.roams {
        checks.push(warn(
            "roaming_profile",
            "roaming profile",
            words::THE_PROFILE_ROAMS,
            words::LOGINS_GO_WITH_IT,
        ));
    }
    checks
}

// What changes what Pitboard or a tool does on Windows, each only where it is so.
pub(super) fn judge_windows_settings(facts: &Facts) -> Vec<Check> {
    let mut checks = Vec::new();
    let settings = facts.windows_settings.clone().unwrap_or_default();
    if !settings.wsl.is_empty() {
        // The ChatGPT app's WSL mode may hand a Linux codex this home (docs/facts/not-measured.md).
        checks.push(Check {
            advice: words::installs_of_their_own(facts.codex.home.display()),
            ..ok("wsl", "WSL", words::wsl_holds(&settings.wsl))
        });
    }
    if let [first, rest @ ..] = facts.pitboards.as_slice()
        && !rest.is_empty()
    {
        let rest: Vec<String> = rest.iter().map(|copy| copy.display().to_string()).collect();
        checks.push(Check {
            advice: words::UNINSTALL_THE_OTHERS.into(),
            ..ok(
                "pitboard_copies",
                "copies of pitboard",
                words::copies_on_path(first.display(), &rest),
            )
        });
    }
    // Codex takes Windows' setting wherever HTTPS_PROXY is unset (`codex_windows_system_proxy`).
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
            [one] if one.protocol.is_none() => words::CODEX_GOES_THROUGH_IT.to_string(),
            [one] => words::codex_fails_through(&one.masked()),
            _ => words::codex_goes(&words::the_way(
                facts.network.sets(&["ALL_PROXY", "all_proxy"]),
            )),
        };
        // Claude Code reads no ALL_PROXY (`proxy_from_variables`).
        let claude = facts.network.sets(&["http_proxy", "HTTP_PROXY"]);
        let pitboard = facts.network.proxy.as_ref().map(|proxy| proxy.variable);
        let others = if claude.is_none() && pitboard.is_none() {
            words::THE_OTHERS_GO_DIRECTLY.to_string()
        } else {
            words::the_others_go(&words::the_way(claude), &words::the_way(pitboard))
        };
        checks.push(Check {
            advice: words::set_https_proxy(&codex, &others),
            ..ok("system_proxy", "Windows proxy", words::system_proxy(&shown))
        });
    }
    checks
}

pub(super) fn judge_wsl(wsl: Option<&WslFact>) -> Option<Check> {
    let wsl = wsl?;
    let advice = match &wsl.codex_home_on_windows {
        None => words::WINDOWS_KEEPS_ITS_LOGINS.to_string(),
        // The ChatGPT app's WSL mode shares that folder with Codex on Windows (H9).
        Some(home) => words::codex_home_on_windows(home.display(), on_windows(home).as_deref()),
    };
    Some(Check {
        advice,
        ..ok("wsl", "WSL", words::inside_wsl(wsl.distribution.as_deref()))
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
        (Some(bash), _) => words::in_git_bash(bash.display()),
        (None, Some(claude::PowerShell::Seven(found))) => words::in_powershell_7(found.display()),
        (None, Some(claude::PowerShell::Windows(found))) => {
            words::in_windows_powershell(found.display())
        }
        (None, None) => words::NO_SHELL.into(),
    };
    let passed_over = line
        .git_bash
        .passed_over
        .as_ref()
        .map_or_else(String::new, |named| words::passes_over(named.display()));
    Some(match &line.pitboard {
        Some(found) => ok(
            "status_line",
            "status line",
            words::pitboard_there(&runs, found.display(), &passed_over),
        ),
        None if line.runs_pitboard => warn(
            "status_line",
            "status line",
            words::no_pitboard_there(&runs, &passed_over),
            words::ADD_PITBOARD_TO_PATH,
        ),
        None => ok(
            "status_line",
            "status line",
            words::no_pitboard_there(&runs, &passed_over),
        ),
    })
}
