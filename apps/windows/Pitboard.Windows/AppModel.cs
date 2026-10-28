using System.ComponentModel;
using System.Reflection;
using System.Runtime.CompilerServices;
using Microsoft.UI.Dispatching;
using Pitboard.Core;

namespace Pitboard.Windows;

// The macOS AppModel's rules: an older snapshot is dropped, and only the parts that changed are told.
public sealed class AppModel : INotifyPropertyChanged
{
    private static readonly PropertyInfo[] Parts =
        typeof(Snapshot).GetProperties(BindingFlags.Public | BindingFlags.Instance);

    private readonly Dictionary<string, object?> _parts = [];
    private readonly IPitboardModel? _model;

    public AppModel(Snapshot first)
    {
        foreach (var part in Parts)
        {
            _parts[part.Name] = part.GetValue(first);
        }
    }

    public AppModel(IPitboardModel model)
        : this(model.Snapshot())
    {
        _model = model;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    // The queue runs nothing it was handed before this returns, so Deliver is set before any snapshot.
    public static AppModel Listening(DispatcherQueue queue, Func<Listener, IPitboardModel> make)
    {
        var listening = new ModelListening(queue);
        var app = new AppModel(make(listening));
        listening.Deliver = snapshot => app.Apply(snapshot);
        return app;
    }

    public void Send(Intent intent) => _model?.Send(intent);

    public void Shutdown() => _model?.Shutdown();

    public IReadOnlyList<string> Apply(Snapshot snapshot)
    {
        if (snapshot.Revision <= Revision)
        {
            return [];
        }

        var assigned = new List<string>();
        foreach (var part in Parts)
        {
            var value = part.GetValue(snapshot);
            if (!Structural.Equal(_parts[part.Name], value))
            {
                _parts[part.Name] = value;
                assigned.Add(part.Name);
            }
        }

        foreach (var name in assigned)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }

        return assigned;
    }

    public ulong Revision => Part<ulong>();

    public bool Reading => Part<bool>();

    public WindowQuestion? QuitQuestion => Part<WindowQuestion?>();

    public WindowQuestion? ReplaceQuestion => Part<WindowQuestion?>();

    public WindowRequest WindowRequest => Part<WindowRequest>();

    public RunningSignIn? SigningIn => Part<RunningSignIn?>();

    public Sheet? Sheet => Part<Sheet?>();

    public FailureText? SheetFailure => Part<FailureText?>();

    public StatusItemShown? StatusItem => Part<StatusItemShown?>();

    public AccountSection[] Sections => Part<AccountSection[]>();

    public AccountMenuShown AccountMenu => Part<AccountMenuShown>();

    public PanelNotice[] Notices => Part<PanelNotice[]>();

    public MenuNotices MenuNotices => Part<MenuNotices>();

    public Footing Footing => Part<Footing>();

    public SetupStep? Setup => Part<SetupStep?>();

    public AccountsShown AccountsShown => Part<AccountsShown>();

    public string? MenuAccountsNote => Part<string?>();

    public long? UpdatedAt => Part<long?>();

    public string UpdatedMenu => Part<string>();

    public string UpdatedWindow => Part<string>();

    public SheetText? SheetText => Part<SheetText?>();

    public SigningInText? SigningInText => Part<SigningInText?>();

    public StowText? StowText => Part<StowText?>();

    public AlertText? FailureAlert => Part<AlertText?>();

    public MachineShown Machine => Part<MachineShown>();

    public AccountWindowsShown AccountWindows => Part<AccountWindowsShown>();

    public ItemOffer Uninstall => Part<ItemOffer>();

    public RemovalShown? Removal => Part<RemovalShown?>();

    public WelcomeShown Welcome => Part<WelcomeShown>();

    public ulong WidgetRevision => Part<ulong>();

    public OutsideAnswer[] Answers => Part<OutsideAnswer[]>();

    public QuickAction[]? QuickActions => Part<QuickAction[]?>();

    private T Part<T>([CallerMemberName] string name = "") => (T)_parts[name]!;
}

// The model's next snapshot waits for this call, so it only queues the snapshot on the app's thread.
public sealed class ModelListening(DispatcherQueue queue) : Listener
{
    public Action<Snapshot>? Deliver { get; set; }

    public void Changed(Snapshot snapshot) => queue.TryEnqueue(Ui.Guarded(() => Deliver?.Invoke(snapshot)));
}
