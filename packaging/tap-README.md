# datlechin's Homebrew tap

```sh
brew install datlechin/tap/pitboard            # the command line, macOS and Linux
brew install --cask datlechin/tap/pitboard-app # the menu bar app, macOS 14 or later
```

[pitboard](https://github.com/datlechin/pitboard) switches between your own Claude Code
and Codex logins and shows how much each one has left. The app includes the command line,
so install one or the other. pitboard's
[README](https://github.com/datlechin/pitboard#install) has the rest.

If you installed the app as `datlechin/tap/pitboard` before, that name is the command line
now. To get the app back, with the command line inside it, run these in this order:

```sh
brew uninstall --cask pitboard
brew uninstall --formula --force pitboard
brew install --cask datlechin/tap/pitboard-app
```

Leave `--zap` out when you remove the old app cask, whose zap moves `~/.pitboard` to the
Trash.

pitboard's release writes every file here, from `packaging/` in that repository, and a
change made here is replaced at the next release.
