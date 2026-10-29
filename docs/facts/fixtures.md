# The fixtures

Measured on 6 October 2026 on macOS 27.0, with `pitboard-ffi` built for
`aarch64-apple-darwin`, in release and in debug, with and without the `fixture` feature.

- The bindings are the same with the feature and without it: the Swift that
  `uniffi-bindgen-swift` generated from each release static library, `PitboardBindings.swift`,
  `PitboardFFI.h` and `module.modulemap`, and the C# that uniffi-bindgen-cs v0.11.0+v0.31.0
  generated, `pitboard_ffi.cs`, had the same SHA-256 each, and the C# generated from each
  debug library did too. The feature changes what the fixture's four exports do, never
  their names, arguments, types or doc comments, which are all a checksum covers
  ([The C# bindings](bindings.md#the-c-bindings)). So one set of bindings links against either library.
  Measured again on 28 October 2026, with the model in `pitboard-model`: the C# generated from
  the debug library with the feature and without, now `pitboard_model.cs`, was the same byte
  for byte, and so were the Swift files generated from the universal release library
  `build-xcframework.sh --fixture` built and the one `build-app.sh` built without it.
- A library built without the feature holds none of a fixture's text: `grep -a -c` for a
  line of a stand-in page and an account of the worlds found it on no line of the release
  `libpitboard_ffi.a` or `libpitboard_ffi.dylib`, and on 2 lines of each built with it, one
  for each, and on 4 of the universal library `build-xcframework.sh --fixture` builds.
  CI's C# jobs check the Linux library and the gated Windows DLL the same way, and
  `build-app.sh` the macOS library and app.
- `fixture_page` gives, byte for byte, what the macOS app's `FixturePages.page(for:)` gives,
  for nine addresses on `pitboard-fixture://`: each site's home and a path of each, two of
  chatgpt.com's sign-in hosts, the artifact's frame, a host of no site, and chatgpt.com in
  capitals with an escape and a slash at its end. Measured by running the Swift's code,
  copied whole, with Swift 6.4 over the same site table.
- `ModelTests.AFixturesModelStartsAndTellsAListenerOfItsAccounts` starts a fixture's model
  from C#, against the debug library built with the feature, and its listener, written in
  C#, is told the accounts read on a thread that is neither the test's nor one of .NET's
  thread pool, with the .NET SDK 10.0.401. Against the library built without, the test is
  skipped and says why.
- The real core answers some things in the fixtures otherwise than the Swift `FixtureCore`
  did, which the UI tests were written against and read since the macOS app launches into
  the Rust worlds, each held by a test of the fixtures, the core or the `--json` contract:
  - A read whose service cannot be reached answers, each account's numbers the last
    measured and saying "Anthropic could not be reached"; `status` fails only where the
    account index cannot be read, and the read of what is known fails with it. The Swift
    fixture failed readFailure's read, `unreachable`, and listed its accounts from what was
    known. So readFailure's account index is one nobody may read, mode `000`, given it once
    the world's history is made, and both its reads fail, `state_unreadable`: "could not
    read Pitboard's account list at …: Permission denied (os error 13)", on macOS 27.0 as an
    ordinary user. The menu's item and the window's notice say "Couldn’t read usage" with
    those words, and the window, with nothing known to list, says "Couldn’t Read Accounts"
    with Try Again, where the Swift fixture listed claude/work under the notice. A service
    out of reach is held in oneTool instead, made so in its test.
  - The fixture's Claude Code refuses a line typed back without both halves of
    `<code>#<state>`, with its own `Invalid code.` line, and reads another, as the register's
    `sign_in_takes_another_code` holds of 2.1.289; the Swift fixture took any line. So a UI
    test types a code with its `#`, such as `fixture-code#state`, and one typed without
    meets the sheet saying the code was refused, with the field offered again.
  - A switch is logged as `use`, after the command, where the Swift fixture logged `switch`.
  - Claude Code's sessions follow a switch within 33 seconds, `ADOPTION_CEILING_SECONDS`,
    where the Swift fixture said 45.
  - Doctor's checks are the core's: none is called "Keychain", and none warns of a schedule
    that is not there, which was the Swift fixture's one thing worth looking at. oneTool's
    one thing is the world's own instead: personal's parked login lapses in two days, where
    the Swift fixture's lasted eleven, and doctor warns of a parked login whose refresh token
    lapses within three days. So oneTool says "One thing is worth looking at." as before, of
    personal's parked login rather than of daily renewal. stuck has doctor's "interrupted
    switch" worth looking at besides, so it says "2 things are worth looking at.".
  - A read lists the account in use first, and the read of what is known lists only
    enrolled accounts, so a login nobody has named is listed once the first read has asked
    its service whose it is.
- leftInFile, which the Swift fixture never had, leaves a login of personal's in
  `.credentials.json` behind Claude Code's store. Its read warns `fallback_login` with the
  notice's Put Away…, the read of what is known says nothing of it, and Put Away keeps the
  login for personal and deletes the file.
- loginReplaced, which the Swift fixture never had, is oneTool after personal signed in
  with Claude Code itself over work's only login, recorded by a terminal's read ten minutes
  before. Every read, the one of what is known too, warns `login_replaced`, work's row reads
  `login_replaced`, and the app's first read posts it.
- stuck shows the notice offering Give Up… as the app starts, as the Swift fixture did, now
  from the core's own read: a read says an interrupted switch is waiting wherever the next
  change would be refused over it, as a warning with the refusal's code,
  `recovery_undetermined`, and its words. The read of what is known says nothing of it
  there, since only Anthropic could say whose Claude Code's login is.
