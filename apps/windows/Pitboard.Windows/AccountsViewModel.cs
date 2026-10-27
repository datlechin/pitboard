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
        [nameof(AppModel.Setup)] = [nameof(Setup)],
        [nameof(AppModel.Footing)] = [nameof(SetupFooting)],
        [nameof(AppModel.Sections)] = [nameof(Sections)],
        [nameof(AppModel.AccountWindows)] = [nameof(HeldWindows)],
    };

    public event PropertyChangedEventHandler? PropertyChanged;

    public ObservableCollection<ShownSection> Sections { get; } = [];

    public string Updated => model.UpdatedWindow;

    public bool CanRefresh => !model.Reading;

    public bool Listed => model.AccountsShown is AccountsShown.List;

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

        // An opened row dims a window whose store is still held.
        if (part is nameof(AppModel.Sections) or nameof(AppModel.AccountWindows))
        {
            ShowSections();
        }

        foreach (var name in told)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }
    }

    private void ShowSections() =>
        Keyed.Sync(
            Sections, model.Sections, section => section.Id, shown => shown.Key,
            section =>
            {
                var shown = new ShownSection(section.Id, section.Heading, model.Send, () => model.AccountWindows.Held);
                shown.Show(section);
                return shown;
            },
            (shown, section) => shown.Show(section));
}
