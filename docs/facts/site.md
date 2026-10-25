# The site's figures

What the figures on usepitboard.com draw for each system: the terminals, the keys a browser
keeps from a page, and [the apps' own controls](#the-apps-controls). The terminals and the keys
were read and measured on 25 October 2026, unless a line names another date:

- zsh 5.9 and bash 3.2.57 on macOS 27.0.1, in a scratch home, driven through a pseudo-terminal
  80 columns wide.
- bash 5.3.0 in the `fedora:43` container and bash 5.2.37 in `ubuntu:25.10`, both arm64, the
  first driven the same way through `docker run -it`.
- Windows PowerShell 5.1.26100.33648 on windows-2025 and 5.1.26100.9711 on windows-11-arm, with
  PSReadLine 2.0.0 in its Windows mode, culture en-US, by `windows-facts.yml` run 40190429034
  (RF-S1) as the job's user. The two images read alike.
- Chromium at 56ebe9d2 and Firefox at efd2d1e6, read from their source.

The completion scripts are the shipped ones, as the snapshots in
`crates/pitboard/src/snapshots` hold them: they complete commands and flags, never a label.
Measure a fact again before changing a figure that draws it.

RF-S1 puts the zip `package-windows.ps1` makes on `powershell.exe`'s `PATH` and dot-sources its
`pitboard.ps1` under `RemoteSigned`, as the docs set it up. Started from PowerShell 7 with the
`PSModulePath` that PowerShell 7 hands on, Windows PowerShell loaded none of its own modules:
`Get-ExecutionPolicy` failed with `CouldNotAutoloadMatchingModule` on both images (run
40186803305). Started with no `PSModulePath`, as RF-S1 starts it now, it makes its own.

## Shells and prompts

- macOS: Terminal runs zsh. Apple's `/etc/zshrc` sets `PS1="%n@%m %1~ %# "`, a prompt that
  reads `me@<host> ~ %`, keeps 2000 lines of history and saves 1000, and sets `BEEP`. It runs no
  `compinit`, so zsh completes nothing of Pitboard's until `~/.zshrc` does, as
  [Integration commands](../reference/integration-commands.mdx) says.
- Linux: Ptyxis runs bash. Fedora 43's `/etc/bashrc` sets `[\u@\h \W]\$ `, which reads
  `[me@fedora ~]$`. Ubuntu 25.10's `/etc/skel/.bashrc` sets `\u@\h:\w\$ `, in colour where the
  terminal has it, which reads `me@ubuntu:~$`.
- Windows: Windows Terminal's "Windows PowerShell" profile runs Windows PowerShell 5.1, whose
  own prompt function writes `PS C:\Users\me>`
  ([about_Prompts](https://learn.microsoft.com/powershell/module/microsoft.powershell.core/about/about_prompts?view=powershell-5.1)).

## A command that is not found

- zsh: `zsh: command not found: foo`, exit 127. `zsh -c` puts the line number in:
  `zsh:1: command not found: foo`.
- bash 3.2 and 5.3: `bash: foo: command not found`, exit 127. Neither container holds a
  command-not-found handler; one installed on a desktop adds lines of its own, not read here.
- Windows PowerShell 5.1: an error record in `NormalView`, its colours red on black. Laid out 80
  columns wide, as a line typed at the prompt runs:

  ```text
  foo : The term 'foo' is not recognized as the name of a cmdlet, function,
  script file, or operable program. Check the spelling of the name, or if a path
  was included, verify that the path is correct and try again.
  At line:1 char:1
  + foo
  + ~~~
      + CategoryInfo          : ObjectNotFound: (foo:String) [],
  CommandNotFoundException
      + FullyQualifiedErrorId : CommandNotFoundException
  ```

  A line holding one space follows. Each line that wraps keeps the space it wrapped at, and the
  `~` run is as long as the word typed. `powershell -Command foo` writes the same lines to a
  redirected error stream, wrapped at 120 columns, and exits 1.

## Tab completion

zsh 5.9, with `compinit` and the shipped `_pitboard`, under zsh's own `AUTO_LIST`,
`LIST_AMBIGUOUS` and `AUTO_MENU`:

- A Tab that leaves one match puts it in, with a space: `pitboard us` gives `pitboard use `.
- A Tab with several matches and nothing they share past what is typed rings the bell and lists
  them under the prompt as `name  -- about`, each about cut to end at the 76th of 80 columns,
  and the line stays. The next Tab rings again and puts in the first match, and each Tab after
  puts in the next. `pitboard u` lists `uninstall` then `use`; `pitboard s` lists `schedule`,
  `status`, `statusline`, `stow`.
- `pitboard -` lists three lines, aligned: `--help     -h  -- Print help`,
  `--json         -- Print JSON instead of text` and `--version  -V  -- Print version`.
- `pitboard use ` lists the folder's file names, from the script's `:label …:_default`.

bash 5.3 with readline 8.3, the shipped script loaded with `-o nosort`:

- A Tab with several matches rings the bell; the next lists them in the script's order and
  draws the line again. `pitboard u` lists `use  uninstall`; `pitboard -` lists
  `-h  -V  --json  --help  --version`.
- Where every match starts with what a Tab can put in, it puts that in and rings: `pitboard use `
  gives `-`, the second Tab rings, and the third lists `-y  -h  --yes  --json  --help`.
- `pitboard ` lists every flag and command in the script's order, down columns.
- bash 3.2 loads the script without `-o nosort`, so readline sorts the list:
  `--help  --json  --version  -V  -h  abandon …`.

Windows PowerShell 5.1, read through `TabExpansion2`, which Tab asks. Tab is
`TabCompleteNext`: it puts the first offer in place of the word, and each Tab after the next
([about_PSReadLine_Functions](https://learn.microsoft.com/powershell/module/psreadline/about/about_psreadline_functions)).
The script sorts its offers with `Sort-Object -Property ListItemText`, which compares by culture
and passes over the hyphens:

- `pitboard ` offers `abandon adopt completions doctor enroll forget -h help --help --json log
  rename renew repair schedule status statusline stow uninstall use -V --version warm watch`.
- `pitboard u` offers `uninstall use`; `pitboard s` offers `schedule status statusline stow`.
- `pitboard use ` offers `-h --help --json -y --yes`; `pitboard watch ` offers
  `--at -h help --help install --json --once status uninstall`; `pitboard completions ` offers
  `-h --help --json`, and no shell name.
- `pitboard -` offers nothing, where PowerShell 7.5.2 on macOS offers the five flags from the
  same script.

## Keys at the prompt

| Key | zsh | bash | Windows PowerShell 5.1 |
| --- | --- | --- | --- |
| Up, Down | `up-line-or-history`, `down-line-or-history`, on `ESC [ A` and `ESC [ B` | `previous-history`, `next-history` | `PreviousHistory`, `NextHistory` |
| Tab | `expand-or-complete` | `complete` | `TabCompleteNext` |
| Shift+Tab | | | `TabCompletePrevious` |
| Ctrl+Space | | | `MenuComplete` |
| Ctrl+A | `beginning-of-line` | `beginning-of-line` | `SelectAll` |
| Ctrl+E | `end-of-line` | `end-of-line` | nothing |
| Ctrl+B, Ctrl+F | `backward-char`, `forward-char` | the same | nothing |
| Ctrl+K | `kill-line` | `kill-line` | nothing |
| Ctrl+U | `kill-whole-line` | `unix-line-discard` | nothing |
| Ctrl+W | `backward-kill-word` | `unix-word-rubout` | `BackwardKillWord` |
| Ctrl+Y | `yank` | `yank` | `Redo` |
| Ctrl+Z, Ctrl+_ | Ctrl+_ `undo` | Ctrl+_ `undo` | Ctrl+Z `Undo` |
| Ctrl+R | `history-incremental-search-backward` | `reverse-search-history` | `ReverseSearchHistory` |
| Ctrl+L | `clear-screen` | `clear-screen` | `ClearScreen` |
| Ctrl+C | | | `CopyOrCancelLine` |
| Escape | the start of a Meta key | the same | `RevertLine` |
| Escape then Tab | `self-insert-unmeta`: a tab goes in | `dynamic-complete-history` | `RevertLine`, then `TabCompleteNext` |

zsh's are `bindkey -M emacs` in `zsh -f`, bash's `bind -p` in `bash --norc`, and PowerShell's
`Get-PSReadLineKeyHandler`; an empty cell is a key not read. Apple's `/etc/zshrc` binds the
`ESC O A` and `ESC O B` of `$terminfo` to `up-line-or-search` and `down-line-or-search`, which a
terminal sends only in its application cursor mode.

Ctrl+C at a prompt with `pitboard u` typed, in bash 5.3 through `docker run -it`, leaves the line
with `^C` after it and gives a new prompt. Through the pseudo-terminal on macOS, bash 3.2 echoed
no `^C`, and zsh left the line as typed. What Terminal and Ptyxis echo is not measured.

## Keys a browser keeps from a page

"Browser and OS shortcuts are not available to web apps" outside fullscreen
([Keyboard Lock](https://wicg.github.io/keyboard-lock/)). Read from source:

- Chromium reserves, outside fullscreen and outside an app window, closing a tab or a window,
  a new window, a new incognito window, a new tab, restoring a tab, moving to the next or
  previous tab, and quitting (`kReservedCommandIds`, `chrome/browser/ui/accelerator_table.cc`).
  On Windows and Linux those are Ctrl+W, Ctrl+F4, Ctrl+Shift+W, Alt+F4, Ctrl+N,
  Ctrl+Shift+N, Ctrl+T, Ctrl+Shift+T, Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+PgDn and Ctrl+PgUp. On
  macOS the main menu gives the same commands their keys, among them ⌘W, ⌘N, ⌘T and ⌘Q.
  Edge is built on Chromium; its own list is not read.
- Firefox marks `reserved="true"` the keys of a new window and a new tab (Accel+N, Accel+T),
  closing a tab and a window (Accel+W, Accel+Shift+W), a private window (Accel+Shift+P),
  quitting (Ctrl+Shift+Q on Windows, Accel+Q elsewhere) and leaving fullscreen
  (`browser/base/content/browser-sets.inc.xhtml`). Accel is ⌘ on macOS and Ctrl elsewhere.
- Safari: not read.

Not measured: which keys reach a page in Safari, Chrome, Firefox and Edge on macOS and Windows,
and Chrome and Firefox on Linux, by hand. Of the keys the figures draw or a shell binds, ⌘N and
⌘Q are reserved in both by their source, and so is Ctrl+W on Windows and Linux.

## The apps' controls

How the apps' own menus, dialogs, lists and Settings controls behave, as the figures draw them,
read by the apps' UI tests in the `twoTools`, `oneTool`, `onlyOne` and `warmUp` fixtures on CI's
runners. Each test asserts what it reads, and keeps it with a picture under a name starting
`site-`, in the run's `macos-app-pictures` and `site-figures-*` artifacts:

- macOS: `apps/macos/UITests/SiteFiguresTests.swift`, the debug build on the `macos-26` image's
  macOS 26.6.2, on a screen of 1024 by 768 points. No runner has macOS 27, which the site draws.
- Windows: `apps/windows/Pitboard.UITests/SiteFiguresTests.cs`, the Debug build as a fresh
  standard user on windows-2025, whose shell is Windows 11 24H2's, and on windows-11-arm, both
  on a screen of 1024 by 768 with a work area 720 high. On windows-11-arm a window of the job
  user's keeps the foreground, as [Windows runners](windows-runners.md) says, so what needs a key
  was read on windows-2025 alone.

Runs 40204121073 and 40206476781 read and assert them, on 25 October 2026, after run
40196222517 read most of them first. A line that names runs was read in those alone.

### macOS: the menu bar item

- The item draws highlighted while its menu is open. Its accessibility title is "Pitboard, work
  42%, Claude Code".
- In `twoTools` the menu reads, top to bottom: the heading Claude Code, work, old, personal; the
  heading Codex, main · Pro, spare · Pro; Add Account… ⌘N, Refresh ⌘R; Open claude.ai ›, Open
  chatgpt.com ›; Open Pitboard, Settings… ⌘,; About Pitboard, Pitboard Help, Quit Pitboard ⌘Q.
  A separator divides each group. The headings are dimmed; each tool's account in use is
  checked.
- Opened with a click, the menu highlights nothing. The first Down highlights work, the first
  enabled item; Down and Up pass over headings and separators. Down stops at Quit Pitboard: the
  menu does not wrap. With nothing highlighted, the first Up goes to Quit Pitboard. Home and
  Page Up go to work, End and Page Down to Quit Pitboard.
- Typing goes to the first item whose title or subtitle starts with what is typed, in any case:
  "spa" goes to spare · Pro, "Open P" to Open Pitboard, "o" to old, and "Needs" to old, whose
  subtitle is "Needs signing in again". With a pause of 0.5 s between keys they make one name,
  and with 1 s a new one starts: "o" then "p" goes to Open claude.ai after 0.5 s and to personal
  after 1 s (runs 40204121073 and 40206476781).
- Space chooses the highlighted item and closes the menu: on Open Pitboard it opened the
  Accounts window.
- ⌘, and ⌘N choose their items while the menu is open, and close it: Settings comes forward, or
  the Accounts window with its Add Account sheet. ⌘Q quits.
- Right on Open claude.ai opens its submenu, work, old and personal, with work highlighted; Left
  closes it and leaves Open claude.ai highlighted (run 40206476781). The menu stands at the
  screen's right edge, so the submenu opens to its left.
- Resting the pointer on Open claude.ai opens its submenu within 0.34 s of the pointer starting
  to move there (runs 40204121073 and 40206476781).
- On the fixture a switch is quicker than the test can see: In Use showed 0.12 to 0.13 s after
  the click on **Use** returned, and no "Switching…" was seen. Refresh was never seen dimmed
  (runs 40204121073 and 40206476781).

### macOS: the menu bar with the Accounts window in front

- The bar: the Apple menu, Pitboard, File, Edit, View, Go, Account, Window, Help.
- Pitboard: About Pitboard; Settings… ⌘,; Services ›; Hide Pitboard ⌘H, Hide Others ⌥⌘H, Show
  All; Quit Pitboard ⌘Q. Every item but About has a symbol before it.
- File: Add Account… ⌘N; Open claude.ai ›; Remove Website Data…, dimmed; Close ⌘W, with Close
  All in its place under ⌥.
- Edit is macOS's own: Undo ⌘Z, Redo ⇧⌘Z; Cut ⌘X, Copy ⌘C, Paste ⌘V, Delete, Select All ⌘A;
  Find ›, Spelling and Grammar ›, Substitutions ›, Transformations ›, Speech ›; AutoFill ›,
  Start Dictation… fn D, Emoji & Symbols fn E. Copy and Delete are dimmed with no account
  selected, and enabled with one. Delete shows no key.
- View: Refresh ⌘R; Actual Size ⌘0, Zoom In ⌘+, Zoom Out ⌘−, dimmed; Enter Full Screen fn F,
  dimmed.
- Go: Back ⌘[ and Forward ⌘], dimmed.
- Account, with no account selected: Use, Sign In Again…, Rename…, Forget… ⌘⌫, all dimmed.
  With personal selected: Use personal, Sign In Again…, Rename…, Warm Up Now, dimmed; Open
  claude.ai; Forget… ⌘⌫. With work, the account in use, selected: the same with Use work, Warm
  Up Now and Forget… dimmed, and ⌘⌫ asks nothing. No item is red.
- Window is macOS's own: Minimize ⌘M, Zoom, Fill ⌃fn F, Center ⌃fn C; Move & Resize ›; Full
  Screen Tile; Remove Window from Set; Pitboard; Bring All to Front. Under ⌥, Minimize All, Zoom
  All and Arrange in Front take their places.
- Help: macOS's search field, then Pitboard Help.
- About Pitboard opens the standard About panel, a dialog of 284 by 170 points whose close
  button alone is enabled: the app's icon, "Pitboard" in bold, and the version and build,
  "0.0.0 (1)" in a debug build, with no "Version" before them.

### macOS: the Forget alert

- A sheet on the Accounts window: the title "Forget “personal”?", the message, then Cancel and
  Forget, in that order and of one width. Forget's title is red on a pale red fill, Cancel's
  grey; neither is drawn as the default button.
- Return presses neither button, where AppKit's own rule gives Return to an alert's first
  button. ⌘. and Escape press Cancel.

### macOS: the account list

- At first no row is selected. A click selects a row, in the accent colour with white text; a
  click on the sidebar leaves it selected in grey. Up and Down move the selection. Typing an
  account's name selects it: "w" selects work.
- **Use** switches without selecting its row.
- A right click on a row not selected leaves the selection where it was and draws an accent
  outline round the row clicked. The shortcut menu acts on that row: Use personal, Sign In
  Again…, Rename…; Open claude.ai; Copy Email Address; Forget…, with no keys and nothing red.
  Copy Email Address copied personal's address.
- After Forget, no row is selected and the list keeps focus: Down selects work.

### macOS: Limits

- The pop-up opens its menu with the chosen item, checked, over the button: Do Nothing, Notify
  Me, Switch Automatically.
- The stepper steps by 1 a click and repeats while held: from 50, 17 steps in a hold of 1 s. It
  stops at 50 and 99, and a click past a bound changes nothing.
- On Return the field clamps a typed share to 50 to 99, puts back the last share for text that
  does not parse, and takes "90" without "%". The Up arrow in the field changes nothing.
- The field also takes each key as it is typed: "7" with no Return set the share to 50 within a
  second (run 40204121073), and a share clamped while typing can rewrite the text under the
  caret: "80%" typed with no Return left "50%0%" in the field (run 40196222517). Its `share`
  binding in `LimitsSettings.swift` clamps and sends every value the field hands it.
- The warm-up toggles are switches, and From and To are time fields with a stepper beside each.

### Windows: the icon's menu

Explorer takes no icon of the test user's ([Windows runners](windows-runners.md)), so the tests
open the menu with the message Explorer sends for a right click and for Shift+F10, which the app
cannot tell apart:

- Opened at a point by the clock, 864,744, the menu's right edge is at the point and its bottom
  on the work area's bottom edge, the taskbar's top. Opened mid-screen, at 512,384, its left edge
  is at the point and its top at the screen's top, since at 501 high it fits neither above nor
  below the point.
- Its items: the heading Claude Code, work · 42%, old, personal · 100%; the heading Codex,
  main · Pro · 7%, spare · Pro · 0%; Add account…, Refresh; Open claude.ai ›, Open chatgpt.com ›;
  Open Pitboard, in semibold; Settings; Pitboard help, Exit. The headings are disabled. Each
  account is a toggle item, on for its tool's account in use. The tooltips are the items' help
  text: "5-hour 42%, weekly 12%", "Needs signing in again", "Updated 3:10 PM" on Refresh, none
  on the headings.
- It opens with focus on work · 42%, the first enabled item, whose tooltip then shows. The first
  Down goes to old. Down and Up pass over the headings and wrap: Down from Exit goes to work, and
  the first Up after opening to Exit. Home, End, a letter and Tab move nothing.

### Windows: the account list and its dialog

- A right click on a row not selected leaves the selection where it was and draws no outline.
  The context menu puts focus on its first item, Use personal, then lists Sign in again…,
  Rename… F2; Open claude.ai; Copy email address Ctrl+C; Forget… Del. Copy email address copied
  personal's address.
- Delete on the selected row opens the Forget dialog with focus, and its focus rectangle, on
  Forget, the primary button on the left; Cancel is on the right; neither is drawn as the
  default. Enter presses Forget.
- After Forget no row is selected and focus is on the window's title bar, `AppTitleBar`, not the
  list: Down moves nothing.
- On the fixture, In use showed 0.19 to 0.37 s after **Use** was invoked on windows-2025 and
  0.25 s on windows-11-arm, and no "Switching" was seen. Refresh was disabled for 14 to 24 ms
  (runs 40204121073 and 40206476781).

### Windows: Settings, windows and Notification Center

- The ComboBox offers Do nothing, Notify me and Switch automatically, Notify me chosen. The
  NumberBox holds 50 to 99 and steps by 1 and by 5. At 99 its Increase button is disabled, at 50
  its Decrease; a share set past a bound is clamped to it.
- An account window is titled "work" until its page has a title, then "claude.ai stand-in -
  work", the page's title first. For its first moments it has its XAML's title, "Account", until
  it reads its account: run 40208954337 saw it at 73 ms.
- Windows+N opens Notification Center on the right of the work area, 360 wide and as tall as
  it, with keyboard focus. In it, 12 from the edges, a card 336 wide holds "Notifications"
  and its Do Not Disturb button, and under it a card 100 high holds the date and Focus's
  duration. The runner's fresh user has Do Not Disturb on, so the first card says "Do not
  disturb is on" over "No new notifications" (windows-2025).

### Not read on a runner

- The icon's UIA name, a double click on the icon, and a real right click on it, with the focus
  rectangle after one: Explorer takes no icon of the test user's.
- A toast, and on macOS a banner: a fixture posts no notification.
- Notification Center on macOS: a click on Control Center's clock opened nothing (run
  40196222517).
- Whether a disabled menu item shows its tooltip.
- macOS 27's own drawing: the chosen item's blink, the alert, and the red of a destructive
  button.
- What eyes or a phone judge: the blink's timing, tooltip delays, VoiceOver and NVDA, how long a
  banner stays on macOS 27, and a long press.
