# The session machine

Measured by hand on 18 October 2026 on the session machine: Windows 11 Pro 26H2, build 26300,
UBR 9711, on ARM64, an OSMates VM, with Defender's real-time protection on and no exclusion.
The bundle was `windows-facts.yml` session run 39114824545 at 91bd8452, the commit that holds
`Release 1.0.0-beta.1`, its sums checked on the Mac and in the guest. Claude Code 2.1.302 and
Codex 0.163.0 came from npm 24.20.0, installed with `winget`; `pitboard-conformance` passed on
both. The account was an administrator that signs in with a password, its programs run
limited at medium integrity. The probe's reports are kept out of the repository, which names
no account.

Steps 9 and 10, Codex's sign-ins and the ChatGPT app, were not done, nor the optional 15 and
16. What they measure waits on the next session: `codex_login_is_driveable`,
`codex_runs_inside_the_chatgpt_app`, `windows_credman_cache`, W18's package family and AUMID,
and the rows of [Documented, not measured](windows-documented.md) that name them.

Logon and homes:

- The interactive logon is type 2. Its token is limited, not elevated, at medium integrity,
  owns what it makes, and reads as `not_elevated`, as Microsoft documents a split token.
- `FOLDERID_Profile` and `FOLDERID_LocalAppData` are the profile and its `AppData\Local`; the
  profile is local, its name ASCII. Only `Path` is spelled, and `CODEX_HOME`,
  `CLAUDE_CONFIG_DIR`, `PITBOARD_HOME` and `HOME` are unset. The WebView2 Runtime is
  155.0.4283.62.
- Windows PowerShell 5.1's execution policy is `Restricted`, so npm's `claude.ps1` and
  `npm.ps1` refuse to run there, and `claude.cmd` runs.

Smart App Control and SmartScreen:

- With Smart App Control on, the unsigned `pitboard.exe` marked as downloaded ran, and the
  unsigned MSI opened from File Explorer installed with no prompt; no CodeIntegrity event was
  logged, before or after a restart.
- With it off, the MSI so marked met SmartScreen's "Windows protected your PC", Publisher
  "Unknown publisher", passed with **More info** and **Run anyway**.

The MSI, `Pitboard.exe` and the notification area:

- It installs per user with no UAC prompt, context 2: `%LOCALAPPDATA%\Programs\Pitboard` with
  `Pitboard.exe` at its top and `pitboard.exe` alone in `bin`, the Start shortcut with the
  AppUserModelID `Pitboard.Pitboard`, `HKCU\Software\Classes\pitboard`, and `bin` last on the
  user `PATH`, which a terminal opened afterwards finds. Settings > Apps lists it as Pitboard,
  publisher Pitboard, with no Modify; its Uninstall key reads under HKLM's view. Pitboard does
  not start after the install.
- A first launch from Start opens the window. The icon starts in the overflow, named and
  tipped "Pitboard", or the account in use and its usage once there is one, and keeps its
  place on the taskbar across a restart. Closing the window hides it. Win+B, Enter and
  Shift+F10 reach the icon and its menu, which opens at the icon in the taskbar's mode.
- Registering for notifications worked with no Singleton package: it wrote
  `HKCU\Software\Classes\AppUserModelId\Pitboard.Pitboard` with `DisplayName`, `IconUri` and
  `CustomActivator`. A login replaced outside Pitboard was said in the window, and no
  notification came for it, though the model asks the app to post one.
- `msiexec /i` of a second MSI, one higher in the fourth field, upgraded 1.0.0.1 to 1.0.0.2
  with no UAC prompt in 7 s. The default schedule removed the old product first, so
  `bin\pitboard.exe` was missing for about 2 s: 9 of 416 runs of the status line every 250 ms
  found no `pitboard`. The MSI now removes it after the new one's files are in place.
- `msiexec /fa` with Pitboard open had Restart Manager close it, not forced, and start it
  again within 4 s; the icon kept its place. Of Pitboard's windows, only the hidden one, never
  shown, ends Pitboard on `WM_ENDSESSION` with `ENDSESSION_CLOSEAPP` or on `WM_CLOSE`, so one
  of them reached it. Which one was not recorded.
- With start at sign-in on, a sign-in started Pitboard with no window, its icon there 7 s
  after. Settings > Apps > Startup listed it; turning it off there wrote 01 as the first byte
  of `StartupApproved\Run`'s `Pitboard` and kept the `Run` value.
- `pitboard uninstall` with the app open deleted `%LOCALAPPDATA%\Pitboard` and the renewal
  task, and the app made the folder again as it quit; `uninstall` now refuses while the app is
  open. Removing Pitboard in Settings then took the install folder, the shortcut, the link
  scheme, the `PATH` entry, the `Run` value and `%LOCALAPPDATA%\Pitboard App`. It left the
  `AppUserModelId` key, which `--unregister` now deletes, and `StartupApproved\Run`'s value,
  Windows' own. Nothing in the MSI removed the renewal task, which `--unregister` now takes
  away where it runs this install's `bin\pitboard.exe`.
- The x64 MSI installed under emulation: `Pitboard.exe` and `bin\pitboard.exe` are machine
  0x8664, and both ran.

Claude Code 2.1.302:

- `pitboard enroll --sign-in` drove Claude Code's manual flow, the code pasted back; the app's
  **Add account** drove its localhost flow, with nothing to paste. Neither left a scratch slot
  in Credential Manager.
- A running session took a switch's `.credentials.json` at its next prompt
  (`windows_file_adoption`). Process Monitor saw it open its files sharing all three access
  modes, and no sharing violation on any `.claude` path (`windows_open_sharing`).
- A `settings.json` that starts with a byte order mark is read
  (`settings_byte_order_mark`), and its first move of a login into Credential Manager lost it
  (`credman_move_lost_a_piece`). A sign-in into an empty Credential Manager wrote one whole
  item of 524 bytes.
- It ran the status line in `powershell.exe` with neither PowerShell 7 nor Git installed, in
  `pwsh.exe` with PowerShell 7.6.7, and in Git 2.55.0's `bash.exe`
  (`windows_statusline_powershell`), an account named `bé` shown whole. Twenty runs of
  `pitboard statusline` took 16 ms each from PowerShell, and 132 ms each through Git Bash.

Daily renewal:

- `pitboard schedule install` registered `\Pitboard\Renew <SID suffix>` to run as the person,
  interactive only, least privilege, hidden. Run at once, it started `pitboard.exe renew
  --scheduled` under svchost with no console window, last result 0x0, and doctor said when it
  last ran.
