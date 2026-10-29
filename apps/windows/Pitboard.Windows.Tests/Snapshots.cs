using Pitboard.Core;

namespace Pitboard.Windows.Tests;

// Each call makes every list anew, as the bindings do for every snapshot told. Each sentence is the model's own, as
// present/words/ pins it, which StandInsTests checks. Each names every part, as Snapshot() does, unless a test says
// what changed.
internal static class Snapshots
{
    public static Snapshot Read(ulong revision) =>
        new(
            Revision: revision, Changed: new Changed(Since: 0, Parts: Enum.GetValues<Part>()), Reading: false,
            QuitQuestion: null, ReplaceQuestion: null,
            WindowRequest: new WindowRequest(Serial: 0, Place: null),
            SigningIn: null, Sheet: null, SheetFailure: null,
            StatusItem: new StatusItemShown(
                NameAndUsage: "work 42%", Usage: "42%", Spoken: "Pitboard, work 42%", Level: UsageLevel.Plenty, Attention: false,
                Tip: "Pitboard\nwork: 5-hour 42% used"),
            Sections: [],
            AccountMenu: new AccountMenuShown(Offers: ["Switch", "Sign in again…", "Rename…"], Forget: "Remove account…"),
            Notices: [],
            MenuNotices: new MenuNotices(Install: null, Switches: [], SignIns: [], Others: null),
            Footing: new Footing.OnlyOne(Account: new AccountKey(Tool.Claude, "work")), Setup: null,
            AccountsShown: new AccountsShown.List(), MenuAccountsNote: null, UpdatedAt: 1_800_000_000,
            UpdatedMenu: "Updated 12:00", UpdatedWindow: "Updated 12:00", SheetText: null,
            SigningInText: null, StowText: null, FailureAlert: null,
            Machine: Unread(),
            AccountWindows: new AccountWindowsShown(
                Accounts: [], Menus: [], Open: [],
                Waiting: new WindowWaiting(
                    WindowTitle: "Account", Shown: new WaitingShown.Reading(Title: "Reading accounts…")),
                Closing: [], Deleting: [], Picker: null, Downloads: [],
                NoLink: new EmptyList(
                    Title: "No link to open",
                    Detail: "Share a claude.ai or chatgpt.com page with Pitboard from your browser’s Share menu to open it as one of your accounts."),
                Held: []),
            Uninstall: new ItemOffer(Title: "Uninstall…", Intent: new Intent.Uninstall(), Enabled: true, Confirm: null),
            Removal: null,
            Welcome: new WelcomeShown(
                Title: "Welcome to Pitboard", Lines: [], IconLink: null, ToolsHeading: "Found on this PC", Tools: [],
                OpenAtLogin: new WelcomeLogin(
                    Title: "Start when you sign in to Windows", On: false, Note: "Pitboard watches your limits only while it’s running."),
                Done: new Choice(Title: "Get started", Intent: new Intent.Show(Place: new Place.Accounts()), Enabled: true)),
            WidgetRevision: 0, Answers: []);

    private static MachineShown Unread() => new(
        Schedule: new ScheduleShown(
            On: false, Enabled: true, Note: null, Failed: null,
            About: "Your PC refreshes your saved sign-ins once a day, so one you leave alone doesn’t expire."),
        Checks: new ChecksShown(
            Groups: [], Summary: null, Verdict: null, Level: Level.Ok, Checking: false, Checked: null, Waiting: "Checking this PC…",
            Report: null),
        Activity: new ActivityShown(
            Lines: [], Empty: new EmptyList(Title: "No activity", Detail: "Pitboard lists every change it makes here.")),
        CommandLine: new CommandLineShown(
            Found: null, InTerminal: null, UpdateNote: null, OffersLink: false, CannotLink: null, TerminalNote: null),
        AtLimit: new AtLimitShown(
            Title: "Before an account runs out",
            Choice: AtLimit.Notify,
            Choices: AtLimitChoices,
            At: 95, Lowest: 50, Highest: 99, Enabled: true, ShareEnabled: true, Heading: "When a limit reaches",
            AtLabel: "When a limit reaches 95%",
            ShareLabel: "At", ShareRange: "From 50% to 99%", Note: null, Standing: null, Terms: Terms),
        Warm: new WarmShown(
            Title: "Warm-up", From: 360, To: 1320, RangeLabel: "6:00 AM to 10:00 PM", Accounts: [],
            NoneEnrolled: "Add a Claude Code account to warm it up.", Standing: null,
            Note: "Pitboard starts each account’s 5-hour limit again as soon as it resets, within these hours, while it’s open.",
            Enabled: false));

    // One array, so two snapshots made here are equal.
    private static readonly AtLimitChoice[] AtLimitChoices =
    [
        new(AtLimit.Nothing, "Do nothing"), new(AtLimit.Notify, "Notify me"), new(AtLimit.Switch, "Switch automatically"),
    ];

    public static readonly TermsShown Terms = new(
        "Anthropic’s terms may cover automatic switching and warm-ups.", "Is this allowed?", "https://docs.usepitboard.com/concepts/limits#is-this-allowed");
}
