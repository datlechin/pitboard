using Microsoft.UI.Xaml.Controls;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AlertTests
{
    private static readonly AlertText Failure = new("Couldn’t switch to spare", "Claude Code is signing in.", null);
    private static readonly WindowQuestion QuitChatGPT = new(
        new Question("Quit ChatGPT to switch?", "ChatGPT signs in again.", "Quit ChatGPT and switch", Destructive: false, ConfirmIsDefault: true),
        Confirmed: new Intent.QuitAndSwitch("codex/spare"),
        Declined: new Intent.KeepAppOpen());

    // The window's content, so a dialog has somewhere to show.
    private static async Task<DialogHost> Window()
    {
        var content = await Views.Shown(new Grid());
        return new DialogHost(() => content.XamlRoot);
    }

    [UITestMethod]
    public async Task AFailedSwitchIsADialogWhoseCloseDismissesIt()
    {
        var model = new StandInModel(Snapshots.Read(1) with { FailureAlert = Failure });
        var app = new AppModel(model);
        var dialogs = await Window();
        using var alerts = new WindowAlerts(app, dialogs);

        await Views.Until(() => dialogs.Open is { Title: "Couldn’t switch to spare" }, "no dialog for the failure");
        dialogs.Open!.Hide();

        await Views.Until(() => model.Sent.Contains(new Intent.DismissFailure()), "the failure was not dismissed");
    }

    // The sheet says it, and once the sheet goes the window does.
    [UITestMethod]
    public async Task AnAlertDuringASheetOpensNoSecondDialog()
    {
        var sheet = Snapshots.Read(1) with { Sheet = new Sheet.Add(Provider: null), FailureAlert = Failure };
        var app = new AppModel(new StandInModel(sheet));
        var dialogs = await Window();
        using var alerts = new WindowAlerts(app, dialogs);
        await Views.Drained();

        Assert.IsNull(dialogs.Open, "a dialog over the sheet");

        app.Apply(sheet with { Revision = 2, Sheet = null });

        await Views.Until(() => dialogs.Open is not null, "no dialog once the sheet went");
        dialogs.Open!.Hide();
    }

    [UITestMethod]
    public async Task TheWindowsQuestionAnswersEnterWithItsVerb()
    {
        var model = new StandInModel(Snapshots.Read(1) with { QuitQuestion = QuitChatGPT });
        var app = new AppModel(model);
        var dialogs = await Window();
        using var alerts = new WindowAlerts(app, dialogs);

        await Views.Until(() => dialogs.Open is not null, "no dialog for the question");
        Assert.AreEqual("Quit ChatGPT and switch", dialogs.Open!.PrimaryButtonText);
        Assert.AreEqual(ContentDialogButton.Primary, dialogs.Open.DefaultButton);
        dialogs.Open.Hide();

        await Views.Until(() => model.Sent.Contains(new Intent.KeepAppOpen()), "closing it did not decline");
    }

    [UITestMethod]
    public async Task AnAlertTheModelDropsIsTakenDownUnanswered()
    {
        var first = Snapshots.Read(1) with { QuitQuestion = QuitChatGPT };
        var model = new StandInModel(first);
        var app = new AppModel(model);
        var dialogs = await Window();
        using var alerts = new WindowAlerts(app, dialogs);
        await Views.Until(() => dialogs.Open is not null, "no dialog for the question");

        app.Apply(first with { Revision = 2, QuitQuestion = null });

        await Views.Until(() => dialogs.Open is null, "the question stayed up");
        Assert.IsFalse(model.Sent.Any(intent => intent is Intent.KeepAppOpen or Intent.QuitAndSwitch), "an answer nobody gave");
    }
}
