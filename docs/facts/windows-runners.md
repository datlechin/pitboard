# Windows runners

Measured on GitHub's runners only, by `pitboard-probe` and `.github/scripts/runner-facts.ps1`
in `windows-facts.yml` run 38693947329 on 15 October 2026, unless a line names another run.
Every run met the same images: windows-2025 (Windows Server 2025 Datacenter, 24H2, build
26100, UBR 33438) and windows-11-arm (build 26200, UBR 9457, 25H2). The script runs the
probe as the job's user and as three throwaway users it makes: a standard user, a standard
user whose name is not ASCII, and a member of Administrators. Its Bun is 1.4.3 from Bun's
release, with proper-lockfile 4.1.2; which Bun each Claude Code build embeds is in its
register. A fact holds on both images unless it
names one. The tokens these list, and the session machine's, are the only ones the Windows face
has been seen to read.

Accounts and tokens:

- The job's user has a token whose `TokenElevationType` is the default one, a token that is
  not split, and that is elevated, at high integrity. Files it makes are owned by
  Administrators. So every program the job starts is elevated, and Pitboard reads it as
  `token::IN_EVERY_PROGRAM`.
- A Safer normal-user token computed from the job's token, with `SaferComputeTokenFromLevel`
  at `SAFER_LEVELID_NORMALUSER`, is elevated still, at high integrity, and so is a program
  started with it. It cannot stand in for a person on the runners.
- UAC is on: `EnableLUA` is 1 and `ConsentPromptBehaviorAdmin` 0. A member of
  Administrators started with `Start-Process -Credential -LoadUserProfile` gets the limited
  token of a split pair, not elevated, at medium integrity, and owns the files it makes.
- A local standard user made in the job, and started with `Start-Process -Credential
  -LoadUserProfile`, its output sent to files, runs an unsigned program, exiting 0, never
  0xC0000142. Its token is the default type, not elevated, at medium integrity, so Pitboard
  reads it as the person. Started without `-Environment`, the program gets the job user's
  variables; in run 37660991344 that left the user's local app data naming the job user's
  folder, which it cannot open, and `SHGetKnownFolderPath` failed. Given its own profile's
  `USERPROFILE`, `HOMEDRIVE`, `HOMEPATH`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP` and
  `USERNAME`, every known folder reads as its own. It can read and run what the job built in
  the workspace. This is how CI runs every Windows test, through
  `.github/scripts/test-as-standard-user.ps1`.
- That user's programs run in the job user's session, 2, and start inside a job already.
- WebView2 tries that user's password blank once for each new user data folder:
  `msedgewebview2.exe` calls `LogonUser` with an empty password, logon type 2, as Chromium's
  `CheckBlankPasswordWithPrefs` does, and the Security log records a failed logon
  (0xC000006A). Read on 154.0.4258.62 on windows-2025 and 155.0.4283.45 on windows-11-arm, in
  run 38825074643 thirteen folders made thirteen. With a folder per WinUI test the user was
  locked out on both images after ten in run 38822700418, so those tests share one user data
  folder for the run, each under profiles of its own. The UI tests' launches share one too,
  named to a Debug build in `PITBOARD_FIXTURE_WEBVIEW2`: a fixture's own folder per launch
  locked the user out on both images in run 38874616742.
- CF4: the WebView2 Runtime is 154.0.4258.62 on windows-2025 and 155.0.4283.45 on
  windows-11-arm, read from `pv` under the machine's EdgeUpdate client key in run
  38825074643.
- CF7, WebView2's downloads, on both images in run 38825074643: a page whose response says
  `Content-Disposition: attachment` raised `NavigationStarting`, then `NavigationCompleted`
  unsuccessful with `ConnectionAborted`, then `DownloadStarting`, and a frame of another
  host did the same with `FrameNavigationStarting` and `FrameNavigationCompleted`; a `blob:`
  or `data:` link raised `DownloadStarting` alone. An `<a download>` of a `pitboard-fixture`
  address raised nothing at all, as in runs 38819589926 and 38822700418. For a second
  `archive.tar.gz` WebView2 offered and saved `archive (1).tar.gz`. A paused download is
  `Interrupted`, `UserPaused`, and can resume.
- CF7: a page's own script that leaves the site raises `NavigationStarting` with
  `IsUserInitiated` false, on both images in run 38825074643.
- CF7, a deleted profile, on both images in run 38842851059: `CoreWebView2Profile.Delete`
  had the name refused with `ERROR_DELETE_PENDING` at once, and the folder outlived its
  browser process, holding `Platform Notifications` and `WebStorage` files on windows-2025
  and `DIPS`, `DashTrackerDatabase` and `Network Action Predictor` ones on windows-11-arm. A
  new environment over the user data folder left it for 20 s; a page of another profile
  started a browser process, and the folder went 5.1 s later. Microsoft's reference has it
  go as the browser process exits, or at a later start where something held its files.
- CF7, a deleted profile, in run 38852781290: on windows-2025 the folder went as its browser
  process exited, and the next browser process refused the name at once all the same; on
  windows-11-arm it outlived its browser process, holding `DIPS`, `DashTrackerDatabase` and
  `Network Action Predictor` files, and went 5127 ms after the next one started.
- CF7, a deleted profile, on both images in runs 38854886274 and 38856204289: with a page of
  another profile keeping the next browser process, that process refused the name for 5.2 to
  5.9 s after it started, took it again as the folder went or just after, and made a profile
  with none of the old one's cookies.
- WinUI's `WebView2` reported a page it could not make, a profile name WebView2 refused,
  only through `CoreWebView2Initialized`, and left `CoreWebView2` null once
  `EnsureCoreWebView2Async` had returned, on both images in run 38854886274.
- CF7, on both images in runs 38842851059 and 38846774768: a navigation that
  `NavigationStarting` cancelled still sent its own request, from a page of the fixture's
  scheme and from one on http, with `Sec-Fetch-Mode: navigate`, `Sec-Fetch-Dest: document`
  and no `Sec-Purpose`. WebView2 raised `NavigationStarting` first, then
  `WebResourceRequested` for it as a `Document` request, where the window's answer kept it
  from the stand-in site.
- CF7, downloads, on both images in run 38842851059: an `<a download>` in a frame of the
  stand-in site raised `FrameNavigationStarting`, a successful `FrameNavigationCompleted`,
  then `DownloadStarting` for the file. A download whose page was the last to close raised
  no `StateChanged`, and its browser process exited; one whose page closed while another
  kept the browser process was still in progress five seconds later.
- CF7, the download list, on both images in run 38872384945: WebView2's own downloads dialog
  listed each file with Open file, and offered Open downloads folder and Search downloads,
  with no Show in folder of a file's own.
- CF8, sparse packages, as the fresh standard user on both images in run 38928930271: with a
  throwaway certificate trusted in `LocalMachine\TrustedPeople` by the job's user,
  `Add-AppxPackage` registered the probe's package from either manifest for that user, status
  `Ok`, signature `Developer`. `pitboard-probe-sparse.exe` in its external location ran with
  the package's identity and an AUMID. The plain probe and a stand-in `claude.exe` it started
  had none, and their writes and its own, a file under `%LOCALAPPDATA%\Pitboard` and a value
  under HKCU, reached the real folder and key with either manifest, none the package's
  folder. `LaunchUriAsync` of the probe's link scheme from the identity process gave false and
  started no handler. With the certificate in the user's own `TrustedPeople`,
  `Add-AppxPackage` refused the package with 0x800B0109 (run 38882775456).
- CF8, the hand-off, as the fresh standard user on both images in runs 39291962999 and
  39301311398: the probe's identity process started the plain probe through `CreateProcessW`
  with a `pitboard-probe-cf8://open?url=…` link as its only argument. With either manifest,
  the child exited 0, got the link whole and had no package identity.
- CF8, PitboardShare's package, as the fresh standard user on both images in run
  38928930271: `PitboardShare.exe --register` from a Release publish, its package signed with
  that certificate, registered `PitboardShare` with the publish's folder as its external
  location, status `Ok`, and exited 0. `Pitboard.exe --unregister-share` from that folder
  exited 0, 4888 ms after its start on windows-2025 and 1245 ms on windows-11-arm, and left
  the user no `PitboardShare` package.
- CF9, Restart Manager, as the fresh standard user on both images in run 38895887782:
  `RmShutdown` without `RmForceShutdown`, given only the GUI process of a stand-in app the
  tests started then, ended a stand-in that answers `WM_QUERYENDSESSION` with `TRUE` and
  quits on `WM_ENDSESSION` with `ENDSESSION_CLOSEAPP`, and gave `ERROR_SUCCESS` after 37 ms
  on windows-2025 and 46 ms on windows-11-arm. A stand-in that answers `FALSE` ran on with
  its window, and `RmShutdown` gave `ERROR_FAIL_SHUTDOWN` after 15 and 19 ms. Each
  stand-in's console child, a `cmd.exe` never registered, ran on.
- A process that gave `RegisterApplicationRestart` `--updated` with `RESTART_NO_CRASH`,
  `RESTART_NO_HANG` and `RESTART_NO_REBOOT` reads them back through
  `GetApplicationRestartSettings`, which gives `HRESULT_FROM_WIN32(ERROR_NOT_FOUND)` before
  it registers (run 38895887782).
- Explorer, the job user's, takes no notification-area icon of that user's: `NIM_ADD` fails
  for an icon named by its window and id and for one named by a GUID (CI run 38753437460).
  So no UI test clicks Pitboard's icon: the view tests hold its menu, `ScreenshotTests` opens
  it with the message a right click sends, and the session's step A2 clicks it.
- A window of that user's program comes to the front and takes a click on windows-2025. On
  windows-11-arm a full-screen `Microsoft account` window, a `Windows.UI.Core.CoreWindow` of
  `WWAHost` in the job user's session, holds the foreground, and Windows keeps it there for a
  program the tests start, as `SetForegroundWindow` documents for a process not started by
  the one in front (runs 38795330950 and 38800569975). A click at Pitboard's window reaches
  that window (run 38797997319). So the test of the first launch is inconclusive where the
  window in front before the launch keeps the foreground, and no test there gives Pitboard
  real input. The UI tests type through the Value pattern, press through the Invoke pattern,
  and there ask for a row's shortcut menu through `IUIAutomationElement3::ShowContextMenu`,
  which WinUI answers with the row's `ContextRequested` (run 38865911995). Ctrl+N and the find
  bar, tests of a key itself, are inconclusive there.
- FlaUI.UIA3 5.0.0, run as that user, finds that user's WinUI elements by AutomationId and
  selects a NavigationView item through its SelectionItem pattern (CF2,
  `LifecycleTests.AWinUIElementIsFoundAndInvokedByItsAutomationId`, run 38804714545).
- A launch redirected to the running copy carries the second copy's arguments: a second Debug
  copy started with a link handed it to the one running, which opened its picker for it
  (`AccountPickerTests`, CI run 39254279757).
- The self-contained Debug app, started by that user, has its window up 1194 ms after the
  start on windows-11-arm (CF5, run 38804714545). Both images hold the Windows App Runtime
  2.5.1.0 package for some user, so that the app needs none is not measured here.
- The WinUI projects build in Debug and publish in Release for win-arm64 on windows-11-arm,
  XAML compiler included, so each architecture is built where it runs (CF6, runs
  38782142541 and 38804714545).
- Developer Mode is on, and that user makes file and directory symbolic links as it is. It
  makes them with the symbolic-link right granted too, but Developer Mode was on for that run
  as well, so the right alone is not measured.
- `RtlGetVersion` gives 10.0.26100 and 10.0.26200, the builds the registry's `CurrentBuild`
  gives. The registry's `ProductName` says `Windows 10 Enterprise` on windows-11-arm, so
  Pitboard never reads which Windows this is from it.
- `VerQueryValueW` reads a program's version resource: `cmd.exe`'s product version is 10.0
  with a build no later than the running Windows, and `pitboard.exe`'s names Pitboard and its
  version (`cmd_exe_carries_the_build_of_the_windows_it_ships_in` and
  `pitboard_exe_carries_its_version_resource`, CI run 39254279757).
- clap's parser of every subcommand outgrows the 1 MB main stack of a debug build: each probe
  exited 0xC00000FD (run 39316394518), so the probe links with an 8 MB stack.

Homes and variables:

- `GetProfileType` gives flags 0, a local profile, for every user.
- `FOLDERID_Profile` and `FOLDERID_LocalAppData` ignore `USERPROFILE` and `HOME`, and local
  app data is the profile's `AppData\Local`. `std::env::home_dir` follows `USERPROFILE` and
  ignores `HOME`.
- `os.homedir()` follows `USERPROFILE` in Node (22.23.3 on windows-2025, 24.21.0 on
  windows-11-arm) and in Bun. Bun's `os.userInfo().homedir` follows it too. Node's keeps the
  account's profile (run 38704825885).
- Git for Windows' `bash.exe -c` hands a program it starts `HOME` set to the profile, as a
  Windows path.
- A program gets only `Path`, and a child std's `Command` starts gets one `Path`. A raw
  environment block holding both `PATH` and `Path` hands both to the child, and `var_os`
  returns the first in block order for either spelling (run 38418961905).
- std's `Command` given a bare name runs the program beside the running one before one on the
  parent's `PATH`, and a `PATH` set for the child wins over both. The working folder is never
  searched, and a `.cmd` on `PATH` is never found by a bare name (run 38418961905).
- A task Task Scheduler starts gets this `PATHEXT`:
  `.COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC`. The runner's own programs carry
  `.CPL` after it, and pass it on to theirs.
- A value `setx` writes into the job user's `HKCU\Environment` reaches a task started after
  it.
- A value written straight into the job user's `HKCU\Environment` never reaches the
  environment of a program already running, even once `WM_SETTINGCHANGE` for `Environment` is
  broadcast as `setx` broadcasts it. That reaches the program's top-level window that is never
  shown, and not its message-only window. The Explorer already running starts a program with
  the new value before any broadcast (RF-T8, runs 39310064516 and 39320680285).
- A profile folder whose name is not ASCII, such as one holding `Đ` and `ạ` (10 UTF-16
  units, 13 UTF-8 bytes, NFC), has an ASCII 8.3 alias holding `~`, and so has the `Temp`
  inside it.
- `std::fs::canonicalize` of a folder in that profile gives the `\\?\` form with every long
  name, as stored and in NFC, whether the folder was named by its long or its 8.3 spelling.
  Without the prefix, it is the long spelling as given (run 38704825885).

Who may read a login:

- A profile folder is owned by SYSTEM, with a protected DACL granting SYSTEM,
  Administrators, the user and an app container SID. Its `AppData\Local`, the `Temp` there and
  a folder the user makes in it are the user's, inheriting SYSTEM, Administrators and the
  user. Nobody else can reach their data.
- A file Bun writes with mode 0o600 and then `chmod`s to 0o600, as Claude Code 2.1.300
  writes `.credentials.json`, keeps only the DACL it inherits. Bun reports its mode as 0o666.
- A file made with a protected DACL granting the user, SYSTEM and Administrators is owned by
  the user who made it, standard or limited administrator, and the elevated job user can
  read, write and delete it.
- A fresh administrator's limited token, in which Administrators is for deny only, gets from
  a file the elevated job user made, owned by Administrators with a protected DACL, what an
  ACE for that user grants and nothing an Administrators ACE does: read, write and delete
  with one for full control, read alone with one for reading, and nothing without one,
  `CreateFileW` failing 5. Delete works with read alone, since the folder it is in grants that
  user full control (RF-T8, run 39320680285).

Replacing a file:

- NTFS and ReFS report `FILE_SUPPORTS_POSIX_UNLINK_RENAME`. There the POSIX rename works over
  a target held open sharing delete and fails 32 over one held without. `MoveFileExW` fails
  5 whenever the target is held. The ReFS volume is plain on windows-2025 and a Dev Drive
  on windows-11-arm.
- exFAT and FAT32 lack the flag, and the POSIX rename fails 87 there. `MoveFileExW` works
  over a target nobody holds and fails 5 over a held one (run 38418961905).
- An SMB loopback share mapped as a drive reports NTFS, a remote drive and no POSIX flag,
  and renames as exFAT does.
- 10000 POSIX renames beside a probe reader and a Bun reader that share delete, Bun holding
  each open 50 ms, needed no retry: about 13 s on NTFS on both images, and 2000 took 1.4 s
  on ReFS. No reader saw a torn or empty file.
- A reader thread in the renaming process itself found the file missing in 10788 of its
  69694 opens during those renames on windows-2025, and in none on windows-11-arm. In later
  runs the probe reader in a process of its own did too, `CreateFileW` failing 2: in 5658 of
  208735 opens on windows-2025 (run 39320680285) and 2323 of 186195 on windows-11-arm (run
  40030964166). In run 38878569423 no reader found it missing on either image, and on ReFS
  none ever has. Every writer of `state.json` loads it under Pitboard's lock, so a read
  without the lock, the app's, the status line's or `pitboard status`'s, could take it for
  missing and show no accounts. On Windows `atomic::read`, which `state.json`'s load uses,
  looks again five times, 2 ms apart, before it takes a file it replaces for missing.
- Beside readers that reopen the file without a pause, one not sharing delete,
  `MoveFileExW` fails 5 most of the time. In 180 s it replaced the file 154 times on
  windows-2025, 30 rounds running out of 50 retries, and 2439 times on windows-11-arm, in up
  to 33 attempts. No reader saw a torn or empty file.

Names, ids and long paths (RF-T8, runs 39310064516 and 39320680285):

- `FILE_ID_INFO` gives a folder, a file, a hard link to the file and a junction to the folder
  one volume serial number, and ids by which the hard link is its file, the junction is its
  folder and the junction's own reparse point is another. A file keeps its id across a
  rename, and another folder or file has another. The ids take at most 51 bits on NTFS and 55
  on the SMB share, and 75 on ReFS, past 64. On the share a hard link is made and a junction
  is not.
- exFAT and FAT32 give no `FILE_ID_INFO`: `GetFileInformationByHandleEx` fails 87. Neither
  takes a hard link or a junction, so there `host::lies_in` compares by path alone.
- `GetFullPathNameW` of a name in a folder makes a device of `NUL` alone, `\\.\NUL`. `CON`,
  `con.txt`, `NUL.json`, `AUX.json`, `PRN.json`, `COM1`, `COM0.json`, `LPT1.json`, `COM¹.json`,
  `LPT².json`, `CONIN$`, `CONOUT$` and `CLOCK$.json` are names a file is made and kept by, as
  written. A trailing dot or space goes: `work.`, `work..`, `work ` and `work. .` make
  `work`, and `work.json.` makes `work.json`. The vault refuses each as a park's name, more
  than Windows asks on Pitboard's floor.
- Both images set `LongPathsEnabled` to 1. There a program whose manifest holds
  `pitboard.exe`'s `longPathAware` makes, opens and finds a file and a folder at 328
  characters through `CreateFileW`, `CreateDirectoryW` and `GetFileAttributesW`, with no
  `\\?\`, and `RtlAreLongPathsEnabled` says so. Without the manifest each call fails 3.
  std's own calls reach the folder either way, adding `\\?\` past `MAX_PATH`.

Flushing and times:

- A standard user can flush a folder opened for generic write with `FlushFileBuffers` on
  NTFS and ReFS, as the job user can on exFAT and FAT32. A handle opened for write
  attributes alone fails 5. On the SMB share the flush fails 1.
- NTFS, ReFS and the SMB share keep mtimes in 100 ns steps, exFAT and FAT32 in whole seconds
  (run 38418961905).
- J2, the test of a warm status line run's budget in `windows_terminal.rs`, runs alone,
  first after the build, and each of its runs writes `sessions.json`. In the 94 CI runs from
  39307046581 to 40083986071 that flushed it, J2's median past a start was 50 ms on
  windows-2025, over 250 ms 7 times in 112, and 54 ms on windows-11-arm, at most 69 ms.
  Unflushed, it was 43.0 and 45.9 ms (run 40081734815).
- Timed by step, with flushed and unflushed runs in turn (runs 40085923782 and
  40087183590), flushing the temporary took a median of 6.0 to 8.6 ms, at most 444 and
  268 ms on windows-2025 and 22 and 517 ms on windows-11-arm, and its folder 1.5 to 3.0 ms.
  A flushed run took up to 571 ms past a start, an unflushed one 61 ms. Beside a thread
  rewriting 1 GB unflushed, the flush took a median of 22 to 26 ms.
- 37 to 40 ms of every J2 run is `Context::with_scratch_claude`, which only a build for
  tests makes. Every other step of an unflushed run took under 2 ms.

Locks:

- proper-lockfile 4.1.2 under Bun and the probe's lock see each other's lock with both of
  Claude Code's timings: held at every one of 25 checks, never stale. Its heartbeat was at
  most 7.5 s old with the write lock and 5 s old with the refresh lock.
- With another process holding `LockFileEx` on `state.lock`, opening it sharing everything
  works and `ReadFile` fails 33. Opening it sharing read alone fails 32.
- On NTFS and ReFS, `remove_dir_all` removes a home while another process holds its
  `state.lock`. On exFAT and FAT32 it fails 145 and leaves the home, and works once the
  holder exits (run 38418961905).

DPAPI:

- `CryptProtectData` with the vault's flags seals in about 0.2 ms and unseals in 0.15 ms on
  windows-2025, 0.27 and 0.2 ms on windows-11-arm. A process's first seal takes 7 ms and
  1.3 ms.
- After the user changes its own password through ADSI, a program started with the new
  password unseals what was sealed before.
- After an administrator resets the password, nothing sealed before unseals: 0x8009000B.
- A task with the job user's interactive token, in its session, unseals what the job user
  sealed.
- A token made from the standard user's as Codex 0.162.1 makes its unelevated sandbox's
  (`codex_windows_sandbox_token`) is not elevated, at medium integrity. A program started with it
  unseals what the user sealed, its first unseal taking 1.5 ms on windows-11-arm and 1 ms on
  windows-2025. It opens a file whose protected DACL grants only the user, SYSTEM and
  Administrators for read and for delete, and fails 5 opening it for write (run
  38821312770). Without the capability SID, the default DACL and the desktop, its programs
  started but neither exited within 30 s nor wrote a report (run 38797729796).

Tasks:

- A plain console program started by a task, or with no console to inherit, gets a visible
  console window. Built with either spelling of the detached `consoleAllocationPolicy`, it
  gets none there, and still attaches to a parent's console (run 38418961905).
- A standard user can make `\Pitboard\` and register a task with an interactive token there
  or in the root. With no signed-in session of its own, the task never starts: its last
  result is 0x00041303.
- `GetFolder` of a missing folder and `GetTask` of a missing task fail with 0x80070002, and
  `CreateFolder` of a folder there with 0x800700B7. A second standard user opens and
  registers in a `\Pitboard\` the first made; the first's `DeleteFolder` fails with
  0x80070091 while the second's task is in it, and succeeds once it is gone (RF-E1 and
  RF-E1c, run 38922866517).
- The job user's task in either folder runs, with last result 0.
- A fresh standard user's `pitboard watch install` registers `\Pitboard\Watch <SID>`. Task
  Scheduler reads its trigger's and principal's user back as that user's name, the logon type
  as an interactive token at least privilege, and the time limit as `PT0S`. With no signed-in
  session its `Run` succeeds and starts nothing: the last result stays 0x00041303 and no
  `pitboard.exe` runs. Installing again at 90% and `watch uninstall` succeed, and the
  uninstall takes `\Pitboard\` with the task. The home keeps `audit.log` and `state.lock`
  (RF-E6, run 40030964166).
- The job user's tasks shaped as the watch service's start their program within 0.4 s of
  `Run`. At 150 s the run with a `PT0S` limit is under way, and Task Scheduler has ended the
  one with `PT1M` and its program, last result 0x00041306. `Stop` ends the `PT0S` run and its
  program within 1.2 s, with the same result, and succeeds again with no run under way
  (RF-E6, run 40030964166).

Processes and jobs:

- As a standard user, WTS at level 1 for any session lists every process Toolhelp does,
  session 0's among them. It gives the SID of the user's own `codex.exe` and `Codex.EXE` and
  of the elevated job user's `codex.exe` in the same session, and each of them opens for
  limited queries, with its token's user and its image path, on both images (run
  38741362483).
- `GetProcessInformation(ProcessMachineTypeInfo)` reports an x64 program emulated on
  windows-11-arm as x64. `IsWow64Process2` gives the native machine.
- A process a standard user starts is in a job already, and a job of its own still takes its
  children. Every leaf of 1000 probe, probe, probe chains and 1000 cmd, node, probe
  chains stayed in it, assigned right after its start or started suspended and resumed.
- In a job without `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, `CREATE_BREAKAWAY_FROM_JOB` failed with 5
  all 200 times. In a job with it, all 200 leaves started with the flag left that job and
  stayed in the outer job the process was already in, however the chain was assigned (run
  38704825885).
- `GetProcessTimes` of a child opened by its pid for limited queries gives a creation time
  between the readings of `GetSystemTimePreciseAsFileTime` either side of its start, the same
  as the child's own reading as text, and the same once it has exited while a handle is open,
  its exit time later. Each of 50 children had its own (RF-T8, runs 39310064516 and
  39320680285).
- Once the probe holds no handle to an exited child, opening its pid at once gives the same
  exited process in 30 and 11 of 50 tries on windows-2025 and windows-11-arm, and fails 6 in
  the rest. 100 ms later it fails 6 in 99 of 100 (RF-T8, run 39320680285). A pid never given
  out fails 87, as the core's Windows tests read, and `proc::open` takes both for gone.
- `GetModuleFileNameW` and `current_exe` give the path a program was started by, through a
  file symbolic link, a junction or a hard link. Canonicalizing resolves the link and the
  junction, not the hard link.
- Edge, opened on a local page by `ShellExecuteW` or by the http association's command from
  a process in a job, runs all its 8 to 10 processes inside that job. `TerminateJobObject`
  ends them all. Ending only the probe's processes, or letting the opener exit, leaves Edge
  running.

Handing a descriptor:

- An anonymous pipe's read end, handed to Bun as C runtime descriptor 3 through
  `STARTUPINFOW.lpReserved2`, reaches it as descriptor 3. Bun's `fstat` calls it a FIFO,
  and a direct `readSync` reads all 46 bytes handed (run 38797729796).
- A loopback socket handed the same way is a FIFO to Bun's `fstat` too, never a socket,
  and opening `/proc/self/fd/3` fails ENOENT. So a reader that reads the descriptor
  directly only when `fstat` calls it a socket, and otherwise opens `/proc/self/fd/3` and
  falls back to the direct read only on EACCES or EPERM, as Claude Code 2.1.300's does,
  gets nothing from a pipe or a socket (run 38821312770).

Claude Code's remote token path, `/home/claude/.claude/remote/.oauth_token`, which it reads
where no descriptor is named (`remote_token_files`), names no drive. Bun 1.4.3 resolves it
against the drive of its working folder (RF-T7, runs 39310064516 and 39320680285):

- Run from a folder on a VHD holding a dummy at `\home\claude\.claude\remote\.oauth_token`,
  it read the dummy whole, a regular file of 39 bytes.
- Run from a folder on `C:`, which has no `C:\home\claude`, it got ENOENT.

So a session reads the file at the root of the drive its working folder is on, and doctor's
`auth_source` check and a warm-up's halt over `.api_key` look at the root of each fixed
drive. Each reads at most 64 KiB of a regular file, as `remote_token_files` has Claude Code
read one, since any account on the PC may put one there.

Credential Manager:

- A generic item's blob holds 2560 bytes, `CRED_MAX_CREDENTIAL_BLOB_SIZE`, and `CredWriteW`
  refuses 2561, as the fresh standard user (run 38858363708,
  `host::windows::credman::tests::an_item_holds_2560_bytes_and_no_more`).
- `Bun.secrets.set`, called as Claude Code 2.1.300 calls it, writes a generic item named
  `<service>/<name>`, with the name as its user name, enterprise persistence, no comment and
  no attributes, and the value's UTF-8 bytes as its blob: 30 UTF-16 units holding `Đ` and `ạ`
  made a 33-byte blob (run 38704825885).
- As the job's user, in an interactive logon, `credman-names` lists the prefixes of every
  live login family and `pitboard-*` without an error, and finds no item. As the fresh
  standard user, after each Windows test run, it does the same and finds nothing (run
  37762408901).
- A fresh standard user's Credential Manager takes about 237 pieces of 2400 characters, then
  `CredWriteW` fails with `ERROR_NOT_ENOUGH_MEMORY`, and pieces a failed run left count
  against that (runs 38856939312 and 38858363708). A login near Claude Code's 256 pieces
  (`credman_chunking`) fails there, so the runner tests write at most 100 pieces, and a round
  trip over items in memory holds the 256-piece edge.
- Beside other readers, the read after a write in pieces has found the `#p` the write deleted
  (runs 39137503718 and 39909919492). Where the pieces hold the login, Pitboard's write
  deletes `#p` again, five times at most, 20 ms apart, and one that still finds it says what
  the delete said.

A network logon, by key through sshd as the standard user: windows-2025's own OpenSSH 9.5p2,
and on windows-11-arm the Win32-OpenSSH 10.0p2 RF-M installs (runs 38821312770, 38922866517,
38928930271, 39291962999, 39310064516 and 39320680285):

- It is logon type 3, in session 0.
- `CredWriteW`, `CredReadW`, `CredDeleteW` and `CredEnumerateW` fail 1312.
- WTS and Toolhelp name the same user's `codex.exe` in session 2.
- On windows-2025 `CryptProtectData` fails 5. On windows-11-arm it seals and unseals, and
  refuses the wrong entropy. Whether sshd's build or the image makes that difference is not
  measured: RF-M installs OpenSSH only where the image ships none, as AGENTS.md allows.

Scoop, winget and the user `PATH`, as the job's user, with the gated build in the zips
`package-windows.ps1` makes for 0.14.0 and 0.14.1, served on loopback (run 38725515950):

- ScoopInstaller/Install's `install.ps1` at 1e2f3340, given `-ScoopDir` and `-RunAsAdmin`,
  installs Scoop v0.6.0 there and puts its shims first on the user `PATH`.
- From a bucket holding `packaging/scoop/pitboard.json` as the release fills it, Scoop takes
  the `64bit` zip on windows-2025 and the `arm64` one on windows-11-arm, checks its hash,
  points `apps\pitboard\current` at the version's folder by a junction, and makes a
  `pitboard` shim that runs the zip's own `pitboard.exe`.
- `scoop update pitboard` while that `pitboard.exe` runs says it is running, skips it and
  exits 0, leaving `current` on the old version. Once it stops, the update goes through.
- A program started after the installs finds `pitboard` on the user `PATH` in this order:
  Scoop's shims, `~\.cargo\bin`, which the images put there, WinGet's Links, then a folder
  appended last, standing in for the MSI's `bin`. windows-11-arm, without winget, keeps the
  same order without Links.

On windows-2025, where winget is:

- winget, under Developer Mode, installs the zip from a local manifest naming its
  `pitboard.exe` a nested portable with the alias `pitboard`. It links `pitboard.exe` in
  `%LOCALAPPDATA%\Microsoft\WinGet\Links` to the exe in the package's folder by a symbolic
  link, and appends Links to the user `PATH`.
- `winget upgrade` while that `pitboard.exe` runs fails 0x8A150052, since it cannot remove the
  running version's folder, and leaves the link and that folder as they were. Once it stops,
  the upgrade goes through and the link leads to the new version.
- `winget validate` exits 0x8A150028, valid with warnings, for a manifest without schema
  headers.
- `winget uninstall --id Pitboard.CLI --exact` finds no package installed from a local
  manifest: 0x8A150014. In run 38741362483, `winget uninstall --manifest` given that manifest
  found it as `ARP\User\X64\Pitboard.CLI__DefaultSource` and left no link, package folder
  or Uninstall key.

On windows-11-arm, where real-time protection is on, with all 19 attack surface reduction
rules of Microsoft's reference in audit, Defender logged no event 1121 or 1122 while the zip's
`pitboard.exe`, marked as downloaded from the internet, ran with `--version`, with no argument
and with `completions bash`. The prevalence and ransomware rules need cloud-delivered
protection, which neither image has, so they judged nothing.

`test-windows-release.ps1` read bottom 0.14.9's x64 `btm.exe` with
`Get-AuthenticodeSignature` on both images in CI run 38722196167, and its ARM64 `btm.exe` on
both in CI run 38741365089. Each is `Valid`, with a `TimeStamperCertificate` and this
`SignerCertificate.Subject`, the form `WINDOWS_SIGNER_SUBJECT` takes:
`CN=SignPath Foundation, O=SignPath Foundation, L=Lewes, S=Delaware, C=US`.

WiX v7's ICE validation of `Pitboard.msi` on both images in CI run 38882810828, read against
the `runtimes-framework` folders of the Windows App SDK 2.5.1 packages as bytes:

- ICE03 refuses as invalid the languages of WinUI's MUI files for ug-CN (1152), mi-NZ (1153)
  and gd-gb (1169), the only files there whose primary language is past 0x7F.
- ICE03 finds the languages Windows Installer reads from `Microsoft.ui.xaml.dll` and
  `Microsoft.UI.Xaml.Phone.dll` longer than the File table's 20 characters, though each
  file's own version resource names 1033 alone.
- ICE57 takes a shortcut in `ProgramMenuFolder` for the user's in a package whose `ALLUSERS`
  is 2, and refuses it an HKMU key path.

The build suppresses ICE03, which checks every column of every table. `validate-windows-msi.ps1`
runs it on the built MSI and lets through only those eight rows, found by their names: an
invalid language on either library's `.mui` file in the ug-CN, mi-NZ or gd-gb folder, and a
language list too long on either library itself.

With ICE03, ICE33 and ICE61 suppressed, WiX v7 builds it on both images in run 38895887782
with no warning, ICE57 passing. `msiexec /a`, run by the job's user, lays out each image's
own MSI in `PFiles64\Pitboard`: the Release publish's 284 programs and libraries on
windows-2025 and 283 on windows-11-arm, no `.pdb`, and `bin\pitboard.exe`. The fresh
standard user opens the MSI read only through `msi.dll`, as `Pitboard.Installer.Tests` does.

The Component, Directory, File and Registry tables of the published MSIs of 1.0.0-beta.2 and
1.0.0-beta.3, read as bytes on 20 October 2026: from one beta to the next each machine's MSI
kept the GUID of every key path, 534 on x64 and 530 on ARM64, and beta.3's two MSIs give the 528
key paths they share the same GUIDs. A standard folder's Directory row goes by its property,
`ProgramFiles64Folder` under `TARGETDIR`, and each folder under `Pitboard` by an id WiX makes
up, such as `dGNBjMVUkheIrP_lIJTI5NBZzo_c` for `af-ZA`. So `check-msi-components.ps1` names a
key path by the standard folder and the long names below it.

The Authenticode certificate tables of the app folders 1.0.0-beta.3's MSIs carry, the artifacts
`app-windows-x64` and `app-windows-arm64` of release run 39205401692, read as bytes with
LibreSSL's `pkcs7 -print_certs` on 20 October 2026. Of the 512 programs on x64 and 511 on ARM64,
284 and 283 DLLs, three executables, 172 `.mui` and 53 `.winmd` files, seven are not signed:
Pitboard's six and `WinSparkle.dll`. The Windows Community Toolkit's five are signed by the .NET
Foundation, as `Community Toolkit (.NET Foundation)` or `Windows Community Toolkit (.NET
Foundation)`, and every other one by a certificate whose organisation is Microsoft Corporation,
issued by Microsoft's own code signing CAs. So `verify-windows-signatures.ps1` holds each to its
maker by its name, Microsoft for a name it does not list.

The images:

- Defender's real-time protection is off on windows-2025 and on, with behaviour monitoring
  and tamper protection, on windows-11-arm. Cloud-delivered protection is off on both:
  `MAPSReporting` is 0 and `SubmitSamplesConsent` 2, never send (run 38741362483). winget,
  v1.11.510, is on windows-2025 alone, and Scoop on neither.
- diskpart formats ReFS on windows-2025. On windows-11-arm, `Format-Volume -DevDrive` makes a
  trusted Dev Drive with Defender's filter attached.
- The Server service runs, and `New-SmbShare` serves a folder to `\\localhost`.
- windows-2025 runs OpenSSH Server, whose `AllowGroups` turned the standard user away until
  it joined OpenSSH Users (run 38691271016). windows-11-arm ships none, and
  `Add-WindowsCapability` did not install it within 10 minutes (run 38685119129). There RF-M
  installs Win32-OpenSSH 10.0p2 from Microsoft's release, its SHA-256 pinned, starts it and
  removes it after (runs 38821312770, 38922866517 and 38928930271).
- Neither image holds Codex's `%ProgramData%\OpenAI\Codex`, Claude Code's
  `C:\Program Files\ClaudeCode` or its policy keys.
