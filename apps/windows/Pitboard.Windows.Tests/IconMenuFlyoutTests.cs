using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class IconMenuFlyoutTests
{
    private static AppModel TwoTools() => new(TwoToolsRead());

    private static Snapshot TwoToolsRead() => Snapshots.Read(1) with
    {
        Sections =
        [
            Items.Section(
                "claude", "Claude Code",
                Items.Account("claude", "work", inUse: true) with { Summary = "5-hour 42% used, Weekly 12%" },
                Items.Account("claude", "personal")),
            Items.Section("codex", "Codex", Items.Account("codex", "main", inUse: true)),
        ],
    };

    private static MenuFlyout Menu(AppModel model, List<IconCommand>? chosen = null) =>
        IconMenuFlyout.For(IconMenu.Entries(model, new NoUpdates()), command => chosen?.Add(command));

    // Each item by what UI Automation finds it by, and a heading, which has nothing to find it by, by its words.
    private static string Said(MenuFlyout menu) => string.Join(" | ", menu.Items.Select(item => item switch
    {
        MenuFlyoutSeparator => "-",
        RadioMenuFlyoutItem radio => (radio.IsChecked ? "(x) " : "( ) ") + Id(radio),
        MenuFlyoutItem entry when Id(entry).Length == 0 => entry.Text,
        MenuFlyoutItem entry => Id(entry),
        _ => item.GetType().Name,
    }));

    private static string Id(DependencyObject item) => AutomationProperties.GetAutomationId(item);

    // A RadioMenuFlyoutItem is a ToggleMenuFlyoutItem inside, and toggles as UI Automation sees it.
    private static void Toggle(MenuFlyoutItem radio) =>
        ((IToggleProvider)FrameworkElementAutomationPeer.CreatePeerForElement(radio).GetPattern(PatternInterface.Toggle)).Toggle();

    private static MenuFlyoutItem Item(MenuFlyout menu, string id) =>
        menu.Items.OfType<MenuFlyoutItem>().FirstOrDefault(item => Id(item) == id)
        ?? throw new AssertFailedException($"no '{id}' in: {Said(menu)}");

    [UITestMethod]
    public void TheMenuShowsTheEntriesInOrder()
    {
        var menu = Menu(TwoTools());

        Assert.AreEqual(
            "menu.openPitboard | - | Claude Code | (x) menu.account.claude/work | ( ) menu.account.claude/personal | Codex "
            + "| (x) menu.account.codex/main | - | menu.addAccount | - | menu.settings | menu.help | - | menu.exit",
            Said(menu));
    }

    // Its windows' items name the account and its site for UI Automation, since an account's own item has its label too.
    [UITestMethod]
    public void ASiteWithSeveralWindowsIsASubmenu()
    {
        var main = Items.Window("codex", "main");
        var spare = Items.Window("codex", "spare");
        var snapshot = Snapshots.Read(1);
        var model = new AppModel(snapshot with
        {
            AccountWindows = snapshot.AccountWindows with { Menus = [new SiteMenu.Several("Open chatgpt.com", main.Site, [main, spare])] },
        });
        var chosen = new List<IconCommand>();

        var site = Menu(model, chosen).Items.OfType<MenuFlyoutSubItem>().Single();
        var window = (MenuFlyoutItem)site.Items[1];
        new MenuFlyoutItemAutomationPeer(window).Invoke();

        Assert.AreEqual("menu.site.chatgpt.com", Id(site));
        Assert.AreEqual("main, spare", string.Join(", ", site.Items.OfType<MenuFlyoutItem>().Select(item => item.Text)));
        Assert.AreEqual("window.chatgpt.com/spare", Id(window));
        Assert.AreEqual(new IconCommand.OpenWindow(spare), chosen.Single());
    }

    // The window it opens from is a pixel wide.
    [UITestMethod]
    public void TheMenuDrawsOutsideTheWindowItOpensFrom()
    {
        Assert.IsFalse(Menu(TwoTools()).ShouldConstrainToRootBounds);
    }

    [UITestMethod]
    public void EachToolsAccountsAreOneRadioGroup()
    {
        var radios = Menu(TwoTools()).Items.OfType<RadioMenuFlyoutItem>().ToArray();

        Assert.AreEqual("claude, claude, codex", string.Join(", ", radios.Select(radio => radio.GroupName)));
    }

    [UITestMethod]
    public void AToolsNameIsAHeadingNobodyChooses()
    {
        var heading = Menu(TwoTools()).Items.OfType<MenuFlyoutItem>().Single(item => item.Text == "Claude Code");

        Assert.IsFalse(heading.IsEnabled);
        Assert.AreEqual(AutomationHeadingLevel.Level1, AutomationProperties.GetHeadingLevel(heading));
    }

    [UITestMethod]
    public void AnAccountsLimitsAreItsTooltipAndHelpText()
    {
        var menu = Menu(TwoTools());

        var work = Item(menu, "menu.account.claude/work");
        Assert.AreEqual("5-hour 42% used, Weekly 12%", ToolTipService.GetToolTip(work));
        Assert.AreEqual("5-hour 42% used, Weekly 12%", AutomationProperties.GetHelpText(work));
        Assert.IsNull(ToolTipService.GetToolTip(Item(menu, "menu.account.claude/personal")));
        Assert.IsNull(ToolTipService.GetToolTip(Item(menu, "menu.exit")));
    }

    // The window's keys do nothing in this menu, and the menu guidance shows a context menu no shortcuts.
    [UITestMethod]
    public void TheMenuShowsNoShortcuts()
    {
        var shown = Menu(TwoTools()).Items.OfType<MenuFlyoutItem>().Where(item => !string.IsNullOrEmpty(item.KeyboardAcceleratorTextOverride));

        Assert.AreEqual("", string.Join(", ", shown.Select(item => Id(item))));
    }

    [UITestMethod]
    public void OpenPitboardIsInBold()
    {
        var menu = Menu(TwoTools());

        Assert.AreEqual(FontWeights.SemiBold.Weight, Item(menu, "menu.openPitboard").FontWeight.Weight);
        Assert.AreEqual(FontWeights.Normal.Weight, Item(menu, "menu.settings").FontWeight.Weight);
    }

    // The tools' names are the only items nobody can choose.
    [UITestMethod]
    public void OnlyTheHeadingsAreDimmed()
    {
        var dimmed = Menu(TwoTools()).Items.OfType<MenuFlyoutItem>().Where(item => !item.IsEnabled);

        Assert.AreEqual("Claude Code, Codex", string.Join(", ", dimmed.Select(item => item.Text)));
    }

    [UITestMethod]
    public void ChoosingAnItemReportsItsCommand()
    {
        var chosen = new List<IconCommand>();
        var menu = Menu(TwoTools(), chosen);

        new MenuFlyoutItemAutomationPeer(Item(menu, "menu.addAccount")).Invoke();
        Toggle(Item(menu, "menu.account.claude/personal"));
        new MenuFlyoutItemAutomationPeer(Item(menu, "menu.exit")).Invoke();

        CollectionAssert.AreEqual(
            new IconCommand[]
            {
                new IconCommand.Send(new Intent.PresentSheet(new Sheet.Add(Tool: null))),
                new IconCommand.Send(new Intent.SwitchTo(new AccountKey(Tool.Claude, "personal"))),
                new IconCommand.Quit(),
            },
            chosen);
    }

    // Rows were taller in one opening of the menu than in another.
    [UITestMethod]
    public void EachRowIsPaddedAsAMouseOpenedMenu()
    {
        var narrow = (Thickness)Application.Current.Resources["MenuFlyoutItemThemePaddingNarrow"];
        var main = Items.Window("codex", "main");
        var spare = Items.Window("codex", "spare");
        var read = TwoToolsRead();
        var model = new AppModel(read with
        {
            AccountWindows = read.AccountWindows with { Menus = [new SiteMenu.Several("Open chatgpt.com", main.Site, [main, spare])] },
        });

        var rows = Rows(Menu(model).Items).ToArray();

        Assert.IsTrue(rows.OfType<MenuFlyoutSubItem>().Any(), "no submenu to look in");
        foreach (var row in rows)
        {
            Assert.AreEqual(narrow, row.Padding, row is MenuFlyoutItem item ? item.Text : ((MenuFlyoutSubItem)row).Text);
        }

        static IEnumerable<MenuFlyoutItemBase> Rows(IEnumerable<MenuFlyoutItemBase> items) =>
            items.Where(item => item is not MenuFlyoutSeparator)
                .SelectMany(item => item is MenuFlyoutSubItem sub ? Rows(sub.Items).Prepend(sub) : [item]);
    }

    // The account in use has nothing to do, and choosing it again does nothing.
    [UITestMethod]
    public void TheAccountInUseReportsNothing()
    {
        var chosen = new List<IconCommand>();
        var menu = Menu(TwoTools(), chosen);

        Toggle(Item(menu, "menu.account.claude/work"));

        Assert.IsEmpty(chosen);
        Assert.IsTrue(((RadioMenuFlyoutItem)Item(menu, "menu.account.claude/work")).IsChecked);
    }
}
