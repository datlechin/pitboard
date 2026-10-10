using System.Globalization;

namespace Pitboard.Core.Tests;

/// <summary>
/// The app model's types, as the Windows app will hold them. One test makes a model and takes
/// its first snapshot without starting it: a model reads nothing until it is sent an intent,
/// and this one is given a fresh folder for every home it could read all the same. So the
/// launch, the listener, AppControl, Notifications, LocalTime and the snapshot cross the
/// library there. The library calls a listener written in C# from a fixture's model, which
/// may be started since it reads no machine, against a library built with the `fixture`
/// feature; against one built without, that test is skipped and says why. What the model
/// does is the Rust tests' to prove.
///
/// Loading the library compares the checksum of every export, the model's constructor and
/// methods, the listener's one method and each of AppControl's among them, and hands the
/// library each trait's table of calls. The rest proves the records, the intents, the
/// listener and AppControl have the shape the app's code is written against.
/// </summary>
[TestClass]
public sealed class ModelTests
{
    private const long At = 1_800_000_000;

    /// <summary>
    /// What an app's listener does: keeps each snapshot it is told of, and refuses one it
    /// cannot take in the one way the model takes a refusal.
    /// </summary>
    private sealed class Keeping : ModelListener
    {
        public List<Snapshot> Told { get; } = [];

        public void Changed(Snapshot snapshot)
        {
            if (snapshot.Revision == 0)
            {
                throw new PlatformException.Failed("the first snapshot is the app's to take");
            }

            Told.Add(snapshot);
        }
    }

    /// <summary>
    /// What an app's LocalTime does: says a moment as the person's clock does, here in UTC
    /// and 24 hours, and whether two moments fall on one of their days.
    /// </summary>
    private sealed class InUtc : LocalTime
    {
        private static DateTimeOffset At(long epoch) => DateTimeOffset.FromUnixTimeSeconds(epoch);

        public string Clock(long epoch, bool withWeekday) =>
            At(epoch).ToString(withWeekday ? "ddd HH:mm" : "HH:mm", CultureInfo.InvariantCulture);

        public bool SameDay(long first, long second) => At(first).Date == At(second).Date;

        public string DateAndTime(long epoch) =>
            At(epoch).ToString("yyyy-MM-dd HH:mm", CultureInfo.InvariantCulture);
    }

    /// <summary>
    /// What an app's Notifications does: posts what it is given, and throws what its system
    /// refuses as the one exception the model takes from it.
    /// </summary>
    private sealed class Posting : Notifications
    {
        public List<RunOutNotice> Posted { get; } = [];

        public bool Allowed { get; set; } = true;

        public void Post(RunOutNotice notice)
        {
            if (!Allowed)
            {
                throw new PlatformException.Failed("notifications are not allowed");
            }

            Posted.Add(notice);
        }
    }

    private static Snapshot Read(ulong revision)
    {
        var work = new Account(
            Id: "claude:work", Provider: "claude", Label: "work", Qualified: "claude/work",
            Unplaced: false, Email: "work@example.com", AccountId: "work", SignedIn: true,
            Switchable: false, Parked: null,
            Usage: new Usage(
                Source.Live, At,
                [new Limit("session", 18_000, null, 42.0, At + 3_600, null, true)],
                ListsEveryLimit: true),
            Stale: null, StaleExplanation: null, Plan: null);
        return new Snapshot(
            Revision: revision, Now: At, Reading: false, UpdatedAt: At,
            Status: new Status(At, [work], []), Warnings: [], ReadFailure: null, Stuck: false,
            Installed: [new Tool(Code: "claude", Name: "Claude Code", Program: "claude", Service: "Anthropic")],
            SwitchUnderWay: null, QuitQuestion: null,
            LastSwitches: [], Abandoned: null, Failure: null,
            WindowRequest: new WindowRequest(Serial: 0, Pane: null),
            SigningIn: null, Sheet: null, SheetFailure: null,
            MenuBar: new MenuBarText(NameAndUsage: "work 42%", Usage: "42%", Spoken: "Pitboard, work 42%"),
            Sections: [], ShowsTools: false, Notices: [],
            MenuNotices: new MenuNotices(Install: null, Switches: [], Others: null),
            Footing: new Footing.OnlyOne(Provider: "claude", Label: "work"), Setup: null,
            AccountsShown: new AccountsShown.List(), MenuAccountsNote: null,
            UpdatedMenu: "Updated 08:00", UpdatedWindow: "Updated 08:00", SheetText: null,
            SigningInText: null, StowText: null, QuitConfirmation: null, FailureAlert: null,
            Machine: Unread(),
            AccountWindows: NoWindows());
    }

    /// <summary>
    /// The account windows of a model that has none open and nothing to say of them.
    /// </summary>
    private static AccountWindowsShown NoWindows() => new(
        Accounts: [], Menus: [], Open: [],
        Waiting: new WindowWaiting(WindowTitle: "Account", Shown: new WaitingShown.Reading(Title: "Reading accounts…")),
        Closing: [], Deleting: [], Picker: null, Downloads: []);

    /// <summary>
    /// What a model shows of the machine before anything about it has been read.
    /// </summary>
    private static MachineShown Unread() => new(
        Schedule: new ScheduleShown(
            Schedule: null, On: false, Changing: false, Enabled: true, Runs: null, ScheduledIn: null, Note: null,
            Failed: null),
        Renewal: new RenewalShown(Renewing: false, Note: "Renew every parked login that is due."),
        Checks: new ChecksShown(Lines: [], Summary: null, Checking: false, Checked: null, Waiting: "Checking this Mac…"),
        Activity: new ActivityShown(
            Lines: [], Empty: new EmptyList(Title: "No Activity", Detail: "Pitboard lists every change it makes here.")),
        CommandLine: new CommandLineShown(
            Found: null, InTerminal: null, UpdateNote: null, OffersLink: false, CannotLink: null),
        AutoSwitch: new AutoSwitchShown(
            On: false, At: 95, Lowest: 50, Highest: 99, Enabled: true,
            AtLabel: "Switch when a limit reaches 95%", Note: "", Standing: null));

    /// <summary>
    /// What an app's AppControl does: says what runs, asks an app to quit, opens one again,
    /// and throws what its system refuses as the one exception the model takes from it.
    /// </summary>
    private sealed class StandInApps : AppControl
    {
        public HashSet<string> Open { get; } = ["OpenAI.ChatGPT"];

        public List<string> Asked { get; } = [];

        public string? Running(string app) =>
            Open.Contains(app) ? $@"C:\Program Files\{app}\{app}.exe" : null;

        public void RequestQuit(string app)
        {
            if (!Open.Remove(app))
            {
                throw new PlatformException.Failed($"{app} is not running");
            }

            Asked.Add($"quit {app}");
        }

        public void Reopen(string location) => Asked.Add($"open {location}");
    }

    [TestMethod]
    public void TheBindingsAgreeWithTheLibraryOnTheModel()
    {
        // The first call loads the library: every checksum is compared, and the listener's
        // calls are registered, before it answers. Saying what a sheet saves reads only what
        // it is given, so it answers at once.
        Assert.AreEqual("home", PitboardFfiMethods.NameToSave(new Sheet.Name("claude", "a@example.com"), " home "));
    }

    /// <summary>
    /// A model made from what the app was started with answers its first snapshot at once,
    /// before anything is read, and stops when told to. Every home it could read is a folder
    /// of its own, and it is never started, so nothing on this machine is read.
    /// </summary>
    [TestMethod]
    public void AModelAnswersItsFirstSnapshotBeforeItReadsAnything()
    {
        var home = Directory.CreateTempSubdirectory("pitboard-model-").FullName;
        try
        {
            var environment = new Dictionary<string, string>
            {
                ["HOME"] = home,
                ["USERPROFILE"] = home,
                ["CODEX_HOME"] = Path.Combine(home, ".codex"),
                ["CLAUDE_CONFIG_DIR"] = Path.Combine(home, ".claude"),
                ["PITBOARD_HOME"] = Path.Combine(home, ".pitboard"),
            };
            var keeping = new Keeping();
            var apps = new StandInApps();
            var posting = new Posting();
            using var model = new PitboardModel(new AppLaunch(environment, null), keeping, apps, posting, new InUtc());

            var first = model.Snapshot();
            model.Shutdown();

            Assert.AreEqual(0UL, first.Revision);
            Assert.IsFalse(first.Reading);
            Assert.IsNull(first.Status);
            Assert.IsNull(first.Installed);
            Assert.AreEqual(new Footing.Ready(), first.Footing);
            Assert.AreEqual(new AccountsShown.Reading("Reading accounts…"), first.AccountsShown);
            Assert.AreEqual("Pitboard", first.MenuBar.Spoken);
            Assert.AreEqual("Not read yet", first.UpdatedMenu);
            Assert.IsNull(first.Machine.Schedule.Schedule);
            Assert.IsFalse(first.Machine.Schedule.On);
            Assert.AreEqual("Renew every parked login that is due.", first.Machine.Renewal.Note);
            Assert.IsEmpty(first.Machine.Checks.Lines);
            Assert.StartsWith("Checking ", first.Machine.Checks.Waiting!);
            Assert.AreEqual("No Activity", first.Machine.Activity.Empty?.Title);
            Assert.IsNull(first.Machine.CommandLine.Found);
            Assert.AreEqual(0UL, model.Snapshot().Revision);
            Assert.IsEmpty(keeping.Told);
            Assert.IsEmpty(apps.Asked);
            Assert.IsEmpty(posting.Posted);
            Assert.IsEmpty(Directory.EnumerateFileSystemEntries(home));
        }
        finally
        {
            Directory.Delete(home, recursive: true);
        }
    }

    /// <summary>
    /// An intent is a variant an app sends, with what it carries, and two of the same are
    /// equal.
    /// </summary>
    [TestMethod]
    public void AnIntentIsAVariantWithWhatItCarries()
    {
        Intent[] intents = [new Intent.Start(), new Intent.Woke(), new Intent.Glanced(), new Intent.Refresh(Asked: true)];

        Assert.AreEqual<Intent>(new Intent.Refresh(true), intents[3]);
        Assert.AreNotEqual<Intent>(new Intent.Refresh(false), intents[3]);
        Assert.IsTrue(intents.OfType<Intent.Refresh>().Single().Asked);
        Assert.AreEqual(1, intents.OfType<Intent.Start>().Count());
    }

    /// <summary>
    /// A switch is asked for by the account's label with its tool, and so is the answer that
    /// lets Pitboard quit the app the question names, which says which question it answers;
    /// what a switch said is put away by its tool, and keeping the app open and giving up
    /// carry nothing.
    /// </summary>
    [TestMethod]
    public void AnIntentToSwitchCarriesWhatItIsAbout()
    {
        Intent[] intents =
        [
            new Intent.SwitchTo(Qualified: "codex/work"), new Intent.QuitAndSwitch(Qualified: "codex/work"),
            new Intent.KeepAppOpen(), new Intent.DismissSwitch(Provider: "codex"), new Intent.AbandonStuckSwitch(),
            new Intent.DismissAbandoned(),
        ];

        Assert.AreEqual<Intent>(new Intent.SwitchTo("codex/work"), intents[0]);
        Assert.AreNotEqual<Intent>(new Intent.SwitchTo("claude/work"), intents[0]);
        Assert.AreEqual("codex", intents.OfType<Intent.DismissSwitch>().Single().Provider);
        Assert.AreEqual("codex/work", intents.OfType<Intent.QuitAndSwitch>().Single().Qualified);
        Assert.AreNotEqual<Intent>(new Intent.QuitAndSwitch("codex/spare"), intents[1]);
        Assert.AreNotEqual<Intent>(new Intent.SwitchTo("codex/work"), intents[1]);
        Assert.AreNotEqual<Intent>(new Intent.KeepAppOpen(), intents[1]);
    }

    /// <summary>
    /// A snapshot carries what a switch said as records of their own: the switch under way and
    /// the question about quitting the app that holds its login, what each tool's last switch
    /// said, what giving up kept, a numbered failure, and the window asked for on a pane.
    /// </summary>
    [TestMethod]
    public void ASnapshotCarriesWhatASwitchSaid()
    {
        var stillRunning = new Warning(
            "sessions_still_running", "2 `codex` sessions are still running", Account: null, Held: null);
        var switched = new LastSwitch(
            Provider: "codex", To: "codex/work", FollowsAt: null,
            Restart: new RestartNeeded(Program: "codex", From: "personal"), Said: null, Warnings: [stillRunning]);
        var snapshot = Read(4) with
        {
            SwitchUnderWay = "codex/spare",
            QuitQuestion = new QuitQuestion(Qualified: "codex/spare", AppId: "com.openai.codex", Name: "ChatGPT"),
            LastSwitches = [switched],
            Abandoned = new Abandoned(From: "personal", To: "work", LoginsKept: 2),
            Failure = new Failure(
                Id: 2, Title: "Couldn’t switch to spare", Message: "ChatGPT is still open, so nothing has changed.",
                Code: null, Warnings: []),
            WindowRequest = new WindowRequest(Serial: 3, Pane: Pane.Accounts),
        };

        Assert.AreEqual("ChatGPT", snapshot.QuitQuestion?.Name);
        Assert.AreEqual("personal", snapshot.LastSwitches[0].Restart?.From);
        Assert.IsNull(snapshot.LastSwitches[0].FollowsAt);
        Assert.AreEqual("sessions_still_running", snapshot.LastSwitches[0].Warnings[0].Code);
        Assert.AreEqual(2U, snapshot.Abandoned?.LoginsKept);
        Assert.AreEqual(2UL, snapshot.Failure?.Id);
        Assert.IsNull(snapshot.Failure?.Code);
        Assert.AreEqual(Pane.Accounts, snapshot.WindowRequest.Pane);
        Assert.AreEqual(new WindowRequest(3, Pane.Accounts), snapshot.WindowRequest);
        Assert.AreEqual(new RestartNeeded("codex", "personal"), switched.Restart);
    }

    /// <summary>
    /// A sign-in is asked for by the tool and the name, a code is typed back as text, and
    /// cancelling and closing a sheet carry nothing.
    /// </summary>
    [TestMethod]
    public void AnIntentToSignInCarriesWhatItIsAbout()
    {
        Intent[] intents =
        [
            new Intent.SignIn(Provider: "claude", Name: "travel"), new Intent.PasteCode(Code: "the-code#the-state"),
            new Intent.CancelSignIn(), new Intent.PresentSheet(Sheet: new Sheet.Add(Provider: null)),
            new Intent.CloseSheet(),
        ];

        Assert.AreEqual<Intent>(new Intent.SignIn("claude", "travel"), intents[0]);
        Assert.AreNotEqual<Intent>(new Intent.SignIn("codex", "travel"), intents[0]);
        Assert.AreEqual("travel", intents.OfType<Intent.SignIn>().Single().Name);
        Assert.AreEqual("the-code#the-state", intents.OfType<Intent.PasteCode>().Single().Code);
        Assert.AreEqual<Intent>(new Intent.PresentSheet(new Sheet.Add(null)), intents[3]);
        Assert.AreNotEqual<Intent>(new Intent.PresentSheet(new Sheet.Add("codex")), intents[3]);
        Assert.AreEqual(1, intents.OfType<Intent.CancelSignIn>().Count());
    }

    /// <summary>
    /// The account windows cross as the Windows app's WebView2 code will hold them: where the
    /// records are kept and what the earlier store held, a window open with the page it loads
    /// and the button that clears its downloads, what a window waiting for its page says, a
    /// store to delete by its ask, the picker's accounts and whether Open answers, a download
    /// by its state, the question before quitting for as many downloads as the app has under
    /// way, and what the app says back. What each means is the Rust tests' to prove.
    /// </summary>
    [TestMethod]
    public void TheAccountWindowsCrossAsAWindowsAppWillHoldThem()
    {
        const string Store = "7e15c34f-69ec-55b4-9542-f1c1fe3d7085";
        var launch = new AppLaunch(
            new Dictionary<string, string>(), null,
            Windows: new WindowsLaunch(
                Directory: @"C:\Users\dana\AppData\Local\Pitboard", Key: @"C:\Users\dana\.pitboard",
                LinkScheme: "pitboard",
                Earlier: new EarlierWindowRecords(
                    Stores: new Dictionary<string, string[]> { [@"C:\Users\dana\.pitboard"] = [Store] },
                    Pages: new Dictionary<string, Dictionary<string, string>>())));
        Assert.AreEqual("pitboard", launch.Windows?.LinkScheme);
        Assert.IsNull(new AppLaunch(new Dictionary<string, string>(), null).Windows);

        var work = PitboardFfiMethods.WindowAccounts(
            [
                new Account(
                    Id: "claude:work", Provider: "claude", Label: "work", Qualified: "claude/work",
                    Unplaced: false, Email: "work@example.com",
                    AccountId: "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f", SignedIn: true, Switchable: false,
                    Parked: null, Usage: null, Stale: null, StaleExplanation: null, Plan: null),
            ])[0];
        var link = PitboardFfiMethods.SiteLink("https://claude.ai/chat/x");
        var windows = NoWindows() with
        {
            Accounts = [work],
            Open =
            [
                new OpenWindow(
                    Store, work, new PageLoad(Serial: 2, Url: "https://claude.ai/"), new WindowNoteKind.SignIn(),
                    ClearDownloads: new Choice("Clear", new Intent.ClearDownloads(Store), Enabled: false)),
            ],
            Waiting = new WindowWaiting(
                WindowTitle: "Account",
                Shown: new WaitingShown.ReadFailed(
                    Title: "Couldn’t Read Accounts", Detail: "could not read Pitboard's account list",
                    Retry: new Choice("Try Again", new Intent.Refresh(Asked: true), Enabled: true))),
            Deleting = [new StoreDeletion(Store: Store, Ask: 1)],
            Picker = new LinkPicker(
                Arrival: 1, Armed: true,
                Shown: new PickerShown.Choose(
                    Title: "Open this claude.ai link as:", Link: link, LinkText: "claude.ai/chat/x",
                    Accounts: [new PickerAccount(work, "work@example.com, window open", true)], Chosen: Store,
                    Open: "Open")),
            Downloads =
            [
                new DownloadShown(
                    Id: "1", Store: Store, Name: "notes.txt", State: new DownloadState.Running(File: @"C:\notes.txt"),
                    Said: null, Running: true),
            ],
        };
        var snapshot = Read(5) with { AccountWindows = windows };

        Assert.AreEqual(Store, snapshot.AccountWindows.Open[0].Store);
        Assert.AreEqual(2UL, snapshot.AccountWindows.Open[0].Load.Serial);
        Assert.AreEqual(1UL, snapshot.AccountWindows.Deleting[0].Ask);
        var choose = Assert.IsInstanceOfType<PickerShown.Choose>(snapshot.AccountWindows.Picker?.Shown);
        Assert.AreEqual(Store, choose.Chosen);
        Assert.IsTrue(choose.Accounts[0].Open);
        Assert.IsInstanceOfType<DownloadState.Running>(snapshot.AccountWindows.Downloads[0].State);
        Assert.IsInstanceOfType<Intent.ClearDownloads>(snapshot.AccountWindows.Open[0].ClearDownloads.Intent);
        Assert.IsFalse(snapshot.AccountWindows.Open[0].ClearDownloads.Enabled);
        var failed = Assert.IsInstanceOfType<WaitingShown.ReadFailed>(snapshot.AccountWindows.Waiting.Shown);
        Assert.AreEqual("Try Again", failed.Retry.Title);
        Assert.AreEqual("A download is in progress. Quit anyway?", PitboardFfiMethods.DownloadsQuitQuestion(1)?.Title);
        Assert.IsNull(PitboardFfiMethods.DownloadsQuitQuestion(0));

        Intent[] intents =
        [
            new Intent.WindowOpened(Store), new Intent.WindowClosed(Store),
            new Intent.PageShown(Store, "https://claude.ai/chat/1"), new Intent.WebsiteDataRemoved(Store),
            new Intent.StoreDeleted(Store), new Intent.StoreHeld(Store),
            new Intent.LinkArrived("pitboard://open?url=https%3A%2F%2Fclaude.ai%2F"),
            new Intent.OpenLink(Arrival: 1, Store: Store), new Intent.DismissLink(Arrival: 1),
            new Intent.DownloadStarted("1", Store, null), new Intent.DownloadSaving("1", @"C:\notes.txt"),
            new Intent.DownloadEnded("1", new DownloadEnd.Failed("The connection was lost.")),
            new Intent.ClearDownloads(Store),
        ];
        Assert.AreEqual<Intent>(new Intent.StoreHeld(Store), intents[5]);
        Assert.AreNotEqual<Intent>(new Intent.StoreDeleted(Store), intents[5]);
        Assert.AreEqual(1UL, intents.OfType<Intent.OpenLink>().Single().Arrival);
        Assert.IsInstanceOfType<DownloadEnd.Failed>(intents.OfType<Intent.DownloadEnded>().Single().End);
    }

    /// <summary>
    /// A sheet is a variant with what it is about, and two of the same are equal, which decides
    /// whether a sheet put up keeps what went wrong in the one up: the same one does, another
    /// does not.
    /// </summary>
    [TestMethod]
    public void ASheetIsAVariantWithWhatItIsAbout()
    {
        Sheet[] sheets =
        [
            new Sheet.Add(Provider: "codex"), new Sheet.SignInAgain(Provider: "claude", Label: "work"),
            new Sheet.Name(Provider: "codex", Email: "c@example.com"), new Sheet.Rename(Provider: "claude", Label: "home"),
        ];

        Assert.AreEqual<Sheet>(new Sheet.Add("codex"), sheets[0]);
        Assert.AreNotEqual<Sheet>(new Sheet.Add(null), sheets[0]);
        Assert.AreEqual("work", sheets.OfType<Sheet.SignInAgain>().Single().Label);
        Assert.AreEqual("c@example.com", sheets.OfType<Sheet.Name>().Single().Email);
        Assert.AreNotEqual<Sheet>(new Sheet.Rename("claude", "work"), sheets[3]);
    }

    /// <summary>
    /// A snapshot carries a sign-in under way, with what its tool said, the address to open and
    /// whether a code is wanted, and refused; the sheet over the window, and what went wrong in
    /// it; and what a sign-in to the account in use said, as its tool's last switch.
    /// </summary>
    [TestMethod]
    public void ASnapshotCarriesASignInAndItsSheet()
    {
        var running = new RunningSignIn(
            Id: 3, Provider: "claude", Name: "travel",
            Said: "Paste code here if prompted > Invalid code. Please make sure the full code was copied.\n",
            Url: "https://claude.com/cai/oauth/authorize?code=true", WantsCode: true, CodeRefused: true);
        var signedIn = new LastSwitch(
            Provider: "claude", To: "work", FollowsAt: null, Restart: null,
            Said: "Signed in to work again. Its new login is the one in use now.", Warnings: []);
        var snapshot = Read(5) with
        {
            SigningIn = running,
            Sheet = new Sheet.SignInAgain(Provider: "claude", Label: "work"),
            SheetFailure = new Failure(
                Id: 4, Title: "Couldn’t sign in to work", Message: "`claude` is not on this machine",
                Code: "claude_program_missing", Warnings: []),
            LastSwitches = [signedIn],
        };

        Assert.AreEqual(3UL, snapshot.SigningIn?.Id);
        Assert.IsTrue(snapshot.SigningIn?.WantsCode);
        Assert.IsTrue(snapshot.SigningIn?.CodeRefused);
        Assert.AreEqual("https://claude.com/cai/oauth/authorize?code=true", snapshot.SigningIn?.Url);
        Assert.AreEqual(running, snapshot.SigningIn);
        Assert.AreEqual<Sheet?>(new Sheet.SignInAgain("claude", "work"), snapshot.Sheet);
        Assert.AreEqual("claude_program_missing", snapshot.SheetFailure?.Code);
        Assert.AreEqual("Signed in to work again. Its new login is the one in use now.", snapshot.LastSwitches[0].Said);
        Assert.IsNull(Read(5).SigningIn);
    }

    /// <summary>
    /// AppControl is an interface the app implements over its system's own apps, by an app id
    /// of one string, and what its system refuses it throws as the one exception the model
    /// takes from it.
    /// </summary>
    [TestMethod]
    public void AppControlIsAnInterfaceTheAppImplements()
    {
        var stand = new StandInApps();
        AppControl apps = stand;

        var copy = apps.Running("OpenAI.ChatGPT");
        apps.RequestQuit("OpenAI.ChatGPT");
        apps.Reopen(copy!);
        var refused = Assert.ThrowsExactly<PlatformException.Failed>(() => apps.RequestQuit("OpenAI.ChatGPT"));

        Assert.AreEqual(@"C:\Program Files\OpenAI.ChatGPT\OpenAI.ChatGPT.exe", copy);
        Assert.IsNull(apps.Running("OpenAI.ChatGPT"));
        CollectionAssert.AreEqual(
            new[] { "quit OpenAI.ChatGPT", $"open {copy}" }, stand.Asked);
        Assert.AreEqual("OpenAI.ChatGPT is not running", refused.reason);
    }

    /// <summary>
    /// A snapshot carries the accounts as the core's records do, and what went wrong as a
    /// record of its own. Its lists are arrays, which a C# record compares by reference: two
    /// snapshots read alike are not equal, so the app goes by the revision and syncs each list
    /// by its rows' ids.
    /// </summary>
    [TestMethod]
    public void ASnapshotCarriesWhatWasRead()
    {
        var snapshot = Read(3);
        var failed = snapshot with { ReadFailure = new ReadFailure("unreachable", "Anthropic could not be reached") };

        Assert.AreEqual("work", snapshot.Status?.Accounts[0].Label);
        Assert.AreEqual(42.0, snapshot.Status?.Accounts[0].Usage?.Windows[0].Percent);
        Assert.AreEqual("Claude Code", snapshot.Installed?[0].Name);
        Assert.AreEqual("unreachable", failed.ReadFailure?.Code);
        Assert.AreEqual(snapshot.Revision, failed.Revision);
        Assert.AreNotEqual(Read(3), Read(3));
    }

    /// <summary>
    /// The listener is an interface the app implements, and what it cannot take in it throws
    /// as the one exception the model takes from it.
    /// </summary>
    [TestMethod]
    public void AListenerIsAnInterfaceTheAppImplements()
    {
        var keeping = new Keeping();
        ModelListener listener = keeping;

        listener.Changed(Read(1));
        var refused = Assert.ThrowsExactly<PlatformException.Failed>(() => listener.Changed(Read(0)));

        Assert.AreEqual(1UL, keeping.Told.Single().Revision);
        Assert.AreEqual("the first snapshot is the app's to take", refused.reason);
    }

    /// <summary>
    /// What the app was started with goes in as it was given: the environment, the folder the
    /// app is in, and its preferences as an earlier store held them, which an app with none
    /// leaves out.
    /// </summary>
    [TestMethod]
    public void TheAppsLaunchIsItsEnvironmentAndWhereItIs()
    {
        var launch = new AppLaunch(
            new Dictionary<string, string> { ["USERPROFILE"] = @"C:\Users\x" }, @"C:\Program Files\Pitboard");
        var moved = launch with
        {
            EarlierPreferences = new EarlierPreferences(
                SecondAccountDeclined: ["codex"], HasBeenSeen: true, SecondAccountNudgeHidden: false),
        };

        Assert.AreEqual(@"C:\Users\x", launch.Environment["USERPROFILE"]);
        Assert.AreEqual(@"C:\Program Files\Pitboard", launch.AppLocation);
        Assert.IsNull(launch.EarlierPreferences);
        Assert.AreEqual("codex", moved.EarlierPreferences?.SecondAccountDeclined.Single());
        Assert.IsTrue(moved.EarlierPreferences?.HasBeenSeen);
    }

    /// <summary>
    /// The window is asked for by an intent, on a pane or none, and a failure it said is put
    /// away by one; naming, renaming, forgetting and declining a second account carry what
    /// they are about.
    /// </summary>
    [TestMethod]
    public void AnIntentForTheWindowOrAnAccountCarriesWhatItIsAbout()
    {
        Intent[] intents =
        [
            new Intent.ShowWindow(Pane: Pane.Machine), new Intent.ShowWindow(Pane: null), new Intent.DismissFailure(),
            new Intent.Enrol(Provider: "codex", Name: "job"),
            new Intent.Rename(Provider: "claude", Label: "work", To: "office"),
            new Intent.Forget(Qualified: "claude/spare"), new Intent.DeclineSecondAccount(Provider: "codex"),
        ];

        Assert.AreEqual<Intent>(new Intent.ShowWindow(Pane.Machine), intents[0]);
        Assert.AreNotEqual<Intent>(new Intent.ShowWindow(null), intents[0]);
        Assert.IsNull(((Intent.ShowWindow)intents[1]).Pane);
        Assert.AreEqual("job", intents.OfType<Intent.Enrol>().Single().Name);
        Assert.AreEqual("office", intents.OfType<Intent.Rename>().Single().To);
        Assert.AreEqual("claude/spare", intents.OfType<Intent.Forget>().Single().Qualified);
        Assert.AreEqual(1, intents.OfType<Intent.DismissFailure>().Count());
        Assert.AreEqual("codex", intents.OfType<Intent.DeclineSecondAccount>().Single().Provider);
    }

    /// <summary>
    /// A snapshot says what the menu bar, the menu and the window show as records of their
    /// own: an account's row with its limits, what pressing it sends, and what its own menu
    /// offers, held back where it cannot be chosen, with the windows it opens and the question
    /// asked before it is forgotten; a notice with what can be done about it and the question
    /// asked first, what the menu says of the notices, the footing and the step it asks for,
    /// and what the pane shows in place of a list, each button with whether it can be pressed.
    /// What pressing something sends is an Intent, which the app sends as it is.
    /// </summary>
    [TestMethod]
    public void ASnapshotSaysWhatTheMenuAndTheWindowShow()
    {
        var limit = new LimitRow(
            Name: "5-hour", Short: "5h", Percent: 72.4, Figure: "72%", Level: UsageLevel.Low,
            Resets: "resets in 1h 05m", Pace: null,
            Spoken: "5-hour limit, 72 percent used, resets in 1 hour, 5 minutes");
        var use = new ItemAction(Title: "Use", Spoken: "Use spare (Codex)", Intent: new Intent.SwitchTo("codex/spare"));
        var window = PitboardFfiMethods.WindowAccounts(
            [
                new Account(
                    Id: "codex:spare", Provider: "codex", Label: "spare", Qualified: "codex/spare",
                    Unplaced: false, Email: "spare@example.com", AccountId: "spare", SignedIn: false,
                    Switchable: true, Parked: null, Usage: null, Stale: null, StaleExplanation: null, Plan: null),
            ])[0];
        var spare = new AccountItem(
            Id: "codex:spare", Provider: "codex", Qualified: "codex/spare", Title: "spare",
            Email: "spare@example.com", Plan: null, SpokenName: "spare (Codex)", Spoken: "spare (Codex)", InUse: false,
            NeedsSignIn: false, Unplaced: false, Switching: false, Action: use,
            Summary: "5-hour 72%", Problem: null, StaleNote: null, Pace: null,
            ParkedNote: "Parked login good for 3 more days", Help: null, Limits: [limit],
            Offers:
            [
                new ItemOffer(Title: "Use spare", Intent: use.Intent, Enabled: true, Confirm: null),
                new ItemOffer(
                    Title: "Sign In Again…", Intent: new Intent.PresentSheet(new Sheet.SignInAgain("codex", "spare")),
                    Enabled: false, Confirm: null),
                new ItemOffer(
                    Title: "Rename…", Intent: new Intent.PresentSheet(new Sheet.Rename("codex", "spare")),
                    Enabled: true, Confirm: null),
            ],
            Windows: [new WindowOffer(Title: "Open chatgpt.com", Window: window)],
            Forget: new ItemOffer(
                Title: "Forget…", Intent: new Intent.Forget("codex/spare"), Enabled: true,
                Confirm: new Question(
                    Title: "Forget “spare (Codex)”?",
                    Message: "Pitboard deletes the login it parked for this account.", Confirm: "Forget")));
        var giveUp = new NoticeAction(
            Title: "Give Up…", Intent: new Intent.AbandonStuckSwitch(), Dismisses: false, Switches: false,
            Enabled: true,
            Confirm: new Question(
                Title: "Give up on the interrupted switch?", Message: "Every login is kept, and nothing is deleted.",
                Confirm: "Give Up"));
        var stuck = new PanelNotice(
            Id: "stuck", Severity: Severity.Error, SpokenSeverity: "Problem", Title: "An interrupted switch is waiting",
            Lines: ["An interrupted switch can’t be finished until OpenAI answers."], Until: null, UntilLabel: null,
            Actions: [giveUp]);
        var others = new MenuEntry(
            Title: stuck.Title, Subtitle: "Show in Pitboard", Help: stuck.Lines[0], Severity: Severity.Error,
            Intent: new Intent.ShowWindow(Pane.Accounts), Link: null, Enabled: true);
        var snapshot = Read(6) with
        {
            Sections = [new AccountSection(Id: "codex", Heading: "Codex", Accounts: [spare])],
            ShowsTools = true,
            Notices = [stuck],
            MenuNotices = new MenuNotices(Install: null, Switches: [], Others: others),
            Setup = new SetupStep(
                Title: "Add a second Codex account", Detail: "job is the only Codex account Pitboard knows.",
                Actions: [new Choice("Add Account…", new Intent.PresentSheet(new Sheet.Add("codex")), Enabled: true)]),
            AccountsShown = new AccountsShown.ReadFailed(
                Title: "Couldn’t Read Accounts", Detail: "OpenAI could not be reached",
                Retry: new Choice("Try Again", new Intent.Refresh(true), Enabled: false)),
        };

        var row = snapshot.Sections[0].Accounts[0];
        Assert.AreEqual<Intent>(new Intent.SwitchTo("codex/spare"), row.Action?.Intent);
        Assert.AreEqual(UsageLevel.Low, row.Limits[0].Level);
        Assert.AreEqual<Intent>(row.Action!.Intent, row.Offers[0].Intent);
        Assert.IsFalse(row.Offers[1].Enabled);
        Assert.AreEqual("chatgpt.com", row.Windows[0].Window.Site.Host);
        Assert.AreEqual<Intent?>(new Intent.Forget("codex/spare"), row.Forget?.Intent);
        Assert.AreEqual("Forget", row.Forget?.Confirm?.Confirm);
        Assert.IsFalse(((AccountsShown.ReadFailed)snapshot.AccountsShown).Retry.Enabled);
        Assert.IsTrue(snapshot.Notices[0].Severity > Severity.Warning);
        Assert.AreEqual<Intent>(new Intent.AbandonStuckSwitch(), snapshot.Notices[0].Actions[0].Intent);
        Assert.AreEqual("Give Up", snapshot.Notices[0].Actions[0].Confirm?.Confirm);
        Assert.AreEqual<Intent?>(new Intent.ShowWindow(Pane.Accounts), snapshot.MenuNotices.Others?.Intent);
        Assert.AreEqual(new Footing.OnlyOne("claude", "work"), snapshot.Footing);
        Assert.IsInstanceOfType<Intent.PresentSheet>(snapshot.Setup?.Actions[0].Intent);
        Assert.AreEqual("Try Again", ((AccountsShown.ReadFailed)snapshot.AccountsShown).Retry.Title);
        Assert.AreEqual("work 42%", snapshot.MenuBar.NameAndUsage);
    }

    /// <summary>
    /// What is about the machine is asked for by intents: a pane shown, the schedule read and
    /// turned on or off, a renewal now, and the command line looked for, each carrying what it
    /// is about, and two of the same equal.
    /// </summary>
    [TestMethod]
    public void AnIntentAboutTheMachineCarriesWhatItIsAbout()
    {
        Intent[] intents =
        [
            new Intent.PaneShown(Pane: Pane.Machine), new Intent.PaneShown(Pane: Pane.Activity),
            new Intent.ReadSchedule(), new Intent.SetSchedule(On: true), new Intent.RenewNow(),
            new Intent.LookForCommandLine(),
        ];

        Assert.AreEqual<Intent>(new Intent.PaneShown(Pane.Machine), intents[0]);
        Assert.AreNotEqual<Intent>(new Intent.PaneShown(Pane.Machine), intents[1]);
        Assert.AreNotEqual<Intent>(new Intent.ShowWindow(Pane.Machine), intents[0]);
        Assert.IsTrue(intents.OfType<Intent.SetSchedule>().Single().On);
        Assert.AreNotEqual<Intent>(new Intent.SetSchedule(false), intents[3]);
        Assert.AreEqual(1, intents.OfType<Intent.ReadSchedule>().Count());
        Assert.AreEqual(1, intents.OfType<Intent.RenewNow>().Count());
        Assert.AreEqual(1, intents.OfType<Intent.LookForCommandLine>().Count());
    }

    /// <summary>
    /// A snapshot says what is known of the machine as records of its own: daily renewal with
    /// the core's schedule, Renew Now's note, doctor's checks with their place in the list,
    /// each level and its spoken word, each change in the activity log with its place in the
    /// list and what stands in for the list when empty, and the command line a terminal runs,
    /// the core's own record of which it found. Two records made with `[]` for each list
    /// compare equal, since every `[]` of one type is the same empty array: comparing those
    /// says nothing of lists.
    /// </summary>
    [TestMethod]
    public void ASnapshotSaysWhatIsKnownOfTheMachine()
    {
        var plist = "/Users/x/Library/LaunchAgents/com.usepitboard.renew.plist";
        var keychain = new CheckLine(
            Id: 0, Code: "keychain", Name: "Keychain", Level: Level.Fail, SpokenLevel: "Failed", Detail: "locked",
            Advice: "Unlock the login keychain.");
        var switched = new ActivityLine(
            Id: 0, Date: "Oct 5, 2026 at 2:05 PM", Change: "Switch", Account: "codex/spare",
            Result: "Nothing parked", Done: false, AskedBy: "Pitboard app");
        var machine = Unread() with
        {
            Schedule = new ScheduleShown(
                Schedule: new Schedule.Installed(Path: plist, EverySeconds: 86_400), On: true, Changing: false,
                Enabled: true, Runs: "Every day", ScheduledIn: plist, Note: null, Failed: null),
            Renewal = new RenewalShown(Renewing: false, Note: "Renewed one."),
            Checks = new ChecksShown(
                Lines: [keychain], Summary: "1 broken: do not switch accounts until fixed.", Checking: false,
                Checked: "Checked at 14:05", Waiting: null),
            Activity = new ActivityShown(Lines: [switched], Empty: null),
            CommandLine = new CommandLineShown(
                Found: new FoundCommandLine.Bundled(Path: "/usr/local/bin/pitboard"),
                InTerminal: "/usr/local/bin/pitboard",
                UpdateNote: "The one inside this app, so it updates with the app.", OffersLink: false,
                CannotLink: null),
        };
        var snapshot = Read(7) with { Machine = machine };

        Assert.AreEqual(plist, ((Schedule.Installed)snapshot.Machine.Schedule.Schedule!).Path);
        Assert.AreEqual(86_400U, ((Schedule.Installed)snapshot.Machine.Schedule.Schedule!).EverySeconds);
        Assert.AreEqual("Renewed one.", snapshot.Machine.Renewal.Note);
        Assert.AreEqual(Level.Fail, snapshot.Machine.Checks.Lines[0].Level);
        Assert.AreEqual("Failed", snapshot.Machine.Checks.Lines[0].SpokenLevel);
        Assert.AreEqual(0UL, snapshot.Machine.Checks.Lines[0].Id);
        Assert.AreEqual(keychain, snapshot.Machine.Checks.Lines[0]);
        Assert.AreEqual(0UL, snapshot.Machine.Activity.Lines[0].Id);
        Assert.IsFalse(snapshot.Machine.Activity.Lines[0].Done);
        Assert.IsNull(snapshot.Machine.Activity.Empty);
        Assert.AreEqual<FoundCommandLine?>(
            new FoundCommandLine.Bundled("/usr/local/bin/pitboard"), snapshot.Machine.CommandLine.Found);
        Assert.IsFalse(snapshot.Machine.CommandLine.OffersLink);
        Assert.AreNotEqual(
            machine, machine with { Checks = machine.Checks with { Lines = [keychain] } },
            "its lists are arrays, compared by reference");
        Assert.AreEqual(Unread(), Unread(), "every list made with [], so the same array");
        Assert.AreSame(Unread().Checks.Lines, Unread().Checks.Lines);
    }

    /// <summary>
    /// LocalTime is an interface the app implements over its system's own clock, which the
    /// model asks for each clock time it says.
    /// </summary>
    [TestMethod]
    public void LocalTimeIsAnInterfaceTheAppImplements()
    {
        LocalTime clock = new InUtc();

        Assert.AreEqual("08:00", clock.Clock(At, false));
        Assert.AreEqual("Fri 08:00", clock.Clock(At, true));
        Assert.IsTrue(clock.SameDay(At, At + 3_600));
        Assert.IsFalse(clock.SameDay(At, At + 86_400));
        Assert.AreEqual("2027-01-15 08:00", clock.DateAndTime(At));
    }

    /// <summary>
    /// A sheet's Save is offered by the library's own rule, which the model saves by: a name
    /// without the white space around it, and in a rename one the account does not have.
    /// </summary>
    [TestMethod]
    public void ANameIsSavedAsTheSheetOffersIt()
    {
        Assert.AreEqual("home", PitboardFfiMethods.NameToSave(new Sheet.Name("claude", "a@example.com"), "  home "));
        Assert.IsNull(PitboardFfiMethods.NameToSave(new Sheet.Name("claude", "a@example.com"), " \n"));
        Assert.IsNull(PitboardFfiMethods.NameToSave(new Sheet.Rename("claude", "work"), " work "));
        Assert.AreEqual("work", PitboardFfiMethods.NameToSave(new Sheet.SignInAgain("codex", "work"), ""));
    }

    /// <summary>
    /// Notifications is an interface the app implements over its system's own, which the
    /// model posts each run-out through once, with the account its Switch button switches
    /// to, and a refusal is the one exception the model takes from it.
    /// </summary>
    [TestMethod]
    public void NotificationsIsAnInterfaceTheAppImplements()
    {
        var posting = new Posting();
        Notifications notifications = posting;
        var ranOut = new RunOutNotice(
            Id: "claude/work/session/-7200", Title: "work has no 5-hour limit left", Subtitle: null,
            Body: "spare has 80% of its own left.", SwitchTo: "claude/spare");

        notifications.Post(ranOut);
        posting.Allowed = false;
        var refused = Assert.ThrowsExactly<PlatformException.Failed>(() => notifications.Post(ranOut));

        Assert.AreEqual(ranOut, posting.Posted.Single());
        var switchTo = posting.Posted[0].SwitchTo;
        Assert.IsNotNull(switchTo);
        Assert.AreEqual<Intent>(new Intent.SwitchTo("claude/spare"), new Intent.SwitchTo(switchTo));
        Assert.IsNull(posting.Posted[0].Subtitle);
        Assert.AreEqual("notifications are not allowed", refused.reason);
    }

    /// <summary>
    /// What an app's listener does with a model that runs: keeps each snapshot it is told of,
    /// and the thread it was told on, and lets a test wait for one.
    /// </summary>
    private sealed class Waiting : ModelListener
    {
        private readonly List<Snapshot> _told = [];

        public int? TellingThread { get; private set; }

        /// <summary>Whether it was told on a thread of .NET's thread pool.</summary>
        public bool? ToldOnThePool { get; private set; }

        public void Changed(Snapshot snapshot)
        {
            lock (_told)
            {
                TellingThread = Environment.CurrentManagedThreadId;
                ToldOnThePool = Thread.CurrentThread.IsThreadPoolThread;
                _told.Add(snapshot);
                Monitor.PulseAll(_told);
            }
        }

        /// <summary>The first snapshot told that <paramref name="done"/> takes, or null once
        /// <paramref name="patience"/> has passed without one.</summary>
        public Snapshot? Until(Func<Snapshot, bool> done, TimeSpan patience)
        {
            var until = DateTime.UtcNow + patience;
            lock (_told)
            {
                while (true)
                {
                    var found = _told.FirstOrDefault(done);
                    var left = until - DateTime.UtcNow;
                    if (found is not null || left <= TimeSpan.Zero)
                    {
                        return found;
                    }

                    Monitor.Wait(_told, left);
                }
            }
        }
    }

    /// <summary>
    /// What a library built without the `fixture` feature, as every one an app ships is, says
    /// of a test that needs a fixture.
    /// </summary>
    private const string NoFixtures =
        "This library was built without the `fixture` feature, so it has no fixture to start. " +
        "Build it with `cargo build -p pitboard-ffi --features fixture` and pass that to " +
        "PitboardNativeLibrary to run this test.";

    /// <summary>The accounts of the fixture <c>oneTool</c>, the one in use first.</summary>
    private static readonly string[] OneToolsAccounts = ["claude/work", "claude/personal"];

    /// <summary>
    /// The fixtures are named by the library, in the order a debug build reads them, and a
    /// name that is none of them is refused saying which there are; a library built without
    /// them names none, and refuses a fixture and its stand-in pages, naming the feature that
    /// has them. Neither refusal makes a model or a folder.
    /// </summary>
    [TestMethod]
    public void AFixtureIsNamedOrRefusedWithWhy()
    {
        var names = PitboardFfiMethods.FixtureNames();
        var listener = new Keeping();

        if (names.Length == 0)
        {
            var refused = Assert.ThrowsExactly<FixtureException.Unavailable>(
                () => PitboardModel.Fixture("twoTools", listener, new InUtc()));
            Assert.Contains("--features fixture", refused.reason);
            Assert.ThrowsExactly<FixtureException.Unavailable>(
                () => PitboardFfiMethods.FixturePage("pitboard-fixture://claude.ai/"));
            return;
        }

        string[] ordered =
        [
            "twoTools", "oneTool", "empty", "firstLaunch", "noClaudeCode", "unnamed", "onlyOne",
            "readFailure", "stuck", "chatGPTOpen",
        ];
        CollectionAssert.AreEqual(ordered, names);
        var unknown = Assert.ThrowsExactly<FixtureException.Unknown>(
            () => PitboardModel.Fixture("twoTool", listener, new InUtc()));
        Assert.Contains("twoTools, oneTool", unknown.reason);
        Assert.Contains("<title>claude.ai stand-in</title>", PitboardFfiMethods.FixturePage("pitboard-fixture://claude.ai/"));
        Assert.IsEmpty(listener.Told);
    }

    /// <summary>
    /// A fixture's model, made by name from a library built with the `fixture` feature, is
    /// started, reads the accounts of its world on the real core, and tells a listener
    /// written here of them from another thread than the test's, and not one of .NET's thread
    /// pool: the call crosses from Rust into C# on a thread of the library's. It reads
    /// nothing of this machine, so it may be started. It launches into a temporary directory
    /// of its own, which it removes once it is done, rather than the one an app's fixture is
    /// kept in, so it empties neither the world of an app launched into a fixture nor that of
    /// another run of these tests, as the Swift test of the same does. Against a library built
    /// without the feature there is no fixture, and the test says so rather than failing.
    /// </summary>
    [TestMethod]
    public void AFixturesModelStartsAndTellsAListenerOfItsAccounts()
    {
        if (PitboardFfiMethods.FixtureNames().Length == 0)
        {
            Assert.Inconclusive(NoFixtures);
        }

        var waiting = new Waiting();
        var own = Path.Combine(Path.GetTempPath(), $"pitboard-bindings-{Guid.NewGuid():N}");
        Snapshot? read;
        try
        {
            using var model = PitboardModel.FixtureIn("oneTool", own, waiting, new InUtc());
            model.Send(new Intent.Start());
            read = waiting.Until(
                snapshot => snapshot is { Reading: false, UpdatedAt: not null, Status.Accounts.Length: > 0 },
                TimeSpan.FromSeconds(30));
            model.Shutdown();
        }
        finally
        {
            if (Directory.Exists(own))
            {
                Directory.Delete(own, recursive: true);
            }
        }

        Assert.IsNotNull(read, "the fixture's model told of no accounts read");
        CollectionAssert.AreEqual(
            OneToolsAccounts, read.Status!.Accounts.Select(account => account.Qualified).ToArray());
        Assert.IsTrue(read.Status.Accounts[0].SignedIn);
        Assert.AreEqual("work", read.Sections.Single().Accounts[0].Title);
        Assert.IsNotNull(waiting.TellingThread);
        Assert.AreNotEqual(Environment.CurrentManagedThreadId, waiting.TellingThread);
        Assert.IsFalse(waiting.ToldOnThePool, "told on a thread of .NET's thread pool");
    }
}
