using System.ComponentModel;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.System.Power;
using Pitboard.Core;
using Pitboard.Views;
using Pitboard.Windows;
using Windows.Graphics;
using Launcher = Windows.System.Launcher;

namespace Pitboard.App;

public partial class App : Application, IIconTargets
{
    private readonly AppStart _start;
    private readonly Handovers _handovers;
    private readonly WindowRequests _requests = new();
    private readonly Uninstaller _uninstaller;
    private readonly string _version = Versions.Of(typeof(App).Assembly);
    private IUpdates _updates = new NoUpdates();
    private AppModel? _model;
    private StartAtSignIn? _startAtSignIn;
    private HiddenWindow? _hidden;
    private IconImage? _image;
    private NotifyIcon? _icon;
    private IconCommands? _commands;
    private Lifetime? _lifetime;
    private MenuHost? _menu;
    private MainWindow? _window;
    private WelcomeWindow? _welcome;
    private AccountWindows? _accountWindows;
    private WindowsNotifications? _notifications;

    public App(AppStart start, Handovers handovers)
    {
        _start = start;
        _handovers = handovers;
        _uninstaller = Uninstaller.For(start.Identity.Build);
#if DEBUG
        // Unhandled, it fails fast with 0xC000027B and says nothing: the UI tests read why from here.
        UnhandledException += (_, args) => Console.Error.WriteLine($"{args.Message}{Environment.NewLine}{args.Exception}");
        DebugSettings.IsXamlResourceReferenceTracingEnabled = true;
        DebugSettings.XamlResourceReferenceFailed += (_, args) => Console.Error.WriteLine(args.Message);
#endif
        InitializeComponent();
        // Closing the window leaves Pitboard running, as the macOS app does.
        DispatcherShutdownMode = DispatcherShutdownMode.OnExplicitShutdown;
    }

    protected override void OnLaunched(Microsoft.UI.Xaml.LaunchActivatedEventArgs args)
    {
        var notifications = WindowsNotifications.For(_start.Identity.Build, AppContext.BaseDirectory);
        _notifications = notifications;
        var queue = DispatcherQueue.GetForCurrentThread();
        var model = Model(queue, notifications);
        if (model is null)
        {
            Exit();
            return;
        }

        _model = model;
        Shell.RestartAfterUpdate(_start.Identity.Build);
        var startAtSignIn = StartAtSignIn.For(_start, Program.Exe);
        _startAtSignIn = startAtSignIn;
        // Turned off in Windows Settings is a choice made there, which the setup tip does not ask about again.
        startAtSignIn.Changed += (_, _) => model.Send(new Intent.OpensAtLogin(startAtSignIn.On || startAtSignIn.TurnedOff));
        model.PropertyChanged += OnModelChanged;
        var image = new IconImage(Shell.NotificationIconPlace, Shell.NotificationIcon, Shell.DestroyIcon);
        _image = image;
        _hidden = new HiddenWindow(
            sessionEnding: () => _lifetime?.EndSession(),
            closing: () => _lifetime?.End(),
            icon: happened => _commands?.Heard(happened),
            taskbarCreated: () => _icon?.Readd(),
            displayChanged: () => image.Follow(shown => _icon?.Redraw(shown)));
        _icon = new NotifyIcon(new ShellNotificationArea(), _hidden.Handle, image.Handle, _start.Identity.IconGuid);
        ShowBar(model.MenuBar);
        var accountWindows = new AccountWindows(model, Web(), _hidden.Handle);
        _accountWindows = accountWindows;
        accountWindows.Follow(model.AccountWindows);
        _updates = SparkleUpdates.For(
            _start.Identity.Build, SparkleUpdates.Key(typeof(App).Assembly), _version, _start.DataFolder,
            action => queue.TryEnqueue(Ui.Guarded(action)), accountWindows.MayQuit, () => _lifetime?.End());
        _lifetime = new Lifetime(model, _icon, _updates, notifications, accountWindows.StopDownloads, CloseAndExit);
        _commands = new IconCommands(model, _updates, this);
        PowerManager.SystemSuspendStatusChanged += OnSuspendStatusChanged;
        Follow(model.WindowRequest);
        // A click arrives on a thread of COM's; the model takes a message from any.
        var launchedBy = notifications.Start(intent => _model?.Send(intent));
        model.Send(new Intent.Start());
        if (startAtSignIn.Refusal is null)
        {
            startAtSignIn.Read();
        }

        if (launchedBy is not null)
        {
            model.Send(launchedBy);
        }

        if (_start.Link is { } link)
        {
            model.Send(new Intent.LinkArrived(link));
        }
        else if (Launching.OpensWindow(_start.Arguments))
        {
            // Windows puts a new icon in the overflow, so a launch from Start shows only through its window.
            model.Send(new Intent.Show(Place: null));
        }

        // A second launch's handover arrives on a thread of its own.
        _handovers.Ready(handed => queue.TryEnqueue(Ui.Guarded(() => Handed(handed))));
        _updates.Start();
    }

    // Another copy's launch: its link waits for an account, and one without a link brings the window forward.
    private void Handed(string? link)
    {
        if (link is null)
        {
            ShowWindow();
        }
        else
        {
            _model?.Send(new Intent.LinkArrived(link));
        }
    }

    private AppModel? Model(DispatcherQueue queue, Notifications notifications)
    {
#if DEBUG
        if (_start.Fixture is { } fixture)
        {
            try
            {
                return AppModel.Listening(
                    queue, listener => PitboardModel.Fixture(fixture, listener, new WindowsLocalTime()));
            }
            catch (FixtureException refused)
            {
                Shell.Refuse(refused switch
                {
                    FixtureException.Unavailable unavailable => unavailable.reason,
                    FixtureException.Unknown unknown => unknown.reason,
                    FixtureException.Failed failed => failed.reason,
                    _ => refused.Message,
                });
                return null;
            }
        }
#endif
        var launch = _start.Launch(Program.Exe);
        // Reopen is called on a thread of the model's, and the window is the UI thread's.
        var apps = new WindowsApps(() => queue.TryEnqueue(Ui.Guarded(() => _window?.Bring())));
        return AppModel.Listening(
            queue,
            listener => new PitboardModel(launch, listener, apps, notifications, new WindowsLocalTime()));
    }

    // A fixture's windows show its stand-in pages, in a user data folder of the fixture's own, or of the UI tests' run.
    private WebEnvironment Web()
    {
#if DEBUG
        if (_start.Fixture is not null)
        {
            var shared = _start.Environment.GetValueOrDefault("PITBOARD_FIXTURE_WEBVIEW2");
            return WebEnvironment.Fixture(_start.DataFolder, string.IsNullOrEmpty(shared) ? null : shared);
        }
#endif
        return WebEnvironment.Live(_start.DataFolder, devTools: _start.Identity.Build is Build.Debug);
    }

    private void OnModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (_model is null)
        {
            return;
        }

        switch (args.PropertyName)
        {
            case nameof(AppModel.WindowRequest):
                Follow(_model.WindowRequest);
                break;
            case nameof(AppModel.MenuBar):
                ShowBar(_model.MenuBar);
                break;
            case nameof(AppModel.AccountWindows):
                _accountWindows?.Follow(_model.AccountWindows);
                break;
            // What was Pitboard's is gone, and the MSI takes the program once Pitboard.exe has exited.
            case nameof(AppModel.Removal) when _model.Removal is RemovalShown.Done:
                _uninstaller.Run();
                _lifetime?.End();
                break;
        }
    }

    // The notification area has no title beside its icon, so the icon and its tooltip carry the glance.
    private void ShowBar(MenuBarText bar)
    {
        _image?.Show(new IconLook(bar.Level, bar.Attention), shown => _icon?.Redraw(shown));
        _icon?.Show(bar.Tip);
    }

    void IIconTargets.ShowMenu(IReadOnlyList<IconMenuEntry> entries, IconEvent.Menu at, Action opened, Action<IconCommand> chosen)
    {
        if (_menu is null)
        {
            _menu = new MenuHost();
            _menu.Closed += (_, _) => _menu = null;
        }

        _menu.Show(entries, new PointInt32(at.X, at.Y), opened, chosen, cancelled: () => _icon?.ReturnFocus());
    }

    void IIconTargets.OpenLink(string url) => _ = Launcher.LaunchUriAsync(new Uri(url));

    void IIconTargets.OpenWindow(WindowAccount window) => _accountWindows?.Open(window);

    void IIconTargets.ShowSettings()
    {
        ShowWindow();
        _window?.ShowSettings();
    }

    Task<bool> IIconTargets.MayQuit() => _accountWindows?.MayQuit() ?? Task.FromResult(true);

    void IIconTargets.Quit() => _lifetime?.End();

    private void Follow(WindowRequest request)
    {
        if (_requests.Opens(request))
        {
            // The first launch's welcome is a window of its own, which stands in for the main window while it is open: the
            // launch's own request for the window can come after it.
            if (request.Place is Place.Welcome || (request.Place is null && _welcome is not null))
            {
                ShowWelcome();
                return;
            }

            ShowWindow();
        }

        if (_window is not null && _requests.PlaceFor(request) is { } place)
        {
            _window.Show(place);
        }
    }

    private void ShowWindow()
    {
        if (_model is null || _startAtSignIn is null)
        {
            return;
        }

        if (_window is null)
        {
            var window = new MainWindow(
                _model, _startAtSignIn, _updates, _version, account => _accountWindows?.Open(account),
                new WindowPlacement(Path.Combine(_start.DataFolder, "placement.json")), () => _notifications?.Read());
            window.Closed += (_, _) => _window = null;
            _window = window;
        }

        _window.Bring();
    }

    private void ShowWelcome()
    {
        if (_model is null || _startAtSignIn is null)
        {
            return;
        }

        if (_welcome is null)
        {
            var welcome = new WelcomeWindow(_model, _startAtSignIn, page => _ = Launcher.LaunchUriAsync(page));
            welcome.Closed += (_, _) => _welcome = null;
            _welcome = welcome;
        }

        _welcome.Bring();
    }

    // Closed first, where the main window keeps its placement.
    private void CloseAndExit()
    {
        _window?.Close();
        Exit();
    }

    // On the system's thread; the model takes a message from any.
    private void OnSuspendStatusChanged(object? sender, object args)
    {
        if (PowerManager.SystemSuspendStatus == SystemSuspendStatus.AutoResume)
        {
            _model?.Send(new Intent.Woke());
        }
    }
}
