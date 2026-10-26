using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;
using Windows.Foundation;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AccountRowTests
{
    private static AccountRow Row(AccountItem item, Action<Intent>? send = null, double width = double.NaN) =>
        new() { Width = width, Account = new ShownAccount(item, send ?? (_ => { })) };

    private static Task<AccountRow> Shown(AccountItem item, Action<Intent>? send = null, double width = double.NaN) =>
        Views.Shown(Row(item, send, width));

    private static Rect Bounds(FrameworkElement element, UIElement within) =>
        element.TransformToVisual(within).TransformBounds(new Rect(0, 0, element.ActualWidth, element.ActualHeight));

    [UITestMethod]
    public async Task AnAccountNotInUseSaysItsLimitsAndOffersItsAction()
    {
        var sent = new List<Intent>();
        var item = Items.Acting(
            Items.Account("codex", "spare") with
            {
                Plan = "Plus",
                Limits = [Items.Limit(42)],
                Lines = ["Its parked login lasts 6 more days"],
            },
            new ItemAction("Use", "Use spare (Codex)", new Intent.SwitchTo("codex/spare")));
        var row = await Shown(item, sent.Add);

        var texts = Views.Texts(row);
        foreach (var said in new[] { "spare", "Plus", "spare@example.com", "5-hour", "42%", "Resets 10:10 AM", "Its parked login lasts 6 more days" })
        {
            Assert.Contains(said, texts);
        }

        Assert.DoesNotContain("In use", texts);
        var use = Views.Button(row, "Use");
        Assert.AreEqual("Use spare (Codex)", AutomationProperties.GetName(use));
        Views.Press(use);
        Assert.AreEqual(new Intent.SwitchTo("codex/spare"), sent.Single());
    }

    [UITestMethod]
    public async Task TheAccountInUseSaysSoAndHasNothingToPress()
    {
        var row = await Shown(Items.Account("claude", "work", inUse: true));

        Assert.Contains("In use", Views.Texts(row));
        Assert.IsFalse(Views.Visible<Button>(row).Any(), "a button on the account in use");
    }

    // Narrator reads a limit once, as a sentence, and not the bar and each figure on their own.
    [UITestMethod]
    public async Task ALimitIsSpokenOnce()
    {
        var limit = Items.Limit(42);
        var row = await Shown(Items.Account("claude", "work") with { Limits = [limit] });

        var spoken = Views.Visible<TextBlock>(row).Single(text => text.Text == "5-hour");
        Assert.AreEqual(limit.Spoken, AutomationProperties.GetName(spoken));
        foreach (var unspoken in Views.Visible<TextBlock>(row).Where(text => text.Text is "42%" or "Resets 10:10 AM"))
        {
            Assert.AreEqual(AccessibilityView.Raw, AutomationProperties.GetAccessibilityView(unspoken));
        }

        Assert.AreEqual(AccessibilityView.Raw, AutomationProperties.GetAccessibilityView(Views.Visible<ProgressBar>(row).Single()));
    }

    // The name sat in a horizontal StackPanel, which gives it all the width it asks for, so it never cut.
    [UITestMethod]
    public async Task ALongNameIsCutAndKeepsItsPlanTag()
    {
        var row = await Shown(Items.Account("claude", "an-account-named-far-too-long-for-its-row") with { Plan = "Max" }, width: 320);
        var name = (TextBlock)row.FindName("TitleText");
        var tag = (FrameworkElement)row.FindName("PlanTag");
        var button = (FrameworkElement)row.FindName("ActionButton");

        await Views.Until(() => name.IsTextTrimmed, "the name was not cut");
        Assert.AreEqual(Visibility.Visible, tag.Visibility);
        Assert.IsGreaterThan(0.0, tag.ActualWidth);
        Assert.IsLessThanOrEqualTo(Bounds(button, row).Left, Bounds(tag, row).Right, "the plan tag runs under the button");
    }

    [UITestMethod]
    public async Task AShortNameIsFollowedByItsPlanTag()
    {
        var row = await Shown(Items.Account("claude", "work") with { Plan = "Max" }, width: 480);
        var name = Bounds((FrameworkElement)row.FindName("TitleText"), row);
        var tag = Bounds((FrameworkElement)row.FindName("PlanTag"), row);

        Assert.IsFalse(((TextBlock)row.FindName("TitleText")).IsTextTrimmed);
        // Layout rounding gives the name's column whole pixels, up to one more than the text: 8.52 at 100% on CI.
        Assert.AreEqual(8.0, tag.Left - name.Right, 1 / row.XamlRoot.RasterizationScale, "the tag is not beside the name");
    }

    // The XAML compiler calls an x:Bind function only for a value that is not null, so a row the list gave another
    // account kept the plan tag of the one before, empty.
    [UITestMethod]
    public async Task ARowGivenAnAccountWithoutAPlanHasNoTag()
    {
        var row = await Shown(Items.Account("codex", "main") with { Plan = "Pro" }, width: 480);
        row.Account = new ShownAccount(Items.Account("claude", "work"), _ => { });
        await Views.Drained();

        Assert.AreEqual(Visibility.Collapsed, ((FrameworkElement)row.FindName("PlanTag")).Visibility);
    }
}
