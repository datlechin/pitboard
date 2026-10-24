using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AccountsPageTests
{
    private static readonly Choice Add = new("Add account…", new Intent.PresentSheet(new Sheet.Add(Provider: null)), Enabled: true);

    private static async Task<(AccountsPage Page, StandInModel Model, AppModel App)> Over(Snapshot snapshot)
    {
        var model = new StandInModel(snapshot);
        var app = new AppModel(model);
        return (await Views.Navigated<AccountsPage>(Views.Context(app)), model, app);
    }

    [UITestMethod]
    public async Task AnEmptyMachineOffersAnAccount()
    {
        var (page, model, _) = await Over(Snapshots.Read(1) with
        {
            AccountsShown = new AccountsShown.NoAccounts("No accounts", "Sign in to Claude Code or Codex to add one.", Add),
        });

        Assert.Contains("No accounts", Views.Texts(page));
        Views.Press(Views.Button(page, "Add account…"));

        Assert.Contains(Add.Intent, model.Sent);
    }

    [UITestMethod]
    public async Task AFailedReadSaysWhyAndHoldsItsRetryBack()
    {
        var retry = new Choice("Try again", new Intent.Refresh(Asked: true), Enabled: false);
        var (page, _, _) = await Over(Snapshots.Read(1) with
        {
            AccountsShown = new AccountsShown.ReadFailed("Couldn’t read accounts", "The keychain is locked.", retry),
        });

        Assert.Contains("The keychain is locked.", Views.Texts(page));
        Assert.IsFalse(Views.Button(page, "Try again").IsEnabled);
    }

    [UITestMethod]
    public async Task BeforeTheFirstReadThePageSaysItIsReading()
    {
        var (page, _, _) = await Over(Snapshots.Read(1) with { AccountsShown = new AccountsShown.Reading("Reading accounts…") });

        Assert.Contains("Reading accounts…", Views.Texts(page));
        Assert.IsTrue(Views.Visible<ProgressRing>(page).Single().IsActive);
        Assert.IsFalse(Views.Visible<Button>(page).Any(button => button.Content is string), "something to press while reading");
    }

    [UITestMethod]
    public async Task ShowingThePageReadsAccountsWhoseNumbersAreOld()
    {
        var (page, model, _) = await Over(Snapshots.Read(1));

        Assert.Contains(new Intent.PaneShown(Pane.Accounts), model.Sent);
    }

    [UITestMethod]
    public async Task EachToolsAccountsAreListedUnderItsName()
    {
        var (page, _, _) = await Over(Snapshots.Read(1) with
        {
            Sections =
            [
                Items.Section("claude", "Claude Code", Items.Account("claude", "work", inUse: true)),
                Items.Section("codex", "Codex", Items.Account("codex", "spare")),
            ],
        });

        var texts = Views.Texts(page);
        Assert.Contains("Claude Code", texts);
        Assert.Contains("Codex", texts);
        Views.Element<ListViewItem>(page, "account.claude/work");
        Views.Element<ListViewItem>(page, "account.codex/spare");
    }

    // A ListView over a CollectionViewSource follows the view's current item, which starts on a row, so twoTools opened
    // with a row selected that nobody chose.
    [UITestMethod]
    public async Task NoRowIsSelectedUntilOneIsChosen()
    {
        var two = Snapshots.Read(1) with
        {
            Sections =
            [
                Items.Section("claude", "Claude Code", Items.Account("claude", "work", inUse: true), Items.Account("claude", "old")),
                Items.Section("codex", "Codex", Items.Account("codex", "spare")),
            ],
        };
        var (page, _, app) = await Over(two);
        var list = Views.Element<ListView>(page, "accounts");
        await Views.Drained();
        Assert.IsNull(list.SelectedItem);

        app.Apply(two with
        {
            Revision = 2,
            Sections =
            [
                Items.Section("claude", "Claude Code", Items.Account("claude", "old"), Items.Account("claude", "work", inUse: true)),
                Items.Section("codex", "Codex", Items.Account("codex", "spare")),
            ],
        });
        await Views.Drained();

        Assert.IsNull(list.SelectedItem);
    }

    // The model makes every list anew for each snapshot, and the frame keeps the page while another shows; the row the
    // person selected stays selected.
    [UITestMethod]
    public async Task ASelectedRowStaysSelectedAcrossSnapshotsAndPages()
    {
        var first = Snapshots.Read(1) with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare"))],
        };
        var (page, _, app) = await Over(first);
        var list = Views.Element<ListView>(page, "accounts");
        list.SelectedIndex = 1;
        var selected = (ShownAccount)list.SelectedItem;

        app.Apply(first with
        {
            Revision = 2,
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare") with { Title = "spare 12%" })],
        });

        Assert.AreSame(selected, list.SelectedItem);
        Assert.AreEqual("spare 12%", selected.Item.Title);

        var frame = page.Frame;
        await Views.Navigate<ActivityPage>(frame, Views.Context(app));
        app.Apply(first with
        {
            Revision = 3,
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare") with { Title = "spare 30%" })],
        });
        var back = await Views.Navigate<AccountsPage>(frame, Views.Context(app));

        Assert.AreSame(page, back, "the frame made the accounts page again");
        Assert.AreSame(selected, list.SelectedItem);
        Assert.AreEqual("spare 30%", selected.Item.Title);
    }

    // A rename keeps the account's id, so the row stays the same object in the same container.
    [UITestMethod]
    public async Task ARenamedRowIsFoundByItsNewName()
    {
        var personal = Items.Account("claude", "personal") with { Id = "claude:same" };
        var first = Snapshots.Read(1) with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), personal)],
        };
        var (page, _, app) = await Over(first);
        Views.Element<ListViewItem>(page, "account.claude/personal");

        app.Apply(first with
        {
            Revision = 2,
            Sections =
            [
                Items.Section(
                    "claude", null, Items.Account("claude", "work", inUse: true),
                    personal with { Qualified = "claude/home", Title = "home" }),
            ],
        });
        page.UpdateLayout();

        Views.Element<ListViewItem>(page, "account.claude/home");
    }

    [UITestMethod]
    public async Task ANoticeAndTheSetupStepComeBeforeTheAccounts()
    {
        var notNow = new Choice("Not now", new Intent.DeclineSecondAccount("claude"), Enabled: true);
        var (page, model, _) = await Over(Snapshots.Read(1) with
        {
            Notices = [Items.Notice("switch/codex", Items.Dismiss("codex"))],
            Setup = new SetupStep("Add a second account", "Pitboard switches once there are two.", [Add, notNow]),
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))],
        });

        Views.Element<InfoBar>(page, "notice.switch/codex");
        Assert.Contains("Add a second account", Views.Texts(page));
        Views.Press(Views.Button(page, "Not now"));

        Assert.Contains(notNow.Intent, model.Sent);
    }

    // Settings says why Windows refused, where the tip would stay with no word.
    [UITestMethod]
    public async Task ATurnOnWindowsRefusesOpensTheSettings()
    {
        var turnOn = new Choice("Turn on", new Intent.OpensAtLogin(On: true), Enabled: true);
        var app = new AppModel(new StandInModel(Snapshots.Read(1) with
        {
            Setup = new SetupStep("Start when you sign in to Windows", "Pitboard watches your limits only while it’s running.", [turnOn]),
        }));
        var shown = 0;
        var start = new StartAtSignIn(new Refusing());
        var page = await Views.Navigated<AccountsPage>(Views.Context(app, start) with { ShowSettings = () => shown++ });

        Views.Press(Views.Button(page, "Turn on"));

        Assert.IsNotNull(start.Failed);
        Assert.AreEqual(1, shown);
    }

    private sealed class Refusing : IStartupItem
    {
        public string? Refusal => null;

        public bool Read() => false;

        public bool TurnedOff() => false;

        public void Write(bool starts) => throw new UnauthorizedAccessException("Access is denied.");
    }

    // Narrator's H goes from the page's title to each tool's accounts.
    [UITestMethod]
    public async Task EachToolsHeadingIsALevel2Heading()
    {
        var (page, _, _) = await Over(Snapshots.Read(1) with
        {
            Sections =
            [
                Items.Section("claude", "Claude Code", Items.Account("claude", "work", inUse: true)),
                Items.Section("codex", "Codex", Items.Account("codex", "spare")),
            ],
        });

        var headings = Views.Visible<TextBlock>(page)
            .Where(text => AutomationProperties.GetHeadingLevel(text) == AutomationHeadingLevel.Level2)
            .Select(text => text.Text);

        Assert.AreEqual("Claude Code, Codex", string.Join(", ", headings));
    }

    // An empty page's one thing to do is its accent button, as Windows' own empty pages have; a retry is not.
    [UITestMethod]
    public async Task AnEmptyPagesOneThingToDoIsItsAccentButton()
    {
        var accent = (Style)Application.Current.Resources["AccentButtonStyle"];
        var (empty, _, _) = await Over(Snapshots.Read(1) with
        {
            AccountsShown = new AccountsShown.NoAccounts("No accounts", "Sign in to Claude Code or Codex to add one.", Add),
        });

        Assert.AreSame(accent, Views.Button(empty, "Add account…").Style);

        var retry = new Choice("Try again", new Intent.Refresh(Asked: true), Enabled: true);
        var (failed, _, _) = await Over(Snapshots.Read(1) with
        {
            AccountsShown = new AccountsShown.ReadFailed("Couldn’t read accounts", "The keychain is locked.", retry),
        });

        Assert.AreNotSame(accent, Views.Button(failed, "Try again").Style);
    }

    [UITestMethod]
    public async Task AToolNotInstalledOpensWhereToGetIt()
    {
        var opened = new List<Uri>();
        var model = new StandInModel(Snapshots.Read(1) with
        {
            AccountsShown = new AccountsShown.NoTool(
                "Claude Code isn’t installed", "Pitboard switches Claude Code’s accounts.", "Get Claude Code",
                "https://claude.com/claude-code"),
        });
        var page = await Views.Navigated<AccountsPage>(Views.Context(new AppModel(model), open: opened.Add));

        Views.Press(Views.Button(page, "Get Claude Code"));

        Assert.AreEqual(new Uri("https://claude.com/claude-code"), opened.Single());
    }

    // The row's own button sends what the model offers it.
    [UITestMethod]
    public async Task ARowsButtonSendsItsAction()
    {
        var (page, model, _) = await Over(Snapshots.Read(1) with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare"))],
        });

        Views.Press(Views.Button(Views.Element<ListViewItem>(page, "account.claude/spare"), "Use"));

        Assert.Contains(new Intent.SwitchTo("claude/spare"), model.Sent);
    }

    [UITestMethod]
    public async Task RefreshIsHeldBackWhileAReadIsUnderWay()
    {
        var (page, _, _) = await Over(Snapshots.Read(1) with { Reading = true });

        Assert.IsFalse(Views.Element<AppBarButton>(page, "accounts.refresh").IsEnabled);
    }
}
