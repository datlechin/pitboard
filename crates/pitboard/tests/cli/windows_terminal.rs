// pitboard.exe in Windows' consoles and shells, as Claude Code, a task or a person starts it.
#![cfg(windows)]

use crate::common;
use crate::common::{Env, two_accounts};
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};

const GIT_BASH: &str = r"C:\Program Files\Git\bin\bash.exe";
const PWSH: &str = r"C:\Program Files\PowerShell\7\pwsh.exe";

// Windows' default, which RF-B1 read on both images and a shell started from Explorer has. The
// harness withholds it, since Pitboard reads it.
const PATHEXT: &str = ".COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC;.CPL";

// Only a run within 300 ms keeps the line up to date (the register's `statusline_run_timing`).
const BUDGET: Duration = Duration::from_millis(300);

#[allow(
    clippy::disallowed_methods,
    reason = "where Windows is, which every program is given as SystemRoot"
)]
fn windows_powershell() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
}

// As Claude Code 2.1.301 runs a status line in each (windows_statusline_shell).
fn shells() -> [(&'static str, PathBuf, &'static [&'static str]); 3] {
    const POWERSHELL: &[&str] = &[
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
    ];
    [
        ("Git Bash", PathBuf::from(GIT_BASH), &["-c"]),
        ("Windows PowerShell", windows_powershell(), POWERSHELL),
        ("pwsh", PathBuf::from(PWSH), POWERSHELL),
    ]
}

fn session(id: &str, limits: Option<(f64, f64)>) -> String {
    let mut session = serde_json::json!({ "session_id": id });
    if let Some((five_hour, seven_day)) = limits {
        session["rate_limits"] = serde_json::json!({
            "five_hour": {"used_percentage": five_hour, "resets_at": 4_000_000_000i64},
            "seven_day": {"used_percentage": seven_day, "resets_at": 4_000_000_000i64},
        });
    }
    session.to_string()
}

fn ran(mut command: std::process::Command, input: &str) -> std::process::Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("started");
    child
        .stdin
        .take()
        .expect("its input")
        .write_all(input.as_bytes())
        .expect("written");
    child.wait_with_output().expect("ended")
}

fn wait_for<T>(what: &str, mut found: impl FnMut() -> Option<T>) -> T {
    let started = Instant::now();
    loop {
        if let Some(found) = found() {
            return found;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "no {what} after 30 s"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn child_of(parent: u32, program: &str) -> Option<u32> {
    pitboard_core::testing::processes()
        .expect("Windows lists its processes")
        .into_iter()
        .find(|(_, of, name)| *of == parent && name.eq_ignore_ascii_case(program))
        .map(|(pid, _, _)| pid)
}

#[derive(Debug, PartialEq)]
enum Console {
    None,
    WithoutWindow,
    WithWindow,
}

// As another process finds it, by attaching to it: this one leaves the console it had first.
fn console_of(pid: u32) -> Console {
    use windows_sys::Win32::System::Console::{AttachConsole, FreeConsole, GetConsoleWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    // SAFETY: none takes a pointer, and the window is only asked whether it is visible.
    unsafe {
        FreeConsole();
        if AttachConsole(pid) == 0 {
            return Console::None;
        }
        let window = GetConsoleWindow();
        let shown = !window.is_null() && IsWindowVisible(window) != 0;
        FreeConsole();
        if shown {
            Console::WithWindow
        } else {
            Console::WithoutWindow
        }
    }
}

// `pitboard statusline` started by cmd.exe, itself started with `flags`, looked at while it waits
// for its input. A console it made would be there within moments of its start.
fn console_of_a_run_from_a_parent_started_with(env: &Env, flags: u32) -> Console {
    let mut parent = env.started("cmd.exe", &[]);
    parent
        .env("PATHEXT", PATHEXT)
        .raw_arg("/d /s /c \"pitboard statusline\"")
        .creation_flags(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut parent = parent.spawn().expect("cmd.exe starts");
    let pitboard = wait_for("pitboard.exe under cmd.exe", || {
        child_of(parent.id(), "pitboard.exe")
    });
    let looking = Instant::now();
    let mut seen = Console::None;
    while seen == Console::None && looking.elapsed() < Duration::from_secs(3) {
        seen = console_of(pitboard);
        std::thread::sleep(Duration::from_millis(100));
    }
    parent
        .stdin
        .take()
        .expect("its input")
        .write_all(b"{}")
        .expect("the session written");
    let out = parent.wait_with_output().expect("cmd.exe ends");
    assert!(out.status.success(), "{:?}", out.status);
    seen
}

// consoleAllocationPolicy=detached, which RF-E4 measured on the probe's builds.
#[test]
fn a_run_with_no_console_to_share_makes_none_and_one_shares_its_parents() {
    let env = Env::new("console-allocation");
    env.put_pitboard_on_path();
    assert_eq!(
        console_of_a_run_from_a_parent_started_with(&env, DETACHED_PROCESS),
        Console::None
    );
    assert_eq!(
        console_of_a_run_from_a_parent_started_with(&env, CREATE_NO_WINDOW),
        Console::WithoutWindow
    );
}

// What a status line's `line` prints, run in `shell` as Claude Code runs it there, from `cwd`
// where given, with `more` folders last on its PATH.
#[allow(clippy::too_many_arguments)]
fn shown(
    env: &Env,
    which: &str,
    shell: &Path,
    args: &[&str],
    cwd: Option<&Path>,
    line: &str,
    input: &str,
    more: &[&Path],
) -> String {
    assert!(shell.is_file(), "{which} is not at {}", shell.display());
    let mut command = env.started(shell, args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let path = common::os::search_path(&env.root.join("bin"));
    let mut folders: Vec<PathBuf> = std::env::split_paths(&path).collect();
    folders.extend(more.iter().map(|folder| folder.to_path_buf()));
    // Claude Code puts Git Bash's own folder first on the line's PATH.
    if which == "Git Bash" {
        folders.insert(0, shell.parent().expect("its folder").to_path_buf());
    }
    let path = std::env::join_paths(folders).expect("a PATH");
    command.arg(line).env("PATHEXT", PATHEXT).env("PATH", path);
    let out = ran(command, input);
    assert!(
        out.status.success(),
        "{which}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap_or_else(|e| panic!("{which}: {e}"))
}

// The line Claude Code shows is the same in each shell it may run it in, colours and all, with
// the session's numbers read from the input the shell hands on.
#[test]
fn the_status_line_runs_in_git_bash_and_both_powershells_as_claude_code_runs_it() {
    let env = two_accounts("statusline-shells");
    env.put_pitboard_on_path();
    for (which, shell, args) in shells() {
        let line = |input: &str| {
            shown(
                &env,
                which,
                &shell,
                args,
                None,
                "pitboard statusline",
                input,
                &[],
            )
        };
        let id = format!("pane {which}");
        let opened = line(&session(&id, None));
        assert_eq!(
            anstream::adapter::strip_str(&opened).to_string().trim_end(),
            "alpha ?·?  beta ?·?",
            "{which}"
        );
        let answered = line(&session(&id, Some((46.0, 70.0))));
        assert!(answered.contains("\u{1b}["), "{which}: {answered:?}");
        assert_eq!(
            anstream::adapter::strip_str(&answered)
                .to_string()
                .trim_end(),
            "alpha? 46%·70%  beta ?·?",
            "{which}"
        );
    }
}

// A PowerShell status line of your own, started with -File as Claude Code's documentation shows,
// hands its input on as docs/guides/status-line.mdx says, from a folder outside ASCII. Bypass,
// which that documentation leaves out, keeps the runner's execution policy out of it.
#[test]
fn a_powershell_status_line_of_your_own_hands_its_input_to_pitboard() {
    let env = two_accounts("statusline-combined");
    env.put_pitboard_on_path();
    let folder = env.root.join("bé");
    std::fs::create_dir_all(&folder).expect("the folder");
    let script = folder.join("statusline.ps1");
    std::fs::write(
        &script,
        "$json = $input | Out-String\n\
         $session = $json | ConvertFrom-Json\n\
         Write-Host \"mine $($session.session_id)\"\n\
         $json | pitboard statusline\n",
    )
    .expect("the script");
    // Forward slashes, since Git Bash takes a backslash for an escape.
    let script = script.display().to_string().replace('\\', "/");
    let (powershell, pwsh) = (windows_powershell(), PathBuf::from(PWSH));
    let folders = [&powershell, &pwsh].map(|program| program.parent().expect("its folder"));
    for runs in ["powershell", "pwsh"] {
        let own = format!("{runs} -NoProfile -ExecutionPolicy Bypass -File \"{script}\"");
        for (which, shell, args) in shells() {
            let id = format!("{runs} in {which}");
            let lines = |limits| {
                let mut input: serde_json::Value =
                    serde_json::from_str(&session(&id, limits)).expect("a session");
                input["cwd"] = serde_json::json!(folder);
                let out = shown(
                    &env,
                    which,
                    &shell,
                    args,
                    Some(&folder),
                    &own,
                    &input.to_string(),
                    &folders,
                );
                anstream::adapter::strip_str(&out)
                    .to_string()
                    .lines()
                    .map(|line| line.trim_end().to_owned())
                    .collect::<Vec<_>>()
            };
            let mine = format!("mine {id}");
            assert_eq!(lines(None), [&*mine, "alpha ?·?  beta ?·?"], "{id}");
            assert_eq!(
                lines(Some((46.0, 70.0))),
                [&*mine, "alpha? 46%·70%  beta ?·?"],
                "{id}"
            );
        }
    }
}

// W2's escaping keeps every byte ASCII, so a PowerShell that decodes a program's output in its
// console's code page parses the label as it was enrolled.
#[test]
fn json_parses_in_windows_powershell_and_pwsh_with_a_label_outside_ascii() {
    let env = two_accounts("json-powershell");
    env.put_pitboard_on_path();
    let (_, err, code) = env.run(&["rename", "beta", "Đạt"]);
    assert_eq!(code, 0, "{err}");
    let script = "(pitboard status --offline --json | ConvertFrom-Json).data.accounts | \
                  ForEach-Object { ($_.label.ToCharArray() | ForEach-Object { [int]$_ }) -join '.' }";
    for shell in [windows_powershell(), PathBuf::from(PWSH)] {
        let mut command = env.started(&shell, &["-NoProfile", "-NonInteractive", "-Command"]);
        command.arg(script).env("PATHEXT", PATHEXT);
        let out = command.output().expect("PowerShell runs");
        assert!(
            out.status.success(),
            "{}: {}",
            shell.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let labels: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|units| {
                let units: Vec<u16> = units
                    .trim()
                    .split('.')
                    .map(|unit| unit.parse::<u16>().expect("a UTF-16 unit"))
                    .collect();
                String::from_utf16(&units).expect("UTF-16")
            })
            .collect();
        assert_eq!(labels, ["alpha", "Đạt"], "{}", shell.display());
    }
}

// J2, on windows-11-arm with Defender on: each run's numbers moved, so each writes under
// usage.lock, as after every response. Runs leave the folder pitboard.exe is in free to replace.
#[test]
fn a_warm_status_line_run_is_within_its_budget_and_holds_nothing_after() {
    let env = two_accounts("statusline-budget");
    let pitboard = env.put_pitboard_on_path();
    let timed = |args: &[&str], input: &str| {
        let started = Instant::now();
        let out = ran(env.started(&pitboard, args), input);
        let took = started.elapsed();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        took
    };
    let run = |n: u32| {
        let input = session("budget", Some((f64::from(n), 70.0)));
        timed(&["statusline"], &input)
    };
    for n in 0..3 {
        run(n);
    }
    // A busy runner only adds time, so the quickest start is what a start costs.
    let (starts, mut over): (Vec<Duration>, Vec<Duration>) = (3..23)
        .map(|n| {
            let start = timed(&["--version"], "");
            (start, run(n).saturating_sub(start))
        })
        .unzip();
    let quickest = *starts.iter().min().expect("a start");
    over.sort();
    let median = over[over.len() / 2];
    println!(
        "{} warm runs of pitboard statusline: quickest start {quickest:?}, median over a start \
         {median:?}",
        over.len()
    );
    assert!(
        quickest + median <= BUDGET,
        "starts {starts:?}, over them {over:?}"
    );

    let bin = pitboard.parent().expect("its folder").to_path_buf();
    let replaced = env.root.join("bin-replaced");
    wait_for("the folder pitboard.exe ran from let go", || {
        std::fs::rename(&bin, &replaced).ok()
    });
}

// J3: sessions side by side each take usage.lock to write what they passed, and none loses
// another's.
#[test]
fn status_lines_side_by_side_each_keep_their_session() {
    let env = two_accounts("statusline-side-by-side");
    let pitboard = env.put_pitboard_on_path();
    let started = Instant::now();
    let mut children: Vec<_> = (0..8)
        .map(|_| {
            env.started(&pitboard, &["statusline"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("started")
        })
        .collect();
    for (n, child) in children.iter_mut().enumerate() {
        let input = session(&format!("pane {n}"), Some((10.0 + n as f64, 70.0)));
        let mut stdin = child.stdin.take().expect("its input");
        stdin.write_all(input.as_bytes()).expect("written");
    }
    for child in children {
        let out = child.wait_with_output().expect("ended");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let all = started.elapsed();
    println!("8 status lines side by side: {all:?}");
    let kept: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(env.root.join("pitboard").join("sessions.json"))
            .expect("sessions.json"),
    )
    .expect("JSON");
    for n in 0..8 {
        assert!(kept.get(format!("pane {n}")).is_some(), "pane {n}: {kept}");
    }
    assert!(all < BUDGET * 8 * 4, "{all:?}");
}

// mintty hands a program its pty as named pipes, which std takes for a terminal by their names
// (docs/facts/windows-runners.md).
#[test]
fn forget_asks_under_mintty_as_in_a_console_and_a_refusal_names_yes() {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{PIPE_ACCESS_INBOUND, PIPE_ACCESS_OUTBOUND};
    use windows_sys::Win32::System::Pipes::{CreateNamedPipeW, PIPE_TYPE_BYTE, PIPE_WAIT};

    let env = two_accounts("confirm-mintty");
    let (_, err, code) = env.run(&["forget", "beta"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("`--yes`"), "{err}");

    let pty = format!("msys-{:016x}-pty0", std::process::id());
    let name = |end: &str| format!(r"\\.\pipe\{pty}-{end}");
    let server = |name: &str, access| {
        let wide: Vec<u16> = std::ffi::OsStr::new(name)
            .encode_wide()
            .chain([0])
            .collect();
        // SAFETY: a NUL-terminated name and no security attributes.
        let handle = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                access,
                PIPE_TYPE_BYTE | PIPE_WAIT,
                1,
                4096,
                4096,
                0,
                std::ptr::null(),
            )
        };
        assert_ne!(
            handle,
            INVALID_HANDLE_VALUE,
            "{name}: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: a pipe this process just made and now owns alone.
        unsafe { std::fs::File::from_raw_handle(handle) }
    };
    let mut answer = server(&name("from-master"), PIPE_ACCESS_OUTBOUND);
    let mut asked = server(&name("to-master"), PIPE_ACCESS_INBOUND);
    let stdin = std::fs::OpenOptions::new()
        .read(true)
        .open(name("from-master"))
        .expect("the pty's input");
    let stderr = std::fs::OpenOptions::new()
        .write(true)
        .open(name("to-master"))
        .expect("the pty's output");
    let mut forget = env.command(&["forget", "beta"]);
    // mintty's own default. Without TERM anstream writes to a terminal through the console API,
    // which a pipe refuses (docs/facts/windows-runners.md).
    forget
        .env("TERM", "xterm")
        .stdin(stdin)
        .stderr(stderr)
        .stdout(Stdio::piped());
    let child = forget.spawn().expect("started");
    drop(forget);

    let (said, heard) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut chunk = [0u8; 1024];
        while let Ok(n) = asked.read(&mut chunk) {
            if n == 0 || said.send(chunk[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut question = Vec::new();
    // A failed test's output shows what came before the timeout.
    wait_for("question", || {
        while let Ok(chunk) = heard.try_recv() {
            println!("heard {:?}", String::from_utf8_lossy(&chunk));
            question.extend(chunk);
        }
        String::from_utf8_lossy(&question)
            .ends_with("[y/N] ")
            .then_some(())
    });
    answer.write_all(b"n\n").expect("answered");
    drop(answer);
    let out = child.wait_with_output().expect("ended");
    let _ = reader.join();
    assert!(out.status.success(), "{:?}", out.status);
    assert!(
        String::from_utf8_lossy(&question).starts_with("Forget beta and delete its parked login?"),
        "{}",
        String::from_utf8_lossy(&question)
    );
    let accounts = env.state()["accounts"].to_string();
    assert!(
        accounts.contains("\"beta\""),
        "said no, and beta is gone: {accounts}"
    );
}

// Through a pseudo console, as Windows Terminal hosts one, anstream turns on the console's VT
// processing, and std writes the bars' glyphs as UTF-16.
#[test]
fn a_console_run_draws_colours_and_the_bars_glyphs() {
    let env = two_accounts("console-glyphs");
    let (code, drawn) =
        in_a_pseudo_console(&env, Path::new(env!("CARGO_BIN_EXE_pitboard")), &["status"]);
    assert_eq!(code, 0, "{drawn}");
    assert!(drawn.contains('█') && drawn.contains('░'), "{drawn:?}");
    assert!(has_a_colour(&drawn), "{drawn:?}");
}

// A foreground colour in any SGR sequence: 30 to 37, 90 to 97, or 38 for an extended one.
fn has_a_colour(drawn: &str) -> bool {
    drawn.split("\u{1b}[").skip(1).any(|sequence| {
        let Some(end) = sequence.find(|c: char| !(c.is_ascii_digit() || c == ';')) else {
            return false;
        };
        sequence[end..].starts_with('m')
            && sequence[..end]
                .split(';')
                .any(|parameter| matches!(parameter.parse::<u32>(), Ok(30..=38 | 90..=97)))
    })
}

// `program` with `args` in a pseudo console of its own, with the harness's environment: its exit
// code and everything the console drew, as UTF-8 with its VT sequences.
fn in_a_pseudo_console(env: &Env, program: &Path, args: &[&str]) -> (u32, String) {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Console::{
        COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON,
    };
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::{
        CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
        EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
        PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES,
        STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
    };

    let pipe = || {
        let (mut read, mut write): (HANDLE, HANDLE) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: two handles of this frame, no attributes, the default size.
        let made = unsafe { CreatePipe(&mut read, &mut write, std::ptr::null(), 0) };
        assert_ne!(made, 0, "{}", std::io::Error::last_os_error());
        // SAFETY: both ends were just made, and are this process's alone.
        unsafe {
            (
                OwnedHandle::from_raw_handle(read),
                OwnedHandle::from_raw_handle(write),
            )
        }
    };
    let (console_reads, keyboard) = pipe();
    let (screen, console_writes) = pipe();
    let mut console: HPCON = 0;
    // SAFETY: the two ends the console keeps copies of, and a handle of this frame for it.
    let made = unsafe {
        CreatePseudoConsole(
            COORD { X: 120, Y: 40 },
            console_reads.as_raw_handle(),
            console_writes.as_raw_handle(),
            0,
            &mut console,
        )
    };
    assert_eq!(made, 0, "CreatePseudoConsole: {made:#x}");
    drop((console_reads, console_writes));

    let mut size = 0usize;
    // SAFETY: a null list asks only for the size one attribute needs.
    unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size) };
    let mut list = vec![0u8; size];
    let attributes = list.as_mut_ptr().cast();
    // SAFETY: a buffer of the size asked for, and the console, which outlives the list.
    unsafe {
        assert_ne!(
            InitializeProcThreadAttributeList(attributes, 1, 0, &mut size),
            0,
            "{}",
            std::io::Error::last_os_error()
        );
        assert_ne!(
            UpdateProcThreadAttribute(
                attributes,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                console as *const core::ffi::c_void,
                size_of::<HPCON>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            ),
            0,
            "{}",
            std::io::Error::last_os_error()
        );
    }

    // Invalid standard handles, or the program would write to this test's own redirected ones.
    let mut start = STARTUPINFOEXW::default();
    start.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    start.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    start.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
    start.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
    start.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
    start.lpAttributeList = attributes;
    let mut line: Vec<u16> = format!("\"{}\" {}", program.display(), args.join(" "))
        .encode_utf16()
        .chain([0])
        .collect();
    let mut variables = env.environment();
    variables.sort_by_key(|(name, _)| name.to_string_lossy().to_uppercase());
    let mut block: Vec<u16> = Vec::new();
    for (name, value) in variables {
        block.extend(name.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    let mut started = PROCESS_INFORMATION::default();
    // SAFETY: a writable command line, an environment block ended by two NULs, the start info
    // with its attribute list, and a PROCESS_INFORMATION of this frame.
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            block.as_ptr().cast(),
            std::ptr::null(),
            &start.StartupInfo,
            &mut started,
        )
    };
    assert_ne!(created, 0, "{}", std::io::Error::last_os_error());
    // SAFETY: the handles CreateProcessW gave this process.
    let (process, _thread) = unsafe {
        (
            OwnedHandle::from_raw_handle(started.hProcess),
            OwnedHandle::from_raw_handle(started.hThread),
        )
    };

    let mut screen = std::fs::File::from(screen);
    let reader = std::thread::spawn(move || {
        let mut drawn = Vec::new();
        let _ = screen.read_to_end(&mut drawn);
        drawn
    });
    let mut code = 0u32;
    // SAFETY: the process's own handle, open until `process` drops.
    let ended = unsafe {
        let ended = WaitForSingleObject(process.as_raw_handle(), 60_000) == WAIT_OBJECT_0;
        GetExitCodeProcess(process.as_raw_handle(), &mut code);
        ended
    };
    // Closing the console ends its output, which ends the reader, and a program still on it.
    // SAFETY: the console made above, closed once; the list, set up above and no longer used.
    unsafe {
        ClosePseudoConsole(console);
        DeleteProcThreadAttributeList(attributes);
    }
    drop(keyboard);
    let drawn = String::from_utf8_lossy(&reader.join().expect("the console's output")).into_owned();
    assert!(ended, "still running after 60 s: {drawn:?}");
    (code, drawn)
}
