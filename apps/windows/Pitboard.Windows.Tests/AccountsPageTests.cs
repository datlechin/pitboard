using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;
using Windows.Foundation;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AccountsPageTests
{
    private static readonly Choice Add = new("Add account…", new Intent.PresentSheet(new Sheet.Add(Provider: null)), Enabled: true);

    private static async Task<(AccountsPage Page, StandInModel Model, AppModel App)> Over(
        Snapshot snapshot, double width = double.NaN, Action<string>? copy = null, Action<WindowAccount>? openWindow = null,
        Action<Uri>? open = null)
    {
        var model = new StandInModel(snapshot);
        var app = new AppModel(model);
        var frame = await Views.Shown(new Frame { IsNavigationStackEnabled = false, Width = width });
        var context = Views.Context(app, open: open, copy: copy) with { OpenWindow = openWindow };
        return (await Views.Navigate<AccountsPage>(frame, context), model, app);
    }

    private static Snapshot Listing(params AccountItem[] accounts) =>
        Snapshots.Read(1) with { Sections = [Items.Section("claude", null, accounts)] };

    // What work and spare look like with both limits read, spare's sign-in about to expire.
    private static AccountItem Spare() =>
        Items.Account("claude", "spare") with
        {
            Plan = "Max 5x",
            Limits = [Items.Limit(100), Items.Limit(61) with { Kind = new LimitKind.Weekly(), Short = "Weekly", Resets = "Resets Thu 2:00 PM" }],
            Notes = [new AccountNote(NoteKind.Expiring, "Sign-in expires in 2 days", Action: null, Learn: null)],
            Detail = new AccountDetail(
                Email: "spare@example.com", Plan: "Max 5x", SignedInUntil: 1_800_172_800, SignIn: "Saved sign-in valid until Nov 20",
                Warm: new WarmDetail(
                    On: true, Said: "Warms up again at 3:45 PM", NextAt: null, RunningUntil: null,
                    Settings: new Choice("Warm-up settings", new Intent.Show(new Place.Settings(SettingsSection.Limits)), Enabled: true)),
                ReadAt: null, Read: null),
            Actions =
            [
                Items.Action(ActionKind.Switch, "Switch to spare", new Intent.SwitchTo("claude/spare")), Items.RenameAction("claude", "spare"),
                Items.WindowAction(Items.Window("claude", "spare")), Items.RemoveAction("claude/spare"),
            ],
        };

    private static AccountItem Work() =>
        Items.Account("claude", "work", inUse: true) with { Plan = "Team 5x", Limits = [Items.Limit(42)] };

    private static SettingsExpander Row(Page page, string qualified) => Views.Element<SettingsExpander>(page, $"account.{qualified}");

    private static Rect Bounds(FrameworkElement element, UIElement within) =>
        element.TransformToVisual(within).TransformBounds(new Rect(0, 0, element.ActualWidth, element.ActualHeight));

    // What a page's open popups hold, a TeachingTip's among them, which no visual parent reaches.
    private static IEnumerable<DependencyObject> Popped(Page page) =>
        VisualTreeHelper.GetOpenPopupsForXamlRoot(page.XamlRoot).Select(popup => popup.Child).OfType<DependencyObject>().SelectMany(Views.Descendants);

    private static async Task Open(SettingsExpander row, string inside)
    {
        row.IsExpanded = true;
        await Views.Until(() => Views.Texts(row).Contains(inside), $"{AutomationProperties.GetAutomationId(row)} did not open on '{inside}'");
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
        var (_, model, _) = await Over(Snapshots.Read(1));

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
        Row(page, "claude/work");
        Row(page, "codex/spare");
    }

    // One tool's accounts are under no heading, which would take room and say nothing.
    [UITestMethod]
    public async Task OneToolsAccountsHaveNoHeading()
    {
        var (page, _, _) = await Over(Listing(Work(), Spare()));

        Assert.IsFalse(Views.Visible<TextBlock>(page).Any(text => AutomationProperties.GetHeadingLevel(text) == AutomationHeadingLevel.Level2));
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

    // Plan 4.9: a row is the account, its plan, a line per limit and its one note, with In use or its button at its end.
    [UITestMethod]
    public async Task ARowSaysItsLimitsAndItsNoteAndOffersItsAction()
    {
        var (page, model, _) = await Over(Listing(Work(), Spare()));
        var spare = Row(page, "claude/spare");

        var texts = Views.Texts(spare);
        foreach (var said in new[] { "spare", "Max 5x", "5-hour", "100%", "Resets 10:10 AM", "Weekly", "61%", "Resets Thu 2:00 PM", "Sign-in expires in 2 days" })
        {
            Assert.Contains(said, texts);
        }

        Assert.DoesNotContain("spare@example.com", texts, "the email before the row is opened");
        Assert.DoesNotContain("In use", texts);
        Assert.Contains("In use", Views.Texts(Row(page, "claude/work")));
        var use = Views.Element<Button>(spare, "account.action");
        Assert.AreEqual("Use spare", AutomationProperties.GetName(use));
        Views.Press(use);
        Assert.Contains(new Intent.SwitchTo("claude/spare"), model.Sent);
    }

    // Plan 3: a note nothing in the app does anything about ends with the docs that say what to do.
    [UITestMethod]
    public async Task ANoteNothingInTheAppPutsRightLinksToItsDocs()
    {
        var learn = new SetupLink("Learn more", "https://docs.usepitboard.com/reference/errors#stale-codes");
        var stale = Spare() with { Notes = [new AccountNote(NoteKind.Stale, "Usage from 2 hours ago", Action: null, Learn: learn)] };
        var opened = new List<Uri>();
        var (page, _, _) = await Over(Listing(Work(), stale), open: opened.Add);
        Assert.IsFalse(
            Views.Visible<HyperlinkButton>(Row(page, "claude/work")).Any(link => AutomationProperties.GetAutomationId(link) == "account.note.learn"),
            "a link under a row with no note");

        var link = Views.Element<HyperlinkButton>(Row(page, "claude/spare"), "account.note.learn");
        Assert.AreEqual("Learn more", link.Content);
        new HyperlinkButtonAutomationPeer(link).Invoke();

        CollectionAssert.AreEqual(new[] { new Uri(learn.Link) }, opened);
    }

    // Narrator reads the row as one sentence, and each limit once, not the bar and each figure on their own.
    [UITestMethod]
    public async Task TheRowAndEachLimitAreSpokenOnce()
    {
        var (page, _, _) = await Over(Listing(Spare()));
        var spare = Row(page, "claude/spare");

        Assert.AreEqual(Spare().Spoken, AutomationProperties.GetName(spare));
        var spoken = Views.Visible<TextBlock>(spare).Single(text => text.Text == "5-hour");
        Assert.AreEqual(Spare().Limits[0].Spoken, AutomationProperties.GetName(spoken));
        foreach (var unspoken in Views.Visible<TextBlock>(spare).Where(text => text.Text is "100%" or "Resets 10:10 AM"))
        {
            Assert.AreEqual(AccessibilityView.Raw, AutomationProperties.GetAccessibilityView(unspoken));
        }

        Assert.IsTrue(Views.Visible<ProgressBar>(spare).All(bar => AutomationProperties.GetAccessibilityView(bar) == AccessibilityView.Raw));
    }

    // Plan 10 asks this be measured first: a SettingsExpander holding bars keeps its layout. At the window's narrowest,
    // every bar is drawn, the bars of two accounts line up, and the button stays at the row's end.
    [UITestMethod]
    public async Task AtTheNarrowestTheBarsLineUpAndTheButtonStaysAtTheEnd()
    {
        // 640 epx less the compact pane's 48.
        var (page, _, _) = await Over(Listing(Work(), Spare()), width: 592);
        var work = Row(page, "claude/work");
        var spare = Row(page, "claude/spare");
        var workBar = Views.Visible<ProgressBar>(work).Single();
        var spareBars = Views.Visible<ProgressBar>(spare).ToArray();

        Assert.HasCount(2, spareBars);
        Assert.IsTrue(
            spareBars.Append(workBar).All(bar => bar.ActualWidth >= 80),
            $"bars {string.Join(", ", spareBars.Append(workBar).Select(bar => bar.ActualWidth))} wide");
        Assert.AreEqual(Bounds(workBar, page).Left, Bounds(spareBars[0], page).Left, 0.5, "work's bar and spare's do not line up");
        var button = Views.Element<Button>(spare, "account.action");
        Assert.IsGreaterThan(Bounds(spareBars[0], page).Right, Bounds(button, page).Left, "the button runs into the bars");
        Assert.AreEqual(Bounds(work, page).Right, Bounds(spare, page).Right, 0.5);
    }

    // At the default 960 epx, a bar is as long as the mockup draws it, not the whole row.
    [UITestMethod]
    public async Task AWideRowsBarsKeepTheirLength()
    {
        var (page, _, _) = await Over(Listing(Spare()), width: 912);

        Assert.IsTrue(Views.Visible<ProgressBar>(page).All(bar => bar.ActualWidth is >= 80 and <= 260.5));
    }

    // Opened, a row says its email with Copy, its sign-in, its warm-up with a way to its settings, and its commands.
    [UITestMethod]
    public async Task AnOpenedRowSaysTheAccountsDetailAndOffersItsCommands()
    {
        var copied = new List<string>();
        var opened = new List<WindowAccount>();
        var (page, model, _) = await Over(Listing(Spare()), copy: copied.Add, openWindow: opened.Add);
        var spare = Row(page, "claude/spare");
        await Open(spare, "Saved sign-in valid until Nov 20");

        var texts = Views.Texts(spare);
        foreach (var said in new[] { "Email", "spare@example.com", "Sign-in", "Warm-up", "Warms up again at 3:45 PM" })
        {
            Assert.Contains(said, texts);
        }

        Views.Press(Views.Element<Button>(spare, "detail.copyEmail"));
        new HyperlinkButtonAutomationPeer(Views.Element<HyperlinkButton>(spare, "detail.warmUpSettings")).Invoke();
        Views.Press(Views.Element<Button>(spare, "detail.window.claude.ai"));
        Views.Press(Views.Element<Button>(spare, "detail.rename"));

        Assert.AreEqual("spare@example.com", copied.Single());
        Assert.IsTrue(Structural.Equal(Items.Window("claude", "spare"), opened.Single()), "another window opened");
        Assert.Contains(new Intent.Show(new Place.Settings(SettingsSection.Limits)), model.Sent);
        Assert.Contains(new Intent.PresentSheet(new Sheet.Rename("claude", "spare")), model.Sent);
        Assert.AreEqual(
            "detail.window.claude.ai, detail.rename, detail.remove",
            string.Join(", ", Views.Visible<Button>(spare).Select(AutomationProperties.GetAutomationId).Where(id => id.StartsWith("detail.window", StringComparison.Ordinal) || id is "detail.rename" or "detail.remove")));
    }

    // Remove account is dimmed for the account in use, whose record is the only one of who is signed in.
    [UITestMethod]
    public async Task TheAccountInUseHasRemoveDimmed()
    {
        var work = Work() with { Actions = [Items.RenameAction("claude", "work"), Items.RemoveAction("claude/work", enabled: false)] };
        var (page, _, _) = await Over(Listing(work));
        var row = Row(page, "claude/work");
        await Open(row, "Email");

        Assert.IsFalse(Views.Element<Button>(row, "detail.remove").IsEnabled);
        Assert.IsTrue(Views.Element<Button>(row, "detail.rename").IsEnabled);
    }

    // The model makes every list anew for each snapshot, and the frame keeps the page while another shows; the row the
    // person opened stays open.
    [UITestMethod]
    public async Task AnOpenedRowStaysOpenAcrossSnapshotsAndPages()
    {
        var first = Listing(Work(), Spare());
        var (page, _, app) = await Over(first);
        var spare = Row(page, "claude/spare");
        await Open(spare, "Email");

        app.Apply(first with { Revision = 2, Sections = [Items.Section("claude", null, Work(), Spare() with { Title = "spare 12%" })] });
        var frame = page.Frame;
        await Views.Navigate<ActivityPage>(frame, Views.Context(app));
        var back = await Views.Navigate<AccountsPage>(frame, Views.Context(app));

        Assert.AreSame(page, back, "the frame made the accounts page again");
        Assert.IsTrue(Row(back, "claude/spare").IsExpanded);
        Assert.Contains("spare 12%", Views.Texts(Row(back, "claude/spare")));
    }

    // A rename keeps the account's id, so the row stays the same object in the same container.
    [UITestMethod]
    public async Task ARenamedRowIsFoundByItsNewName()
    {
        var personal = Items.Account("claude", "personal") with { Id = "claude:same" };
        var first = Listing(Items.Account("claude", "work", inUse: true), personal);
        var (page, _, app) = await Over(first);
        Row(page, "claude/personal");

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

        Row(page, "claude/home");
    }

    // The name sat in a horizontal StackPanel, which gives it all the width it asks for, so it never cut.
    [UITestMethod]
    public async Task ALongNameIsCutAndKeepsItsPlan()
    {
        var (page, _, _) = await Over(Listing(Items.Account("claude", "an-account-named-far-too-long-for-its-row-at-the-narrowest") with { Plan = "Max" }), width: 400);
        var row = Row(page, "claude/an-account-named-far-too-long-for-its-row-at-the-narrowest");
        var name = Views.Visible<TextBlock>(row).Single(text => text.Name == "TitleText");
        var plan = Views.Visible<TextBlock>(row).Single(text => text.Name == "PlanText");

        await Views.Until(() => name.IsTextTrimmed, "the name was not cut");
        Assert.IsGreaterThan(0.0, plan.ActualWidth);
        Assert.IsLessThanOrEqualTo(Bounds(row, page).Right, Bounds(plan, page).Right, "the plan runs past the row");
    }

    // The XAML compiler calls an x:Bind function only for an argument that is not null, so a row given another account
    // kept the plan of the one before.
    [UITestMethod]
    public async Task ARowGivenAnAccountWithoutAPlanSaysNone()
    {
        var first = Listing(Items.Account("claude", "work", inUse: true) with { Plan = "Pro" });
        var (page, _, app) = await Over(first);

        app.Apply(first with { Revision = 2, Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))] });
        await Views.Drained();

        Assert.AreEqual("", Views.Descendants(Row(page, "claude/work")).OfType<TextBlock>().Single(text => text.Name == "PlanText").Text);
    }

    [UITestMethod]
    public async Task ANoticeComesBeforeTheAccounts()
    {
        var (page, _, _) = await Over(Snapshots.Read(1) with
        {
            Notices = [Items.Notice("switch/codex", Items.Dismiss("codex"))],
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))],
        });

        var notice = Views.Element<InfoBar>(page, "notice.switch/codex");

        Assert.IsLessThan(Bounds(Row(page, "claude/work"), page).Top, Bounds(notice, page).Bottom);
    }

    // With no notice, the accounts start at the top of what scrolls, with no gap held for one.
    [UITestMethod]
    public async Task WithNoNoticeTheAccountsStartAtTheTop()
    {
        var (page, _, _) = await Over(Listing(Work()));

        Assert.AreEqual(Bounds(Views.Element<ScrollViewer>(page, "accounts"), page).Top, Bounds(Row(page, "claude/work"), page).Top, 0.5);
    }

    // Microsoft's guidance keeps an InfoBar for what lasts and gives a tip a TeachingTip: the step's choice is its action
    // button, pointed at Add account, and Not now its close button.
    [UITestMethod]
    public async Task ASetupStepIsATipAtAddAccountWhoseCloseTurnsItDown()
    {
        var notNow = new Choice("Not now", new Intent.DeclineSecondAccount("claude"), Enabled: true);
        var step = new SetupStep("Add a second account", "Pitboard switches once there are two.", [Add, notNow]);
        var (page, model, _) = await Over(Snapshots.Read(1) with
        {
            Setup = step,
            Footing = new Footing.OnlyOne(Provider: "claude", Label: "work"),
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))],
        });
        var tip = Views.Descendants(page).OfType<TeachingTip>().Single();

        await Views.Until(() => tip.IsOpen, "the tip did not open");
        Assert.AreEqual(
            "Add a second account | Pitboard switches once there are two. | Add account… | Not now",
            string.Join(" | ", tip.Title, tip.Subtitle, tip.ActionButtonContent, tip.CloseButtonContent));
        Assert.AreSame(Views.Element<AppBarButton>(page, "accounts.add"), tip.Target);
        Assert.AreEqual("setup.onlyOne", AutomationProperties.GetAutomationId(tip));
        Assert.IsFalse(Views.Descendants(page).OfType<InfoBar>().Any(bar => bar.IsOpen), "the step in an InfoBar");

        var close = Popped(page).OfType<Button>()
            .Single(button => button.Name == "CloseButton");
        Views.Press(close);

        Assert.Contains(notNow.Intent, model.Sent);
    }

    // What happens at a limit is set in Settings, which is not on this page.
    [UITestMethod]
    public async Task TheTipAboutLimitsPointsAtNothing()
    {
        var notNow = new Choice("Not now", new Intent.DeclineTip(Tip.AtLimit), Enabled: true);
        var settings = new Choice("Open settings", new Intent.ShowLimits(), Enabled: true);
        var (page, model, _) = await Over(Snapshots.Read(1) with
        {
            Setup = new SetupStep("Switch before a limit runs out", "Pitboard notifies you at 95%.", [settings, notNow]),
            Footing = new Footing.AtLimitUnchosen(),
        });
        var tip = Views.Descendants(page).OfType<TeachingTip>().Single();
        await Views.Until(() => tip.IsOpen, "the tip did not open");

        Assert.IsNull(tip.Target);
        var action = Popped(page).OfType<Button>()
            .Single(button => button.Name == "ActionButton");
        Views.Press(action);

        Assert.Contains(settings.Intent, model.Sent);
        Assert.IsFalse(tip.IsOpen, "the tip stayed up over Settings");
    }

    // A tip's popup stays over the window, so it goes with the page and comes back with it.
    [UITestMethod]
    public async Task TheTipGoesWithThePage()
    {
        var notNow = new Choice("Not now", new Intent.DeclineSecondAccount("claude"), Enabled: true);
        var (page, _, app) = await Over(Snapshots.Read(1) with
        {
            Setup = new SetupStep("Add a second account", "Pitboard switches once there are two.", [Add, notNow]),
            Footing = new Footing.OnlyOne(Provider: "claude", Label: "work"),
        });
        var tip = Views.Descendants(page).OfType<TeachingTip>().Single();
        await Views.Until(() => tip.IsOpen, "the tip did not open");

        var frame = page.Frame;
        await Views.Navigate<ActivityPage>(frame, Views.Context(app));
        Assert.IsFalse(tip.IsOpen, "the tip stayed over another page");

        await Views.Navigate<AccountsPage>(frame, Views.Context(app));
        await Views.Until(() => tip.IsOpen, "the tip did not come back with the page");
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

    [UITestMethod]
    public async Task RefreshIsHeldBackWhileAReadIsUnderWay()
    {
        var (page, _, _) = await Over(Snapshots.Read(1) with { Reading = true });

        Assert.IsFalse(Views.Element<AppBarButton>(page, "accounts.refresh").IsEnabled);
    }

    // Microsoft's access keys guidance, for the page's commands.
    [UITestMethod]
    public async Task ThePagesCommandsHaveAccessKeys()
    {
        var (page, _, _) = await Over(Snapshots.Read(1));

        string[] commands = ["accounts.refresh", "accounts.add"];

        Assert.AreEqual("R, A", string.Join(", ", commands.Select(id => Views.Element<AppBarButton>(page, id).AccessKey)));
    }

    // A row's toggle opens and closes it, and its button is the next stop for Tab.
    [UITestMethod]
    public async Task ARowsToggleComesBeforeItsButton()
    {
        var (page, _, _) = await Over(Listing(Work(), Spare()));
        var spare = Row(page, "claude/spare");
        var toggle = Views.Descendants(spare).OfType<ToggleButton>().First();

        Assert.IsTrue(toggle.Focus(FocusState.Keyboard), "the row's toggle took no focus");
        var next = FocusManager.FindNextElement(FocusNavigationDirection.Next, new FindNextElementOptions { SearchRoot = page.XamlRoot.Content });

        Assert.AreEqual("account.action", AutomationProperties.GetAutomationId((DependencyObject)next));
    }
}
