using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class SettingsViewModelTests
{
    private static readonly AtLimitShown Switching = Snapshots.Read(1).Machine.AtLimit with
    {
        Choice = AtLimit.Switch,
        Note = "Pitboard switches Claude Code.",
    };

    private static readonly string[] AtLimitChoices = ["Do nothing", "Notify me", "Switch automatically"];

    private static readonly WarmShown TwoWarmed = new(
        Title: "Warm-up", From: 360, To: 24 * 60, RangeLabel: "6:00 AM to midnight",
        Accounts:
        [
            new WarmAccountShown(new AccountKey(Tool.Claude, "personal"), "claude/personal", "personal · Max", On: true, Standing: "Warms up again at 22:50"),
            new WarmAccountShown(new AccountKey(Tool.Claude, "work"), "claude/work", "work", On: false, Standing: null),
        ],
        NoneEnrolled: null, Standing: "Next warm-up: personal at 22:50.", Note: "Pitboard warms up Claude Code accounts only.",
        Enabled: true);

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

    // Plan 4.11: Uninstall… asks the model's question first, then says what it is doing, and where it stopped short why,
    // with Try again.
    [TestMethod]
    public void UninstallAsksFirstThenSaysHowItGoes()
    {
        var question = new Question(
            "Uninstall Pitboard?", "Pitboard deletes its saved sign-ins, settings and account windows.", "Uninstall",
            Destructive: false, ConfirmIsDefault: false);
        var offered = Snapshots.Read(1) with { Uninstall = new ItemOffer("Uninstall…", new Intent.Uninstall(), Enabled: true, question) };
        var (shown, model, app) = Over(offered);

        Assert.AreEqual(new Choice("Uninstall…", new Intent.Uninstall(), Enabled: true), shown.UninstallChoice);
        Assert.AreEqual(question, shown.UninstallQuestion);
        Assert.AreEqual(Words.Settings.UninstallNote, shown.UninstallSays);
        Assert.IsNull(shown.UninstallStopped);
        shown.Uninstall();
        CollectionAssert.AreEqual(new Intent[] { new Intent.Uninstall() }, model.Sent.ToArray());

        app.Apply(offered with
        {
            Revision = 2,
            Uninstall = offered.Uninstall with { Enabled = false },
            Removal = new RemovalShown.Removing("Uninstalling Pitboard…"),
        });
        Assert.IsTrue(shown.Uninstalling);
        Assert.IsFalse(shown.OffersUninstall);
        Assert.AreEqual("Uninstalling Pitboard…", shown.UninstallSays);

        var again = new Choice("Try again", new Intent.Uninstall(), Enabled: true);
        app.Apply(offered with
        {
            Revision = 3,
            Removal = new RemovalShown.Stopped(
                "Pitboard couldn’t remove everything", "A saved sign-in couldn’t be deleted yet, so Pitboard kept its folder.", again),
        });
        Assert.IsFalse(shown.Uninstalling);
        Assert.AreEqual(again, shown.UninstallChoice);
        Assert.IsNull(shown.UninstallQuestion, "asked again after it was answered");
        Assert.AreEqual("Pitboard couldn’t remove everything", shown.UninstallStopped);
        Assert.AreEqual("A saved sign-in couldn’t be deleted yet, so Pitboard kept its folder.", shown.UninstallSays);
    }

    [TestMethod]
    public void TheVersionLeavesOutTheCommitTheSdkAdds()
    {
        Assert.AreEqual("0.0.0", Versions.Of("0.0.0+3f2a9c1"));
        Assert.AreEqual("1.0.0-beta.1", Versions.Of("1.0.0-beta.1"));
        Assert.AreEqual("", Versions.Of((string?)null));
    }

    [TestMethod]
    public void TheVersionIsNamedAsTheModelNamesItAndAnUpdateFoundAsReadyToInstall()
    {
        var updates = new StandInUpdates();
        var (shown, _, _) = Over(Snapshots.Read(1), updates: updates, version: "1.0.0");

        Assert.AreEqual(PitboardModelMethods.NamedVersion("1.0.0"), shown.UpdatesState);

        updates.Found("1.0.1");
        Assert.AreEqual(PitboardModelMethods.ReadyToInstall("1.0.1"), shown.UpdatesState);
    }

    [TestMethod]
    public void ABuildThatCannotUpdateSaysSoAndABetaSaysHowItGetsTheNext()
    {
        Assert.AreEqual(PitboardModelMethods.NoUpdateKey(), SettingsViewModel.UpdatesNotes(new NoUpdates(), "0.0.0").Single());
        CollectionAssert.AreEqual(
            new[] { PitboardModelMethods.NoUpdateKey(), Words.Settings.BetaUpdates },
            SettingsViewModel.UpdatesNotes(new NoUpdates(), "1.0.0-beta.1"));
        Assert.IsEmpty(SettingsViewModel.UpdatesNotes(new StandInUpdates(), "1.0.0"));
        Assert.AreEqual(
            Words.Settings.UpdateKeyInvalid,
            SettingsViewModel.UpdatesNotes(new NoUpdates(Words.Settings.UpdateKeyInvalid), "1.0.0").Single());

        var waiting = new StandInUpdates(beta: true);
        waiting.Feed(found: false);
        CollectionAssert.AreEqual(
            new[] { Words.Settings.NoReleaseToUpdateTo, Words.Settings.BetaUpdates },
            SettingsViewModel.UpdatesNotes(waiting, "1.0.0-beta.4"));
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
    public void TheCommandLineSaysWhoPutsItOnThePathWhereTheModelDoes()
    {
        const string note = "Pitboard’s installer puts pitboard on your PATH.";
        var (shown, _, _) = Over(With(1, machine => machine with
        {
            CommandLine = machine.CommandLine with { Found = new FoundCommandLine.Nowhere(), TerminalNote = note },
        }));

        Assert.IsTrue(shown.HasTerminalNote);
        Assert.AreEqual(note, shown.TerminalNote);
        Assert.IsFalse(Over(Snapshots.Read(1)).Shown.HasTerminalNote, "not looked for yet");
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
                new Intent.SetWarm(new AccountKey(Tool.Claude, "work"), true),
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

    // The choice is named by the share it is made at, under a heading without it; the share's card keeps its header.
    [TestMethod]
    public void TheChoiceIsNamedByItsShareAndTheShareItsRange()
    {
        var (shown, _, app) = Over(With(1, machine => machine with { AtLimit = Switching }));

        Assert.AreEqual("When a limit reaches", shown.AtLimitHeading);
        Assert.AreEqual("When a limit reaches 95%", shown.AtLimitLabel);
        CollectionAssert.AreEqual(AtLimitChoices, shown.AtLimitTitles);
        Assert.AreEqual(2, shown.AtLimitIndex);
        Assert.AreEqual("At", shown.ShareTitle);
        Assert.AreEqual("From 50% to 99%", shown.ShareRange);
        Assert.IsTrue(shown.CanSetShare);

        app.Apply(With(2, machine => machine with
        {
            AtLimit = Switching with { Choice = AtLimit.Nothing, ShareEnabled = false, Note = null },
        }));

        Assert.AreEqual(0, shown.AtLimitIndex);
        Assert.IsFalse(shown.CanSetShare, "no share while nothing happens at it");
        Assert.IsFalse(shown.HasAtLimitNote);
    }

    [TestMethod]
    public void TheTermsAreTheModelsLink()
    {
        var (shown, _, _) = Over(With(1, machine => machine with { AtLimit = Switching }));

        Assert.AreEqual(Snapshots.Terms.LinkTitle, shown.TermsLinkTitle);
        Assert.AreEqual(Snapshots.Terms.Link, shown.TermsLink);
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
        Assert.AreEqual(Words.Settings.DebugNeverStarts, shown.StartupNote);
    }

    // As the welcome says under the same switch: what starting at sign-in is for, where nothing holds it back.
    [TestMethod]
    public void StartAtSignInSaysWhatItIsFor()
    {
        var (shown, _, _) = Over(Snapshots.Read(1));

        Assert.AreEqual(Words.Settings.OpenAtLoginNote, shown.StartupNote);
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
        Assert.AreEqual(Words.Settings.StartupTurnedOff, shown.StartupNote);
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

        NotificationsBlocked? blocked = new(Words.Settings.NotificationsUnoffered!, InSettings: false);
        var (off, _, _) = Over(Snapshots.Read(1), notifications: () => blocked);
        off.Shown();

        Assert.IsTrue(off.HasNotificationsOff);
        Assert.AreEqual(Words.Settings.NotificationsUnoffered, off.NotificationsOff);
        Assert.IsFalse(off.OffersNotificationSettings, "a link to a switch that is not the person's");

        blocked = new(Words.Settings.NotificationsOff, InSettings: true);
        off.ReadWindowsSettings();
        Assert.AreEqual(Words.Settings.NotificationsOff, off.NotificationsOff);
        Assert.IsTrue(off.OffersNotificationSettings);

        blocked = null;
        off.ReadWindowsSettings();
        Assert.IsFalse(off.HasNotificationsOff);
        Assert.IsFalse(off.OffersNotificationSettings);
    }
}
