using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Shapes;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class LimitLinesTests
{
    [UITestMethod]
    public async Task PaceMarksExpectedUsedShareAndSpeaksTheSuppliedSentenceOnce()
    {
        var limit = Items.Limit(59) with
        {
            Pace = new LimitPace(25, PaceStanding.Over, "34% over pace", "At an even pace, 25% would be used."),
            Spoken = "5-hour limit, 59 percent used, 34 percent over pace, resets 3:45 PM",
        };
        var lines = await Views.Shown(new LimitLines { Width = 460, Limits = [limit] });
        var marker = Views.Element<Rectangle>(lines, "limit.paceMark");
        var bar = Views.Visible<ProgressBar>(lines).Single();

        Assert.AreEqual(59, bar.Value);
        Assert.AreEqual((bar.ActualWidth * .25) - 1, Canvas.GetLeft(marker), .5, "the mark uses used share, not remaining share");
        Assert.AreEqual(limit.Pace.Help, ToolTipService.GetToolTip(bar.Parent));
        var name = Views.Visible<TextBlock>(lines).Single(text => text.Text == limit.Short);
        Assert.AreEqual(limit.Spoken, AutomationProperties.GetName(name));
        var pace = Views.Element<TextBlock>(lines, "limit.pace");
        Assert.AreEqual(limit.Pace.Said, pace.Text);
        Assert.AreEqual(AccessibilityView.Raw, AutomationProperties.GetAccessibilityView(pace));

        lines.Width = 360;
        lines.UpdateLayout();
        Assert.AreEqual((bar.ActualWidth * .25) - 1, Canvas.GetLeft(marker), .5, "the marker follows the resized bar");

        lines.Limits = [limit with { Pace = null }];
        await Views.Until(() => !Views.Visible<TextBlock>(lines).Any(text => text.Text == limit.Pace.Said), "pace stayed after it was turned off");
        Assert.IsFalse(Views.Visible<Rectangle>(lines).Any(mark => AutomationProperties.GetAutomationId(mark) == "limit.paceMark"));
    }

    [UITestMethod]
    public async Task UnderPaceUsesTheSameAxisAndTheDetailClearsANullPace()
    {
        var pace = new LimitPace(75, PaceStanding.Under, "16% under pace", "At an even pace, 75% would be used.");
        var lines = await Views.Shown(new LimitLines { Width = 460, Limits = [Items.Limit(59) with { Pace = pace }] });
        var marker = Views.Element<Rectangle>(lines, "limit.paceMark");
        var bar = Views.Visible<ProgressBar>(lines).Single();
        Assert.AreEqual((bar.ActualWidth * .75) - 1, Canvas.GetLeft(marker), .5);
        Assert.AreEqual(Application.Current.Resources["UnderPaceMarkStyle"], marker.Style);

        var detail = await Views.Shown(new LimitPaceText { Pace = pace });
        Assert.Contains(pace.Said, Views.Texts(detail));
        detail.Pace = null;
        Assert.AreEqual(Visibility.Collapsed, detail.Visibility);
        Assert.AreEqual("", ((TextBlock)detail.Content).Text);
    }

    [UITestMethod]
    public async Task EvenPaceHasWordsAndHelpButNoMarker()
    {
        var pace = new LimitPace(40, PaceStanding.Even, "On pace", "At an even pace, 40% would be used.");
        var lines = await Views.Shown(new LimitLines { Width = 460, Limits = [Items.Limit(40) with { Pace = pace }] });

        Assert.Contains(pace.Said, Views.Texts(lines));
        Assert.AreEqual(pace.Help, ToolTipService.GetToolTip(Views.Element<TextBlock>(lines, "limit.pace")));
        Assert.IsFalse(Views.Visible<Rectangle>(lines).Any(mark => AutomationProperties.GetAutomationId(mark) == "limit.paceMark"));
    }
}
