# The macOS app

## What the Swift app said

Measured on macOS 27.0 with Swift 6.4 on 5 October 2026, so that `pitboard-ffi`'s
`present` says what the Swift app said.

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
- Read on macOS 27.0.1 (26A434): the Notifications pane's settings extension takes the
  `x-apple.systempreferences` scheme (`allowsXAppleSystemPreferencesURLScheme`) and reveals
  an app by its bundle identifier, which Apple does not document. So the settings' **Open
  Notification Settings** opens
  `x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=` and the app's
  bundle identifier. Nobody has opened the URL to see it land on Pitboard's page.
- Measured on macOS 27.0.1 with Swift 6.4 on 27 October 2026, in an offscreen `NSWindow`
  holding an `NSHostingController`: SwiftUI's `.inspector` adds its column's width to the
  window's minimum only for a minimum set on the view it is attached to. With the list's
  minimum width there, 560 beside a 260-point inspector, the window's `contentMinSize` was 820
  while the inspector showed and 560 once `toggleInspector(_:)` hid it. With the minimum on a
  frame outside `.inspector`, the window kept that minimum alone, and at that width the
  inspector kept its width and clipped the list. Measured the same way on the same day with the
  column's ideal width 280 and its minimum 240: 840 while it showed and 560 once hidden, in 3 of
  3 runs, so the window adds the column's ideal width.
  `theInspectorWidensTheWindowRatherThanNarrowingTheList` holds it.
- Measured the same way on the same day, with a 260-point inspector: `toggleInspector(_:)` sent
  as soon as the window's `contentMinSize` first read 820 collapsed the inspector and left the
  minimum at 820 for 5 seconds in 13 of 26 runs. Sent once the window had been handed over
  300 ms earlier, the minimum read 560 within milliseconds in 18 of 18. So the test waits a
  fixed time before the toggle, and for the minimum after it.
- Measured the same day, the inspector drawn offscreen with `NSHostingView.cacheDisplay`: at 260
  points the weekly limit's "Resets Mon 9:00 AM" wrapped beside "in 19 hr 24 min", and at 280
  it kept one line.
- At 280 points the Warm-Up row's `LabeledContent` keeps **Warm-Up Settings…** beside its two
  lines with "Off", and puts it under them with "Warms up again at 2:21 PM" (recorded, pictures
  `macos-window-accounts` and `macos-window-warm-up` of run 40475483454, on the `macos-26`
  image's macOS 26.6.2).
- Measured the same day: a test window that reaches the window server, such as one made by
  `NSWindow(contentViewController:)`, let `NSAppleScript.executeAndReturnError` run by another
  test on the main thread stop the main run loop, which ended `swift test` with status 0
  before every test had run, in 2 of 3 runs. Its nested event loop pulled the window server's
  events, and a block it left called `CFRunLoopStop` on the run loop Swift's main executor
  runs. A window made with `defer: true`, which never reaches the window server, ended none of
  9 runs.
- Measured on macOS 27.0.1 with Swift 6.4 on 27 October 2026, in an `NSHostingMenu` built
  against the macOS 27 SDK for macOS 14 and for macOS 27 alike: a `Button` or `Toggle` whose
  label is two `Text`s is one item, the first its `title` and the second its native `subtitle`.
  A `LabeledContent` as the label keeps only its label as the title and drops its value, and a
  `LabeledContent` alone, in a submenu too, is two disabled items, its label and its value. So
  the menu bar item's menu gives each account its subtitle with two `Text`s, where plan 4.2
  expected macOS 27 to map `LabeledContent`'s value. `Section` with a title is an item whose
  `isSectionHeader` is true, and a `Toggle` that is on an item whose `state` is `.on`.
  `MenuBarMenuTests` holds the menu as AppKit has it.
- Measured on macOS 27.0.1 with Swift 6.4 on 27 October 2026, in a program of its own that put
  a SwiftUI sheet over an offscreen window and sent it key events: a sheet draws its toolbar's
  `.cancellationAction` and `.confirmationAction` items at the trailing end of a bar of its own,
  under a separator, and `.automatic` and `.destructiveAction` items at its leading end, while
  `.primaryAction` draws nothing. Escape closes a sheet without running its Cancel button. Under
  `interactiveDismissDisabled(true)` it runs the `.cancellationAction` item instead, and nothing
  where that item's shortcut is `.keyboardShortcut(nil)`. So a sheet whose Cancel stops a
  sign-in keeps Escape for that button.
- Measured the same way: a SwiftUI `.alert` gives Return to its first button that is not
  `.cancel`, whatever `.keyboardShortcut(nil)` says of it. `.keyboardShortcut(.defaultAction)` on
  Cancel gives Cancel Return, leaves the answer with no key, and leaves Escape to no button.
- Measured on macOS 27.0.1 (26A434) on 27 October 2026, in a program of its own that put an
  `NSAlert` as a sheet over an offscreen window and read its buttons after `layout()`: the first
  button takes Return (`keyEquivalent` `\r`, the window's `defaultButtonCell`) unless it is
  called Cancel, and a button called Cancel takes Escape wherever it is. With Cancel first, no
  button takes Return and the window has no default button cell, and an Escape sent to the
  sheet answered `alertFirstButtonReturn`. A key equivalent can be one key, so no alert gives
  Cancel both. So a question whose answer is not the default, Uninstall's, is an `NSAlert` with
  Cancel first: Escape cancels and Return answers nothing.
- Measured on macOS 27.0.1 with Swift 6.4 on 27 October 2026, in a SwiftUI app of its own: a
  `Window` scene's window has the scene's id as its `identifier`, which the UI tests find it
  by, as `windows["welcome"]`.

## What the UI tests find a control by

Measured on macOS 27.0.1 with Swift 6.4 on 25 October 2026, in a program of its own that
made the views and read AppKit's objects.

- A SwiftUI menu's items carry no accessibility identifier. An `NSHostingMenu` of `Button`s,
  a `Toggle`, a `Menu` and a `Link`, each given `.accessibilityIdentifier`, made `NSMenuItem`s
  whose `identifier` was `menuAction:`, or `ID(id: 5)` for the submenu, and whose
  `accessibilityIdentifier()` was empty. So the macOS UI tests find a menu's items by their
  words, in `apps/macos/UITests/Words.swift`.
- Measured again on 27 October 2026, the same way: `.id(_:)` and `.accessibilityLabel(_:)`
  reach no item either. Each item is a `SwiftUIMenuItem` whose `accessibilityLabel()` is its
  title, whatever label it was given.
- A SwiftUI `.alert` is an `_NSAlertPanel` attached to its window as a sheet, whose buttons
  AppKit names by their order, whatever `.accessibilityIdentifier` they are given: the first
  declared, the answer, is `action-button-1`, and Cancel after it is `action-button-2`.

## How the UI tests take a picture in Dark

- `XCTestCase.runsForEachTargetApplicationUIConfiguration` is the only way XCTest offers on
  macOS: `XCUIDevice` has no `appearance` there. Read with `otool` in XCTest.framework of
  Xcode 27.1: for an app whose Info.plist names no style and does not require Aqua, the
  configurations are Light and Dark, and `XCUIDeviceSetupManager testCaseWillStart:` sets the
  device's appearance from each run's configuration and from nothing else, so nothing sets it
  back.
- Measured on macOS 27.0.1 on 27 October 2026, in a program of its own: launched with
  `-AppleInterfaceStyle Dark`, `UserDefaults` read Dark and `NSApp.effectiveAppearance` stayed
  Aqua.
- Measured in CI run 40440661624 (macos-26-arm64, Xcode 26.6): each `ScreenshotTests` test ran
  in Light, then in Dark. The app's windows, menus and status item went Dark with it, so did
  the UI test runner's own `NSApp.effectiveAppearance`, and `SiteFiguresTests`, which ran
  next, drew in Dark. So CI runs `ScreenshotTests` last, in an `xcodebuild` run of their own.
