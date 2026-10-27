using Pitboard.Core;

namespace Pitboard.Windows.Tests;

// What the model presents, made by hand, so each view test says only what it is about.
internal static class Items
{
    public static AccountItem Account(string provider, string label, bool inUse = false) =>
        Acting(
            new(
                Id: $"{provider}:{label}", Qualified: $"{provider}/{label}", Title: label,
                Email: $"{label}@example.com", EmailLine: $"{label}@example.com", Plan: null, MenuTitle: label,
                Spoken: inUse ? $"{label}, in use" : label, InUse: inUse, NeedsSignIn: false,
                Standing: inUse ? AccountStanding.InUse : AccountStanding.Ready, Unplaced: false,
                End: inUse ? new RowEnd.InUse("In use") : null, Action: null, Summary: "", Notes: [], Lines: [],
                Detail: new AccountDetail(
                    Email: $"{label}@example.com", Plan: null, SignedInUntil: null, SignIn: null, Warm: null,
                    ReadAt: null, Read: null, Notes: []),
                Help: null, Limits: [], Offers: [], Windows: [], Forget: null, Actions: []),
            inUse ? null : new ItemAction("Use", $"Use {label}", new Intent.SwitchTo($"{provider}/{label}")));

    // What pressing it does, and the button at its row's end that does it.
    public static AccountItem Acting(AccountItem item, ItemAction? action) =>
        action is null ? item with { Action = null } : item with { Action = action, End = new RowEnd.Button(action) };

    // pitboard-sites' site for the tool, with the hosts and paths a snapshot carries.
    public static Site Site(string provider) =>
        provider == "codex"
            ? new Site(
                Host: "chatgpt.com", Name: "chatgpt.com", Provider: "codex",
                Aliases: ["chat.openai.com", "www.chatgpt.com", "chat.com"],
                Hosts: ["chatgpt.com", "chat.openai.com", "www.chatgpt.com", "chat.com"],
                SignInHosts: ["auth.openai.com", "login.live.com", "login.microsoftonline.com", "appleid.apple.com"],
                BlockedHosts: ["accounts.google.com"], SignInPaths: [["api", "auth"]], StoreName: "codex",
                SignInSteps: "", BlockedServices: "")
            : new Site(
                Host: "claude.ai", Name: "claude.ai", Provider: "claude", Aliases: [], Hosts: ["claude.ai"],
                SignInHosts: [], BlockedHosts: ["accounts.google.com"], SignInPaths: [["magic-link"]],
                StoreName: "claude", SignInSteps: "", BlockedServices: "");

    public static WindowAccount Window(string provider, string label, bool inUse = false) =>
        new(
            Site: Site(provider), Label: label, Email: $"{label}@example.com",
            Store: $"{provider}-{label}", InUse: inUse, Title: label);

    public static LimitRow Limit(double percent) =>
        new(
            Kind: new LimitKind.FiveHour(), Name: "5-hour limit", Short: "5-hour", Percent: percent,
            Figure: $"{percent:0}%", Used: $"{percent:0}% used",
            Level: percent >= 90 ? UsageLevel.Out : percent >= 70 ? UsageLevel.Low : UsageLevel.Plenty,
            ResetsAt: 1_800_007_800, Resets: "Resets 10:10 AM", ResetsIn: "in 2 hr 10 min",
            Spoken: $"5-hour limit, {percent:0} percent used, resets 10:10 AM");

    public static ItemOffer Forget(string qualified) =>
        new(
            Title: "Forget…", Intent: new Intent.Forget(qualified), Enabled: true,
            Confirm: new Question("Forget this account?", "Its parked login is deleted.", "Forget", Destructive: true, ConfirmIsDefault: true));

    public static PanelNotice Notice(string id, params NoticeAction[] actions) =>
        new(
            Id: id, Severity: Severity.Info, SpokenSeverity: "Note", Title: "Switched Codex to spare",
            Lines: ["Any codex session started before this switch keeps using main."], Until: null, UntilLabel: null,
            Actions: actions, Learn: null);

    public static NextStep LearnMore =>
        new NextStep.Learn(new SetupLink("Learn more", "https://docs.usepitboard.com/reference/errors"));

    public static NoticeAction Dismiss(string provider) =>
        new(Title: "Dismiss", Intent: new Intent.DismissSwitch(provider), Dismisses: true, Switches: false, Enabled: true, Confirm: null);

    public static AccountSection Section(string provider, string? heading, params AccountItem[] accounts) =>
        new(Id: provider, Heading: heading, Accounts: accounts);

    public static SheetTool Tool(string code, string name, Missing? missing = null) =>
        new(Code: code, Name: name, Message: $"Pitboard opens {name}’s own sign-in in your browser.", Confirm: "Sign in", Missing: missing, Steps: []);

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

    public static RunningSignIn Running(string? url = null, bool wantsCode = false) =>
        new(Id: 1, Url: url, WantsCode: wantsCode);

    public static SigningInText SigningIn(string tool) =>
        new(
            Title: $"Signing in to {tool}",
            Message: "Finish signing in in your browser.",
            CodeNote: $"{tool} takes the code shown after you sign in, whether or not your browser came back to it.",
            Refused: null, Waiting: "Waiting for your browser", OffersLink: true,
            Cancel: new Choice("Cancel", new Intent.CancelSignIn(), Enabled: true), EscapeCancels: true, Undo: null,
            Announce: null, Steps: []);

    // `found`: what the look said the file holds, none while it looks.
    public static StowText Stow(string? found = null) =>
        new(
            Title: "Clean up the leftover sign-in file",
            Message: found ?? "Clean Up deletes the sign-in file Claude Code left behind Credential Manager.",
            Looking: found is null ? "Finding out whose sign-in it is…" : null, Confirm: "Clean up",
            CanConfirm: found is not null, Saving: false);
}
