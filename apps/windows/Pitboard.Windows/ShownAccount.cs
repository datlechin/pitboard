using System.Collections.ObjectModel;
using System.ComponentModel;
using Pitboard.Core;

namespace Pitboard.Windows;

// One of an expanded row's buttons, named for UI Automation apart from the same command in its shortcut menu.
public sealed record ShownCommand(string Id, AccountAction Action);

public sealed partial class ShownAccount(AccountItem item, Action<Intent> send) : INotifyPropertyChanged
{
    private bool _expanded;

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Key { get; } = item.Id;

    public AccountItem Item { get; private set; } = item;

    // Made again only when they change, so a button with focus keeps it through a read of the usage.
    public ObservableCollection<ShownCommand> Commands { get; } = [.. CommandsOf(item)];

    public bool HasCommands => Commands.Count > 0;

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

    public void Show(AccountItem item)
    {
        if (Structural.Equal(Item, item))
        {
            return;
        }

        Item = item;
        var commands = CommandsOf(item);
        if (!Structural.Equal(Commands.ToArray(), commands))
        {
            Commands.Clear();
            foreach (var command in commands)
            {
                Commands.Add(command);
            }

            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(HasCommands)));
        }

        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Item)));
    }

    public void Act()
    {
        if (Item.Action is { } action)
        {
            send(action.Intent);
        }
    }

    // Plan 4.9's order: its windows, then Rename and Remove; the rest is in its shortcut menu.
    private static ShownCommand[] CommandsOf(AccountItem item) =>
    [
        .. item.Actions.Where(action => action is { Kind: ActionKind.OpenWindow, Window: not null })
            .Select(action => new ShownCommand($"detail.window.{action.Window!.Site.Host}", action)),
        .. item.Actions.Where(action => action.Kind is ActionKind.Rename or ActionKind.Remove)
            .Select(action => new ShownCommand($"detail.{Identifiers.Of(action.Kind)}", action)),
    ];
}

public sealed partial class ShownSection(string key, string? heading, Action<Intent> send) : ObservableCollection<ShownAccount>
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
            this, section.Accounts, item => item.Id, shown => shown.Key, item => new ShownAccount(item, send),
            (shown, item) => shown.Show(item));
    }
}
