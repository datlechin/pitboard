# The macOS app

## What the Swift app said

Measured on macOS 27.0 with Swift 6.4 on 5 October 2026, so that `pitboard-ffi`'s
`present` says what the Swift app said.

- VoiceOver heard a limit's reset as a span Foundation's `Duration.UnitsFormatStyle` said:
  days, hours and minutes, wide, at most two units, in `en_US_POSIX`, of at least a minute.
  Asked for 100 spans, it rounds the span to whole minutes, a half to the even one, to see
  which units it has, and says the two largest that are not nought, so 1 day and 1 minute
  is "1 day, 1 minute". The last unit it says is the rest of the span in that unit, rounded
  the same way, and a rest that rounds up to a whole one of the unit before it is carried:
  1 day 23 hours 30 minutes is "2 days, 0 hours". It writes no thousands separator: 11,574
  days is "11574 days, 2 hours". `words::spoken_span` is tested against every one.
- The menu bar cut a label past 12 `Character`s to 11 and an ellipsis, and Swift counts
  extended grapheme clusters. Over 22 labels, accents written as combining marks, flags,
  emoji joined by zero-width joiners, keycaps, tag sequences, Hangul written in jamo, Thai,
  Arabic's prepended number sign, a carriage return before a line feed and Devanagari
  conjuncts, joined and not, `unicode-segmentation` 1.13.3's extended graphemes counted and
  cut each as Swift did.

## What the macOS app's glue rests on

Measured on macOS 27.0 with Swift 6.4 of Xcode 27.1, on 6 October 2026, unless a fact gives
its own date.

- Foundation's temporary directory does not read `TMPDIR`: with `TMPDIR=/private/tmp/xyz/`
  and with it unset, `NSTemporaryDirectory()` and `FileManager.default.temporaryDirectory`
  both gave the user's own `/var/folders/…/T/`. Rust's `std::env::temp_dir` reads `TMPDIR`
  first, and only then asks the system for that directory (Rust 1.98.1's
  `library/std/src/sys/paths/unix.rs`). So the app finds a fixture's folder by `TMPDIR`
  where it is set, as the core makes it, and by Foundation where it is not.
- `@Observable` does not tell an observer of an assignment of a value equal to the one held:
  over `withObservationTracking`, setting an equal `Equatable` struct, an equal `Int` or the
  same object again told nothing, and setting another struct told. So `AppModel`'s
  comparison before each assignment changes nothing that Observation does not already skip,
  and a test of what is drawn again passed with the comparison taken away. The comparison is
  kept so the rule does not rest on how the macro expands, and `apply` returns the parts it
  assigned, which its test reads: with the comparison taken away, that test fails.
- `Date.FormatStyle` with `time: .shortened` follows the locale's hour cycle, which macOS
  keeps in the locale: 14:05 in UTC is `2:05 PM` for `en_US`, with U+202F NARROW NO-BREAK
  SPACE before `PM`, `14:05` for `en_GB` and for `en_US@hours=h23`, and `2:05 pm` for
  `en_GB@hours=h12`. `MacLocalTime` formats with the person's own locale, and its test with
  `en_US` and `en_US@hours=h23`.
- Read on 12 October 2026: `NSWindow.h` in the macOS 27.0 SDK says a window that has a
  sheet queues another until the first is dismissed. So an alert attached to the window
  waits while a sheet is up, and one attached to the sheet's own view is a sheet of the
  sheet's window and shows over it. The window's failure alert and quit question are
  attached through `WindowAlerts` to whichever is in front.
  `testAddingAClaudeDesktopAccountAsksToQuitClaudeOverTheSheet` holds it.
