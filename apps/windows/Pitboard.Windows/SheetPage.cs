using Pitboard.Core;

namespace Pitboard.Windows;

// What the sheet over the main window shows and sends, by the macOS sheets' rules.
public abstract record SheetPage
{
    private SheetPage()
    {
    }

    public abstract bool Closable { get; }

    public virtual bool EscapeCloses => Closable;

    // A sheet on its way out keeps what it showed, so it does not flash its form as it goes.
    public static SheetPage? Of(AppModel model, SheetPage? shown)
    {
        if (model.Sheet is not { } sheet)
        {
            return shown;
        }

        if (sheet is Sheet.Stow)
        {
            return model.StowText is { } stow ? new Stowing(stow) : shown;
        }

        // The model runs one sign-in at a time, so a sign-in sheet shows the one running, whichever sheet started it.
        var signsIn = sheet is Sheet.Add or Sheet.SignInAgain;
        if (signsIn && (model.SigningIn, model.SigningInText) is ({ } running, { } said))
        {
            return new SigningIn(running, said);
        }

        return model.SheetText switch
        {
            { } text when signsIn => new Form(sheet, text, Running: model.SigningIn is not null),
            { } text => new Naming(sheet, text),
            null => shown,
        };
    }

    public virtual IReadOnlyList<Intent> Closed() => [new Intent.CloseSheet()];

    // Microsoft's forms guidance holds the button back until what it needs is filled in; the field says so meanwhile.
    private static string? Needed(string typed, string confirm) =>
        string.IsNullOrWhiteSpace(typed) && confirm.Length > 0
            ? $"Type a name for this account to {char.ToLowerInvariant(confirm[0])}{confirm[1..]}."
            : null;

    public sealed record Form(Sheet Sheet, SheetText Text, bool Running) : SheetPage
    {
        public override bool Closable => !Text.Saving;

        public string Message(string tool) => Chosen(tool)?.Message ?? Text.Message;

        public string Confirm(string tool) => Chosen(tool)?.Confirm ?? Text.Confirm;

        // Null where Pitboard found the tool.
        public Missing? NotFound(string tool) => Chosen(tool)?.Missing;

        // Null while Sign in cannot be pressed. A new account is named once it is added.
        public Intent? Start(string tool) =>
            !Text.Saving && !Running && NotFound(tool) is null ? PitboardFfiMethods.CheckName(Sheet, tool, "").Intent : null;

        private SheetTool? Chosen(string tool) => Text.Tools.FirstOrDefault(offered => offered.Code == tool);
    }

    public sealed record Naming(Sheet Sheet, SheetText Text) : SheetPage
    {
        public override bool Closable => !Text.Saving;

        public string? Problem(string typed) => PitboardFfiMethods.CheckName(Sheet, Text.Tool, typed).Problem;

        public string? Note(string typed) => Problem(typed) ?? Needed(typed, Text.Confirm);

        public Intent? Save(string typed) => Text.Saving ? null : PitboardFfiMethods.CheckName(Sheet, Text.Tool, typed).Intent;
    }

    public sealed record SigningIn(RunningSignIn SignIn, SigningInText Text) : SheetPage
    {
        public override bool Closable => Text.Cancel.Enabled;

        public override bool EscapeCloses => Text.Cancel.Enabled && Text.EscapeCancels;

        // Held back until the tool prints it.
        public Uri? Page => Uri.TryCreate(SignIn.Url, UriKind.Absolute, out var page) ? page : null;

        public override IReadOnlyList<Intent> Closed() => [Text.Cancel.Intent, new Intent.CloseSheet()];

        public Intent? Submit(string code) => SignIn.WantsCode && code.Length > 0 ? new Intent.PasteCode(code) : null;
    }

    public sealed record Stowing(StowText Text) : SheetPage
    {
        public override bool Closable => !Text.Saving;

        public Intent? PutAway() => Text.CanConfirm ? new Intent.Stow() : null;
    }
}
