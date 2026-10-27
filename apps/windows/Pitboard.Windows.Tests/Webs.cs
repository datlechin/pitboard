using System.Runtime.InteropServices;
using Microsoft.Web.WebView2.Core;
using Pitboard.Core;
using Windows.Foundation;
using WinRT.Interop;

namespace Pitboard.Windows.Tests;

// WebView2 in the test app's window, as the app makes it: one browser process over a user data folder, and a page per
// profile name. Every test's profiles are named apart in one user data folder for the run: WebView2 tries the test
// user's password blank once for each new folder, and in run 38822700418 a folder per test locked the user out. Pages a
// test sends elsewhere are kept, and opened nowhere; downloads are saved in the test's own folder, and what they tell
// the model is kept.
internal sealed class Webs : IAsyncDisposable
{
    public static readonly TimeSpan Patience = TimeSpan.FromSeconds(60);

    private static readonly Lazy<DirectoryInfo> Run = new(() => Directory.CreateTempSubdirectory("pitboard-webview2-"));

    private readonly DirectoryInfo _folder;
    private readonly string _tag = Guid.NewGuid().ToString("N")[..8];
    // Each page with its controller, whose CoreWebView2 is a new object each time it is read.
    private readonly List<(CoreWebView2Controller Controller, CoreWebView2 Page)> _pages = [];
    private TaskCompletionSource _exited = new(TaskCreationOptions.RunContinuationsAsynchronously);

    private Webs(DirectoryInfo folder, WebEnvironment web, List<string> elsewhere)
    {
        _folder = folder;
        Web = web;
        Elsewhere = elsewhere;
        Downloads = new Downloads(web, Sent.Add);
        web.BrowserExited += () => _exited.TrySetResult();
    }

    public WebEnvironment Web { get; }

    public List<string> Elsewhere { get; }

    public Downloads Downloads { get; }

    public List<Intent> Sent { get; } = [];

    // Set once every page of the browser process has closed and it has let go of the folder. A page made after that
    // starts another.
    public Task Exited => _exited.Task;

    public static nint Window => WindowNative.GetWindowHandle(UnitTestApp.Window);

    public static async Task<Webs> Make()
    {
        var folder = Directory.CreateTempSubdirectory("pitboard-downloads-");
        var elsewhere = new List<string>();
        var web = new WebEnvironment(
            WebEnvironment.FixtureScheme, Path.Combine(Run.Value.FullName, "WebView2"), Path.Combine(folder.FullName, "Downloads"),
            devTools: false, elsewhere.Add);
        var apartment = Thread.CurrentThread.GetApartmentState();
        try
        {
            await web.Made().WaitAsync(Patience);
            return new Webs(folder, web, elsewhere);
        }
        catch (COMException failed)
        {
            throw new AssertFailedException($"WebView2 made no environment, asked from a thread in the {apartment}: {failed.Message}", failed);
        }
    }

    // The run's user data folder, once the last browser process over it has exited.
    public static void RemoveRun()
    {
        if (!Run.IsValueCreated)
        {
            return;
        }

        try
        {
            Run.Value.Delete(recursive: true);
        }
        catch (Exception held) when (held is IOException or UnauthorizedAccessException)
        {
        }
    }

    // A profile's or a store's name of this test's own.
    public string Name(string profile) => $"{_tag}-{profile}";

    public WindowAccount Own(WindowAccount account) => account with { Store = Name(account.Store) };

    // The account's window, its store this test's own, at the page given.
    public OpenWindow Opened(WindowAccount account, string address)
    {
        var owned = Own(account);
        return new(
            Store: owned.Store, Account: owned, Load: new PageLoad(1, address), Note: null,
            ClearDownloads: new Choice("Clear", new Intent.ClearDownloads(owned.Store), Enabled: false));
    }

    // A page in this test's profile of that name, which has loaded nothing and answers to nothing of the app's yet.
    public async Task<CoreWebView2> Page(string profile)
    {
        if (_exited.Task.IsCompleted)
        {
            _exited = new(TaskCreationOptions.RunContinuationsAsynchronously);
        }

        var environment = await Web.Made().WaitAsync(Patience);
        var controller = await environment.CreateCoreWebView2ControllerAsync(
            CoreWebView2ControllerWindowReference.CreateFromWindowHandle((ulong)Window),
            WebEnvironment.Options(environment, Name(profile))).AsTask().WaitAsync(Patience);
        controller.Bounds = new Rect(0, 0, 800, 600);
        var page = controller.CoreWebView2;
        _pages.Add((controller, page));
        return page;
    }

    // As its window closing closes it.
    public void Close(CoreWebView2 page) => _pages.Single(made => made.Page == page).Controller.Close();

    // Whether a page loads after `start`. A navigation stopped before it, which may end later, does not count.
    public static async Task<bool> Loads(CoreWebView2 page, Func<Task> start)
    {
        var done = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        void Completed(CoreWebView2 sender, CoreWebView2NavigationCompletedEventArgs args)
        {
            if (args.IsSuccess)
            {
                done.TrySetResult();
            }
        }

        page.NavigationCompleted += Completed;
        try
        {
            await start();
            return await Task.WhenAny(done.Task, Task.Delay(Patience)) == done.Task;
        }
        finally
        {
            page.NavigationCompleted -= Completed;
        }
    }

    public static Task<bool> Loads(CoreWebView2 page, string address) =>
        Loads(page, () =>
        {
            page.Navigate(address);
            return Task.CompletedTask;
        });

    // The session's window page, which loads the page the session starts at.
    public static Task<bool> Attached(WebSession session, CoreWebView2 page) =>
        Loads(page, () =>
        {
            session.Attach(page);
            return Task.CompletedTask;
        });

    public async ValueTask DisposeAsync()
    {
        foreach (var (controller, _) in _pages)
        {
            try
            {
                controller.Close();
            }
            catch (Exception closed) when (closed is COMException or InvalidOperationException)
            {
            }
        }

        // A test that needs the browser process gone waits for it itself.
        if (_pages.Count > 0)
        {
            await Task.WhenAny(Exited, Task.Delay(TimeSpan.FromSeconds(10)));
        }

        try
        {
            _folder.Delete(recursive: true);
        }
        catch (Exception held) when (held is IOException or UnauthorizedAccessException)
        {
        }
    }
}

internal static class Pages
{
    // A page's script, given a minute: a page held up, as by a window.open WebView2 waits on, fails its test alone.
    public static Task<string> Script(this CoreWebView2 page, string script) =>
        page.ExecuteScriptAsync(script).AsTask().WaitAsync(Webs.Patience);
}

// Deletes the run's user data folder after the last test.
[TestClass]
public static class WebsRun
{
    [AssemblyCleanup]
    public static void Remove() => Webs.RemoveRun();
}

// The window around a session, with no window: it keeps what the session asked of it, makes a sign-in window's page in
// the account's profile, and answers a download's question as it was told.
internal sealed class StandInWindow(Webs webs, WindowAccount account, bool answer = false) : ISessionWindow
{
    public List<SignInWindowSize> Sizes { get; } = [];

    public CoreWebView2? SignIn { get; private set; }

    public int Closed { get; private set; }

    public List<string> Pages { get; } = [];

    public List<Question> Questions { get; } = [];

    // An answer still to come, in place of the one given.
    public Task<bool>? Pending { get; init; }

    // Done as each question is asked, before it is answered.
    public Action<Question>? Asking { get; init; }

    // Whether a sign-in window fails as one closed while its page was made does.
    public bool SignInFails { get; init; }

    public async Task<CoreWebView2?> OpenSignIn(SignInWindowSize size)
    {
        Sizes.Add(size);
        if (SignInFails)
        {
            throw new ObjectDisposedException("SignInWindow", "The sign-in window closed while its page was made.");
        }

        SignIn = await webs.Page(account.Store);
        return SignIn;
    }

    public void CloseSignIn() => Closed++;

    public Task<bool> AllowDownload(Question question)
    {
        Questions.Add(question);
        Asking?.Invoke(question);
        return Pending ?? Task.FromResult(answer);
    }

    public void Shown(string address) => Pages.Add(address);

    public void Changed()
    {
    }
}
