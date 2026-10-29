using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AccountMenuTests
{
    private static readonly AccountAction Switch = Items.Action(ActionKind.Switch, "Switch to spare", new Intent.SwitchTo(new AccountKey(Tool.Claude, "spare")));
    private static readonly AccountAction SignInAgain = Items.Action(
        ActionKind.SignInAgain, "Sign in again…", new Intent.PresentSheet(new Sheet.SignInAgain(new AccountKey(Tool.Claude, "spare"))), enabled: false);
    private static readonly AccountAction Rename = Items.RenameAction("claude", "spare");

    // Each item by what UI Automation finds it by, a separator as "-".
    private static string[] Said(MenuFlyout menu) =>
        [.. menu.Items.Select(entry => entry is MenuFlyoutItem item ? AutomationProperties.GetAutomationId(item) : "-")];

    private static MenuFlyout Menu(AccountItem item, Action<string>? copy = null) => AccountMenu.For(item, (_, _) => { }, copy ?? (_ => { }));

    // Microsoft's menu guidance removes a context menu's items that do not apply now, as the HIG does.
    [UITestMethod]
    public void TheMenuOffersWhatTheModelOffersThenCopyingThenRemoving()
    {
        var item = Items.Account("claude", "spare") with { Actions = [Switch, SignInAgain, Rename, Items.RemoveAction("claude", "spare")] };

        Assert.AreEqual("account.switchTo, account.rename, -, account.copyEmail, -, account.forget", string.Join(", ", Said(Menu(item))));
    }

    // The keys AccountKeys takes on the row, shown beside the items that do the same, and a letter on each item.
    [UITestMethod]
    public void TheMenuShowsTheRowsKeysAndAnAccessKeyOnEachItem()
    {
        var item = Items.Account("claude", "spare") with { Actions = [Switch, Rename, Items.RemoveAction("claude", "spare")] };

        var shown = Menu(item).Items.OfType<MenuFlyoutItem>().Select(entry => $"{entry.KeyboardAcceleratorTextOverride} {entry.AccessKey}");

        Assert.AreEqual(" S, F2 M, Ctrl+C C, Del R", string.Join(", ", shown));
    }

    [UITestMethod]
    public void TheAccountInUseOffersNoRemove()
    {
        var item = Items.Account("claude", "work", inUse: true) with
        {
            Actions = [Switch with { Enabled = false }, Rename, Items.RemoveAction("claude", "work", enabled: false)],
        };

        Assert.AreEqual("account.rename, -, account.copyEmail", string.Join(", ", Said(Menu(item))));
    }

    [UITestMethod]
    public void AnAccountWithoutAnAddressOffersNoCopy()
    {
        var item = Items.Account("claude", "work") with { Email = "", Actions = [Rename] };

        Assert.AreEqual("account.rename", string.Join(", ", Said(Menu(item))));
    }

    [UITestMethod]
    public void RemovingCarriesItsQuestion()
    {
        var remove = Items.RemoveAction("claude", "spare");
        var asked = new List<(Question?, Intent)>();
        var menu = AccountMenu.For(Items.Account("claude", "spare") with { Actions = [remove] }, (question, intent) => asked.Add((question, intent)), _ => { });

        new MenuFlyoutItemAutomationPeer((MenuFlyoutItem)menu.Items[^1]).Invoke();

        Assert.AreEqual((remove.Confirm, remove.Intent!), asked.Single());
    }

    // Through the page's own copy, as Ctrl+C on the row and the expanded row's Copy do.
    [UITestMethod]
    public void CopyingGivesTheAddress()
    {
        var copied = new List<string>();
        var menu = Menu(Items.Account("claude", "spare"), copied.Add);

        new MenuFlyoutItemAutomationPeer(menu.Items.OfType<MenuFlyoutItem>().Single()).Invoke();

        Assert.AreEqual("spare@example.com", copied.Single());
    }

    [UITestMethod]
    public void TheAccountsWindowsComeAfterWhatItDoes()
    {
        var spare = Items.Window("codex", "spare");
        var item = Items.Account("codex", "spare") with { Actions = [Rename, Items.WindowAction(spare), Items.RemoveAction("codex", "spare")] };
        var opened = new List<WindowAccount>();

        var menu = AccountMenu.For(item, (_, _) => { }, _ => { }, opened.Add, held: []);
        new MenuFlyoutItemAutomationPeer((MenuFlyoutItem)menu.Items[2]).Invoke();

        Assert.AreEqual(
            "account.rename, -, window.chatgpt.com/spare, -, account.copyEmail, -, account.forget", string.Join(", ", Said(menu)));
        Assert.AreEqual(spare, opened.Single());
    }

    [UITestMethod]
    public void AWindowWhoseStoreIsHeldCannotBeOpened()
    {
        var spare = Items.Window("codex", "spare");
        var item = Items.Account("codex", "spare") with { Actions = [Items.WindowAction(spare)] };

        var menu = AccountMenu.For(item, (_, _) => { }, _ => { }, _ => { }, held: [spare.Store]);

        var entry = menu.Items.OfType<MenuFlyoutItem>().Single(found => AutomationProperties.GetAutomationId(found) == "window.chatgpt.com/spare");
        Assert.IsFalse(entry.IsEnabled);
        StringAssert.StartsWith((string)ToolTipService.GetToolTip(entry), "What this window kept before is deleted once every account window has closed.");
    }
}
