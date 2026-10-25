using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class WindowAlertTests
{
    private static readonly AlertText Failure = new("Couldn’t switch to spare", "Claude Code is signing in.", null);
    private static readonly WindowQuestion QuitChatGPT = new(
        new Question("Quit ChatGPT to switch?", "ChatGPT signs in again.", "Quit ChatGPT and switch"),
        Confirmed: new Intent.QuitAndSwitch("codex/spare"),
        Declined: new Intent.KeepAppOpen());

    private static AppModel Model(Snapshot snapshot) => new(snapshot);

    [TestMethod]
    public void AFailureIsSaidBeforeAQuestion()
    {
        var model = Model(Snapshots.Read(1) with { FailureAlert = Failure, QuitQuestion = QuitChatGPT });

        Assert.AreEqual(new WindowAlert.Failed(Failure), WindowAlert.Of(model));
    }

    [TestMethod]
    public void TheQuestionSendsTheModelsOwnAnswers()
    {
        var model = Model(Snapshots.Read(1) with { QuitQuestion = QuitChatGPT });
        var alert = WindowAlert.Of(model)!;

        Assert.AreEqual(new Intent.QuitAndSwitch("codex/spare"), alert.Answered(confirmed: true));
        Assert.AreEqual(new Intent.KeepAppOpen(), alert.Answered(confirmed: false));
    }

    [TestMethod]
    public void TheReplaceQuestionNamesTheAccountItPutsInPlace()
    {
        var replace = new WindowQuestion(
            new Question("Replace Claude Code’s login?", "Anthropic no longer accepts it.", "Replace"),
            Confirmed: new Intent.ReplaceAndSwitch("claude/work"),
            Declined: new Intent.KeepLogin());
        var alert = WindowAlert.Of(Model(Snapshots.Read(1) with { ReplaceQuestion = replace }))!;

        Assert.AreEqual(new WindowAlert.Asking(replace.Question, replace.Confirmed, replace.Declined), alert);
        Assert.AreEqual(new Intent.ReplaceAndSwitch("claude/work"), alert.Answered(confirmed: true));
        Assert.AreEqual(new Intent.KeepLogin(), alert.Answered(confirmed: false));
    }

    [TestMethod]
    public void ClosingAFailureDismissesIt()
    {
        Assert.AreEqual(new Intent.DismissFailure(), new WindowAlert.Failed(Failure).Answered(confirmed: false));
    }

    [TestMethod]
    public void NothingIsAskedWhileTheModelAsksNothing()
    {
        Assert.IsNull(WindowAlert.Of(Model(Snapshots.Read(1))));
    }

    // One ContentDialog shows per thread at a time, and the sheet is one.
    [TestMethod]
    public void WhileASheetIsUpItsAlertIsSaidInTheSheet()
    {
        Assert.IsFalse(WindowAlert.InSheet(Model(Snapshots.Read(1))));
        Assert.IsTrue(WindowAlert.InSheet(Model(Snapshots.Read(1) with { Sheet = new Sheet.Add(Provider: null) })));
    }
}
