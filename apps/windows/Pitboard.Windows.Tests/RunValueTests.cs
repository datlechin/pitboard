using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class RunValueTests
{
    private const string Installed = @"C:\Users\Dana Lee\AppData\Local\Programs\Pitboard\Pitboard.exe";

    // A path with a space is quoted, or Windows would start C:\Users\Dana with the rest as its arguments.
    [TestMethod]
    public void TheReleaseBuildNamesItsOwnPitboardExeQuotedAndStartsWithoutItsWindow()
    {
        var key = new StandInRunKey();
        var item = new RunValue(Build.Release, Installed, key);

        item.Write(starts: true);

        Assert.AreEqual($"\"{Installed}\" --sign-in", key.Values["Pitboard"]);
        Assert.IsTrue(item.Read());
    }

    [TestMethod]
    public void ItIsOnOnlyWhileTheValueNamesThisPitboardExe()
    {
        var key = new StandInRunKey();
        var item = new RunValue(Build.Release, Installed, key);

        key.Values["Pitboard"] = @"""C:\Old\Pitboard.exe"" --sign-in";
        Assert.IsFalse(item.Read());

        key.Values["Pitboard"] = $"\"{Installed}\"";
        Assert.IsFalse(item.Read(), "it would open the window at every sign-in");

        key.Values["Pitboard"] = $"\"{Installed.ToUpperInvariant()}\" --sign-in";
        Assert.IsTrue(item.Read(), "Windows compares paths without case");
    }

    [TestMethod]
    public void TurningItOffRemovesOnlyAValueNamingThisPitboardExe()
    {
        var key = new StandInRunKey();
        var item = new RunValue(Build.Release, Installed, key);
        key.Values["Pitboard"] = @"""C:\Other\Pitboard.exe""";

        item.Write(starts: false);
        Assert.AreEqual(@"""C:\Other\Pitboard.exe""", key.Values["Pitboard"]);

        item.Write(starts: true);
        item.Write(starts: false);
        Assert.IsFalse(key.Values.ContainsKey("Pitboard"));
    }

    // Settings > Apps > Startup turned Pitboard off on the session's PC (2026-10-18): StartupApproved's first byte 01,
    // the Run value kept, and the switch still showed on.
    [TestMethod]
    public void SettingsTurningItOffShowsItOffAndPitboardNeverWritesWhatSettingsKeeps()
    {
        var key = new StandInRunKey();
        var item = new RunValue(Build.Release, Installed, key);
        item.Write(starts: true);

        foreach (var off in new byte[] { 0x01, 0x03 })
        {
            key.Approvals["Pitboard"] = [off, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            Assert.IsFalse(item.Read(), $"{off}");
            Assert.IsTrue(item.TurnedOff(), $"{off}");
            Assert.ThrowsExactly<InvalidOperationException>(() => item.Write(starts: true));
        }

        key.Approvals["Pitboard"] = [0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        Assert.IsTrue(item.Read());
        Assert.IsFalse(item.TurnedOff());

        // Turned off in Settings with no Run value left, Settings lists nothing to turn on, so Pitboard writes one.
        key.Approvals["Pitboard"] = [0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        key.Values.Remove("Pitboard");
        Assert.IsFalse(item.TurnedOff());
        item.Write(starts: true);
        Assert.IsTrue(item.TurnedOff());
        Assert.IsFalse(item.Read());
        CollectionAssert.DoesNotContain(key.Asked, "write approval Pitboard");
    }

    [TestMethod]
    public void ADebugBuildRefusesAndNeverTouchesTheRunKey()
    {
        var key = new StandInRunKey();
        var item = new RunValue(Build.Debug, Installed, key);

        Assert.IsNotNull(item.Refusal);
        Assert.IsFalse(item.Read());
        Assert.ThrowsExactly<InvalidOperationException>(() => item.Write(starts: true));
        Assert.ThrowsExactly<InvalidOperationException>(() => item.Write(starts: false));
        Assert.IsEmpty(key.Asked);
    }

    [TestMethod]
    public void ACommandLineLongerThanTheRunKeyTakesIsRefused()
    {
        var key = new StandInRunKey();
        var item = new RunValue(Build.Release, @"C:\" + new string('a', 250) + @"\Pitboard.exe", key);

        Assert.ThrowsExactly<InvalidOperationException>(() => item.Write(starts: true));
        Assert.IsFalse(key.Values.ContainsKey("Pitboard"));
    }

    [TestMethod]
    public void StartAtSignInSaysWhyAChangeDidNotHappen()
    {
        var shown = new StartAtSignIn(new RunValue(Build.Debug, Installed, new StandInRunKey()));
        var told = 0;
        shown.Changed += (_, _) => told++;

        shown.Set(on: true);

        Assert.IsFalse(shown.On);
        Assert.AreEqual(shown.Refusal, shown.Failed);
        Assert.AreEqual(1, told);
    }

    [TestMethod]
    public void AFailedWriteIsSaidAndTheSwitchShowsWhatWindowsHas()
    {
        var key = new StandInRunKey { Refuses = new UnauthorizedAccessException("Access is denied.") };
        var shown = new StartAtSignIn(new RunValue(Build.Release, Installed, key));

        shown.Set(on: true);

        Assert.IsFalse(shown.On);
        Assert.AreEqual("Access is denied.", shown.Failed);

        key.Refuses = null;
        shown.Set(on: true);

        Assert.IsTrue(shown.On);
        Assert.IsNull(shown.Failed);
    }

    [TestMethod]
    public void AFixtureTurnsItOnAndOffWithNothingRegistered()
    {
        var shown = new StartAtSignIn(new FixtureStartupItem());

        shown.Set(on: true);
        Assert.IsTrue(shown.On);

        shown.Set(on: false);
        Assert.IsFalse(shown.On);
        Assert.IsNull(shown.Refusal);
    }

    // PITBOARD_FIXTURE_STARTUP=off stands for a Run value Settings > Apps > Startup turned off, which a UI test reaches.
    [TestMethod]
    public void AFixtureTurnedOffInSettingsIsHeldBackAsRunValueIs()
    {
        var shown = StartAtSignIn.For(
            new AppStart(new Identity(Build.Debug), new() { [FixtureStartupItem.Variable] = "off" }, "oneTool", @"C:\Temp\pitboard-fixture"),
            Installed);

        shown.Read();
        Assert.IsFalse(shown.On);
        Assert.IsTrue(shown.TurnedOff);

        shown.Set(on: true);
        Assert.IsFalse(shown.On);
        Assert.AreEqual(Words.Own.StartupTurnedOff, shown.Failed);

        shown.Set(on: false);
        Assert.IsTrue(shown.TurnedOff, "turning it off in Pitboard undid what Settings keeps");
        Assert.IsFalse(StartAtSignIn.For(new AppStart(new Identity(Build.Debug), new(), "oneTool", @"C:\Temp\pitboard-fixture"), Installed).TurnedOff);
    }

    // A Debug build's writer refuses before it reads the Run key, so asking it touches nothing.
    [TestMethod]
    public void AFixtureGetsAStandInAndADebugBuildWithoutOneItsRefusingWriter()
    {
        var debug = new Identity(Build.Debug);

        var fixture = StartAtSignIn.For(new AppStart(debug, new(), "oneTool", @"C:\Temp\pitboard-fixture"), Installed);
        var unfixed = StartAtSignIn.For(new AppStart(debug, new(), null, @"C:\Local\Pitboard App Debug"), Installed);

        Assert.IsNull(fixture.Refusal);
        Assert.AreEqual("A Debug build never starts when you sign in to Windows.", unfixed.Refusal);
    }
}
