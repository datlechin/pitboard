using System.Net;
using System.Runtime.InteropServices;
using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class SparkleUpdatesTests
{
    private const string Key = "1Wb1S84vDjECMyyPtwI3DyqLtVqGNPdp8jhPzyejdzs=";

    private const string Offered = """
        <rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
          <channel>
            <item>
              <sparkle:version>1.0.1</sparkle:version>
              <sparkle:shortVersionString>1.0.1</sparkle:shortVersionString>
              <enclosure url="https://example.com/x64.msi" sparkle:os="windows-x64" sparkle:edSignature="c2ln"/>
            </item>
            <item>
              <sparkle:version>1.0.1</sparkle:version>
              <sparkle:shortVersionString>1.0.1</sparkle:shortVersionString>
              <enclosure url="https://example.com/arm64.msi" sparkle:os="windows-arm64" sparkle:edSignature="c2ln"/>
            </item>
          </channel>
        </rss>
        """;

    // releases/latest/download/appcast-windows.xml on 19 October 2026: a redirect to v0.15.0's, then 404.
    private static readonly Func<Task<string>> Missing =
        () => Task.FromException<string>(new HttpRequestException("Not Found", null, HttpStatusCode.NotFound));

    private readonly List<string> _folders = [];

    [TestCleanup]
    public void RemoveFolders()
    {
        foreach (var folder in _folders)
        {
            Directory.Delete(folder, recursive: true);
        }
    }

    private string DataFolder()
    {
        var folder = Directory.CreateTempSubdirectory("pitboard-updates-").FullName;
        _folders.Add(folder);
        return folder;
    }

    private IUpdates Made(
        StandInSparkle sparkle, Build build = Build.Release, string? key = Key, string version = "1.0.0",
        Func<Task<bool>>? mayQuit = null, Action? quit = null, string? folder = null, Func<Task<string>>? feed = null) =>
        SparkleUpdates.For(
            build, key, version, folder ?? DataFolder(), sparkle, action =>
            {
                action();
                return true;
            },
            mayQuit ?? (() => Task.FromResult(true)), quit ?? (() => { }), feed ?? (() => Task.FromResult(Offered)));

    private SparkleUpdates Started(
        StandInSparkle sparkle, string version = "1.0.0", Func<Task<bool>>? mayQuit = null, Action? quit = null)
    {
        var updates = (SparkleUpdates)Made(sparkle, version: version, mayQuit: mayQuit, quit: quit);
        updates.Start();
        return updates;
    }

    [TestMethod]
    public void ADebugBuildOrOneWithoutAKeyCannotUpdate()
    {
        foreach (var (build, key) in new (Build, string?)[] { (Build.Debug, Key), (Build.Release, null), (Build.Release, "") })
        {
            var sparkle = new StandInSparkle();
            var updates = Made(sparkle, build, key);

            Assert.IsFalse(updates.Available);
            Assert.AreEqual(PitboardModelMethods.NoUpdateKey(), updates.Refusal);
            Assert.IsNull(sparkle.Key, "WinSparkle was handed a key");
        }
    }

    // With no key WinSparkle would install updates nobody signed.
    [TestMethod]
    public void AKeyWinSparkleRefusesLeavesTheUpdaterOff()
    {
        var sparkle = new StandInSparkle { Takes = false };
        var updates = Made(sparkle);

        Assert.IsFalse(updates.Available);
        Assert.AreEqual(Words.Settings.UpdateKeyInvalid, updates.Refusal);
        Assert.IsFalse(sparkle.Started);
    }

    // WinSparkle.dll itself, which the build copies beside the tests: it sets nothing but its key in this process.
    [TestMethod]
    public void WinSparkleTakesOnlyTheBase64OfThirtyTwoBytes()
    {
        var updates = SparkleUpdates.For(Build.Release, "not a key", "1.0.0", DataFolder(), _ => true, () => Task.FromResult(true), () => { });

        Assert.AreEqual(Words.Settings.UpdateKeyInvalid, updates.Refusal);
        Assert.AreEqual(0, new WinSparkle().SetKey(Convert.ToBase64String(new byte[31])));
        Assert.AreEqual(1, new WinSparkle().SetKey(Key));
    }

    [TestMethod]
    public void AKeyItTakesStartsWinSparkleOnTheFeedOfFullReleasesWithThePlainVersion()
    {
        var sparkle = new StandInSparkle();
        var updates = Started(sparkle);

        Assert.IsTrue(updates.Available);
        Assert.IsNull(updates.Refusal);
        Assert.AreEqual(Key, sparkle.Key);
        Assert.AreEqual("https://github.com/datlechin/pitboard/releases/latest/download/appcast-windows.xml", sparkle.Feed);
        Assert.AreEqual("1.0.0", sparkle.Version);
        Assert.IsTrue(sparkle.Started);
        Assert.IsTrue(sparkle.ChecksAutomatically, "automatic checks start on, as on macOS");
    }

    // Once the person turned them off, a later start leaves them off.
    [TestMethod]
    public void AutomaticChecksStayAsThePersonLeftThem()
    {
        var folder = DataFolder();
        new UpdateSettings(Path.Combine(folder, "updates.json")).Write("CheckForUpdates", "0");
        var sparkle = new StandInSparkle();
        var updates = (SparkleUpdates)Made(sparkle, folder: folder);

        Assert.IsFalse(updates.ChecksAutomatically, "read before WinSparkle starts");
        updates.Start();

        Assert.IsFalse(sparkle.ChecksAutomatically);
        updates.ChecksAutomatically = true;
        Assert.IsTrue(sparkle.ChecksAutomatically);
    }

    // A beta keeps checking on its own, and finds 1.0.0 once it is out.
    [TestMethod]
    public void ABetaChecksOnItsOwn()
    {
        var sparkle = new StandInSparkle();
        var updates = Started(sparkle, version: "1.0.0-beta.1");

        Assert.IsTrue(updates.Beta);
        Assert.AreEqual("1.0.0-beta.1", sparkle.Version);
        Assert.IsTrue(sparkle.ChecksAutomatically);
        Assert.IsFalse(Started(new StandInSparkle()).Beta);
    }

    // Before 1.0.0 no full release carries a feed: WinSparkle's own checks failed, and Check now showed its error.
    [TestMethod]
    public void ABetaChecksNothingUntilAFullReleaseHasAFeed()
    {
        var feed = Missing;
        var sparkle = new StandInSparkle();
        var updates = Made(sparkle, version: "1.0.0-beta.4", feed: () => feed());
        var changes = 0;
        updates.Changed += (_, _) => changes++;

        updates.Start();
        updates.Check();

        Assert.IsFalse(sparkle.Started, "WinSparkle checks on its own");
        Assert.IsFalse(updates.Available);
        Assert.AreEqual(Words.Settings.NoReleaseToUpdateTo, updates.Refusal);
        Assert.IsFalse(updates.ChecksAutomatically);
        Assert.AreEqual(0, sparkle.Checks);
        Assert.AreEqual(1, changes);

        feed = () => Task.FromResult(Offered);
        updates.LookForFeed();

        Assert.IsTrue(sparkle.Started);
        Assert.IsTrue(updates.Available);
        Assert.IsNull(updates.Refusal);
        Assert.IsTrue(sparkle.ChecksAutomatically);
        Assert.AreEqual(2, changes);
    }

    // Offline, a beta cannot tell, so WinSparkle starts as it does for a full release and says what it finds.
    [TestMethod]
    public void ABetaThatCannotReadTheFeedStartsWinSparkle()
    {
        var sparkle = new StandInSparkle();
        var updates = Made(
            sparkle, version: "1.0.0-beta.4",
            feed: () => Task.FromException<string>(new HttpRequestException("No such host is known.")));

        updates.Start();

        Assert.IsTrue(sparkle.Started);
        Assert.IsTrue(updates.Available);
        Assert.IsNull(updates.Refusal);
    }

    [TestMethod]
    public void AFullReleaseNeverWaitsForTheFeed()
    {
        var sparkle = new StandInSparkle();
        var updates = Made(sparkle, feed: Missing);

        Assert.IsTrue(updates.Available, "before it starts, as the window may show first");
        updates.Start();
        updates.LookForFeed();

        Assert.IsTrue(sparkle.Started);
        Assert.IsNull(updates.Refusal);
    }

    // Pitboard ends while the read is out, and WinSparkle would start over an app that is quitting.
    [TestMethod]
    public void ABetaStoppedBeforeTheFeedAnswersNeverStartsWinSparkle()
    {
        var answer = new TaskCompletionSource<string>();
        var sparkle = new StandInSparkle();
        var updates = Made(sparkle, version: "1.0.0-beta.4", feed: () => answer.Task);

        updates.Start();
        updates.Shutdown();
        answer.SetResult(Offered);
        updates.LookForFeed();

        Assert.IsFalse(sparkle.Started);
        Assert.AreEqual(0, sparkle.Stops);
    }

    [TestMethod]
    public void AnUpdateFoundIsWaitingUnderTheVersionTheFeedOffersThisMachine()
    {
        var sparkle = new StandInSparkle();
        var updates = Started(sparkle);
        var changes = 0;
        updates.Changed += (_, _) => changes++;

        sparkle.Events!.Found();
        Assert.AreEqual("1.0.1", updates.Waiting);
        Assert.AreEqual(1, changes);
        Assert.AreEqual("1.0.1", UpdateFeed.Newest(Offered, UpdateFeed.Marker(RuntimeInformation.ProcessArchitecture)));

        sparkle.Events.Skipped();
        Assert.IsNull(updates.Waiting);
        sparkle.Events.Found();
        sparkle.Events.NotFound();
        Assert.IsNull(updates.Waiting);
        Assert.AreEqual(4, changes);
    }

    // As the macOS updater's check(): WinSparkle's own window says what it finds.
    [TestMethod]
    public void CheckingShowsWinSparklesWindowAfresh()
    {
        var sparkle = new StandInSparkle();
        var updates = Started(sparkle);
        sparkle.Events!.Found();

        updates.Check();

        Assert.AreEqual(1, sparkle.Checks);
        Assert.IsNull(updates.Waiting);
    }

    // The account windows' downloads stop as Pitboard quits for the installer, so WinSparkle asks first.
    [TestMethod]
    public void InstallingAsksAboutDownloadsThenQuits()
    {
        var answer = false;
        var quits = 0;
        var sparkle = new StandInSparkle();
        Started(sparkle, mayQuit: () => Task.FromResult(answer), quit: () => quits++);

        Assert.IsFalse(sparkle.Events!.MayShutDown());
        answer = true;
        Assert.IsTrue(sparkle.Events.MayShutDown());

        sparkle.Events.ShutDown();
        Assert.AreEqual(1, quits);
    }

    // WinSparkle's thread waits on the question, and Stop joins that thread.
    [TestMethod]
    public void StoppingAnswersAQuestionStillAskedNo()
    {
        var never = new TaskCompletionSource<bool>();
        using var asking = new ManualResetEventSlim();
        var sparkle = new StandInSparkle();
        var updates = Started(sparkle, mayQuit: () =>
        {
            asking.Set();
            return never.Task;
        });

        var asked = Task.Run(sparkle.Events!.MayShutDown);
        Assert.IsTrue(asking.Wait(TimeSpan.FromSeconds(10)), "the question was never asked");
        updates.Shutdown();
        updates.Shutdown();

        Assert.IsTrue(asked.Wait(TimeSpan.FromSeconds(10)));
        Assert.IsFalse(asked.Result);
        Assert.AreEqual(1, sparkle.Stops);
        Assert.IsFalse(sparkle.Events.MayShutDown(), "asked once Pitboard stopped");
    }

    [TestMethod]
    public void AnUpdaterNeverStartedIsNeverStopped()
    {
        var sparkle = new StandInSparkle();
        var updates = (SparkleUpdates)Made(sparkle);

        updates.Shutdown();
        updates.Check();

        Assert.AreEqual(0, sparkle.Stops);
        Assert.AreEqual(0, sparkle.Checks);
    }

    private sealed class StandInSparkle : ISparkle
    {
        public bool Takes { get; init; } = true;

        public string? Key { get; private set; }

        public string? Feed { get; private set; }

        public string? Version { get; private set; }

        public SparkleEvents? Events { get; private set; }

        public bool ChecksAutomatically
        {
            get => Settings?.Read("CheckForUpdates") == "1";
            set => Settings?.Write("CheckForUpdates", value ? "1" : "0");
        }

        public bool Started { get; private set; }

        public int Checks { get; private set; }

        public int Stops { get; private set; }

        private UpdateSettings? Settings { get; set; }

        public int SetKey(string key)
        {
            Key = key;
            return Takes ? 1 : 0;
        }

        public void Configure(string feed, string version, UpdateSettings settings, SparkleEvents events)
        {
            Feed = feed;
            Version = version;
            Settings = settings;
            Events = events;
        }

        public void Start() => Started = true;

        public void Check() => Checks++;

        public void Stop() => Stops++;
    }
}
