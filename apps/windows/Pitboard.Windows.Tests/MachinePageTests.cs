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
public sealed class MachinePageTests
{
    private static readonly ChecksShown TwoChecks = new(
        Lines:
        [
            new CheckLine(0, "Claude Code", Level.Ok, "Passed", "2.1.299", Advice: null),
            new CheckLine(
                1, "Pitboard home", Level.Warn, "Worth looking at",
                @"C:\Users\dana\AppData\Local\Temp\pitboard-fixture\home\AppData\Local\Pitboard is readable by others",
                "Make it yours alone."),
        ],
        Summary: "One thing is worth looking at.", Level: Level.Warn, Checking: false, Checked: "Checked at 09:00", Waiting: null);

    private static Snapshot With(Snapshot snapshot, ChecksShown checks) =>
        snapshot with { Machine = snapshot.Machine with { Checks = checks } };

    private static Task<MachinePage> Shown(ChecksShown checks) =>
        Views.Navigated<MachinePage>(Views.Context(new AppModel(new StandInModel(With(Snapshots.Read(1), checks)))));

    private static Rect Bounds(FrameworkElement element, UIElement within) =>
        element.TransformToVisual(within).TransformBounds(new Rect(0, 0, element.ActualWidth, element.ActualHeight));

    [UITestMethod]
    public async Task ThisPCSaysEachCheckUnderDoctorsLastLine()
    {
        var checks = new ChecksShown(
            Lines:
            [
                new CheckLine(0, "Claude Code", Level.Ok, "Passed", "2.1.299", Advice: null),
                new CheckLine(1, "account personal", Level.Warn, "Warning", "lapses in 2 days", "Use it to renew it."),
            ],
            Summary: "One thing is worth looking at.", Level: Level.Warn, Checking: false, Checked: "Checked at 09:00", Waiting: null);
        var model = new StandInModel(With(Snapshots.Read(1), checks));
        var page = await Views.Navigated<MachinePage>(Views.Context(new AppModel(model)));

        var texts = Views.Texts(page);
        foreach (var said in new[] { "One thing is worth looking at.", "Checked at 09:00", "account personal", "Use it to renew it.", "lapses in 2 days" })
        {
            Assert.Contains(said, texts);
        }

        Assert.IsTrue(Views.Element<AppBarButton>(page, "machine.check").IsEnabled);
        Assert.Contains(new Intent.PaneShown(Pane.Machine), model.Sent);
    }

    [UITestMethod]
    public async Task CheckAgainIsHeldBackWhileTheChecksRun()
    {
        var checks = new ChecksShown(Lines: [], Summary: "Everything looks right.", Level: Level.Ok, Checking: true, Checked: null, Waiting: null);
        var page = await Views.Navigated<MachinePage>(Views.Context(new AppModel(new StandInModel(With(Snapshots.Read(1), checks)))));

        Assert.IsFalse(Views.Element<AppBarButton>(page, "machine.check").IsEnabled);
        Assert.IsTrue(Views.Visible<ProgressRing>(page).Single().IsActive);
    }

    [UITestMethod]
    public async Task BeforeTheFirstCheckThePageSaysItIsChecking()
    {
        var page = await Views.Navigated<MachinePage>(Views.Context(new AppModel(new StandInModel(Snapshots.Read(1)))));

        Assert.Contains("Checking this PC…", Views.Texts(page));
    }

    // The summary was a band fixed over the list, which the rows slid under.
    [UITestMethod]
    public async Task TheSummaryScrollsWithTheChecks()
    {
        var page = await Shown(TwoChecks);
        var list = Views.Element<ListView>(page, "checks");

        var summary = (InfoBar)list.Header;
        Assert.IsTrue(summary.IsOpen);
        Assert.AreEqual("One thing is worth looking at.", summary.Title);
        Assert.IsFalse(summary.IsClosable);
    }

    // The model says how bad the summary is, as it says the summary's words.
    [UITestMethod]
    public async Task TheSummaryIsAsBadAsTheModelSays()
    {
        static async Task<InfoBarSeverity> Said(Level level)
        {
            var page = await Shown(TwoChecks with { Level = level });
            return Views.Element<InfoBar>(page, "machine.summary").Severity;
        }

        Assert.AreEqual(InfoBarSeverity.Success, await Said(Level.Ok));
        Assert.AreEqual(InfoBarSeverity.Warning, await Said(Level.Warn));
        Assert.AreEqual(InfoBarSeverity.Error, await Said(Level.Fail));
    }

    // The detail sat right-aligned in a column of its own and broke a path mid-word.
    [UITestMethod]
    public async Task ACheckSaysItsDetailUnderItsNameLeftAligned()
    {
        var page = await Shown(TwoChecks);
        var row = Views.Visible<ListViewItem>(page).ElementAt(1);
        var name = Views.Visible<TextBlock>(row).Single(text => text.Text == "Pitboard home");
        var detail = Views.Visible<TextBlock>(row).Single(text => text.Text.EndsWith("is readable by others", StringComparison.Ordinal));
        var advice = Views.Visible<TextBlock>(row).Single(text => text.Text == "Make it yours alone.");

        Assert.AreEqual(Bounds(name, row).Left, Bounds(detail, row).Left, 0.5);
        Assert.IsGreaterThanOrEqualTo(Bounds(name, row).Bottom, Bounds(detail, row).Top);
        Assert.IsGreaterThanOrEqualTo(Bounds(detail, row).Bottom, Bounds(advice, row).Top);
        Assert.AreEqual(TextAlignment.Left, detail.TextAlignment);
        Assert.IsTrue(detail.IsTextSelectionEnabled);
        Assert.AreEqual(Bounds(name, row).Left, Bounds(advice, row).Left, 0.5);
    }

    // A detail's paths carry the marks that wrap them by folder, which TextBlock's own copy would keep.
    [UITestMethod]
    public async Task CopyingADetailGivesWhatItSays()
    {
        var copied = new List<string>();
        var page = await Views.Navigated<MachinePage>(
            Views.Context(new AppModel(new StandInModel(With(Snapshots.Read(1), TwoChecks))), copy: copied.Add));
        var row = Views.Visible<ListViewItem>(page).ElementAt(1);
        var detail = Views.Visible<TextBlock>(row).Single(text => text.IsTextSelectionEnabled);
        Assert.AreNotEqual(TwoChecks.Lines[1].Detail, detail.Text, "the path is shown as it is");

        detail.SelectAll();
        var menu = (MenuFlyout)detail.ContextFlyout;
        menu.ShowAt(detail);
        await Views.Until(() => menu.IsOpen, "the detail's menu did not open");
        new MenuFlyoutItemAutomationPeer((MenuFlyoutItem)menu.Items[0]).Invoke();

        Assert.AreEqual(TwoChecks.Lines[1].Detail, copied.Single());
    }

    [UITestMethod]
    public async Task ACheckWithNothingToDoSaysNoAdvice()
    {
        var page = await Shown(TwoChecks);
        var row = Views.Visible<ListViewItem>(page).First();

        Assert.AreEqual("Claude Code, 2.1.299", string.Join(", ", Views.Texts(row)));
        Assert.AreEqual("Passed, Claude Code, 2.1.299", AutomationProperties.GetName(row));
    }
}
