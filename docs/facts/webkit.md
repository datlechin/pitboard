# WebKit and SwiftUI

Measured on macOS 27.0 on 4 October 2026, with probe apps built by Xcode 27.1 against the
macOS 27 SDK, unless a fact gives its own date. Several were checked again with the app's
own code driving WebKit on a fixture's stand-in pages. Before changing code that depends on
one, measure it again.

- SwiftUI's `WebPage` cannot host an account window. `window.open` returns null for every
  trigger and no app callback fires, so a sign-in that opens a window never starts. A
  `target=_blank` link loads nothing unless the app loads it itself, which loses the
  opener. A download never reaches the app or the disk, and `WebPage` has no page zoom or
  print. So `Page` keeps a `WKWebView`, in `WebPage`'s shape.
- On a `WKWebView`, a `target=_blank` link reaches `decidePolicyFor` first, with no target
  frame. Cancelled there, it never reaches `createWebViewWith`; allowed, it does, with the
  same action. `window.open` goes straight to `createWebViewWith`.
- `window.open('')` reaches `createWebViewWith` with an empty address. The page in the new
  window then navigates, through its own `decidePolicyFor`. `webViewDidClose` fires on
  `window.close()`. A `target=_blank` link without `rel=opener` gives the new page no
  opener.
- WebKit quarantines a downloaded file itself, and makes the suggested name safe:
  `../../evil:name.txt` becomes `_.._evil_name.txt`. A response turned into a download also
  fails its navigation with code 102 in `WebKitErrorDomain`, which the window does not show.
  `WKDownload.originatingFrame` is macOS 15.2 and later.
- A response from an app's own scheme handler loses its headers and cannot become a
  download, so a fixture's downloads are `data:` and `blob:` links.
- `allDataStoreIdentifiers` and `remove(forIdentifier:)` each end in a segmentation fault in
  `WTF::RunLoop::dispatch` when they are a process's first WebKit call. Making any store
  first prevents it. So `WebKitDataStores` makes a non-persistent store before its first
  deletion, not at launch, and a person who never opens a window never starts WebKit.
- `remove(forIdentifier:)` reports a store in use for 20 to 60 ms after its last web view
  and store object are released, then succeeds. It reports it in use for as long as a
  `WKWebsiteDataStore` object for it is held. The janitor's retries, from 50 ms doubling to
  1600 ms, cover the first case, and the next read covers the second.
- `removeData(ofTypes:modifiedSince:)` with every type since `.distantPast`, on a store a
  page is using, clears every cookie, `HttpOnly` ones included, local and session storage,
  and IndexedDB. **Remove Website Data** rests on it to sign a window out while it stays
  open.
- A `WKWebView` answers neither `performFindPanelAction:` nor `performTextFinderAction:`.
  An `NSTextFinder` with the web view as its client and `WebContainer` as its bar's
  container shows the system find bar and steps through the matches.
- With incremental searching on, **Find Next** left the page's selection where it was, so
  it is off and a search runs on Return. A finder whose bar is already in the view, hidden,
  never shows it, so `WebContainer` adds the bar only while it shows.
  `TextEditingCommands()` adds the **Edit** > **Find** items, which send
  `performFindPanelAction:` with `NSTextFinder.Action` tags.
- Setting `pageZoom` leaves a pinch's `magnification` as it was, so **Actual Size** resets
  both.
- SwiftUI hands a URL to the `Window` scene that has `.handlesExternalEvents(matching:)`,
  through its `onOpenURL`, whether the app was running or not. It opens that window when
  none is open, reuses one that is, and opens no other window. With no scene claiming the
  URL, it went to the first window scene, the Pitboard window's, and opened that window.
- With an app delegate's `application(_:open:)` as well, the claiming scene's `onOpenURL`
  gets the URL and the delegate is then called with an empty list. With no scene claiming
  it, only the delegate gets it, and no window opens. So the **Open Link** window alone
  takes Pitboard links, and `AppDelegate` has no `application(_:open:)`.
- Those URLs were sent with `open -a` from a terminal. A cold launch that way left the app
  inactive, behind the terminal, so the **Open Link** window brings Pitboard forward when a
  link arrives. `AppPresence.comeForward` records that a link from the Share extension at a
  launch left Pitboard in the background too. A link opened from a browser was not measured.
- In CI, on macOS 26.6.2, the running app's **Open Link** window took each link too.
  `XCUIApplication.open(_:)` sent them, and it launched a second copy of the app rather
  than handing the link to the one running: the picker opened in one copy and the Pitboard
  window in the one the test watched. The copy left running kept its menu bar item, and
  with a few of those a later test's own item sat under the menus, out of reach. So the
  UI tests share a link through `XCUIDevice.shared.system.open(_:)`, which opens it with
  Launch Services, as the Share extension does, and so reaches the copy running.
- A window's root view gets `onAppear` when its window opens and `onDisappear` when it
  closes, not when it is minimised or becomes a background tab. `AppPresence` counts open
  windows by them. `openWindow(id:value:)` with the value of an open window brings that
  window back and opens no other.
- `SceneStorage`'s documentation in the macOS 27 SDK says its data is destroyed when a
  window is closed on macOS. So an account window's last page is kept in the app's
  preferences, by the window's store, and comes back in a window opened from a menu as well
  as in one macOS restores. This was read, not measured.
- The account picker's root view gets `onDisappear` before the account window it opened
  gets `onAppear`. Going back to `.accessory` in between gives the app's activation away, so
  the account window opens behind the browser; `AppPresence` waits a moment before going
  back to the menu bar.
- SwiftUI puts an item that opens each `Window` scene in the **Window** menu, unless
  `.commandsRemoved()` is applied, and does not list that scene's window there. A
  `WindowGroup`'s windows are listed while they are open.
- After `setActivationPolicy(.regular)` on an app that is already active,
  `NSWorkspace.menuBarOwningApplication` goes on naming the app before it, but the menu bar
  shows the app's own menus, seen in a screen capture on 4 October 2026 with a probe that
  has a main menu of its own. The reported owner is not what the person sees. An app made
  regular while inactive and then opened by Launch Services is reported as the owner as
  well. `NSApp.deactivate()` from an active app left it active.
- Measured on 4 October 2026 with a probe app driven by `open -g`: `NSApp.activate()` from
  an app in the background, with no click in it, is refused. Launch Services opening the
  app, `NSWorkspace.openApplication` with `activates` set, brings it forward. So
  `AppPresence.comeForward` asks Launch Services to open Pitboard when a shared link arrives
  while Pitboard is in the background.
- The macOS 27 SDK has no SwiftUI API for a Dock menu. `applicationDockMenu(_:)` on the app
  delegate is the API.
- Measured on 29 September 2026: WebKit keeps the persistent stores of an app that is not
  sandboxed under `~/Library/WebKit/<bundle id>/WebsiteDataStore/<identifier>`, cookies
  included, as ordinary files. It roots them at the Library folder Foundation gives the
  app, and Foundation does not read `HOME`. With `HOME` pointed at a scratch directory,
  `NSHomeDirectory()`, `.libraryDirectory` and `homeDirectoryForCurrentUser` all still gave
  the real home. The core reads `HOME` and `PITBOARD_HOME`, so the record of stores is kept
  per Pitboard directory.
