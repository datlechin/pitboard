using System.ComponentModel;

namespace Pitboard.Share.Core.Tests;

[TestClass]
public sealed class SharingTests
{
    private static readonly ShareWords Words = PitboardShareFfiMethods.ShareWords();

    [TestMethod]
    public async Task ASitesPageStartsPitboardWithItsPitboardLinkAndCompletes()
    {
        var share = new RecordedShare(webLink: "https://chat.openai.com/c/x?y=1#z");

        await Sharing.HandOverAsync(share, "pitboard");

        string[] said = ["started", "Pitboard.exe pitboard://open?url=https%3A%2F%2Fchatgpt.com%2Fc%2Fx%3Fy%3D1%23z", "completed"];
        CollectionAssert.AreEqual(said, share.Said);
    }

    // The refusal's own sentence, as pitboard-sites says it.
    [TestMethod]
    [DataRow("https://claude.ai/magic-link#a:b", DisplayName = "a sign-in page")]
    [DataRow("https://example.com/", DisplayName = "another site")]
    [DataRow(null, DisplayName = "no web link")]
    public async Task APageItRefusesReportsTheRefusalAndStartsNothing(string? webLink)
    {
        var share = new RecordedShare(webLink);
        var refused = Assert.ThrowsExactly<ShareRefusal.Refused>(() => PitboardShareFfiMethods.ShareLink(webLink ?? "", "pitboard"));

        await Sharing.HandOverAsync(share, "pitboard");

        string[] said = ["started", "error: " + refused.reason];
        CollectionAssert.AreEqual(said, share.Said);
    }

    // The source app gone, or its delayed rendering failed.
    [TestMethod]
    public async Task AShareWhoseLinkCannotBeReadIsReportedAsAnError()
    {
        var share = new RecordedShare(unreadable: true);

        await Sharing.HandOverAsync(share, "pitboard");

        string[] said = ["started", "error: " + Words.NotRead];
        CollectionAssert.AreEqual(said, share.Said);
    }

    [TestMethod]
    public async Task APitboardExeThatDoesNotStartIsReportedAsAnError()
    {
        var share = new RecordedShare(webLink: "https://chatgpt.com/", startFails: true);

        await Sharing.HandOverAsync(share, "pitboard");

        string[] said = ["started", "Pitboard.exe pitboard://open?url=https%3A%2F%2Fchatgpt.com%2F", "error: " + Words.NotStarted];
        CollectionAssert.AreEqual(said, share.Said);
    }

    private sealed class RecordedShare(string? webLink = null, bool unreadable = false, bool startFails = false)
        : IShareOperation
    {
        public List<string> Said { get; } = [];

        public void ReportStarted() => Said.Add("started");

        public Task<string?> WebLinkAsync() =>
            unreadable ? Task.FromException<string?>(new InvalidOperationException("The data is no longer there.")) : Task.FromResult(webLink);

        public void StartPitboard(string link)
        {
            Said.Add("Pitboard.exe " + link);
            if (startFails)
            {
                throw new Win32Exception(2, "The system cannot find the file specified.");
            }
        }

        public void ReportCompleted() => Said.Add("completed");

        public void ReportError(string sentence) => Said.Add("error: " + sentence);
    }
}
