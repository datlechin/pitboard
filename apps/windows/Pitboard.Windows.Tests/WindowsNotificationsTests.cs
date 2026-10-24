using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using System.Text.RegularExpressions;
using System.Xml.Linq;
using Microsoft.Windows.AppNotifications;
using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class WindowsNotificationsTests
{
    private const string Icon = @"C:\Users\Dana Lee\AppData\Local\Programs\Pitboard\Pitboard.ico";

    private static readonly RunOutNotice RunOut = new(
        Id: "claude/personal/session/-1800018000", Title: "personal has no 5-hour limit left", Subtitle: "Claude Code",
        Body: "work has 58% of its own left.", Clicked: "accounts", Button: new NotificationButton("Switch", "switch/claude/work"));

    // A warm-up by itself that came to nothing, by its account and code (crates/pitboard-ffi/src/model/warming.rs).
    private static readonly RunOutNotice WarmFailed = new(
        Id: "warm-failed/personal/warm_up_not_started",
        Title: "Pitboard couldn’t warm up an account", Subtitle: null,
        Body: "`personal` was not warmed up: the request was answered, and Anthropic still has no period running.",
        Clicked: "limits", Button: null);

    private static readonly TimeSpan Patience = TimeSpan.FromSeconds(30);

    private static XElement Toast(RunOutNotice notice) => XElement.Parse(WindowsNotifications.Notification(notice).Payload);

    [TestMethod]
    public void TheNotificationSaysTheRunOutAndTheToolAndOffersSwitch()
    {
        var notification = WindowsNotifications.Notification(RunOut);
        var toast = XElement.Parse(notification.Payload);

        string[] said = ["personal has no 5-hour limit left", "Claude Code", "work has 58% of its own left."];
        CollectionAssert.AreEqual(said, toast.Descendants("text").Select(text => text.Value).ToArray());
        var button = toast.Descendants("action").Single();
        Assert.AreEqual("Switch", (string?)button.Attribute("content"));
        Assert.AreEqual("clicked=switch/claude/work", (string?)button.Attribute("arguments"));
        Assert.AreEqual("clicked=accounts", (string?)toast.Attribute("launch"));
        Assert.AreEqual(WindowsNotifications.Tag(RunOut.Id), notification.Tag);
    }

    [TestMethod]
    public void ANoticeWithNothingToSwitchToHasNoButton()
    {
        var notification = WindowsNotifications.Notification(WarmFailed);
        var toast = XElement.Parse(notification.Payload);

        Assert.HasCount(2, toast.Descendants("text"));
        Assert.IsEmpty(toast.Descendants("actions"));
        Assert.AreEqual(WindowsNotifications.Tag(WarmFailed.Id), notification.Tag);
    }

    [TestMethod]
    public void ALongIdGivesATagOfSixtyFourCharacters()
    {
        // A warm-up failure of no account, whose id carries the core's words.
        const string id = "warm-failed/warm_up_in_progress/another Pitboard is warming up an account. Try again in a minute.";
        var tag = WindowsNotifications.Tag(id);

        Assert.IsGreaterThan(64, id.Length);
        Assert.AreEqual(64, tag.Length);
        Assert.IsTrue(tag.All(char.IsAsciiHexDigitLower), tag);
        Assert.AreEqual(tag, WindowsNotifications.Tag(id), "the same notice replaces itself");
        Assert.AreNotEqual(tag, WindowsNotifications.Tag(RunOut.Id));
    }

    // Windows hands a button's arguments back as the payload holds them.
    [TestMethod]
    public void AButtonGivesBackWhatItAsksFor()
    {
        foreach (var clicked in new[] { "switch/claude/work", "switch/codex/a;b=c%3B", "sign-in-again/claude/Zoë & <Ö>'s \"team\"" })
        {
            var notice = RunOut with { Button = new NotificationButton("Switch", clicked) };
            var arguments = (string)Toast(notice).Descendants("action").Single().Attribute("arguments")!;

            Assert.AreEqual(new Intent.NotificationClicked(clicked), WindowsNotifications.Read(Decoded(arguments)), arguments);
        }
    }

    // One an older Pitboard posted hands back nothing the model reads, which opens the accounts.
    [TestMethod]
    public void AClickOnTheNotificationItselfGivesBackWhatItAsksFor()
    {
        var launch = (string)Toast(WarmFailed).Attribute("launch")!;

        Assert.AreEqual(new Intent.NotificationClicked("limits"), WindowsNotifications.Read(Decoded(launch)));
        Assert.AreEqual(new Intent.NotificationClicked(""), WindowsNotifications.Read(Decoded("")));
    }

    [TestMethod]
    public void ADebugBuildNeverRegisters()
    {
        var notifications = new WindowsNotifications(
            Build.Debug, () => throw new AssertFailedException("Windows' app notifications were asked"), Icon);

        Assert.IsNull(notifications.Start(_ => Assert.Fail("a click")));
        notifications.Post(RunOut);
        notifications.Stop();

        Assert.AreEqual("A Debug build posts no notifications.", notifications.Off);
        Assert.IsNull(notifications.Refusal, "the settings would say a Debug build's own off");
    }

    [TestMethod]
    public void AReleaseBuildListensRegistersAndReadsItsLaunchThenPostsAndUnregisters()
    {
        var system = new StandInNotifications();
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);

        Assert.IsNull(notifications.Start(_ => Assert.Fail("a click")));
        notifications.Post(RunOut);
        notifications.Stop();
        notifications.Post(RunOut);

        string[] asked =
        [
            "supported", "listen", $"register Pitboard {Icon}", "launched by", $"show {WindowsNotifications.Tag(RunOut.Id)}",
            "unregister",
        ];
        CollectionAssert.AreEqual(asked, system.Asked);
        Assert.IsNull(notifications.Off);
    }

    [TestMethod]
    public void TheSwitchButtonSwitchesWhilePitboardRuns()
    {
        var system = new StandInNotifications();
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);
        var clicked = new List<Intent>();
        notifications.Start(clicked.Add);

        system.Click(Decoded("clicked=switch/claude/work"));
        system.Click(Decoded("clicked=accounts"));

        Intent[] handed = [new Intent.NotificationClicked("switch/claude/work"), new Intent.NotificationClicked("accounts")];
        CollectionAssert.AreEqual(handed, clicked);
    }

    [TestMethod]
    public void TheSwitchButtonThatStartedPitboardSwitches()
    {
        var system = new StandInNotifications { Launched = Decoded("clicked=switch/claude/work") };
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);

        Assert.AreEqual(new Intent.NotificationClicked("switch/claude/work"), notifications.Start(_ => Assert.Fail("a click while running")));
    }

    [TestMethod]
    public void AProcessWindowsOffersNoNotificationsRegistersNothing()
    {
        var system = new StandInNotifications { Supported = false };
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);

        Assert.IsNull(notifications.Start(_ => Assert.Fail("a click")));
        notifications.Post(RunOut);
        notifications.Stop();

        Assert.AreEqual("supported", string.Join(", ", system.Asked));
        Assert.IsNotNull(notifications.Off);
    }

    [TestMethod]
    public void ANotificationWindowsRefusesIsRefusedByItsTypeAndHresultAlone()
    {
        var gone = Marshal.GetExceptionForHR(unchecked((int)0x80070490));
        var system = new StandInNotifications { RefusesToShow = gone };
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);
        notifications.Start(_ => Assert.Fail("a click"));

        var refused = Assert.ThrowsExactly<PlatformException.Failed>(() => notifications.Post(RunOut));

        Assert.AreEqual("COMException 0x80070490", refused.reason);
        Assert.AreEqual($"show {WindowsNotifications.Tag(RunOut.Id)}", system.Asked[^1]);
    }

    [TestMethod]
    public void AFailedRegistrationIsSaidAndPostsNothing()
    {
        var system = new StandInNotifications { Refuses = new UnauthorizedAccessException("Access is denied.") };
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);

        Assert.IsNull(notifications.Start(_ => Assert.Fail("a click")));
        notifications.Post(RunOut);
        notifications.Stop();

        CollectionAssert.AreEqual(new[] { "supported", "listen", $"register Pitboard {Icon}" }, system.Asked);
        Assert.AreEqual("Access is denied.", notifications.Off);
        Assert.AreEqual("Access is denied.", notifications.Refusal);
    }

    // Settings > System > Notifications turns them off and on while Pitboard runs, so each settings visit reads again.
    [TestMethod]
    public void EachReadSaysWhatWindowsHoldsAndWhetherItsSettingsTurnThemOn()
    {
        var system = new StandInNotifications();
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);
        notifications.Start(_ => Assert.Fail("a click"));

        Assert.IsNull(notifications.Read());

        (AppNotificationSetting Setting, NotificationsBlocked Said)[] blocked =
        [
            (AppNotificationSetting.DisabledForApplication, new(WindowsNotifications.OffForPitboard, InSettings: true)),
            (AppNotificationSetting.DisabledForUser, new(WindowsNotifications.OffForEveryApp, InSettings: true)),
            (AppNotificationSetting.DisabledByGroupPolicy, new(WindowsNotifications.OffByPolicy, InSettings: false)),
            (AppNotificationSetting.DisabledByManifest, new(WindowsNotifications.Unoffered, InSettings: false)),
            (AppNotificationSetting.Unsupported, new(WindowsNotifications.Unoffered, InSettings: false)),
        ];
        foreach (var (setting, said) in blocked)
        {
            system.Turned = setting;
            Assert.AreEqual(said, notifications.Read(), $"{setting}");
        }

        system.Turned = AppNotificationSetting.Enabled;
        Assert.IsNull(notifications.Read(), "turned on again");

        system.SettingRefuses = new InvalidOperationException("The RPC server is unavailable.");
        Assert.IsNull(notifications.Read(), "a read that failed says nothing it does not know");
    }

    [TestMethod]
    public void ADebugBuildOrOneNotStartedReadsNothing()
    {
        var debug = new WindowsNotifications(
            Build.Debug, () => throw new AssertFailedException("Windows' app notifications were asked"), Icon);
        debug.Start(_ => Assert.Fail("a click"));
        var system = new StandInNotifications { Turned = AppNotificationSetting.DisabledForApplication };

        Assert.IsNull(debug.Read());
        Assert.IsNull(new WindowsNotifications(Build.Release, () => system, Icon).Read());
        Assert.IsEmpty(system.Asked);
    }

    [TestMethod]
    public void AFailedRegistrationIsReadAsItsReason()
    {
        var system = new StandInNotifications { Refuses = new UnauthorizedAccessException("Access is denied.") };
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);
        notifications.Start(_ => Assert.Fail("a click"));

        Assert.AreEqual(new NotificationsBlocked("Access is denied.", InSettings: false), notifications.Read());
        CollectionAssert.DoesNotContain(system.Asked, "setting");
    }

    // ModelTests.ARunOutIsPostedThroughTheAppsNotifications, through to what Windows is asked to show.
    [TestMethod]
    public void AFixturesRunOutIsShownWithItsSwitchButton()
    {
        var folder = Directory.CreateTempSubdirectory("pitboard-notifications-").FullName;
        var system = new StandInNotifications();
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);
        notifications.Start(_ => Assert.Fail("a click"));
        var told = new Told();
        using var model = PitboardModel.FixtureIn("oneTool", folder, told, new NoOtherApps(), notifications, new WindowsLocalTime());

        model.Send(new Intent.Start());
        Assert.IsNotNull(told.Until(snapshot => snapshot is { Reading: false, UpdatedWindow.Length: > 0, Sections.Length: > 0 }), "no accounts were read");
        model.Send(new Intent.SwitchTo("claude/personal"));
        var shown = system.Shown.TryTake(out var notification, Patience);
        model.Shutdown();

        Assert.IsTrue(shown, "nothing was shown");
        var toast = XElement.Parse(notification!.Payload);
        Assert.AreEqual("personal has no 5-hour limit left", toast.Descendants("text").First().Value);
        Assert.AreEqual("clicked=switch/claude/work", (string?)toast.Descendants("action").Single().Attribute("arguments"));
    }

    // As the Windows App SDK 2.5.1 reads Arguments: pairs at ';', each at its first '=', then %25, %3B and %3D.
    private static Dictionary<string, string> Decoded(string arguments)
    {
        static string Decode(string value) =>
            Regex.Replace(value, "%25|%3B|%3D", found => found.Value switch { "%25" => "%", "%3B" => ";", _ => "=" });

        var decoded = new Dictionary<string, string>();
        foreach (var pair in arguments.Split(';'))
        {
            var at = pair.IndexOf('=', StringComparison.Ordinal);
            decoded[Decode(at < 0 ? pair : pair[..at])] = at < 0 ? "" : Decode(pair[(at + 1)..]);
        }

        return decoded;
    }

    private sealed class Told : ModelListener
    {
        private readonly BlockingCollection<Snapshot> _told = [];

        public void Changed(Snapshot snapshot) => _told.Add(snapshot);

        public Snapshot? Until(Func<Snapshot, bool> done)
        {
            while (_told.TryTake(out var snapshot, Patience))
            {
                if (done(snapshot))
                {
                    return snapshot;
                }
            }

            return null;
        }
    }

    internal sealed class StandInNotifications : IAppNotifications
    {
        private Action<IDictionary<string, string>>? _invoked;

        public List<string> Asked { get; } = [];

        public bool Supported { get; init; } = true;

        public Exception? Refuses { get; init; }

        public Exception? RefusesToShow { get; init; }

        public IDictionary<string, string>? Launched { get; init; }

        public AppNotificationSetting Turned { get; set; } = AppNotificationSetting.Enabled;

        public Exception? SettingRefuses { get; set; }

        public void Click(IDictionary<string, string> arguments) => _invoked!(arguments);

        public bool IsSupported()
        {
            Asked.Add("supported");
            return Supported;
        }

        public void Listen(Action<IDictionary<string, string>> invoked)
        {
            Asked.Add("listen");
            _invoked = invoked;
        }

        public void Register(string displayName, Uri icon)
        {
            Asked.Add($"register {displayName} {icon.OriginalString}");
            if (Refuses is { } refused)
            {
                throw refused;
            }
        }

        public IDictionary<string, string>? LaunchedBy()
        {
            Asked.Add("launched by");
            return Launched;
        }

        // Shown on the model's thread.
        public BlockingCollection<AppNotification> Shown { get; } = [];

        public void Show(AppNotification notification)
        {
            Asked.Add($"show {notification.Tag}");
            if (RefusesToShow is { } refused)
            {
                throw refused;
            }

            Shown.Add(notification);
        }

        public AppNotificationSetting Setting()
        {
            Asked.Add("setting");
            return SettingRefuses is { } refused ? throw refused : Turned;
        }

        public void Unregister() => Asked.Add("unregister");

        public void UnregisterAll() => Asked.Add("unregister all");
    }
}
