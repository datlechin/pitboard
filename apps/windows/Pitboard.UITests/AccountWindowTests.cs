using FlaUI.Core.AutomationElements;
using FlaUI.Core.Definitions;
using FlaUI.Core.Input;
using FlaUI.Core.Tools;
using FlaUI.Core.WindowsAPI;

namespace Pitboard.UITests;

// The macOS AccountWindowTests. In a fixture an account's window loads the fixture's stand-in pages, and saves in the
// fixture's folder, so nothing reaches either site or the Downloads of whoever runs them.
[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed class AccountWindowTests
{
    [TestMethod]
    public void OpeningAClaudeAccountsWindowFromItsRow()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "onlyOne");
        launched.OpenWindow();

        OpenFromItsRow(launched, "claude/work", "claude.ai");

        Launched.Loads(launched.AccountWindow("work"), "claude.ai stand-in");
    }

    [TestMethod]
    public void OpeningAWindowAgainBringsTheSameOneForward()
    {
        using var launched = OpenChatGPT();

        OpenFromItsRow(launched, "codex/main");

        Assert.IsFalse(
            Retry.WhileFalse(() => Windows(launched, "main") > 1, TimeSpan.FromSeconds(3)).Result, "a second window of main opened");
        Assert.AreEqual(1, Windows(launched, "main"));
    }

    // The link asks for a new window, which WebView2 asks the app for, and the page set there loads it.
    [TestMethod]
    public void ASignInLinkOpensASignInWindowThatClosesItself()
    {
        using var launched = OpenChatGPT();
        var window = launched.AccountWindow("main");

        Launched.Press(window, "Continue with appleid.apple.com");
        var signIn = launched.Find(found => found.ByAutomationId("sign-in"), "sign-in window");
        Launched.Press(signIn, "Done");

        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sign-in")), "the sign-in window stayed");
        Launched.Shows(window, "chatgpt.com stand-in");
    }

    [TestMethod]
    public void ASignInScriptOpensASignInWindow()
    {
        using var launched = OpenChatGPT();

        Launched.Press(launched.AccountWindow("main"), "Sign in in a window");
        var signIn = launched.Find(found => found.ByAutomationId("sign-in"), "sign-in window");
        Launched.Press(signIn, "Done");

        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sign-in")), "the sign-in window stayed");
    }

    [TestMethod]
    public void ClosingTheAccountsWindowClosesItsSignInWindow()
    {
        using var launched = OpenChatGPT();
        var window = launched.AccountWindow("main");
        Launched.Press(window, "Continue with appleid.apple.com");
        launched.Find(found => found.ByAutomationId("sign-in"), "sign-in window");

        window.Close();

        // A wait's Result is whether what it waited for came.
        Assert.IsTrue(
            Retry.WhileTrue(() => Windows(launched, "main") > 0, TimeSpan.FromSeconds(10), ignoreException: true).Result,
            $"the account's window stayed: {launched.Shown()}");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sign-in")), $"the sign-in window stayed: {launched.Shown()}");
    }

    [TestMethod]
    public void GoogleSignInIsStoppedInTheWindow()
    {
        using var launched = OpenChatGPT();
        var window = launched.AccountWindow("main");

        Launched.Press(window, "Continue with Google");

        Assert.IsTrue(Says(window, "window.note", Words.GoogleStopped), "the window did not say Google was stopped");
        Launched.Shows(window, "chatgpt.com stand-in");
    }

    // WebView2 names no frame that started a download, so the site's own asks first; WebView2's own list then names it,
    // and opens the folder it went to, which no surface of Pitboard's does.
    [TestMethod]
    public void ASiteDownloadIsListedInTheWindow()
    {
        using var launched = OpenChatGPT();
        var window = launched.AccountWindow("main");

        Launched.Press(window, "Download notes");
        launched.Answer("question", "PrimaryButton");
        // Saved before the list opens: an open list named nothing in runs 38874616742, 38879058247 and 38896104748.
        var saved = Path.Combine(Path.GetTempPath(), "pitboard-fixture", "Downloads", "notes.txt");
        Assert.IsTrue(
            Retry.WhileFalse(() => File.Exists(saved), TimeSpan.FromSeconds(30), TimeSpan.FromMilliseconds(250)).Result,
            $"notes.txt was not saved as {saved}: {Saved(Path.GetDirectoryName(saved)!)}; {launched.Shown()}");
        launched.Invoke("window.downloads");

        // Opened again where it is not up, as a person would.
        var listed = Retry.WhileFalse(
            () =>
            {
                if (Launched.Lines(window).Any(line => line.Contains("notes.txt", StringComparison.Ordinal)))
                {
                    return true;
                }

                if (window.FindFirstDescendant(found => found.ByName("Open downloads folder")) is null)
                {
                    window.FindFirstDescendant(found => found.ByAutomationId("window.downloads"))
                        ?.Patterns.Invoke.Pattern.Invoke();
                }

                return false;
            },
            TimeSpan.FromSeconds(30), TimeSpan.FromSeconds(1), ignoreException: true).Result;
        Assert.IsTrue(listed, $"WebView2's download list does not name notes.txt: {Launched.Pressables(window)}");
        Assert.IsNotNull(
            Launched.Pressable(window, "Open downloads folder"),
            $"WebView2's download list does not open the downloads folder: {Launched.Pressables(window)}");
    }

    [TestMethod]
    public void FindShowsTheFindBar()
    {
        using var launched = OpenChatGPT();
        var window = launched.AccountWindow("main");
        Foreground.Require(window);
        Launched.Within(window, found => found.ByName("chatgpt.com stand-in").And(found.ByControlType(ControlType.Text)), "the page")
            .Click();

        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_F);

        var bar = Retry.WhileNull(
            () => window.FindFirstDescendant(found => found.ByControlType(ControlType.Edit)),
            TimeSpan.FromSeconds(5), TimeSpan.FromMilliseconds(250), ignoreException: true).Result;
        Assert.IsNotNull(bar, "WebView2's find bar did not show");
    }

    // From the toolbar, where the page's own keys do not reach.
    [TestMethod]
    public void TheToolbarTakesBackAndCtrlW()
    {
        using var launched = OpenChatGPT();
        var window = launched.AccountWindow("main");
        Launched.Press(window, "A chat");
        Launched.Shows(window, "A Pitboard fixture page at /chat/fixture", exact: false);
        Foreground.Require(window);
        Launched.Within(window, found => found.ByAutomationId("MoreButton"), "the command bar's More").Focus();

        Keyboard.TypeSimultaneously(VirtualKeyShort.ALT, VirtualKeyShort.LEFT);

        Launched.Shows(window, "A Pitboard fixture page at /.", exact: false);
        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_W);
        Assert.IsTrue(
            Retry.WhileTrue(() => Windows(launched, "main") > 0, TimeSpan.FromSeconds(10), ignoreException: true).Result,
            $"the account's window stayed: {launched.Shown()}");
    }

    [TestMethod]
    public void RemovingWebsiteDataAsksFirstAndStartsOver()
    {
        using var launched = OpenChatGPT();
        var window = launched.AccountWindow("main");
        // The first opening says how to sign in; put it away, and leave the home page.
        var note = Launched.Within(window, found => found.ByAutomationId("window.note"), "note");
        Launched.Within(note, found => found.ByAutomationId("CloseButton"), "the note's close button").Patterns.Invoke.Pattern.Invoke();
        Launched.Press(window, "A chat");
        Launched.Shows(window, "A Pitboard fixture page at /chat/fixture", exact: false);

        Launched.Within(window, found => found.ByAutomationId("MoreButton"), "the command bar's More").Patterns.Invoke.Pattern.Invoke();
        launched.Invoke("window.removeData");
        launched.Answer("question", "PrimaryButton");

        Assert.IsTrue(Says(window, "window.note", Words.SignInTo("chatgpt.com")), "the window did not say how to sign in");
        Launched.Shows(window, "A Pitboard fixture page at /.", exact: false);
    }

    [TestMethod]
    public void AnAccountsShortcutMenuOpensItsWindow()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        launched.OpenWindow();

        OpenFromItsRow(launched, "codex/spare");

        Launched.Loads(launched.AccountWindow("spare"), "chatgpt.com stand-in");
    }

    // From its row's shortcut menu, which needs no notification-area icon. Ended if it fails here: a Pitboard left
    // running takes every later launch's hand-over.
    private static Launched OpenChatGPT()
    {
        var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        try
        {
            launched.OpenWindow();
            OpenFromItsRow(launched, "codex/main");
            Launched.Loads(launched.AccountWindow("main"), "chatgpt.com stand-in");
            return launched;
        }
        catch
        {
            launched.Dispose();
            throw;
        }
    }

    internal static void OpenFromItsRow(Launched launched, string qualified, string site = "chatgpt.com")
    {
        Launched.OpenMenu(launched.Row(qualified));
        var label = qualified[(qualified.IndexOf('/', StringComparison.Ordinal) + 1)..];
        launched.Find(found => found.ByAutomationId($"window.{site}/{label}"), "the row's window item").Patterns.Invoke.Pattern.Invoke();
    }

    private static int Windows(Launched launched, string label) => launched.Titles().Count(shown => Launched.OfAccount(shown, label));

    private static bool Says(AutomationElement window, string automationId, string text) =>
        Retry.WhileFalse(
            () => window.FindFirstDescendant(found => found.ByAutomationId(automationId)) is { } element
                && Launched.Lines(element).Any(line => line.Contains(text, StringComparison.Ordinal)),
            TimeSpan.FromSeconds(10), TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    private static string Saved(string folder) =>
        Directory.Exists(folder)
            ? $"{folder} holds {string.Join(", ", Directory.GetFileSystemEntries(folder).Select(Path.GetFileName))}"
            : $"there is no {folder}";
}
