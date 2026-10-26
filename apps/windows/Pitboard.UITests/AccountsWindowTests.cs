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

    // Open is never pressed: it would open the fixture's address in a real browser. The account is added as the sign-in
    // finishes, under the name its email suggests, and named next.
    [TestMethod]
    public void AddingAClaudeCodeAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();

        launched.Invoke("accounts.add");
        launched.Answer("sheet", "PrimaryButton");
        var code = launched.Field("sheet.code");
        Assert.IsTrue(Launched.Enables(launched.Element("sheet.openLink")), "Open stayed dimmed");
        launched.Invoke("sheet.copyLink");
        Assert.AreEqual("https://claude.com/cai/oauth/authorize?code=true&state=pitboard-fixture", ClipboardText.Read());
        Launched.Type(code, "fixture-code#state");
        launched.Answer("sheet", "PrimaryButton");

        launched.Row("claude/travel");
        var name = launched.Field("sheet.name");
        Assert.AreEqual("travel", name.Text, "the name dana@travel.example suggests");
        Launched.Type(name, "third");
        launched.Answer("sheet", "PrimaryButton");

        launched.Row("claude/third");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.name")), "the sheet stayed up");
    }

    // A Codex sign-in finishes by itself once the browser is done.
    [TestMethod]
    public void AddingACodexAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        launched.OpenWindow();

        launched.Invoke("accounts.add");
        launched.Element("sheet.tool.codex").Patterns.SelectionItem.Pattern.Select();
        launched.Answer("sheet", "PrimaryButton");

        launched.Row("codex/travel");
        launched.Field("sheet.name");
        launched.Answer("sheet", "PrimaryButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.name")), "kept as it is, the sheet stayed up");
    }

    // Sign In stays dimmed while a sign-in runs, so a second one finishing shows the first was stopped.
    [TestMethod]
    public void CancellingASignInAddsNothing()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();

        launched.Invoke("accounts.add");
        launched.Answer("sheet", "PrimaryButton");
        launched.Field("sheet.code");
        launched.Answer("sheet", "CloseButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.code")), "the sheet stayed up");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("account.claude/travel")), "travel was added");

        launched.Invoke("accounts.add");
        launched.Answer("sheet", "PrimaryButton");
        Launched.Type(launched.Field("sheet.code"), "fixture-code#state");
        launched.Answer("sheet", "PrimaryButton");

        launched.Row("claude/travel");
    }

    [TestMethod]
    public void RenamingAnAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();

        Launched.OpenMenu(launched.Row("claude/personal"));
        launched.Invoke("account.rename");
        var name = launched.Field("sheet.name");
        Launched.Type(name, "work");
        launched.Answer("sheet", "PrimaryButton");
        launched.Element("sheet.failure");
        Launched.Type(name, "home");
        launched.Answer("sheet", "PrimaryButton");

        launched.Row("claude/home");
    }

    [TestMethod]
    public void ForgettingAsksFirst()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();
        var row = launched.Row("claude/personal");

        Launched.OpenMenu(row);
        launched.Invoke("account.forget");
        launched.Answer("question", "CloseButton");
        Assert.IsTrue(row.IsAvailable, "forgotten after Cancel");

        Launched.OpenMenu(row);
        launched.Invoke("account.forget");
        launched.Answer("question", "PrimaryButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("account.claude/personal")), "personal is still listed");
    }

    [TestMethod]
    public void TheAccountInUseOffersNoForget()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();

        Launched.OpenMenu(launched.Row("claude/work"));
        launched.Element("account.rename");

        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("account.forget")), "the account in use offers Forget…");
        Keyboard.Type(VirtualKeyShort.ESCAPE);
    }

    // Plan 10 asked this be measured first: a row that is a SettingsExpander keeps the keyboard. Its toggle takes the
    // focus, F2 renames, Space opens it, Ctrl+C copies its email, Tab goes on to its button, and Enter switches.
    [TestMethod]
    public void ARowsKeysActOnItsAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        var personal = launched.Row("claude/personal");
        var toggle = Launched.Within(personal, found => found.ByAutomationId("ExpanderToggleButton"), "personal's toggle");
        Foreground.Require(window);
        string Focused() => launched.Automation.FocusedElement()?.Properties.AutomationId.ValueOrDefault ?? "";
        bool Becomes(Func<bool> condition) => Retry.WhileFalse(condition, TimeSpan.FromSeconds(10), TimeSpan.FromMilliseconds(100)).Result;

        toggle.Focus();
        Keyboard.Type(VirtualKeyShort.F2);
        Assert.AreEqual("personal", launched.Field("sheet.name").Text);
        launched.Answer("sheet", "CloseButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.name")), "the sheet stayed up");

        toggle.Focus();
        Keyboard.Type(VirtualKeyShort.SPACE);
        Launched.Within(personal, found => found.ByAutomationId("detail.copyEmail"), "personal's email");
        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_C);
        Assert.IsTrue(Becomes(() => ClipboardText.Read() == "dana@home.example"), $"Ctrl+C copied '{ClipboardText.Read()}'");

        Keyboard.Type(VirtualKeyShort.TAB);
        Assert.IsTrue(Becomes(() => Focused() == "account.action"), $"Tab left the focus on '{Focused()}'");

        toggle.Focus();
        Keyboard.Type(VirtualKeyShort.ENTER);
        Launched.Within(personal, found => found.ByAutomationId("account.inUse"), "personal in use");
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

        Launched.Within(launched.Row("codex/spare"), found => found.ByAutomationId("account.action"), "spare's switch").AsButton().Invoke();
        var notice = launched.Element("notice.switch/codex");

        Assert.IsGreaterThan(1, Launched.Lines(notice).Count(), $"it says what open sessions keep: {Launched.Said(notice)}");
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

        Launched.Within(spare, found => found.ByAutomationId("account.action"), "spare's switch").AsButton().Invoke();
        launched.Answer("question", "CloseButton");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("question")), "the question stayed");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("notice.switch/codex")), "something was switched");

        Launched.Within(spare, found => found.ByAutomationId("account.action"), "spare's switch").AsButton().Invoke();
        launched.Answer("question", "PrimaryButton");

        launched.Find(found => found.ByAutomationId("notice.switch/codex"), "notice switch/codex");
    }

    [TestMethod]
    public void NamingTheAccountInUse()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "unnamed");
        launched.OpenWindow();

        launched.Invoke("account.action");
        Assert.AreEqual("work", launched.Field("sheet.name").Text, "the name dana@work.example suggests");
        launched.Row("claude/work");
        launched.Answer("sheet", "PrimaryButton");

        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.name")), "the sheet stayed up");
    }

    // macOS has no UI test of this: no world of its fixtures left a login in a file until this one.
    [TestMethod]
    public void PuttingAwayALoginLeftInAFile()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "leftInFile");
        launched.OpenWindow();

        launched.Invoke("notice.action.stow");
        launched.Answer("sheet", "PrimaryButton");

        launched.Element("notice.stowed");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("notice.action.stow")), "the file is still said to be there");
    }

    // With Codex installed, after the tip to use Pitboard with it.
    [TestMethod]
    public void AnEmptyMachineOffersAnAccount()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "empty");
        launched.OpenWindow();
        launched.Tip(Words.UseWithCodex);
        launched.DeclineTip();

        launched.Element("empty");
        launched.Invoke("empty.choice");

        launched.Element("sheet.tool");
    }

    [TestMethod]
    public void GivingUpOnAnInterruptedSwitch()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "stuck");
        launched.OpenWindow();

        launched.Invoke("notice.action.abandonStuckSwitch");
        launched.Answer("question", "PrimaryButton");
        var notice = launched.Element("notice.abandoned");

        // The notice shows before its lines do.
        var said = Retry.WhileFalse(
            () => Launched.Lines(notice).Count() > 1, TimeSpan.FromSeconds(10), TimeSpan.FromMilliseconds(250), ignoreException: true).Result;
        Assert.IsTrue(said, $"it says what was kept: {Launched.Said(notice)}");
    }

    [TestMethod]
    public void OneAccountIsOfferedASecondUntilNotNow()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "onlyOne");
        launched.OpenWindow();
        launched.Tip(Words.AddASecondAccount);

        launched.DeclineTip();

        launched.Row("claude/work");
    }

    // The window goes to the accounts, where the sheet comes up.
    [TestMethod]
    public void CtrlNAddsAnAccountFromAnyPage()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        launched.OpenFromSettings(window, "settings.activity");
        Launched.Within(launched.Element("activity"), found => found.ByControlType(ControlType.ListItem), "a change");

        Foreground.Require(window);
        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_N);
        launched.Find(found => found.ByAutomationId("sheet.tool.codex"), "sheet.tool.codex");
        launched.Answer("sheet", "CloseButton");

        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("sheet.tool")), "the sheet stayed up");
        Assert.IsTrue(launched.Selected("pane.accounts"), "Ctrl+N left the window on Activity");
        launched.Row("claude/work");
    }

    // Plan 4.12: the first launch opens the welcome in place of the window, which Get started opens.
    [TestMethod]
    public void TheFirstLaunchOpensTheWelcomeAlone()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "firstLaunch");

        var welcome = launched.Window(Words.Welcome);

        Assert.IsEmpty(launched.Windows("Pitboard"), "the window opened under the welcome");
        Launched.Within(welcome, found => found.ByAutomationId("welcome.done"), "Get started").AsButton().Invoke();
        launched.Window("Pitboard");
        Assert.IsTrue(Retry.WhileTrue(() => welcome.IsAvailable, TimeSpan.FromSeconds(10)).Result, "the welcome stayed open");
        launched.Tip(Words.UseWithCodex);
    }
}
