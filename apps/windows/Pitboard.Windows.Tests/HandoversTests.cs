using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class HandoversTests
{
    private static readonly Handover Window = new Handover.Window();

    [TestMethod]
    public void ALaunchHandedOverWhilePitboardStartsOpensTheWindowOnceItCan()
    {
        var handovers = new Handovers();
        var taken = new List<Handover>();

        handovers.Arrived(Window);
        handovers.Arrived(new Handover.Window());
        handovers.Ready(taken.Add);

        CollectionAssert.AreEqual(new[] { Window }, taken);
    }

    [TestMethod]
    public void EachLaunchHandedOverLaterOpensTheWindow()
    {
        var handovers = new Handovers();
        var taken = new List<Handover>();
        handovers.Ready(taken.Add);

        handovers.Arrived(Window);
        handovers.Arrived(Window);

        CollectionAssert.AreEqual(new[] { Window, Window }, taken);
    }

    [TestMethod]
    public void NoLaunchHandedOverOpensNothing()
    {
        var handovers = new Handovers();
        var taken = new List<Handover>();

        handovers.Ready(taken.Add);

        Assert.AreEqual(0, taken.Count);
    }

    // A copy started a moment after the first hands its launch over between the first's key and its model.
    [TestMethod]
    public void ALaunchHandedOverBeforeTheAppCanTakeItWaitsForIt()
    {
        var handovers = new Handovers();
        var taken = new List<Handover>();

        handovers.Arrived(Window);
        handovers.Arrived(new Handover.Link("pitboard://open?url=a"));
        handovers.Ready(taken.Add);
        handovers.Arrived(new Handover.Link("pitboard://open?url=b"));

        CollectionAssert.AreEqual(
            new[] { Window, new Handover.Link("pitboard://open?url=a"), new Handover.Link("pitboard://open?url=b") }, taken);
    }

    [TestMethod]
    public void EachLinkHandedOverWhilePitboardStartsIsKept()
    {
        var handovers = new Handovers();
        var taken = new List<Handover>();
        var link = new Handover.Link("pitboard://open?url=a");

        handovers.Arrived(link);
        handovers.Arrived(link);
        handovers.Ready(taken.Add);

        CollectionAssert.AreEqual(new[] { link, link }, taken);
    }

    // Each is a request of its own, which the model answers on its own.
    [TestMethod]
    public void EachTaskHandedOverWhilePitboardStartsIsKept()
    {
        var handovers = new Handovers();
        var taken = new List<Handover>();
        var task = new Handover.Jump(new JumpAsk.Switch("35c0d1c9-6b25-5cdc-b543-64c0c0d8de46"));

        handovers.Arrived(task);
        handovers.Arrived(Window);
        handovers.Arrived(task);
        handovers.Ready(taken.Add);

        CollectionAssert.AreEqual(new[] { task, Window, task }, taken);
    }

    [TestMethod]
    public void ALaunchHandedOverOnAnotherThreadIsNeverLost()
    {
        for (var round = 0; round < 200; round++)
        {
            var handovers = new Handovers();
            var taken = 0;
            using var go = new Barrier(2);
            var arriving = Task.Run(() =>
            {
                go.SignalAndWait();
                handovers.Arrived(Window);
            });

            go.SignalAndWait();
            handovers.Ready(_ => Interlocked.Increment(ref taken));
            arriving.Wait();

            Assert.AreEqual(1, taken, $"round {round}");
        }
    }
}
