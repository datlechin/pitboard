using Pitboard.Core;

namespace Pitboard.Windows;

public abstract record WindowAlert
{
    private WindowAlert()
    {
    }

    public static WindowAlert? Of(AppModel model) =>
        model.FailureAlert is { } failure ? new Failed(failure)
        : (model.QuitQuestion ?? model.ReplaceQuestion) is { } asked ? new Asking(asked.Question, asked.Confirmed, asked.Declined)
        : null;

    // One ContentDialog shows per thread at a time, and a sheet is one, so its alert is said in the sheet.
    public static bool InSheet(AppModel model) => model.Sheet is not null;

    public abstract IReadOnlyList<Intent> Answered(bool confirmed);

    public sealed record Failed(AlertText Text) : WindowAlert
    {
        // The button that puts it right, where one does; otherwise the docs about it.
        public Choice? Act => (Text.Next as NextStep.Act)?.Choice;

        public SetupLink? Learn => (Text.Next as NextStep.Learn)?.Link;

        // Read either way, and its next step taken where that was chosen.
        public override IReadOnlyList<Intent> Answered(bool confirmed) =>
            confirmed && Act is { } act ? [new Intent.DismissFailure(), act.Intent] : [new Intent.DismissFailure()];
    }

    public sealed record Asking(Question Question, Intent Confirmed, Intent Declined) : WindowAlert
    {
        public override IReadOnlyList<Intent> Answered(bool confirmed) => [confirmed ? Confirmed : Declined];
    }
}
