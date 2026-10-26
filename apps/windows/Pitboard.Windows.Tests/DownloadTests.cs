using System.Text;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Microsoft.Web.WebView2.Core;
using Pitboard.Core;

namespace Pitboard.Windows.Tests;

// An account window's downloads as WebView2 runs them (CF7), over the fixture's stand-in pages and a stand-in site on
// the loopback, with no model. Each kind prints the events WebView2 raised for it, and whether it was asked about.
[TestClass]
public sealed class DownloadTests
{
    private static readonly WindowAccount Main = Items.Window("codex", "main", inUse: true);

    private static readonly string Home = $"{WebEnvironment.FixtureScheme}://chatgpt.com/";

    private static readonly string Report = $"{WebEnvironment.FixtureScheme}://chatgpt.com/files/report.pdf";

    [UITestMethod]
    public async Task EachKindOfDownloadSavesAtTheNumberedPath()
    {
        using var site = new StandInSite();
        await using var webs = await Webs.Make();
        var window = new StandInWindow(webs, Main, answer: true);
        var page = await Open(webs, window);
        var events = Events(page);
        // The site's own file, as the site serves one: after the fixture's pages, so this answer is the one taken.
        page.WebResourceRequested += (_, args) =>
        {
            if (args.Request.Uri == Report)
            {
                args.Response = page.Environment.CreateWebResourceResponse(
                    new MemoryStream(Encoding.UTF8.GetBytes("report")).AsRandomAccessStream(), 200, "OK",
                    "Content-Type: application/pdf\r\nContent-Disposition: attachment; filename=\"report.pdf\"");
            }
        };
        Directory.CreateDirectory(webs.Web.Downloads);
        var failures = new List<string>();

        // A download the page's own navigation became is the page's; one a script starts names no frame, so it asks.
        foreach (var (kind, name, saved, content, asks, script) in new[]
        {
            // WebView2 starts no download for an <a download> of the fixture's scheme (run 38825074643).
            ("<a download> in a frame", "page.txt", "page (1).txt", "page.txt", true,
                $"document.body.insertAdjacentHTML('beforeend', '<iframe src=\"{site.Address("/anchor/page.txt")}\"></iframe>')"),
            ("blob:", "blob.txt", "blob (1).txt", "blob", true,
                "const a = document.createElement('a'); a.download = 'blob.txt'; a.href = URL.createObjectURL(new Blob(['blob'])); "
                + "document.body.append(a); a.click()"),
            ("data:", "notes.txt", "notes (1).txt", "notes", true, "document.getElementById('download').click()"),
            ("Content-Disposition: attachment", "report.pdf", "report (1).pdf", "report", false, $"location.href = '{Report}'"),
        })
        {
            File.WriteAllText(Path.Combine(webs.Web.Downloads, name), "taken");
            var (from, asked) = (webs.Sent.Count, window.Questions.Count);
            events.Clear();

            // In a block of its own: a const a script declares at the top stays declared for the next script.
            await page.Script($"{{ {script} }} 0");

            // Every kind is tried, so one run measures each.
            try
            {
                var (file, end) = await Ended(webs, from, kind, events);
                Console.WriteLine($"CF7: {kind}: {string.Join(", ", events)}; asked: {window.Questions.Count > asked}");
                Assert.AreEqual(asks, window.Questions.Count > asked, $"{kind} asked");
                Assert.IsInstanceOfType<DownloadEnd.Finished>(end, kind);
                Assert.AreEqual(Path.Combine(webs.Web.Downloads, saved), file, kind);
                Assert.AreEqual(content, File.ReadAllText(file!), kind);
            }
            catch (AssertFailedException failed)
            {
                failures.Add(failed.Message);
            }
        }

        Assert.IsEmpty(failures, string.Join(Environment.NewLine, failures));
        Assert.AreEqual(webs.Web.Downloads, page.Profile.DefaultDownloadFolderPath);
    }

    // A frame's download, an artifact's among them, asks first, as does one whose frame WebView2 cannot tell.
    [UITestMethod]
    public async Task AFramesDownloadAndOneFromAnotherHostAskAndSaveNothingOnCancel()
    {
        using var site = new StandInSite();
        await using var webs = await Webs.Make();
        var window = new StandInWindow(webs, Main, answer: false);
        var page = await Open(webs, window);
        var events = Events(page);

        foreach (var (kind, script, question) in new[]
        {
            ("an artifact's frame", "document.querySelector('iframe').contentWindow.postMessage('download', '*')",
                "Download “artifact.txt”?"),
            ("a frame of another host",
                $"document.body.insertAdjacentHTML('beforeend', '<iframe src=\"{site.Address("/attachment/other.bin")}\"></iframe>')",
                "Download “other.bin” from 127.0.0.1?"),
        })
        {
            var (from, asked) = (webs.Sent.Count, window.Questions.Count);
            events.Clear();

            await page.Script($"{script}; 0");

            var (file, end) = await Ended(webs, from, kind, events);
            Console.WriteLine($"CF7: {kind}: {string.Join(", ", events)}");
            Assert.AreEqual(question, window.Questions.Skip(asked).Single().Title, kind);
            Assert.IsInstanceOfType<DownloadEnd.Cancelled>(end, kind);
            Assert.IsNull(file, kind);
        }

        await Views.Until(
            () => !Directory.Exists(webs.Web.Downloads) || Directory.GetFiles(webs.Web.Downloads).Length == 0,
            "a download refused left a file", Webs.Patience);
    }

    [UITestMethod]
    public async Task ASignInWindowSavesNothing()
    {
        await using var webs = await Webs.Make();
        var window = new StandInWindow(webs, Main, answer: true);
        var page = await Open(webs, window);
        var host = Main.Site.SignInHosts[0];
        await page.Script(
            $"window.open('{WebEnvironment.FixtureScheme}://{host}/sign-in', 'sign-in', 'width=480,height=600'); 0");
        await Views.Until(() => window.SignIn?.DocumentTitle == $"{host} sign-in stand-in", "no sign-in window loaded its page", Webs.Patience);
        var cancelled = new TaskCompletionSource<bool>();
        window.SignIn!.DownloadStarting += (_, args) => cancelled.TrySetResult(args.Cancel);

        await window.SignIn.Script(
            "const a = document.createElement('a'); a.download = 'x.txt'; a.href = 'data:text/plain,x'; document.body.append(a); a.click(); 0");

        Assert.IsTrue(await cancelled.Task.WaitAsync(Webs.Patience), "the sign-in window's download went on");
        Assert.IsEmpty(window.Questions);
        Assert.IsEmpty(webs.Sent.OfType<Intent.DownloadStarted>());
    }

    // CF7: the name WebView2 gives a second archive.tar.gz on its own, which a window gives it too.
    [UITestMethod]
    public async Task ASecondArchiveIsNamedAsWebView2NamesOne()
    {
        using var site = new StandInSite();
        await using var webs = await Webs.Make();
        var archive = site.Address("/attachment/archive.tar.gz");
        var plain = await webs.Page("plain");
        var own = Directory.CreateDirectory(Path.Combine(webs.Web.Downloads, "plain")).FullName;
        plain.Profile.DefaultDownloadFolderPath = own;
        File.WriteAllText(Path.Combine(own, "archive.tar.gz"), "taken");
        var offered = new TaskCompletionSource<(string Proposed, CoreWebView2DownloadOperation Operation)>();
        plain.DownloadStarting += (_, args) => offered.TrySetResult((args.ResultFilePath, args.DownloadOperation));
        plain.Navigate(archive);
        var (proposed, operation) = await offered.Task.WaitAsync(Webs.Patience);
        await Views.Until(() => operation.State == CoreWebView2DownloadState.Completed, "WebView2's own download did not finish", Webs.Patience);
        Console.WriteLine($"CF7: WebView2 offers {proposed} for a second archive.tar.gz and saves {operation.ResultFilePath}");
        Assert.AreEqual("archive (1).tar.gz", Path.GetFileName(operation.ResultFilePath));

        // Taken only once WebView2 has offered the name, so the window numbers it itself.
        var window = new StandInWindow(webs, Main, answer: true)
        {
            Asking = _ => File.WriteAllText(Path.Combine(webs.Web.Downloads, "archive.tar.gz"), "taken"),
        };
        var page = await Open(webs, window);
        var from = webs.Sent.Count;
        await page.Script($"document.body.insertAdjacentHTML('beforeend', '<iframe src=\"{archive}\"></iframe>'); 0");

        var (file, end) = await Ended(webs, from, "a window's archive.tar.gz");
        Assert.IsInstanceOfType<DownloadEnd.Finished>(end);
        Assert.AreEqual("archive.tar.gz", webs.Sent.Skip(from).OfType<Intent.DownloadStarted>().Single().Name, "the name WebView2 offered");
        Assert.AreEqual(Path.GetFileName(operation.ResultFilePath), Path.GetFileName(file));
    }

    // Quitting stops every download, so the question counts each one under way, a paused one too, and the model hears of
    // each end once. WebView2 pauses a download as interrupted, paused by the person, and able to resume.
    [UITestMethod]
    public async Task TheQuitQuestionAsksWhileADownloadIsUnderWay()
    {
        using var site = new StandInSite();
        await using var webs = await Webs.Make();
        var window = new StandInWindow(webs, Main, answer: true);
        var page = await Open(webs, window);
        var operations = new List<CoreWebView2DownloadOperation>();
        page.DownloadStarting += (_, args) => operations.Add(args.DownloadOperation);
        Assert.IsNull(webs.Downloads.QuitQuestion);

        await Stall(webs, page, site, "paused.bin");
        var paused = operations.Single();
        paused.Pause();
        await Views.Until(() => paused.State == CoreWebView2DownloadState.Interrupted, "the download did not pause", Webs.Patience);
        Console.WriteLine($"CF7: a paused download is {paused.State}, {paused.InterruptReason}, can resume: {paused.CanResume}");
        Assert.AreEqual("A download is in progress. Exit anyway?", webs.Downloads.QuitQuestion?.Title);
        CollectionAssert.AreEqual(new[] { webs.Own(Main).Store }, webs.Downloads.Stores.ToArray());

        paused.Cancel();
        await Views.Until(() => webs.Sent.OfType<Intent.DownloadEnded>().Any(), "the model did not hear the download stopped", Webs.Patience);
        Assert.IsInstanceOfType<DownloadEnd.Cancelled>(webs.Sent.OfType<Intent.DownloadEnded>().Single().End);
        Assert.IsNull(webs.Downloads.QuitQuestion);

        await Stall(webs, page, site, "stopped.bin");
        var stopped = new TaskCompletionSource();
        operations.Last().StateChanged += (_, _) => stopped.TrySetResult();
        webs.Downloads.StopAll();
        await stopped.Task.WaitAsync(Webs.Patience);

        Assert.IsNull(webs.Downloads.QuitQuestion);
        Assert.HasCount(2, webs.Sent.OfType<Intent.DownloadEnded>());
        // On windows-11-arm the file was still there as the download said it stopped (run 38822700418).
        var file = webs.Sent.OfType<Intent.DownloadSaving>().Last().File;
        await Views.Until(() => !File.Exists(file), "the stopped download left its file", Webs.Patience);
    }

    // A window closed while it asks answers no, and the closed page takes its download with it.
    [UITestMethod]
    public async Task ADownloadWhosePageClosedWhileAskedIsCancelled()
    {
        using var site = new StandInSite();
        await using var webs = await Webs.Make();
        var answer = new TaskCompletionSource<bool>();
        var window = new StandInWindow(webs, Main, answer: true) { Pending = answer.Task };
        var page = await Open(webs, window);
        await page.Script(
            $"document.body.insertAdjacentHTML('beforeend', '<iframe src=\"{site.Address("/attachment/late.bin")}\"></iframe>'); 0");
        await Views.Until(() => window.Questions.Count == 1, "the download was not asked about", Webs.Patience);

        webs.Close(page);
        answer.SetResult(false);

        await Views.Until(() => webs.Sent.OfType<Intent.DownloadEnded>().Any(), "the model did not hear the download ended", Webs.Patience);
        Assert.AreEqual(new DownloadEnd.Cancelled(), webs.Sent.OfType<Intent.DownloadEnded>().Single().End);
        Assert.IsEmpty(webs.Sent.OfType<Intent.DownloadSaving>());
    }

    // A download goes with the browser process of the windows' pages, which exits once the last of them closes.
    [UITestMethod]
    public async Task ADownloadWhoseLastPageClosedEnds()
    {
        using var site = new StandInSite();
        await using var webs = await Webs.Make();
        var window = new StandInWindow(webs, Main, answer: true);
        var page = await Open(webs, window);
        var changes = new List<string>();
        page.DownloadStarting += (_, args) =>
            args.DownloadOperation.StateChanged += (operation, _) => changes.Add($"{operation.State} {operation.InterruptReason}");
        await Stall(webs, page, site, "closed.bin");

        webs.Close(page);

        await Views.Until(() => webs.Sent.OfType<Intent.DownloadEnded>().Any(), "the model did not hear the download ended", Webs.Patience);
        Console.WriteLine(
            $"CF7: a download whose last page closed changed to {(changes.Count == 0 ? "nothing" : string.Join(", ", changes))}; "
            + $"its browser process had exited: {webs.Exited.IsCompleted}");
        Assert.IsInstanceOfType<DownloadEnd.Cancelled>(webs.Sent.OfType<Intent.DownloadEnded>().Single().End);
        Assert.IsNull(webs.Downloads.QuitQuestion);
    }

    // CF7: whether a download carries on once its page closes while another page keeps the browser process. The model
    // hears it ended only once WebView2 says so.
    [UITestMethod]
    public async Task ADownloadWhosePageClosedIsUnderWayWhileWebView2SaysSo()
    {
        using var site = new StandInSite();
        await using var webs = await Webs.Make();
        var window = new StandInWindow(webs, Main, answer: true);
        _ = await webs.Page("keeping");
        var page = await Open(webs, window);
        CoreWebView2DownloadOperation? operation = null;
        page.DownloadStarting += (_, args) => operation = args.DownloadOperation;
        await Stall(webs, page, site, "orphan.bin");

        webs.Close(page);
        await Task.Delay(TimeSpan.FromSeconds(5));

        var ended = webs.Sent.OfType<Intent.DownloadEnded>().Any();
        Console.WriteLine(
            $"CF7: 5 s after its page closed, with another page open, a download is {operation!.State} "
            + $"{operation.InterruptReason}, and the model heard it ended: {ended}");
        Assert.AreEqual(operation.State != CoreWebView2DownloadState.InProgress, ended);
        Assert.AreEqual(!ended, webs.Downloads.QuitQuestion is not null);
    }

    // A download from another host's frame that never finishes, once it is saving.
    private static async Task Stall(Webs webs, CoreWebView2 page, StandInSite site, string name)
    {
        var from = webs.Sent.Count;
        await page.Script(
            $"document.body.insertAdjacentHTML('beforeend', '<iframe src=\"{site.Address($"/stalled/{name}")}\"></iframe>'); 0");
        await Views.Until(() => webs.Sent.Skip(from).OfType<Intent.DownloadSaving>().Any(), $"{name} did not start saving", Webs.Patience);
    }

    // The account's window at the fixture's chatgpt.com, which gives one profile a second download without asking.
    private static async Task<CoreWebView2> Open(Webs webs, StandInWindow window)
    {
        var session = new WebSession(webs.Opened(Main, Home), webs.Web, webs.Downloads, window);
        var page = await webs.Page(Main.Store);
        Assert.IsTrue(await Webs.Attached(session, page), "the fixture's page did not load");
        page.PermissionRequested += (_, args) =>
        {
            if (args.PermissionKind == CoreWebView2PermissionKind.MultipleAutomaticDownloads)
            {
                args.State = CoreWebView2PermissionState.Allow;
            }
        };
        return page;
    }

    // After the session's own handlers.
    private static List<string> Events(CoreWebView2 page)
    {
        var events = new List<string>();
        page.NavigationStarting += (_, args) => events.Add($"NavigationStarting {args.Uri}");
        page.FrameNavigationStarting += (_, args) => events.Add($"FrameNavigationStarting {args.Uri}");
        page.NavigationCompleted += (_, args) => events.Add($"NavigationCompleted {args.IsSuccess} {args.WebErrorStatus}");
        page.FrameNavigationCompleted += (_, args) => events.Add($"FrameNavigationCompleted {args.IsSuccess} {args.WebErrorStatus}");
        page.DownloadStarting += (_, args) => events.Add($"DownloadStarting {args.DownloadOperation.Uri}");
        page.LaunchingExternalUriScheme += (_, args) => events.Add($"LaunchingExternalUriScheme {args.Uri}");
        return events;
    }

    // The file the first download told the model of after `from` saved to, if any, and how it ended.
    private static async Task<(string? File, DownloadEnd End)> Ended(
        Webs webs, int from, string what, List<string>? events = null)
    {
        try
        {
            await Views.Until(() => webs.Sent.Skip(from).OfType<Intent.DownloadEnded>().Any(), $"{what} did not end", Webs.Patience);
        }
        catch (AssertFailedException late)
        {
            throw new AssertFailedException(
                $"{late.Message}, after {string.Join(", ", events ?? [])}; the model was told {string.Join(", ", webs.Sent.Skip(from))}",
                late);
        }

        var ended = webs.Sent.Skip(from).OfType<Intent.DownloadEnded>().First();
        var saving = webs.Sent.Skip(from).OfType<Intent.DownloadSaving>().SingleOrDefault(saving => saving.Id == ended.Id);
        return (saving?.File, ended.End);
    }
}
