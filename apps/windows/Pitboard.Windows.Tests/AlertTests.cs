using Microsoft.UI.Xaml.Automation.Peers;
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
        Confirmed: new Intent.QuitAndSwitch(new AccountKey(Tool.Codex, "spare")),
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
        using var alerts = new WindowAlerts(app, dialogs, _ => { });

        await Views.Until(() => dialogs.Open is { Title: "Couldn’t switch to spare" }, "no dialog for the failure");
        dialogs.Open!.Hide();

        await Views.Until(() => model.Sent.Contains(new Intent.DismissFailure()), "the failure was not dismissed");
    }

    // Plan 3: a failure ends with what to do in the app, as its button.
    [UITestMethod]
    public async Task AFailureOffersItsNextStepAsItsButton()
    {
        var signIn = new Choice("Sign in again…", new Intent.PresentSheet(new Sheet.SignInAgain(new AccountKey(Tool.Claude, "work"))), Enabled: true);
        var model = new StandInModel(Snapshots.Read(1) with { FailureAlert = Failure with { Next = new NextStep.Act(signIn) } });
        var app = new AppModel(model);
        var dialogs = await Window();
        using var alerts = new WindowAlerts(app, dialogs, _ => { });
        await Views.Until(() => dialogs.Open is not null, "no dialog for the failure");

        Assert.AreEqual("Sign in again…", dialogs.Open!.PrimaryButtonText);
        Assert.AreEqual(ContentDialogButton.Primary, dialogs.Open.DefaultButton);
        Assert.AreEqual("Close", dialogs.Open.CloseButtonText, "OK beside the next step reads as taking it");
        Views.Press(Views.Descendants(dialogs.Open).OfType<Button>().Single(button => button.Name == "PrimaryButton"));

        await Views.Until(() => model.Sent.Count == 2, "the next step was not taken");
        CollectionAssert.AreEqual(new Intent[] { new Intent.DismissFailure(), signIn.Intent }, model.Sent.ToArray());
    }

    // Where nothing in the app puts it right, the docs that say what to do, under what it says.
    [UITestMethod]
    public async Task AFailureNothingInTheAppPutsRightLinksToItsDocs()
    {
        var opened = new List<Uri>();
        var model = new StandInModel(Snapshots.Read(1) with { FailureAlert = Failure with { Next = Items.LearnMore } });
        var app = new AppModel(model);
        var dialogs = await Window();
        using var alerts = new WindowAlerts(app, dialogs, opened.Add);
        await Views.Until(() => dialogs.Open is { IsLoaded: true }, "no dialog for the failure");

        Assert.AreEqual("", dialogs.Open!.PrimaryButtonText);
        var learn = Views.Element<HyperlinkButton>(dialogs.Open, "failure.learn");
        Assert.AreEqual("Learn more", learn.Content);
        new HyperlinkButtonAutomationPeer(learn).Invoke();

        CollectionAssert.AreEqual(new[] { new Uri("https://docs.usepitboard.com/reference/errors") }, opened);
        dialogs.Open.Hide();
        await Views.Until(() => model.Sent.Contains(new Intent.DismissFailure()), "the failure was not dismissed");
    }

    // The sheet says it, and once the sheet goes the window does.
    [UITestMethod]
    public async Task AnAlertDuringASheetOpensNoSecondDialog()
    {
        var sheet = Snapshots.Read(1) with { Sheet = new Sheet.Add(Tool: null), FailureAlert = Failure };
        var app = new AppModel(new StandInModel(sheet));
        var dialogs = await Window();
        using var alerts = new WindowAlerts(app, dialogs, _ => { });
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
        using var alerts = new WindowAlerts(app, dialogs, _ => { });

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
        using var alerts = new WindowAlerts(app, dialogs, _ => { });
        await Views.Until(() => dialogs.Open is not null, "no dialog for the question");

        app.Apply(first with { Revision = 2, QuitQuestion = null });

        await Views.Until(() => dialogs.Open is null, "the question stayed up");
        Assert.IsFalse(model.Sent.Any(intent => intent is Intent.KeepAppOpen or Intent.QuitAndSwitch), "an answer nobody gave");
    }
}
