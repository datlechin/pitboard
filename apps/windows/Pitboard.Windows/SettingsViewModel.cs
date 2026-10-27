using System.Collections.ObjectModel;
using System.ComponentModel;
using Pitboard.Core;

namespace Pitboard.Windows;

public sealed record SettingsLink(string Title, string Url);

// What the settings page shows and sends, by the macOS Settings' rules. A setter sends only what differs from what the
// model says, so a control set to the model's value sends nothing.
public sealed partial class SettingsViewModel : INotifyPropertyChanged
{
    public const string TurnedOffNote = "Turned off in Settings > Apps > Startup, where you can turn it on again.";

    // Settings > Apps > Startup and System > Notifications, by the URIs Microsoft's "Launch the Windows Settings app" lists.
    public const string StartupSettings = "ms-settings:startupapps";

    public const string NotificationSettings = "ms-settings:notifications";

    public const string BetaNote = "A beta updates itself to 1.0.0 once that is out. Install a later beta by hand.";

    public const string Copyright = "Copyright 2026 the Pitboard authors";

    private const int MinutesInADay = 24 * 60;

    public static readonly SettingsLink[] Links =
    [
        new("usepitboard.com", "https://usepitboard.com"),
        new("Pitboard help", IconMenu.HelpLink),
        new("Source code on GitHub", "https://github.com/datlechin/pitboard"),
        new("Report a problem", "https://github.com/datlechin/pitboard/issues"),
        new("Apache License 2.0", "https://github.com/datlechin/pitboard/blob/main/LICENSE"),
    ];

    private readonly AppModel _model;
    private readonly StartAtSignIn _start;
    private readonly IUpdates _updates;
    private readonly Func<NotificationsBlocked?> _notifications;
    private NotificationsBlocked? _blocked;

    public SettingsViewModel(
        AppModel model, StartAtSignIn start, IUpdates updates, string version, Func<NotificationsBlocked?> notifications)
    {
        _model = model;
        _start = start;
        _updates = updates;
        _notifications = notifications;
        Version = version;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    private MachineShown Machine => _model.Machine;

    private AtLimitShown AtLimit => Machine.AtLimit;

    private WarmShown Warm => Machine.Warm;

    private ScheduleShown Schedule => Machine.Schedule;

    private CommandLineShown CommandLine => Machine.CommandLine;

    public bool StartsAtSignIn => _start.On;

    public bool CanStartAtSignIn => _start.Refusal is null && !_start.TurnedOff;

    public string StartupNote => _start.Refusal ?? (_start.TurnedOff ? TurnedOffNote : "");

    public bool HasStartupNote => StartupNote.Length > 0;

    public string? StartupFailed => _start.Failed;

    public bool OffersStartupSettings => _start.TurnedOff;

    // Why Windows shows none of a Release build's notifications; empty where it shows them.
    public string NotificationsOff => _blocked?.Why ?? "";

    public bool HasNotificationsOff => _blocked is not null;

    public bool OffersNotificationSettings => _blocked?.InSettings == true;

    // The choice's name, which says the share; its card's header stays put.
    public string AtLimitHeading => AtLimit.Heading;

    public string AtLimitLabel => AtLimit.AtLabel;

    public string[] AtLimitTitles => [.. AtLimit.Choices.Select(choice => choice.Title)];

    public int AtLimitIndex => Array.FindIndex(AtLimit.Choices, choice => choice.Choice == AtLimit.Choice);

    public bool CanChooseAtLimit => AtLimit.Enabled;

    public string AutoSwitchStanding => AtLimit.Standing ?? "";

    public bool HasAutoSwitchStanding => AtLimit.Standing is not null;

    public string AtLimitNote => AtLimit.Note ?? "";

    public bool HasAtLimitNote => AtLimit.Note is not null;

    public string AtLimitTerms => AtLimit.Terms?.Sentence ?? "";

    public bool HasAtLimitTerms => AtLimit.Terms is not null;

    public string WarmTerms => Warm.Terms?.Sentence ?? "";

    // One page, said beside switching while it is chosen and beside warm-up otherwise.
    public string TermsLinkTitle => (AtLimit.Terms ?? Warm.Terms)?.LinkTitle ?? "";

    public string TermsLink => (AtLimit.Terms ?? Warm.Terms)?.Link ?? "";

    public double Share => AtLimit.At;

    public double LowestShare => AtLimit.Lowest;

    public double HighestShare => AtLimit.Highest;

    public string ShareTitle => AtLimit.ShareLabel;

    public string ShareRange => AtLimit.ShareRange;

    public bool CanSetShare => AtLimit.ShareEnabled;

    public string WarmRange => Warm.RangeLabel;

    public string WarmStanding => Warm.Standing ?? "";

    public bool HasWarmStanding => Warm.Standing is not null;

    public TimeSpan? WarmFrom => TimeOfDay(Warm.From);

    public TimeSpan? WarmTo => TimeOfDay(Warm.To);

    public bool CanWarm => Warm.Enabled;

    public ObservableCollection<WarmAccountRow> WarmAccounts { get; } = [];

    public string WarmTitle => Warm.Title;

    public bool NoWarmAccounts => Warm.NoneEnrolled is not null;

    public string NoWarmAccount => Warm.NoneEnrolled ?? "";

    public string WarmNote => Warm.Note;

    public bool RenewsDaily => Schedule.On;

    public bool CanRenewDaily => Schedule.Enabled;

    public string RenewalAbout => Schedule.About;

    public string ScheduleNote => Schedule.Note ?? "";

    public bool HasScheduleNote => Schedule.Note is not null;

    public string? ScheduleFailed => Schedule.Failed;

    public bool LookingForCommandLine => CommandLine.InTerminal is null;

    public string InTerminal => CommandLine.InTerminal ?? "";

    public string UpdateNote => CommandLine.UpdateNote ?? "";

    public bool HasUpdateNote => CommandLine.UpdateNote is not null;

    public string TerminalNote => CommandLine.TerminalNote ?? "";

    public bool HasTerminalNote => CommandLine.TerminalNote is not null;

    public bool UpdatesAvailable => _updates.Available;

    public bool ChecksAutomatically => _updates.ChecksAutomatically;

    public string UpdatesState => _updates.Waiting is { } waiting ? $"Pitboard {waiting} is ready to install." : Versions.Named(Version);

    public string[] UpdatesNoteLines => UpdatesNotes(_updates, Version);

    public string Version { get; }

    public static string[] UpdatesNotes(IUpdates updates, string version) =>
        [.. updates.Refusal is { } refusal ? new[] { refusal } : [], .. Versions.IsBeta(version) ? new[] { BetaNote } : []];

    // 24:00, the end of a day, shows as midnight, as the macOS pickers show it.
    public static TimeSpan TimeOfDay(ushort minutes) => TimeSpan.FromMinutes(minutes % MinutesInADay);

    public static ushort Minutes(TimeSpan time) => (ushort)(((time.Hours * 60) + time.Minutes) % MinutesInADay);

    // A NumberBox holds NaN once its text is cleared.
    public static byte? ShareOf(double value, AtLimitShown atLimit) =>
        double.IsNaN(value) ? null : (byte)Math.Clamp(Math.Round(value), atLimit.Lowest, atLimit.Highest);

    public void Listen()
    {
        Stop();
        _model.PropertyChanged += OnModelChanged;
        _start.Changed += OnChanged;
        _updates.Changed += OnChanged;
        Tell();
    }

    public void Stop()
    {
        _model.PropertyChanged -= OnModelChanged;
        _start.Changed -= OnChanged;
        _updates.Changed -= OnChanged;
    }

    // Every visit: a terminal can change the schedule and the command line, and another program the Run key.
    public void Shown()
    {
        ReadWindowsSettings();
        _model.Send(new Intent.ReadSchedule());
        _model.Send(new Intent.LookForCommandLine());
        _updates.LookForFeed();
    }

    // Also as the window comes back, as from Settings > Apps > Startup or System > Notifications, as the macOS settings do.
    public void ReadWindowsSettings()
    {
        _blocked = _notifications();
        Tell();
        _start.Read();
    }

    public void SetStartsAtSignIn(bool on)
    {
        if (on != _start.On)
        {
            _start.Set(on);
        }
    }

    public void ChooseAtLimit(int index)
    {
        if (index >= 0 && index < AtLimit.Choices.Length && AtLimit.Choices[index].Choice != AtLimit.Choice)
        {
            _model.Send(new Intent.SetAtLimit(AtLimit.Choices[index].Choice, AtLimit.At));
        }
    }

    // False for a cleared box, which then shows the share again.
    public bool SetShare(double value)
    {
        if (ShareOf(value, AtLimit) is not { } at)
        {
            return false;
        }

        if (at != AtLimit.At)
        {
            _model.Send(new Intent.SetAtLimit(AtLimit.Choice, at));
        }

        return true;
    }

    // Each picker sends the model's own other end, so a span to 24:00 is not cut to midnight.
    public void SetWarmFrom(TimeSpan picked)
    {
        if (picked != TimeOfDay(Warm.From))
        {
            _model.Send(new Intent.SetWarmRange(Minutes(picked), Warm.To));
        }
    }

    public void SetWarmTo(TimeSpan picked)
    {
        if (picked != TimeOfDay(Warm.To))
        {
            _model.Send(new Intent.SetWarmRange(Warm.From, Minutes(picked)));
        }
    }

    public void SetWarm(string qualified, bool on)
    {
        if (Warm.Accounts.FirstOrDefault(account => account.Qualified == qualified) is { } account && account.On != on)
        {
            _model.Send(new Intent.SetWarm(qualified, on));
        }
    }

    public void SetRenewsDaily(bool on)
    {
        if (on != Schedule.On)
        {
            _model.Send(new Intent.SetSchedule(on));
        }
    }

    public void SetChecksAutomatically(bool on)
    {
        if (on != _updates.ChecksAutomatically)
        {
            _updates.ChecksAutomatically = on;
        }
    }

    public void CheckNow() => _updates.Check();

    private void OnModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (args.PropertyName == nameof(AppModel.Machine))
        {
            Tell();
        }
    }

    private void OnChanged(object? sender, EventArgs args) => Tell();

    private void Tell()
    {
        Keyed.Sync(
            WarmAccounts, Warm.Accounts, account => account.Qualified, shown => shown.Qualified,
            account => new WarmAccountRow(account, Warm.Enabled), (shown, account) => shown.Show(account, Warm.Enabled));
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(string.Empty));
    }
}
