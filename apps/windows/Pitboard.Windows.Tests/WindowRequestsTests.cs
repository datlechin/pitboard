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
        Assert.IsTrue(requests.Opens(new WindowRequest(3, new Place.Checks())));
        Assert.IsFalse(requests.Opens(new WindowRequest(2, null)), "an older one");
    }

    // Opened again some other way, the window stays on the pane it was left on.
    [TestMethod]
    public void TheWindowShowsThePaneItIsAskedForOnce()
    {
        var requests = new WindowRequests();
        var notice = new WindowRequest(1, new Place.Accounts());

        Assert.AreEqual(Pane.Accounts, requests.PaneFor(notice, Pane.Checks).Pane);
        Assert.AreEqual(Pane.Activity, requests.PaneFor(notice, Pane.Activity).Pane, "answered already");
        Assert.AreEqual(Pane.Activity, requests.PaneFor(new WindowRequest(2, null), Pane.Activity).Pane);
        Assert.AreEqual(Pane.Checks, requests.PaneFor(new WindowRequest(3, new Place.Checks()), Pane.Activity).Pane);
        Assert.AreEqual(Pane.Activity, requests.PaneFor(new WindowRequest(4, new Place.Activity()), null).Pane);
        Assert.AreEqual(Pane.Accounts, requests.PaneFor(new WindowRequest(5, new Place.Welcome()), null).Pane, "no welcome yet");
        Assert.IsNull(requests.PaneFor(new WindowRequest(6, null), null).Pane, "the settings page");
    }

    // What switching or warming up by itself came to is said beside its setting, under Limits.
    [TestMethod]
    public void ARequestForTheSettingsShowsTheSettingsPageOnItsSection()
    {
        var requests = new WindowRequests();
        var limits = new WindowRequest(1, new Place.Settings(SettingsSection.Limits));

        Assert.AreEqual((null, true), requests.PaneFor(limits, Pane.Accounts));
        Assert.AreEqual((Pane.Accounts, false), requests.PaneFor(limits, Pane.Accounts), "answered already");
        Assert.AreEqual((null, false), requests.PaneFor(new WindowRequest(2, new Place.Settings(SettingsSection.General)), Pane.Accounts));
        Assert.AreEqual((null, false), requests.PaneFor(new WindowRequest(3, null), null), "the settings page as it was left");
    }
}
