using Pitboard.Core;

namespace Pitboard.Windows.Tests;

// What the model presents, made by hand, so each view test says only what it is about.
internal static class Items
{
    public static AccountItem Account(string provider, string label, bool inUse = false) =>
        new(
            Id: $"{provider}:{label}", Provider: provider, Qualified: $"{provider}/{label}", Title: label,
            Email: $"{label}@example.com", Plan: null, MenuTitle: label, SpokenName: label,
            Spoken: inUse ? $"{label}, in use" : label, InUse: inUse, NeedsSignIn: false, Unplaced: false,
            Switching: false,
            Action: inUse ? null : new ItemAction("Use", $"Use {label}", new Intent.SwitchTo($"{provider}/{label}")),
            Summary: "", Problem: null, StaleNote: null, Pace: null, ParkedNote: null, WarmNote: null, Help: null,
            Limits: [], Offers: [], Windows: [], Forget: null);

    public static WindowAccount Window(string provider, string label, bool inUse = false) =>
        new(
            Site: PitboardFfiMethods.SitesFor(provider).Single(), Label: label, Email: $"{label}@example.com",
            Store: $"{provider}-{label}", InUse: inUse, Title: label);

    public static LimitRow Limit(double percent, PaceStanding standing = PaceStanding.Even) =>
        new(
            Name: "5-hour", Short: "5h", Percent: percent, Figure: $"{percent:0}%",
            Level: percent >= 90 ? UsageLevel.Out : percent >= 70 ? UsageLevel.Low : UsageLevel.Plenty,
            Resets: "resets in 2h 10m",
            Pace: new LimitPace(Expected: 30, Standing: standing, Said: standing == PaceStanding.Over ? "over pace" : "", Help: "Even use would be at 30% by now"),
            Spoken: $"5-hour limit {percent:0}% used, resets in 2 hours 10 minutes");

    public static ItemOffer Forget(string qualified) =>
        new(
            Title: "Forget…", Intent: new Intent.Forget(qualified), Enabled: true,
            Confirm: new Question("Forget this account?", "Its parked login is deleted.", "Forget"));

    public static PanelNotice Notice(string id, params NoticeAction[] actions) =>
        new(
            Id: id, Severity: Severity.Info, SpokenSeverity: "Note", Title: "Switched Codex to spare",
            Lines: ["Any codex session started before this switch keeps using main."], Until: null, UntilLabel: null,
            Actions: actions);

    public static NoticeAction Dismiss(string provider) =>
        new(Title: "Dismiss", Intent: new Intent.DismissSwitch(provider), Dismisses: true, Switches: false, Enabled: true, Confirm: null);

    public static AccountSection Section(string provider, string? heading, params AccountItem[] accounts) =>
        new(Id: provider, Heading: heading, Accounts: accounts);

    public static SheetTool Tool(string code, string name, Missing? missing = null) =>
        new(Code: code, Name: name, Message: $"Pitboard opens {name}’s own sign-in in your browser.", Confirm: "Sign in", Missing: missing);

    public static Missing CodexCliMissing() =>
        new(
            Title: "Pitboard can’t find the Codex CLI on this PC.",
            Detail: "Adding a Codex account runs the Codex CLI’s own sign-in. Pitboard doesn’t use the copy inside the ChatGPT app. Install the CLI, then come back to this window.",
            LinkTitle: "How to install the Codex CLI", Link: "https://developers.openai.com/codex/cli");

    public static SheetText AddAccount(params SheetTool[] tools) =>
        new(
            Title: "Add account", Message: "Pitboard opens Claude Code’s own sign-in in your browser.", Tools: tools,
            Tool: "claude", Account: null, Name: "", Prompt: "work", Confirm: "Sign in", Saving: false, Progress: null);

    public static SheetText Rename(string label) =>
        new(
            Title: $"Rename “{label}”", Message: "The account keeps its parked login.", Tools: [], Tool: "claude",
            Account: null, Name: label, Prompt: label, Confirm: "Rename", Saving: false, Progress: null);

    public static RunningSignIn Running(string provider, string? url = null, bool wantsCode = false) =>
        new(Id: 1, Provider: provider, Name: "third", Said: "", Url: url, WantsCode: wantsCode, CodeRefused: false);

    public static SigningInText SigningIn(string tool) =>
        new(
            Title: $"Signing in to {tool}",
            Message: $"Finish signing in as third in your browser. This closes once {tool} says you’re in.",
            CodeNote: $"{tool} takes the code shown after you sign in, whether or not your browser came back to it.",
            Refused: null, Waiting: "Waiting for your browser", OffersLink: true,
            Cancel: new Choice("Cancel", new Intent.CancelSignIn(), Enabled: true), EscapeCancels: true, Undo: null,
            Announce: null);

    public static StowText Stow(params string[] lines) =>
        new(
            Title: "Put away the login left in a file",
            Message: "Claude Code left a login in a file behind the keychain.", Lines: lines,
            Looking: lines.Length == 0 ? "Finding out whose login it is…" : null, Confirm: "Put away",
            CanConfirm: lines.Length > 0, Saving: false);
}
