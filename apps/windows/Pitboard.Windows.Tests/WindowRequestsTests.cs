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

    // Opened again some other way, the window stays where it was left.
    [TestMethod]
    public void TheWindowShowsThePlaceItIsAskedForOnce()
    {
        var requests = new WindowRequests();
        var notice = new WindowRequest(1, new Place.Checks());

        Assert.AreEqual(new Place.Checks(), requests.PlaceFor(notice));
        Assert.IsNull(requests.PlaceFor(notice), "answered already");
        Assert.IsNull(requests.PlaceFor(new WindowRequest(2, null)), "the window as it was left");
        Assert.AreEqual(new Place.Activity(), requests.PlaceFor(new WindowRequest(3, new Place.Activity())));
        Assert.AreEqual(
            new Place.Settings(SettingsSection.Limits), requests.PlaceFor(new WindowRequest(4, new Place.Settings(SettingsSection.Limits))));
        Assert.AreEqual(new Place.Accounts(), requests.PlaceFor(new WindowRequest(5, new Place.Welcome())), "no welcome yet");
    }
}
