using Pitboard.Core;

namespace Pitboard.Windows.Tests;

// Each call makes every list anew, as the bindings do for every snapshot told.
internal static class Snapshots
{
    public static Snapshot Read(ulong revision) =>
        new(
            Revision: revision, Reading: false, QuitQuestion: null, ReplaceQuestion: null,
            WindowRequest: new WindowRequest(Serial: 0, Pane: null),
            SigningIn: null, Sheet: null, SheetFailure: null,
            MenuBar: new MenuBarText(
                NameAndUsage: "work 42%", Usage: "42%", Spoken: "Pitboard, work 42%", Level: UsageLevel.Plenty, Attention: false,
                Tip: "Pitboard\nwork: 5-hour 42%"),
            Sections: [], AccountMenu: new AccountMenuShown(Offers: ["Use", "Sign in again…", "Rename…"], Forget: "Forget…"),
            Notices: [],
            MenuNotices: new MenuNotices(Install: null, Switches: [], Others: null),
            Footing: new Footing.OnlyOne(Provider: "claude", Label: "work"), Setup: null,
            AccountsShown: new AccountsShown.List(), MenuAccountsNote: null,
            UpdatedMenu: "Updated 08:00", UpdatedWindow: "Updated 08:00", SheetText: null,
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
                Held: []));

    private static MachineShown Unread() => new(
        Schedule: new ScheduleShown(
            On: false, Enabled: true, Note: null, Failed: null,
            About: "A parked login is renewed whenever Pitboard runs, and otherwise not."),
        Checks: new ChecksShown(Lines: [], Summary: null, Level: Level.Ok, Checking: false, Checked: null, Waiting: "Checking this PC…"),
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
            ShareLabel: "Share of a limit", ShareRange: "From 50% to 99%", Note: null, Standing: null, Terms: null),
        Warm: new WarmShown(
            Title: "Warm up limits", From: 360, To: 1320, RangeLabel: "06:00 to 22:00", Accounts: [],
            NoneEnrolled: "No Claude Code account is enrolled yet.", Standing: null, Note: "", Enabled: false, Terms: Terms));

    // One array, so two snapshots made here are equal.
    private static readonly AtLimitChoice[] AtLimitChoices =
    [
        new(AtLimit.Nothing, "Do nothing"), new(AtLimit.Notify, "Notify me"), new(AtLimit.Switch, "Switch automatically"),
    ];

    public static readonly TermsShown Terms = new(
        "Anthropic's terms may cover automatic switching and warm-ups.", "Is this allowed?", "https://docs.usepitboard.com/concepts/limits#is-this-allowed");
}
