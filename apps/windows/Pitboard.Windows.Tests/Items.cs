using Pitboard.Core;

namespace Pitboard.Windows.Tests;

// What the model presents, made by hand, so each view test says only what it is about, in the model's words as
// present/words/ pins them for these labels, which StandInsTests checks.
internal static class Items
{
    // The model's tool a test names by its code, which the rows' identifiers are made of.
    public static Pitboard.Core.Tool ToolOf(string provider) =>
        provider switch
        {
            "claude" => Pitboard.Core.Tool.Claude,
            "codex" => Pitboard.Core.Tool.Codex,
            "desktop" => Pitboard.Core.Tool.Desktop,
            _ => throw new ArgumentOutOfRangeException(nameof(provider), provider, "a tool's code"),
        };

    public static AccountKey Key(string provider, string label) => new(ToolOf(provider), label);

    public static AccountItem Account(string provider, string label, bool inUse = false) =>
        Acting(
            new(
                Id: $"{provider}:{label}", Qualified: $"{provider}/{label}", Title: label,
                Email: $"{label}@example.com", Plan: null, MenuTitle: label,
                Spoken: inUse ? $"{label}, in use" : label, InUse: inUse, NeedsSignIn: false,
                Standing: inUse ? AccountStanding.InUse : AccountStanding.Ready, Unplaced: false,
                End: inUse ? new RowEnd.InUse("In use") : null, Action: null, Summary: "", Notes: [],
                Detail: new AccountDetail(
                    Email: $"{label}@example.com", Plan: null, SignedInUntil: null, SignIn: null, Warm: null,
                    ReadAt: null, Read: null, Notes: []),
                Help: null, Limits: [], Actions: []),
            inUse ? null : new ItemAction("Switch", $"Switch to {label}", new Intent.SwitchTo(Key(provider, label))));

    // What pressing it does, and the button at its row's end that does it.
    public static AccountItem Acting(AccountItem item, ItemAction? action) =>
        action is null ? item with { Action = null } : item with { Action = action, End = new RowEnd.Button(action) };

    // pitboard-sites' site for the tool, with the hosts and paths a snapshot carries.
    public static Site Site(string provider) =>
        provider == "codex"
            ? new Site(
                Host: "chatgpt.com", Name: "chatgpt.com", Tool: Pitboard.Core.Tool.Codex,
                Aliases: ["chat.openai.com", "www.chatgpt.com", "chat.com"],
                Hosts: ["chatgpt.com", "chat.openai.com", "www.chatgpt.com", "chat.com"],
                SignInHosts: ["auth.openai.com", "login.live.com", "login.microsoftonline.com", "appleid.apple.com"],
                BlockedHosts: ["accounts.google.com"], SignInPaths: [["api", "auth"]], StoreName: "codex",
                SignInSteps: "", BlockedServices: "")
            : new Site(
                Host: "claude.ai", Name: "claude.ai", Tool: Pitboard.Core.Tool.Claude, Aliases: [], Hosts: ["claude.ai"],
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
            ResetsAt: 1_800_007_799, Resets: "Resets 3:45 PM", ResetsIn: "in 2 hr 9 min",
            Spoken: $"5-hour limit, {percent:0} percent used, resets 3:45 PM");

    // What an account's own menu and expanded row offer to do to it.
    public static AccountAction Action(ActionKind kind, string title, Intent intent, bool enabled = true, Question? confirm = null) =>
        new(kind, title, intent, Window: null, enabled, confirm);

    public static AccountAction RenameAction(string provider, string label) =>
        Action(ActionKind.Rename, "Rename…", new Intent.PresentSheet(new Sheet.Rename(Key(provider, label))));

    public static AccountAction RemoveAction(string provider, string label, bool enabled = true) =>
        Action(
            ActionKind.Remove, "Remove account…", new Intent.Forget(Key(provider, label)), enabled,
            new Question(
                "Remove “spare”?", "Pitboard deletes its saved sign-in, so using it again needs a sign-in in your browser.", "Remove",
                Destructive: true, ConfirmIsDefault: true));

    public static AccountAction WindowAction(WindowAccount window) =>
        new(ActionKind.OpenWindow, $"Open {window.Site.Name}", Intent: null, window, Enabled: true, Confirm: null);

    public static PanelNotice Notice(string id, params NoticeAction[] actions) =>
        new(
            Id: id, Severity: Severity.Info, SpokenSeverity: "Note", Title: "Switched to work",
            Lines: ["Any codex session started before this switch keeps using personal until it is quit and started again."],
            Until: null, UntilLabel: null,
            Actions: actions, Learn: null);

    public static NextStep LearnMore =>
        new NextStep.Learn(new SetupLink("Learn more", "https://docs.usepitboard.com/reference/errors"));

    public static NoticeAction Dismiss(string provider) =>
        new(Title: "Dismiss", Intent: new Intent.DismissSwitch(ToolOf(provider)), Dismisses: true, Switches: false, Enabled: true, Confirm: null);

    public static AccountSection Section(string provider, string? heading, params AccountItem[] accounts) =>
        new(Id: provider, Tool: ToolOf(provider), Heading: heading, Accounts: accounts);

    public static SheetTool Tool(string code, string name, Missing? missing = null) =>
        new(
            Tool: ToolOf(code), Code: code, Name: name,
            Message: $"Pitboard opens {name}’s sign-in in your browser, where you sign in to the account you’re adding.",
            Confirm: "Sign in…", Missing: missing, Steps: []);

    public static Missing CodexCliMissing() =>
        new(
            Title: "Pitboard can’t find the Codex CLI on this PC.",
            Detail: "Adding a Codex account runs the Codex CLI’s sign-in, not the copy inside the ChatGPT app. Install the CLI, then come back to this window.",
            LinkTitle: "How to install the Codex CLI", Link: "https://developers.openai.com/codex/cli");

    public static SheetText AddAccount(params SheetTool[] tools) =>
        new(
            Title: "Add account",
            Message: "Pitboard opens Claude Code’s sign-in in your browser, where you sign in to the account you’re adding.",
            Tools: tools, Tool: Pitboard.Core.Tool.Claude, Account: null, Name: "", Prompt: "work", Confirm: "Sign in…", Saving: false,
            Progress: null);

    public static SheetText Rename(string label) =>
        new(
            Title: $"Rename “{label}”", Message: "Only its name changes, here and in the command line.", Tools: [],
            Tool: Pitboard.Core.Tool.Claude,
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
            Message: found ?? "Clean up deletes the sign-in file Claude Code left behind Credential Manager, and keeps any sign-in an account needs.",
            Looking: found is null ? "Finding out whose sign-in it is…" : null, Confirm: "Clean up",
            CanConfirm: found is not null, Saving: false);
}
