using System.Diagnostics;
using System.Drawing;
using System.Runtime.InteropServices;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Capturing;
using FlaUI.Core.Definitions;
using FlaUI.Core.Input;
using FlaUI.Core.Tools;
using FlaUI.Core.WindowsAPI;

namespace Pitboard.UITests;

// How WinUI's controls behave in the app, for usepitboard.com's figures. Each test keeps what it reads and a picture
// under a name starting "site-", which docs/facts/site.md records, before it asserts what the site rests on.
[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed partial class SiteFiguresTests
{
    // SPI_GETWORKAREA.
    private const uint GetWorkArea = 0x0030;

    private static readonly TimeSpan Shortly = TimeSpan.FromSeconds(10);

    private static readonly string[] IconMenu =
    [
        "Claude Code", "work · 42%", "old", "personal · 100%", "Codex", "main · Pro · 7%", "spare · Pro · 0%", "Add account…",
        "Refresh", "Open claude.ai", "Open chatgpt.com", "Open Pitboard", "Settings", "Pitboard help", "Exit",
    ];

    private static readonly string[] Headings = ["Claude Code", "Codex"];

    public TestContext TestContext { get; set; } = null!;

    // Explorer tells Pitboard of a right click and of Shift+F10 on its icon with the same message, at a point, so the
    // menu opens the same way for both. CF3 keeps the icon itself out of reach.
    [TestMethod]
    public void WhereTheIconMenuOpens()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        launched.Running();
        var screen = launched.Automation.GetDesktop().BoundingRectangle;
        var work = WorkArea();
        var seen = new List<string> { $"Windows {Environment.OSVersion.Version}", $"screen {screen}", $"work area {work}" };

        var nearClock = new Point(work.Right - 160, work.Bottom + ((screen.Bottom - work.Bottom) / 2));
        var middle = new Point(screen.Width / 2, screen.Height / 2);
        var placed = new Dictionary<string, Rectangle>();
        var focused = new Dictionary<string, string>();
        foreach (var (at, name) in new[] { (nearClock, "by-the-clock"), (middle, "mid-screen") })
        {
            HiddenWindows.OpenMenu(launched.Process, at.X, at.Y);
            var menu = Menu(launched, at);
            placed[name] = menu.BoundingRectangle;
            focused[name] = OwnsForeground(launched) ? FocusedName(launched) : "foreground held";
            seen.Add($"{name}: opened at {at.X},{at.Y}, menu {menu.BoundingRectangle}, focused {Focused(launched)}");
            Keep(Capture.Rectangle(Rectangle.Union(menu.BoundingRectangle, new Rectangle(at.X - 40, at.Y - 40, 80, 80))), $"site-tray-menu-{name}");
        }

        var entries = Menu(launched, middle).FindAllChildren();
        seen.AddRange(entries.Select(entry => "  " + Describe(entry)));
        Record(seen, "site-tray-menu-place");

        Assert.AreEqual(nearClock.X, placed["by-the-clock"].Right, "by the clock, the menu's right edge");
        Assert.AreEqual(work.Bottom, placed["by-the-clock"].Bottom, "by the clock, the menu's bottom edge");
        Assert.AreEqual(middle.X, placed["mid-screen"].Left, "mid-screen, the menu's left edge");
        Assert.AreEqual(work.Top, placed["mid-screen"].Top, "mid-screen, the menu's top edge");
        Assert.AreEqual(Joined(IconMenu), Joined(entries.Select(entry => entry.Name)));
        Assert.AreEqual(Joined(Headings), Joined(entries.Where(entry => !entry.IsEnabled).Select(entry => entry.Name)));
        Assert.AreEqual(
            "work · 42% | main · Pro · 7%",
            Joined(entries.Where(entry => entry.Patterns.Toggle.PatternOrDefault?.ToggleState.ValueOrDefault == ToggleState.On).Select(entry => entry.Name)));
        Assert.AreEqual(
            "Open claude.ai | Open chatgpt.com",
            Joined(entries.Where(entry => entry.ClassName == "MenuFlyoutSubItem").Select(entry => entry.Name)));
        foreach (var name in focused.Values.Where(name => name != "foreground held"))
        {
            Assert.AreEqual("work · 42%", name, "focused as the menu opened");
        }
    }

    [TestMethod]
    public void TheArrowsInTheIconMenu()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        launched.Running();
        var screen = launched.Automation.GetDesktop().BoundingRectangle;
        var at = new Point(screen.Width / 2, screen.Height / 2);
        HiddenWindows.OpenMenu(launched.Process, at.X, at.Y);
        var menu = Menu(launched, at);
        RequireForeground(launched);

        var opened = FocusedName(launched);
        Keep(Capture.Element(menu), "site-tray-menu-opened");
        var down = new List<string>();
        for (var press = 1; press <= 20; press++)
        {
            Press(VirtualKeyShort.DOWN);
            down.Add(FocusedName(launched));
            if (press == 1)
            {
                Keep(Capture.Element(menu), "site-tray-menu-first-down");
            }
        }

        var others = new List<string>();
        foreach (var key in new[] { VirtualKeyShort.HOME, VirtualKeyShort.END, VirtualKeyShort.KEY_O, VirtualKeyShort.TAB })
        {
            Press(key);
            others.Add(FocusedName(launched));
        }

        HiddenWindows.OpenMenu(launched.Process, at.X, at.Y);
        Menu(launched, at);
        var reopened = FocusedName(launched);
        Press(VirtualKeyShort.UP);
        var up = FocusedName(launched);
        Press(VirtualKeyShort.ESCAPE);
        Record(
            [$"opened: {opened}", $"Down: {string.Join(" | ", down)}", $"Home, End, O, Tab: {string.Join(" | ", others)}", $"opened again: {reopened}", $"first Up: {up}"],
            "site-tray-menu-arrows");

        var enabled = IconMenu.Except(Headings).ToArray();
        Assert.AreEqual("work · 42%", opened);
        Assert.AreEqual(Joined(enabled.Skip(1).Concat(enabled).Concat(enabled).Take(20)), Joined(down));
        Assert.AreEqual(Joined(Enumerable.Repeat(down[^1], 4)), Joined(others));
        Assert.AreEqual("work · 42%", reopened);
        Assert.AreEqual("Exit", up);
    }

    [TestMethod]
    public void WhereFocusGoesAfterForgettingFromTheKeyboard()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        var row = launched.Row("claude/personal");
        RequireForeground(window);

        var place = row.BoundingRectangle;
        Mouse.Click(new Point(place.Left + 24, place.Top + (place.Height / 2)));
        Until(() => SelectedAccounts(window).Length > 0);
        var clicked = (Focused: FocusedId(launched), Selected: SelectedAccounts(window));
        Press(VirtualKeyShort.DELETE);
        launched.Find(found => found.ByName("Forget").And(found.ByControlType(ControlType.Button)), "the dialog's Forget");
        Until(() => InDialog(FocusedId(launched)));
        var asked = Focused(launched);
        var askedId = FocusedId(launched);
        Keep(Capture.Element(window), "site-forget-dialog");
        Press(VirtualKeyShort.ENTER);
        var forgotten = launched.Gone(found => found.ByAutomationId("account.claude/personal"));
        Until(() => !InDialog(FocusedId(launched)));
        var after = (Focused: FocusedId(launched), Selected: SelectedAccounts(window));
        var afterSaid = Focused(launched);
        Keep(Capture.Element(window), "site-focus-after-forget");
        Press(VirtualKeyShort.DOWN);
        var down = (Focused: FocusedId(launched), Selected: SelectedAccounts(window));
        Record(
            [
                $"clicked: focused {clicked.Focused}, selected {string.Join(", ", clicked.Selected)}",
                $"Delete asked, focused {asked}",
                $"Enter forgot personal: {forgotten}, focused {afterSaid}, selected {string.Join(", ", after.Selected)}",
                $"Down: focused {down.Focused}, selected {string.Join(", ", down.Selected)}",
            ],
            "site-focus-after-forget");

        Assert.AreEqual("account.claude/personal", clicked.Focused);
        Assert.AreEqual("claude/personal", Joined(clicked.Selected));
        Assert.AreEqual("PrimaryButton", askedId, "Forget is not focused as the dialog opens");
        Assert.IsTrue(forgotten);
        Assert.AreEqual("AppTitleBar", after.Focused);
        Assert.AreEqual("", Joined(after.Selected));
        Assert.AreEqual("AppTitleBar", down.Focused);
        Assert.AreEqual("", Joined(down.Selected));
    }

    [TestMethod]
    public void TheContextMenuOfARowNotSelected()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        var work = launched.Row("claude/work");
        var personal = launched.Row("claude/personal");
        RequireForeground(window);

        var place = work.BoundingRectangle;
        Mouse.Click(new Point(place.Left + 24, place.Top + (place.Height / 2)));
        Until(() => SelectedAccounts(window).Length > 0);
        var clicked = SelectedAccounts(window);
        launched.RightClick(personal);
        var copy = launched.Find(found => found.ByName("Copy email address").And(found.ByControlType(ControlType.MenuItem)), "Copy email address");
        var rightClicked = SelectedAccounts(window);
        var focused = FocusedName(launched);
        var menu = Around(copy, ControlType.Menu);
        var entries = menu.FindAllChildren();
        var lines = entries.Select(entry => "  " + Describe(entry)).ToList();
        Keep(Capture.Rectangle(Rectangle.Union(menu.BoundingRectangle, window.BoundingRectangle)), "site-context-menu-row-not-selected");
        var keys = entries.Select(entry => $"{entry.Name} {entry.Properties.AcceleratorKey.ValueOrDefault}".Trim()).ToArray();
        copy.Patterns.Invoke.Pattern.Invoke();
        Until(() => ClipboardText.Read() == "dana@home.example");
        var copied = ClipboardText.Read();
        var after = SelectedAccounts(window);
        Record(
            [
                $"work clicked: selected {string.Join(", ", clicked)}",
                $"personal right-clicked: selected {string.Join(", ", rightClicked)}, focused {focused}",
                .. lines,
                $"copied {copied}, selected {string.Join(", ", after)}",
            ],
            "site-context-menu-row-not-selected");

        Assert.AreEqual("claude/work", Joined(clicked));
        Assert.AreEqual("claude/work", Joined(rightClicked));
        Assert.AreEqual("Use personal", focused);
        Assert.AreEqual("Use personal | Sign in again… | Rename… F2 | Open claude.ai | Copy email address Ctrl+C | Forget… Del", Joined(keys));
        Assert.AreEqual("dana@home.example", copied);
        Assert.AreEqual("claude/work", Joined(after));
    }

    [TestMethod]
    public void HowLongASwitchAndAReadTake()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        var use = Launched.Within(launched.Row("claude/personal"), found => found.ByName("Use personal"), "Use personal").AsButton();

        var switching = false;
        var watch = Stopwatch.StartNew();
        use.Invoke();
        var invoked = watch.ElapsedMilliseconds;
        while (watch.Elapsed < Shortly && !InUse(launched))
        {
            switching |= launched.Row("claude/personal").FindFirstDescendant(found => found.ByName("Switching")) is not null;
        }

        var switched = watch.ElapsedMilliseconds;
        var inUse = InUse(launched);
        var refresh = launched.Find(found => found.ByName("Refresh").And(found.ByControlType(ControlType.Button)), "Refresh");
        var dimmed = false;
        watch.Restart();
        refresh.Patterns.Invoke.Pattern.Invoke();
        var pressed = watch.ElapsedMilliseconds;
        var undimmed = 0L;
        while (watch.Elapsed < Shortly)
        {
            if (!refresh.IsEnabled)
            {
                dimmed = true;
            }
            else if (dimmed || watch.Elapsed > TimeSpan.FromSeconds(2))
            {
                undimmed = watch.ElapsedMilliseconds;
                break;
            }
        }

        Keep(Capture.Element(window), "site-after-use");
        Record(
            [
                $"Use: Invoke took {invoked} ms, In use {switched - invoked} ms after it, Switching seen {switching}",
                $"Refresh: Invoke took {pressed} ms, dimmed seen {dimmed}, enabled again at {undimmed} ms",
            ],
            "site-switch-and-read");
        Assert.IsTrue(inUse, "personal is not in use");
    }

    [TestMethod]
    public void TheLimitControls()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        launched.Select("SettingsItem");
        var choice = launched.Find(found => found.ByAutomationId("settings.atLimit"), "settings.atLimit").AsComboBox();
        var group = launched.Find(found => found.ByAutomationId("settings.atLimitGroup"), "settings.atLimitGroup");
        group.FindAllDescendants().First(found => found.Patterns.ExpandCollapse.IsSupported).Patterns.ExpandCollapse.Pattern.Expand();
        var share = launched.Find(found => found.ByAutomationId("settings.atLimitShare"), "settings.atLimitShare");
        Assert.IsTrue(Retry.WhileFalse(() => share.IsEnabled, Shortly, TimeSpan.FromMilliseconds(250)).Result);

        choice.Expand();
        Until(() => choice.Items.Length > 0);
        var choices = choice.Items.Select(item => item.Name).ToArray();
        var chosen = choice.SelectedItem?.Name;
        Keep(Capture.Element(window), "site-limits-combo-box-open");
        choice.Collapse();

        var range = share.Patterns.RangeValue.Pattern;
        var bounds = (Value: range.Value.Value, Lowest: range.Minimum.Value, Highest: range.Maximum.Value, Small: range.SmallChange.Value, Large: range.LargeChange.Value);
        var seen = new List<string>
        {
            $"choices: {string.Join(", ", choices)}, chosen {chosen}",
            $"share {bounds.Value} from {bounds.Lowest} to {bounds.Highest}, steps {bounds.Small} and {bounds.Large}",
        };
        var set = new Dictionary<double, (double Value, string Buttons)>();
        foreach (var value in new[] { 99.0, 50.0, 120.0, 10.0 })
        {
            try
            {
                var before = range.Value.Value;
                range.SetValue(value);
                Until(() => range.Value.Value != before);
                set[value] = (range.Value.Value, Buttons(share));
                seen.Add($"set {value}: {set[value].Value}; {set[value].Buttons}");
            }
            catch (Exception refused) when (refused is ArgumentException or InvalidOperationException or COMException)
            {
                seen.Add($"set {value}: refused, {refused.GetType().Name}, {range.Value.Value}");
            }
        }

        Keep(Capture.Element(window), "site-limits-number-box");
        Record(seen, "site-limits-controls");

        Assert.AreEqual("Do nothing | Notify me | Switch automatically", Joined(choices));
        Assert.AreEqual("Notify me", chosen);
        Assert.AreEqual((95.0, 50.0, 99.0, 1.0, 5.0), bounds);
        Assert.AreEqual((99.0, "Increase disabled, Decrease enabled"), set[99.0]);
        Assert.AreEqual((50.0, "Increase enabled, Decrease disabled"), set[50.0]);
        Assert.AreEqual(99.0, set[120.0].Value);
        Assert.AreEqual(50.0, set[10.0].Value);
    }

    [TestMethod]
    public void AnAccountWindowsTitleAsItsPageLoads()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "onlyOne");
        launched.OpenWindow();

        Launched.OpenMenu(launched.Row("claude/work"));
        var item = launched.Find(found => found.ByAutomationId("window.claude.ai/work"), "work's window item");
        var titles = new List<string>();
        var lines = new List<string>();
        var known = new HashSet<string> { "Pitboard" };
        var watch = Stopwatch.StartNew();
        item.Patterns.Invoke.Pattern.Invoke();
        while (watch.Elapsed < Shortly)
        {
            try
            {
                foreach (var title in launched.Titles().Where(known.Add))
                {
                    titles.Add(title);
                    lines.Add($"'{title}' at {watch.ElapsedMilliseconds} ms");
                }
            }
            catch (Exception unread) when (unread is InvalidOperationException or COMException)
            {
                lines.Add($"unread: {unread.Message}");
            }
        }

        Record(lines, "site-account-window-titles");
        Launched.Loads(launched.AccountWindow("work"), "claude.ai stand-in");

        // The XAML's own title, until the window reads its account, which the test sees at times.
        Assert.AreEqual("work | claude.ai stand-in - work", Joined(titles.SkipWhile(title => title == "Account")));
    }

    [TestMethod]
    public void NotificationCenterFromTheKeyboard()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        var window = launched.OpenWindow();
        Foreground.Require(window);
        var desktop = launched.Automation.GetDesktop();
        var work = WorkArea();

        Keyboard.TypeSimultaneously(VirtualKeyShort.LWIN, VirtualKeyShort.KEY_N);
        var center = Retry.WhileNull(
            () => desktop.FindFirstChild(found => found.ByName("Windows Shell Experience Host").And(found.ByClassName("Windows.UI.Core.CoreWindow"))),
            Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;
        var calendar = center is null ? null : Retry.WhileNull(
            () => center.FindFirstDescendant(found => found.ByAutomationId("CalendarCenterGrid")),
            Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;
        var seen = new List<string> { $"work area {work}", "after Windows+N:" };
        seen.AddRange(desktop.FindAllChildren().Select(Describe));
        if (center is not null)
        {
            seen.Add("in it:");
            seen.AddRange(center.FindAllDescendants().Take(80).Select(found => "  " + Describe(found)));
        }

        Keep(Capture.Screen(), "site-notification-center");
        var place = center?.BoundingRectangle ?? Rectangle.Empty;
        var focused = center?.Properties.HasKeyboardFocus.ValueOrDefault ?? false;
        var notifications = center?.FindFirstDescendant(found => found.ByAutomationId("NotificationCenterGrid"))?.BoundingRectangle ?? Rectangle.Empty;
        var dated = calendar?.BoundingRectangle ?? Rectangle.Empty;
        Keyboard.Type(VirtualKeyShort.ESCAPE);
        Record(seen, "site-notification-center");

        Assert.IsNotNull(center, "Windows+N opened no Notification Center");
        Assert.IsTrue(focused, "Notification Center has no keyboard focus");
        Assert.AreEqual(Rectangle.FromLTRB(work.Right - 360, work.Top, work.Right, work.Bottom), place);
        Assert.AreEqual(work.Right - 12, notifications.Right, "the notifications' card from the right");
        Assert.AreEqual(336, notifications.Width);
        Assert.AreEqual((work.Right - 12, work.Bottom - 12, 336, 100), (dated.Right, dated.Bottom, dated.Width, dated.Height), "the date's card");
    }

    private static bool InUse(Launched launched)
    {
        try
        {
            return Launched.Lines(launched.Row("claude/personal")).Contains("In use");
        }
        catch (Exception gone) when (gone is InvalidOperationException or COMException)
        {
            return false;
        }
    }

    // The icon's menu, once it has opened at the point given and any it replaced has gone. WinUI places the menu
    // and moves focus in it for a while after it opens, with nothing to wait on: the reading is where they settle.
    private static AutomationElement Menu(Launched launched, Point at)
    {
        Thread.Sleep(1000);
        var item = launched.Find(found => found.ByName("Open Pitboard").And(found.ByControlType(ControlType.MenuItem)), $"the menu opened at {at}");
        var menu = Around(item, ControlType.Menu);
        Assert.IsTrue(Until(() => menu.BoundingRectangle.Contains(item.BoundingRectangle) && !item.IsOffscreen), $"no menu opened at {at}");
        Thread.Sleep(500);
        return menu;
    }

    // Waits for what a step changes, and says whether it came.
    private static bool Until(Func<bool> condition) =>
        Retry.WhileFalse(condition, Shortly, TimeSpan.FromMilliseconds(100), ignoreException: true).Result;

    // The Forget dialog's buttons.
    private static bool InDialog(string focused) => focused is "PrimaryButton" or "SecondaryButton" or "CloseButton";

    private static string Describe(AutomationElement element)
    {
        try
        {
            var properties = element.Properties;
            var toggled = element.Patterns.Toggle.PatternOrDefault?.ToggleState.ValueOrDefault;
            var selected = element.Patterns.SelectionItem.PatternOrDefault?.IsSelected.ValueOrDefault;
            return string.Join(
                " ",
                new[]
                {
                    properties.ControlType.ValueOrDefault.ToString(),
                    $"'{properties.Name.ValueOrDefault}'",
                    properties.IsEnabled.ValueOrDefault ? "" : "disabled",
                    toggled is null ? "" : $"toggle {toggled}",
                    selected is true ? "selected" : "",
                    properties.HasKeyboardFocus.ValueOrDefault ? "focused" : "",
                    $"#{properties.AutomationId.ValueOrDefault}",
                    $"class {properties.ClassName.ValueOrDefault}",
                    string.IsNullOrEmpty(properties.AcceleratorKey.ValueOrDefault) ? "" : $"key {properties.AcceleratorKey.ValueOrDefault}",
                    string.IsNullOrEmpty(properties.HelpText.ValueOrDefault) ? "" : $"help '{properties.HelpText.ValueOrDefault}'",
                    $"{properties.BoundingRectangle.ValueOrDefault}",
                }.Where(part => part.Length > 0));
        }
        catch (Exception unread) when (unread is InvalidOperationException or COMException)
        {
            return $"unreadable: {unread.Message}";
        }
    }

    private static string Focused(Launched launched)
    {
        var focused = launched.Automation.FocusedElement();
        return focused is null ? "nothing" : Describe(focused);
    }

    private static string FocusedName(Launched launched) => launched.Automation.FocusedElement()?.Properties.Name.ValueOrDefault ?? "nothing";

    private static string FocusedId(Launched launched) => launched.Automation.FocusedElement()?.Properties.AutomationId.ValueOrDefault ?? "nothing";

    // The accounts whose rows are selected; the NavigationView's items are list items too.
    private static string[] SelectedAccounts(AutomationElement window) =>
        [.. window.FindAllDescendants(found => found.ByControlType(ControlType.ListItem))
            .Where(item => item.Patterns.SelectionItem.PatternOrDefault?.IsSelected.ValueOrDefault == true)
            .Select(item => item.Properties.AutomationId.ValueOrDefault ?? "")
            .Where(id => id.StartsWith("account.", StringComparison.Ordinal))
            .Select(id => id["account.".Length..])];

    private static string Joined(IEnumerable<string> items) => string.Join(" | ", items);

    private static string Buttons(AutomationElement scope) =>
        string.Join(
            ", ",
            scope.FindAllDescendants(found => found.ByControlType(ControlType.Button))
                .Select(button => $"{button.Properties.Name.ValueOrDefault} {(button.IsEnabled ? "enabled" : "disabled")}"));

    private static AutomationElement Around(AutomationElement element, ControlType kind)
    {
        for (var parent = element.Parent; parent is not null; parent = parent.Parent)
        {
            if (parent.Properties.ControlType.ValueOrDefault == kind)
            {
                return parent;
            }
        }

        throw new AssertFailedException($"no {kind} around '{element.Name}'");
    }

    // A key and its release, then the moment WinUI takes to move focus.
    private static void Press(VirtualKeyShort key)
    {
        Keyboard.Type(key);
        Wait.UntilInputIsProcessed();
        Thread.Sleep(150);
    }

    private static void RequireForeground(Window window)
    {
        if (!Foreground.Takes(window))
        {
            Assert.Inconclusive($"CF1: {Foreground.Holder()} keeps the foreground, so no key reaches Pitboard");
        }
    }

    // The menu's own window, which Pitboard brings forward as it opens the menu.
    private static void RequireForeground(Launched launched)
    {
        if (!OwnsForeground(launched))
        {
            Assert.Inconclusive($"CF1: {Foreground.Holder()} keeps the foreground, so no key reaches Pitboard's menu");
        }
    }

    private static bool OwnsForeground(Launched launched) =>
        GetWindowThreadProcessId(Foreground.Window(), out var owner) != 0 && owner == (uint)launched.Process.Id;

    private static Rectangle WorkArea()
    {
        if (!SystemParametersInfo(GetWorkArea, 0, out var area, 0))
        {
            throw new AssertFailedException($"SPI_GETWORKAREA failed: {Marshal.GetLastPInvokeError()}");
        }

        return Rectangle.FromLTRB(area.Left, area.Top, area.Right, area.Bottom);
    }

    private void Record(IEnumerable<string> lines, string name)
    {
        var file = Path.Combine(Directory.CreateTempSubdirectory("pitboard-site-").FullName, $"{name}.txt");
        File.WriteAllLines(file, lines);
        TestContext.AddResultFile(file);
    }

    private void Keep(CaptureImage picture, string name)
    {
        using (picture)
        {
            var file = Path.Combine(Directory.CreateTempSubdirectory("pitboard-site-").FullName, $"{name}.png");
            picture.ToFile(file);
            TestContext.AddResultFile(file);
        }
    }

    [LibraryImport("user32.dll")]
    private static partial uint GetWindowThreadProcessId(nint window, out uint process);

    [LibraryImport("user32.dll", EntryPoint = "SystemParametersInfoW", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool SystemParametersInfo(uint action, uint parameter, out Area area, uint change);

    [StructLayout(LayoutKind.Sequential)]
    private struct Area
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
