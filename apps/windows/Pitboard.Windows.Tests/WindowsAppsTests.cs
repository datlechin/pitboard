using Pitboard.Core;
using Windows.Win32.Foundation;
using Windows.Win32.System.RestartManager;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class WindowsAppsTests
{
    private const string Family = "Vendor.App_0000000000000";

    private const string App = $"{Family}!App";

    private static readonly AppId TheApp = new AppId.WindowsAumid(App);

    private const uint RmForceShutdown = 0x1;

    private static readonly TimeSpan Patience = TimeSpan.FromSeconds(30);

    // ChatGPT as the process list shows it: its window, and a codex it started, which has none.
    private static readonly AppProcess[] ChatGptOpen =
    [
        new(Id: 20, Parent: 4, Family: Family, Aumid: App, OwnsAWindow: true),
        new(Id: 21, Parent: 20, Family: Family, Aumid: null, OwnsAWindow: false),
    ];

    private static RM_UNIQUE_PROCESS Unique(uint id) =>
        new() { dwProcessId = id, ProcessStartTime = new() { dwLowDateTime = (int)id * 7, dwHighDateTime = 1 } };

    private static WindowsApps Apps(IReadOnlyList<AppProcess> listed, StandInRestartManager restart) =>
        new(() => listed, id => Unique(id), restart, _ => { }, () => { });

    // A Store app's own process starts helpers that may have windows, and console children that have none.
    [TestMethod]
    public void RunningGivesTheAumidOfThePackagesWindowedProcessWhichReopenTakes()
    {
        var did = new List<string>();
        AppProcess[] listed =
        [
            new(Id: 10, Parent: 4, Family: null, Aumid: null, OwnsAWindow: true),
            new(Id: 21, Parent: 20, Family: Family, Aumid: null, OwnsAWindow: false),
            new(Id: 22, Parent: 20, Family: Family.ToUpperInvariant(), Aumid: $"{Family}!Helper", OwnsAWindow: true),
            new(Id: 20, Parent: 4, Family: Family.ToUpperInvariant(), Aumid: $"{Family}!Main", OwnsAWindow: true),
        ];
        var apps = new WindowsApps(
            () => listed, id => Unique(id), new StandInRestartManager(), aumid => did.Add($"activate {aumid}"), () => did.Add("Pitboard in front"));

        var location = apps.Running(TheApp);
        apps.Reopen(location!, front: true);
        apps.Reopen(location!, front: false);

        Assert.AreEqual($"{Family}!Main", location);
        CollectionAssert.AreEqual(
            new[] { $"activate {Family}!Main", $"activate {Family}!Main", "Pitboard in front" }, did);
        Assert.IsNull(apps.Running(new AppId.WindowsAumid("Other.App_1111111111111!App")));
        CollectionAssert.AreEqual(new uint[] { 21 }, apps.Windowless(App).ToArray());
    }

    // Its codex can outlive it, and the model waits for that through the process list.
    [TestMethod]
    public void AnAppWithOnlyWindowlessProcessesLeftIsNotRunning()
    {
        Assert.IsNull(Apps([ChatGptOpen[1]], new StandInRestartManager()).Running(TheApp));
    }

    [TestMethod]
    public void AnAppWhoseOwnIdCannotBeReadRunsAsTheCoreNamesIt()
    {
        AppProcess[] listed = [ChatGptOpen[0] with { Aumid = null }];

        Assert.AreEqual(App, Apps(listed, new StandInRestartManager()).Running(TheApp));
    }

    // Never its codex, which a shutdown would end as a console process, and never forced. RmShutdown returns only once the
    // app has ended or declined, so it waits on a thread of its own and the model's call returns at once.
    [TestMethod]
    public async Task QuittingRegistersOnlyTheWindowedProcessAndShutsDownWithoutForceOnAThreadOfItsOwn()
    {
        using var restart = new StandInRestartManager { Holds = true };
        var apps = Apps(ChatGptOpen, restart);
        var caller = Environment.CurrentManagedThreadId;

        var quitting = apps.Quit(App);
        Assert.IsTrue(restart.ShutdownAsked.Wait(Patience), "RmShutdown was never asked");
        Assert.IsFalse(quitting.IsCompleted, "the quit waited for RmShutdown's answer");
        restart.Answer.Set();
        var shut = await quitting.WaitAsync(Patience);

        Assert.AreEqual(0, shut);
        Assert.AreEqual("start, register 20 started 140, shutdown 0, end", string.Join(", ", restart.Asked));
        Assert.AreNotEqual(caller, restart.ShutdownThread);
        Assert.AreEqual(0u, restart.ShutdownFlags & RmForceShutdown);
    }

    // The model reads the answer from the process list: an app that declined still runs, and nothing fails.
    [TestMethod]
    [DataRow((uint)WIN32_ERROR.ERROR_SUCCESS)]
    [DataRow((uint)WIN32_ERROR.ERROR_FAIL_SHUTDOWN)]
    public async Task EitherAnswerEndsTheSessionAndOnlyTheProcessListTellsTheModel(uint answer)
    {
        using var asked = new StandInRestartManager { Answers = (WIN32_ERROR)answer };
        using var waited = new StandInRestartManager { Answers = (WIN32_ERROR)answer };
        var apps = Apps(ChatGptOpen, asked);

        apps.RequestQuit(TheApp);
        var shut = await Apps(ChatGptOpen, waited).Quit(App).WaitAsync(Patience);

        Assert.IsTrue(asked.Ended.Wait(Patience), "the session never ended");
        Assert.AreEqual("start, register 20 started 140, shutdown 0, end", string.Join(", ", asked.Asked));
        Assert.AreEqual(App, apps.Running(TheApp));
        Assert.AreEqual((int)answer, shut);
        CollectionAssert.AreEqual(asked.Asked, waited.Asked);
    }

    [TestMethod]
    public void ARegistrationRestartManagerRefusesEndsTheSessionAndFails()
    {
        using var restart = new StandInRestartManager { RegisterGives = WIN32_ERROR.ERROR_ACCESS_DENIED };
        var apps = Apps(ChatGptOpen, restart);

        var refused = Assert.ThrowsExactly<PlatformException.Failed>(() => apps.RequestQuit(TheApp));

        StringAssert.Contains(refused.Message, "register process 20 with Restart Manager");
        Assert.AreEqual("start, register 20 started 140, end", string.Join(", ", restart.Asked));
    }

    [TestMethod]
    public void ASessionRestartManagerRefusesFailsWithNothingToEnd()
    {
        using var restart = new StandInRestartManager { StartGives = WIN32_ERROR.ERROR_SESSION_CREDENTIAL_CONFLICT };
        var apps = Apps(ChatGptOpen, restart);

        var refused = Assert.ThrowsExactly<PlatformException.Failed>(() => apps.RequestQuit(TheApp));

        StringAssert.Contains(refused.Message, "start a Restart Manager session");
        Assert.AreEqual("start", string.Join(", ", restart.Asked));
    }

    // The core names a Mac app only on a Mac: nothing here runs one, and none is quit.
    [TestMethod]
    public void AMacAppNeverRunsHere()
    {
        using var restart = new StandInRestartManager();
        var apps = Apps(ChatGptOpen, restart);

        Assert.IsNull(apps.Running(new AppId.MacBundle(App)));
        apps.RequestQuit(new AppId.MacBundle(App));
        Assert.IsEmpty(restart.Asked);
    }

    [TestMethod]
    public async Task AnAppThatIsNotRunningOrHasJustEndedStartsNoSession()
    {
        using var restart = new StandInRestartManager();
        var ended = new WindowsApps(() => ChatGptOpen, _ => null, restart, _ => { }, () => { });

        Assert.AreEqual(0, await Apps([ChatGptOpen[1]], restart).Quit(App).WaitAsync(Patience));
        Assert.AreEqual(0, await ended.Quit(App).WaitAsync(Patience));
        Assert.IsEmpty(restart.Asked);
    }

    [TestMethod]
    public void TheProcessListSaysThisUnpackagedProcessOwnsAWindow()
    {
        var own = PackagedApps.Listed().Single(listed => listed.Id == (uint)Environment.ProcessId);

        Assert.IsNull(own.Family);
        Assert.IsNull(own.Aumid);
        Assert.IsTrue(own.OwnsAWindow);
    }

    [TestMethod]
    public void RestartManagerTellsThisProcessByWhenItStarted()
    {
        var own = PackagedApps.Unique((uint)Environment.ProcessId);

        Assert.AreEqual((uint)Environment.ProcessId, own?.dwProcessId);
        Assert.AreNotEqual(0, own?.ProcessStartTime.dwHighDateTime);
    }

    // Restart Manager's calls in memory: no test starts a session, which Restart Manager keeps under the user's HKCU.
    private sealed class StandInRestartManager : IRestartManager, IDisposable
    {
        private readonly Lock _lock = new();

        private readonly List<string> _asked = [];

        public bool Holds { get; init; }

        public WIN32_ERROR StartGives { get; init; } = WIN32_ERROR.ERROR_SUCCESS;

        public WIN32_ERROR RegisterGives { get; init; } = WIN32_ERROR.ERROR_SUCCESS;

        public WIN32_ERROR Answers { get; init; } = WIN32_ERROR.ERROR_SUCCESS;

        public ManualResetEventSlim ShutdownAsked { get; } = new();

        public ManualResetEventSlim Answer { get; } = new();

        public ManualResetEventSlim Ended { get; } = new();

        public int ShutdownThread { get; private set; }

        public uint ShutdownFlags { get; private set; }

        public List<string> Asked
        {
            get
            {
                lock (_lock)
                {
                    return [.. _asked];
                }
            }
        }

        public WIN32_ERROR StartSession(out uint session)
        {
            Add("start");
            session = 5;
            return StartGives;
        }

        public WIN32_ERROR Register(uint session, RM_UNIQUE_PROCESS process)
        {
            Assert.AreEqual(5u, session);
            Add($"register {process.dwProcessId} started {process.ProcessStartTime.dwLowDateTime}");
            return RegisterGives;
        }

        public WIN32_ERROR Shutdown(uint session, uint flags)
        {
            Add($"shutdown {flags}");
            ShutdownThread = Environment.CurrentManagedThreadId;
            ShutdownFlags = flags;
            ShutdownAsked.Set();
            if (Holds)
            {
                Answer.Wait(Patience);
            }

            return Answers;
        }

        public WIN32_ERROR EndSession(uint session)
        {
            Add("end");
            Ended.Set();
            return WIN32_ERROR.ERROR_SUCCESS;
        }

        public void Dispose()
        {
            ShutdownAsked.Dispose();
            Answer.Dispose();
            Ended.Dispose();
        }

        private void Add(string asked)
        {
            lock (_lock)
            {
                _asked.Add(asked);
            }
        }
    }
}
