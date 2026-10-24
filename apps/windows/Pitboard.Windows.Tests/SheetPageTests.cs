using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class SheetPageTests
{
    private static readonly Sheet Add = new Sheet.Add(Provider: null);

    private static SheetPage? Shown(Snapshot snapshot, SheetPage? before = null) =>
        SheetPage.Of(new AppModel(snapshot), before);

    [TestMethod]
    public void TheSheetForANewAccountSaysWhatThePickedToolDoes()
    {
        var text = Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex"));
        var form = (SheetPage.Form)Shown(Snapshots.Read(1) with { Sheet = Add, SheetText = text })!;

        Assert.AreEqual("Pitboard opens Codex’s own sign-in in your browser.", form.Message("codex"));
        Assert.AreEqual("Sign in", form.Confirm("codex"));
        Assert.AreEqual(text.Message, form.Message("desktop"), "a tool the picker does not offer");
    }

    [TestMethod]
    public void SignInWaitsForANameAndForNothingToBeUnderWay()
    {
        var form = new SheetPage.Form(Add, Items.AddAccount(), Running: false);

        Assert.IsNull(form.Start("claude", " \t"));
        Assert.AreEqual(new Intent.SignIn("codex", "third"), form.Start("codex", "  third\n"));
        Assert.IsNull((form with { Running = true }).Start("claude", "third"), "a second sign-in");
        Assert.IsNull((form with { Text = form.Text with { Saving = true } }).Start("claude", "third"), "while saving");
    }

    [TestMethod]
    public void SignInWaitsForAToolPitboardFoundAndANameThatCanBeOne()
    {
        var missing = Items.CodexCliMissing();
        var form = new SheetPage.Form(
            Add, Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex", missing)), Running: false);

        Assert.AreEqual(missing, form.NotFound("codex"));
        Assert.IsNull(form.NotFound("claude"));
        Assert.IsNull(form.Start("codex", "third"), "a tool Pitboard did not find");
        Assert.IsNull(form.Start("claude", "codex/phu"));
        Assert.AreEqual("A name can’t contain “/”. Choose the tool above.", form.Problem("codex/phu"));
        Assert.AreEqual("A name is one word, with no spaces.", form.Problem("my work"));
        Assert.IsNull(form.Problem(""), "nothing typed is nothing wrong");
        Assert.AreEqual(new Intent.SignIn("claude", "third"), form.Start("claude", "third"));
    }

    // Sign in was dimmed under an empty field whose placeholder read "work", with nothing to say why.
    [TestMethod]
    public void TheNameFieldSaysTheButtonWaitsForAName()
    {
        var form = new SheetPage.Form(Add, Items.AddAccount(), Running: false);
        var rename = new SheetPage.Naming(new Sheet.Rename("claude", "personal"), Items.Rename("personal"));

        Assert.AreEqual("Type a name for this account to sign in.", form.Note("claude", ""));
        Assert.AreEqual("Type a name for this account to sign in.", form.Note("claude", " \t"));
        Assert.IsNull(form.Note("claude", "work"));
        Assert.AreEqual(form.Problem("my work"), form.Note("claude", "my work"));
        Assert.AreEqual("Type a name for this account to rename.", rename.Note(""));
        Assert.IsNull(rename.Note("personal"), "the name it has, which Rename waits to change");
    }

    // Without Claude Code, the field asked for a name while Sign in waited for the tool, which the bar under it says.
    [TestMethod]
    public void TheNameFieldAsksForANameOnlyWhenSignInWaitsForNothingElse()
    {
        var form = new SheetPage.Form(
            Add, Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex", Items.CodexCliMissing())),
            Running: false);

        Assert.IsNull(form.Note("codex", ""), "a tool Pitboard did not find");
        Assert.AreEqual(form.Problem("a/b"), form.Note("codex", "a/b"), "what is wrong with what is typed");
        Assert.IsNull((form with { Running = true }).Note("claude", ""), "a second sign-in");
        Assert.IsNull((form with { Text = form.Text with { Saving = true } }).Note("claude", ""), "while saving");
        Assert.AreEqual("Type a name for this account to sign in.", form.Note("claude", ""));
    }

    [TestMethod]
    public void SigningInAgainIsAsTheAccountItself()
    {
        var again = new SheetPage.Form(new Sheet.SignInAgain("claude", "work"), Items.AddAccount() with { Account = "work" }, Running: false);

        Assert.AreEqual(new Intent.SignIn("claude", "work"), again.Start("claude", ""));
    }

    [TestMethod]
    public void NamingEnrolsAndRenamingRenames()
    {
        var name = new SheetPage.Naming(new Sheet.Name("claude", "dana@work.example"), Items.Rename("") with { Confirm = "Save" });
        var rename = new SheetPage.Naming(new Sheet.Rename("claude", "personal"), Items.Rename("personal"));

        Assert.AreEqual(new Intent.Enrol("claude", "work"), name.Save(" work "));
        Assert.IsNull(name.Save(""));
        Assert.IsNull(rename.Save("personal "), "the name it has");
        Assert.AreEqual(new Intent.Rename("claude", "personal", "home"), rename.Save("home"));
        Assert.IsNull((rename with { Text = rename.Text with { Saving = true } }).Save("home"), "a second save");
    }

    [TestMethod]
    public void ASignInUnderWayShowsInPlaceOfTheForm()
    {
        var running = Items.Running("https://claude.com/cai/oauth/authorize?code=true", wantsCode: true);
        var shown = Shown(Snapshots.Read(1) with
        {
            Sheet = Add,
            SheetText = Items.AddAccount(),
            SigningIn = running,
            SigningInText = Items.SigningIn("Claude Code"),
        });

        var signingIn = (SheetPage.SigningIn)shown!;
        Assert.AreEqual(new Uri("https://claude.com/cai/oauth/authorize?code=true"), signingIn.Page);
        Assert.AreEqual(new Intent.PasteCode("fixture-code#state"), signingIn.Submit("fixture-code#state"));
        Assert.IsNull(signingIn.Submit(""));
        Assert.IsNull((signingIn with { SignIn = running with { WantsCode = false } }).Submit("code"), "a code nobody asked for");
        Assert.IsNull((signingIn with { SignIn = running with { Url = null } }).Page, "before the tool prints it");
    }

    // Back to the form, with what went wrong and the name still in it.
    [TestMethod]
    public void ASignInThatStopsWhileTheSheetStaysGoesBackToTheForm()
    {
        var signingIn = new SheetPage.SigningIn(Items.Running(), Items.SigningIn("Claude Code"));

        var shown = Shown(Snapshots.Read(1) with { Sheet = Add, SheetText = Items.AddAccount() }, signingIn);

        Assert.IsInstanceOfType<SheetPage.Form>(shown);
    }

    [TestMethod]
    public void ASheetOnItsWayOutKeepsWhatItShowed()
    {
        var signingIn = new SheetPage.SigningIn(Items.Running(), Items.SigningIn("Claude Code"));

        Assert.AreSame(signingIn, Shown(Snapshots.Read(1), signingIn));
        Assert.IsNull(Shown(Snapshots.Read(1)));
    }

    [TestMethod]
    public void TheSheetsThatNameShowTheirNameField()
    {
        Assert.IsInstanceOfType<SheetPage.Naming>(
            Shown(Snapshots.Read(1) with { Sheet = new Sheet.Rename("claude", "work"), SheetText = Items.Rename("work") }));
        Assert.IsInstanceOfType<SheetPage.Naming>(
            Shown(Snapshots.Read(1) with { Sheet = new Sheet.Name("claude", "dana@work.example"), SheetText = Items.Rename("") }));
    }

    [TestMethod]
    public void PuttingAwayWaitsForTheLookToFindALoginItCanPutAway()
    {
        var looking = (SheetPage.Stowing)Shown(Snapshots.Read(1) with { Sheet = new Sheet.Stow(), StowText = Items.Stow() })!;
        var found = looking with { Text = Items.Stow("The file holds work’s login.") };

        Assert.IsNull(looking.PutAway());
        Assert.AreEqual(new Intent.Stow(), found.PutAway());
    }

    [TestMethod]
    public void ClosingASignInStopsItThenClosesTheSheet()
    {
        var signingIn = new SheetPage.SigningIn(Items.Running(), Items.SigningIn("Codex"));

        CollectionAssert.AreEqual(new Intent[] { new Intent.CancelSignIn(), new Intent.CloseSheet() }, signingIn.Closed().ToArray());
        CollectionAssert.AreEqual(new Intent[] { new Intent.CloseSheet() }, new SheetPage.Stowing(Items.Stow()).Closed().ToArray());
    }

    // A name or a login being saved cannot be withdrawn, so there is nothing to cancel.
    [TestMethod]
    public void WhatIsBeingSavedKeepsItsSheetUp()
    {
        var form = new SheetPage.Form(Add, Items.AddAccount(), Running: false);
        var signingIn = new SheetPage.SigningIn(Items.Running(), Items.SigningIn("Claude Code"));

        Assert.IsTrue(form.Closable);
        Assert.IsFalse((form with { Text = form.Text with { Saving = true } }).Closable);
        Assert.IsFalse(new SheetPage.Stowing(Items.Stow("work") with { Saving = true }).EscapeCloses);
        Assert.IsFalse((signingIn with { Text = signingIn.Text with { Cancel = signingIn.Text.Cancel with { Enabled = false } } }).Closable);
        var later = signingIn with { Text = signingIn.Text with { EscapeCancels = false } };
        Assert.IsTrue(later.Closable, "its button");
        Assert.IsFalse(later.EscapeCloses, "Esc");
    }
}
