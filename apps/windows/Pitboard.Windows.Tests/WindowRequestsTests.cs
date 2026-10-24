using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class WindowRequestsTests
{
    [TestMethod]
    public void TheWindowOpensOnceForEachRequest()
    {
        var requests = new WindowRequests();

        Assert.IsFalse(requests.Opens(new WindowRequest(0, null)), "nothing asked yet");
        Assert.IsTrue(requests.Opens(new WindowRequest(1, null)));
        Assert.IsFalse(requests.Opens(new WindowRequest(1, null)), "answered already");
        Assert.IsTrue(requests.Opens(new WindowRequest(3, Pane.Machine)));
        Assert.IsFalse(requests.Opens(new WindowRequest(2, null)), "an older one");
    }

    // Opened again some other way, the window stays on the pane it was left on.
    [TestMethod]
    public void TheWindowShowsThePaneItIsAskedForOnce()
    {
        var requests = new WindowRequests();
        var notice = new WindowRequest(1, Pane.Accounts);

        Assert.AreEqual(Pane.Accounts, requests.PaneFor(notice, Pane.Machine).Pane);
        Assert.AreEqual(Pane.Activity, requests.PaneFor(notice, Pane.Activity).Pane, "answered already");
        Assert.AreEqual(Pane.Activity, requests.PaneFor(new WindowRequest(2, null), Pane.Activity).Pane);
        Assert.AreEqual(Pane.Machine, requests.PaneFor(new WindowRequest(3, Pane.Machine), Pane.Activity).Pane);
        Assert.IsNull(requests.PaneFor(new WindowRequest(3, null), null).Pane, "the settings page");
    }

    // What switching or warming up by itself came to is said beside its setting, under Limits.
    [TestMethod]
    public void ARequestForTheSettingsShowsTheSettingsPageOnItsLimits()
    {
        var requests = new WindowRequests();
        var settings = new WindowRequest(1, null, Settings: true);

        Assert.AreEqual((null, true), requests.PaneFor(settings, Pane.Accounts));
        Assert.AreEqual((Pane.Accounts, false), requests.PaneFor(settings, Pane.Accounts), "answered already");
        Assert.AreEqual((null, false), requests.PaneFor(new WindowRequest(2, null), null), "the settings page as it was left");
    }
}
