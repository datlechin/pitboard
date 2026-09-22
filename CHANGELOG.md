# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- No parked login goes unnamed. Every name pitboard is about to write a login into is
  written down first, and the next command resolves any that nothing refers to: given back
  to the account whose name it carries when that account holds nothing, deleted when nobody
  wants it, and left alone when the store could not be read. Before this, a run killed
  between writing a login and recording it left a live refresh token that no entry named,
  never renewed, never deleted by `pitboard uninstall`, and on macOS not listable by any
  tool a person has. `pitboard doctor` reports anything still outstanding.
- `pitboard repair` asks the credential store itself what parked logins are on this
  machine, rather than reading pitboard's own index, and accounts for every one it finds:
  given back to the account whose name it carries, or deleted when no account here wants
  it, or reported and left exactly where it is. Only a name this pitboard wrote down itself
  is ever deleted: a keychain belongs to a whole login session while pitboard's records
  belong to one `PITBOARD_HOME`, so a parked login it cannot account for is evidence of
  another pitboard rather than of an orphan, and deleting it would end that account's
  session for somebody who never ran the command. Giving one back is additive and safe on a
  guess; deleting one is not. Measured first: `security dump-keychain` without `-d` never
  prompts, takes 0.06 seconds, emits no secret of any item, and does not slow later reads.
- A crash matrix: every durable step of a switch, an enrolment, a renewal and a forget,
  killed where it stands, recovered, and checked against what must be true afterwards
  rather than against a particular outcome. Every case runs recovery twice, because a
  recovery that only works once leaves a machine nobody can fix, and once with Anthropic
  unreachable, because the answer then must be to change nothing. The two windows above
  are what it found on its first run.
- A failure that came from asking Anthropic now carries a cause beside its code, in both
  `--json` and the app's bindings: `unreachable`, `rate_limited`, `server_error`,
  `answer_not_understood`, `login_refused` or `token_expired`, each saying whether asking
  again is worth anything. Before this, everything that was not a 401 arrived as
  `identity_unverifiable` and a sentence of prose, so nothing could tell being offline from
  being rate limited from a login Anthropic had finished with. `status` tells the same three
  apart too, where they used to share one code.

### Changed
- `pitboard-core` says what it supports. The interface other programs may build on is
  `service::Pitboard`, `context::Context` and what they return; the rest is reachable for
  this repository's own front ends and may change in any release. The enums a caller reads
  codes out of are now `#[non_exhaustive]`, so adding a code is not a breaking change for a
  consumer, which is what the command line's JSON contract has always promised. Writing
  pitboard's index is no longer reachable from outside the crate: every change goes through
  `switch`, which records its intent first. Marking those enums and withdrawing `state::save`
  are themselves breaking changes for anyone who built on 0.2.0, so this is the release that
  makes them, while the crate is young enough for that to cost nothing. Nothing changes for
  anyone using the command line or the app.
- Claude Code's supervisor daemon is named as what it is, a second writer of the login that
  runs on a schedule of its own. It takes the same write lock and re-reads the credential
  inside it, so it cannot put an older account back over a switch. `doctor` reports it.
- The storage v5 question is settled rather than open. The successor backend replaces the
  fallback half of Claude Code's chain and only for a caller that hands a backend in, so an
  ordinary `claude` still reads the keychain first. `doctor` now warns only for the
  combination that can mislead, the flag on and the login in the fallback.
- A login too large for `security`'s standard input is now written the only other way
  `security` offers, as an argument, which is what Claude Code does for the same login on
  every token refresh. The switch says so, and `doctor` shows the size. `PITBOARD_NO_ARGV=1`
  refuses instead. Measured first: writing the item in process would have made every later
  read by `security` take about a second instead of 0.01, for good.

## [0.2.0] - 2026-09-22

Everything a stranger hits in the first ten minutes, every state a person could be stuck in,
and what the app was missing to stand on its own.

### Added
- `pitboard log` shows what pitboard has changed and when, from the record it was already
  keeping. The log now names which front end asked.
- `pitboard uninstall` deletes every parked login and then pitboard's own files, in that
  order, because the account list is the only index of those keychain items.
- `pitboard abandon` gives up on an interrupted switch that cannot be finished, keeping
  every login. The way out when recovery cannot reach Anthropic.
- `pitboard status --offline` answers from what was last measured, without asking Anthropic
  or touching a login. Milliseconds instead of seconds, and it works with no network.
- `forget` asks before deleting a parked login, unless `--yes` or `--json`.
- Each usage row in `--json` carries `severity`, Anthropic's own grade for that limit. A
  field added to the v1 envelope; nothing was removed or renamed.
- The app: each account's email, when each limit resets, when a parked login stops working,
  doctor's checks on demand, its own version, a mark in the menu bar, and VoiceOver labels.
  It can record the account in use, drop an account, and run Claude Code's own sign-in for
  a new one, showing what that sign-in says rather than borrowing a terminal.
- `cargo binstall pitboard` fetches the built binary instead of compiling the tree.

### Fixed
- `pitboard statusline` typed at a prompt waited for input that was never coming. It reads
  stdin only when something is piping into it.
- Offline, the account in use rendered as one with nothing parked, advising a sign-in it did
  not need.
- A switch that failed before installing anything left its journal behind, so the next
  command announced a recovery for something that never happened.
- `enroll --sign-in` checked what could refuse the enrolment only after the browser sign-in.
- The status line showed `?·?` for every account but the one in use until someone ran
  `pitboard` by hand. It now keeps the numbers Claude Code hands it.
- doctor and forget read pitboard's record of its last switch rather than who is signed in,
  so a sign-in made with Claude Code's own `/login` made both wrong.
- Three ways a parked login could be left in the keychain with nothing naming it.
- A renewal that could not be written left the account with a login already spent.
- The keychain ceiling has its own error, saying the size, the limit, and what to do. A
  login with MCP server tokens in it is past that limit, which is not theory.
- `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN` and `CLAUDE_CODE_OAUTH_TOKEN` raise a warning
  on every change: Claude Code signs in with those, not with the login pitboard moved.
- Columns line up in what a terminal draws, so a label in Chinese or Japanese no longer
  pushes everything after it out of line.
- One state file serves every credential slot, and what was switched to in one slot is no
  longer claimed in another.
- The menu bar showed the largest percentage of any limit, so a row scoped to one model
  read as though everything had stopped.
- Windows gets one sentence instead of a screen of type errors.

### Internal
- MSRV is 1.91, measured by building it, and CI builds at whatever the manifest claims.
- The state file can be read forwards, and says which half to upgrade when it cannot.
- A release is guarded, re-runnable, and carries build provenance; the macOS command line
  binaries are signed and notarised like the app. A tag like `v0.2.0-rc1` is a pre-release:
  no crates.io, no update feed.
- The app can be tested without a keychain, and is.

## [0.1.4] - 2026-09-22

### Fixed
- A switch no longer leaves the outgoing account's `trustedDeviceToken`, `organizationUuid`,
  `enterpriseGateway` or `designOauth` behind for the incoming account to present as its
  own. Claude Code deletes all of them with the login on logout; pitboard now does the same,
  which is the state a logout and a fresh sign-in leave.
- Renewing a parked login whose answer carries no refresh-token lifetime keeps the deadline
  it had, as Claude Code does. Dropping it made a park that was about to lapse look as
  though it never expires, so pitboard went on offering and renewing it.
- pitboard refuses to act when `CLAUDE_CODE_CUSTOM_OAUTH_URL` is set. Claude Code then keeps
  its login under a different name, so pitboard would park nothing and restore into an item
  nobody reads.

## [0.1.3] - 2026-09-22

### Added
- The menu bar panel says when an account has run out and which account has the most left,
  whether or not notifications are allowed, and asks for permission only when there is
  something to say. After a switch it counts down the time until sessions that were already
  open follow.
- A Homebrew tap: `brew install datlechin/tap/pitboard`, and `--cask` for the app.

### Fixed
- The keychain account name now falls back to the passwd entry when `USER` is not in the
  environment, which is what Claude Code does. Without it, pitboard run from a launchd
  agent, a cron job or an app opened from Finder read a different keychain item than the
  one Claude Code writes, and reported no login where there was one.
- Renewing a parked login issued to another client now renews it as that client, instead of
  as the first-party one.
- The app's build number now counts up with its version. 0.1.2 shipped with the build
  number the template carried, which Sparkle would have read as newer than the release
  after it.

### Internal
- Every assumption pitboard makes about Claude Code re-checked against 2.1.278. Three
  comments described behaviour that has changed: the credential cache is a rolling window
  rather than one anchored at process start, `/logout` gives up on the write lock after 7.5
  seconds and deletes without it, and the organization fields in the config come from
  separate fetches and are usually absent.
- A release now fails if the update feed is missing or unsigned, and a job after the release
  reads the feed back the way an installed copy will.
- Dead code, duplicated constants and a thrice-written test fixture removed.

## [0.1.2] - 2026-09-22

### Added
- A macOS menu bar app: the account in use and its tightest limit in the menu bar, every
  account's limits in the panel, one click to switch, a notification when an account runs
  out, and launch at login. It calls the same core the command line does, directly.
- `status` renews a parked login whose access token has expired, so every account's usage
  is live and a parked login no longer lapses after its refresh token's lifetime. Only
  parked logins are renewed; the one signed in stays Claude Code's to renew. A login
  Anthropic refuses is dropped, with the command that signs in to it again.

## [0.1.1] - 2026-09-21

### Fixed
- `enroll --sign-in` no longer holds pitboard's lock while the browser sign-in waits, so
  `use`, `forget` and `rename` go ahead meanwhile; a second sign-in is refused, not queued.
- What Claude Code's sign-in prints goes to stderr, so `enroll --sign-in --json` prints
  exactly one JSON line.

## [0.1.0] - 2026-09-21

First release.

### Added
- `enroll`, `use`, `forget`, `status`, `doctor` and `statusline`.
- Live usage in `status`, asked of Anthropic for every enrolled account at once, with
  which accounts can be switched to and until when.
- `enroll <label> --sign-in` on an enrolled label renews its parked login.
- `rename` changes an account's label; its parked login stays as it is.
- A switch refuses a parked login that has expired instead of installing it.
- `doctor` checks every parked login, and reports an interrupted switch.
- `--json` on every command, emitting a versioned envelope with stable error codes, also
  for a mistyped command line.
- Shell completions and a man page, generated from the command definition.
- Linux support, using a file vault for parked logins. Not yet confirmed against a
  signed-in Claude Code on Linux.
- An audit log of every change pitboard makes.

### State file
- Schema 3: one parked login per account, with when it expires. Earlier files are refused
  rather than migrated; nothing was ever released that wrote them.

[Unreleased]: https://github.com/datlechin/pitboard/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/datlechin/pitboard/compare/v0.1.4...v0.2.0
[0.1.4]: https://github.com/datlechin/pitboard/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/datlechin/pitboard/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/datlechin/pitboard/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/datlechin/pitboard/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/datlechin/pitboard/releases/tag/v0.1.0
