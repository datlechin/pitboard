using System.Runtime.InteropServices;
using Microsoft.Web.WebView2.Core;
using Pitboard.Core;

namespace Pitboard.Windows;

// What the window around a session does for it.
public interface ISessionWindow
{
    // A sign-in window at the size given, whose page is made from the same environment and profile and has loaded
    // nothing yet; null where none could be made.
    Task<CoreWebView2?> OpenSignIn(SignInWindowSize size);

    void CloseSignIn();

    // Whether the person lets a download the policy asks about save, asked over the account's window.
    Task<bool> AllowDownload(AlertText question);

    // The window's page is now at this address.
    void Shown(string address);

    // Its note or its failure changed.
    void Changed();
}

// One account's window as WebView2 runs it: its own page, and the sign-in window one of its pages opened, each under
// the policy of the account's site in its role.
public sealed class WebSession
{
    private readonly NavigationPolicy _policy;
    private readonly WebEnvironment _web;
    private readonly Downloads _downloads;
    private readonly ISessionWindow _window;
    private readonly TimeProvider _time;
    private readonly Dictionary<PageRole, DateTimeOffset> _lastEnded = [];
    private CoreWebView2? _page;
    private ulong _loaded;
    private string _first;
    private string? _shown;

    public WebSession(OpenWindow open, WebEnvironment web, Downloads downloads, ISessionWindow window, TimeProvider? time = null)
    {
        Account = open.Account;
        _policy = web.Policy(open.Account.Site);
        Note = open.Note;
        _web = web;
        _downloads = downloads;
        _window = window;
        _time = time ?? TimeProvider.System;
        _loaded = open.Load.Serial;
        _first = open.Load.Url;
    }

    public WindowAccount Account { get; private set; }

    public WindowNoteKind? Note { get; private set; }

    // Why the page is not shown, after its content stopped twice within a minute or its browser process ended.
    public string? Failure { get; private set; }

    // WebView2 closed the page with its browser process, and only a new page shows the window again.
    public bool PageClosed { get; private set; }

    // The window's page, which loads the page the session starts at.
    public void Attach(CoreWebView2 page)
    {
        _page = page;
        Prepare(page, PageRole.Window);
        _web.Serve(page);
        page.SourceChanged += (sender, _) =>
        {
            _shown = sender.Source;
            _window.Shown(sender.Source);
        };
        page.Navigate(_first);
    }

    // A page of its own in place of one its browser process took, at the page the window was on.
    public void Recover(CoreWebView2 page)
    {
        Failure = null;
        PageClosed = false;
        _first = _shown ?? _first;
        _window.Changed();
        Attach(page);
    }

    // A rename retitles the window and keeps everything else.
    public void Update(WindowAccount account) => Account = account;

    // A link chosen for this window while it is open; Back returns to where the window was.
    public void Load(PageLoad load)
    {
        if (load.Serial == _loaded)
        {
            return;
        }

        _loaded = load.Serial;
        if (_page is null)
        {
            _first = load.Url;
            return;
        }

        // A window back in sight is offered the page it still shows.
        if (Failure is null && load.Url == _shown)
        {
            return;
        }

        if (Failure is not null)
        {
            Failure = null;
            _window.Changed();
        }

        _page.Navigate(load.Url);
    }

    public void Reload()
    {
        Failure = null;
        _window.Changed();
        _page?.Reload();
    }

    public void Dismiss()
    {
        Note = null;
        _window.Changed();
    }

    // The signed-in page leaves first, so nothing it still does lands in the profile after it is cleared.
    public async Task RemoveWebsiteData()
    {
        _window.CloseSignIn();
        if (_page is not { } page)
        {
            return;
        }

        await Leave(page);
        await page.Profile.ClearBrowsingDataAsync();
    }

    // A page of its own after the data went, so Back cannot bring the signed-in one back.
    public void StartOver(CoreWebView2 page)
    {
        Note = PitboardFfiMethods.OpeningNote(storeMadeBefore: false);
        Failure = null;
        PageClosed = false;
        _first = PitboardFfiMethods.WindowHome(_policy);
        _window.Changed();
        Attach(page);
    }

    private void Prepare(CoreWebView2 page, PageRole role)
    {
        _web.Prepare(page);
        // WebView2 still sent the request of a navigation the policy stopped (run 38842851059), so each one's document
        // request is answered here, empty, before it leaves.
        var stopped = new HashSet<string>(StringComparer.Ordinal);
        page.NavigationStarting += (sender, args) => Navigating(role, sender, args, frame: false, stopped);
        page.FrameNavigationStarting += (sender, args) => Navigating(role, sender, args, frame: true, stopped);
        page.AddWebResourceRequestedFilter(
            "*", CoreWebView2WebResourceContext.Document, CoreWebView2WebResourceRequestSourceKinds.Document);
        page.WebResourceRequested += (sender, args) =>
        {
            if (stopped.Remove(args.Request.Uri))
            {
                args.Response = sender.Environment.CreateWebResourceResponse(null, 204, "No Content", "");
            }
        };
        page.NewWindowRequested += (sender, args) => NewWindow(role, sender, args);
        // A sign-in window's page closes it once it is done; an account window's page cannot close it.
        page.WindowCloseRequested += (_, _) =>
        {
            if (PitboardFfiMethods.PageMayClose(role))
            {
                _window.CloseSignIn();
            }
        };
        page.PermissionRequested += (_, args) => Permission(args);
        page.ProcessFailed += (sender, args) => ProcessFailed(role, sender, args);
        // After the policy's own handlers, so a navigation it stopped is not under way.
        var navigations = new PageNavigations();
        page.NavigationStarting += (_, args) => navigations.Starting(args.NavigationId, args.Uri, frame: false, args.Cancel);
        page.FrameNavigationStarting += (_, args) => navigations.Starting(args.NavigationId, args.Uri, frame: true, args.Cancel);
        page.NavigationCompleted += (_, args) =>
            navigations.Completed(args.NavigationId, frame: false, args.IsSuccess, args.WebErrorStatus);
        page.FrameNavigationCompleted += (_, args) =>
            navigations.Completed(args.NavigationId, frame: true, args.IsSuccess, args.WebErrorStatus);
        page.DownloadStarting += (_, args) => Download(role, navigations, args);
        // What a frame loads is the page's to decide, and an address it loads that WebView2 hands to another app never
        // leaves through Pitboard.
        page.LaunchingExternalUriScheme += (_, args) => args.Cancel = true;
    }

    private void Navigating(
        PageRole role, CoreWebView2 page, CoreWebView2NavigationStartingEventArgs args, bool frame, HashSet<string> stopped)
    {
        var request = PageRequests.Navigation(_policy, role, args.Uri, frame, args.IsUserInitiated, frame ? null : page.Source);
        var decision = PitboardFfiMethods.DecideNavigation(_policy, role, request);
        if (decision is NavigationDecision.Load or NavigationDecision.LoadInPage or NavigationDecision.Popup)
        {
            stopped.Remove(args.Uri);
            return;
        }

        args.Cancel = true;
        stopped.Add(args.Uri);
        switch (decision)
        {
            case NavigationDecision.OpenElsewhere away:
                HandOver(away.Url, away.Note);
                return;
            case NavigationDecision.Refuse refused:
                Say(refused.Note);
                return;
        }
    }

    // Always handled, so WebView2 opens no window of its own. A sign-in window keeps window.opener: its page is set as
    // the new window before it loads.
    private async void NewWindow(PageRole role, CoreWebView2 page, CoreWebView2NewWindowRequestedEventArgs args)
    {
        var source = args.OriginalSourceFrameInfo;
        var request = PageRequests.NewWindow(
            _policy, role, args.Uri, args.IsUserInitiated, source?.Source,
            mainFrame: source?.FrameKind == CoreWebView2FrameKind.MainFrame);
        args.Handled = true;
        switch (PitboardFfiMethods.DecideNavigation(_policy, role, request))
        {
            case NavigationDecision.Popup:
                var deferral = args.GetDeferral();
                try
                {
                    var features = args.WindowFeatures;
                    var size = PitboardFfiMethods.SignInWindowSize(
                        features.HasSize ? features.Width : (double?)null, features.HasSize ? features.Height : (double?)null);
                    // WebView2 takes a new window's settings before it is set, and its resource requests after.
                    if (await _window.OpenSignIn(size) is { } signIn)
                    {
                        Prepare(signIn, PageRole.Popup);
                        args.NewWindow = signIn;
                        _web.Serve(signIn);
                    }

                    deferral.Complete();
                }
                // The sign-in window or its opener closed meanwhile: the page's window.open gets no window.
                catch (Exception closed) when (closed is COMException or InvalidOperationException)
                {
                    try
                    {
                        deferral.Complete();
                    }
                    catch (Exception gone) when (gone is COMException or InvalidOperationException)
                    {
                    }
                }

                break;
            case NavigationDecision.LoadInPage:
                page.Navigate(args.Uri);
                break;
            case NavigationDecision.OpenElsewhere away:
                HandOver(away.Url, away.Note);
                break;
            case NavigationDecision.Refuse refused:
                Say(refused.Note);
                break;
        }
    }

    // Left unhandled, so WebView2's own download dialog shows it, as Edge's does.
    private async void Download(PageRole role, PageNavigations navigations, CoreWebView2DownloadStartingEventArgs args)
    {
        var operation = args.DownloadOperation;
        var facts = PageRequests.Download(operation.Uri, operation.ContentDisposition, navigations.MainFrame(operation.Uri));
        if (PitboardFfiMethods.DecideResponse(_policy, role, facts) is ResponseDecision.Download saving)
        {
            await _downloads.Start(Account.Store, args, saving.Ask, _window.AllowDownload);
        }
        else
        {
            args.Cancel = true;
        }
    }

    // A camera or a microphone is never given; WebView2 asks the person about anything else.
    private static void Permission(CoreWebView2PermissionRequestedEventArgs args)
    {
        PagePermission? asked = args.PermissionKind switch
        {
            CoreWebView2PermissionKind.Camera => PagePermission.Camera,
            CoreWebView2PermissionKind.Microphone => PagePermission.Microphone,
            _ => null,
        };
        if (asked is { } permission && !PitboardFfiMethods.PageMayUse(permission))
        {
            args.State = CoreWebView2PermissionState.Deny;
            args.Handled = true;
        }
    }

    // An unresponsive render process is reported every few seconds until it answers, and is left to answer.
    private void ProcessFailed(PageRole role, CoreWebView2 page, CoreWebView2ProcessFailedEventArgs args)
    {
        var now = _time.GetUtcNow();
        switch (args.ProcessFailedKind)
        {
            case CoreWebView2ProcessFailedKind.RenderProcessExited:
                double? last = _lastEnded.TryGetValue(role, out var ended) ? Seconds(ended) : null;
                _lastEnded[role] = now;
                switch (PitboardFfiMethods.AfterContentProcessEnded(last, Seconds(now)))
                {
                    case ProcessEnded.Reload:
                        page.Reload();
                        break;
                    case ProcessEnded.Stopped stopped when role == PageRole.Window:
                        Failure = stopped.Reason;
                        _window.Changed();
                        break;
                }

                break;
            case CoreWebView2ProcessFailedKind.BrowserProcessExited when role == PageRole.Window:
                // Said as a second end within the minute: no reload brings back a page WebView2 has closed.
                if (PitboardFfiMethods.AfterContentProcessEnded(Seconds(now), Seconds(now)) is ProcessEnded.Stopped closed)
                {
                    Failure = closed.Reason;
                }

                PageClosed = true;
                _window.Changed();
                break;
            case CoreWebView2ProcessFailedKind.BrowserProcessExited:
                _window.CloseSignIn();
                break;
        }
    }

    private void HandOver(string address, WindowNoteKind? note)
    {
        _web.OpenElsewhere(address);
        if (note is not null)
        {
            Say(note);
        }
    }

    private void Say(WindowNoteKind note)
    {
        Note = note;
        _window.Changed();
    }

    // Up to two seconds, until the page has left for a blank one and run its unload handlers.
    private static async Task Leave(CoreWebView2 page)
    {
        var left = new TaskCompletionSource();
        void Done(CoreWebView2 sender, CoreWebView2NavigationCompletedEventArgs args) => left.TrySetResult();
        page.NavigationCompleted += Done;
        try
        {
            page.Stop();
            page.Navigate("about:blank");
            await Task.WhenAny(left.Task, Task.Delay(TimeSpan.FromSeconds(2)));
        }
        finally
        {
            page.NavigationCompleted -= Done;
        }
    }

    private static double Seconds(DateTimeOffset at) => at.ToUnixTimeMilliseconds() / 1000.0;
}
