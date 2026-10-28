using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class LaunchingTests
{
    private const string Fixture = @"C:\Temp\pitboard-fixture";
    private const string Shared = "pitboard-debug://open?url=https%3A%2F%2Fclaude.ai%2Fnew";
    private const string Account = "35c0d1c9-6b25-5cdc-b543-64c0c0d8de46";

    private static readonly ReadFailure Elevated = new("elevated", "Pitboard runs elevated.");
    private static readonly Identity Debug = new(Build.Debug);
    private static readonly Identity Release = new(Build.Release);

    private static Func<T> Unasked<T>(string what) => () => throw new AssertFailedException($"{what} was asked");

    [TestMethod]
    public void UnregisteringAsksNeitherTheCoreNorARunningCopy()
    {
        var start = Launching.Decide(["--unregister"], Release, null, Unasked<ReadFailure?>("the core"), Unasked<bool?>("the key"));

        Assert.AreEqual(Start.Unregister, start);
    }

    // The MSI's removal of PitboardShare's package, which a 0.x build refuses nothing of.
    [TestMethod]
    public void UnregisteringTheShareTargetAsksNeitherTheCoreNorARunningCopy()
    {
        var start = Launching.Decide(["--unregister-share"], Release, null, Unasked<ReadFailure?>("the core"), Unasked<bool?>("the key"));

        Assert.AreEqual(Start.UnregisterShare, start);
    }

    [TestMethod]
    public void ARefusedLaunchTakesNoKey()
    {
        var start = Launching.Decide([], Release, null, () => Elevated, Unasked<bool?>("the key"));

        Assert.AreEqual(Start.Refused, start);
    }

    [TestMethod]
    public void TheFirstCopyRunsAndASecondHandsItsLaunchOver()
    {
        Assert.AreEqual(Start.Run, Launching.Decide([], Release, null, () => null, () => true));
        Assert.AreEqual(Start.Redirect, Launching.Decide([], Release, null, () => null, () => false));
    }

    // Windows App SDK found no copy holding the key for one of two launches at once (docs/facts/windows-app.md).
    [TestMethod]
    public void ALaunchThatFindsNoCopyHoldingTheKeyIsDropped()
    {
        Assert.AreEqual(Start.Dropped, Launching.Decide([], Release, null, () => null, () => null));
    }

    [TestMethod]
    public void TheCopyHoldingTheKeyIsAskedForAgainUntilFound()
    {
        var answers = new Queue<string?>([null, null, "running"]);
        var pauses = 0;

        Assert.AreEqual("running", Launching.Holder(answers.Dequeue, () => pauses++));
        Assert.AreEqual(2, pauses);
    }

    [TestMethod]
    public void TheCopyHoldingTheKeyIsAskedForAFewTimesAtMost()
    {
        var asked = 0;

        Assert.IsNull(Launching.Holder<string>(() => { asked++; return null; }, () => { }));
        Assert.AreEqual(Launching.Asks, asked);
    }

    [TestMethod]
    public void TheLaunchAfterAnUpdateIsDroppedByARunningCopy()
    {
        Assert.AreEqual(Start.Dropped, Launching.Decide(["--updated"], Release, null, () => null, () => false));
        Assert.AreEqual(Start.Run, Launching.Decide(["--updated"], Release, null, () => null, () => true));
    }

    // Windows starts Pitboard at sign-in through the Run value, before the person asked for anything.
    [TestMethod]
    public void AStartAtSignInIsDroppedByARunningCopy()
    {
        Assert.AreEqual(Start.Dropped, Launching.Decide(["--sign-in"], Release, null, () => null, () => false));
        Assert.AreEqual(Start.Run, Launching.Decide(["--sign-in"], Release, null, () => null, () => true));
    }

    // A new icon goes to the overflow, so a launch from Start or a shortcut that opened no window would show nothing.
    [TestMethod]
    public void OnlyALaunchThePersonAskedForOpensTheWindow()
    {
        Assert.IsTrue(Launching.OpensWindow([]));
        Assert.IsFalse(Launching.OpensWindow(["--sign-in"]));
        Assert.IsFalse(Launching.OpensWindow(["--updated"]));
        Assert.IsFalse(Launching.OpensWindow(["----AppNotificationActivated:", "-Embedding"]));
    }

    // COM gives a click's arguments only to the copy that registered for clicks.
    [TestMethod]
    public void AClickOnANotificationIsDroppedByASecondCopy()
    {
        string[] clicked = ["----AppNotificationActivated:", "-Embedding"];

        Assert.AreEqual(Start.Dropped, Launching.Decide(clicked, Release, null, () => null, () => false));
        Assert.AreEqual(Start.Run, Launching.Decide(clicked, Release, null, () => null, () => true));
    }

    [TestMethod]
    public void ALaunchCarriesTheLinkOnItsBuildsScheme()
    {
        Assert.AreEqual("pitboard://open?url=x", Launching.Link(["pitboard://open?url=x"], "pitboard"));
        Assert.IsNull(Launching.Link(["--updated", "pitboard://open?url=x"], "pitboard"), "a link after another argument");
        Assert.AreEqual("PITBOARD://open?url=x", Launching.Link(["PITBOARD://open?url=x"], "pitboard"), "a scheme in another case");
        Assert.IsNull(Launching.Link([Shared], "pitboard"), "the Debug build's");
        Assert.IsNull(Launching.Link(["https://claude.ai/new"], "pitboard"), "a site's own");
        Assert.IsNull(Launching.Link([], "pitboard"));
    }

    [TestMethod]
    public void TheAppStartsWithTheLinkOnItsBuildsScheme()
    {
        Assert.AreEqual(Shared, new AppStart(Debug, new(), "oneTool", Fixture) { Arguments = [Shared] }.Link);
        Assert.IsNull(new AppStart(Release, new(), null, @"C:\Local\Pitboard App") { Arguments = [Shared] }.Link);
        Assert.IsNull(new AppStart(Release, new(), null, @"C:\Local\Pitboard App").Link, "a launch with no arguments");
    }

    // The installer puts the command line in bin beside Pitboard.exe: daily renewal runs it, and In your terminal calls it the app's own.
    [TestMethod]
    public void TheAppsLaunchNamesItsPitboardExeAndSoTheCommandLineInstalledBesideIt()
    {
        const string Exe = @"C:\Users\x\AppData\Local\Programs\Pitboard\Pitboard.exe";
        var environment = new Dictionary<string, string>
        {
            ["USERPROFILE"] = @"C:\Users\x",
            ["LOCALAPPDATA"] = @"C:\Users\x\AppData\Local",
        };

        var launch = new AppStart(Release, environment, null, @"C:\Users\x\AppData\Local\Pitboard App").Launch(Exe);

        Assert.AreEqual(Exe, launch.AppLocation);
        Assert.AreEqual(
            @"C:\Users\x\AppData\Local\Programs\Pitboard\bin\pitboard.exe", PitboardModelMethods.AppCommandLine(launch.AppLocation!));
        Assert.AreEqual(@"C:\Users\x\AppData\Local\Pitboard App", launch.AccountWindows?.Directory);
        Assert.AreEqual("pitboard", launch.AccountWindows?.LinkScheme);
        Assert.IsNull(launch.AccountWindows?.Key, "the model keys the Windows app's records");
    }

    // Nothing registers pitboard-debug, so only a person or a test hands a Debug build one, which outside a fixture
    // would open on whatever homes that launch has.
    [TestMethod]
    public void ADebugBuildOutsideAFixtureRefusesALinkAndTakesNoKey()
    {
        Assert.AreEqual(Start.LinkRefused, Launching.Decide([Shared], Debug, null, () => null, Unasked<bool?>("the key")));
        Assert.AreEqual(Start.Run, Launching.Decide([Shared], Debug, Fixture, () => null, () => true));
        Assert.AreEqual(Start.Run, Launching.Decide([], Debug, null, () => null, () => true), "a launch with no link");
        Assert.AreEqual(
            Start.Run, Launching.Decide(["pitboard://open?url=x"], Release, null, () => null, () => true), "a Release build's");
    }

    [TestMethod]
    public void ASecondCopyHandsItsLinkOver()
    {
        Assert.AreEqual(Start.Redirect, Launching.Decide([Shared], Debug, Fixture, () => null, () => false));
        Assert.AreEqual(Start.Redirect, Launching.Decide(["pitboard://open?url=x"], Release, null, () => null, () => false));
    }

    // Microsoft's guide to URI schemes: a link with unencoded quotes or spaces reaches Pitboard split into more
    // arguments, so nothing after a link is read as a flag.
    [TestMethod]
    public void ALinkLaunchReadsNoFlagAfterItsLink()
    {
        string[] split = ["pitboard://open?x=", "--unregister", ""];

        Assert.AreEqual(Start.Run, Launching.Decide(split, Release, null, () => null, () => true));
        Assert.AreEqual(Start.Redirect, Launching.Decide(split, Release, null, () => null, () => false));
        Assert.AreEqual(Start.Redirect, Launching.Decide(["pitboard://open?x=", "--updated"], Release, null, () => null, () => false));
        Assert.AreEqual(
            new Handover.Link("pitboard://open?x="),
            Launching.Handed(@"""C:\Pitboard.exe"" ""pitboard://open?x="" --unregister """"", Release, null));
        Assert.AreEqual(
            new Handover.Window(), Launching.Handed(@"""C:\Pitboard.exe"" --updated pitboard://open?url=x", Release, null),
            "a link after a flag");
    }

    // Windows App SDK hands the running copy an unpackaged launch's whole command line, its program first, as
    // GetCommandLine has it.
    [TestMethod]
    public void TheRunningCopyReadsTheLinkFromTheCommandLineHandedOver()
    {
        Assert.AreEqual(
            new Handover.Link(Shared), Launching.Handed($@"""C:\Program Files\Pitboard\Pitboard.exe"" ""{Shared}""", Debug, Fixture));
        Assert.AreEqual(new Handover.Link(Shared), Launching.Handed($@"C:\pb\Pitboard.exe {Shared}", Debug, Fixture));
        Assert.AreEqual(
            new Handover.Link("pitboard://open?url=x"),
            Launching.Handed(@"""C:\Pitboard.exe"" ""pitboard://open?url=x""", Release, null));
        Assert.AreEqual(
            new Handover.Window(), Launching.Handed(@"""C:\Program Files\Pitboard\Pitboard.exe""", Release, null),
            "a launch with no link");
        Assert.AreEqual(new Handover.Window(), Launching.Handed($@"""C:\pb\Pitboard.exe"" {Shared}", Debug, null), "outside a fixture");
    }

    // Each task the model gives starts Pitboard.exe with arguments that read back as the same ask.
    [TestMethod]
    public void AJumpListTaskReadsBackAsItsAsk()
    {
        JumpAsk[] asks = [new JumpAsk.Switch(Account), new JumpAsk.AddAccount(), new JumpAsk.OpenWindow(Account)];

        foreach (var ask in asks)
        {
            Assert.AreEqual(ask, Launching.Asked(Launching.Of(ask)));
        }
    }

    // Read as strictly as a link: a flag alone, or with the one GUID the model gave it.
    [TestMethod]
    public void ATaskIsItsFlagAndAGuidAndNothingElse()
    {
        Assert.AreEqual(new JumpAsk.Switch(Account), Launching.Asked(["--SWITCH-TO", Account.ToUpperInvariant()]), "in another case");
        Assert.IsNull(Launching.Asked(["--switch-to"]), "no account");
        Assert.IsNull(Launching.Asked(["--switch-to", "claude/work"]), "a label");
        Assert.IsNull(Launching.Asked(["--switch-to", Account, "--add-account"]), "more after it");
        Assert.IsNull(Launching.Asked(["--add-account", Account]), "an id it takes none of");
        Assert.IsNull(Launching.Asked(["pitboard://open?x=", "--switch-to", Account]), "after a link");
        Assert.IsNull(Launching.Asked([]));
    }

    // What a task asks for opens, and Pitboard's window only where that is the window.
    [TestMethod]
    public void ATaskOpensNoWindowOfItsOwn()
    {
        Assert.IsFalse(Launching.OpensWindow(["--switch-to", Account]));
        Assert.IsFalse(Launching.OpensWindow(["--add-account"]));
        Assert.IsTrue(Launching.OpensWindow(["--switch-to"]), "no task");
    }

    // Explorer starts a second Pitboard.exe for a task while one runs, which hands the task over as it hands a link.
    [TestMethod]
    public void ASecondCopyHandsItsTaskOverAndTheRunningCopyReadsIt()
    {
        Assert.AreEqual(Start.Redirect, Launching.Decide(["--switch-to", Account], Release, null, () => null, () => false));
        Assert.AreEqual(Start.Run, Launching.Decide(["--switch-to", Account], Release, null, () => null, () => true));
        Assert.AreEqual(
            new Handover.Jump(new JumpAsk.Switch(Account)),
            Launching.Handed($@"""C:\Program Files\Pitboard\Pitboard.exe"" --switch-to {Account}", Release, null));
        Assert.AreEqual(
            new Handover.Jump(new JumpAsk.AddAccount()), Launching.Handed(@"""C:\Pitboard.exe"" --add-account", Release, null));
        Assert.AreEqual(
            new Handover.Window(), Launching.Handed(@"""C:\Pitboard.exe"" --switch-to claude/work", Release, null), "no task");
    }

    [TestMethod]
    public void ADebugBuildOutsideAFixtureTakesNoTask()
    {
        const string Launch = @"""C:\pb\Pitboard.exe"" --add-account";

        Assert.AreEqual(new Handover.Window(), Launching.Handed(Launch, Debug, null));
        Assert.AreEqual(new Handover.Jump(new JumpAsk.AddAccount()), Launching.Handed(Launch, Debug, Fixture));
        Assert.IsNull(new AppStart(Debug, new(), null, @"C:\Local\Pitboard App Debug") { Arguments = ["--add-account"] }.Asked);
        Assert.AreEqual(
            new JumpAsk.AddAccount(), new AppStart(Debug, new(), "oneTool", Fixture) { Arguments = ["--add-account"] }.Asked);
        Assert.AreEqual(
            new JumpAsk.AddAccount(), new AppStart(Release, new(), null, @"C:\Local\Pitboard App") { Arguments = ["--add-account"] }.Asked);
    }
}
