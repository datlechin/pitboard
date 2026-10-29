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

    // What its close button says, where it has one.
    public virtual string? CloseTitle => Words.Window.Cancel;

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

    public sealed record Form(Sheet Sheet, SheetText Text, bool Running) : SheetPage
    {
        public override bool Closable => !Text.Saving;

        public string Message(Tool tool) => Chosen(tool)?.Message ?? Text.Message;

        public string Confirm(Tool tool) => Chosen(tool)?.Confirm ?? Text.Confirm;

        // Null where Pitboard found the tool.
        public Missing? NotFound(Tool tool) => Chosen(tool)?.Missing;

        // Null while Sign in cannot be pressed. A new account is named once it is added.
        public Intent? Start(Tool tool) =>
            !Text.Saving && !Running && NotFound(tool) is null ? PitboardModelMethods.CheckName(Sheet, tool, "").Intent : null;

        private SheetTool? Chosen(Tool tool) => Text.Tools.FirstOrDefault(offered => offered.Tool == tool);
    }

    public sealed record Naming(Sheet Sheet, SheetText Text) : SheetPage
    {
        public override bool Closable => !Text.Saving;

        // The account a sign-in added is saved under the name the step starts on, so Done is its one button.
        public override string? CloseTitle => Sheet is Sheet.Name ? null : Words.Window.Cancel;

        // Microsoft's forms guidance holds the button back until what it needs is filled in; the field says so meanwhile.
        public string? Problem(string typed) => PitboardModelMethods.CheckName(Sheet, Text.Tool, typed).Problem;

        public Intent? Save(string typed) => Text.Saving ? null : PitboardModelMethods.CheckName(Sheet, Text.Tool, typed).Intent;
    }

    public sealed record SigningIn(RunningSignIn SignIn, SigningInText Text) : SheetPage
    {
        public override bool Closable => Text.Cancel.Enabled;

        public override bool EscapeCloses => Text.Cancel.Enabled && Text.EscapeCancels;

        public override string? CloseTitle => Text.Cancel.Title;

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
