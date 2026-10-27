using System.Collections.ObjectModel;
using System.ComponentModel;
using Pitboard.Core;

namespace Pitboard.Windows;

// One of an expanded row's buttons, named for UI Automation apart from the same command in its shortcut menu, and why
// its window cannot open yet where its store is still held.
public sealed record ShownCommand(string Id, AccountAction Action, string? Held = null)
{
    public bool Enabled => Action.Enabled && Held is null;

    public string HelpText => Held ?? "";
}

// A note past the one a row has room for, with the docs about it where nothing in the app acts on it.
public sealed record ShownNote(string Text, SetupLink? Learn)
{
    public bool HasLearn => Learn is not null;

    public string LearnTitle => Learn?.Title ?? "";
}

public sealed partial class ShownAccount : INotifyPropertyChanged
{
    private readonly Action<Intent> _send;
    private readonly Func<IReadOnlyCollection<string>> _held;
    private bool _expanded;

    // `held`: the stores whose windows open nothing yet.
    public ShownAccount(AccountItem item, Action<Intent> send, Func<IReadOnlyCollection<string>>? held = null)
    {
        _send = send;
        _held = held ?? (() => []);
        Key = item.Id;
        Item = item;
        Commands = [.. CommandsOf(item, _held())];
        MoreNotes = [.. NotesOf(item)];
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Key { get; }

    public AccountItem Item { get; private set; }

    // Made again only when they change, so a button with focus keeps it through a read of the usage.
    public ObservableCollection<ShownCommand> Commands { get; }

    public bool HasCommands => Commands.Count > 0;

    public ObservableCollection<ShownNote> MoreNotes { get; }

    // Kept with the account, so a row made again for it opens as it was left.
    public bool Expanded
    {
        get => _expanded;
        set
        {
            if (_expanded != value)
            {
                _expanded = value;
                PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Expanded)));
            }
        }
    }

    // Also as a held store lets go, which changes no account.
    public void Show(AccountItem item)
    {
        if (Sync(Commands, CommandsOf(item, _held())))
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(HasCommands)));
        }

        Sync(MoreNotes, NotesOf(item));
        if (Structural.Equal(Item, item))
        {
            return;
        }

        Item = item;
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Item)));
    }

    public void Act()
    {
        if (Item.Action is { } action)
        {
            _send(action.Intent);
        }
    }

    // Plan 4.9's order: its windows, then what its shortcut menu offers past the row's own button, and Remove, dimmed for
    // the account in use.
    private static ShownCommand[] CommandsOf(AccountItem item, IReadOnlyCollection<string> held) =>
    [
        .. item.Actions.Where(action => action is { Kind: ActionKind.OpenWindow, Window: not null })
            .Select(action => new ShownCommand($"detail.window.{action.Window!.Site.Host}", action, HeldStores.Note(action.Window!, held))),
        .. item.Actions
            .Where(action => action is { Kind: ActionKind.SignInAgain or ActionKind.Rename or ActionKind.WarmUpNow, Enabled: true }
                && !Equals(action.Intent, item.Action?.Intent))
            .Select(action => new ShownCommand($"detail.{Identifiers.Of(action.Kind)}", action)),
        .. item.Actions.Where(action => action.Kind is ActionKind.Remove)
            .Select(action => new ShownCommand($"detail.{Identifiers.Of(action.Kind)}", action)),
    ];

    // What a note does is among the commands, so only its docs are beside it, as on the row.
    private static ShownNote[] NotesOf(AccountItem item) => [.. item.Notes.Skip(1).Select(note => new ShownNote(note.Text, note.Learn))];

    private static bool Sync<T>(ObservableCollection<T> shown, T[] wanted)
    {
        if (Structural.Equal(shown.ToArray(), wanted))
        {
            return false;
        }

        shown.Clear();
        foreach (var one in wanted)
        {
            shown.Add(one);
        }

        return true;
    }
}

public sealed partial class ShownSection(string key, string? heading, Action<Intent> send, Func<IReadOnlyCollection<string>>? held = null)
    : ObservableCollection<ShownAccount>
{
    public string Key { get; } = key;

    public string? Heading { get; private set; } = heading;

    // A tool's name heads its accounts only once another tool's are shown.
    public bool Headed => Heading is not null;

    public void Show(AccountSection section)
    {
        if (Heading != section.Heading)
        {
            Heading = section.Heading;
            OnPropertyChanged(new PropertyChangedEventArgs(nameof(Heading)));
            OnPropertyChanged(new PropertyChangedEventArgs(nameof(Headed)));
        }

        Keyed.Sync(
            this, section.Accounts, item => item.Id, shown => shown.Key, item => new ShownAccount(item, send, held),
            (shown, item) => shown.Show(item));
    }
}
