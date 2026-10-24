namespace Pitboard.Core.Tests;

// The core tests what each rule says; these prove its records, enums and variants cross.
[TestClass]
public sealed class AccountWindowsTests
{
    [TestMethod]
    public void APageGoesWhereThePolicySays()
    {
        var policy = new NavigationPolicy(Sites.ChatGpt, "https");
        NavigationDecision Decide(string url, bool clicked) =>
            PitboardFfiMethods.DecideNavigation(
                policy, PageRole.Window,
                new NavigationRequest(url, NavigationTarget.Page, clicked, false, Asker.Other));

        Assert.IsInstanceOfType<NavigationDecision.Load>(Decide("https://chatgpt.com/c/x", false));
        var away = Assert.IsInstanceOfType<NavigationDecision.OpenElsewhere>(
            Decide("https://example.com/connect", false));
        Assert.AreEqual("https://example.com/connect", away.Url);
        Assert.IsInstanceOfType<WindowNoteKind.OpenedInBrowser>(away.Note);
        var refused = Assert.IsInstanceOfType<NavigationDecision.Refuse>(
            Decide("vscode://file/x", true));
        var otherApp = Assert.IsInstanceOfType<WindowNoteKind.OtherApp>(refused.Note);
        Assert.AreEqual("vscode", otherApp.Scheme);
        Assert.AreEqual("https://chatgpt.com/", PitboardFfiMethods.WindowHome(policy));
    }

    [TestMethod]
    public void AWindowSaysWhatHappenedAndWhoAsks()
    {
        var main = new WindowAccount(
            Sites.ChatGpt, Label: "main", Email: "main@example.com", Store: "8ff9e0e6-a7e2-53e2-a594-6e53ddadd38a",
            InUse: false, Title: "main");

        StringAssert.StartsWith(
            PitboardFfiMethods.WindowNote(new WindowNoteKind.SignIn(), main),
            "Sign in to chatgpt.com as main@example.com.");
        Assert.AreEqual("chatgpt.com says", PitboardFfiMethods.DialogTitle("chatgpt.com", true));
        Assert.AreEqual(
            "Download “x.zip” from claude.ai?",
            PitboardFfiMethods.DownloadQuestion("x.zip", PitboardFfiMethods.DownloadHost("blob:https://claude.ai/1")).Title);
        Assert.IsFalse(PitboardFfiMethods.PageMayUse(PagePermission.Microphone));
        Assert.AreEqual(new SignInWindowSize(500, 640), PitboardFfiMethods.SignInWindowSize(null, null));
        Assert.IsInstanceOfType<ProcessEnded.Stopped>(PitboardFfiMethods.AfterContentProcessEnded(0, 5));
    }
}
