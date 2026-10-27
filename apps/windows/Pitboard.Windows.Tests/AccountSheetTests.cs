using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AccountSheetTests
{
    private const string Address = "https://claude.com/cai/oauth/authorize?code=true&state=pitboard-fixture";

    private static readonly Sheet Add = new Sheet.Add(Provider: null);

    private static readonly WindowQuestion QuitChatGPT = new(
        new Question("Quit ChatGPT to sign in?", "ChatGPT signs in again.", "Quit ChatGPT and continue", Destructive: false, ConfirmIsDefault: true),
        Confirmed: new Intent.QuitAndSwitch("codex/spare"),
        Declined: new Intent.KeepAppOpen());

    // The account just added, which the model saved under the name it starts on.
    private static readonly Snapshot Naming = Snapshots.Read(1) with
    {
        Sheet = new Sheet.Name("claude", "work", "dana@work.example"),
        SheetText = Items.Rename("work") with { Title = "Name this account", Confirm = "Save" },
    };

    // A new account is named once it is added, so the sheet asks for the tool alone.
    [UITestMethod]
    public async Task TheSheetForANewAccountSignsInToThePickedTool()
    {
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex")),
        });
        var picker = up.Element<RadioButtons>("sheet.tool");
        Assert.AreEqual("Add account", up.Sheet.Title);
        Assert.AreEqual(0, picker.SelectedIndex);
        Assert.IsFalse(up.Visible("sheet.name"), "a name asked for before the sign-in");
        Assert.IsTrue(up.Sheet.IsPrimaryButtonEnabled);

        picker.SelectedIndex = 1;
        Views.Press(up.Part("PrimaryButton"));

        Assert.Contains("Pitboard opens Codex’s sign-in in your browser, where you sign in to the account you’re adding.", Views.Texts(up.Sheet));
        await Views.Until(() => up.Model.Sent.Contains(new Intent.SignIn("codex", null)), "no sign-in to Codex");
        Assert.AreSame(up.Sheet, up.Dialogs.Open, "the sheet went before the model closed it");
    }

    // The mockup's step 3: the account is saved already, so Done is the one button and Cancel would cancel nothing.
    [UITestMethod]
    public async Task TheNameStepAfterASignInOffersDoneAlone()
    {
        await using var up = await Up.Over(Naming);

        Assert.AreEqual("Save", up.Sheet.PrimaryButtonText);
        Assert.AreEqual("", up.Sheet.CloseButtonText);
        Assert.AreEqual("", up.Sheet.SecondaryButtonText);
    }

    // Save was dimmed under an empty field, with nothing to say why.
    [UITestMethod]
    public async Task TheNameFieldSaysUnderItselfThatSaveWaitsForAName()
    {
        await using var up = await Up.Over(Naming);
        var field = up.Element<TextBox>("sheet.name");

        Assert.AreEqual("work", field.Text, "the name it was added under");
        Assert.IsNull(Views.Description(field));
        Assert.IsTrue(AutomationProperties.GetIsRequiredForForm(field));
        Assert.IsTrue(up.Sheet.IsPrimaryButtonEnabled, "Save keeps the name it has");

        await up.Type("sheet.name", "");

        Assert.AreEqual("Type a name for this account.", Views.Description(field));
        Assert.IsFalse(up.Sheet.IsPrimaryButtonEnabled);
    }

    // The dialog focuses its first control as it loads, at times after Opened, which took the focus from the name field.
    [UITestMethod]
    public async Task OnlyTheNameFieldTakesFocusAsTheSheetOpens()
    {
        var given = new List<DependencyObject>();
        void Getting(object? sender, GettingFocusEventArgs args) => given.Add(args.NewFocusedElement);
        FocusManager.GettingFocus += Getting;
        try
        {
            await using var up = await Up.Over(Naming);
            var name = up.Element<TextBox>("sheet.name");

            await Views.Until(() => given.Contains(name), "the name field took no focus");
            await Views.Drained();

            Assert.AreSame(name, FocusManager.GetFocusedElement(up.Sheet.XamlRoot));
            var elsewhere = given.Where(element => element != name && Look.Ancestor<AccountSheet>(element) == up.Sheet).ToArray();
            Assert.IsEmpty(elsewhere, $"focus went to {string.Join(", ", elsewhere.Select(element => element.GetType().Name))}");
        }
        finally
        {
            FocusManager.GettingFocus -= Getting;
        }
    }

    [UITestMethod]
    public async Task TheNameFieldOffersOnlyANameTheAccountDoesNotHave()
    {
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = new Sheet.Rename("claude", "personal"),
            SheetText = Items.Rename("personal"),
        });
        Assert.AreEqual("personal", up.Element<TextBox>("sheet.name").Text);
        Assert.IsFalse(up.Sheet.IsPrimaryButtonEnabled, "the name it has");

        await up.Type("sheet.name", "  ");
        Assert.IsFalse(up.Sheet.IsPrimaryButtonEnabled, "white space");
        await up.Type("sheet.name", "home");
        Views.Press(up.Part("PrimaryButton"));

        await Views.Until(() => up.Model.Sent.Contains(new Intent.Rename("claude", "personal", "home")), "no rename to home");
    }

    // Microsoft's text box guidance: text that is never editable is a TextBlock, not a read-only box that looks like a field.
    [UITestMethod]
    public async Task SigningInAgainShowsTheAccountAsText()
    {
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = new Sheet.SignInAgain("claude", "work"),
            SheetText = Items.AddAccount() with { Title = "Sign in to work again", Account = "work" },
        });
        Assert.IsFalse(up.Visible("sheet.name"), "a name field for an account that has one");
        Assert.IsFalse(Views.Visible<TextBox>(up.Sheet).Any(field => field.Text == "work"), "the account in a field");
        var account = Views.Visible<TextBlock>(up.Sheet).FirstOrDefault(text => text.Text == "work");
        Assert.IsNotNull(account, "the account it signs in to");
        Assert.IsTrue(account.IsTextSelectionEnabled, "the account cannot be copied");

        Views.Press(up.Part("PrimaryButton"));

        await Views.Until(() => up.Model.Sent.Contains(new Intent.SignIn("claude", "work")), "no sign-in as work");
    }

    // Saying what went wrong where the name typed is still there to correct.
    [UITestMethod]
    public async Task AFailureIsSaidInTheSheetWithTheNameStillInIt()
    {
        var renaming = Snapshots.Read(1) with { Sheet = new Sheet.Rename("claude", "personal"), SheetText = Items.Rename("personal") };
        await using var up = await Up.Over(renaming);
        await up.Type("sheet.name", "work");
        var failure = new FailureText(7, "Couldn’t rename personal", "work is already the name of another account.", [], Items.LearnMore);

        up.App.Apply(renaming with { Revision = 2, SheetFailure = failure });

        var bar = up.Element<InfoBar>("sheet.failure");
        await Views.Until(() => bar.IsOpen, "the failure was not said");
        Assert.AreEqual("Couldn’t rename personal", bar.Title);
        Assert.AreEqual("work", up.Element<TextBox>("sheet.name").Text);
    }

    // Plan 3: what went wrong ends with what to do, an action in the app or the docs about it, as the bar's own button.
    [UITestMethod]
    public async Task AFailureInTheSheetEndsWithItsNextStep()
    {
        var renaming = Snapshots.Read(1) with { Sheet = new Sheet.Rename("claude", "personal"), SheetText = Items.Rename("personal") };
        await using var up = await Up.Over(renaming);
        var bar = up.Element<InfoBar>("sheet.failure");
        up.App.Apply(renaming with
        {
            Revision = 2,
            SheetFailure = new FailureText(7, "Couldn’t rename personal", "work is already the name of another account.", [], Items.LearnMore),
        });
        await Views.Until(() => bar.IsOpen, "the failure was not said");

        var learn = Assert.IsInstanceOfType<HyperlinkButton>(bar.ActionButton);
        Assert.AreEqual("Learn more", learn.Content);
        new HyperlinkButtonAutomationPeer(learn).Invoke();
        CollectionAssert.AreEqual(new[] { new Uri("https://docs.usepitboard.com/reference/errors") }, up.Opened);

        var refresh = new Choice("Refresh", new Intent.Refresh(Asked: true), Enabled: true);
        up.App.Apply(renaming with
        {
            Revision = 3,
            SheetFailure = new FailureText(8, "Couldn’t rename personal", "Pitboard stopped before it could say.", [], new NextStep.Act(refresh)),
        });
        await Views.Until(() => bar.ActionButton is Button { Content: "Refresh" }, "the failure's action was not offered");
        Views.Press((Button)bar.ActionButton);

        await Views.Until(() => up.Model.Sent.Contains(refresh.Intent), "the failure's action was not taken");
    }

    // Microsoft's InfoBar guidance closes and opens the bar again for a new message, which Narrator hears as the bar opens.
    [UITestMethod]
    public async Task ASecondFailureReopensItsInfoBar()
    {
        var renaming = Snapshots.Read(1) with { Sheet = new Sheet.Rename("claude", "personal"), SheetText = Items.Rename("personal") };
        await using var up = await Up.Over(renaming);
        var bar = up.Element<InfoBar>("sheet.failure");
        up.App.Apply(renaming with
        {
            Revision = 2,
            SheetFailure = new FailureText(7, "Couldn’t rename personal", "work is already the name of another account.", [], Items.LearnMore),
        });
        await Views.Until(() => bar.IsOpen, "the failure was not said");
        var opened = new List<bool>();
        bar.RegisterPropertyChangedCallback(InfoBar.IsOpenProperty, (_, _) => opened.Add(bar.IsOpen));

        up.App.Apply(renaming with
        {
            Revision = 3,
            SheetFailure = new FailureText(8, "Couldn’t rename personal", "home is already the name of another account.", [], Items.LearnMore),
        });

        await Views.Until(() => bar.Message == "home is already the name of another account.", "the second failure was not said");
        Assert.AreEqual("closed, opened", string.Join(", ", opened.Select(open => open ? "opened" : "closed")));
    }

    // The sign-in ended back at the form, where Enter tries again.
    [UITestMethod]
    public async Task AfterAFailedSignInSignInIsTheDefault()
    {
        var signingIn = Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(),
            SigningIn = Items.Running(Address),
            SigningInText = Items.SigningIn("Claude Code"),
        };
        await using var up = await Up.Over(signingIn);

        up.App.Apply(signingIn with
        {
            Revision = 2,
            SigningIn = null,
            SigningInText = null,
            SheetFailure = new FailureText(
                9, "Couldn’t add the account", "The sign-in did not finish, so nothing was enrolled.", [], Items.LearnMore),
        });

        var bar = up.Element<InfoBar>("sheet.failure");
        await Views.Until(() => bar.IsOpen, "the failure was not said");
        Assert.AreEqual(ContentDialogButton.Primary, up.Sheet.DefaultButton);
        Assert.IsTrue(up.Sheet.IsPrimaryButtonEnabled, "Sign in again");
    }

    // Microsoft's InfoBar guidance gives an error its own bar, where a line of plain text was.
    [UITestMethod]
    public async Task ARefusedCodeIsAnErrorInfoBar()
    {
        var refused = "Claude Code didn’t take that code. Copy the whole code your browser shows, and paste it again.";
        var signingIn = Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(),
            SigningIn = Items.Running(Address, wantsCode: true),
            SigningInText = Items.SigningIn("Claude Code"),
        };
        await using var up = await Up.Over(signingIn);
        var bar = up.Element<InfoBar>("sheet.refused");
        Assert.IsFalse(bar.IsOpen, "a refusal before any code");

        up.App.Apply(signingIn with { Revision = 2, SigningInText = Items.SigningIn("Claude Code") with { Refused = refused } });

        await Views.Until(() => bar.IsOpen, "the refusal was not said");
        Assert.AreEqual(InfoBarSeverity.Error, bar.Severity);
        Assert.AreEqual(refused, bar.Message);
        Assert.IsFalse(bar.IsClosable);
    }

    // A tool Pitboard did not find says what it needs and links to how, and cannot be signed in to.
    [UITestMethod]
    public async Task AToolPitboardDidNotFindSaysWhatToInstall()
    {
        var missing = Items.CodexCliMissing();
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex", missing)),
        });
        var bar = up.Element<InfoBar>("sheet.missing");
        Assert.IsFalse(bar.IsOpen, "Claude Code is here");

        up.Element<RadioButtons>("sheet.tool").SelectedIndex = 1;
        await Views.Until(() => bar.IsOpen, "the missing CLI was not said");
        // The bar's link joins the tree as the opened bar is laid out.
        up.Sheet.UpdateLayout();

        Assert.AreEqual(missing.Title, bar.Title);
        Assert.IsFalse(up.Sheet.IsPrimaryButtonEnabled, "Sign in to a tool not here");
        new Microsoft.UI.Xaml.Automation.Peers.HyperlinkButtonAutomationPeer(up.Element<HyperlinkButton>("sheet.missing.link")).Invoke();
        CollectionAssert.AreEqual(new[] { new Uri(missing.Link) }, up.Opened);
    }

    // The dialog was as wide as its widest line, so it changed width with the tool.
    [UITestMethod]
    public async Task TheSheetKeepsOneWidthWhateverItShows()
    {
        var adding = Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex")),
        };
        await using var up = await Up.Over(adding);
        var widths = new List<double> { up.Width() };

        up.Element<RadioButtons>("sheet.tool").SelectedIndex = 1;
        widths.Add(up.Width());
        up.App.Apply(adding with
        {
            Revision = 2,
            SheetText = Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex", Items.CodexCliMissing())),
        });
        await Views.Until(() => up.Element<InfoBar>("sheet.missing").IsOpen, "the missing CLI was not said");
        widths.Add(up.Width());

        var widest = (double)Application.Current.Resources["ContentDialogMaxWidth"];
        CollectionAssert.AreEqual(new[] { widest, widest, widest }, widths, "Claude Code, Codex, then Codex missing");
    }

    // Microsoft's combo box guidance: fewer than five items to pick one of are radio buttons.
    [UITestMethod]
    public async Task TheToolsAreRadioButtons()
    {
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex")),
        });
        var picker = up.Element<RadioButtons>("sheet.tool");
        await Views.Until(() => Views.Descendants(picker).OfType<RadioButton>().Count() == 2, "the tools were not shown");
        var claude = up.Element<RadioButton>("sheet.tool.claude");
        var codex = up.Element<RadioButton>("sheet.tool.codex");
        Assert.AreEqual("Tool", picker.Header);
        Assert.AreEqual("Claude Code", claude.Content);
        Assert.IsTrue(claude.IsChecked);

        ((ISelectionItemProvider)new RadioButtonAutomationPeer(codex)).Select();

        await Views.Until(() => picker.SelectedIndex == 1, "Codex was not picked");
        Assert.Contains("Pitboard opens Codex’s sign-in in your browser, where you sign in to the account you’re adding.", Views.Texts(up.Sheet));
    }

    // A code is no word to check or predict, and what to paste is said under the field, as a field's description.
    [UITestMethod]
    public async Task TheCodeFieldChecksNoSpellingAndHasItsNoteAsItsDescription()
    {
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(),
            SigningIn = Items.Running(Address, wantsCode: true),
            SigningInText = Items.SigningIn("Claude Code"),
        });
        var field = up.Element<TextBox>("sheet.code");

        Assert.IsFalse(field.IsSpellCheckEnabled);
        Assert.IsFalse(field.IsTextPredictionEnabled);
        Assert.AreEqual(
            "Claude Code takes the code shown after you sign in, whether or not your browser came back to it.", Views.Description(field));
    }

    // TextBox's template shows a string description on one line, which ran past the dialog's edge.
    [UITestMethod]
    public async Task ACodeNoteLongerThanItsFieldWrapsUnderIt()
    {
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(),
            SigningIn = Items.Running(Address, wantsCode: true),
            SigningInText = Items.SigningIn("Claude Code"),
        });
        var field = up.Element<TextBox>("sheet.code");
        var note = Assert.IsInstanceOfType<TextBlock>(field.Description);
        await Views.Until(() => note.ActualWidth > 0, "the note was not laid out");

        Assert.AreEqual(TextWrapping.Wrap, note.TextWrapping);
        Assert.IsTrue(note.ActualWidth <= field.ActualWidth, $"the note is {note.ActualWidth} wide under a field {field.ActualWidth} wide");
    }

    // The name field says under itself why what is typed cannot be a name, and a failure goes once it changes.
    [UITestMethod]
    public async Task ANameThatCannotBeOneSaysWhyAndAFailureGoesWithAnEdit()
    {
        var renaming = Snapshots.Read(1) with { Sheet = new Sheet.Rename("claude", "personal"), SheetText = Items.Rename("personal") };
        await using var up = await Up.Over(renaming);
        var field = up.Element<TextBox>("sheet.name");

        await up.Type("sheet.name", "a/b");
        Assert.AreEqual("A name can’t contain “/”.", Views.Description(field));
        Assert.IsFalse(up.Sheet.IsPrimaryButtonEnabled);

        var failure = new FailureText(7, "Couldn’t rename personal", "work is already the name of another account.", [], Items.LearnMore);
        up.App.Apply(renaming with { Revision = 2, SheetFailure = failure });
        await Views.Until(() => up.Element<InfoBar>("sheet.failure").IsOpen, "the failure was not said");
        await up.Type("sheet.name", "home");

        Assert.IsNull(Views.Description(field));
        await Views.Until(() => up.Model.Sent.Contains(new Intent.DismissSheetFailure()), "the failure stayed with the name");
    }

    [UITestMethod]
    public async Task TheCodeFieldAppearsOnceTheToolAsksForIt()
    {
        var signingIn = Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(),
            SigningIn = Items.Running(Address),
            SigningInText = Items.SigningIn("Claude Code"),
        };
        await using var up = await Up.Over(signingIn);
        Assert.IsFalse(up.Visible("sheet.code"), "a code field before the tool asks");
        Assert.AreEqual("", up.Sheet.PrimaryButtonText);
        Assert.AreEqual("Waiting for your browser", AutomationProperties.GetName(up.Element<ProgressRing>("Waiting")));

        up.App.Apply(signingIn with { Revision = 2, SigningIn = Items.Running(Address, wantsCode: true) });
        await Views.Until(() => up.Visible("sheet.code"), "no code field once the tool asked");
        Assert.AreEqual("Submit code", up.Sheet.PrimaryButtonText);
        Assert.IsFalse(up.Sheet.IsPrimaryButtonEnabled, "Submit code with no code");
        await up.Type("sheet.code", "fixture-code#state");
        Views.Press(up.Part("PrimaryButton"));

        await Views.Until(() => up.Model.Sent.Contains(new Intent.PasteCode("fixture-code#state")), "the code was not sent");
        Assert.AreEqual("", up.Element<TextBox>("sheet.code").Text);
    }

    // The fixture's address only goes to the opener a test hands over, never to a browser.
    [UITestMethod]
    public async Task TheSignInPageIsOfferedOnceTheToolPrintsIt()
    {
        var signingIn = Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(),
            SigningIn = Items.Running(),
            SigningInText = Items.SigningIn("Claude Code"),
        };
        await using var up = await Up.Over(signingIn);
        Assert.IsFalse(Views.Button(up.Sheet, "Open").IsEnabled, "Open with nothing to open");
        Assert.IsFalse(Views.Button(up.Sheet, "Copy link").IsEnabled);

        up.App.Apply(signingIn with { Revision = 2, SigningIn = Items.Running(Address) });
        await Views.Until(() => Views.Button(up.Sheet, "Open").IsEnabled, "Open stayed dimmed");
        Views.Press(Views.Button(up.Sheet, "Open"));

        Assert.AreEqual(new Uri(Address), up.Opened.Single());
    }

    [UITestMethod]
    public async Task ClosingASignInStopsItAndClosesTheSheet()
    {
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(),
            SigningIn = Items.Running(),
            SigningInText = Items.SigningIn("Codex"),
        });

        Views.Press(up.Part("CloseButton"));

        await Views.Until(() => up.Dialogs.Open is null, "the sheet stayed up");
        CollectionAssert.AreEqual(new Intent[] { new Intent.CancelSignIn(), new Intent.CloseSheet() }, up.Model.Sent.ToArray());
    }

    // As it starts to go: a sheet asked for while it goes would otherwise be refused for the sign-in the model still runs.
    [UITestMethod]
    public async Task ClosingASignInTellsTheModelAsTheSheetStartsToGo()
    {
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(),
            SigningIn = Items.Running(),
            SigningInText = Items.SigningIn("Codex"),
        });
        var upWhenTold = new List<bool>();
        up.Model.Told = _ => upWhenTold.Add(up.Dialogs.Open is not null);

        Views.Press(up.Part("CloseButton"));

        await Views.Until(() => up.Model.Sent.Count == 2, "the model was not told");
        CollectionAssert.AreEqual(new Intent[] { new Intent.CancelSignIn(), new Intent.CloseSheet() }, up.Model.Sent.ToArray());
        Assert.HasCount(2, upWhenTold);
        Assert.IsTrue(upWhenTold.TrueForAll(open => open), "the model heard once the sheet had gone");
        await Views.Until(() => up.Dialogs.Open is null, "the sheet stayed up");
    }

    // A name being saved cannot be withdrawn, so nothing in the sheet can be pressed and it stays up.
    [UITestMethod]
    public async Task ASheetSavingANameCannotBeClosed()
    {
        await using var up = await Up.Over(Snapshots.Read(1) with
        {
            Sheet = new Sheet.Rename("claude", "work"),
            SheetText = Items.Rename("work") with { Saving = true },
        });

        Assert.IsFalse(up.Part("CloseButton").IsEnabled, "Cancel while saving");
        Assert.IsFalse(up.Element<TextBox>("sheet.name").IsEnabled);
        var closing = new TaskCompletionSource();
        up.Sheet.Closing += (_, _) => closing.TrySetResult();
        up.Sheet.Hide();
        await closing.Task.WaitAsync(TimeSpan.FromSeconds(10));

        Assert.AreSame(up.Sheet, up.Dialogs.Open, "the sheet closed while saving");
        Assert.IsEmpty(up.Model.Sent);
    }

    [UITestMethod]
    public async Task TheModelClosingTheSheetTakesItDownWithoutAWord()
    {
        await using var up = await Up.Over(Naming);

        up.App.Apply(Naming with { Revision = 2, Sheet = null, SheetText = null });

        await Views.Until(() => up.Dialogs.Open is null, "the sheet stayed up");
        Assert.IsEmpty(up.Model.Sent);
    }

    [UITestMethod]
    public async Task AnotherSheetIsPutUpAfresh()
    {
        var adding = Snapshots.Read(1) with { Sheet = Add, SheetText = Items.AddAccount() };
        await using var up = await Up.Over(adding);
        var first = up.Sheet;

        up.App.Apply(adding with { Revision = 2, Sheet = new Sheet.Rename("claude", "work"), SheetText = Items.Rename("work") });

        await Views.Until(() => up.Dialogs.Open is AccountSheet next && next != first, "the rename sheet did not replace the first");
        Assert.AreEqual("Rename “work”", up.Sheet.Title);
        Assert.IsEmpty(up.Model.Sent);
    }

    // The model's Sheet records compare by value, so the same sheet keeps its dialog and another account being named is a
    // new one.
    [UITestMethod]
    public async Task SheetsAreToldApartByTheModelsSheet()
    {
        await using var up = await Up.Over(Naming);
        var first = up.Sheet;

        up.App.Apply(Naming with
        {
            Revision = 2,
            Sheet = new Sheet.Name("claude", "work", "dana@work.example"),
            SheetText = Items.Rename("dana"),
        });
        Assert.AreSame(first, up.Dialogs.Open, "the same sheet was put up again");
        up.App.Apply(Naming with { Revision = 3, Sheet = new Sheet.Name("claude", "home", "dana@home.example") });

        await Views.Until(() => up.Dialogs.Open is AccountSheet next && next != first, "the sheet for another login kept its dialog");
        Assert.IsEmpty(up.Model.Sent);
    }

    [UITestMethod]
    public async Task PutAwayWaitsForTheLookAndThenPutsTheLoginAway()
    {
        var stowing = Snapshots.Read(1) with { Sheet = new Sheet.Stow(), StowText = Items.Stow() };
        await using var up = await Up.Over(stowing);
        Assert.Contains("Finding out whose sign-in it is…", Views.Texts(up.Sheet));
        Assert.IsFalse(up.Sheet.IsPrimaryButtonEnabled, "Put away before the look");

        up.App.Apply(stowing with { Revision = 2, StowText = Items.Stow("The file holds work’s login, signed in 2 days ago.") });
        await Views.Until(() => up.Sheet.IsPrimaryButtonEnabled, "Put away stayed dimmed");
        Views.Press(up.Part("PrimaryButton"));

        Assert.Contains("The file holds work’s login, signed in 2 days ago.", Views.Texts(up.Sheet));
        await Views.Until(() => up.Model.Sent.Contains(new Intent.Stow()), "nothing was put away");
    }

    // A window shows one ContentDialog at a time, so the question the sign-in asks is the sheet's, where it was a bar inside it.
    [UITestMethod]
    public async Task AQuestionDuringASignInTakesTheDialogOver()
    {
        var signingIn = Snapshots.Read(1) with
        {
            Sheet = new Sheet.Add(Provider: "codex"),
            SheetText = Items.AddAccount(),
            SigningIn = Items.Running(),
            SigningInText = Items.SigningIn("Codex"),
        };
        await using var up = await Up.Over(signingIn);

        up.App.Apply(signingIn with { Revision = 2, QuitQuestion = QuitChatGPT });

        await Views.Until(() => up.Sheet.Title as string == "Quit ChatGPT to sign in?", "the question did not take the sheet over");
        Assert.AreSame(up.Sheet, up.Dialogs.Open, "a second dialog");
        Assert.AreEqual("question", AutomationProperties.GetAutomationId(up.Sheet));
        Assert.Contains("ChatGPT signs in again.", Views.Texts(up.Sheet));
        Assert.DoesNotContain("Finish signing in in your browser. This closes once Codex says you’re in.", Views.Texts(up.Sheet));
        Assert.AreEqual("Quit ChatGPT and continue", up.Sheet.PrimaryButtonText);
        Assert.AreEqual(ContentDialogButton.Primary, up.Sheet.DefaultButton);
        Assert.AreEqual("Cancel", up.Sheet.CloseButtonText);

        Views.Press(up.Part("PrimaryButton"));

        await Views.Until(() => up.Model.Sent.Contains(new Intent.QuitAndSwitch("codex/spare")), "the answer did not reach the model");
        Assert.AreSame(up.Sheet, up.Dialogs.Open, "the sheet went with the question");
    }

    // Cancel declines the question, and the step it was asked over comes back as it was left.
    [UITestMethod]
    public async Task CancelingAQuestionInTheSheetGivesTheStepBack()
    {
        var renaming = Snapshots.Read(1) with
        {
            Sheet = new Sheet.Rename("claude", "personal"),
            SheetText = Items.Rename("personal"),
            QuitQuestion = QuitChatGPT,
        };
        await using var up = await Up.Over(renaming);
        await Views.Until(() => up.Sheet.Title as string == "Quit ChatGPT to sign in?", "the question did not take the sheet over");

        Views.Press(up.Part("CloseButton"));

        await Views.Until(() => up.Model.Sent.Contains(new Intent.KeepAppOpen()), "Cancel did not decline the question");
        Assert.AreSame(up.Sheet, up.Dialogs.Open, "the sheet went with the question");
        Assert.DoesNotContain(new Intent.CloseSheet(), up.Model.Sent.ToArray());

        up.App.Apply(renaming with { Revision = 2, QuitQuestion = null });

        await Views.Until(() => up.Sheet.Title as string == "Rename “personal”", "the step did not come back");
        Assert.AreEqual("sheet", AutomationProperties.GetAutomationId(up.Sheet));
        Assert.AreEqual("personal", up.Element<TextBox>("sheet.name").Text);
        Assert.AreEqual("Rename", up.Sheet.PrimaryButtonText);
    }

    // A failure of something done elsewhere, such as a switch from the icon's menu, is read in the sheet and then put away.
    [UITestMethod]
    public async Task AFailureDuringASheetIsReadInItAndOkGivesTheStepBack()
    {
        var naming = Naming with { FailureAlert = new AlertText("Couldn’t switch to spare", "Claude Code is signing in.", null) };
        await using var up = await Up.Over(naming);
        await Views.Until(() => up.Sheet.Title as string == "Couldn’t switch to spare", "the failure did not take the sheet over");
        Assert.AreEqual("failure", AutomationProperties.GetAutomationId(up.Sheet));
        Assert.Contains("Claude Code is signing in.", Views.Texts(up.Sheet));
        Assert.AreEqual("OK", up.Sheet.CloseButtonText);
        Assert.AreEqual("", up.Sheet.PrimaryButtonText);

        Views.Press(up.Part("CloseButton"));

        await Views.Until(() => up.Model.Sent.Contains(new Intent.DismissFailure()), "OK did not put the failure away");
        Assert.AreSame(up.Sheet, up.Dialogs.Open, "the sheet went with the failure");
        up.App.Apply(naming with { Revision = 2, FailureAlert = null });
        await Views.Until(() => up.Sheet.Title as string == "Name this account", "the step did not come back");
    }

    // Plan 3: a failure said in the sheet ends with its next step there too.
    [UITestMethod]
    public async Task AFailureDuringASheetOffersItsNextStepInIt()
    {
        var signIn = new Choice("Sign in again…", new Intent.PresentSheet(new Sheet.SignInAgain("claude", "spare")), Enabled: true);
        var failed = new AlertText("Couldn’t switch to spare", "spare’s saved sign-in no longer works.", new NextStep.Act(signIn));
        var naming = Naming with { FailureAlert = failed };
        await using var up = await Up.Over(naming);
        await Views.Until(() => up.Sheet.Title as string == "Couldn’t switch to spare", "the failure did not take the sheet over");
        Assert.AreEqual("Sign in again…", up.Sheet.PrimaryButtonText);
        Assert.AreEqual(ContentDialogButton.Primary, up.Sheet.DefaultButton);
        Assert.AreEqual("Close", up.Sheet.CloseButtonText);
        Assert.AreEqual(Visibility.Collapsed, up.Element<HyperlinkButton>("failure.learn").Visibility);

        Views.Press(up.Part("PrimaryButton"));
        await Views.Until(() => up.Model.Sent.Count == 2, "the next step was not taken");
        CollectionAssert.AreEqual(new Intent[] { new Intent.DismissFailure(), signIn.Intent }, up.Model.Sent.ToArray());

        up.App.Apply(naming with { Revision = 2, FailureAlert = failed with { Message = "Claude Code is signing in.", Next = Items.LearnMore } });
        var learn = up.Element<HyperlinkButton>("failure.learn");
        await Views.Until(() => learn.Visibility == Visibility.Visible, "the docs were not offered");
        Assert.AreEqual("", up.Sheet.PrimaryButtonText);
        Assert.AreEqual(ContentDialogButton.None, up.Sheet.DefaultButton);
        new HyperlinkButtonAutomationPeer(learn).Invoke();

        CollectionAssert.AreEqual(new[] { new Uri("https://docs.usepitboard.com/reference/errors") }, up.Opened);
    }

    // The sheet over a window's content, up once it is in front, and taken down after each test, since one dialog shows at a time.
    private sealed class Up : IAsyncDisposable
    {
        private Up(StandInModel model, AppModel app, DialogHost dialogs, WindowSheets sheets, List<Uri> opened)
        {
            (Model, App, Dialogs, Sheets, Opened) = (model, app, dialogs, sheets, opened);
        }

        public StandInModel Model { get; }

        public AppModel App { get; }

        public DialogHost Dialogs { get; }

        public WindowSheets Sheets { get; }

        public List<Uri> Opened { get; }

        public AccountSheet Sheet => (AccountSheet)Dialogs.Open!;

        // Attached before the dialog lays out, so a test sees what it does as it opens.
        public static async Task<Up> Over(Snapshot snapshot, Action<AccountSheet>? watch = null)
        {
            var model = new StandInModel(snapshot);
            var app = new AppModel(model);
            var content = await Views.Shown(new Grid());
            var dialogs = new DialogHost(() => content.XamlRoot);
            var opened = new List<Uri>();
            var sheets = new WindowSheets(app, dialogs, opened.Add);
            watch?.Invoke(sheets.Shown!);
            var up = new Up(model, app, dialogs, sheets, opened);
            await Views.Until(() => dialogs.Open is AccountSheet { IsLoaded: true }, "no sheet");
            return up;
        }

        public T Element<T>(string automationId)
            where T : DependencyObject =>
            Views.Descendants(Sheet).OfType<T>().FirstOrDefault(found => AutomationProperties.GetAutomationId(found) == automationId)
            ?? Views.Descendants(Sheet).OfType<T>().FirstOrDefault(found => found is FrameworkElement { Name: var name } && name == automationId)
            ?? throw new AssertFailedException($"no {typeof(T).Name} {automationId} in the sheet");

        public bool Visible(string automationId) =>
            Views.Visible<TextBox>(Sheet).Any(found => AutomationProperties.GetAutomationId(found) == automationId);

        // The dialog's own box, its template's part, laid out with what the sheet shows now.
        public double Width()
        {
            Sheet.UpdateLayout();
            return Views.Descendants(Sheet).OfType<Border>().Single(border => border.Name == "BackgroundElement").ActualWidth;
        }

        // The dialog's own buttons, its template's parts. The sheet's InfoBars have a CloseButton of their own.
        public Button Part(string name) =>
            Views.Descendants(Sheet).OfType<Button>().Single(button => button.Name == name && !InInfoBar(button));

        public async Task Type(string automationId, string text)
        {
            var field = Element<TextBox>(automationId);
            var changed = new TaskCompletionSource();
            void Changed(object sender, TextChangedEventArgs args) => changed.TrySetResult();
            field.TextChanged += Changed;
            field.Text = text;
            await changed.Task.WaitAsync(TimeSpan.FromSeconds(10));
            field.TextChanged -= Changed;
        }

        public async ValueTask DisposeAsync()
        {
            Sheets.Dispose();
            await Views.Until(() => Dialogs.Open is null, "the sheet stayed up after its test");
        }

        private bool InInfoBar(DependencyObject element)
        {
            for (var parent = VisualTreeHelper.GetParent(element); parent is not null && parent != Sheet; parent = VisualTreeHelper.GetParent(parent))
            {
                if (parent is InfoBar)
                {
                    return true;
                }
            }

            return false;
        }
    }
}
