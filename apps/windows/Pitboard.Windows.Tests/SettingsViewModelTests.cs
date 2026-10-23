using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class SettingsViewModelTests
{
    private static readonly AtLimitShown Switching = Snapshots.Read(1).Machine.AtLimit with
    {
        Choice = AtLimit.Switch,
        Note = "Pitboard switches Claude Code.",
        Terms = Snapshots.Terms,
    };

    private static readonly WarmShown TwoWarmed = new(
        Title: "Warm up limits", From: 360, To: 24 * 60, RangeLabel: "6:00 AM to midnight",
        Accounts:
        [
            new WarmAccountShown("claude/personal", "personal", "Max", On: true, Standing: "Warms up again at 22:50"),
            new WarmAccountShown("claude/work", "work", Plan: null, On: false, Standing: null),
        ],
        NoneEnrolled: null, Standing: "Next warm-up: personal at 22:50.", Note: "Pitboard warms up Claude Code accounts only.",
        Enabled: true, Terms: Snapshots.Terms);

    private static Snapshot With(ulong revision, Func<MachineShown, MachineShown> change)
    {
        var read = Snapshots.Read(revision);
        return read with { Machine = change(read.Machine) };
    }

    private static (SettingsViewModel Shown, StandInModel Model, AppModel App) Over(
        Snapshot snapshot, StartAtSignIn? start = null, IUpdates? updates = null, string version = "0.0.0",
        Func<NotificationsBlocked?>? notifications = null)
    {
        var model = new StandInModel(snapshot);
        var app = new AppModel(model);
        var shown = new SettingsViewModel(
            app, start ?? new StartAtSignIn(new FixtureStartupItem()), updates ?? new NoUpdates(), version, notifications ?? (() => null));
        shown.Listen();
        return (shown, model, app);
    }

    [TestMethod]
    public void TheVersionLeavesOutTheCommitTheSdkAdds()
    {
        Assert.AreEqual("0.0.0", Versions.Of("0.0.0+3f2a9c1"));
        Assert.AreEqual("1.0.0-beta.1", Versions.Of("1.0.0-beta.1"));
        Assert.AreEqual("", Versions.Of((string?)null));
        Assert.AreEqual("Pitboard 1.0.0", Versions.Named("1.0.0"));
        Assert.AreEqual("Pitboard", Versions.Named(""));
    }

    [TestMethod]
    public void ABuildThatCannotUpdateSaysSoAndABetaSaysHowItGetsTheNext()
    {
        Assert.AreEqual(PitboardFfiMethods.NoUpdateKey(), SettingsViewModel.UpdatesNotes(new NoUpdates(), "0.0.0").Single());
        CollectionAssert.AreEqual(
            new[] { PitboardFfiMethods.NoUpdateKey(), SettingsViewModel.BetaNote },
            SettingsViewModel.UpdatesNotes(new NoUpdates(), "1.0.0-beta.1"));
        Assert.IsEmpty(SettingsViewModel.UpdatesNotes(new StandInUpdates(), "1.0.0"));
        Assert.AreEqual(NoUpdates.BadKey, SettingsViewModel.UpdatesNotes(new NoUpdates(NoUpdates.BadKey), "1.0.0").Single());

        var waiting = new StandInUpdates(beta: true);
        waiting.Feed(found: false);
        CollectionAssert.AreEqual(
            new[] { SparkleUpdates.NoFeed, SettingsViewModel.BetaNote }, SettingsViewModel.UpdatesNotes(waiting, "1.0.0-beta.4"));
    }

    // A beta that found no feed when Pitboard started looks again, as 1.0.0 may have come out since.
    [TestMethod]
    public void ShowingThePageLooksForTheFeedAgain()
    {
        var updates = new StandInUpdates(beta: true);
        var (shown, _, _) = Over(Snapshots.Read(1), updates: updates);

        shown.Shown();

        Assert.AreEqual(1, updates.Looks);
    }

    // The model's span runs to 24:00, which a time picker cannot hold.
    [TestMethod]
    public void TheEndOfADayShowsAsMidnight()
    {
        Assert.AreEqual(TimeSpan.Zero, SettingsViewModel.TimeOfDay(24 * 60));
        Assert.AreEqual(new TimeSpan(22, 30, 0), SettingsViewModel.TimeOfDay((22 * 60) + 30));
        Assert.AreEqual((ushort)360, SettingsViewModel.Minutes(new TimeSpan(6, 0, 0)));
        Assert.AreEqual((ushort)0, SettingsViewModel.Minutes(TimeSpan.Zero));
    }

    [TestMethod]
    public void TheShareIsAWholePercentageTheModelTakes()
    {
        Assert.AreEqual((byte)96, SettingsViewModel.ShareOf(96.4, Switching));
        Assert.AreEqual((byte)99, SettingsViewModel.ShareOf(120, Switching));
        Assert.AreEqual((byte)50, SettingsViewModel.ShareOf(10, Switching));
        Assert.IsNull(SettingsViewModel.ShareOf(double.NaN, Switching), "a cleared box sends nothing");
    }

    [TestMethod]
    public void OnlyACommandLineFoundNowhereSaysWhoPutsItOnThePath()
    {
        static CommandLineShown Found(FoundCommandLine found) =>
            new(Found: found, InTerminal: "", UpdateNote: null, OffersLink: false, CannotLink: null);

        Assert.AreEqual(SettingsViewModel.NewTerminals, SettingsViewModel.TerminalNoteFor(Found(new FoundCommandLine.Nowhere())));
        Assert.IsNull(SettingsViewModel.TerminalNoteFor(Found(new FoundCommandLine.Bundled(@"C:\Pitboard\bin\pitboard.exe"))));
        Assert.IsNull(SettingsViewModel.TerminalNoteFor(new CommandLineShown(null, null, null, false, null)), "not looked for yet");
    }

    // A control set to what the model says raises its change event as a person's change would; the page's echo guard
    // was a flag around every assignment.
    [TestMethod]
    public void SettingWhatTheModelSaysSendsNothing()
    {
        var (shown, model, _) = Over(With(1, machine => machine with
        {
            AtLimit = Switching,
            Warm = TwoWarmed,
            Schedule = machine.Schedule with { On = true },
        }));

        shown.ChooseAtLimit(2);
        shown.ChooseAtLimit(-1);
        Assert.IsTrue(shown.SetShare(95));
        shown.SetWarmFrom(new TimeSpan(6, 0, 0));
        shown.SetWarmTo(TimeSpan.Zero);
        shown.SetWarm("claude/personal", true);
        shown.SetWarm("claude/work", false);
        shown.SetRenewsDaily(true);
        shown.SetStartsAtSignIn(false);

        Assert.IsEmpty(model.Sent, string.Join(", ", model.Sent));
    }

    [TestMethod]
    public void EachChangeSendsWhatTheModelTakes()
    {
        var (shown, model, _) = Over(With(1, machine => machine with { Warm = TwoWarmed }));

        shown.ChooseAtLimit(2);
        shown.SetShare(96.4);
        shown.SetWarmFrom(new TimeSpan(7, 0, 0));
        shown.SetWarmTo(new TimeSpan(21, 0, 0));
        shown.SetWarm("claude/work", true);
        shown.SetRenewsDaily(true);

        CollectionAssert.AreEqual(
            new Intent[]
            {
                new Intent.SetAtLimit(AtLimit.Switch, 95),
                new Intent.SetAtLimit(AtLimit.Notify, 96),
                new Intent.SetWarmRange(420, 1440),
                new Intent.SetWarmRange(360, 1260),
                new Intent.SetWarm("claude/work", true),
                new Intent.SetSchedule(true),
            },
            model.Sent.ToArray());
    }

    [TestMethod]
    public void AClearedShareBoxSendsNothingAndIsShownTheShareAgain()
    {
        var (shown, model, _) = Over(Snapshots.Read(1));

        Assert.IsFalse(shown.SetShare(double.NaN));
        Assert.IsEmpty(model.Sent);
        Assert.AreEqual(95.0, shown.Share);
    }

    [TestMethod]
    public void ShowingThePageReadsWhatATerminalOrAnotherProgramCanChange()
    {
        var (shown, model, _) = Over(Snapshots.Read(1));

        shown.Shown();

        CollectionAssert.AreEqual(new Intent[] { new Intent.ReadSchedule(), new Intent.LookForCommandLine() }, model.Sent.ToArray());
    }

    // The choice is named by the share it is made at; the share's card keeps its header.
    [TestMethod]
    public void TheChoiceIsNamedByItsShareAndTheShareItsRange()
    {
        var (shown, _, app) = Over(With(1, machine => machine with { AtLimit = Switching }));

        Assert.AreEqual("When a limit reaches 95%", shown.AtLimitLabel);
        CollectionAssert.AreEqual(new[] { "Do nothing", "Notify me", "Switch automatically" }, shown.AtLimitTitles);
        Assert.AreEqual(2, shown.AtLimitIndex);
        Assert.AreEqual("Share of a limit", shown.ShareTitle);
        Assert.AreEqual("From 50% to 99%", shown.ShareRange);
        Assert.IsTrue(shown.CanSetShare);
        Assert.IsTrue(shown.HasAtLimitTerms);
        Assert.AreEqual("Anthropic's terms may cover automatic switching and warm-ups.", shown.AtLimitTerms);

        app.Apply(With(2, machine => machine with
        {
            AtLimit = Switching with { Choice = AtLimit.Nothing, ShareEnabled = false, Note = null, Terms = null },
        }));

        Assert.AreEqual(0, shown.AtLimitIndex);
        Assert.IsFalse(shown.CanSetShare, "no share while nothing happens at it");
        Assert.IsFalse(shown.HasAtLimitNote);
        Assert.IsFalse(shown.HasAtLimitTerms);
    }

    [TestMethod]
    public void WarmUpListsEachAccountAndKeepsItsRow()
    {
        var (shown, _, app) = Over(With(1, machine => machine with { Warm = TwoWarmed }));
        var personal = shown.WarmAccounts[0];

        Assert.AreEqual("personal · Max, work", string.Join(", ", shown.WarmAccounts.Select(row => row.Title)));
        Assert.AreEqual("settings.warm.claude/personal", personal.AutomationId);
        Assert.IsTrue(personal.On);
        Assert.IsFalse(shown.NoWarmAccounts);

        app.Apply(With(2, machine => machine with
        {
            Warm = TwoWarmed with { Accounts = [TwoWarmed.Accounts[0] with { On = false, Standing = null }], Enabled = false },
        }));

        Assert.AreSame(personal, shown.WarmAccounts.Single());
        Assert.IsFalse(personal.On);
        Assert.IsFalse(personal.Enabled);
        Assert.AreEqual("", personal.Standing);
    }

    [TestMethod]
    public void ADebugBuildHoldsStartAtSignInBackAndSaysWhy()
    {
        var start = StartAtSignIn.For(
            new AppStart(new Identity(Build.Debug), new(), null, @"C:\Local\Pitboard App Debug"), @"C:\Build\Pitboard.exe");
        var (shown, _, _) = Over(Snapshots.Read(1), start);

        Assert.IsFalse(shown.CanStartAtSignIn);
        Assert.AreEqual("A Debug build never starts when you sign in to Windows.", shown.StartupNote);
    }

    [TestMethod]
    public void StartAtSignInTurnedOffInSettingsShowsOffHeldBackAndSaysWhere()
    {
        var key = new StandInRunKey();
        var item = new RunValue(Build.Release, @"C:\Users\Dana Lee\AppData\Local\Programs\Pitboard\Pitboard.exe", key);
        item.Write(starts: true);
        key.Approvals["Pitboard"] = [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        var (shown, _, _) = Over(Snapshots.Read(1), new StartAtSignIn(item));

        shown.Shown();

        Assert.IsFalse(shown.StartsAtSignIn);
        Assert.IsFalse(shown.CanStartAtSignIn);
        Assert.AreEqual(SettingsViewModel.TurnedOffNote, shown.StartupNote);
        Assert.IsTrue(shown.OffersStartupSettings);
    }

    // Back from Settings > Apps > Startup, where the person turned Pitboard off while the page showed.
    [TestMethod]
    public void ReadingWindowsSettingsAgainShowsWhatSettingsChangedAndAsksTheModelNothing()
    {
        var key = new StandInRunKey();
        var item = new RunValue(Build.Release, @"C:\Users\Dana Lee\AppData\Local\Programs\Pitboard\Pitboard.exe", key);
        item.Write(starts: true);
        var (shown, model, _) = Over(Snapshots.Read(1), new StartAtSignIn(item));
        shown.Shown();
        var sent = model.Sent.Count;
        var told = 0;
        shown.PropertyChanged += (_, _) => told++;

        Assert.IsTrue(shown.StartsAtSignIn);
        Assert.IsFalse(shown.OffersStartupSettings);

        key.Approvals["Pitboard"] = [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        shown.ReadWindowsSettings();

        Assert.IsFalse(shown.StartsAtSignIn);
        Assert.IsTrue(shown.OffersStartupSettings);
        Assert.IsGreaterThan(0, told);
        Assert.AreEqual(sent, model.Sent.Count, string.Join(", ", model.Sent.Skip(sent)));
    }

    // Read at each visit and as the window comes back, from Settings > System > Notifications among others.
    [TestMethod]
    public void NotificationsAreSaidOnlyWhileWindowsBlocksThem()
    {
        var (on, _, _) = Over(Snapshots.Read(1));
        on.Shown();
        Assert.IsFalse(on.HasNotificationsOff);

        NotificationsBlocked? blocked = new(WindowsNotifications.Unoffered, InSettings: false);
        var (off, _, _) = Over(Snapshots.Read(1), notifications: () => blocked);
        off.Shown();

        Assert.IsTrue(off.HasNotificationsOff);
        Assert.AreEqual(WindowsNotifications.Unoffered, off.NotificationsOff);
        Assert.IsFalse(off.OffersNotificationSettings, "a link to a switch that is not the person's");

        blocked = new(WindowsNotifications.OffForPitboard, InSettings: true);
        off.ReadWindowsSettings();
        Assert.AreEqual(WindowsNotifications.OffForPitboard, off.NotificationsOff);
        Assert.IsTrue(off.OffersNotificationSettings);

        blocked = null;
        off.ReadWindowsSettings();
        Assert.IsFalse(off.HasNotificationsOff);
        Assert.IsFalse(off.OffersNotificationSettings);
    }
}
