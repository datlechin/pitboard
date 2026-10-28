using System.ComponentModel;
using System.Reflection;
using System.Runtime.CompilerServices;
using Microsoft.UI.Dispatching;
using Pitboard.Core;

namespace Pitboard.Windows;

// The macOS AppModel's rules: an older snapshot is dropped, and only the parts a snapshot names are told where it
// changed since the one shown, every part where it changed since another. Nothing is compared.
public sealed class AppModel : INotifyPropertyChanged
{
    // Each part is the snapshot's property of its name, and this model's: EveryPartNamesOneProperty and
    // TheModelShowsEveryPartOfASnapshot hold both.
    private static readonly Dictionary<Part, PropertyInfo> Properties =
        Enum.GetValues<Part>().ToDictionary(part => part, part => typeof(Snapshot).GetProperty(part.ToString())!);

    private readonly Dictionary<string, object?> _parts = [];
    private readonly IPitboardModel? _model;

    public AppModel(Snapshot first)
    {
        Revision = first.Revision;
        foreach (var property in Properties.Values)
        {
            _parts[property.Name] = property.GetValue(first);
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

        var parts = snapshot.Changed.Since == Revision ? snapshot.Changed.Parts : Enum.GetValues<Part>();
        Revision = snapshot.Revision;
        string[] assigned = [nameof(Revision), .. parts.Select(part => Properties[part].Name)];
        foreach (var part in parts)
        {
            var property = Properties[part];
            _parts[property.Name] = property.GetValue(snapshot);
        }

        foreach (var name in assigned)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }

        return assigned;
    }

    public ulong Revision { get; private set; }

    public bool Reading => Shown<bool>();

    public WindowQuestion? QuitQuestion => Shown<WindowQuestion?>();

    public WindowQuestion? ReplaceQuestion => Shown<WindowQuestion?>();

    public WindowRequest WindowRequest => Shown<WindowRequest>();

    public RunningSignIn? SigningIn => Shown<RunningSignIn?>();

    public Sheet? Sheet => Shown<Sheet?>();

    public FailureText? SheetFailure => Shown<FailureText?>();

    public StatusItemShown? StatusItem => Shown<StatusItemShown?>();

    public AccountSection[] Sections => Shown<AccountSection[]>();

    public AccountMenuShown AccountMenu => Shown<AccountMenuShown>();

    public PanelNotice[] Notices => Shown<PanelNotice[]>();

    public MenuNotices MenuNotices => Shown<MenuNotices>();

    public Footing Footing => Shown<Footing>();

    public SetupStep? Setup => Shown<SetupStep?>();

    public AccountsShown AccountsShown => Shown<AccountsShown>();

    public string? MenuAccountsNote => Shown<string?>();

    public long? UpdatedAt => Shown<long?>();

    public string UpdatedMenu => Shown<string>();

    public string UpdatedWindow => Shown<string>();

    public SheetText? SheetText => Shown<SheetText?>();

    public SigningInText? SigningInText => Shown<SigningInText?>();

    public StowText? StowText => Shown<StowText?>();

    public AlertText? FailureAlert => Shown<AlertText?>();

    public MachineShown Machine => Shown<MachineShown>();

    public AccountWindowsShown AccountWindows => Shown<AccountWindowsShown>();

    public ItemOffer Uninstall => Shown<ItemOffer>();

    public RemovalShown? Removal => Shown<RemovalShown?>();

    public WelcomeShown Welcome => Shown<WelcomeShown>();

    public ulong WidgetRevision => Shown<ulong>();

    public OutsideAnswer[] Answers => Shown<OutsideAnswer[]>();

    public QuickAction[]? QuickActions => Shown<QuickAction[]?>();

    private T Shown<T>([CallerMemberName] string name = "") => (T)_parts[name]!;
}

// The model's next snapshot waits for this call, so it only queues the snapshot on the app's thread.
public sealed class ModelListening(DispatcherQueue queue) : Listener
{
    public Action<Snapshot>? Deliver { get; set; }

    public void Changed(Snapshot snapshot) => queue.TryEnqueue(Ui.Guarded(() => Deliver?.Invoke(snapshot)));
}
