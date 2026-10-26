using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class MachineViewModelTests
{
    private static readonly ChecksShown Checked = new(
        Groups:
        [
            new CheckGroup("claude", "Claude Code",
            [
                new CheckLine(0, "Claude Code", Level.Ok, "Passed", "2.1.299", Advice: null, Action: null, Learn: null),
                new CheckLine(1, "Account personal", Level.Warn, "Worth looking at", "Lapses in 2 days", "Use it to renew it.", Action: null, Learn: null),
            ]),
        ],
        Summary: "One thing is worth looking at.", Verdict: null, Level: Level.Warn, Checking: false, Checked: "Checked at 09:00",
        Waiting: null, Report: null);

    private static Snapshot With(ulong revision, ChecksShown checks)
    {
        var read = Snapshots.Read(revision);
        return read with { Machine = read.Machine with { Checks = checks } };
    }

    private static (MachineViewModel Shown, AppModel App, List<string> Told) Over(ChecksShown checks)
    {
        var app = new AppModel(new StandInModel(With(1, checks)));
        var shown = new MachineViewModel(app);
        shown.Listen();
        var told = new List<string>();
        shown.PropertyChanged += (_, args) => told.Add(args.PropertyName ?? "");
        return (shown, app, told);
    }

    [TestMethod]
    public void TheChecksAreSaidUnderDoctorsLastLine()
    {
        var (shown, _, _) = Over(Checked);

        Assert.HasCount(2, shown.Lines);
        Assert.IsTrue(shown.HasSummary);
        Assert.AreEqual("One thing is worth looking at.", shown.Summary);
        Assert.AreEqual(Level.Warn, shown.Level);
        Assert.AreEqual("Checked at 09:00", shown.Checked);
        Assert.IsTrue(shown.CanCheck);
        Assert.IsFalse(shown.Waiting);
    }

    // The summary's level is the model's, as its words are, whatever the app would make of the lines.
    [TestMethod]
    public void TheSummarysLevelIsTheModels()
    {
        Assert.AreEqual(Level.Fail, Over(Checked with { Level = Level.Fail }).Shown.Level);
        Assert.AreEqual(Level.Ok, Over(Checked with { Level = Level.Ok }).Shown.Level);
    }

    [TestMethod]
    public void BeforeTheFirstCheckItSaysItIsChecking()
    {
        var (shown, _, _) = Over(new ChecksShown(
            Groups: [], Summary: null, Verdict: null, Level: Level.Ok, Checking: false, Checked: null, Waiting: "Checking this PC…",
            Report: null));

        Assert.IsTrue(shown.Waiting);
        Assert.AreEqual("Checking this PC…", shown.WaitingText);
        Assert.IsFalse(shown.HasSummary);
    }

    // A list told its lines again goes back to its top: checking again leaves them be until they change.
    [TestMethod]
    public void CheckingAgainLeavesTheLinesBe()
    {
        var (shown, app, told) = Over(Checked);

        app.Apply(With(2, Checked with { Checking = true, Checked = null }));

        Assert.AreEqual("Checking, CanCheck, Checked", string.Join(", ", told));
        Assert.IsFalse(shown.CanCheck);
        Assert.AreEqual("", shown.Checked);

        told.Clear();
        var first = Checked.Groups[0];
        app.Apply(With(3, Checked with
        {
            Groups = [first with { Lines = [first.Lines[0]] }],
            Summary = "Everything looks right.",
            Level = Level.Ok,
        }));

        Assert.AreEqual("Lines, HasSummary, Summary, Level, Checking, CanCheck, Checked", string.Join(", ", told));
        Assert.AreEqual(Level.Ok, shown.Level);
    }

    [TestMethod]
    public void CheckingAgainAsksTheModel()
    {
        var model = new StandInModel(With(1, Checked));
        var shown = new MachineViewModel(new AppModel(model));

        shown.Check();

        Assert.AreEqual(new Intent.PaneShown(Pane.Checks), model.Sent.Single());
    }
}
