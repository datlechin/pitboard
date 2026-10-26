using System.Net;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Definitions;
using FlaUI.Core.Tools;
using Microsoft.Win32;

namespace Pitboard.UITests;

[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed class ReleaseGateTests
{
    private const string Run = @"Software\Microsoft\Windows\CurrentVersion\Run";

    private const string Notifier = @"Software\Classes\AppUserModelId\Pitboard.Pitboard";

    private const string Feed = "https://github.com/datlechin/pitboard/releases/latest/download/appcast-windows.xml";

    private const string NoFeed = "No full release of Pitboard for Windows is out yet, so there is no update to check for.";

    private static readonly TimeSpan Shortly = TimeSpan.FromSeconds(30);

    [TestMethod]
    public void TheReleaseBuildRefusesUntilPitboardForWindowsIsReleased()
    {
        if (Builds.ProductMajor() > 0)
        {
            Assert.Inconclusive(
                "the release gate is open at 1.x, where TheReleasedBuildTakesBackWhatItRegisteredOnUnregister starts the Release build");
        }

        using var launched = Launched.Start(Builds.Release);

        var window = launched.Window("Pitboard");
        StringAssert.Contains(Launched.Said(window), "Pitboard for Windows isn’t released yet.");
        Launched.Dismiss(window);
        Assert.IsTrue(launched.Process.WaitForExit(TimeSpan.FromSeconds(10)), "Pitboard went on running");
        Assert.AreEqual(1, launched.Process.ExitCode);
        Assert.AreEqual("", string.Join(", ", launched.Written()));
    }

    // What the MSI's full uninstall finds, which AGENTS.md allows only on the fresh account test-as-standard-user.ps1 makes.
    [TestMethod]
    public void TheReleasedBuildTakesBackWhatItRegisteredOnUnregister()
    {
        if (Builds.ProductMajor() == 0)
        {
            Assert.Inconclusive("the Release build refuses at 0.x, which TheReleaseBuildRefusesUntilPitboardForWindowsIsReleased holds");
        }

        var dataFolder = Launched.DataFolders[0];
        Assert.AreEqual("", string.Join(", ", Registered(activator: null)), "this account is not a fresh one");
        Assert.IsFalse(Directory.Exists(dataFolder), $"this account is not a fresh one: {dataFolder} is there");

        using var launched = Launched.Start(Builds.Release);
        var unregistered = false;
        try
        {
            // A fresh account's first launch, whose welcome's Get started opens the window.
            var welcome = launched.Window(Words.Welcome);
            Launched.Within(welcome, found => found.ByAutomationId("welcome.done"), "Get started").AsButton().Invoke();
            var window = launched.Window("Pitboard");
            launched.Select("SettingsItem");
            var toggle = launched.Find(found => found.ByAutomationId("settings.startAtSignIn"), "settings.startAtSignIn");
            toggle.Patterns.Toggle.Pattern.Toggle();
            Assert.IsTrue(
                Retry.WhileFalse(
                    () => toggle.Patterns.Toggle.Pattern.ToggleState.Value == ToggleState.On, Shortly, TimeSpan.FromMilliseconds(250),
                    ignoreException: true).Result,
                $"start at sign-in stayed off: {launched.Shown()}");
            var activator = Activator() ?? throw new AssertFailedException($"Pitboard registered no notifications: {launched.Shown()}");
            CollectionAssert.AreEquivalent(
                new[] { $@"{Run}\Pitboard", Notifier, Class(activator) }, Registered(activator).ToArray());
            FollowsTheFeed(launched, window);

            HiddenWindows.AskToEnd(launched.Process);
            Assert.IsTrue(launched.Process.WaitForExit(Shortly), $"Pitboard went on running: {launched.State()}");
            Assert.AreEqual(0, launched.Process.ExitCode, launched.State());
            using var unregistering = launched.Again("--unregister");
            unregistered = unregistering.Process.WaitForExit(Shortly);
            Assert.IsTrue(unregistered, $"--unregister went on running: {unregistering.State()}");
            Assert.AreEqual(0, unregistering.Process.ExitCode, unregistering.State());
            Assert.AreEqual("", string.Join(", ", Registered(activator)));
        }
        finally
        {
            if (!unregistered)
            {
                TakeBack(launched);
            }

            Launched.Remove(dataFolder);
        }
    }

    // As a beta's Settings show what GitHub answers for the feed: none until a full release carries one.
    private static void FollowsTheFeed(Launched launched, AutomationElement window)
    {
        if (!Builds.ProductBeta() || FeedAnswer() is not { } answer || answer is not (HttpStatusCode.OK or HttpStatusCode.NotFound))
        {
            return;
        }

        var checkNow = launched.Find(found => found.ByAutomationId("settings.checkNow"), "settings.checkNow");
        var offered = answer is HttpStatusCode.OK;
        Assert.IsTrue(
            Retry.WhileFalse(() => checkNow.IsEnabled == offered, Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result,
            $"GitHub answered the feed {answer}, and Check now is {(checkNow.IsEnabled ? "enabled" : "dimmed")}: {launched.Shown()}");
        if (!offered)
        {
            Launched.Shows(window, NoFeed, within: Shortly);
        }
    }

    private static HttpStatusCode? FeedAnswer()
    {
        using var http = new HttpClient { Timeout = Shortly };
        try
        {
            using var answer = http.GetAsync(Feed, HttpCompletionOption.ResponseHeadersRead).GetAwaiter().GetResult();
            return answer.StatusCode;
        }
        catch (Exception unread) when (unread is HttpRequestException or TaskCanceledException)
        {
            return null;
        }
    }

    // A failure above leaves only itself: the copy ends and what it registered goes.
    private static void TakeBack(Launched launched)
    {
        if (!launched.Process.HasExited)
        {
            launched.Process.Kill(entireProcessTree: true);
            launched.Process.WaitForExit();
        }

        using var unregistering = launched.Again("--unregister");
        unregistering.Process.WaitForExit(Shortly);
    }

    // The class Register names for a click, which Register alone writes.
    private static Guid? Activator()
    {
        using var key = Registry.CurrentUser.OpenSubKey(Notifier);
        return Guid.TryParse(key?.GetValue("CustomActivator") as string, out var named) ? named : null;
    }

    private static string Class(Guid activator) => $@"Software\Classes\CLSID\{activator:B}";

    private static IEnumerable<string> Registered(Guid? activator)
    {
        using (var run = Registry.CurrentUser.OpenSubKey(Run))
        {
            if (run?.GetValue("Pitboard") is not null)
            {
                yield return $@"{Run}\Pitboard";
            }
        }

        string[] names = activator is { } named ? [Notifier, Class(named)] : [Notifier];
        foreach (var name in names)
        {
            using var key = Registry.CurrentUser.OpenSubKey(name);
            if (key is not null)
            {
                yield return name;
            }
        }
    }
}
