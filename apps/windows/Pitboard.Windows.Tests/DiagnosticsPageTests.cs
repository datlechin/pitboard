using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;
using Windows.Foundation;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class DiagnosticsPageTests
{
    private static readonly CheckLine Home = new(
        1, "Pitboard’s folder", Level.Warn, "Warning",
        @"C:\Users\dana\AppData\Local\Temp\pitboard-fixture\home\AppData\Local\Pitboard is readable by others",
        "Make it yours alone.", Action: null, Learn: new SetupLink("Learn more", "https://docs.usepitboard.com/reference/errors"));

    private static readonly CheckLine SavedSignIns = new(
        2, "Saved sign-ins", Level.Warn, "Warning", "old’s expired on Oct 21.", Advice: null,
        Action: new ItemOffer("Sign in…", new Intent.PresentSheet(new Sheet.SignInAgain("claude", "old")), Enabled: true, Confirm: null),
        Learn: null);

    private static readonly ChecksShown TwoGroups = new(
        Groups:
        [
            new CheckGroup("pitboard", "Pitboard", [Home]),
            new CheckGroup("claude", "Claude Code",
            [
                new CheckLine(0, "Claude Code", Level.Ok, "Passed", "Version 2.1.299", Advice: null, Action: null, Learn: null),
                SavedSignIns,
            ]),
        ],
        Summary: "2 issues", Verdict: "Everything else is set up.", Level: Level.Warn, Checking: false, Checked: "Checked at 9:00 AM",
        Waiting: null, Report: "{\"checks\":[]}");

    private static Snapshot With(Snapshot snapshot, ChecksShown checks) =>
        snapshot with { Machine = snapshot.Machine with { Checks = checks } };

    private static Task<DiagnosticsPage> Shown(ChecksShown checks, Action<string>? copy = null, Action<Uri>? open = null) =>
        Views.Navigated<DiagnosticsPage>(Views.Context(new AppModel(new StandInModel(With(Snapshots.Read(1), checks))), copy: copy, open: open));

    private static Rect Bounds(FrameworkElement element, UIElement within) =>
        element.TransformToVisual(within).TransformBounds(new Rect(0, 0, element.ActualWidth, element.ActualHeight));

    private static SettingsCard Card(Page page, CheckLine line) => Views.Element<SettingsCard>(page, $"check.{line.Id}");

    [UITestMethod]
    public async Task DiagnosticsSaysEachCheckUnderItsToolBelowDoctorsLastLine()
    {
        var model = new StandInModel(With(Snapshots.Read(1), TwoGroups));
        var page = await Views.Navigated<DiagnosticsPage>(Views.Context(new AppModel(model)));

        var texts = Views.Texts(page);
        foreach (var said in new[] { "Checked at 9:00 AM", "Pitboard", "Claude Code", "Saved sign-ins", "old’s expired on Oct 21.", "Make it yours alone." })
        {
            Assert.Contains(said, texts);
        }

        var summary = Views.Element<InfoBar>(page, "machine.summary");
        Assert.AreEqual("2 issues", summary.Title);
        Assert.AreEqual("Everything else is set up.", summary.Message);
        Assert.IsTrue(Views.Element<AppBarButton>(page, "machine.check").IsEnabled);
        Assert.Contains(new Intent.PaneShown(Pane.Checks), model.Sent);
    }

    // Each tool's name heads its checks as a settings section's does, and Narrator's H goes from one to the next.
    [UITestMethod]
    public async Task EachToolsNameIsALevel2Heading()
    {
        var page = await Shown(TwoGroups);

        var headings = Views.Visible<TextBlock>(page)
            .Where(text => AutomationProperties.GetHeadingLevel(text) == AutomationHeadingLevel.Level2)
            .Select(text => text.Text);

        Assert.AreEqual("Pitboard, Claude Code", string.Join(", ", headings));
    }

    [UITestMethod]
    public async Task CheckAgainIsHeldBackWhileTheChecksRun()
    {
        var checks = new ChecksShown(
            Groups: [], Summary: "Everything is set up.", Verdict: null, Level: Level.Ok, Checking: true, Checked: null, Waiting: null,
            Report: null);
        var page = await Shown(checks);

        Assert.IsFalse(Views.Element<AppBarButton>(page, "machine.check").IsEnabled);
        Assert.IsTrue(Views.Visible<ProgressRing>(page).Single().IsActive);
    }

    [UITestMethod]
    public async Task BeforeTheFirstCheckThePageSaysItIsChecking()
    {
        var page = await Views.Navigated<DiagnosticsPage>(Views.Context(new AppModel(new StandInModel(Snapshots.Read(1)))));

        Assert.Contains("Checking this PC…", Views.Texts(page));
        Assert.IsFalse(Views.Element<AppBarButton>(page, "machine.copyReport").IsEnabled, "a report before any check");
    }

    // The summary was a band fixed over the list, which the rows slid under.
    [UITestMethod]
    public async Task TheSummaryScrollsWithTheChecks()
    {
        var page = await Shown(TwoGroups);
        var scroller = Views.Element<ScrollViewer>(page, "checks");
        var summary = Views.Element<InfoBar>(page, "machine.summary");

        Assert.IsTrue(Views.Descendants(scroller).Contains(summary));
        Assert.IsTrue(summary.IsOpen);
        Assert.IsFalse(summary.IsClosable);
    }

    // The model says how bad the summary is, as it says the summary's words.
    [UITestMethod]
    public async Task TheSummaryIsAsBadAsTheModelSays()
    {
        static async Task<InfoBarSeverity> Said(Level level)
        {
            var page = await Shown(TwoGroups with { Level = level });
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
        var page = await Shown(TwoGroups);
        var card = Card(page, Home);
        var name = Views.Visible<TextBlock>(card).First(text => text.Text == "Pitboard’s folder");
        var detail = Views.Visible<TextBlock>(card).Single(text => text.Text.EndsWith("is readable by others", StringComparison.Ordinal));
        var advice = Views.Visible<TextBlock>(card).Single(text => text.Text == "Make it yours alone.");

        Assert.AreEqual(Bounds(name, card).Left, Bounds(detail, card).Left, 0.5);
        Assert.IsGreaterThanOrEqualTo(Bounds(name, card).Bottom, Bounds(detail, card).Top);
        Assert.IsGreaterThanOrEqualTo(Bounds(detail, card).Bottom, Bounds(advice, card).Top);
        Assert.AreEqual(TextAlignment.Left, detail.TextAlignment);
        Assert.IsTrue(detail.IsTextSelectionEnabled);
    }

    // A detail's paths carry the marks that wrap them by folder, which TextBlock's own copy would keep.
    [UITestMethod]
    public async Task CopyingADetailGivesWhatItSays()
    {
        var copied = new List<string>();
        var page = await Shown(TwoGroups, copy: copied.Add);
        var detail = Views.Visible<TextBlock>(Card(page, Home)).Single(text => text.IsTextSelectionEnabled);
        Assert.AreNotEqual(Home.Detail, detail.Text, "the path is shown as it is");

        detail.SelectAll();
        var menu = (MenuFlyout)detail.ContextFlyout;
        menu.ShowAt(detail);
        await Views.Until(() => menu.IsOpen, "the detail's menu did not open");
        new MenuFlyoutItemAutomationPeer((MenuFlyoutItem)menu.Items[0]).Invoke();

        Assert.AreEqual(Home.Detail, copied.Single());
    }

    // A check that did not pass ends with its fix in the app, or the docs where nothing in the app fixes it.
    [UITestMethod]
    public async Task ACheckOffersItsFixOrWhereTheDocsExplainIt()
    {
        var opened = new List<Uri>();
        var model = new StandInModel(With(Snapshots.Read(1), TwoGroups));
        var page = await Views.Navigated<DiagnosticsPage>(Views.Context(new AppModel(model), open: opened.Add));

        Views.Press(Views.Button(Card(page, SavedSignIns), "Sign in…"));
        new HyperlinkButtonAutomationPeer(Views.Visible<HyperlinkButton>(Card(page, Home)).Single()).Invoke();

        Assert.Contains(SavedSignIns.Action!.Intent, model.Sent);
        Assert.AreEqual(new Uri(Home.Learn!.Link), opened.Single());
    }

    // What Copy Report copies is doctor's own report, as the model holds it.
    [UITestMethod]
    public async Task CopyReportCopiesDoctorsReport()
    {
        var copied = new List<string>();
        var page = await Shown(TwoGroups, copy: copied.Add);

        new ButtonAutomationPeer(Views.Element<AppBarButton>(page, "machine.copyReport")).Invoke();

        Assert.AreEqual(TwoGroups.Report, copied.Single());
    }

    [UITestMethod]
    public async Task ACheckIsReadAsOneLine()
    {
        var page = await Shown(TwoGroups);
        var card = Card(page, TwoGroups.Groups[1].Lines[0]);

        Assert.AreEqual("Passed, Claude Code, Version 2.1.299", AutomationProperties.GetName(card));
        Assert.IsFalse(Views.Visible<ButtonBase>(card).Any(button => button != card), "something to press on a check that passed");
    }
}
