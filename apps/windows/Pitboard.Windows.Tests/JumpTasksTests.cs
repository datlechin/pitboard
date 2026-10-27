using System.Runtime.InteropServices;
using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class JumpTasksTests
{
    private const string Personal = "35c0d1c9-6b25-5cdc-b543-64c0c0d8de46";

    private static readonly JumpTask[] Tasks =
    [
        new("Switch to personal", new JumpAsk.Switch(Personal)),
        new("Add account…", new JumpAsk.AddAccount()),
        new("Open claude.ai as work", new JumpAsk.OpenWindow("claude-work")),
    ];

    private sealed record Parts(
        JumpTasks Jumps, AppModel App, StandInModel Model, StandInJumpList List, List<string> Opened, List<RunOutNotice> Posted);

    private static Parts Made(Snapshot first, StandInJumpList? list = null)
    {
        var model = new StandInModel(first);
        var app = new AppModel(model);
        var shown = list ?? new StandInJumpList();
        var opened = new List<string>();
        var posted = new List<RunOutNotice>();
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
            JumpList = Tasks,
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

    // Windows keeps the list it had. CI's fresh standard user got ERROR_PATH_NOT_FOUND from SetAppID (2026-10-27).
    [TestMethod]
    [DataRow(unchecked((int)0x80004005))]
    [DataRow(unchecked((int)0x80070005))]
    [DataRow(unchecked((int)0x80070003))]
    public void AListWindowsRefusesLeavesPitboardRunning(int refusal)
    {
        var parts = Made(Read(1), new StandInJumpList { Refuses = Marshal.GetExceptionForHR(refusal) });

        parts.Jumps.Follow();

        Assert.IsEmpty(parts.List.Shown);
    }

    [TestMethod]
    public void EachSwitchIsARequestFromOutsideWithAnIdOfItsOwn()
    {
        var parts = Made(Read(1));

        parts.Jumps.Take(new JumpAsk.Switch(Personal));
        parts.Jumps.Take(new JumpAsk.Switch(Personal));

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
        parts.Jumps.Take(new JumpAsk.Switch(Personal));
        parts.Jumps.Take(new JumpAsk.Switch(Personal));
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

        CollectionAssert.AreEqual(new[] { PitboardFfiMethods.JumpRefused(refused) }, parts.Posted);
    }

    [TestMethod]
    public void AddAccountOpensItsDialog()
    {
        var parts = Made(Read(1));

        parts.Jumps.Take(new JumpAsk.AddAccount());

        Assert.AreEqual(new Intent.PresentSheet(new Sheet.Add(Provider: null)), parts.Model.Sent.Single());
    }

    [TestMethod]
    public void AWindowTaskOpensTheWindowTheAccountsList()
    {
        var parts = Made(Read(1));

        parts.Jumps.Take(new JumpAsk.OpenWindow("CLAUDE-WORK"));
        parts.Jumps.Take(new JumpAsk.OpenWindow("claude-gone"));

        Assert.AreEqual("claude-work", parts.Opened.Single());
    }

    // A task can be what starts Pitboard, before it has read the accounts.
    [TestMethod]
    public void AWindowAskedBeforeTheReadOpensOnceTheAccountsAreRead()
    {
        var parts = Made(Snapshots.Read(1));

        parts.Jumps.Take(new JumpAsk.OpenWindow("claude-work"));
        Assert.IsEmpty(parts.Opened, "before the read");
        parts.App.Apply(Read(2));
        parts.Jumps.Follow();
        parts.Jumps.Follow();

        Assert.AreEqual("claude-work", parts.Opened.Single());
    }

    internal sealed class StandInJumpList : IJumpList
    {
        public List<JumpTask[]> Shown { get; } = [];

        public int Deleted { get; private set; }

        public Exception? Refuses { get; init; }

        public void Show(IReadOnlyList<JumpTask> tasks)
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
