# What the Windows app's glue rests on

Read on 15 October 2026, in the sources of the builds the app is made with and in
Microsoft's documentation.

- `Environment.GetFolderPath(SpecialFolder.LocalApplicationData)` asks
  `SHGetKnownFolderPath(FOLDERID_LocalAppData)`, and only where that fails, as on Nano
  Server, returns `LOCALAPPDATA` (dotnet/runtime v10.0.13,
  `System.Private.CoreLib/src/System/Environment.Win32.cs`). So `Shell.LocalAppData` asks
  `SHGetKnownFolderPath` itself, and no variable moves the app's data folder.
  `ShellTests` holds it on Windows.
- An unpackaged process's `AppInstance.FindOrRegisterForKey` holds a key as the named mutex
  `<module>_<key>_Mutex`, with every backslash in the key made an underscore, where
  `<module>` is `App` and a hash of the lowercased path of the process's executable (Windows
  App SDK v2.5.1, commit 13160d54, `dev/AppLifecycle/AppInstance.cpp` `TrySetKey` and
  `Association.cpp` `ComputeAppId`). So a key is shared only by copies of one executable: a
  Debug build and a Release in other folders never meet, and the fixture's folder in a Debug
  build's key keeps two fixtures of one build apart. `RedirectActivationToAsync` queues the
  launch for the running copy from a background thread and calls `AllowSetForegroundWindow`
  for it, so the running copy may bring its window forward. The running copy raises
  `Activated` for it once, on a thread pool thread, to the handlers connected then, and only
  then lets the second copy quit (`ProcessRedirectionRequests`). So `Program` connects its
  handler before it takes the key, and `Handovers` keeps a launch that arrives before the
  window can open: connected after, a launch handed over while the first copy started was lost
  without a word from either copy (run 38789385677). A second copy hands its launch over off
  the STA thread, as Microsoft's [App instancing](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/applifecycle/applifecycle-instancing)
  does it, to a copy that runs and to one that is starting, on both images (`LifecycleTests`,
  run 38795330950).
- `FindOrRegisterForKey` returns the `AppInstance` that `GetCurrent` does, so the handler
  `Program` connects before it takes the key hears each launch handed to that key. An
  unpackaged launch's arguments are its whole command line, as `GetCommandLine` has it, its
  program first, which the SDK splits with `CommandLineToArgvW` to find its own (commit
  13160d54 again, read on 16 October 2026: `AppInstance.cpp` `FindOrRegisterForKey` and
  `GetActivatedEventArgs`, and `LaunchActivatedEventArgs.h`). So `Handovers` keeps each
  launch with its link, and `Launching.Handed` splits the command line as the SDK does.
- Under `DispatcherShutdownMode.OnExplicitShutdown`, closing the main window leaves
  Pitboard.exe running ([DispatcherShutdownMode](https://learn.microsoft.com/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.application.dispatchershutdownmode)),
  on both images (`LifecycleTests.ClosingTheWindowLeavesPitboardRunning`, run 38804714545).
- Microsoft's NavigationApp template (WindowsAppSDK main,
  `dev/Templates/Source/ProjectTemplates/Desktop/CSharp/NavigationApp`, read on 16 October
  2026) puts a `TitleBar` with the app's icon and the pane toggle above a `NavigationView`
  under `MicaBackdrop`, extends the content into the title bar, asks for the tall one and calls
  `AppWindow.SetIcon`. `MainWindow` does the same but for the height, so Mica reaches the title
  bar, and its icon names the window in the title bar and Alt+Tab (`LifecycleTests.TheWindowCarriesPitboardsIcon`).
  The title bar guidance (read on 25 October 2026) gives the standard title bar 32 px and the
  tall one to a title bar with a search box or a person picture, which Pitboard's has neither
  of. WinUI 2.3.9's `Themes/generic.xaml` draws a `TitleBar` 32 epx tall in its `CompactHeight`
  state and 48 in `ExpandedHeight`, and its back button as the template part `PART_BackButton`,
  which `PanesTests` presses; its `BackRequested` is a `TypedEventHandler<TitleBar, object>`
  (`Microsoft.WinUI.dll` metadata). `MainWindowTests.TheTitleBarIsTheStandardHeight` holds the
  height.
  The PRI file indexes `Pitboard.ico` as `Files/Pitboard.ico` (run 38774669459), which
  `ms-appx:///Pitboard.ico` reads. The template's pages own their titles, in
  `TitleTextBlockStyle` at the top of a Grid padded 24,16,24,16 with 24 between its rows, and
  the NavigationView page (updated 2 October 2026) recommends 24 epx margins for the content
  outside Minimal, where a LeftCompact pane never goes, and navigates its frame only to a page
  not shown already. So
  `MainWindow` sets no `Header`: each page carries its title as a Level1 heading, its content
  24 epx under it and 24 epx from its sides, which `Styles.xaml` holds, and the lists and the
  settings' scroll viewer reach the window's edges and pad what they hold, which keeps a scroll
  bar at the edge. Its pages are kept (`NavigationCacheMode.Required`), with no back stack
  (`MainWindowTests`).
- WinUI 2.3.9's `Themes/generic.xaml` sizes a `BreadcrumbBar`'s items by the theme resources
  `BreadcrumbBarItemThemeFontSize` and `BreadcrumbBarItemFontWeight`, and its chevron by
  `BreadcrumbBarChevronFontSize` and `BreadcrumbBarChevronPadding`, and its
  `BreadcrumbBarItemAutomationPeer` implements `IInvokeProvider` (`Microsoft.WinUI.dll`
  metadata). So `SettingsBreadcrumb` sets them in its own resources to head Diagnostics and
  Activity at a page title's size, as Windows Settings heads a page under another, and
  `MainWindowTests` presses its Settings item.
- `AppWindowTitleBar.PreferredTheme`'s reference gives `Legacy` as its default, and WinUI's
  window class carries no icon. So `WindowChrome` gives every window of Pitboard's
  `UseDefaultAppMode` and the app's icon (`WindowChromeTests`,
  `LifecycleTests.TheAccountAndOpenLinkWindowsCarryPitboardsIcon`).
  The WinUI Gallery's `TitleBarHelper` (`WinUIGallery/Helpers/TitleBarHelper.cs`, read on 17
  October 2026) sets the caption buttons' glyph and hover colours from the content's
  `ActualTheme`, on `Loaded` and `ActualThemeChanged`, because `AppWindowTitleBar` does not
  change them when the mode changes while the app runs, and WinUI's own TableView sample says
  the system draws them black whatever the content's theme. So `WindowChrome` does the same
  for a window whose content extends into its title bar, the main window
  (`WindowChromeTests.TheMainWindowsCaptionButtonsFollowItsTheme`). Microsoft's title bar
  guidance (updated 14 July 2026) has a title bar's colours follow a contrast theme, and the
  Windows App SDK's `ThemeSettings` (`Microsoft.InteractiveExperiences.Projection.xml`,
  InteractiveExperiences 2.1.9 in Windows App SDK 2.5.1) tells a Win32 app whether one is on
  and when that changes. So under a contrast theme `WindowChrome` sets none of the caption's
  colours, which the system then draws, and follows `ThemeSettings.Changed`
  (`WindowChromeTests.UnderAContrastThemeTheCaptionButtonsTakeWindowsOwnColours`). How they
  look under one is measured by hand.
- Microsoft's Manage app windows page (updated 14 October 2026) gives a window that belongs
  to another an owner window, set through Win32 interop, and the WinUI Gallery's
  `ModalWindow.xaml.cs` sets it with `SetWindowLongPtr(GWLP_HWNDPARENT)`. An owned window
  stays above its owner, is hidden while its owner is minimized and is destroyed with it
  ([Window features](https://learn.microsoft.com/windows/win32/winmsg/window-features#owned-windows),
  updated 21 February 2026). So a sign-in window is owned
  by its account window, and not modal: the account window's page stays usable behind it
  (`AccountWindowTests.ASignInWindowIsOwnedByItsAccountWindow`). The account and Open link
  windows are under `MicaBackdrop`, as the main window is, with the system title bar the
  Gallery's `ModalWindow` keeps.
- Microsoft's window management guidance (updated 11 March 2025) saves a window's monitor,
  size, place and state as it closes and restores them as it opens, fits a window whose last
  place no screen holds within one, cascades a new application window from the upper left,
  and has a resizable window keep a usable minimum. `SetWindowPlacement`'s reference (updated
  1 July 2025) moves a window its bounds would put wholly off screen onto one. So
  `WindowPlacement` keeps `GetWindowPlacement`'s restored bounds and whether the window was
  maximized, a minimized one by the state it comes back to, in `placement.json` in the app's
  data folder, and `MainWindow` puts them back with `SetWindowPlacement` before its first
  activation, which shows a window already maximized maximized (microsoft-ui-xaml at
  `winui3/release/2.5.1`, `DesktopWindowImpl.cpp` `ActivateImpl`). With none saved it opens
  at Windows' own place, 1100 by 720 epx, which `WindowChrome` fits within its screen's work
  area. A saved place is put back as it was, even partly under the taskbar, as the person left
  it. `WindowChrome` keeps the window at 640 by 500 epx at least at its screen's scale, as the
  WinUI Gallery's main window does. Exit closes the window first, so it is kept then too
  (`WindowPlacementTests`, the `LifecycleTests` of placement).
- Microsoft's keyboard accelerator page names F6 and Shift+F6 for the next and previous pane
  and Ctrl+W for Close, and its keyboard accessibility page has the app implement F6 itself,
  Shift+F6 the same cycle reversed. So F6 and Shift+F6 move between the main window's two
  regions, its navigation pane and its page, and Ctrl+W closes it as its caption's close does
  (`LifecycleTests.F6MovesFocusBetweenThePaneAndThePage`, `CtrlWClosesTheWindowAndPitboardKeepsRunning`).
- WinUI's WebView2 (microsoft-ui-xaml main at 1af995b, `controls/dev/WebView2/WebView2.cpp`)
  handles no `AcceleratorKeyPressed`, so no XAML accelerator hears a key pressed in the page,
  where WebView2's own browser keys serve Back, Forward and Reload. So an account window takes
  Ctrl+W, and the browser keys for its Back, Forward and Reload, on its root, for its toolbar
  and its note (`AccountWindowTests.TheWindowTakesTheKeysOfItsCommands`,
  `TheToolbarTakesBackAndCtrlW`). An accelerator for Alt+Left on the root missed the key with
  the command bar's More focused on windows-2025 (run 39310757537), though
  `CommandBar_Partial.cpp` at `winui3/release/2.5.1` moves focus on Left or Right only with
  Alt up, so Alt+Left and Alt+Right are read in the root's `PreviewKeyDown`, Alt from the
  thread's key state.
- Microsoft's User Interface Text guidelines (updated 15 April 2025) title a top-level
  document-centric window "document name - program name", the document first. So an account
  window is titled with its page's `DocumentTitle`, a hyphen and the account's title, where
  the macOS window shows the page as its subtitle
  (`AccountWindowTests.ItsTitleIsItsPagesAndThenItsAccounts`). An account window's list of
  downloads is WebView2's own, with no Clear of Pitboard's, so `OpenWindow.clear_downloads`
  is `None` on Windows (`model::windowing`'s
  `a_window_on_windows_offers_no_clear_of_its_downloads`).
- The Windows App SDK's `AppWindow` reference (`Microsoft.UI.xml` in InteractiveExperiences
  2.1.9, Windows App SDK 2.5.1, and Microsoft Learn, updated 28 July 2026) raises `Closing` for
  a close through a system affordance, which `Cancel` stops, and not for `Destroy`. `Hide`
  keeps the window alive and `Show` brings it back, and `Window.Close` destroys the window
  (updated 31 July 2026). So an account window that keeps its page for a download hears the
  caption's close and Alt+F4 in `Closing`, puts Ctrl+W through the same check, and closes
  itself with `Close` as its last download ends. The tests close it with `WM_SYSCOMMAND`
  `SC_CLOSE`, as Alt+F4 does.
- Microsoft's ResourceDictionary page (updated 13 October 2026) has `XamlReader.Load` parse in
  a context that knows no other ResourceDictionary, not even `Application.Resources`, and take
  no ThemeResource, and has `XamlControlsResources` merged first. So the views' shared styles
  are `Styles.xaml`, which `App.xaml` and the test app merge after it
  (`StylesTests.EachSharedStyleIsFoundBeforeAnyWindowShows`).
- Rust's `std::env::temp_dir` (Rust 1.98.1, `library/std/src/sys/paths/windows.rs`) and
  .NET's `Path.GetTempPath` (dotnet/runtime v10.0.13, `Path.Windows.cs`) both ask
  `GetTempPath2W`, .NET falling back to `GetTempPathW` where it is missing. So the app finds
  a fixture's folder, `pitboard-fixture`, where the core makes it.
- One ContentDialog shows per thread at a time, and opening a second throws, even in another
  window ([ContentDialog](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.contentdialog),
  updated 28 July 2026). So `DialogHost` queues the main window's dialogs, and while a sheet
  is up `WindowAlerts` leaves the failure alert and the quit question to the sheet's
  `AlertBar`, an InfoBar, where the macOS app shows them over the sheet.
  `DialogHostTests.OnlyOneContentDialogShowsAtATime` holds it on the runners.
- The WinUI Gallery's `ContentDialogPage` gives each dialog it makes its page's
  `ActualTheme`, and an Add account sheet opened after a switch to dark while Pitboard ran
  drew light under white text, where one opened in dark was right. So `DialogHost` gives every
  dialog its window content's theme and follows `ActualThemeChanged` while it is up
  (`DialogHostTests.ADialogTakesItsWindowsThemeAndFollowsIt`).
- An exception that ends a `DispatcherQueue` callback fails fast in `CoreMessagingXP.dll` and
  reaches no handler. So `Ui.Guarded` wraps each callback the app hands `TryEnqueue`, and
  writes what ended it, with the `RestrictedDescription` C#/WinRT keeps WinUI's words in,
  before throwing it on (`UiTests`).
- A sheet is a ContentDialog, which Microsoft's guidance gives for a question that asks for
  input, with its "do it" actions as the primary and secondary buttons and a safe close
  button that Esc presses ([Dialog controls](https://learn.microsoft.com/windows/apps/design/controls/dialogs-and-flyouts/dialogs),
  updated 15 July 2026). The tool is a RadioButtons group starting on the tool the sheet is
  about, as the combo box guidance gives fewer than five items to pick one of as radio
  buttons ([Combo box and list box](https://learn.microsoft.com/windows/apps/develop/ui/controls/combo-box),
  updated 21 September 2026). The account signed in to again is selectable text under a
  label, and the code field checks no spelling and predicts no text, as the text box guidance
  gives text never edited to a TextBlock and names `IsSpellCheckEnabled` and
  `IsTextPredictionEnabled` ([Text box](https://learn.microsoft.com/windows/apps/develop/ui/controls/text-box),
  updated 17 July 2026). A sign-in waits under an indeterminate ProgressRing, the control that
  the progress guidance names for "Signing in..."
  ([Progress controls](https://learn.microsoft.com/windows/apps/design/controls/progress-controls),
  updated 8 July 2026).
- WinUI 2.3.9's `Themes/generic.xaml` sizes a ContentDialog's `BackgroundElement` between
  `ContentDialogMinWidth` (320) and `ContentDialogMaxWidth` (548), so the Add account sheet
  was as wide as its widest line and changed width with the tool picked. `AccountSheet` sets
  its own `ContentDialogMinWidth` to the maximum, the lightweight styling Microsoft's XAML
  styles page describes, and keeps one width
  (`AccountSheetTests.TheSheetKeepsOneWidthWhateverItShows`).
- The same `generic.xaml` shows a TextBox's `Description` in `DescriptionPresenter`, a
  ContentPresenter with no `TextWrapping`, so a note given as a string stays on one line: the
  code field's ran past the sheet's edge. `AccountSheet` gives each field's note as a
  TextBlock that wraps (`AccountSheetTests.ACodeNoteLongerThanItsFieldWrapsUnderIt`).
- The code WinUI 2.3.9's XAML compiler writes for a OneWay `x:Bind` function calls it again,
  as a path in its argument changes, only for a value that is not null (`AccountRow.g.cs`,
  `Update_Account_Item_Plan`). So a row the list gave an account without a plan kept the tag
  of the account before, and the functions an account's row binds take the account item, which
  is never null (`AccountsPageTests.ARowGivenAnAccountWithoutAPlanSaysNone`). The same compiler
  stops on WMC9999 where a template's `x:Bind` names both a member of its `x:DataType` and an
  element its page names: it goes to warn and finds no resources for the warning
  (`BindPathListener.ResolveNameOnRoot` in microsoft-ui-xaml's main, read on 16 October 2026).
  So no page names an element after a member its templates bind.
- A Selector's `IsSynchronizedWithCurrentItem` is null by default, which keeps its selection on
  a collection view's current item ([Selector.IsSynchronizedWithCurrentItem](https://learn.microsoft.com/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.primitives.selector.issynchronizedwithcurrentitem)).
  The Accounts page groups its rows through a CollectionViewSource, so twoTools opened with a
  row selected that nobody chose. Its list sets it false
  (`AccountsPageTests.NoRowIsSelectedUntilOneIsChosen`).
- Read in microsoft-ui-xaml at `winui3/release/2.5.1`, the WinUI of Windows App SDK 2.5.1.
  `ContentDialog_Partial.cpp`: Esc clicks the close button where it is enabled and
  otherwise hides the dialog through a `Closing` it may cancel; Enter clicks the default
  button where it is enabled; the first focusable element of the content takes focus as the
  dialog opens; a dialog made in code, outside the live tree, is put whole in its popup.
  `KeyboardInputProcessor.cpp` raises `PreviewKeyDown` and `KeyDown` with one event
  argument, so a `PreviewKeyDown` handled on the dialog keeps Esc from its close button.
  `InfoBar.cpp` announces an InfoBar as it opens, by its severity, title and message, only
  through an automation peer it has already. So `AccountSheet` cancels each button's click
  and leaves closing to the model, dims its close button through `CloseButtonStyle` while
  nothing may close it, keeps Esc from a sign-in that waits on its app, and keeps its
  failure's InfoBar in the tree while it is closed. `AccountSheetTests` holds what runs
  without a person on the runners.
- Microsoft's InfoBar page raises no notification for a bar changed while open, and has it
  closed and opened again for a screen reader to hear the new content
  ([InfoBar](https://learn.microsoft.com/windows/apps/develop/ui/controls/infobar), updated 25
  June 2026). So the sheet's failure bar closes and opens again for each new failure, an
  account window's note bar for a note in place of another, as the macOS window announces
  each (`AccountWindowTests.ANoteInPlaceOfAnotherOpensItsBarAgain`), and a
  code the tool refused is an error InfoBar under the code field, where plain text was. A new
  failure gives focus back to the field it is about, as the sheet's opening does, so the
  default button is the one Enter presses.
- WinUI 2.3.9's `Themes/generic.xaml`, in the Windows App SDK 2.5.1 package: the InfoBar's
  close button is the template part `CloseButton`, which `NoticeBarTests` presses, and a
  `ListViewItem` is padded `16,0,12,0`, which only the Activity page's column headers repeat,
  so they line up with the rows under them. A `ContentDialog`'s buttons are the template
  parts `PrimaryButton`, `SecondaryButton` and `CloseButton`, with no AutomationId of their
  own, so UI Automation finds each by its part's name, as the UI tests find the InfoBar's
  `CloseButton` and the command bar's `MoreButton`. A `TeachingTip`'s buttons are the parts
  `ActionButton` and `CloseButton` in the popup it opens, and it raises `ActionButtonClick` and
  `CloseButtonClick` (`Microsoft.WinUI.dll` metadata), so the setup tip's choice and Not now are
  those two, which `Launched.Tip` and `AccountsPageTests` press.
- `Shell_NotifyIconW` and `NOTIFYICONDATAW` (Microsoft Learn, updated 1 July 2025 and 20
  November 2024): `NIM_SETVERSION` follows every `NIM_ADD`; under `NOTIFYICON_VERSION_4` the
  event is `lParam`'s low word and the point `wParam`'s two words, the icon's upper-left
  corner when the keyboard sent it; the standard tooltip shows only with `NIF_SHOWTIP`;
  `szTip` holds 128 UTF-16 units with its null; and a GUID is tied to the path of the exe
  that added it. Microsoft's NotificationIcon sample in Windows-classic-samples reads
  `WM_CONTEXTMENU`'s point from `wParam`, makes its window the foreground window before its
  menu, and asks for Common Controls 6 to call `LoadIconMetric`. So `NotifyIcon` cuts the
  tooltip to 127 units, names its icon by a GUID in the Release build alone and falls back
  to its window when Explorer refuses the GUID, and `IconEvent` reads the point as signed.
  `NotifyIconTests` and `IconEventTests` hold them, and `HiddenWindowTests` the icon's
  return on `TaskbarCreated`.
- The notification area guidance (updated 15 April 2025) has a right click bring up a normal
  shortcut menu and a left click show what the person expects, the program window among
  them, and prefers the standard tooltip. Its Windows 7 page (updated 11 March 2025) gives
  the icon's menu no About command where the program has a window. The Windows 7 menu
  guidance (updated 26 July 2026) names Exit, puts no ellipsis on Help or Settings, shows a
  context menu's default command in bold and no shortcut keys, and writes labels in
  sentence-style capitalization, keeping title style for a legacy program that would
  otherwise mix styles. So the icon's menu says Exit, bolds Open Pitboard, shows no shortcuts
  and leaves About to the settings page, and the Windows app writes all its text in sentence
  case: its own, and the model's buttons, menu items and headings, which `words::titled`
  words for each system, title case on macOS as Apple's guidelines have it.
- WinUI 2.3.9's `Themes/generic.xaml` draws a `RadioMenuFlyoutItem`'s text in one
  TextBlock that trims, where the macOS menu puts an account's limits on a line under its
  name. So on Windows `AccountItem.menu_title` ends with the figure the menu bar would show
  of the account, `headline`'s, where that line says its limits, and the line is the item's
  tooltip and help text (`present::accounts`'
  `a_windows_menu_item_ends_with_the_figure_the_bar_would_show`).
- Read in microsoft-ui-xaml at `winui3/release/2.5.1`: a light-dismiss popup hides its
  flyout when its window is deactivated (`Popup_Partial.cpp`), so `MenuHost` is made the
  foreground window before the menu opens; a `MenuFlyoutItem` raises `Click` before it closes
  its menu (`MenuFlyoutItem_Partial.cpp`), so a choice runs once the menu has closed; a
  `RadioMenuFlyoutItem` is a `ToggleMenuFlyoutItem` inside that
  undoes an uncheck by choosing (`RadioMenuFlyoutItem.cpp`); and NavigationView names its
  settings item `SettingsItem` (`NavigationView.cpp`), which the UI tests select.
  `OverlappedPresenter.CreateForContextMenu` keeps a border (Microsoft Learn, updated 28 July
  2026), which `MenuHost` turns off.
- A `MenuFlyoutItem` goes to its `NarrowPadding` state only when a mouse, pen or keyboard
  opened its menu (`MenuFlyoutItemBase_Partial.cpp` at `winui3/release/2.5.1`), and WinUI
  2.3.9's `Themes/generic.xaml` pads it `11,8,11,9` otherwise and `11,4,11,5` then.
  `WM_CONTEXTMENU` from the notification area names no device, so the icon's rows were
  taller in one opening than in another. `IconMenuFlyout` gives each row
  `MenuFlyoutItemThemePaddingNarrow`, which the template binds the row's padding to outside
  that state, so every opening draws the rows a mouse would
  (`IconMenuFlyoutTests.EachRowIsPaddedAsAMouseOpenedMenu`).
- The .NET SDK 10.0.401 numbers an `ApplicationIcon` 32512, `IDI_APPLICATION`'s number, and
  copies it into the apphost on any system (dotnet/runtime v10.0.13,
  `src/installer/managed/Microsoft.NET.HostModel/AppHost/HostWriter.cs`): `llvm-readobj
  --coff-resources` on a win-x64 build made on macOS lists `GROUP_ICON` 32512 with eight
  `ICON` entries in the exe. So `Shell.AppIcon` loads 32512 from `Pitboard.exe`.
- Microsoft's high DPI desktop development page (updated 10 September 2026) has a
  per-monitor aware app load its images again for a new DPI, answers `GetSystemMetrics` at
  the primary display's DPI as the session started and `GetSystemMetricsForDpi` at any, and
  calls the primary display the one the system tray is on. So `IconImage` loads the icon
  with `LoadIconWithScaleDown` at `SM_CXSMICON` for the primary display's DPI, in place of
  `LoadIconMetric`, whose `LIM_SMALL` names no DPI, and loads it again when
  `HiddenWindow` hears `WM_DISPLAYCHANGE`, `WM_DPICHANGED` or `WM_SETTINGCHANGE` and that
  size has changed. `NotifyIcon` sends the new one with `NIM_MODIFY` before the one before
  is destroyed (`IconImageTests`, `NotifyIconTests.ANewImageModifiesTheIconAddedAlready`,
  `HiddenWindowTests.AChangeOfTheDisplaysReachesTheApp`). Which of the three a scale change
  sends a hidden window, and how the icon looks after one, are measured by hand.
- Microsoft's notification area guidance (`winenv-notification`) asks for icon variations to
  show changes in quantities, overlays to show a status, and no swaths of pure red, yellow or
  green in a base icon; the icon shows no title, where the macOS menu bar item shows the
  figure. So the model's `MenuBarText` carries the step of the limit the bar names and
  whether the menu opens on something to look at, and its tooltip as whether it does, which
  Narrator reads with the icon, then a line for each account in use. `Tray/Plenty.ico`,
  `Low.ico` and `Out.ico`, drawn in `apps/windows/Icon` at 16, 20, 24, 32, 40 and 48 px, the
  sizes `NotificationIconSize` gives up to 300%, light the mark's row in a neutral tone as
  long as what is left, `None.ico` lights none before a reading, and attention draws the
  shell's stock `SIID_WARNING` over the bottom right at five eighths of the icon.
  `DrawIconEx` blends into a 32-bit DIB premultiplied, and an icon's pixels are
  straight, so `Shell.NotificationIcon` straightens them before `CreateIconIndirect`
  (`IconImageTests.AttentionPutsTheStandardWarningOverTheBottomRight`, which compares the
  icon's top third with the plain one's at 20 px, where its edges are partly covered).
- Microsoft's app icon guidance (updated 14 July and 5 August 2026) asks for a transparent
  icon drawn straight on, half of it at 3:1 against light and dark backgrounds, at 16, 24,
  32, 48 and 256 px at least, and the notification area draws it at 16 px at 100% up to 64
  px at 400%. So `Pitboard.ico` holds 16, 20, 24, 32, 40, 48, 64 and 256 px of the macOS
  icon's board, untilted and unplated, whose grey rim, pole and dim row hold 3:1 against
  both #202020 and #f3f3f3: at each size 50% or more of it does against the first and 81%
  or more against the second.
- Microsoft's app settings guidance (updated 25 June 2026) puts settings behind
  NavigationView's own settings item, in one scrollable column about 1,000 px wide under
  BodyStrong headers, of cards with a header, a description and the control on the right,
  sub-options under an expander, and About at the bottom in an expander with the app's name
  and version, links and legal lines. It names the Windows Community Toolkit's SettingsCard
  and SettingsExpander, which the WinUI Gallery's settings page uses, so `SettingsPage` is
  built of them (`CommunityToolkit.WinUI.Controls.SettingsControls` 8.2.251219). A change
  takes effect at once, through a toggle switch for on and off (Toggle switches, updated 21
  September 2026), and a control that cannot change says why in its card's description.
- Read in the Community Toolkit for Windows at `rel/8.2.251219` (commit a6b4dc45),
  `components/SettingsControls` and `components/Triggers`. A SettingsCard gives a ToggleSwitch
  in it `RightAlignedCompactToggleSwitchStyle`, the switch at the card's right edge with its
  state before it, and its Disabled state dims its header and description
  (`SettingsCard.xaml`). A SettingsExpander's peer is a Group with no ExpandCollapse, and its
  template hands the Expander inside it its name, not its id (`SettingsExpander.xaml`), so the
  UI tests expand through that Expander. Its items come from `Items` or `ItemsSource`, never
  both, so the warm-up's hours sit in `ItemsHeader` and its notes in `ItemsFooter`, as cards
  drawn as the toolkit draws an item (58, 8, 44 and 8 epx of padding, 52 high, a top border).
  A clickable SettingsCard's peer returns itself for the Invoke pattern
  (`SettingsCardAutomationPeer.cs`), and WinUI 2.3.9's `ButtonBaseAutomationPeer` implements
  no `IInvokeProvider` (`Microsoft.WinUI.dll` metadata), so UI Automation could not press it:
  the page copies through a Copy button, as Settings > System > About copies its
  specifications, and links through HyperlinkButtons. Troubleshoot's cards to Diagnostics and
  Activity are clickable, as Windows Settings draws a card that opens a page of its
  own, and take Enter and Space as a button does, so the UI tests open them from the keyboard
  (`Launched.OpenFromSettings`). `ControlSizeTrigger` is active while
  `MinWidth <= ActualWidth < MaxWidth` of an element it names (`ControlSizeTrigger.cs`), the
  trigger SettingsCard wraps its own content with; AdaptiveTrigger reads the window's width
  instead, so the Activity page stacks its changes by its own width with it.
- Run and RunOnce registry keys (updated 21 February 2026): a value under the person's `Run`
  key is a command line of at most 260 characters that Windows starts at each of their
  sign-ins, with no promise how soon, and a program started from the key should not write
  to it while it runs. So `RunValue` writes `"<path>\Pitboard.exe" --sign-in`, its path
  quoted, refuses a longer one, and writes only when somebody turns the switch, never as
  Pitboard starts. What Settings > Apps > Startup keeps, `StartupApproved\Run`, has no
  documented layout: turning Pitboard off there wrote 01 as its value's first byte and kept
  the `Run` value on the session machine. So `RunValue` reads an odd first byte as off, and
  the switch then shows off and refuses to turn on, its note sending the person to Settings,
  as Microsoft's support article on startup apps does. `RunValueTests` hold it.
- Read in microsoft-ui-xaml at `winui3/release/2.5.1`: a ToggleSwitch raises `Toggled`
  whenever `IsOn` changes (`ToggleSwitch_Partial.cpp`), a NumberBox raises `ValueChanged` on
  any change of `Value`, NaN for cleared text (`NumberBox.cpp`), and a TimePicker raises
  `SelectedTimeChanged` within the setter, with its clock the person's own through
  `DateTimeFormatter` unless set (`TimePicker_Partial.cpp`). So `SettingsViewModel` sends only
  a value that differs from the model's, which `SettingsViewModelTests` and
  `SettingsPageTests.ShowingWhatTheModelSaysSendsNothing` hold, the page shows the share
  again after a cleared box, and the warm-up pickers stay on the person's 12- or 24-hour
  clock, which `LocalTime::time_of_day` gives the hours' summary too.
- Read in the Windows App SDK at v2.5.1 (commit 13160d54), `dev/AppNotifications` and
  `dev/AppLifecycle`. `IsSupported` refuses only an elevated process. For an unpackaged
  process the manager takes the explicit AppUserModelID, and where none is set its
  constructor writes a key of its own under HKCU. `Register(displayName, iconUri)` writes
  `DisplayName`, `IconUri` and `CustomActivator` under
  `HKCU\Software\Classes\AppUserModelId\<id>` and a `LocalServer32` that starts the exe with
  `----AppNotificationActivated:`, takes the icon as a local `.bmp`, `.ico`, `.jpg` or
  `.png` file, and in a self-contained app leaves out the Singleton package's long-running
  process. A click on a running copy that listened before it registered raises
  `NotificationInvoked`. In a copy COM starts for a click, `Activate` keeps the arguments
  for `GetActivatedEventArgs`, which reports an `AppNotification` activation once `Register`
  has run, and raises no `NotificationInvoked` for them. `Unregister` revokes the class
  object alone; `UnregisterAll` deletes the keys, and first makes the AppUserModelId key as
  it looks for the class. The builder escapes `%`, `;` and `=` in an argument as
  `DecodeArguments` reads them back. So a Debug build never asks for the manager,
  `Shell.Name` sets the AppUserModelID first, `WindowsNotifications` checks `IsSupported`,
  listens, registers and then reads the launch, `Launching` drops a second copy started for
  a click, which has no arguments to hand over, `Pitboard.ico` ships beside `Pitboard.exe`,
  and `Pitboard.Probe` calls `UnregisterAll` only after a `Register`.
  `WindowsNotificationsTests` and `LaunchingTests` hold them.
- `AppNotificationManager.Setting` answers `Unsupported` where `IsSupported` refuses, and
  otherwise `ToastNotifications_QuerySettings` for the AppUserModelID its constructor took,
  registered or not (`dev/AppNotifications/AppNotificationManager.cpp` at v2.5.1, commit
  13160d54). `AppNotificationSetting` holds `Enabled`, `DisabledForApplication`,
  `DisabledForUser`, `DisabledByGroupPolicy`, `DisabledByManifest` and `Unsupported`
  (Microsoft.WindowsAppSDK.Foundation 2.3.12's metadata, which WindowsAppSDK 2.5.1 pulls
  in); Microsoft's reference calls the second an app setting of the person's and the third a
  global one. "Launch the Windows Settings app" (updated 26 September 2026) lists
  `ms-settings:notifications` and `ms-settings:startupapps`. So `WindowsNotifications.Read`
  asks for `Setting` only once it has registered, says each value but `Enabled`, and the
  settings link to Settings for the person's own two. Which value Windows gives an
  unpackaged Pitboard turned off in Settings is not measured. `WindowsNotificationsTests`
  holds it.
- The app notifications quickstart (updated 10 September 2026) listens, registers and then
  calls `GetActivatedEventArgs`, and says a cold start reports `Launch` with its arguments
  in `NotificationInvoked`, where the source above reports `AppNotification`; the app takes
  a click from either. The `Register` reference (updated 28 July 2026) asks for `Unregister`
  before the app ends, so a later click starts it again, and says the manager needs the
  Singleton package; the self-contained deployment guide (updated 2 October 2026) says app
  notifications do not, and has an API that may be missing checked with `IsSupported`.
  `ToastNotification.Tag` (updated 23 August 2026) holds 64 characters, and the notice id of
  a warm-up failure of no account carries its message, so the tag is the id's SHA-256 in hex. The app notification
  content guidance (updated 17 July 2026) gives a notification three text elements at most,
  the first its title, and keeps attribution text for a source; the UX guidance (updated 21
  April 2026) has a click on a notification open its context. So a notification says its
  title, its tool and its body, as the macOS one does, with Switch as its button, and a
  click on it opens the window on Accounts.
- WebView2 SDK 1.0.4258.31's reference (Microsoft Learn, updated 12 October 2026): a page
  set as `NewWindow` must be in the opener's environment and profile and not yet navigated,
  takes its settings before it is set and `WebResourceRequested` after, and is what
  `window.open` returns to the opener; with neither `NewWindow` nor `Handled` WebView2 opens
  a window of its own. So `WebSession` handles every request, makes a sign-in window's page
  in the account's profile, prepares it, sets it and only then serves a fixture's pages to
  it. The multiple profiles guide (updated 26 November 2025) gives each profile its own
  folder of cookies, storage and caches under one user data folder, and makes a profile that
  a controller names and none has yet. The distribution guide (updated 2 October 2026) reads
  the Runtime's version from `pv` under
  `EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`, per machine and per user, and
  has an app feature-detect what a Runtime older than its SDK may lack. In run 38825074643
  `WebSessionTests.ASignInWindowKeepsItsOpenerAndClosesWhenItsPageIsDone` and
  `ProfileTests.TwoProfilesKeepTheirCookiesApart` passed on both images.
- `CoreWebView2Profile.Delete` for WebView2 1.0.4258.31, read in Microsoft's documentation on
  16 October 2026: deleting a profile marks it for deletion, closes the web views using it and
  raises its `Deleted` event, and its folder is deleted only as the browser process exits, or
  at a later start where something still held its files. Making a profile of the same name
  before then fails with `HRESULT_FROM_WIN32(ERROR_DELETE_PENDING)`. A profile's folder is
  `EBWebView\WV2Profile_<name>` under the user data folder, as
  `CoreWebView2Profile.ProfilePath` read on both runners in run 38819589926. On the runners
  the folder went as its browser process exited, or outlived it and went as the next one
  started, and that next one refused the name for its first seconds either way (CF7 in
  [Windows runners](windows-runners.md)).
- The WebView2 SDK the Windows App SDK 2.5.1 brings is 1.0.3719.77
  (`CORE_WEBVIEW_TARGET_PRODUCT_VERSION` in its `WebView2EnvironmentOptions.h`), whose Runtime
  is 144.0.3719.77.
- Chromium 155.0.8059.87, read on 16 October 2026: a download whose name is taken is
  numbered by `CreateUniqueFilename` (`download_path_reservation_tracker.cc`), which puts
  ` (1)` before `FilePath::Extension`, and that keeps a second extension of one to four
  characters before `bz`, `bz2`, `gz`, `lz`, `lzma`, `lzo`, `xz`, `z` or `zst`, takes
  `user.js` whole, and lets an extension hold a space (`base/files/file_path.cc`). A
  download not yet named is written into a temporary file in the download manager's default
  folder (`base_file.cc`, `in_progress_download_manager.cc`). So `download_destination`
  splits a name that way on Windows, and `WebEnvironment` sets each profile's download
  folder. `CoreWebView2DownloadOperation`'s reference (updated 28 September 2026) has
  `Pause` leave a download interrupted with `CanResume` true, and `Downloads` ends one in
  the model only once it cannot resume. The reference gives the reason as `UserCanceled`;
  the runners give `UserPaused` (CF7 under [Windows](windows-runners.md)).
- Restart Manager's guidelines for applications (updated 11 March 2025) and the references
  of `RmStartSession` and `RmShutdown` (updated 1 July 2025), read on 16 October 2026: it
  asks a GUI app with `WM_QUERYENDSESSION`, then `WM_ENDSESSION`, both with
  `ENDSESSION_CLOSEAPP`, then `WM_CLOSE` where it does not end. One `FALSE` cancels, and
  only `RmForceShutdown` ends an app regardless. A console app gets `CTRL_C_EVENT`. Only the
  process that started a session may shut down through it, a user session holds at most 64,
  and the session lives in the registry, under the user's HKCU. So `WindowsApps` registers a
  Store app's windowed process alone, never its console `codex` children, ends the session
  once `RmShutdown` returns, and calls it without flags on a thread of its own, since it
  returns only once the app has ended or declined. Tests stand in for Restart Manager: on a
  PC only the app and `Pitboard.Probe` start a session, in the session's steps A4 and A7.
  `IApplicationActivationManager::ActivateApplication`'s reference (updated 22 February
  2024) offers no option that opens an app behind another window, so `WindowsApps` brings
  Pitboard's window forward after it.
- WinSparkle 0.9.4, read on 16 October 2026 in its source at tag `v0.9.4` and in its NuGet
  package. `win_sparkle_set_eddsa_public_key` returns 0 and keeps no key for anything but the
  base64 of 32 bytes, and with no key the downloader logs "Using unsigned updates!" and
  installs anyway (`dll_api.cpp`, `signatureverifier.cpp`, `updatedownloader.cpp`).
  Versions compare by Sparkle's comparator, which sorts `1.0.0-beta.1` and `1.0.0+<commit>`
  below `1.0.0`, against the build version where one is set (`updatechecker.cpp`,
  `settings.h`). The feed's parser takes an enclosure whose `sparkle:os` is empty,
  `windows` or the DLL's own `windows-x64` or `windows-arm64`, and `minimumSystemVersion`
  only as an element of the item, checked with `VerifyVersionInfoW` (`appcast.cpp`). The
  did-find-update callback names no version (`appcontroller.cpp`). A check that fails shows
  nothing unless WinSparkle's window is already open (`ui.cpp`). `win_sparkle_init` asks
  whether to check at the second launch unless `CheckForUpdates` is set, and starts the daily
  checks only where it is on (`dll_api.cpp`, `updatechecker.cpp`). The can-shutdown callback
  runs on WinSparkle's thread before the installer starts, and the shutdown request once it
  has (`ui.cpp`). Settings go to `HKCU\Software\<company>\<app>\WinSparkle` unless the
  app gives its own config methods (`settings.cpp`). The package's `runtimes/win-x64` and
  `win-arm64` DLLs are AMD64 and ARM64, import only system DLLs and export the API by its C
  names (llvm-readobj 22). So `SparkleUpdates` starts WinSparkle only with a key it takes and
  the version without a commit, turns automatic checks on before the first start, reads the
  version of an update found from the feed, asks the downloads question in the can-shutdown
  callback, and keeps WinSparkle's settings in `updates.json` in the app's data folder, which
  an uninstall removes with the folder. Turned on after a start with them off, automatic
  checks begin at Pitboard's next start. GitHub answered
  `releases/latest/download/appcast-windows.xml` with a redirect to v0.15.0's asset, which
  is missing, and then 404 (19 October 2026): no full release carries a Windows feed before
  1.0.0, and a beta's check ended in WinSparkle's error. So a beta reads the feed first and
  starts WinSparkle only on an answer other than 404, reading it again at each settings
  visit. `SparkleUpdatesTests` and `UpdateFeedTests` hold it.
  An update's MSI is checked whole with orlp's `ed25519_verify`, its signature the base64 of
  64 bytes (`signatureverifier.cpp`). Sparkle 2.10.0's `sign_update` signs a file that is no
  feed or release notes whole with the same library, reads the key's base64 seed from
  standard input with `--ed-key-file -` and prints the signature alone with `-p`
  (`sign_update/main.swift`, `common_cli/Secret.swift`, read the same day). A rehearsal on
  16 October 2026 signed two files with a throwaway seed, and `eddsa-verify.swift` took each
  signature under its public key and refused one another seed made. So `windows-update-sign`
  signs each MSI of every Windows release with `sign_update` and checks it with
  `eddsa-verify.swift`, and for the latest full release `windows-appcast.py` then writes the
  feed.
- The .NET 8 SDK adds the commit to `InformationalVersion`, which
  `IncludeSourceRevisionInInformationalVersion` turns off
  ([Source Link included in the .NET SDK](https://learn.microsoft.com/dotnet/core/compatibility/sdk/8.0/source-link),
  updated 26 July 2025), and a `Win32Resource` takes the place of the version information
  the compiler writes and cannot be given with a `Win32Manifest`
  ([Resource options](https://learn.microsoft.com/dotnet/csharp/language-reference/compiler-options/resources),
  updated 28 May 2026). So `Pitboard.App` turns the commit off, and carries the update key as
  `AssemblyMetadata("WindowsUpdatePublicKey")` from `WINDOWS_UPDATE_PUBLIC_KEY` rather than
  WinSparkle's `EdDSAPub` resource. `check-windows-app.ps1` refuses a Release build whose
  `Pitboard.exe` version carries a commit, whose `Pitboard.dll` carries another key than the
  one it was given or a key WinSparkle keeps none of, which reads another feed than the latest
  full release's, and a `WinSparkle.dll` for another machine. `read-assembly.ps1` reads the
  key and the feed from the assemblies' metadata without loading them.
