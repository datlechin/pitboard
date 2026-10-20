namespace Pitboard.Core.Tests;

[TestClass]
public sealed class BindingsTests
{
    private static readonly string[] Codes = ["claude", "codex"];

    // The first call checks every checksum against the library, so one that answers proves they agree.
    [TestMethod]
    public void TheBindingsLoadTheCoreAndAgreeWithIt()
    {
        var sites = PitboardFfiMethods.Sites();

        CollectionAssert.AreEqual(Codes, sites.Select(site => site.Provider).ToArray());
    }

    [TestMethod]
    public void TheBindingsOfferTheModelAndNothingOlder()
    {
        var exported = typeof(PitboardModel).Assembly.GetExportedTypes().Where(type => !type.IsNested);
        // uniffi-bindgen-cs writes an exported object as a class with an interface named I and its name.
        var objects = exported
            .Where(type => type.IsClass && type.GetInterface("I" + type.Name) is not null)
            .Select(type => type.Name)
            .Order(StringComparer.Ordinal);
        string[] older =
        [
            "Pitboard", "SignIn", "Settings", "PitboardException", "Cause", "Switched", "Switch",
            "Adoption", "Enrolled", "EnrolledAs", "Changed", "Holding", "Remedy", "Diagnosis", "Change",
            "SignInView", "Check", "Renewed",
        ];
        var stillOffered = exported.Select(type => type.Name).Intersect(older).Order(StringComparer.Ordinal);
        string[] olderCalls =
        [
            "WindowsOf", "ForgetMessage", "Tools", "CommandLinePlaces", "SignInView", "FindCommandLine",
            "HomeDirectory", "LimitColumn", "LimitName", "UsageLevel", "Resets", "Runway", "ParkedLife",
            "RenewalNote", "DoctorSummary", "SameReset", "PitboardLink", "LinkRefusalReason",
        ];
        var stillCalled = typeof(PitboardFfiMethods).GetMethods()
            .Select(method => method.Name)
            .Intersect(olderCalls)
            .Order(StringComparer.Ordinal);

        Assert.AreEqual(nameof(PitboardModel), string.Join(", ", objects));
        Assert.AreEqual("", string.Join(", ", stillOffered));
        Assert.AreEqual("", string.Join(", ", stillCalled));
    }

    [TestMethod]
    public void TheAppAsksTheCoreWhereThingsAreAndWhatRuns()
    {
        Assert.IsNull(PitboardFfiMethods.AppCommandLine("/home/x/pitboard"));
        Assert.IsFalse(PitboardFfiMethods.CanRun("/nowhere/at/all"));
    }

    // The home is USERPROFILE on Windows, whatever HOME says, and HOME elsewhere.
    [TestMethod]
    public void TheAppsDirectoryIsTheCoresDefaultInItsHome()
    {
        var windows = OperatingSystem.IsWindows();
        var environment = new Dictionary<string, string>
        {
            ["HOME"] = windows ? "/c/Users/elsewhere" : "/home/x",
            ["USERPROFILE"] = windows ? @"C:\Users\x" : "/elsewhere",
        };

        Assert.AreEqual(
            windows ? @"C:\Users\x\AppData\Local\Pitboard" : "/home/x/.pitboard",
            PitboardFfiMethods.PitboardDirectory(environment));
        environment["PITBOARD_HOME"] = "/elsewhere/./p/";
        Assert.AreEqual("/elsewhere/./p/", PitboardFfiMethods.PitboardDirectory(environment));
    }

    // The core's own tests take a task away on a scheduler in memory; this one reaches none.
    [TestMethod]
    public void APathThatIsNoAppTakesNoRenewalTaskAway()
    {
        var home = Path.Combine(Path.GetTempPath(), $"pitboard-unschedule-{Guid.NewGuid():N}");
        var environment = new Dictionary<string, string>
        {
            ["HOME"] = home,
            ["USERPROFILE"] = home,
            ["SHELL"] = "/nowhere/at/all/sh",
        };

        Assert.IsFalse(PitboardFfiMethods.UnscheduleApp(environment, Path.Combine(home, "Pitboard")));
        Assert.IsFalse(Directory.Exists(home), "nothing is made");
    }

    [TestMethod]
    public void TheWindowsAppKeysItsRecordsByItsDirectoryInAnyCase()
    {
        var key = PitboardFfiMethods.WindowsRecordsKey(@"c:\users\x\appdata\local\pitboard\");

        Assert.AreEqual(@"C:\USERS\X\APPDATA\LOCAL\PITBOARD", key);
        Assert.AreEqual(key, PitboardFfiMethods.WindowsRecordsKey(@"\\?\C:\Users\X\AppData\Local\Pitboard"));
    }
}
