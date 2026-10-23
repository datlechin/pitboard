using FlaUI.Core.AutomationElements;
using FlaUI.Core.Definitions;
using FlaUI.Core.Input;
using FlaUI.Core.Tools;
using FlaUI.Core.WindowsAPI;

namespace Pitboard.UITests;

[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed class AccountsWindowTests
{
    [TestMethod]
    public void TheWindowListsEachAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        launched.OpenWindow();

        launched.Row("claude/work");
        launched.Row("codex/main");
    }

    // Open is never pressed: it would open the fixture's address in a real browser.
    [TestMethod]
    public void AddingAClaudeCodeAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();

        launched.Invoke("Add account");
        Launched.Type(launched.Field("sheet.name"), "third");
        launched.Invoke("Sign in");
        var code = launched.Field("sheet.code");
        Assert.IsTrue(launched.Enables("Open"), "Open stayed dimmed");
        launched.Invoke("Copy link");
        Assert.AreEqual("https://claude.com/cai/oauth/authorize?code=true&state=pitboard-fixture", ClipboardText.Read());
        Launched.Type(code, "fixture-code#state");
        launched.Invoke("Submit code");

        launched.Row("claude/third");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.code")), "the sheet stayed up");
    }

    // A Codex sign-in finishes by itself once the browser is done.
    [TestMethod]
    public void AddingACodexAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        launched.OpenWindow();

        launched.Invoke("Add account");
        launched.Find(found => found.ByAutomationId("sheet.tool.codex"), "sheet.tool.codex").Patterns.SelectionItem.Pattern.Select();
        Launched.Type(launched.Field("sheet.name"), "third");
        launched.Invoke("Sign in");

        launched.Row("codex/third");
    }

    // Sign In stays dimmed while a sign-in runs, so a second one finishing shows the first was stopped.
    [TestMethod]
    public void CancellingASignInAddsNothing()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();

        launched.Invoke("Add account");
        Launched.Type(launched.Field("sheet.name"), "third");
        launched.Invoke("Sign in");
        launched.Field("sheet.code");
        launched.Invoke("Cancel");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.code")), "the sheet stayed up");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("account.claude/third")), "third was added");

        launched.Invoke("Add account");
        Launched.Type(launched.Field("sheet.name"), "third");
        launched.Invoke("Sign in");
        Launched.Type(launched.Field("sheet.code"), "fixture-code#state");
        launched.Invoke("Submit code");

        launched.Row("claude/third");
    }

    [TestMethod]
    public void RenamingAnAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();

        Launched.OpenMenu(launched.Row("claude/personal"));
        launched.Invoke("Rename…");
        var name = launched.Field("sheet.name");
        Launched.Type(name, "work");
        launched.Invoke("Rename");
        launched.Named("Couldn’t rename personal");
        Launched.Type(name, "home");
        launched.Invoke("Rename");

        launched.Row("claude/home");
    }

    [TestMethod]
    public void ForgettingAsksFirst()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();
        var row = launched.Row("claude/personal");

        Launched.OpenMenu(row);
        launched.Invoke("Forget…");
        launched.Named("Forget");
        launched.Invoke("Cancel");
        Assert.IsTrue(row.IsAvailable, "forgotten after Cancel");

        Launched.OpenMenu(row);
        launched.Invoke("Forget…");
        launched.Invoke("Forget");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("account.claude/personal")), "personal is still listed");
    }

    [TestMethod]
    public void TheAccountInUseOffersNoForget()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();

        Launched.OpenMenu(launched.Row("claude/work"));
        launched.Named("Rename…");

        Assert.IsTrue(launched.Gone(found => found.ByName("Forget…")), "the account in use offers Forget…");
        Keyboard.Type(VirtualKeyShort.ESCAPE);
    }

    // The other tests ask for a row's menu through UI Automation.
    [TestMethod]
    public void ARightClickOpensTheRowsMenu()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "onlyOne");
        var window = launched.OpenWindow();
        var row = launched.Row("claude/work");
        Foreground.Require(window);

        launched.RightClick(row);

        launched.Find(found => found.ByAutomationId("window.claude.ai/work"), "work's window item");
        Keyboard.Type(VirtualKeyShort.ESCAPE);
    }

    [TestMethod]
    public void UsingACodexAccountSaysSessionsKeepTheOldOne()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        launched.OpenWindow();

        // Named with its tool, since Claude Code could have a spare too.
        Launched.Within(launched.Row("codex/spare"), found => found.ByName("Use spare (Codex)"), "Use spare (Codex)").AsButton().Invoke();
        var notice = launched.Find(found => found.ByAutomationId("notice.switch/codex"), "notice switch/codex");

        Assert.IsTrue(
            Launched.Lines(notice).Any(line => line.StartsWith("Any codex session started before this switch keeps using main", StringComparison.Ordinal)),
            Launched.Said(notice));
        Launched.Within(notice, found => found.ByAutomationId("CloseButton").Or(found.ByName("Close")), "close button")
            .AsButton().Invoke();
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("notice.switch/codex")), "the notice stayed");
    }

    [TestMethod]
    public void UsingACodexAccountAsksToQuitChatGptFirst()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "chatGPTOpen");
        launched.OpenWindow();
        var spare = launched.Row("codex/spare");

        Launched.Within(spare, found => found.ByName("Use spare (Codex)"), "Use spare (Codex)").AsButton().Invoke();
        launched.Named("Quit ChatGPT to switch?");
        launched.Invoke("Cancel");
        Assert.IsTrue(launched.Gone(found => found.ByName("Quit ChatGPT to switch?")), "the question stayed");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("notice.switch/codex")), "something was switched");

        Launched.Within(spare, found => found.ByName("Use spare (Codex)"), "Use spare (Codex)").AsButton().Invoke();
        launched.Invoke("Quit ChatGPT and switch");

        launched.Find(found => found.ByAutomationId("notice.switch/codex"), "notice switch/codex");
    }

    [TestMethod]
    public void NamingTheAccountInUse()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "unnamed");
        launched.OpenWindow();

        launched.Named("Give this account a name");
        launched.Invoke("Name…");
        Assert.AreEqual("work", launched.Field("sheet.name").Text, "the name dana@work.example suggests");
        launched.Invoke("Save");

        launched.Row("claude/work");
    }

    // macOS has no UI test of this: no world of its fixtures left a login in a file until this one.
    [TestMethod]
    public void PuttingAwayALoginLeftInAFile()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "leftInFile");
        launched.OpenWindow();

        launched.Invoke("Put away…");
        Assert.IsTrue(launched.Enables("Put away"), "Put away stayed dimmed after the look");
        launched.Invoke("Put away");

        launched.Find(found => found.ByAutomationId("notice.stowed"), "notice stowed");
        Assert.IsTrue(launched.Gone(found => found.ByName("Put away…")), "the file is still said to be there");
    }

    // With Codex installed, after the tip to use Pitboard with it.
    [TestMethod]
    public void AnEmptyMachineOffersAnAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "empty");
        launched.OpenWindow();
        launched.Named("Use Pitboard with Codex");
        launched.Invoke("Not now");

        launched.Named("No accounts");
        launched.Invoke("Add account…");

        launched.Field("sheet.name");
    }

    [TestMethod]
    public void GivingUpOnAnInterruptedSwitch()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "stuck");
        launched.OpenWindow();

        launched.Invoke("Give up…");
        launched.Invoke("Give up");
        var notice = launched.Find(found => found.ByAutomationId("notice.abandoned"), "notice abandoned");

        Assert.Contains("Gave up on the interrupted switch", Launched.Lines(notice));
        Assert.Contains("2 logins kept", Launched.Said(notice));
    }

    [TestMethod]
    public void OneAccountIsOfferedASecondUntilNotNow()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "onlyOne");
        launched.OpenWindow();
        launched.Named("Add a second account");

        launched.Invoke("Not now");

        Assert.IsTrue(launched.Gone(found => found.ByName("Add a second account")), "the tip stayed");
        launched.Row("claude/work");
    }

    // The window goes to the accounts, where the sheet comes up with focus in its name field, not the tool picker.
    [TestMethod]
    public void CtrlNAddsAnAccountFromAnyPane()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        launched.Select("pane.activity");
        launched.Named("Switch");

        Foreground.Require(window);
        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_N);
        launched.Field("sheet.name");
        launched.Find(found => found.ByAutomationId("sheet.tool.codex"), "sheet.tool.codex");
        string Focused() => launched.Automation.FocusedElement()?.Properties.AutomationId.ValueOrDefault ?? "";
        Assert.IsTrue(
            Retry.WhileFalse(() => Focused() == "sheet.name", TimeSpan.FromSeconds(5), TimeSpan.FromMilliseconds(100)).Result,
            $"the sheet opened with the focus on '{Focused()}'");
        launched.Find(found => found.ByName("Add account").And(found.ByControlType(ControlType.Text)), "the sheet's title");
        launched.Invoke("Cancel");

        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.name")), "the sheet stayed up");
        Assert.IsTrue(launched.Selected("pane.accounts"), "Ctrl+N left the window on Activity");
        launched.Row("claude/work");
    }

    [TestMethod]
    public void TheFirstLaunchOpensTheWindow()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");

        launched.Window("Pitboard");

        launched.Named("Use Pitboard with Codex");
    }
}
