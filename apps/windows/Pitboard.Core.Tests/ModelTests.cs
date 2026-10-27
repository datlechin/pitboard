using System.Globalization;
using System.Reflection;
using System.Text.RegularExpressions;

namespace Pitboard.Core.Tests;

[TestClass]
public sealed class ModelTests
{
    private const long At = 1_800_000_000;

    private sealed class Keeping : ModelListener
    {
        public List<Snapshot> Told { get; } = [];

        public void Changed(Snapshot snapshot) => Told.Add(snapshot);
    }

    private sealed class InUtc : LocalTime
    {
        private static DateTimeOffset At(long epoch) => DateTimeOffset.FromUnixTimeSeconds(epoch);

        public string Clock(long epoch, bool withWeekday) =>
            At(epoch).ToString(withWeekday ? "ddd HH:mm" : "HH:mm", CultureInfo.InvariantCulture);

        public bool SameDay(long first, long second) => At(first).Date == At(second).Date;

        public string DateAndTime(long epoch) =>
            At(epoch).ToString("yyyy-MM-dd HH:mm", CultureInfo.InvariantCulture);

        public string TimeOfDay(ushort minutes) =>
            TimeSpan.FromMinutes(minutes).ToString(@"hh\:mm", CultureInfo.InvariantCulture);

        public string Date(long epoch) => At(epoch).ToString("MMM d", CultureInfo.InvariantCulture);
    }

    private sealed class Posting : Notifications
    {
        public Calls<RunOutNotice> Posted { get; } = new();

        // Why the system refuses each notification, where it does.
        public string? Refuses { get; init; }

        public void Post(RunOutNotice notice)
        {
            Posted.Add(notice);
            if (Refuses is { } reason)
            {
                throw new PlatformException.Failed(reason);
            }
        }
    }

    // Rust calls the app's interfaces on threads of its own, so a test waits for a call.
    private sealed class Calls<T>
    {
        private readonly List<T> _calls = [];

        public T[] All
        {
            get
            {
                lock (_calls)
                {
                    return [.. _calls];
                }
            }
        }

        public void Add(T call)
        {
            lock (_calls)
            {
                _calls.Add(call);
                Monitor.PulseAll(_calls);
            }
        }

        public T[] Until(int count, TimeSpan patience)
        {
            var until = DateTime.UtcNow + patience;
            lock (_calls)
            {
                for (var left = patience; _calls.Count < count && left > TimeSpan.Zero; left = until - DateTime.UtcNow)
                {
                    Monitor.Wait(_calls, left);
                }

                return [.. _calls];
            }
        }

        public T? First(Func<T, bool> matching, TimeSpan patience)
        {
            var until = DateTime.UtcNow + patience;
            lock (_calls)
            {
                for (var left = patience; !_calls.Any(matching) && left > TimeSpan.Zero; left = until - DateTime.UtcNow)
                {
                    Monitor.Wait(_calls, left);
                }

                return _calls.FirstOrDefault(matching);
            }
        }
    }

    private static Snapshot Read(ulong revision) =>
        new(
            Revision: revision, Reading: false, QuitQuestion: null, ReplaceQuestion: null,
            WindowRequest: new WindowRequest(Serial: 0, Place: null),
            SigningIn: null, Sheet: null, SheetFailure: null,
            MenuBar: new MenuBarText(
                NameAndUsage: "work 42%", Usage: "42%", Spoken: "Pitboard, work 42%", Level: UsageLevel.Plenty, Attention: false,
                Tip: "Pitboard\nwork: 5-hour 42%"),
            Sections: [], AccountMenu: new AccountMenuShown(Offers: ["Use", "Sign in again…", "Rename…"], Forget: "Forget…"),
            Notices: [],
            MenuNotices: new MenuNotices(Install: null, Switches: [], SignIns: [], Others: null),
            Footing: new Footing.OnlyOne(Provider: "claude", Label: "work"), Setup: null,
            AccountsShown: new AccountsShown.List(), MenuAccountsNote: null, UpdatedAt: 1_800_000_000,
            UpdatedMenu: "Updated 08:00", UpdatedWindow: "Updated 08:00", SheetText: null,
            SigningInText: null, StowText: null, FailureAlert: null,
            Machine: Unread(),
            AccountWindows: NoWindows(),
            Uninstall: new ItemOffer(Title: "Uninstall Pitboard", Intent: new Intent.Uninstall(), Enabled: true, Confirm: null),
            Removal: null,
            Welcome: new WelcomeShown(
                Title: "Welcome to Pitboard", Lines: [], IconLink: null, ToolsHeading: "Found on this PC", Tools: [],
                OpenAtLogin: new WelcomeLogin(Title: "Start when you sign in to Windows", On: false, Note: ""),
                Done: new Choice(Title: "Get started", Intent: new Intent.Show(Place: new Place.Accounts()), Enabled: true)),
            GlanceRevision: 0, Answers: []);

    private static AccountWindowsShown NoWindows() => new(
        Accounts: [], Menus: [], Open: [],
        Waiting: new WindowWaiting(WindowTitle: "Account", Shown: new WaitingShown.Reading(Title: "Reading accounts…")),
        Closing: [], Deleting: [], Picker: null, Downloads: [],
        NoLink: new EmptyList(
            Title: "No Link to Open",
            Detail: "Share a claude.ai or chatgpt.com page with Pitboard from your browser’s Share menu to open it as one of your accounts."));

    private static MachineShown Unread() => new(
        Schedule: new ScheduleShown(
            On: false, Enabled: true, Note: null, Failed: null,
            About: "A parked login is renewed whenever Pitboard runs, and otherwise not."),
        Checks: new ChecksShown(
            Groups: [], Summary: null, Verdict: null, Level: Level.Ok, Checking: false, Checked: null, Waiting: "Checking this Mac…",
            Report: null),
        Activity: new ActivityShown(
            Lines: [], Empty: new EmptyList(Title: "No Activity", Detail: "Pitboard lists every change it makes here.")),
        CommandLine: new CommandLineShown(
            Found: null, InTerminal: null, UpdateNote: null, OffersLink: false, CannotLink: null, TerminalNote: null),
        AtLimit: new AtLimitShown(
            Title: "Before an account runs out",
            Choice: AtLimit.Notify,
            Choices: AtLimitChoices,
            At: 95, Lowest: 50, Highest: 99, Enabled: true, ShareEnabled: true, Heading: "When a limit reaches",
            AtLabel: "When a limit reaches 95%",
            ShareLabel: "Share of a limit", ShareRange: "From 50% to 99%", Note: null, Standing: null, Terms: Terms),
        Warm: new WarmShown(
            Title: "Warm up limits", From: 360, To: 1320, RangeLabel: "06:00 to 22:00", Accounts: [],
            NoneEnrolled: "No Claude Code account is enrolled yet.", Standing: null, Note: "", Enabled: false));

    // One array, so two snapshots made here are equal.
    private static readonly AtLimitChoice[] AtLimitChoices =
    [
        new(AtLimit.Nothing, "Do nothing"), new(AtLimit.Notify, "Notify me"), new(AtLimit.Switch, "Switch automatically"),
    ];

    private static readonly TermsShown Terms = new(
        "Anthropic’s terms may cover automatic switching and warm-ups.", "Is this allowed?", "https://docs.usepitboard.com/concepts/limits#is-this-allowed");

    // ChatGPT, open, as the fixture chatGPTOpen's process list has it. On Windows its running copy is its AUMID.
    private sealed class StandInApps : AppControl
    {
        public static readonly string ChatGpt = OperatingSystem.IsWindows() ? "OpenAI.Codex_2p2nqsd0c76g0!App" : "com.openai.codex";

        public static readonly string ChatGptCopy = OperatingSystem.IsWindows() ? ChatGpt : "/Applications/ChatGPT.app";

        private bool _chatGptOpen = true;

        public Calls<string> Asked { get; } = new();

        public bool RefusesToQuit { get; init; }

        public string? Running(string app)
        {
            Asked.Add($"running {app}");
            return app == ChatGpt && _chatGptOpen ? ChatGptCopy : null;
        }

        public void RequestQuit(string app)
        {
            Asked.Add($"quit {app}");
            if (RefusesToQuit)
            {
                throw new PlatformException.Failed($"{app} would not quit");
            }

            if (app == ChatGpt)
            {
                _chatGptOpen = false;
            }
        }

        public void Reopen(string location, bool front)
        {
            Asked.Add(front ? $"open-front {location}" : $"open {location}");
            if (location == ChatGptCopy)
            {
                _chatGptOpen = true;
            }
        }
    }

    [TestMethod]
    public void TheBindingsAgreeWithTheLibraryOnTheModel()
    {
        // The first call loads the library: every checksum is compared, and the listener's
        // calls are registered, before it answers. Saying what a sheet sends reads only what
        // it is given, so it answers at once.
        Assert.AreEqual(
            new Intent.Rename("claude", "acme", "home"),
            PitboardFfiMethods.CheckName(new Sheet.Name("claude", "acme", "a@example.com"), "claude", " home ").Intent);
    }

    // Never started, so nothing on this machine is read.
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
            Assert.IsEmpty(first.Sections);
            Assert.AreEqual(new Footing.Ready(), first.Footing);
            Assert.IsInstanceOfType<AccountsShown.Reading>(first.AccountsShown);
            Assert.AreEqual("", first.MenuBar.NameAndUsage);
            Assert.AreEqual("", first.UpdatedWindow);
            Assert.IsFalse(first.Machine.Schedule.On);
            Assert.IsEmpty(first.Machine.Checks.Groups);
            Assert.IsNotNull(first.Machine.Checks.Waiting);
            Assert.IsNotNull(first.Machine.Activity.Empty);
            Assert.IsNull(first.Machine.CommandLine.Found);
            Assert.AreEqual(0UL, model.Snapshot().Revision);
            Assert.IsEmpty(keeping.Told);
            Assert.IsEmpty(apps.Asked.All);
            Assert.IsEmpty(posting.Posted.All);
            Assert.IsEmpty(Directory.EnumerateFileSystemEntries(home));
        }
        finally
        {
            Directory.Delete(home, recursive: true);
        }
    }

    [TestMethod]
    public void AnIntentIsAVariantWithWhatItCarries()
    {
        Intent[] intents = [new Intent.Start(), new Intent.Woke(), new Intent.Glanced(), new Intent.Refresh(Asked: true)];

        Assert.AreEqual<Intent>(new Intent.Refresh(true), intents[3]);
        Assert.AreNotEqual<Intent>(new Intent.Refresh(false), intents[3]);
        Assert.IsTrue(intents.OfType<Intent.Refresh>().Single().Asked);
        Assert.AreEqual(1, intents.OfType<Intent.Start>().Count());
    }

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

    [TestMethod]
    public void ASnapshotCarriesWhatASwitchAsks()
    {
        var snapshot = Read(4) with
        {
            QuitQuestion = new WindowQuestion(
                new Question("Quit ChatGPT to switch?", "ChatGPT signs in again.", "Quit ChatGPT and switch", Destructive: false, ConfirmIsDefault: true),
                Confirmed: new Intent.QuitAndSwitch("codex/spare"),
                Declined: new Intent.KeepAppOpen()),
            FailureAlert = new AlertText(
                Title: "Couldn’t switch to spare", Message: "ChatGPT is still open, so nothing has changed.",
                Next: new NextStep.Learn(new SetupLink("Learn more", "https://docs.usepitboard.com/reference/errors"))),
            WindowRequest = new WindowRequest(Serial: 3, Place: new Place.Accounts()),
        };

        Assert.AreEqual(new Intent.QuitAndSwitch("codex/spare"), snapshot.QuitQuestion?.Confirmed);
        Assert.AreEqual("Couldn’t switch to spare", snapshot.FailureAlert?.Title);
        Assert.IsInstanceOfType<Place.Accounts>(snapshot.WindowRequest.Place);
        Assert.AreEqual(new WindowRequest(3, new Place.Accounts()), snapshot.WindowRequest);
    }

    [TestMethod]
    public void AnIntentToSignInCarriesWhatItIsAbout()
    {
        Intent[] intents =
        [
            new Intent.SignIn(Provider: "claude", Again: "work"), new Intent.PasteCode(Code: "the-code#the-state"),
            new Intent.CancelSignIn(), new Intent.PresentSheet(Sheet: new Sheet.Add(Provider: null)),
            new Intent.CloseSheet(), new Intent.SignIn(Provider: "codex", Again: null),
        ];

        Assert.AreEqual<Intent>(new Intent.SignIn("claude", "work"), intents[0]);
        Assert.AreNotEqual<Intent>(new Intent.SignIn("claude", null), intents[0]);
        Assert.AreEqual("work", intents.OfType<Intent.SignIn>().First().Again);
        Assert.IsNull(intents.OfType<Intent.SignIn>().Last().Again, "a new account, named once it is added");
        Assert.AreEqual("the-code#the-state", intents.OfType<Intent.PasteCode>().Single().Code);
        Assert.AreEqual<Intent>(new Intent.PresentSheet(new Sheet.Add(null)), intents[3]);
        Assert.AreNotEqual<Intent>(new Intent.PresentSheet(new Sheet.Add("codex")), intents[3]);
        Assert.AreEqual(1, intents.OfType<Intent.CancelSignIn>().Count());
    }

    // The Rust tests prove what each means; this proves they cross.
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

        var work = new WindowAccount(
            Sites.Claude, Label: "work", Email: "work@example.com", Store: Store, InUse: true, Title: "work");
        var link = new SiteLink(work.Site, "https://claude.ai/chat/x");
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
                    Accounts: [new PickerAccount(work, "work@example.com, window open")], Chosen: Store,
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
        Assert.AreEqual("work@example.com, window open", choose.Accounts[0].Detail);
        Assert.IsInstanceOfType<DownloadState.Running>(snapshot.AccountWindows.Downloads[0].State);
        var clear = snapshot.AccountWindows.Open[0].ClearDownloads;
        Assert.IsNotNull(clear);
        Assert.IsInstanceOfType<Intent.ClearDownloads>(clear.Intent);
        Assert.IsFalse(clear.Enabled);
        var failed = Assert.IsInstanceOfType<WaitingShown.ReadFailed>(snapshot.AccountWindows.Waiting.Shown);
        Assert.AreEqual("Try Again", failed.Retry.Title);
        Assert.AreEqual(
            OperatingSystem.IsWindows() ? "A download is in progress. Exit anyway?" : "A download is in progress. Quit anyway?",
            PitboardFfiMethods.DownloadsQuitQuestion(1)?.Title);
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

    // Equality decides whether a sheet put up keeps what went wrong in the one up.
    [TestMethod]
    public void ASheetIsAVariantWithWhatItIsAbout()
    {
        Sheet[] sheets =
        [
            new Sheet.Add(Provider: "codex"), new Sheet.SignInAgain(Provider: "claude", Label: "work"),
            new Sheet.Name(Provider: "codex", Label: "acme", Email: "c@example.com"),
            new Sheet.Rename(Provider: "claude", Label: "home"),
        ];

        Assert.AreEqual<Sheet>(new Sheet.Add("codex"), sheets[0]);
        Assert.AreNotEqual<Sheet>(new Sheet.Add(null), sheets[0]);
        Assert.AreEqual("work", sheets.OfType<Sheet.SignInAgain>().Single().Label);
        Assert.AreEqual("c@example.com", sheets.OfType<Sheet.Name>().Single().Email);
        Assert.AreEqual("acme", sheets.OfType<Sheet.Name>().Single().Label);
        Assert.AreNotEqual<Sheet>(new Sheet.Rename("claude", "work"), sheets[3]);
    }

    [TestMethod]
    public void ASnapshotCarriesASignInAndItsSheet()
    {
        var running = new RunningSignIn(Id: 3, Url: "https://claude.com/cai/oauth/authorize?code=true", WantsCode: true);
        var snapshot = Read(5) with
        {
            SigningIn = running,
            Sheet = new Sheet.SignInAgain(Provider: "claude", Label: "work"),
            SheetFailure = new FailureText(
                Id: 4, Title: "Couldn’t sign in to work", Message: "`claude` is not on this machine",
                Warnings: ["The account in use is unchanged."],
                Next: new NextStep.Learn(new SetupLink("Learn more", "https://docs.usepitboard.com/reference/errors#signing-in"))),
        };

        Assert.AreEqual(3UL, snapshot.SigningIn?.Id);
        Assert.IsTrue(snapshot.SigningIn?.WantsCode);
        Assert.AreEqual("https://claude.com/cai/oauth/authorize?code=true", snapshot.SigningIn?.Url);
        Assert.AreEqual(running, snapshot.SigningIn);
        Assert.AreEqual<Sheet?>(new Sheet.SignInAgain("claude", "work"), snapshot.Sheet);
        Assert.AreEqual("The account in use is unchanged.", snapshot.SheetFailure?.Warnings.Single());
        Assert.IsNull(Read(5).SigningIn);
    }

    [TestMethod]
    public void ASnapshotCarriesWhatWasRead()
    {
        var snapshot = Read(3);
        var failed = snapshot with
        {
            AccountsShown = new AccountsShown.ReadFailed(
                Title: "Couldn’t Read Accounts", Detail: "Anthropic could not be reached",
                Retry: new Choice("Try Again", new Intent.Refresh(true), Enabled: true)),
        };

        Assert.AreEqual("work 42%", snapshot.MenuBar.NameAndUsage);
        Assert.AreEqual("Updated 08:00", snapshot.UpdatedMenu);
        Assert.AreEqual(
            "Anthropic could not be reached", ((AccountsShown.ReadFailed)failed.AccountsShown).Detail);
        Assert.AreEqual(snapshot.Revision, failed.Revision);
        // Lists are arrays, which a record compares by reference, so the app goes by the revision.
        Assert.AreNotEqual(Read(3), Read(3));
    }

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

    [TestMethod]
    public void AnIntentForTheWindowOrAnAccountCarriesWhatItIsAbout()
    {
        Intent[] intents =
        [
            new Intent.Show(Place: new Place.Checks()), new Intent.Show(Place: null), new Intent.DismissFailure(),
            new Intent.Enrol(Provider: "codex"),
            new Intent.Rename(Provider: "claude", Label: "work", To: "office"),
            new Intent.Forget(Qualified: "claude/spare"), new Intent.DeclineSecondAccount(Provider: "codex"),
        ];

        Assert.AreEqual<Intent>(new Intent.Show(new Place.Checks()), intents[0]);
        Assert.AreNotEqual<Intent>(new Intent.Show(null), intents[0]);
        Assert.IsNull(((Intent.Show)intents[1]).Place);
        Assert.AreEqual("codex", intents.OfType<Intent.Enrol>().Single().Provider);
        Assert.AreEqual("office", intents.OfType<Intent.Rename>().Single().To);
        Assert.AreEqual("claude/spare", intents.OfType<Intent.Forget>().Single().Qualified);
        Assert.AreEqual(1, intents.OfType<Intent.DismissFailure>().Count());
        Assert.AreEqual("codex", intents.OfType<Intent.DeclineSecondAccount>().Single().Provider);
    }

    [TestMethod]
    public void ASnapshotSaysWhatTheMenuAndTheWindowShow()
    {
        var limit = new LimitRow(
            Kind: new LimitKind.FiveHour(), Name: "5-hour limit", Short: "5-hour", Percent: 72.4, Figure: "72%",
            Used: "72% used", Level: UsageLevel.Low, ResetsAt: 1_800_003_900, Resets: "Resets 09:05",
            ResetsIn: "in 1 hr 5 min", Spoken: "5-hour limit, 72 percent used, resets 09:05");
        var use = new ItemAction(Title: "Use", Spoken: "Use spare (Codex)", Intent: new Intent.SwitchTo("codex/spare"));
        var window = new WindowAccount(
            Sites.ChatGpt, Label: "spare", Email: "spare@example.com", Store: "5d3a8c72-9e1b-5f4a-8c2d-3e6f7a8b9c40",
            InUse: false, Title: "spare");
        var spare = new AccountItem(
            Id: "codex:spare", Qualified: "codex/spare", Title: "spare",
            Email: "spare@example.com", Plan: null, MenuTitle: "spare",
            Spoken: "spare (Codex)", InUse: false,
            NeedsSignIn: false, Standing: AccountStanding.Ready, Unplaced: false, End: new RowEnd.Button(use), Action: use,
            Summary: "5-hour 72% used",
            Notes:
            [
                new AccountNote(
                    Kind: NoteKind.Expiring, Text: "Parked login good for 3 more days: sign in again to keep it",
                    Action: null, Learn: null),
            ],
            Detail: new AccountDetail(
                Email: "spare@example.com", Plan: null, SignedInUntil: 1_800_259_200,
                SignIn: "Parked login good for 3 more days", Warm: null, ReadAt: 1_800_000_000, Read: "Updated 08:00",
                Notes: []),
            Help: null, Limits: [limit],
            Actions:
            [
                new AccountAction(
                    Kind: ActionKind.Switch, Title: "Use spare", Intent: use.Intent, Window: null, Enabled: true,
                    Confirm: null),
                new AccountAction(
                    Kind: ActionKind.SignInAgain, Title: "Sign In Again…",
                    Intent: new Intent.PresentSheet(new Sheet.SignInAgain("codex", "spare")), Window: null,
                    Enabled: false, Confirm: null),
                new AccountAction(
                    Kind: ActionKind.OpenWindow, Title: "Open chatgpt.com", Intent: null, Window: window, Enabled: true,
                    Confirm: null),
                new AccountAction(
                    Kind: ActionKind.Remove, Title: "Forget…", Intent: new Intent.Forget("codex/spare"), Window: null,
                    Enabled: true,
                    Confirm: new Question(
                        Title: "Forget “spare (Codex)”?",
                        Message: "Pitboard deletes the login it parked for this account.", Confirm: "Forget",
                        Destructive: true, ConfirmIsDefault: true)),
            ]);
        var giveUp = new NoticeAction(
            Title: "Give Up…", Intent: new Intent.AbandonStuckSwitch(), Dismisses: false, Switches: false,
            Enabled: true,
            Confirm: new Question(
                Title: "Give up on the interrupted switch?", Message: "Every login is kept, and nothing is deleted.",
                Confirm: "Give Up", Destructive: false, ConfirmIsDefault: true));
        var stuck = new PanelNotice(
            Id: "stuck", Severity: Severity.Error, SpokenSeverity: "Problem", Title: "An interrupted switch is waiting",
            Lines: ["An interrupted switch can’t be finished until OpenAI answers."], Until: null, UntilLabel: null,
            Actions: [giveUp], Learn: null);
        var others = new MenuEntry(
            Title: stuck.Title, Subtitle: "Show in Pitboard", Help: stuck.Lines[0], Severity: Severity.Error,
            Intent: new Intent.Show(new Place.Accounts()), Link: null, Enabled: true);
        var snapshot = Read(6) with
        {
            Sections = [new AccountSection(Id: "codex", Heading: "Codex", Accounts: [spare])],
            Notices = [stuck],
            MenuNotices = new MenuNotices(Install: null, Switches: [], SignIns: [], Others: others),
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
        Assert.IsInstanceOfType<LimitKind.FiveHour>(row.Limits[0].Kind);
        Assert.AreEqual(1_800_003_900L, row.Limits[0].ResetsAt);
        Assert.AreEqual(NoteKind.Expiring, row.Notes[0].Kind);
        Assert.AreEqual(1_800_259_200L, row.Detail.SignedInUntil);
        Assert.AreEqual<Intent?>(row.Action!.Intent, row.Actions[0].Intent);
        Assert.IsFalse(row.Actions[1].Enabled);
        Assert.AreEqual(ActionKind.OpenWindow, row.Actions[2].Kind);
        Assert.AreEqual("chatgpt.com", row.Actions[2].Window?.Site.Host);
        Assert.AreEqual<Intent?>(new Intent.Forget("codex/spare"), row.Actions[3].Intent);
        Assert.AreEqual("Forget", row.Actions[3].Confirm?.Confirm);
        Assert.AreEqual(1_800_000_000L, snapshot.UpdatedAt);
        Assert.IsFalse(((AccountsShown.ReadFailed)snapshot.AccountsShown).Retry.Enabled);
        Assert.IsTrue(snapshot.Notices[0].Severity > Severity.Warning);
        Assert.AreEqual<Intent>(new Intent.AbandonStuckSwitch(), snapshot.Notices[0].Actions[0].Intent);
        Assert.AreEqual("Give Up", snapshot.Notices[0].Actions[0].Confirm?.Confirm);
        Assert.AreEqual<Intent?>(new Intent.Show(new Place.Accounts()), snapshot.MenuNotices.Others?.Intent);
        Assert.AreEqual(new Footing.OnlyOne("claude", "work"), snapshot.Footing);
        Assert.IsInstanceOfType<Intent.PresentSheet>(snapshot.Setup?.Actions[0].Intent);
        Assert.AreEqual("Try Again", ((AccountsShown.ReadFailed)snapshot.AccountsShown).Retry.Title);
        Assert.AreEqual("work 42%", snapshot.MenuBar.NameAndUsage);
    }

    [TestMethod]
    public void AnIntentAboutTheMachineCarriesWhatItIsAbout()
    {
        Intent[] intents =
        [
            new Intent.PaneShown(Pane: Pane.Checks), new Intent.PaneShown(Pane: Pane.Activity),
            new Intent.ReadSchedule(), new Intent.SetSchedule(On: true), new Intent.LookForCommandLine(),
        ];

        Assert.AreEqual<Intent>(new Intent.PaneShown(Pane.Checks), intents[0]);
        Assert.AreNotEqual<Intent>(new Intent.PaneShown(Pane.Checks), intents[1]);
        Assert.AreNotEqual<Intent>(new Intent.Show(new Place.Checks()), intents[0]);
        Assert.IsTrue(intents.OfType<Intent.SetSchedule>().Single().On);
        Assert.AreNotEqual<Intent>(new Intent.SetSchedule(false), intents[3]);
        Assert.AreEqual(1, intents.OfType<Intent.ReadSchedule>().Count());
        Assert.AreEqual(1, intents.OfType<Intent.LookForCommandLine>().Count());
    }

    [TestMethod]
    public void ASnapshotSaysWhatIsKnownOfTheMachine()
    {
        var keychain = new CheckLine(
            Id: 0, Name: "Keychain", Level: Level.Fail, SpokenLevel: "Failed", Detail: "locked",
            Advice: "Unlock the login keychain.", Action: null,
            Learn: new SetupLink(Title: "Learn More", Link: "https://docs.usepitboard.com/reference/maintenance-commands#doctor"));
        var pitboard = new CheckGroup(Id: "pitboard", Heading: "Pitboard", Lines: [keychain]);
        var switched = new ActivityLine(
            Id: 0, Date: "Oct 5, 2026 at 2:05 PM", Change: "Switch", Account: "codex/spare",
            Result: "Nothing parked", Done: false, AskedBy: "Pitboard app");
        var machine = Unread() with
        {
            Schedule = new ScheduleShown(
                On: true, Enabled: true, Note: null, Failed: null,
                About: "A parked login is renewed whenever Pitboard runs, and otherwise not."),
            Checks = new ChecksShown(
                Groups: [pitboard], Summary: "1 Issue", Verdict: "Don’t switch accounts until the failed check is fixed.",
                Level: Level.Fail, Checking: false, Checked: "Checked at 14:05", Waiting: null, Report: "{}"),
            Activity = new ActivityShown(Lines: [switched], Empty: null),
            CommandLine = new CommandLineShown(
                Found: new FoundCommandLine.Bundled(Path: "/usr/local/bin/pitboard"),
                InTerminal: "/usr/local/bin/pitboard",
                UpdateNote: "The one inside this app, so it updates with the app.", OffersLink: false,
                CannotLink: null, TerminalNote: null),
        };
        var snapshot = Read(7) with { Machine = machine };

        Assert.IsTrue(snapshot.Machine.Schedule.On);
        Assert.AreEqual(Level.Fail, snapshot.Machine.Checks.Groups[0].Lines[0].Level);
        Assert.AreEqual("Failed", snapshot.Machine.Checks.Groups[0].Lines[0].SpokenLevel);
        Assert.AreEqual(0UL, snapshot.Machine.Checks.Groups[0].Lines[0].Id);
        Assert.AreEqual(keychain, snapshot.Machine.Checks.Groups[0].Lines[0]);
        Assert.AreEqual(0UL, snapshot.Machine.Activity.Lines[0].Id);
        Assert.IsFalse(snapshot.Machine.Activity.Lines[0].Done);
        Assert.IsNull(snapshot.Machine.Activity.Empty);
        Assert.AreEqual<FoundCommandLine?>(
            new FoundCommandLine.Bundled("/usr/local/bin/pitboard"), snapshot.Machine.CommandLine.Found);
        Assert.IsFalse(snapshot.Machine.CommandLine.OffersLink);
        Assert.AreNotEqual(
            machine, machine with { Checks = machine.Checks with { Groups = [pitboard] } },
            "its lists are arrays, compared by reference");
        Assert.AreEqual(Unread(), Unread(), "every list made with [], so the same array");
        Assert.AreSame(Unread().Checks.Groups, Unread().Checks.Groups);
    }

    [TestMethod]
    public void ANameComesWithWhatItsSheetSends()
    {
        var named = new Sheet.Name("claude", "acme", "a@example.com");
        Assert.AreEqual(
            new NameCheck(null, new Intent.Rename("claude", "acme", "home")), PitboardFfiMethods.CheckName(named, "claude", "  home "));
        Assert.AreEqual(new NameCheck(null, new Intent.CloseSheet()), PitboardFfiMethods.CheckName(named, "claude", " acme"));
        Assert.AreEqual(new NameCheck("Type a name for this account.", null), PitboardFfiMethods.CheckName(named, "claude", " \n"));
        Assert.IsNull(PitboardFfiMethods.CheckName(new Sheet.Rename("claude", "work"), "claude", " work ").Intent);
        Assert.AreEqual(
            new Intent.SignIn("codex", "work"),
            PitboardFfiMethods.CheckName(new Sheet.SignInAgain("codex", "work"), "codex", "").Intent);
        Assert.AreEqual(
            new NameCheck(null, new Intent.SignIn("codex", null)),
            PitboardFfiMethods.CheckName(new Sheet.Add("codex"), "codex", "codex/phu"),
            "a new account is named once it is added");
        Assert.AreEqual(
            new NameCheck("A name can’t contain “/”.", null), PitboardFfiMethods.CheckName(named, "claude", "codex/phu"));
    }

    private sealed class Waiting : ModelListener
    {
        private readonly List<Snapshot> _told = [];

        public int? TellingThread { get; private set; }

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

    private const string NoFixtures =
        "This library was built without the `fixture` feature, so it has no fixture to start. " +
        "Build it with `cargo build -p pitboard-ffi --features fixture` and pass that to " +
        "PitboardNativeLibrary to run this test.";

    private static readonly string[] OneToolsAccounts = ["claude/work", "claude/personal"];

    private static readonly string[] OneToolsLabels = ["work", "personal"];

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
            "readFailure", "stuck", "chatGPTOpen", "claudeDesktop", "claudeDesktopOpen",
            "warmUp", "leftInFile", "loginReplaced", "switchedByItself",
        ];
        CollectionAssert.AreEqual(ordered, names);
        var unknown = Assert.ThrowsExactly<FixtureException.Unknown>(
            () => PitboardModel.Fixture("twoTool", listener, new InUtc()));
        Assert.Contains("twoTools, oneTool", unknown.reason);
        Assert.Contains("<title>claude.ai stand-in</title>", PitboardFfiMethods.FixturePage("pitboard-fixture://claude.ai/"));
        Assert.IsEmpty(listener.Told);
    }

    private static readonly TimeSpan Patience = TimeSpan.FromSeconds(30);

    // Each test's own, so none empties the fixture of a debug build or of another run.
    private readonly string _own = Path.Combine(Path.GetTempPath(), $"pitboard-bindings-{Guid.NewGuid():N}");

    [TestCleanup]
    public void RemoveTheFixture() => DeleteOnceLanesStop(_own);

    // Shutdown waits for the actor alone, so a lane may still write here for a moment.
    private static void DeleteOnceLanesStop(string path)
    {
        var until = DateTime.UtcNow + TimeSpan.FromSeconds(5);
        while (Directory.Exists(path))
        {
            try
            {
                Directory.Delete(path, recursive: true);
            }
            catch (IOException) when (DateTime.UtcNow < until)
            {
                Thread.Sleep(50);
            }
        }
    }

    private static void NeedsFixtures()
    {
        if (PitboardFfiMethods.FixtureNames().Length == 0)
        {
            Assert.Inconclusive(NoFixtures);
        }
    }

    private static bool ReadIn(Snapshot snapshot) =>
        snapshot is { Reading: false, UpdatedWindow.Length: > 0, Sections.Length: > 0 };

    private static IEnumerable<AccountItem> Rows(Snapshot snapshot) =>
        snapshot.Sections.SelectMany(section => section.Accounts);

    private static bool Uses(Snapshot snapshot, string qualified) =>
        !Rows(snapshot).Any(row => row.End is RowEnd.Switching) && Rows(snapshot).Any(row => row.InUse && row.Qualified == qualified);

    private static Snapshot Start(PitboardModel model, Waiting waiting)
    {
        model.Send(new Intent.Start());
        var read = waiting.Until(ReadIn, Patience);
        Assert.IsNotNull(read, "the fixture's model told of no accounts read");
        return read;
    }

    [TestMethod]
    public void AFixturesModelStartsAndTellsAListenerOfItsAccounts()
    {
        NeedsFixtures();
        var waiting = new Waiting();
        var clock = new InUtc();
        using var model = PitboardModel.FixtureIn("oneTool", _own, waiting, new StandInApps(), new Posting(), clock);

        var read = Start(model, waiting);
        model.Shutdown();

        CollectionAssert.AreEqual(OneToolsAccounts, Rows(read).Select(row => row.Qualified).ToArray());
        Assert.IsTrue(Rows(read).First().InUse);
        Assert.AreEqual("work", read.Sections.Single().Accounts[0].Title);
        var claude = Assert.IsInstanceOfType<SiteMenu.Several>(read.AccountWindows.Menus.Single());
        Assert.AreEqual("Open claude.ai", claude.Title);
        Assert.AreEqual("claude.ai", claude.Site.Host);
        CollectionAssert.AreEqual(
            OneToolsLabels, read.AccountWindows.Accounts.Select(window => window.Label).ToArray());
        Assert.IsNotNull(waiting.TellingThread);
        Assert.AreNotEqual(Environment.CurrentManagedThreadId, waiting.TellingThread);
        Assert.IsFalse(waiting.ToldOnThePool, "told on a thread of .NET's thread pool");
        Assert.AreEqual(read.UpdatedWindow, read.UpdatedMenu);
    }

    [TestMethod]
    public void ACodexSwitchAsksTheAppsAppControlToQuitChatGptAndOpenItAgain()
    {
        NeedsFixtures();
        var waiting = new Waiting();
        var apps = new StandInApps();
        using var model = PitboardModel.FixtureIn("chatGPTOpen", _own, waiting, apps, new Posting(), new InUtc());
        Start(model, waiting);

        model.Send(new Intent.SwitchTo("codex/spare"));
        var asked = waiting.Until(snapshot => snapshot.QuitQuestion is not null, Patience);
        Assert.IsNotNull(asked, "never asked about quitting ChatGPT");
        Assert.AreEqual(new Intent.QuitAndSwitch("codex/spare"), asked.QuitQuestion?.Confirmed);
        model.Send(asked.QuitQuestion!.Confirmed);
        var switched = waiting.Until(snapshot => Uses(snapshot, "codex/spare"), Patience);
        var calls = apps.Asked.Until(5, Patience);
        model.Shutdown();

        Assert.IsNotNull(switched, "the switch was never read back");
        string[] walked =
        [
            $"running {StandInApps.ChatGpt}", $"running {StandInApps.ChatGpt}", $"quit {StandInApps.ChatGpt}",
            $"running {StandInApps.ChatGpt}", $"open {StandInApps.ChatGptCopy}",
        ];
        CollectionAssert.AreEqual(walked, calls);
    }

    [TestMethod]
    public void APlatformExceptionFromTheAppsAppControlStopsTheSwitch()
    {
        NeedsFixtures();
        var waiting = new Waiting();
        var apps = new StandInApps { RefusesToQuit = true };
        using var model = PitboardModel.FixtureIn("chatGPTOpen", _own, waiting, apps, new Posting(), new InUtc());
        Start(model, waiting);

        model.Send(new Intent.SwitchTo("codex/spare"));
        Assert.IsNotNull(waiting.Until(snapshot => snapshot.QuitQuestion is not null, Patience));
        model.Send(new Intent.QuitAndSwitch("codex/spare"));
        var stopped = waiting.Until(snapshot => snapshot.FailureAlert is not null, Patience);
        model.Shutdown();

        Assert.IsNotNull(stopped, "the switch never stopped");
        var failure = stopped.FailureAlert!;
        Assert.AreEqual("Couldn’t switch to spare", failure.Title);
        Assert.AreEqual("ChatGPT is still open, so nothing has changed. Quit it, then switch again.", failure.Message);
        Assert.IsTrue(Uses(stopped, "codex/main"));
        // Taken as a success, the quit would have been followed by a wait on Running.
        string[] refused = [$"running {StandInApps.ChatGpt}", $"running {StandInApps.ChatGpt}", $"quit {StandInApps.ChatGpt}"];
        CollectionAssert.AreEqual(refused, apps.Asked.All);
    }

    [TestMethod]
    public void ARunOutIsPostedThroughTheAppsNotifications()
    {
        NeedsFixtures();
        var waiting = new Waiting();
        var posting = new Posting();
        using var model = PitboardModel.FixtureIn("oneTool", _own, waiting, new StandInApps(), posting, new InUtc());
        Start(model, waiting);

        model.Send(new Intent.SwitchTo("claude/personal"));
        var switched = waiting.Until(snapshot => Uses(snapshot, "claude/personal"), Patience);
        var posted = posting.Posted.Until(1, Patience);
        model.Shutdown();

        Assert.IsNotNull(switched, "the switch was never read back");
        // When the limit resets is the read's.
        var resetsAt = posted[0].Id["claude/personal/session/-".Length..];
        Assert.IsTrue(long.TryParse(resetsAt, CultureInfo.InvariantCulture, out _), resetsAt);
        RunOutNotice[] ranOut =
        [
            new(
                Id: $"claude/personal/session/-{resetsAt}",
                Title: LimitReached, Subtitle: "Claude Code", Body: "personal has used 100%. work has used 42%.",
                Clicked: "accounts",
                Button: new NotificationButton(OperatingSystem.IsWindows() ? "Switch to work" : "Switch", "switch/claude/work")),
        ];
        CollectionAssert.AreEqual(ranOut, posted);
    }

    [TestMethod]
    public void ALoginReplacedOutsidePitboardIsPostedThroughTheAppsNotifications()
    {
        NeedsFixtures();
        var waiting = new Waiting();
        var posting = new Posting();
        using var model = PitboardModel.FixtureIn("loginReplaced", _own, waiting, new StandInApps(), posting, new InUtc());
        Start(model, waiting);

        // The account in use has run out too, which is posted after it.
        var posted = posting.Posted.Until(2, Patience);
        model.Shutdown();

        var again = OperatingSystem.IsWindows() ? "Sign in again…" : "Sign In Again";
        var notice = posted.Single(notice => notice.Id.StartsWith("warning/login_replaced/", StringComparison.Ordinal));
        Assert.AreEqual(OperatingSystem.IsWindows() ? "Sign-in replaced" : "Sign-In Replaced", notice.Title);
        Assert.AreEqual(
            "work’s sign-in was replaced outside Pitboard: Claude Code is now signed in to personal, " +
            "and Pitboard has no sign-in saved for work.",
            notice.Body);
        Assert.AreEqual(new NotificationButton(again, "sign-in-again/claude/work"), notice.Button);
    }

    // Plan 4.13: whom it switched to, under the tool, and Switch Back, which switches to the account it left.
    [TestMethod]
    public void AnAutomaticSwitchIsPostedWithSwitchBack()
    {
        NeedsFixtures();
        var waiting = new Waiting();
        var posting = new Posting();
        using var model = PitboardModel.FixtureIn("switchedByItself", _own, waiting, new StandInApps(), posting, new InUtc());
        Start(model, waiting);

        var notice = posting.Posted.First(posted => posted.Id.StartsWith("auto/", StringComparison.Ordinal), Patience);
        Assert.IsNotNull(notice, "no automatic switch was posted");
        Assert.AreEqual("Switched to personal", notice.Title);
        Assert.AreEqual("Claude Code", notice.Subtitle);
        Assert.AreEqual(OperatingSystem.IsWindows() ? "Switch back" : "Switch Back", notice.Button?.Title);

        model.Send(new Intent.NotificationClicked(notice.Button!.Clicked));
        var back = waiting.Until(snapshot => Uses(snapshot, "claude/work"), Patience);
        model.Shutdown();

        Assert.IsNotNull(back, "Switch Back left Claude Code on personal");
    }

    [TestMethod]
    public void ANotificationTheAppsNotificationsRefusedIsSaidAfterDoctorsChecks()
    {
        NeedsFixtures();
        var waiting = new Waiting();
        var posting = new Posting { Refuses = "COMException 0x80070490" };
        using var model = PitboardModel.FixtureIn("loginReplaced", _own, waiting, new StandInApps(), posting, new InUtc());
        Start(model, waiting);
        // The login replaced, then the run-out of the account in use, the last refused.
        Assert.HasCount(2, posting.Posted.Until(2, Patience));

        model.Send(new Intent.PaneShown(Pane.Checks));
        static CheckLine? Refused(Snapshot snapshot) =>
            snapshot.Machine.Checks.Groups.SelectMany(group => group.Lines).FirstOrDefault(line => line.Name == "Notifications");
        var checkedIn = waiting.Until(snapshot => Refused(snapshot) is not null, Patience);
        model.Shutdown();

        Assert.IsNotNull(checkedIn, "the machine pane never said the refusal");
        var line = Refused(checkedIn)!;
        Assert.AreEqual(Level.Warn, line.Level);
        Assert.StartsWith($"“{LimitReached}” not posted at ", line.Detail);
        Assert.EndsWith(" · COMException 0x80070490", line.Detail);
    }

    // A notification's title, in the case each system gives one.
    private static string LimitReached => OperatingSystem.IsWindows() ? "5-hour limit reached" : "5-Hour Limit Reached";

    private static bool LibraryIsOpened =>
        typeof(ModelTests).Assembly.GetCustomAttributes<AssemblyMetadataAttribute>()
            .Any(metadata => metadata is { Key: "PitboardNativeLibraryOpened", Value: "true" });

    // The version the gate reads, from Cargo.toml rather than the library under test.
    private static int ProductMajor()
    {
        for (var folder = new DirectoryInfo(AppContext.BaseDirectory); folder is not null; folder = folder.Parent)
        {
            var manifest = Path.Combine(folder.FullName, "Cargo.toml");
            var version = File.Exists(manifest)
                ? Regex.Match(File.ReadAllText(manifest), @"^\[workspace\.package\]$[^\[]*?^version = ""(\d+)\.", RegexOptions.Multiline)
                : Match.Empty;
            if (version.Success)
            {
                return int.Parse(version.Groups[1].Value, CultureInfo.InvariantCulture);
            }
        }

        throw new InvalidOperationException($"no workspace Cargo.toml above {AppContext.BaseDirectory}");
    }

    [TestMethod]
    [OSCondition(OperatingSystems.Windows)]
    public void AWindowsModelReadsNothingUntilPitboardForWindowsIsReleased()
    {
        if (LibraryIsOpened)
        {
            Assert.Inconclusive(
                "This library was built with `--cfg pitboard_unreleased_windows`, which opens it to Windows. " +
                "Pass one built as a release is to PitboardNativeLibrary, without PitboardNativeLibraryOpened, to run this test.");
        }

        var root = Directory.CreateTempSubdirectory("pitboard-gate-").FullName;
        try
        {
            string[] homes = ["USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA", "CODEX_HOME", "CLAUDE_CONFIG_DIR", "PITBOARD_HOME"];
            var environment = homes.ToDictionary(
                home => home, home => Directory.CreateDirectory(Path.Combine(root, home)).FullName);
            var refused = PitboardFfiMethods.LaunchRefusal(environment);
            var waiting = new Waiting();
            using var model = new PitboardModel(
                new AppLaunch(environment, null), waiting, new StandInApps(), new Posting(), new InUtc());

            model.Send(new Intent.Start());
            var read = waiting.Until(
                snapshot => snapshot is { Reading: false }
                    && (snapshot.AccountsShown is AccountsShown.ReadFailed || snapshot.UpdatedWindow.Length > 0),
                Patience);
            model.Shutdown();

            Assert.IsNotNull(read, "the model told of no read");
            var failed = read.AccountsShown as AccountsShown.ReadFailed;
            if (ProductMajor() == 0)
            {
                Assert.AreEqual(
                    "windows_not_released", refused?.Code,
                    "A library built with `--cfg pitboard_unreleased_windows` takes PitboardNativeLibraryOpened=true.");
                Assert.AreEqual(refused?.Message, failed?.Detail);
                // Shutdown waits for the actor alone, so a lane may still write after it.
                var settled = DateTime.UtcNow + TimeSpan.FromSeconds(2);
                do
                {
                    foreach (var home in environment.Values)
                    {
                        Assert.IsEmpty(Directory.EnumerateFileSystemEntries(home), home);
                    }

                    Thread.Sleep(50);
                } while (DateTime.UtcNow < settled);
            }
            else
            {
                Assert.IsNull(failed, failed?.Detail);
                Assert.IsNull(refused, refused?.Message);
            }
        }
        finally
        {
            DeleteOnceLanesStop(root);
        }
    }
}
