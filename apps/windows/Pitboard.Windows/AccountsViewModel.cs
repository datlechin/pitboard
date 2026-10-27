using System.Collections.ObjectModel;
using System.ComponentModel;
using Pitboard.Core;

namespace Pitboard.Windows;

public enum EmptyKind
{
    None,
    Reading,
    NoTool,
    ReadFailed,
    NoAccounts,
}

public sealed record EmptyShown(EmptyKind Kind, string Title, string Detail, string Choice, bool Prominent, bool Enabled)
{
    public static readonly EmptyShown None = new(EmptyKind.None, "", "", "", Prominent: false, Enabled: false);
}

public sealed partial class AccountsViewModel(AppModel model, Action<Uri> open) : INotifyPropertyChanged
{
    private static readonly Dictionary<string, string[]> Told = new()
    {
        [nameof(AppModel.AccountsShown)] = [nameof(Listed), nameof(ShowsEmpty), nameof(Empty)],
        [nameof(AppModel.UpdatedWindow)] = [nameof(Updated)],
        [nameof(AppModel.Reading)] = [nameof(CanRefresh)],
        [nameof(AppModel.Notices)] = [nameof(Notices)],
        [nameof(AppModel.Setup)] =
            [nameof(Setup), nameof(HasSetup), nameof(SetupTitle), nameof(SetupDetail), nameof(Listed), nameof(ShowsEmpty)],
        [nameof(AppModel.Footing)] = [nameof(SetupFooting)],
        [nameof(AppModel.Sections)] = [nameof(Sections)],
    };

    public event PropertyChangedEventHandler? PropertyChanged;

    // Without headings while there is one tool: an empty group header still takes room.
    public ObservableCollection<ShownAccount> Rows { get; } = [];

    public ObservableCollection<ShownSection> Sections { get; } = [];

    public bool Grouped { get; private set; }

    public string Updated => model.UpdatedWindow;

    public bool CanRefresh => !model.Reading;

    // The setup tip sits in the list's header, so No Accounts under a tip keeps the list it is in.
    public bool Listed =>
        model.AccountsShown is AccountsShown.List || (model.AccountsShown is AccountsShown.NoAccounts && HasSetup);

    public bool ShowsEmpty => !Listed;

    public EmptyShown Empty => model.AccountsShown switch
    {
        AccountsShown.NoTool none => new(EmptyKind.NoTool, none.Title, none.Detail, none.LinkTitle, Prominent: true, Enabled: true),
        AccountsShown.ReadFailed failed =>
            new(EmptyKind.ReadFailed, failed.Title, failed.Detail, failed.Retry.Title, Prominent: false, failed.Retry.Enabled),
        AccountsShown.NoAccounts none =>
            new(EmptyKind.NoAccounts, none.Title, none.Detail, none.Add.Title, Prominent: true, none.Add.Enabled),
        AccountsShown.Reading reading => new(EmptyKind.Reading, reading.Title, "", "", Prominent: false, Enabled: false),
        _ => EmptyShown.None,
    };

    public PanelNotice[] Notices => model.Notices;

    public SetupStep? Setup => model.Setup;

    public bool HasSetup => model.Setup is not null;

    public string SetupTitle => model.Setup?.Title ?? "";

    public string SetupDetail => model.Setup?.Detail ?? "";

    public Footing SetupFooting => model.Footing;

    public IReadOnlyCollection<string> HeldWindows => model.AccountWindows.Held;

    // From the page's first showing until it is unloaded; nothing it missed meanwhile is lost.
    public void Listen()
    {
        model.PropertyChanged -= OnModelChanged;
        model.PropertyChanged += OnModelChanged;
        ShowSections();
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(string.Empty));
    }

    public void Stop() => model.PropertyChanged -= OnModelChanged;

    public void Send(Intent intent) => model.Send(intent);

    // Every visit: the model reads the accounts whose numbers are old.
    public void Shown() => model.Send(new Intent.PaneShown(Pane.Accounts));

    public void Refresh() => model.Send(new Intent.Refresh(Asked: true));

    public void Add() => model.Send(new Intent.PresentSheet(new Sheet.Add(Provider: null)));

    public void ChooseEmpty()
    {
        switch (model.AccountsShown)
        {
            case AccountsShown.NoTool none when Uri.TryCreate(none.Link, UriKind.Absolute, out var link):
                open(link);
                break;
            case AccountsShown.ReadFailed failed:
                model.Send(failed.Retry.Intent);
                break;
            case AccountsShown.NoAccounts none:
                model.Send(none.Add.Intent);
                break;
        }
    }

    private void OnModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (args.PropertyName is not { } part || !Told.TryGetValue(part, out var told))
        {
            return;
        }

        if (part == nameof(AppModel.Sections))
        {
            ShowSections();
        }

        foreach (var name in told)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }
    }

    private void ShowSections()
    {
        var sections = model.Sections;
        var grouped = sections.Any(section => section.Heading is not null);
        if (grouped)
        {
            Rows.Clear();
            Keyed.Sync(
                Sections, sections, section => section.Id, shown => shown.Key,
                section =>
                {
                    var shown = new ShownSection(section.Id, section.Heading, model.Send);
                    shown.Show(section);
                    return shown;
                },
                (shown, section) => shown.Show(section));
        }
        else
        {
            Sections.Clear();
            Keyed.Sync(
                Rows, sections.SelectMany(section => section.Accounts).ToArray(), item => item.Id, shown => shown.Key,
                item => new ShownAccount(item, model.Send), (shown, item) => shown.Show(item));
        }

        if (grouped != Grouped)
        {
            Grouped = grouped;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Grouped)));
        }
    }
}
