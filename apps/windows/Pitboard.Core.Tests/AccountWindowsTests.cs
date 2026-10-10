namespace Pitboard.Core.Tests;

/// <summary>
/// The rules of an account's window, reached from C# as the Windows app's WebView2 code will
/// reach them: which accounts have a window and which store keeps each one's data, where a
/// page may go, and what a window says. What each rule says is the core's to test; this
/// proves the records, the enums and their variants cross.
/// </summary>
[TestClass]
public sealed class AccountWindowsTests
{
    private static readonly string[] CodexLabels = ["main", "spare"];

    private static Account Enrolled(string label, string provider, string uuid) =>
        new(
            Id: $"{provider}:{uuid}", Provider: provider, Label: label,
            Qualified: $"{provider}/{label}", Unplaced: false, Email: $"{label}@example.com",
            AccountId: uuid, SignedIn: false, Switchable: true, Parked: null, Usage: null,
            Stale: null, StaleExplanation: null, Plan: null);

    /// <summary>
    /// The store is the one the macOS app keeps each window's data under, so it names the
    /// WebView2 profile of the same account, and a window is found by it in any case.
    /// </summary>
    [TestMethod]
    public void EachEnrolledAccountHasAWindowOnItsToolsSite()
    {
        Account[] accounts =
        [
            Enrolled("work", "claude", "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f"),
            Enrolled("main", "codex", "team_user-1"),
            Enrolled("spare", "codex", "team_user-2"),
        ];

        var windows = PitboardFfiMethods.WindowAccounts(accounts);
        var menus = PitboardFfiMethods.SiteMenus(accounts);

        Assert.AreEqual("7e15c34f-69ec-55b4-9542-f1c1fe3d7085", windows[0].Store);
        Assert.AreEqual(
            windows[0].Store,
            PitboardFfiMethods.StoreId(PitboardFfiMethods.Sites()[0], "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f"));
        Assert.AreEqual(
            "main", PitboardFfiMethods.WindowOfStore(accounts, "8FF9E0E6-A7E2-53E2-A594-6E53DDADD38A")?.Label);
        var one = Assert.IsInstanceOfType<SiteMenu.One>(menus[0]);
        Assert.AreEqual("Open claude.ai as work", one.Title);
        var several = Assert.IsInstanceOfType<SiteMenu.Several>(menus[1]);
        Assert.AreEqual("Open chatgpt.com", several.Title);
        CollectionAssert.AreEqual(CodexLabels, several.Windows.Select(window => window.Label).ToArray());
    }

    /// <summary>
    /// A decision comes back as its variant, with what the window says about it.
    /// </summary>
    [TestMethod]
    public void APageGoesWhereThePolicySays()
    {
        var policy = new NavigationPolicy(PitboardFfiMethods.SitesFor("codex").Single(), "https");
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

    /// <summary>
    /// What a window says, and what its dialogs and downloads are titled.
    /// </summary>
    [TestMethod]
    public void AWindowSaysWhatHappenedAndWhoAsks()
    {
        var main = PitboardFfiMethods.WindowAccounts([Enrolled("main", "codex", "team_user-1")]).Single();

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
