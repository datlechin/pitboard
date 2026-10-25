# The site's terminals

What the terminals on usepitboard.com draw for each system, and the keys a browser keeps from a
page. Read and measured on 25 October 2026, unless a line names another date:

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
