using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class SheetPageTests
{
    private static readonly Sheet Add = new Sheet.Add(Tool: null);

    private static SheetPage? Shown(Snapshot snapshot, SheetPage? before = null) =>
        SheetPage.Of(new AppModel(snapshot), before);

    [TestMethod]
    public void TheSheetForANewAccountSaysWhatThePickedToolDoes()
    {
        var text = Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex"));
        var form = (SheetPage.Form)Shown(Snapshots.Read(1) with { Sheet = Add, SheetText = text })!;

        Assert.AreEqual("Pitboard opens Codex’s sign-in in your browser, where you sign in to the account you’re adding.", form.Message(Tool.Codex));
        Assert.AreEqual("Sign in…", form.Confirm(Tool.Codex));
        Assert.AreEqual(text.Message, form.Message(Tool.Desktop), "a tool the picker does not offer");
    }

    // A new account is named once it is added, so all Sign in waits for is no other sign-in or save.
    [TestMethod]
    public void SignInWaitsForNothingToBeUnderWay()
    {
        var form = new SheetPage.Form(Add, Items.AddAccount(), Running: false);

        Assert.AreEqual(new Intent.SignIn(Tool.Codex, null), form.Start(Tool.Codex));
        Assert.IsNull((form with { Running = true }).Start(Tool.Claude), "a second sign-in");
        Assert.IsNull((form with { Text = form.Text with { Saving = true } }).Start(Tool.Claude), "while saving");
    }

    [TestMethod]
    public void SignInWaitsForAToolPitboardFound()
    {
        var missing = Items.CodexCliMissing();
        var form = new SheetPage.Form(
            Add, Items.AddAccount(Items.Tool("claude", "Claude Code"), Items.Tool("codex", "Codex", missing)), Running: false);

        Assert.AreEqual(missing, form.NotFound(Tool.Codex));
        Assert.IsNull(form.NotFound(Tool.Claude));
        Assert.IsNull(form.Start(Tool.Codex), "a tool Pitboard did not find");
        Assert.AreEqual(new Intent.SignIn(Tool.Claude, null), form.Start(Tool.Claude));
    }

    // Plan 4.4: the account a sign-in added is already saved under the name its step starts on, so that step has Done
    // alone; a rename keeps Cancel.
    [TestMethod]
    public void OnlyTheNameStepAfterASignInHasNoCancel()
    {
        Assert.IsNull(new SheetPage.Naming(new Sheet.Name(new AccountKey(Tool.Claude, "work"), "dana@work.example"), Items.Rename("work")).CloseTitle);
        Assert.AreEqual("Cancel", new SheetPage.Naming(new Sheet.Rename(new AccountKey(Tool.Claude, "work")), Items.Rename("work")).CloseTitle);
        Assert.AreEqual("Cancel", new SheetPage.Form(Add, Items.AddAccount(), Running: false).CloseTitle);
    }

    // The button was dimmed under an empty field whose placeholder read "work", with nothing to say why.
    [TestMethod]
    public void TheNameFieldSaysTheButtonWaitsForAName()
    {
        var rename = new SheetPage.Naming(new Sheet.Rename(new AccountKey(Tool.Claude, "personal")), Items.Rename("personal"));

        Assert.AreEqual("Type a name for this account.", rename.Problem(""));
        Assert.AreEqual("Type a name for this account.", rename.Problem(" \t"));
        Assert.AreEqual("A name is one word, with no spaces.", rename.Problem("my work"));
        Assert.AreEqual("A name can’t contain “/”.", rename.Problem("codex/phu"));
        Assert.IsNull(rename.Problem("personal"), "the name it has, which Rename waits to change");
    }

    [TestMethod]
    public void SigningInAgainIsAsTheAccountItself()
    {
        var again = new SheetPage.Form(new Sheet.SignInAgain(new AccountKey(Tool.Claude, "work")), Items.AddAccount() with { Account = "work" }, Running: false);

        Assert.AreEqual(new Intent.SignIn(Tool.Claude, "work"), again.Start(Tool.Claude));
    }

    // The account is added under the name it shows already, so Save renames it only where the name changed.
    [TestMethod]
    public void NamingTheAccountJustAddedRenamesItAndRenamingRenames()
    {
        var name = new SheetPage.Naming(
            new Sheet.Name(new AccountKey(Tool.Claude, "work"), "dana@work.example"), Items.Rename("work") with { Confirm = "Save" });
        var rename = new SheetPage.Naming(new Sheet.Rename(new AccountKey(Tool.Claude, "personal")), Items.Rename("personal"));

        Assert.AreEqual(new Intent.Rename(new AccountKey(Tool.Claude, "work"), "office"), name.Save(" office "));
        Assert.AreEqual(new Intent.CloseSheet(), name.Save("work"), "kept as it is");
        Assert.IsNull(name.Save(""));
        Assert.IsNull(rename.Save("personal "), "the name it has");
        Assert.AreEqual(new Intent.Rename(new AccountKey(Tool.Claude, "personal"), "home"), rename.Save("home"));
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

    // Back to the form, with what went wrong in it.
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
            Shown(Snapshots.Read(1) with { Sheet = new Sheet.Rename(new AccountKey(Tool.Claude, "work")), SheetText = Items.Rename("work") }));
        Assert.IsInstanceOfType<SheetPage.Naming>(
            Shown(Snapshots.Read(1) with { Sheet = new Sheet.Name(new AccountKey(Tool.Claude, "work"), "dana@work.example"), SheetText = Items.Rename("work") }));
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
