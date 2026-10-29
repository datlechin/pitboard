using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

// Settings > Accessibility > Text size grows text up to 225%: text wraps rather than clips, and a mark keeps its size.
[TestClass]
public sealed class TextSizeTests
{
    private static readonly string[] Marks = ["Mark", "InUseCheck", "EmptyMark", "LevelMark"];

    [UITestMethod]
    public async Task EachActivityCellWraps()
    {
        var activity = new ActivityShown(
            Lines: [new ActivityLine(0, "15/01/2027 09:00", "Switch", "claude/work", "done", Done: true, AskedBy: "pitboard")],
            Empty: null);
        var snapshot = Snapshots.Read(1);
        var pane = await Views.Navigated<ActivityPage>(Views.Context(new AppModel(new StandInModel(
            snapshot with { Machine = snapshot.Machine with { Activity = activity } }))));

        var cells = Views.Descendants((Grid)pane.FindName("Columns"))
            .Concat(Views.Visible<ListViewItem>(pane).SelectMany(Views.Descendants))
            .OfType<TextBlock>()
            .ToArray();

        Assert.HasCount(10, cells);
        Assert.AreEqual("", string.Join(", ", cells.Where(text => text.TextWrapping == TextWrapping.NoWrap).Select(text => text.Text)));
    }

    [UITestMethod]
    public async Task NoMarkGrowsWithTheText()
    {
        var snapshot = Snapshots.Read(1);
        var checks = new ChecksShown(
            Groups: [new CheckGroup("claude", "Claude Code", [new CheckLine(0, "Claude Code", Level.Ok, "Passed", "2.1.299", Advice: null, Action: null, Learn: null)])],
            Summary: "", Verdict: null, Level: Level.Ok, Checking: false, Checked: "Checked at 09:00", Waiting: null, Report: null);
        FrameworkElement[] views =
        [
            await Views.Navigated<AccountsPage>(Views.Context(new AppModel(new StandInModel(
                snapshot with { Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))] })))),
            await Views.Navigated<ActivityPage>(Views.Context(new AppModel(new StandInModel(snapshot)))),
            await Views.Navigated<DiagnosticsPage>(Views.Context(new AppModel(new StandInModel(
                snapshot with { Machine = snapshot.Machine with { Checks = checks } })))),
            await Views.Navigated<AccountsPage>(Views.Context(
                new AppModel(new StandInModel(snapshot with
                {
                    AccountsShown = new AccountsShown.NoAccounts(
                        "No accounts", "Sign in to add one.",
                        new Choice("Add account…", new Intent.PresentSheet(new Sheet.Add(Tool: null)), Enabled: true)),
                })))),
        ];

        foreach (var view in views)
        {
            var marks = Views.Descendants(view).OfType<FontIcon>().Where(icon => Marks.Contains(icon.Name)).ToArray();
            Assert.IsNotEmpty(marks, view.GetType().Name);
            Assert.AreEqual(0, marks.Count(icon => icon.IsTextScaleFactorEnabled), $"marks that grow with the text in {view.GetType().Name}");
        }
    }
}
