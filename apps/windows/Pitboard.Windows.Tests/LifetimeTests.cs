using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class LifetimeTests
{
    private const string Icon = @"C:\Users\Dana Lee\AppData\Local\Programs\Pitboard\Pitboard.ico";

    private sealed record Parts(
        Lifetime Lifetime, NotifyIconTests.StandInArea Area, StandInUpdates Updates, StandInModel Model,
        WindowsNotificationsTests.StandInNotifications Notifications, List<string> Done);

    private static Parts Made()
    {
        var area = new NotifyIconTests.StandInArea();
        var icon = new NotifyIcon(area, 0x1234, 0x5678, iconGuid: null);
        icon.Show("Pitboard");
        var system = new WindowsNotificationsTests.StandInNotifications();
        var notifications = new WindowsNotifications(Build.Release, () => system, Icon);
        notifications.Start(_ => { });
        var updates = new StandInUpdates();
        updates.Start();
        var model = new StandInModel(Snapshots.Read(1));
        var done = new List<string>();
        var lifetime = new Lifetime(
            new AppModel(model), icon, updates, notifications,
            () => done.Add(model.ShutDown ? "stop downloads after the model" : "stop downloads"),
            () => done.Add(model.ShutDown && updates.Shutdowns == 1 ? "exit" : "exit before letting go"));
        return new Parts(lifetime, area, updates, model, system, done);
    }

    [TestMethod]
    public void TheSessionsEndRemovesTheIconAndStopsTheUpdaterTheModelAndNotifications()
    {
        var parts = Made();

        parts.Lifetime.EndSession();

        Assert.AreEqual(IconMessage.Delete, parts.Area.Sent[^1].Message);
        Assert.AreEqual(1, parts.Updates.Shutdowns);
        Assert.IsTrue(parts.Model.ShutDown);
        Assert.AreEqual("unregister", parts.Notifications.Asked[^1]);
        Assert.IsEmpty(parts.Done, "the session's end stopped the downloads or exited");
    }

    // Exit, Restart Manager and an update's installer: downloads go with the process, so they are ended first.
    [TestMethod]
    public void EndingStopsTheDownloadsLetsGoAndExits()
    {
        var parts = Made();

        parts.Lifetime.End();

        Assert.AreEqual("stop downloads, exit", string.Join(", ", parts.Done));
        Assert.AreEqual(IconMessage.Delete, parts.Area.Sent[^1].Message);
        Assert.AreEqual(1, parts.Updates.Shutdowns);
        Assert.IsTrue(parts.Model.ShutDown);
        Assert.AreEqual("unregister", parts.Notifications.Asked[^1]);
    }
}
