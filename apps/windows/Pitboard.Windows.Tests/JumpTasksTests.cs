using System.Runtime.InteropServices;
using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class JumpTasksTests
{
    private const string Personal = "35c0d1c9-6b25-5cdc-b543-64c0c0d8de46";

    private static readonly QuickAction[] Tasks =
    [
        new("Switch to personal", new QuickAsk.Switch(Personal)),
        new("Add account…", new QuickAsk.AddAccount()),
        new("Open claude.ai as work", new QuickAsk.OpenWindow("claude-work")),
    ];

    private sealed record Parts(
        JumpTasks Jumps, AppModel App, StandInModel Model, StandInJumpList List, List<string> Opened, List<Notice> Posted);

    private static Parts Made(Snapshot first, StandInJumpList? list = null)
    {
        var model = new StandInModel(first);
        var app = new AppModel(model);
        var shown = list ?? new StandInJumpList();
        var opened = new List<string>();
        var posted = new List<Notice>();
        return new Parts(new JumpTasks(app, shown, window => opened.Add(window.Store), posted.Add), app, model, shown, opened, posted);
    }

    // What the model shows once it has read the accounts.
    private static Snapshot Read(ulong revision, params OutsideAnswer[] answers)
    {
        var read = Snapshots.Read(revision);
        return read with
        {
            AccountWindows = read.AccountWindows with { Accounts = [Items.Window("claude", "work")] },
            Answers = answers,
            QuickActions = Tasks,
        };
    }

    [TestMethod]
    public void TheListIsWrittenOnceTheModelHasOne()
    {
        var parts = Made(Snapshots.Read(1));

        parts.Jumps.Follow();
        Assert.IsEmpty(parts.List.Shown, "before the accounts are read");
        parts.App.Apply(Read(2));
        parts.Jumps.Follow();

        CollectionAssert.AreEqual(Tasks, parts.List.Shown.Single());
    }

    // Windows keeps the list it had. SetAppID gave CI's fresh standard user 0x80070003 (run 40494932963).
    [TestMethod]
    [DataRow(unchecked((int)0x80004005))]
    [DataRow(unchecked((int)0x80070003))]
    public void AListWindowsRefusesLeavesPitboardRunning(int refused)
    {
        var parts = Made(Read(1), new StandInJumpList { Refuses = Marshal.GetExceptionForHR(refused) });

        parts.Jumps.Follow();

        Assert.IsEmpty(parts.List.Shown);
    }

    [TestMethod]
    public void EachSwitchIsARequestFromOutsideWithAnIdOfItsOwn()
    {
        var parts = Made(Read(1));

        parts.Jumps.Take(new QuickAsk.Switch(Personal));
        parts.Jumps.Take(new QuickAsk.Switch(Personal));

        CollectionAssert.AreEqual(
            new Intent[]
            {
                new Intent.Outside(1, new OutsideAsk.Switch(Personal)), new Intent.Outside(2, new OutsideAsk.Switch(Personal)),
            },
            parts.Model.Sent.ToArray());
    }

    // A Jump List has no reply and asks nothing, so a switch it asked for that did not happen says why, once, in a
    // notification, the question its answer offers left unasked.
    [TestMethod]
    public void ASwitchRefusedSaysWhyInANotificationAndOneDoneSaysNothing()
    {
        var parts = Made(Read(1));
        parts.Jumps.Take(new QuickAsk.Switch(Personal));
        parts.Jumps.Take(new QuickAsk.Switch(Personal));
        var quit = new Question(
            "Quit ChatGPT to switch?",
            "ChatGPT keeps using the account it started with until it quits. Pitboard quits it, switches, and opens it again.",
            "Quit ChatGPT and switch", Destructive: false, ConfirmIsDefault: true);
        var refused = new OutsideAnswer(1, Done: false, "Open Pitboard to switch while ChatGPT is open.", quit);
        OutsideAnswer[] answers =
        [
            refused, new OutsideAnswer(2, Done: true, "personal is already in use.", Question: null),
            new OutsideAnswer(7, Done: false, "Pitboard is being uninstalled.", Question: null),
        ];

        parts.App.Apply(Read(2, answers));
        parts.Jumps.Answered();
        parts.App.Apply(Read(3, [.. answers, new OutsideAnswer(8, Done: false, "Pitboard is being uninstalled.", Question: null)]));
        parts.Jumps.Answered();

        CollectionAssert.AreEqual(new[] { PitboardModelMethods.OutsideRefused(Identity.Front, refused) }, parts.Posted);
    }

    [TestMethod]
    public void AddAccountOpensItsDialog()
    {
        var parts = Made(Read(1));

        parts.Jumps.Take(new QuickAsk.AddAccount());

        Assert.AreEqual(new Intent.PresentSheet(new Sheet.Add(Tool: null)), parts.Model.Sent.Single());
    }

    [TestMethod]
    public void AWindowTaskOpensTheWindowTheAccountsList()
    {
        var parts = Made(Read(1));

        parts.Jumps.Take(new QuickAsk.OpenWindow("CLAUDE-WORK"));
        parts.Jumps.Take(new QuickAsk.OpenWindow("claude-gone"));

        Assert.AreEqual("claude-work", parts.Opened.Single());
    }

    // A task can be what starts Pitboard, before it has read the accounts.
    [TestMethod]
    public void AWindowAskedBeforeTheReadOpensOnceTheAccountsAreRead()
    {
        var parts = Made(Snapshots.Read(1));

        parts.Jumps.Take(new QuickAsk.OpenWindow("claude-work"));
        Assert.IsEmpty(parts.Opened, "before the read");
        parts.App.Apply(Read(2));
        parts.Jumps.Follow();
        parts.Jumps.Follow();

        Assert.AreEqual("claude-work", parts.Opened.Single());
    }

    internal sealed class StandInJumpList : IJumpList
    {
        public List<QuickAction[]> Shown { get; } = [];

        public int Deleted { get; private set; }

        public Exception? Refuses { get; init; }

        public void Show(IReadOnlyList<QuickAction> tasks)
        {
            if (Refuses is { } refused)
            {
                throw refused;
            }

            Shown.Add([.. tasks]);
        }

        public void Delete()
        {
            Deleted++;
            if (Refuses is { } refused)
            {
                throw refused;
            }
        }
    }
}
