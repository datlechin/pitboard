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

    private static string[] Said(MenuFlyout menu) =>
        [.. menu.Items.Select(entry => entry is MenuFlyoutItem item ? item.Text : "-")];

    [UITestMethod]
    public void TheMenuOffersWhatTheModelOffersThenCopyingThenForgetting()
    {
        var item = Items.Account("claude", "spare") with
        {
            Offers = [Use, SignInAgain, Rename],
            Forget = Items.Forget("claude/spare"),
        };

        var menu = AccountMenu.For(item, (_, _) => { });

        Assert.AreEqual("Use spare, Sign in again…, Rename…, -, Copy email address, -, Forget…", string.Join(", ", Said(menu)));
        Assert.IsFalse(((MenuFlyoutItem)menu.Items[1]).IsEnabled, "a sign-in held back");
    }

    [UITestMethod]
    public void TheAccountInUseOffersNoForget()
    {
        var item = Items.Account("claude", "work", inUse: true) with { Offers = [Rename], Forget = null };

        Assert.DoesNotContain("Forget…", Said(AccountMenu.For(item, (_, _) => { })));
    }

    [UITestMethod]
    public void AnAccountWithoutAnAddressOffersNoCopy()
    {
        var item = Items.Account("claude", "work") with { Email = "", Offers = [Rename] };

        Assert.AreEqual("Rename…", string.Join(", ", Said(AccountMenu.For(item, (_, _) => { }))));
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

        Assert.AreEqual("Rename…, -, Open chatgpt.com, -, Copy email address, -, Forget…", string.Join(", ", Said(menu)));
        Assert.AreEqual(spare, opened.Single());
    }

    [UITestMethod]
    public void AWindowWhoseStoreIsHeldCannotBeOpened()
    {
        var spare = Items.Window("codex", "spare");
        var item = Items.Account("codex", "spare") with { Windows = [new WindowOffer("Open chatgpt.com", spare)] };

        var menu = AccountMenu.For(item, (_, _) => { }, _ => { }, held: [spare.Store]);

        var entry = menu.Items.OfType<MenuFlyoutItem>().Single(found => found.Text == "Open chatgpt.com");
        Assert.IsFalse(entry.IsEnabled);
        StringAssert.StartsWith((string)ToolTipService.GetToolTip(entry), "What this window kept before is deleted once every account window has closed.");
    }
}
