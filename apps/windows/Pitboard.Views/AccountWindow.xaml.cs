using System.Runtime.InteropServices;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.Web.WebView2.Core;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.Graphics;
using Windows.System;
using Windows.UI.Core;
using WinRT.Interop;

namespace Pitboard.Views;

public sealed partial class AccountWindow : Window, ISessionWindow
{
    private readonly WebEnvironment _web;
    private readonly Downloads _downloads;
    private readonly Action<Intent> _send;
    // Withdrawn as the window goes out of sight, which answers its questions no, and made again as it comes back.
    private DialogHost _dialogs;
    private WebView2? _view;
    private WebSession? _session;
    private SignInWindow? _signIn;
    private Action? _standInChoice;
    private WindowNoteKind? _noteShown;
    private bool _starting;
    private bool _closed;

    public AccountWindow(string store, WebEnvironment web, Downloads downloads, Action<Intent> send)
    {
        Store = store;
        _web = web;
        _downloads = downloads;
        _send = send;
        InitializeComponent();
        WindowChrome.Apply(this);
        _dialogs = new DialogHost(() => Root.XamlRoot);
        var scale = Shell.Scale(WindowNative.GetWindowHandle(this));
        AppWindow.Resize(new SizeInt32((int)(1100 * scale), (int)(800 * scale)));
        WindowChrome.MinimumSize(this, 480, 360);

        // The caption's close and Alt+F4. Close() raises no Closing.
        AppWindow.Closing += (_, args) => args.Cancel = GoAway();
        _downloads.Ended += OnDownloadEnded;
        Closed += (_, _) =>
        {
            _closed = true;
            _downloads.Ended -= OnDownloadEnded;
            _dialogs.Withdraw();
            _signIn?.Close();
            _view?.Close();
        };
        Show(null);
    }

    public event Action? WentAway;

    // Its account's store, which is also its identity: one window per account.
    public string Store { get; }

    // Closed by the person while a download of it was under way, and kept out of sight with its page until none is.
    public bool Away { get; private set; }

    internal SignInWindow? SignIn => _signIn;

    // The last Remove website data, until it has done all it does.
    internal Task? Removing { get; private set; }

    // The window's page, until WebView2 closes it with its browser process.
    private CoreWebView2? Page => _session is { PageClosed: true } ? null : _view?.CoreWebView2;

    // What the model says of the windows: this one's page once it can show it, what it shows until then, and whether it
    // has downloads to list.
    public void Follow(AccountWindowsShown shown)
    {
        var downloaded = shown.Downloads.Any(
            download => string.Equals(download.Store, Store, StringComparison.OrdinalIgnoreCase));
        DownloadsButton.Visibility = Look.Shown(downloaded);
        var open = shown.Open.FirstOrDefault(window => string.Equals(window.Store, Store, StringComparison.OrdinalIgnoreCase));
        if (open is not null)
        {
            Show(open);
        }
        else if (_session is null && !_starting)
        {
            Wait(shown.Waiting);
        }
    }

    // A window closed or out of sight meanwhile answers no.
    public Task<bool> Confirm(Question question) => _dialogs.Confirm(question);

    public void Return()
    {
        Away = false;
        _dialogs = new DialogHost(() => Root.XamlRoot);
        if (Page is { } page)
        {
            page.IsMuted = false;
        }

        AppWindow.Show();
    }

    public async Task<CoreWebView2?> OpenSignIn(SignInWindowSize size)
    {
        CloseSignIn();
        if (_session is null)
        {
            return null;
        }

        var window = new SignInWindow(size, this, _session.Account.Title);
        window.Closed += (_, _) =>
        {
            if (ReferenceEquals(_signIn, window))
            {
                _signIn = null;
            }
        };
        _signIn = window;
        window.Activate();
        var environment = await _web.Made();
        return await window.Page(environment, WebEnvironment.Options(environment, Store));
    }

    public void CloseSignIn() => _signIn?.Close();

    // As the macOS app asks, over the window.
    public Task<bool> AllowDownload(AlertText question) =>
        Confirm(new Question(question.Title, question.Message, "Download"));

    public void Shown(string address) => _send(new Intent.PageShown(Store, address));

    public void Changed() => ShowSession();

    private void Show(OpenWindow? open)
    {
        if (open is null)
        {
            Title = "Account";
            RemoveDataButton.IsEnabled = false;
            BackButton.IsEnabled = ForwardButton.IsEnabled = ReloadButton.IsEnabled = false;
            return;
        }

        ShowTitle(open.Account);
        if (_session is { } session)
        {
            session.Update(open.Account);
            session.Load(open.Load);
            ShowSession();
            return;
        }

        if (!_starting)
        {
            _starting = true;
            _ = Start(open);
        }
    }

    private async Task Start(OpenWindow open)
    {
        if (WebEnvironment.Refusal() is { } refusal)
        {
            Unopened(refusal);
            return;
        }

        var session = new WebSession(open, _web, _downloads, this);
        _session = session;
        try
        {
            var page = await _web.Reusing(
                WindowNative.GetWindowHandle(this), () => _closed ? throw new ObjectDisposedException(nameof(AccountWindow)) : NewPage());
            session.Attach(page);
            Watch(page);
            ShowSession();
        }
        catch (Exception failed)
        {
            _session = null;
            if (!_closed)
            {
                Unopened(failed.Message);
            }
        }
    }

    // A page of its own each time, so after its data was removed Back cannot bring the signed-in page back.
    private async Task<CoreWebView2> NewPage()
    {
        _view?.Close();
        if (_view is not null)
        {
            PageArea.Children.Remove(_view);
        }

        var view = new WebView2();
        _view = view;
        PageArea.Children.Insert(0, view);
        var environment = await _web.Made();
        var page = await WebViews.Made(view, environment, WebEnvironment.Options(environment, Store));
        page.DefaultDownloadDialogCornerAlignment = CoreWebView2DefaultDownloadDialogCornerAlignment.TopLeft;
        return page;
    }

    // After the session's own handlers, so a navigation it stopped shows no loading.
    private void Watch(CoreWebView2 page)
    {
        page.HistoryChanged += (_, _) => ShowHistory();
        page.NavigationStarting += (_, args) => ShowLoading(!args.Cancel);
        page.NavigationCompleted += (_, _) => ShowLoading(false);
        page.DocumentTitleChanged += (_, _) =>
        {
            if (_session is { } session)
            {
                ShowTitle(session.Account);
            }
        };
    }

    // Microsoft's title bar text guidance names the document before the program.
    private void ShowTitle(WindowAccount account) =>
        Title = Page?.DocumentTitle is { Length: > 0 } page ? $"{page} - {account.Title}" : account.Title;

    private void Wait(WindowWaiting waiting)
    {
        Title = waiting.WindowTitle;
        switch (waiting.Shown)
        {
            case WaitingShown.Reading reading:
                ShowStandIn(mark: "", reading.Title, detail: "", choice: null, enabled: false, chosen: null, busy: true);
                break;
            case WaitingShown.ReadFailed failed:
                ShowStandIn(Glyphs.Warning, failed.Title, failed.Detail, failed.Retry.Title, failed.Retry.Enabled, () => _send(failed.Retry.Intent));
                break;
        }
    }

    private void ShowStandIn(string mark, string title, string detail, string? choice, bool enabled, Action? chosen, bool busy = false)
    {
        StandIn.Glyph = mark;
        StandIn.Busy = busy;
        StandIn.Title = title;
        StandIn.Detail = detail;
        StandIn.Choice = chosen is null ? "" : choice ?? "";
        StandIn.IsChoiceEnabled = enabled;
        _standInChoice = chosen;
        StandIn.Visibility = Visibility.Visible;
        if (_view is not null)
        {
            _view.Visibility = Visibility.Collapsed;
        }
    }

    private void ShowSession()
    {
        if (_session is not { } session)
        {
            return;
        }

        // Narrator hears an InfoBar as it opens, so a note in place of another opens it again.
        var note = session.Note;
        if (note is not null && Note.IsOpen && !Equals(note, _noteShown))
        {
            Note.IsOpen = false;
        }

        _noteShown = note;
        Note.Message = note is not null ? PitboardFfiMethods.WindowNote(note, session.Account) : "";
        Note.IsOpen = note is not null;
        RemoveDataButton.IsEnabled = !session.PageClosed;
        ReloadButton.IsEnabled = true;
        ShowHistory();
        if (session.Failure is { } failure)
        {
            ShowStandIn(Glyphs.Warning, "Couldn’t load the page", failure, "Try again", enabled: true, TryAgain);
            return;
        }

        StandIn.Visibility = Visibility.Collapsed;
        if (_view is not null)
        {
            _view.Visibility = Visibility.Visible;
        }
    }

    // A page WebView2 closed with its browser process comes back only in a web view of its own.
    private async void TryAgain()
    {
        if (_session is not { } session)
        {
            return;
        }

        if (!session.PageClosed)
        {
            session.Reload();
            return;
        }

        try
        {
            var page = await NewPage();
            session.Recover(page);
            Watch(page);
            ShowSession();
        }
        catch (Exception failed)
        {
            if (!_closed)
            {
                Unopened(failed.Message);
            }
        }
    }

    private void Unopened(string why) =>
        ShowStandIn(Glyphs.Warning, "Couldn’t open this window", why, choice: null, enabled: false, chosen: null);

    private void ShowHistory()
    {
        BackButton.IsEnabled = Page?.CanGoBack == true;
        ForwardButton.IsEnabled = Page?.CanGoForward == true;
    }

    private void ShowLoading(bool loading)
    {
        ReloadButton.Visibility = Look.Shown(!loading);
        StopButton.Visibility = Look.Shown(loading);
    }

    private void OnBack(object sender, RoutedEventArgs args) => Page?.GoBack();

    private void OnForward(object sender, RoutedEventArgs args) => Page?.GoForward();

    private void OnReload(object sender, RoutedEventArgs args) => TryAgain();

    private void OnStop(object sender, RoutedEventArgs args) => Page?.Stop();

    private void OnClose(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        if (!GoAway())
        {
            Close();
        }
    }

    // WebView2 ends a download with its browser process, which exits with its last page.
    private bool GoAway()
    {
        if (_closed || !_downloads.UnderWay(Store))
        {
            return false;
        }

        if (Away)
        {
            return true;
        }

        Away = true;
        _dialogs.Withdraw();
        _signIn?.Close();
        if (Page is { } page)
        {
            page.IsMuted = true;
        }

        AppWindow.Hide();
        WentAway?.Invoke();
        return true;
    }

    // After the event, which may be WebView2's own.
    private void OnDownloadEnded() =>
        DispatcherQueue.TryEnqueue(Ui.Guarded(() =>
        {
            if (Away && !_closed && !_downloads.UnderWay(Store))
            {
                Close();
            }
        }));

    // An accelerator on Root missed Alt+Left from the command bar's More on windows-2025 (docs/facts/windows-app.md).
    private void OnPreviewKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (!Held(VirtualKey.Menu) || Held(VirtualKey.Control) || Held(VirtualKey.Shift))
        {
            return;
        }

        if (args.Key == VirtualKey.Left)
        {
            args.Handled = Commanded(BackButton, () => Page?.GoBack());
        }
        else if (args.Key == VirtualKey.Right)
        {
            args.Handled = Commanded(ForwardButton, () => Page?.GoForward());
        }
    }

    private static bool Held(VirtualKey key) =>
        InputKeyboardSource.GetKeyStateForCurrentThread(key).HasFlag(CoreVirtualKeyStates.Down);

    private void OnReloadKey(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args) =>
        args.Handled = Commanded(ReloadButton, TryAgain);

    private static bool Commanded(Control button, Action does)
    {
        if (!button.IsEnabled)
        {
            return false;
        }

        does();
        return true;
    }

    private void OnShowDownloads(object sender, RoutedEventArgs args) => Page?.OpenDefaultDownloadDialog();

    private void OnRemoveData(object sender, RoutedEventArgs args) => Removing = RemoveWebsiteData();

    private void OnNoteClosed(InfoBar sender, object args) => _session?.Dismiss();

    private void OnStandInChosen(object sender, RoutedEventArgs args) => _standInChoice?.Invoke();

    // The site is not told: the account stays signed in everywhere else.
    private async Task RemoveWebsiteData()
    {
        if (_session is not { } session)
        {
            return;
        }

        var alert = PitboardFfiMethods.RemoveDataAlert(session.Account);
        if (!await Confirm(new Question(alert.Title, alert.Message, "Remove")))
        {
            return;
        }

        try
        {
            await session.RemoveWebsiteData();
        }
        // Its page closed meanwhile, with the window or its browser process, and its data stays.
        catch (Exception closed) when (closed is COMException or InvalidOperationException)
        {
            return;
        }

        _send(new Intent.WebsiteDataRemoved(Store));
        if (_closed)
        {
            return;
        }

        try
        {
            var page = await NewPage();
            session.StartOver(page);
            Watch(page);
            ShowSession();
        }
        catch (Exception failed)
        {
            if (!_closed)
            {
                Unopened(failed.Message);
            }
        }
    }
}
