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
    private static readonly ItemOffer Use = new("Use spare", new Intent.SwitchTo("claude/spare"), true, null);
    private static readonly ItemOffer SignInAgain = new("Sign in again…", new Intent.PresentSheet(new Sheet.SignInAgain("claude", "spare")), false, null);
    private static readonly ItemOffer Rename = new("Rename…", new Intent.PresentSheet(new Sheet.Rename("claude", "spare")), true, null);

    // Each item by what UI Automation finds it by, a separator as "-".
    private static string[] Said(MenuFlyout menu) =>
        [.. menu.Items.Select(entry => entry is MenuFlyoutItem item ? AutomationProperties.GetAutomationId(item) : "-")];

    // Microsoft's menu guidance removes a context menu's items that do not apply now, as the HIG does.
    [UITestMethod]
    public void TheMenuOffersWhatTheModelOffersThenCopyingThenForgetting()
    {
        var item = Items.Account("claude", "spare") with
        {
            Offers = [Use, SignInAgain, Rename],
            Forget = Items.Forget("claude/spare"),
        };

        var menu = AccountMenu.For(item, (_, _) => { });

        Assert.AreEqual(
            "account.switchTo, account.rename, -, account.copyEmail, -, account.forget", string.Join(", ", Said(menu)));
    }

    // The keys AccountKeys takes on the row, shown beside the items that do the same.
    [UITestMethod]
    public void TheMenuShowsTheRowsKeys()
    {
        var item = Items.Account("claude", "spare") with { Offers = [Use, Rename], Forget = Items.Forget("claude/spare") };

        var shown = AccountMenu.For(item, (_, _) => { }).Items.OfType<MenuFlyoutItem>()
            .ToDictionary(entry => AutomationProperties.GetAutomationId(entry), entry => entry.KeyboardAcceleratorTextOverride);

        Assert.AreEqual("F2", shown["account.rename"]);
        Assert.AreEqual("Ctrl+C", shown["account.copyEmail"]);
        Assert.AreEqual("Del", shown["account.forget"]);
        Assert.AreEqual("", shown["account.switchTo"]);
    }

    [UITestMethod]
    public void TheAccountInUseOffersNoForget()
    {
        var item = Items.Account("claude", "work", inUse: true) with
        {
            Offers = [Use with { Enabled = false }, Rename],
            Forget = Items.Forget("claude/work") with { Enabled = false },
        };

        Assert.AreEqual("account.rename, -, account.copyEmail", string.Join(", ", Said(AccountMenu.For(item, (_, _) => { }))));
    }

    [UITestMethod]
    public void AnAccountWithoutAnAddressOffersNoCopy()
    {
        var item = Items.Account("claude", "work") with { Email = "", Offers = [Rename] };

        Assert.AreEqual("account.rename", string.Join(", ", Said(AccountMenu.For(item, (_, _) => { }))));
    }

    [UITestMethod]
    public void ForgettingCarriesItsQuestion()
    {
        var forget = Items.Forget("claude/spare");
        var asked = new List<(Question?, Intent)>();
        var menu = AccountMenu.For(Items.Account("claude", "spare") with { Forget = forget }, (question, intent) => asked.Add((question, intent)));

        new MenuFlyoutItemAutomationPeer((MenuFlyoutItem)menu.Items[^1]).Invoke();

        Assert.AreEqual((forget.Confirm, forget.Intent), asked.Single());
    }

    [UITestMethod]
    public void TheAccountsWindowsComeAfterItsOffers()
    {
        var spare = Items.Window("codex", "spare");
        var item = Items.Account("codex", "spare") with
        {
            Offers = [Rename],
            Windows = [new WindowOffer("Open chatgpt.com", spare)],
            Forget = Items.Forget("codex/spare"),
        };
        var opened = new List<WindowAccount>();

        var menu = AccountMenu.For(item, (_, _) => { }, opened.Add, held: []);
        new MenuFlyoutItemAutomationPeer((MenuFlyoutItem)menu.Items[2]).Invoke();

        Assert.AreEqual(
            "account.rename, -, window.chatgpt.com/spare, -, account.copyEmail, -, account.forget", string.Join(", ", Said(menu)));
        Assert.AreEqual(spare, opened.Single());
    }

    [UITestMethod]
    public void AWindowWhoseStoreIsHeldCannotBeOpened()
    {
        var spare = Items.Window("codex", "spare");
        var item = Items.Account("codex", "spare") with { Windows = [new WindowOffer("Open chatgpt.com", spare)] };

        var menu = AccountMenu.For(item, (_, _) => { }, _ => { }, held: [spare.Store]);

        var entry = menu.Items.OfType<MenuFlyoutItem>().Single(found => AutomationProperties.GetAutomationId(found) == "window.chatgpt.com/spare");
        Assert.IsFalse(entry.IsEnabled);
        StringAssert.StartsWith((string)ToolTipService.GetToolTip(entry), "What this window kept before is deleted once every account window has closed.");
    }
}
