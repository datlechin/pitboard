using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class PageRequestsTests
{
    private static readonly NavigationPolicy ChatGPT = new(Items.Site("codex"), "https");

    private static readonly NavigationPolicy Fixture = ChatGPT with { Scheme = WebEnvironment.FixtureScheme };

    [TestMethod]
    public void APagesOwnNavigationAsksAsWhatItShows()
    {
        var request = PageRequests.Navigation(
            ChatGPT, PageRole.Window, "https://chatgpt.com/c/1", frame: false, userInitiated: true, shown: "https://chatgpt.com/");

        Assert.AreEqual(
            new NavigationRequest("https://chatgpt.com/c/1", NavigationTarget.Page, Clicked: true, Download: false, Asker.Site),
            request);
    }

    // As WebKit leaves out the frame of a load the app starts, before the window shows anything.
    [TestMethod]
    public void ALoadTheAppStartsAsksAsNoPage()
    {
        var request = PageRequests.Navigation(
            ChatGPT, PageRole.Window, "https://chatgpt.com/", frame: false, userInitiated: false, shown: "about:blank");

        Assert.AreEqual(Asker.Other, request.Asker);
    }

    [TestMethod]
    public void AFramesNavigationIsTheFramesOwn()
    {
        var request = PageRequests.Navigation(
            ChatGPT, PageRole.Window, "https://artifact.example/", frame: true, userInitiated: false, shown: null);

        Assert.AreEqual(NavigationTarget.Frame, request.Target);
        Assert.AreEqual(Asker.Other, request.Asker);
    }

    [TestMethod]
    public void ANewWindowAsksAsTheFrameThatAskedForIt()
    {
        NavigationRequest From(string? source, bool mainFrame) =>
            PageRequests.NewWindow(ChatGPT, PageRole.Window, "https://auth.openai.com/", false, source, mainFrame);

        Assert.AreEqual(Asker.Site, From("https://chatgpt.com/c/1", mainFrame: true).Asker);
        Assert.AreEqual(Asker.SignIn, From("https://auth.openai.com/log-in", mainFrame: true).Asker);
        Assert.AreEqual(Asker.Other, From("https://chatgpt.com/c/1", mainFrame: false).Asker, "a frame of the site's own");
        Assert.AreEqual(Asker.Other, From(null, mainFrame: false).Asker, "a request WebView2 named no frame for");
        Assert.AreEqual(NavigationTarget.NewWindow, From("https://chatgpt.com/", mainFrame: true).Target);
    }

    [TestMethod]
    public void ASignInWindowsOwnPageAsksAsTheSignIn()
    {
        var own = PageRequests.Navigation(ChatGPT, PageRole.Popup, "about:blank", frame: false, userInitiated: false, shown: null);
        var frame = PageRequests.Navigation(ChatGPT, PageRole.Popup, "about:blank", frame: true, userInitiated: false, shown: null);

        Assert.AreEqual(Asker.SignIn, own.Asker);
        Assert.AreEqual(Asker.Other, frame.Asker);
    }

    [TestMethod]
    public void AFixturesPagesAreItsSitesOnItsScheme()
    {
        var signIn = ChatGPT.Site.SignInHosts[0];
        var request = PageRequests.NewWindow(
            Fixture, PageRole.Window, $"pitboard-fixture://{signIn}/sign-in", false, "pitboard-fixture://chatgpt.com/", mainFrame: true);

        Assert.AreEqual(Asker.Site, request.Asker);
        Assert.IsInstanceOfType<NavigationDecision.Popup>(PitboardFfiMethods.DecideNavigation(Fixture, PageRole.Window, request));
    }

    // WebView2 raises DownloadStarting only for what it will not show.
    [TestMethod]
    public void ADownloadIsAResponseThePageCannotShow()
    {
        var site = PageRequests.Download("https://chatgpt.com/files/x.pdf", "attachment; filename=x.pdf", mainFrame: true);
        var none = PageRequests.Download("data:text/plain,x", "", mainFrame: false);

        Assert.AreEqual(new ResponseFacts("https://chatgpt.com/files/x.pdf", MainFrame: true, CanShow: false, "attachment; filename=x.pdf"), site);
        Assert.IsNull(none.Disposition, "WebView2 gives an empty header for none");
        Assert.AreEqual(new ResponseDecision.Download(Ask: false), PitboardFfiMethods.DecideResponse(ChatGPT, PageRole.Window, site));
        Assert.AreEqual(new ResponseDecision.Download(Ask: true), PitboardFfiMethods.DecideResponse(ChatGPT, PageRole.Window, none));
        Assert.AreEqual(new ResponseDecision.Ignore(), PitboardFfiMethods.DecideResponse(ChatGPT, PageRole.Popup, site));
    }

    [TestMethod]
    public void AnOriginNamesAPortOnlyWhereItIsNotTheSchemesOwn()
    {
        Assert.AreEqual(new FrameOrigin(true, "http", "127.0.0.1", 8123), PageRequests.Origin("http://127.0.0.1:8123/set", true));
        Assert.AreEqual(new FrameOrigin(true, "https", "chatgpt.com", 0), PageRequests.Origin("https://chatgpt.com:443/", true));
        Assert.AreEqual(new FrameOrigin(false, "pitboard-fixture", "chatgpt.com", 0), PageRequests.Origin("pitboard-fixture://chatgpt.com/", false));
        Assert.AreEqual(new FrameOrigin(true, "", "", 0), PageRequests.Origin("", true));
    }
}
