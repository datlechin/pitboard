// One line per release, betas included, for the home page's Recent releases, 60 characters at
// most. The build fails when the newest version in CHANGELOG.md, or one the table shows, has none.
export const highlights: Record<string, string> = {
  "1.0.0-beta.8": "Pitboard has a Linux app for GNOME and KDE Plasma.",
  "1.0.0-beta.7": "The macOS app keeps working through a switch.",
  "1.0.0-beta.6": "Both apps are redone, and the macOS app has widgets.",
  "1.0.0-beta.5": "Claude Desktop accounts switch again with Claude 3.",
  "1.0.0-beta.4": "Pitboard warms up Claude Code accounts on Windows too.",
  "1.0.0-beta.3": "Claude Desktop accounts switch again with Claude 2.42304.0.",
  "1.0.0-beta.2": "The Windows app turns on daily renewal of parked logins.",
  "1.0.0-beta.1": "Pitboard runs on Windows 11 24H2 and later, in beta.",
  "0.15.2": "Claude Desktop accounts switch after a Claude 3 sign-in.",
  "0.15.1": "Claude Desktop accounts switch again with Claude 3.",
  "0.15.0": "**Add Account** says what to install when a tool is missing.",
  "0.14.0": "Pitboard switches Claude Desktop accounts on macOS.",
  "0.13.0": "Warm-up starts a five-hour period once the last one resets.",
  "0.12.0": "An install script puts the command line on macOS and Linux.",
  "0.11.0": "`forget`, `stow` and `uninstall` need `--yes` when they cannot ask.",
  "0.10.0": "`pitboard stow` puts away a login file behind the keychain.",
};
