using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Capturing;
using FlaUI.Core.Definitions;
using FlaUI.Core.Tools;

namespace Pitboard.UITests;

// A picture of each surface, named platform-surface-state, as the macOS ScreenshotTests takes them, for the docs and for
// pull requests. CI keeps windows-2025's as windows-app-pictures.
[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed partial class ScreenshotTests
{
    private const uint ExtendedFrameBounds = 9;

    // UIA_ScrollPatternNoScroll: the other direction stays where it is.
    private const double NoScroll = -1;

    private static readonly TimeSpan Patience = TimeSpan.FromSeconds(10);

    private static readonly TimeSpan OneFrame = TimeSpan.FromMilliseconds(50);

    private readonly DirectoryInfo _pictures = Directory.CreateTempSubdirectory("pitboard-pictures-");

    public TestContext TestContext { get; set; } = null!;

    [TestMethod]
    public void TheMenuAndTheWindowForTheDocs()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        var window = InFront(launched.OpenWindow());
        launched.Tip(Words.SwitchBeforeRunningOut);
        KeepWindow(window, "windows-window-tip-at-limit");

        // The other pictures show the accounts, past the tip on what happens at a limit.
        launched.DeclineTip(Words.SwitchBeforeRunningOut);
        launched.Row("codex/spare");
        var work = launched.Row("claude/work");
        Launched.Within(work, found => found.ByAutomationId("account.inUse"), "work in use");
        KeepWindow(window, "windows-window-accounts");
        Launched.Expand(work);
        Launched.Within(work, found => found.ByAutomationId("detail.copyEmail"), "work's email");
        KeepWindow(window, "windows-window-account-detail");

        launched.OpenFromSettings(window, "settings.diagnostics");
        launched.Element("machine.summary");
        KeepWindow(window, "windows-diagnostics");

        // The menu opens over the window, as the macOS picture shows it.
        var frame = Frame(window);
        HiddenWindows.OpenMenu(launched.Process, frame.Left + 40, frame.Top + 60);
        var menu = Around(launched.Element("menu.account.codex/main"), ControlType.Menu);
        Keep(menu.BoundingRectangle, menu, "windows-tray-menu-accounts");
    }

    [TestMethod]
    public void TheDetailsOfASavedAndAnExpiredAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        var window = InFront(launched.OpenWindow());
        launched.DeclineTip(Words.SwitchBeforeRunningOut);
        var personal = launched.Row("claude/personal");
        Opened(personal);
        KeepWindow(window, "windows-window-account-detail-saved");

        Launched.Expander(personal).Patterns.ExpandCollapse.Pattern.Collapse();
        Opened(launched.Row("claude/old"));
        KeepWindow(window, "windows-window-account-detail-expired");
    }

    [TestMethod]
    public void AnAccountsMenuAndItsNameDialog()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = InFront(launched.OpenWindow());
        launched.DeclineTip(Words.UseWithCodex);
        var row = launched.Row("claude/personal");
        Launched.OpenMenu(row);
        var menu = Around(launched.Element("account.copyEmail"), ControlType.Menu);
        Keep(Rectangle.Union(Frame(window), menu.BoundingRectangle), window, "windows-menu-account");
        launched.Invoke("account.rename");
        var name = launched.Field("sheet.name");
        KeepWindow(window, "windows-sheet-rename");

        // The dialog says why under the field as it dims Save, so each picture waits for Save to change.
        var save = Launched.Within(launched.Element("sheet"), found => found.ByAutomationId("PrimaryButton"), "Save");
        Launched.Type(name, "claude/third");
        Assert.IsTrue(Dims(save, true), "Save took claude/third");
        KeepWindow(window, "windows-sheet-rename-problem");
        Launched.Type(name, "home");
        Assert.IsTrue(Dims(save, false), "Save refused home");
        Launched.Type(name, "");
        Assert.IsTrue(Dims(save, true), "Save took no name");
        KeepWindow(window, "windows-sheet-rename-empty");
        Launched.Type(name, "work");
        launched.Answer("sheet", "PrimaryButton");
        Launched.Within(launched.Element("sheet.failure"), found => found.ByControlType(ControlType.Text), "why work is no name");
        KeepWindow(window, "windows-sheet-rename-failure");
        launched.Answer("sheet", "CloseButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.name")), "the dialog stayed up");

        Launched.OpenMenu(row);
        launched.Invoke("account.forget");
        launched.Element("question");
        KeepWindow(window, "windows-question-remove");
        launched.Answer("question", "CloseButton");
    }

    // Open is never pressed: it would open the fixture's address in a real browser.
    [TestMethod]
    public void AddingAnAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = InFront(launched.OpenWindow());
        launched.Invoke("accounts.add");
        launched.Element("sheet.tool");
        KeepWindow(window, "windows-sheet-add");

        launched.Answer("sheet", "PrimaryButton");
        var code = launched.Field("sheet.code");
        Assert.IsTrue(Launched.Enables(launched.Element("sheet.openLink")), "Open stayed dimmed");
        KeepWindow(window, "windows-sheet-add-signing-in");
        Launched.Type(code, "fixture-code#state");
        launched.Answer("sheet", "PrimaryButton");

        launched.Field("sheet.name");
        KeepWindow(window, "windows-sheet-add-name");
        launched.Answer("sheet", "PrimaryButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.name")), "the dialog stayed up");
    }

    [TestMethod]
    public void SigningInAgainAndSwitching()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        var window = InFront(launched.OpenWindow());
        launched.DeclineTip(Words.SwitchBeforeRunningOut);

        Switch(launched, "claude/old");
        launched.Element("sheet");
        KeepWindow(window, "windows-sheet-sign-in-again");
        launched.Answer("sheet", "CloseButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet")), "the dialog stayed up");

        Switch(launched, "codex/spare");
        Said(launched.Element("notice.switch/codex"));
        KeepWindow(window, "windows-notice-switched-codex");
        Switch(launched, "claude/personal");
        launched.Element("notice.switch/claude");
        KeepWindow(window, "windows-notice-switched");
    }

    // The fixture's ChatGPT is a stand-in: nothing is quit.
    [TestMethod]
    public void AskingToQuitChatGpt()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "chatGPTOpen");
        var window = InFront(launched.OpenWindow());
        launched.DeclineTip(Words.SwitchBeforeRunningOut);
        Switch(launched, "codex/spare");
        launched.Element("question");
        KeepWindow(window, "windows-question-quit-chatgpt");
        launched.Answer("question", "CloseButton");
    }

    [TestMethod]
    public void CancellingASwitchThatDidNotFinish()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "stuck");
        var window = InFront(launched.OpenWindow());
        launched.Element("notice.action.abandonStuckSwitch");
        KeepWindow(window, "windows-notice-switch-unfinished");
        launched.Invoke("notice.action.abandonStuckSwitch");
        launched.Element("question");
        KeepWindow(window, "windows-question-cancel-switch");
        launched.Answer("question", "PrimaryButton");
        Said(launched.Element("notice.abandoned"));
        KeepWindow(window, "windows-notice-switch-cancelled");
    }

    [TestMethod]
    public void CleaningUpALeftoverSignIn()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "leftInFile");
        var window = InFront(launched.OpenWindow());
        launched.Element("notice.action.stow");
        KeepWindow(window, "windows-notice-left-in-file");

        launched.Invoke("notice.action.stow");
        var cleanUp = Launched.Within(launched.Element("sheet"), found => found.ByAutomationId("PrimaryButton"), "Clean up");
        Assert.IsTrue(Launched.Enables(cleanUp), "Clean up stayed dimmed");
        KeepWindow(window, "windows-sheet-clean-up");
        launched.Answer("sheet", "PrimaryButton");
        launched.Element("notice.stowed");
        KeepWindow(window, "windows-notice-cleaned-up");
    }

    [TestMethod]
    public void ASignInReplacedOutsidePitboard()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "loginReplaced");
        var window = InFront(launched.OpenWindow());
        launched.Element("notice.action.signInAgain");
        KeepWindow(window, "windows-notice-login-replaced");
    }

    // The app switches by itself a moment after its first read.
    [TestMethod]
    public void ASwitchByItselfOffersSwitchBack()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "switchedByItself");
        var window = InFront(launched.OpenWindow());
        var offered = Retry.WhileNull(
            () => window.FindFirstDescendant(found => found.ByAutomationId("notice.action.switchBack")),
            TimeSpan.FromSeconds(30), TimeSpan.FromMilliseconds(250), ignoreException: true).Result;
        Assert.IsNotNull(offered, $"no Switch back within 30 seconds: {launched.Shown()}");
        KeepWindow(window, "windows-notice-switched-by-itself");
    }

    [TestMethod]
    public void TheFirstLaunch()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");
        var welcome = InFront(launched.Window(Words.Welcome));
        launched.Element("welcome.done");
        KeepWindow(welcome, "windows-welcome");
        launched.Invoke("welcome.done");
        var window = InFront(launched.Window("Pitboard"));
        launched.Tip(Words.UseWithCodex);
        KeepWindow(window, "windows-window-first-launch");
    }

    // Claude Code's sign-in from the welcome goes through the window's dialog, and the welcome then says what it added.
    [TestMethod]
    public void TheWelcomeOnceAnAccountIsAdded()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");
        var welcome = launched.Window(Words.Welcome);
        var claude = Launched.Within(welcome, found => found.ByAutomationId("welcome.tool.claude"), "Claude Code");
        Launched.Within(claude, found => found.ByAutomationId("welcome.add"), "Claude Code's sign-in").AsButton().Invoke();
        launched.Answer("sheet", "PrimaryButton");
        var code = launched.Field("sheet.code");
        Assert.IsTrue(Launched.Enables(launched.Element("sheet.openLink")), "Open stayed dimmed");
        Launched.Type(code, "fixture-code#state");
        launched.Answer("sheet", "PrimaryButton");
        launched.Field("sheet.name");
        launched.Answer("sheet", "PrimaryButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.name")), "the dialog stayed up");

        Launched.Shows(welcome, Words.AddedAs, exact: false);
        KeepWindow(InFront(welcome), "windows-welcome-added");
    }

    [TestMethod]
    public void NoAccounts()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "empty");
        var window = InFront(launched.OpenWindow());
        launched.Tip(Words.UseWithCodex);
        launched.DeclineTip(Words.UseWithCodex);
        launched.Element("empty.choice");
        KeepWindow(window, "windows-window-no-accounts");
    }

    [TestMethod]
    public void NoClaudeCode()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "noClaudeCode");
        var window = InFront(launched.OpenWindow());
        launched.Element("empty.choice");
        KeepWindow(window, "windows-window-no-tool");
    }

    [TestMethod]
    public void AFailedRead()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "readFailure");
        var window = InFront(launched.OpenWindow());
        launched.Element("empty.choice");
        KeepWindow(window, "windows-window-read-failure");

        var frame = Frame(window);
        HiddenWindows.OpenMenu(launched.Process, frame.Left + 40, frame.Top + 60);
        var menu = Around(launched.Element("menu.openPitboard"), ControlType.Menu);
        Keep(menu.BoundingRectangle, menu, "windows-tray-menu-read-failure");
    }

    [TestMethod]
    public void ASignInPitboardHasNoNameFor()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "unnamed");
        var window = InFront(launched.OpenWindow());
        launched.DeclineTip(Words.UseWithCodex);
        launched.Element("account.action");
        KeepWindow(window, "windows-window-unnamed");
        launched.Invoke("account.action");
        launched.Field("sheet.name");
        KeepWindow(window, "windows-sheet-name-in-use");
    }

    [TestMethod]
    public void OneAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "onlyOne");
        var window = InFront(launched.OpenWindow());
        launched.Tip(Words.AddASecondAccount);
        KeepWindow(window, "windows-window-tip-second-account");
    }

    [TestMethod]
    public void ActivityAndSettings()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = InFront(launched.OpenWindow());
        launched.Select("SettingsItem");
        launched.Element("settings.startAtSignIn");
        KeepWindow(window, "windows-settings-general");
        var choice = launched.Element("settings.atLimit").AsComboBox();
        Launched.Expand(launched.Element("settings.atLimitGroup"));
        Assert.IsTrue(Launched.Enables(choice), "the choice stayed dimmed");
        // The model's choices, in its order: Do nothing, Notify me, Switch automatically.
        choice.Select(2);
        launched.Element("settings.autoSwitchStanding");
        KeepWindow(window, "windows-settings-limits-switch");

        var scroller = Scroller(launched.Element("settings.renewDaily"));
        scroller.Patterns.Scroll.Pattern.SetScrollPercent(NoScroll, 100);
        KeepWindow(window, "windows-settings-advanced");

        // Asked, and never answered: Uninstall would end the app.
        launched.Invoke("settings.uninstall");
        launched.Element("question");
        KeepWindow(window, "windows-question-uninstall");
        launched.Answer("question", "CloseButton");

        // Last, since its card opens by Enter, after which WinUI draws the focus where it moves.
        launched.OpenFromSettings(window, "settings.activity");
        Launched.Within(launched.Element("activity"), found => found.ByControlType(ControlType.ListItem), "a change");
        KeepWindow(window, "windows-activity");
    }

    // The fixture's personal is on for warm-ups with no five-hour period running, so it warms up as the app starts.
    [TestMethod]
    public void WarmingUp()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "warmUp");
        var window = InFront(launched.OpenWindow());
        Launched.Expand(launched.Row("claude/personal"));
        Launched.Shows(window, Words.WarmsUpAgain, exact: false, within: TimeSpan.FromSeconds(30));
        KeepWindow(window, "windows-window-warm-up");

        // Its link opens Settings on the warm-up's expander.
        launched.Invoke("detail.warmUpSettings");
        launched.Element("settings.warmStanding");
        KeepWindow(window, "windows-settings-warm-up");
    }

    // Neither question is answered: the fixture's page saves nothing, and its window keeps its data.
    [TestMethod]
    public void AnAccountsWindowAsksFirst()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        launched.OpenWindow();
        AccountWindowTests.OpenFromItsRow(launched, "codex/main");
        var window = launched.AccountWindow("main");
        Launched.Loads(window, "chatgpt.com stand-in");
        InFront(window);
        // As the main window opens: its own size runs past the runner's screen, under the taskbar.
        window.Patterns.Transform.Pattern.Resize(960, 680);

        Launched.Press(window, "Download notes");
        launched.Element("question");
        KeepWindow(window, "windows-question-download");
        launched.Answer("question", "CloseButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("question")), "the question stayed");

        Launched.Within(window, found => found.ByAutomationId("MoreButton"), "the command bar's More").Patterns.Invoke.Pattern.Invoke();
        launched.Invoke("window.removeData");
        launched.Element("question");
        KeepWindow(window, "windows-question-remove-website-data");
        launched.Answer("question", "CloseButton");
    }

    // A picture is of the screen, so it shows whatever is in front.
    private static Window InFront(Window window)
    {
        if (!Foreground.Takes(window))
        {
            Assert.Inconclusive($"CF1: {Foreground.Holder()} keeps the foreground, so a picture would show it");
        }

        window.Move(0, 0);
        return window;
    }

    private static bool Dims(AutomationElement button, bool dimmed) =>
        Retry.WhileFalse(() => button.IsEnabled != dimmed, Patience, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    // Its last command takes the focus, which scrolls the opened row into view.
    private static void Opened(AutomationElement row)
    {
        Launched.Expand(row);
        Launched.Within(row, found => found.ByAutomationId("detail.remove"), $"{row.AutomationId}'s Remove account").Focus();
    }

    private static void Switch(Launched launched, string qualified) =>
        Launched.Within(launched.Row(qualified), found => found.ByAutomationId("account.action"), $"{qualified}'s button")
            .AsButton().Invoke();

    // A notice shows before its lines do.
    private static void Said(AutomationElement notice) =>
        Assert.IsTrue(
            Retry.WhileFalse(() => Launched.Lines(notice).Count() > 1, Patience, TimeSpan.FromMilliseconds(250), ignoreException: true).Result,
            $"{notice.AutomationId} says nothing: {Launched.Said(notice)}");

    private static AutomationElement Scroller(AutomationElement inside)
    {
        for (var parent = inside.Parent; parent is not null; parent = parent.Parent)
        {
            if (parent.Patterns.Scroll.IsSupported)
            {
                return parent;
            }
        }

        throw new AssertFailedException($"nothing scrolls {inside.AutomationId}");
    }

    private void KeepWindow(Window window, string name) => Keep(Frame(window), window, name);

    private void Keep(Rectangle area, AutomationElement scope, string name)
    {
        using var picture = Drawn(area, scope, name);
        var file = Path.Combine(_pictures.FullName, $"{name}.png");
        picture.ToFile(file);
        TestContext.AddResultFile(file);
    }

    // A window, a menu and a dialog animate as they open, and UI Automation gives the bounds they end at before they are
    // drawn there: a picture is kept once the next one, a frame later, is the same. A ring turns until what it waits for
    // is done, so the rings in sight are left out of that comparison.
    private static CaptureImage Drawn(Rectangle area, AutomationElement scope, string what)
    {
        // A ring's class name may carry WinUI's namespace, and its arc's antialiased edge can spill past its bounds.
        var rings = scope.FindAllDescendants()
            .Where(found => found.Properties.ClassName.ValueOrDefault?.EndsWith("ProgressRing", StringComparison.Ordinal) == true)
            .Select(ring => ring.BoundingRectangle)
            .Select(ring => Rectangle.Inflate(new Rectangle(ring.X - area.X, ring.Y - area.Y, ring.Width, ring.Height), 2, 2))
            .ToArray();
        var deadline = DateTime.UtcNow + Patience;
        var taken = Capture.Rectangle(area);
        while (true)
        {
            Thread.Sleep(OneFrame);
            var next = Capture.Rectangle(area);
            var drawn = Pixels(taken, rings).SequenceEqual(Pixels(next, rings));
            taken.Dispose();
            taken = next;
            if (drawn)
            {
                return taken;
            }

            if (DateTime.UtcNow > deadline)
            {
                taken.Dispose();
                throw new AssertFailedException($"{what} was still changing after {Patience}");
            }
        }
    }

    private static byte[] Pixels(CaptureImage picture, Rectangle[] rings)
    {
        using var copy = new Bitmap(picture.Bitmap);
        using (var paint = Graphics.FromImage(copy))
        {
            foreach (var ring in rings)
            {
                paint.FillRectangle(Brushes.Black, ring);
            }
        }

        using var png = new MemoryStream();
        copy.Save(png, ImageFormat.Png);
        return png.ToArray();
    }

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

    // GetWindowRect takes in the window's drop shadow; the frame DWM draws leaves it out.
    private static Rectangle Frame(Window window)
    {
        var handle = window.Properties.NativeWindowHandle.Value;
        Marshal.ThrowExceptionForHR(DwmGetWindowAttribute(handle, ExtendedFrameBounds, out var frame, (uint)Marshal.SizeOf<Bounds>()));
        return Rectangle.FromLTRB(frame.Left, frame.Top, frame.Right, frame.Bottom);
    }

    [LibraryImport("dwmapi.dll")]
    private static partial int DwmGetWindowAttribute(nint window, uint attribute, out Bounds value, uint size);

    [StructLayout(LayoutKind.Sequential)]
    private struct Bounds
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
