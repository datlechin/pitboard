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

    // The window's content, with the bar a sheet carries, so a dialog has somewhere to show.
    private static async Task<(DialogHost Dialogs, AlertBar Bar)> Window(AppModel app)
    {
        var bar = new AlertBar(app);
        var content = await Views.Shown(new Grid { Children = { bar } });
        return (new DialogHost(() => content.XamlRoot), bar);
    }

    [UITestMethod]
    public async Task AFailedSwitchIsADialogWhoseCloseDismissesIt()
    {
        var model = new StandInModel(Snapshots.Read(1) with { FailureAlert = Failure });
        var app = new AppModel(model);
        var (dialogs, _) = await Window(app);
        using var alerts = new WindowAlerts(app, dialogs);

        await Views.Until(() => dialogs.Open is { Title: "Couldn’t switch to spare" }, "no dialog for the failure");
        dialogs.Open!.Hide();

        await Views.Until(() => model.Sent.Contains(new Intent.DismissFailure()), "the failure was not dismissed");
    }

    [UITestMethod]
    public async Task AnAlertDuringASheetIsSaidInTheSheetAndNoSecondDialogOpens()
    {
        var sheet = Snapshots.Read(1) with { Sheet = new Sheet.Add(Provider: null), FailureAlert = Failure };
        var app = new AppModel(new StandInModel(sheet));
        var (dialogs, bar) = await Window(app);
        using var alerts = new WindowAlerts(app, dialogs);
        await Views.Drained();

        Assert.IsNull(dialogs.Open, "a dialog over the sheet");
        Assert.IsTrue(Views.Element<InfoBar>(bar, "alert").IsOpen);
        Assert.Contains("Couldn’t switch to spare", Views.Texts(bar));

        app.Apply(sheet with { Revision = 2, Sheet = null });

        await Views.Until(() => dialogs.Open is not null, "no dialog once the sheet went");
        Assert.IsFalse(Views.Element<InfoBar>(bar, "alert").IsOpen, "the alert said twice");
        dialogs.Open!.Hide();
    }

    [UITestMethod]
    public async Task TheQuestionInTheSheetIsAnsweredThere()
    {
        var model = new StandInModel(Snapshots.Read(1) with
        {
            Sheet = new Sheet.Add(Provider: "codex"),
            QuitQuestion = QuitChatGPT,
        });
        var (_, bar) = await Window(new AppModel(model));

        Views.Press(Views.Button(bar, "Quit ChatGPT and switch"));

        Assert.AreEqual(new Intent.QuitAndSwitch("codex/spare"), model.Sent.Single(intent => intent is Intent.QuitAndSwitch));
    }

    [UITestMethod]
    public async Task AnAlertTheModelDropsIsTakenDownUnanswered()
    {
        var first = Snapshots.Read(1) with { QuitQuestion = QuitChatGPT };
        var model = new StandInModel(first);
        var app = new AppModel(model);
        var (dialogs, _) = await Window(app);
        using var alerts = new WindowAlerts(app, dialogs);
        await Views.Until(() => dialogs.Open is not null, "no dialog for the question");

        app.Apply(first with { Revision = 2, QuitQuestion = null });

        await Views.Until(() => dialogs.Open is null, "the question stayed up");
        Assert.IsFalse(model.Sent.Any(intent => intent is Intent.KeepAppOpen or Intent.QuitAndSwitch), "an answer nobody gave");
    }
}
