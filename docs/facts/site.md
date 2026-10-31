# The site's figures

What the figures on usepitboard.com draw for each system: the terminals, the keys a browser
keeps from a page, a box's edges in Firefox, [the apps' own controls](#the-apps-controls) and
[the Linux app on GNOME and KDE Plasma](#the-linux-app-on-gnome-and-kde-plasma). The terminals
and the keys were read and measured on 25 October 2026, unless a line names another date:

- zsh 5.9 and bash 3.2.57 on macOS 27.0.1, in a scratch home, driven through a pseudo-terminal
  80 columns wide.
- bash 5.3.0 in the `fedora:43` container and bash 5.2.37 in `ubuntu:25.10`, both arm64, the
  first driven the same way through `docker run -it`. bash 5.3.9 and setup 2.15.0-28 in
  `fedora:44` at `43b29f65`, arm64, read on 31 October 2026.
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
- Linux: Ptyxis on GNOME and Konsole on KDE Plasma run bash. Fedora 43's and Fedora 44's
  `/etc/bashrc` set `[\u@\h \W]\$ `, which reads `[me@fedora ~]$`. Ubuntu 25.10's
  `/etc/skel/.bashrc` sets `\u@\h:\w\$ `, in colour where the terminal has it, which reads
  `me@ubuntu:~$`.
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
  rename renew repair schedule status statusline stow uninstall use -V vault --version warm
  watch`. RF-S1 read the list before there was a `vault`, which stands where the same order puts
  it, unread until RF-S1 runs again.
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

## A box's edges in Firefox

Playwright's Firefox 1555 on macOS gives a box's edges in single precision. On 27 October 2026,
once the macOS window figure above it had widened, the Windows context menu's bottom read
411.9333496 against its row's top plus 1 at 411.9333191, one single-precision step apart (commit
28cbdbbc). So a test that holds two edges together allows 1e-3.

## The apps' controls

How the apps' own menus, dialogs, lists and Settings controls behave, as the figures draw them,
read by the apps' UI tests in the `twoTools`, `oneTool`, `onlyOne` and `warmUp` fixtures on CI's
runners. Each test keeps what it reads, with a picture, under a name starting `site-` in the
run's `macos-app-pictures` and `site-figures-*` artifacts, which GitHub keeps for 7 days, and
asserts what the figures draw. What the site rests on is copied here. A timing, a reading that
only a picture shows, and the share field's keys are recorded and not asserted, and say so:

- macOS: `apps/macos/UITests/SiteFiguresTests.swift`, the debug build on the `macos-26` image's
  macOS 26.6.2, on a screen of 1024 by 768 points. No runner has macOS 27, which the site draws.
- Windows: `apps/windows/Pitboard.UITests/SiteFiguresTests.cs`, the Debug build as a fresh
  standard user on windows-2025, whose shell is Windows 11 24H2's, and on windows-11-arm, both
  on a screen of 1024 by 768 with a work area 720 high. On windows-11-arm a window of the job
  user's keeps the foreground, as [Windows runners](windows-runners.md) says, so what needs a key
  was read on windows-2025 alone.

Runs 40204121073 and 40206476781 read them on 25 October 2026, after run 40196222517 read most
of them first, and runs 40211517451 and 40247375339 read them again. Run 40292624691 read them
on 26 October 2026, once the apps said Switch, Remove Account and Sign-in expired. A line that
names runs was read in those alone.

### macOS: the menu bar item

- The item draws highlighted while its menu is open. Its accessibility title is "Pitboard, work
  42%, Claude Code, needs attention" in `twoTools`, whose old needs a sign-in (run 40404550549).
- In `twoTools` the menu reads, top to bottom: Sign In to old Again…; the heading
  Claude Code, work, old, personal; the heading Codex, main, spare; Add Account… ⌘N, Refresh ⌘R;
  Open claude.ai ›, Open chatgpt.com ›; Open Pitboard, Settings… ⌘,; About Pitboard, Pitboard Help,
  Quit Pitboard ⌘Q. A separator divides each group. The headings are dimmed; each tool's account in
  use is checked. Its items carry no symbols and no tooltips. The redone menu's order and names are
  asserted by `testTheStatusMenuListsItsItemsInOrder`, and were read in run 40404550549, and
  without "(Claude Code)" in the first item's title in run 40429818225.
- Opened with a click, the menu highlights nothing. The first Down highlights the first enabled
  item, Sign In to old Again… in run 40429818225; Down and Up pass over headings and
  separators. Down stops at Quit Pitboard: the menu does not wrap. With nothing highlighted, the
  first Up goes to Quit Pitboard. Home and Page Up go to the first enabled item, End and Page
  Down to Quit Pitboard.
- Typing goes to the first item whose title or subtitle starts with what is typed, in any case:
  "spa" goes to spare, "Open P" to Open Pitboard and "o" to old. "Needs" went to old while
  its subtitle was "Needs signing in again", and still did in run 40292624691, where the subtitle
  reads "Sign-in expired". With a pause of 0.5 s between keys they make one name,
  and with 1 s a new one starts: "o" then "p" goes to Open claude.ai after 0.5 s and to personal
  after 1 s, 1.5 s and 3 s. The pauses measured 500 to 505 ms and 1007 to 1014 ms (runs
  40204121073, 40206476781, 40211517451 and 40247375339).
- Space chooses the highlighted item and closes the menu: on Open Pitboard it opened the
  Accounts window.
- ⌘, and ⌘N choose their items while the menu is open, and close it: Settings comes forward, or
  the Accounts window with its Add Account sheet. ⌘Q quits.
- Right on Open claude.ai opens its submenu, work, old and personal, with work highlighted; Left
  closes it and leaves Open claude.ai highlighted (run 40206476781). The menu stands at the
  screen's right edge, so the submenu opens to its left.
- Resting the pointer on Open claude.ai opens its submenu within 0.34 s of the pointer starting
  to move there (recorded, runs 40204121073 and 40206476781).
- On the fixture a switch is quicker than the test can see: In Use showed 0.11 to 0.13 s after
  the click on the row's button returned, and no "Switching…" was seen. Refresh was never seen dimmed
  (recorded, runs 40204121073, 40206476781, 40211517451 and 40247375339).

### macOS: the menu bar with the Accounts window in front

- The bar: the Apple menu, Pitboard, File, Edit, View, Go, Account, Window, Help.
- Pitboard: About Pitboard; Settings… ⌘,; Services ›; Hide Pitboard ⌘H, Hide Others ⌥⌘H, Show
  All; Quit Pitboard ⌘Q. Show All is dimmed. Every item but About has a symbol before it.
- That is a release's app menu, which the site draws. The debug build the test reads is named
  Pitboard Debug. Its Hide and Quit items read Hide Pitboard Debug and Quit Pitboard Debug, and
  About reads About Pitboard. Its app menu is Pitboard on macOS 26, as run 40494932963 read it,
  and Pitboard Debug on macOS 27 ([The macOS app](macos-app.md)). So on CI the menu's pictures
  are `site-menu-bar-pitboard-first` and `site-menu-bar-pitboard-personal`.
- File: Add Account… ⌘N; Open claude.ai ›; Remove Website Data…; Close ⌘W, with Close All in its
  place under ⌥. Remove Website Data… is dimmed.
- Edit is macOS's own, and Copy Report: Undo ⌘Z, Redo ⇧⌘Z; Cut ⌘X, Copy ⌘C, Paste ⌘V, Delete,
  Select All ⌘A, Copy Report (run 40429818225); Find ›, Spelling and Grammar ›, Substitutions ›,
  Transformations ›, Speech ›; AutoFill ›, Start Dictation… fn D, Emoji & Symbols fn E. Undo,
  Redo, Cut, Paste, Select All and Copy Report are dimmed.
  Copy and Delete are dimmed with no account selected, and enabled with one. Delete shows no key.
- View: Refresh ⌘R; Actual Size ⌘0, Zoom In ⌘+, Zoom Out ⌘−; Hide Inspector ⌃⌘I, Enter Full
  Screen fn F. Actual Size, Zoom In, Zoom Out and Enter Full Screen are dimmed. Hide Inspector is
  `InspectorCommands`' own, with a symbol before it (run 40404550549).
- Go: Back ⌘[, Forward ⌘]. Back and Forward are dimmed.
- Account, with no account selected: Switch, Sign In Again…, Rename…, Remove Account… ⌘⌫, all
  dimmed. The window selects work, the account in use, as it opens, so at first the menu is work's
  (run 40404550549). With personal selected: Switch to personal, Sign In Again…, Rename…, Warm Up
  Now, dimmed; Open claude.ai; Remove Account… ⌘⌫. With work, the account in use, selected: the same
  with Switch to work, Warm Up Now and Remove Account… dimmed, and ⌘⌫ asks nothing. No item is red.
- Window: Minimize ⌘M, Zoom, Fill ⌃fn F, Center ⌃fn C; Move & Resize ›; Full Screen Tile; Remove
  Window from Set; Accounts, Diagnostics, Activity, the app's windows, with no symbol (run
  40404550549); Bring All to Front. Full Screen Tile and Remove Window from Set are dimmed. Under ⌥,
  Minimize All, Zoom All and Arrange in Front take their places.
- Help: macOS's search field, then Pitboard Help.
- About Pitboard opens the standard About panel, a dialog of 284 by 170 points whose close
  button alone is enabled: the app's icon, "Pitboard" in bold, and the version and build,
  "0.0.0 (1)" in a debug build, with no "Version" before them.

### macOS: the Remove alert and the sheets

- A sheet on the Accounts window: the title "Remove “personal”?", the message, then Cancel and
  Remove, in that order and of one width. Remove's title is red on a pale red fill, Cancel's
  grey; neither is drawn as the default button.
- Return presses neither button, where AppKit's own rule gives Return to an alert's first
  button. ⌘. and Escape press Cancel.
- The alert and the Add Account sheet each sit in the middle of the window, across and down. The
  window under either is drawn over: in light, its white reads 204 and its sidebar's grey 242
  reads 199, as rgb(172, 172, 172) at 62% over it would leave them, and its labels grey further.
  Its close button greys to 220 while minimize and zoom keep their colours (recorded, pictures
  `site-forget-alert-window` and `site-status-menu-key-equivalents`, against
  `site-focus-after-forget`, run 40247375339). Dark was not read.

### macOS: the account list

- At first the account in use of the first tool is selected, work in `oneTool`, and the inspector
  beside the list shows it (`AccountsWindow.swift`'s `onAppear`, asserted by
  `testSelectingAndTypingInTheAccountList` and read from source before a run of it). A click
  selects a row, in the accent colour with white text, and the inspector shows that account. Up
  and Down move the selection. Typing an account's name selects it: "w" selects work.
- The window has no sidebar: Diagnostics and Activity are windows of their own in the Window menu.
  Control-Command-I, the View menu's Hide Inspector in run 40404550549, and the toolbar's Inspector
  hide the inspector and show it again, and the list keeps the keyboard
  (`testTheInspectorHidesAndTheListKeepsTheKeyboard`).
- **Switch** switches without selecting its row.
- A right click on a row not selected leaves the selection where it was and draws an accent
  outline round the row clicked. The shortcut menu acts on that row: Switch to personal, Sign In
  Again…, Rename…; Open claude.ai; Copy Email Address; Remove Account…, with no keys and nothing
  red. Copy Email Address copied personal's address.
- After Remove Account, no row is selected and the list keeps focus: Down selects work.

### macOS: Limits

- The pop-up opens its menu with the chosen item, checked, over the button: Do Nothing, Notify
  Me, Switch Automatically.
- The stepper steps by 1 a click and repeats while held: from 50, 6 steps in a hold of 0.5 s, 16
  to 17 in 1 s and 36 in 2 s. That is one step at the press, then 20 a second from 0.3 s on
  (recorded, runs 40206476781 to 40252330837, the holds of 0.5 and 2 s in 40252330837 alone). It
  stops at 50 and 99, and a click past a bound changes nothing.
- On Return the field clamps a typed share to 50 to 99, puts back the last share for text that
  does not parse, and takes "90" without "%". The Up arrow in the field changes nothing.
- The field took each key as it was typed until run 40404550549: "7" with no Return set the share
  to 50 within a second (recorded, runs 40204121073 to 40247375339), a share clamped while typing
  could rewrite the text under the caret, "80%" typed with no Return leaving "50%0%" in the field
  (run 40196222517), and "120" then Return left "120" in the field over a share of 99 (run
  40404550549). Since, `ShareField` in `LimitsSettings.swift` sends a typed share on Return or as
  the field loses focus, as the site does.
- The warm-up toggles are switches, and From and To are time fields with a stepper beside each.
  How fast their steppers repeat was not read: the site repeats them as the share's.

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
  text: "5-hour 42% used, weekly 12%", "Sign-in expired", "Updated 3:10 PM" on Refresh, none on
  the headings (run 40292624691).
- It opens with focus on work · 42%, the first enabled item, whose tooltip then shows (recorded,
  picture `site-tray-menu-mid-screen` of run 40206476781). The first
  Down goes to old. Down and Up pass over the headings and wrap: Down from Exit goes to work, and
  the first Up after opening to Exit. Home, End, a letter and Tab move nothing.

### Windows: the account list and its dialog

- A right click on a row not selected leaves the selection where it was and draws no outline.
  The context menu puts focus on its first item, Switch to personal, then lists Sign in again…,
  Rename… F2; Open claude.ai; Copy email address Ctrl+C; Remove account… Del. Copy email address
  copied personal's address.
- Delete on the selected row opens the Remove dialog with focus, and its focus rectangle, on
  Remove, the primary button on the left; Cancel is on the right. Enter presses Remove.
- Each question's default button is the accent one, which the focus opens on: its verb, or
  Cancel where the model says the verb is not the default, as of Uninstall Pitboard?
  (`DialogHost.cs` `Buttons`, read from source). The Name step after a sign-in offers Done alone
  (`SheetPage.cs` `CloseTitle`).
- After Remove no row is selected and focus is on the window's title bar, `AppTitleBar`, not the
  list: Down moves nothing.
- On the fixture, In use showed 0.19 to 0.37 s after the row's button was invoked on
  windows-2025 and 0.25 s on windows-11-arm, and no "Switching" was seen. Refresh was disabled
  for 14 to 24 ms (recorded, runs 40204121073 and 40206476781).

### Windows: Settings, windows and Notification Center

- The ComboBox offers Do nothing, Notify me and Switch automatically, Notify me chosen. The
  NumberBox holds 50 to 99 and steps by 1 and by 5. At 99 its Increase button is disabled, at 50
  its Decrease; a share set past a bound is clamped to it.
- An account window is titled "work" until its page has a title, then "claude.ai stand-in -
  work", the page's title first. For its first moments it has its XAML's title, "Account", until
  it reads its account: run 40208954337 saw it at 73 ms (recorded).
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
- The macOS menu's issues entry, "1 Issue" over the first pressing notice's title, and the badged
  mark in the menu bar: no fixture leaves a notice that presses. The site draws the entry from
  `MenuBarContent.swift` and the mark from the asset catalog's
  `pitboard.badge.exclamationmark.symbolset`.
- macOS 27's own drawing: the chosen item's blink, the alert, and the red of a destructive
  button.
- A macOS window under a sheet in dark. The site draws black at 40% over it until it is read.
- What eyes or a phone judge: the blink's timing, tooltip delays, VoiceOver and NVDA, how long a
  banner stays on macOS 27, and a long press.

## The Linux app on GNOME and KDE Plasma

The Linux figures draw one Fedora 44 computer, with stock GNOME 50 or with KDE Plasma 6.7. No
runner reads them as `SiteFiguresTests` reads the other apps. They are drawn from the app's
source and checked against CI's pictures of it ([The pictures](linux-app.md#the-pictures)), with
what the Linux sessions of 30 October 2026 saw
([The Linux session machines](linux-session.md)), `linux-facts.yml` run 41107324579, and the
toolkits' and desktops' source at the versions named. `e2e/linux-pictures.spec.ts` has not been
run against the final drawings yet.

### Read in the Linux sessions

- The status item's menu, drawn by Plasma and by the AppIndicator extension alike: Open
  Pitboard; each tool's name as a disabled header where there are two; its accounts as radio
  entries named with the share used, "work (42%)", the one in use checked; Add Account…,
  Refresh, Preferences, Help and Quit, with separators between. A click on an account switched
  in 1.06 s through Plasma, with no notification, and the tooltip followed.
- Plasma's tooltip for the item: "Pitboard" over a line an account in use,
  "<name> (Claude Code): 5-hour 0% used, weekly (Fable) 0%".
- A left click on the item in Plasma's panel, Firefox in front, brought the window in front.
- The limit's notification: "5-Hour Limit Almost Reached", "Claude Code" and "<name> has used
  93%. <other> has used 0%.", from GLib through `org.freedesktop.Notifications` on Plasma, which
  drew it as a "Pitboard" popup with a "Switch to <other>" button, and through
  `org.gtk.Notifications` on GNOME, which drew a banner, then a message under "Pitboard" in the
  date menu. A click on either, Firefox in front, brought the window in front.
- The window after a switch: the notice "Switched Claude Code to <name>" over "Claude Code
  sessions already open follow in 0:31", counting down.
- GNOME Shell's search gives "Switch to <name> (Claude Code)" with the account's limits, or "In
  Use", and switches with no window, and KRunner's match switches since #362, which the
  questions' row on search rests on.
- Closing the window where nothing shows the status item ends the app. Uninstall names
  `sudo dnf remove pitboard-app pitboard` on Fedora.
- No desktop set `clock-format`, so GNOME's clock follows `LC_TIME`: 12-hour under en_US, 24-hour
  under en_GB. The page's computer runs under en_GB, so the window, the top bar, the date menu
  and Plasma's clock say 24-hour times and en_GB's dates.

### The KDE Plasma window

`linux-facts.yml` run 41107324579, on 31 October 2026, installed Fedora 44's
`@kde-desktop-environment` (Plasma and KWin 6.7.5, xdg-desktop-portal 1.22.1,
xdg-desktop-portal-kde 6.7.5 and -gtk 1.15.3, GTK 4.22.5, libadwaita 1.9.4), applied Fedora's
global theme as `startplasma` does, loaded kded6's gtkconfig, and read the Settings portal and a
stand-in window built as the app's on a private bus. Fedora's xdg-desktop-portal-kde requires
the gtk backend, and Plasma's `kde-portals.conf` asks both for Settings, so GTK takes what
gtkconfig writes to dconf over any `settings.ini`:

| | KDE Plasma on Fedora 44 | Stock GNOME |
| --- | --- | --- |
| Decoration layout | `icon:minimize,maximize,close` | `appmenu:close` |
| Header bar | the app's icon at 16 in a 34 box before Add; Refresh, the main menu, Minimize, Maximize and Close | Add; Refresh, the main menu and Close |
| Font | Noto Sans 10, 13.33 pixels | Adwaita Sans 11, 14.67 pixels |
| Icons | Breeze's `actions/16`, GTK's own dialog icons, libadwaita's `adw-` icons | Adwaita's |
| Accent and colours | blue, `#3584e4`, libadwaita's own light and dark | the same |

Breeze's `#3daee9` reaches libadwaita as an accent and snaps to its blue. Breeze Dark turned
`color-scheme` dark and changed nothing else. Breeze has no `battery-level-50-symbolic`,
`package-x-generic-symbolic` or `web-browser-symbolic`, which the app carries in its own
resources ([The icons](linux-app.md#the-icons)), so the page draws Adwaita's for them.

Fedora's global theme gives Plasma itself the breeze-dark style with the BreezeLight colour
scheme, read from the theme files. So the panel, its tooltip and its popups stay dark in both of
the page's schemes, while the app, Konsole and the tray menu follow the page's. The tray menu is
a QMenu plasmashell makes of the app's DBusMenu (`dbusmenuimporter.cpp`), in Breeze's widget
style and the colour scheme's colours, read from source.

### Read from source

- GTK 4.22.0's popover menus choose on the primary button's release, move focus with the
  pointer, cycle with the arrows and Tab, and bind no Home or End (`gtkmodelbutton.c`,
  `gtkpopovermenu.c`).
- Its spin buttons step with the arrows and a page with Page Up and Page Down, go to their ends
  with Ctrl+Home and Ctrl+End, wrap to the far end only from an end, and repeat a held button
  after 500 ms every 50 ms (`gtkspinbutton.c`).
- Qt 6.10.0's QMenu chooses on any button's release off Windows, moves to the first and last
  item with Home and End in Breeze's scrollable menus, takes Tab for Down, and leaves Space
  alone (`qmenu.cpp`).
- libadwaita's alert focuses its default response, so Remove is focused
  (`adw-alert-dialog.c`), and a preferences group has the group role, so an account's group is
  no landmark.
- The window's keys: Ctrl+N, Ctrl+R, Ctrl+Comma, Ctrl+?, F10, Alt+Left, Ctrl+W and Ctrl+Q
  (`apps/linux/src/window/keys.rs`).
- GNOME Shell keeps a banner 4 s (`NOTIFICATION_TIMEOUT`, `js/ui/messageTray.js`), Plasma a popup
  5 s (`PopupTimeout`, `notificationsettings.kcfg`), and Plasma's tooltip waits 700 ms
  (`ToolTipArea`'s `Delay`).
- At 95% the tray icon asks no attention: the model's advice, which puts a switch in the menu,
  comes only once an account has run out (`crates/pitboard-model/src/model/advice.rs`).

### Not pictured

- The tray menu, the tooltip, the panel and the popup as Plasma draws them: the session kept no
  picture, and CI's pictures have no tray.
- GNOME Shell's top bar, banner and date menu, drawn from gnome-shell 50's source.
- The KDE window's pixels: the portal read gave its settings and the files they resolve, not a
  rendering.

### Not measured on Linux

Each is drawn from the app's source:

- A click on the notification's Switch button: the popup had gone first.
- An automatic switch, by the app or by the watch service.
- A warm-up that started an account's 5-hour limit.
- A renewal that refreshed a login: each run found none due.
- How long a switch takes on GNOME. The page takes Plasma's 1.06 s for both.
- How long Refresh reads. The page takes Windows' 24 ms.

## Where the site departs from the apps

Each is the price of a web page or a WCAG rule, or something the page cannot do. Code that departs
says so in a comment naming its line here, which `website/src/scripts/departures.test.ts` checks.

- Shift+Tab leaves the PowerShell prompt, where PSReadLine binds it to `TabCompletePrevious`:
  no keyboard trap (WCAG 2.1.2).
- Escape then Tab leaves every prompt, where zsh puts a tab in, bash runs
  `dynamic-complete-history` and PowerShell reverts the line: the way forward out of a field
  that binds Tab, which each prompt names to a screen reader.
- Ctrl+R's history search is not drawn: each shell draws a search line of its own.
- The bell is silent: a page that beeps would surprise.
- Tooltips stay while the pointer is over them and Escape closes them, where a native one goes
  once the pointer leaves its owner (WCAG 1.4.13).
- The macOS window's menu bar is a Tab stop, which macOS's never is: without it Rename, Sign In
  Again, Open claude.ai and Warm Up Now would need a pointer (WCAG 2.1.1).
- A sheet, an alert, a ContentDialog or an AdwDialog blocks the whole page, where the app blocks
  only its window: HTML's modal dialog does.
- A simulated sign-in keeps Copy Link and Open dimmed, shows no Code field, and draws no Claude
  window while Claude Desktop's waits: the page loads no sign-in page.
- Open Pitboard, Settings on Windows, Preferences on Linux, Warm-Up Settings, Warm-Up
  Preferences, a click on a notification and the tray icon bring the page's figure into view,
  where the app brings a window forward.
- Check Now and Uninstall Pitboard do nothing: the page updates and removes no app.
- After Remove Account on Windows the page draws a focus ring on the title bar, where WinUI draws
  none (WCAG 2.4.7).
- The Windows icon draws no warning over it while its menu needs attention, where the app lays the
  Shell's stock `SIID_WARNING` over it: the page has no drawing of that icon. Its tooltip and name
  say "Needs attention", as the app's do.
- The Mac's clock opens nothing: macOS's Notification Center was not read.
- Uninstall Pitboard on Windows quits Pitboard until Reset, after its question: the page removes
  nothing and starts no installer.
- Uninstall Pitboard on Linux quits Pitboard until Reset, after Remove the App: the page removes
  no package.
- Closing the Pitboard window on GNOME, or Ctrl+W there, quits Pitboard until Reset, as the app
  quits where nothing shows its status item: the page has no overview to start it from again.
- Maximize keeps the Linux window at the size its desk draws, and Minimize hides it until the
  tray's Open Pitboard or a click on the tray icon: the page's desk has no screen to fill and no
  task manager.
- The Linux window's Ctrl+N, Ctrl+W and Ctrl+Q stay with the browser where it reserves them, as
  [above](#keys-a-browser-keeps-from-a-page): the page is not given them. The main menu and the
  buttons do the same.
- Paste Link in Open Link reads nothing, where the app reads a link from the clipboard: the page
  reads no clipboard it was not given.
- KDE Plasma's notification history is not drawn: the tray's bell shows while the notification
  is unread and opens nothing, since Plasma's history was not read.
- Diagnostics stays on its spinner, and its buttons to check again and copy the report are held
  back: the page draws no computer's checks yet.
  The macOS figures draw no Diagnostics window, which the page has no figure for.
- `--json` of `pitboard`, status, doctor and watch prints nothing: those reports are not drawn
  from the page's state yet.
- On a phone each figure fits the page's width, so a macOS menu row on a 390 pixel phone is 13
  pixels tall (WCAG 2.5.8). A terminal's field takes no tap there, since iOS zooms into a field
  under 16 pixels: a phone runs a terminal's commands from the buttons under it. The Linux window
  is drawn 600 wide and shrunk there, so its dialogs float where libadwaita draws a bottom sheet
  in a window under 450 pixels.
