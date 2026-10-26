using System.Diagnostics;
using System.Runtime.InteropServices;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Definitions;
using FlaUI.Core.Input;
using FlaUI.Core.Tools;
using FlaUI.Core.WindowsAPI;

namespace Pitboard.UITests;

[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed partial class LifecycleTests
{
    private const uint GetIcon = 0x007F;
    private const nuint BigIcon = 1;

    public TestContext TestContext { get; set; } = null!;

    // CF1 and CF5. Windows may keep the foreground for a window that held it before the launch (SetForegroundWindow).
    [TestMethod]
    public void TheFirstLaunchWindowComesToTheFrontAndTakesAClick()
    {
        var before = Foreground.Window();
        var clock = Stopwatch.StartNew();
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");

        var window = launched.Window("Pitboard");
        TestContext.WriteLine($"CF5: the window was up {clock.ElapsedMilliseconds} ms after the start");
        var settings = window.FindFirstDescendant(found => found.ByAutomationId("SettingsItem"))!;
        var handle = window.Properties.NativeWindowHandle.Value;
        var inFront = Retry.WhileFalse(() => Foreground.Window() == handle, TimeSpan.FromSeconds(5)).Result;
        if (!inFront && before != 0 && Foreground.Window() == before)
        {
            Assert.Inconclusive($"CF1: {Foreground.Holder()} kept the foreground it held before the launch, over {window.BoundingRectangle}");
        }

        Assert.IsTrue(inFront, $"CF1: the window is not in front, {Foreground.Holder()} is");
        Mouse.Click(settings.GetClickablePoint());

        var selected = Retry.WhileFalse(
            () => settings.Patterns.SelectionItem.Pattern.IsSelected.Value, TimeSpan.FromSeconds(5)).Result;
        Assert.IsTrue(selected, "CF1: the click did not reach the window");
    }

    // CF2: UI Automation finds a WinUI element by its AutomationId and invokes it.
    [TestMethod]
    public void AWinUIElementIsFoundAndInvokedByItsAutomationId()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");

        var settings = launched.Window("Pitboard").FindFirstDescendant(found => found.ByAutomationId("SettingsItem"));

        Assert.IsNotNull(settings, "CF2: no element SettingsItem");
        settings.Patterns.SelectionItem.Pattern.Select();
        Assert.IsTrue(settings.Patterns.SelectionItem.Pattern.IsSelected.Value, "CF2: SettingsItem was not selected");
    }

    // Its title bar, Alt+Tab and a Debug build's taskbar button show it; WinUI's window class has none.
    [TestMethod]
    public void TheWindowCarriesPitboardsIcon()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");

        var handle = launched.Window("Pitboard").Properties.NativeWindowHandle.Value;

        Assert.AreNotEqual(0, SendMessage(handle, GetIcon, BigIcon, 0), "the window has no icon of its own");
    }

    [TestMethod]
    public void TheAccountAndOpenLinkWindowsCarryPitboardsIcon()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        launched.OpenWindow();
        AccountWindowTests.OpenFromItsRow(launched, "codex/main");
        var account = launched.AccountWindow("main");
        var picker = launched.Share("https://chatgpt.com/c/shared");

        foreach (var window in new[] { account, picker })
        {
            var handle = window.Properties.NativeWindowHandle.Value;
            Assert.AreNotEqual(0, SendMessage(handle, GetIcon, BigIcon, 0), $"the {window.Title} window has no icon of its own");
        }
    }

    [TestMethod]
    public void ClosingTheWindowLeavesPitboardRunning()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");
        var window = launched.Window("Pitboard");

        window.Close();

        Assert.IsTrue(Retry.WhileTrue(() => window.IsAvailable, TimeSpan.FromSeconds(10)).Result, "the window stayed open");
        Assert.IsFalse(launched.Process.WaitForExit(TimeSpan.FromSeconds(3)), "Pitboard quit with its window");
    }

    [TestMethod]
    public void AClosedWindowOpensWhereItWasAtItsSize()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");
        var window = launched.Window("Pitboard");
        var transform = window.Patterns.Transform.Pattern;
        transform.Move(120, 90);
        transform.Resize(900, 640);
        var placed = window.BoundingRectangle;
        Close(window);

        var again = launched.OpenWindow();

        Assert.AreEqual(placed, again.BoundingRectangle);
    }

    [TestMethod]
    public void AMaximizedWindowOpensMaximizedAgain()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");
        var window = launched.Window("Pitboard");
        Show(window, WindowVisualState.Maximized);
        Close(window);

        var again = launched.OpenWindow();

        Assert.IsTrue(Shows(again, WindowVisualState.Maximized), $"it opened {again.Patterns.Window.Pattern.WindowVisualState.Value}");
    }

    // A second launch brings the window forward as a click on its taskbar button would.
    [TestMethod]
    public void AWindowMinimizedFromMaximizedComesBackMaximized()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");
        var window = launched.Window("Pitboard");
        Show(window, WindowVisualState.Maximized);
        Show(window, WindowVisualState.Minimized);

        var again = launched.OpenWindow();

        Assert.IsTrue(Shows(again, WindowVisualState.Maximized), $"it came back {again.Patterns.Window.Pattern.WindowVisualState.Value}");
    }

    [TestMethod]
    public void CtrlWClosesTheWindowAndPitboardKeepsRunning()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");
        var window = launched.Window("Pitboard");
        Foreground.Require(window);

        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_W);

        Assert.IsTrue(Retry.WhileTrue(() => window.IsAvailable, TimeSpan.FromSeconds(10)).Result, "the window stayed open");
        Assert.IsFalse(launched.Process.WaitForExit(TimeSpan.FromSeconds(3)), "Pitboard quit with its window");
    }

    // firstLaunch's page starts with its command bar's buttons, accounts.refresh and accounts.add.
    [TestMethod]
    public void F6MovesFocusBetweenThePaneAndThePage()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");
        var window = launched.Window("Pitboard");
        Foreground.Require(window);
        window.FindFirstDescendant(found => found.ByAutomationId("pane.accounts"))!.Focus();
        string Focused() => launched.Automation.FocusedElement()?.Properties.AutomationId.ValueOrDefault ?? "";
        bool Reaches(Func<string, bool> place) =>
            Retry.WhileFalse(() => place(Focused()), TimeSpan.FromSeconds(5), TimeSpan.FromMilliseconds(100)).Result;

        Keyboard.Type(VirtualKeyShort.F6);
        Assert.IsTrue(Reaches(id => id.StartsWith("accounts.", StringComparison.Ordinal)), $"F6 left the focus on '{Focused()}'");

        Keyboard.TypeSimultaneously(VirtualKeyShort.SHIFT, VirtualKeyShort.F6);
        Assert.IsTrue(Reaches(id => id == "pane.accounts"), $"Shift+F6 left the focus on '{Focused()}'");

        Keyboard.Type(VirtualKeyShort.F6);
        Assert.IsTrue(Reaches(id => id.StartsWith("accounts.", StringComparison.Ordinal)), $"F6 again left the focus on '{Focused()}'");
    }

    [TestMethod]
    public void ASecondLaunchOpensTheWindowAgain()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");
        var window = launched.Window("Pitboard");
        window.Close();
        Assert.IsTrue(Retry.WhileTrue(() => window.IsAvailable, TimeSpan.FromSeconds(10)).Result, "the window stayed open");

        launched.OpenWindow();
    }

    // As a double click on Pitboard's shortcut can.
    [TestMethod]
    public void TwoLaunchesAtOnceOpenOneWindow()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "noClaudeCode");
        using var again = launched.Again();

        var either = Retry.WhileFalse(
            () => launched.Process.HasExited || again.Process.HasExited, TimeSpan.FromSeconds(60)).Result;
        Assert.IsTrue(either, "neither copy handed its launch to the other");
        var (gone, stays) = launched.Process.HasExited ? (launched, again) : (again, launched);
        Assert.IsTrue(gone.HandedOver(out var why), why);
        stays.Window("Pitboard");
        Assert.HasCount(1, stays.Windows("Pitboard"));
    }

    // noClaudeCode is not a first launch, and Windows puts a new icon in the overflow.
    [TestMethod]
    public void ALaunchAsFromStartOpensTheWindow()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "noClaudeCode");

        launched.Window("Pitboard");
    }

    [TestMethod]
    public void AStartAtSignInOpensNoWindow()
    {
        using var launched = Launched.Start(Builds.Debug, "noClaudeCode", "--sign-in");

        launched.Running();
        Assert.IsFalse(
            Retry.WhileFalse(() => launched.Windows("Pitboard").Length > 0, TimeSpan.FromSeconds(10)).Result,
            $"a start at sign-in opened the window: {launched.State()}");
    }

    private static bool Shows(Window window, WindowVisualState state) =>
        Retry.WhileFalse(
            () => window.Patterns.Window.Pattern.WindowVisualState.Value == state, TimeSpan.FromSeconds(10),
            TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    private static void Show(Window window, WindowVisualState state)
    {
        window.Patterns.Window.Pattern.SetWindowVisualState(state);
        Assert.IsTrue(Shows(window, state), $"the window did not become {state}");
    }

    private static void Close(Window window)
    {
        window.Close();
        Assert.IsTrue(Retry.WhileTrue(() => window.IsAvailable, TimeSpan.FromSeconds(10)).Result, "the window stayed open");
    }

    [LibraryImport("user32.dll", EntryPoint = "SendMessageW")]
    private static partial nint SendMessage(nint window, uint message, nuint wParam, nint lParam);
}
