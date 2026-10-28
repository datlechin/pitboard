using System.Collections.ObjectModel;
using System.ComponentModel;
using Pitboard.Core;

namespace Pitboard.Windows;

public sealed record SettingsLink(string Title, string Url);

// What the settings page shows and sends, by the macOS Settings' rules. A setter sends only what differs from what the
// model says, so a control set to the model's value sends nothing.
public sealed partial class SettingsViewModel : INotifyPropertyChanged
{
    // Settings > Apps > Startup and System > Notifications, by the URIs Microsoft's "Launch the Windows Settings app" lists.
    public const string StartupSettings = "ms-settings:startupapps";

    public const string NotificationSettings = "ms-settings:notifications";

    private const int MinutesInADay = 24 * 60;

    private const string Site = "https://usepitboard.com";

    // The site's own link says its name.
    public static readonly SettingsLink[] Links =
    [
        new(new Uri(Site).Host, Site),
        new(Words.Settings.Help, IconMenu.HelpLink),
        new(Words.Settings.SourceCode, "https://github.com/datlechin/pitboard"),
        new(Words.Settings.ReportAProblem, "https://github.com/datlechin/pitboard/issues"),
        new(Words.Settings.License, "https://github.com/datlechin/pitboard/blob/main/LICENSE"),
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

    public bool CanStartAtSignIn => _start.CanTurnOn;

    public string StartupNote => _start.Note(Words.Settings.OpenAtLoginNote);

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

    public string TermsLinkTitle => AtLimit.Terms.LinkTitle;

    public string TermsLink => AtLimit.Terms.Link;

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

    // Uninstall…, or once it stopped short, Try again, which asks nothing more.
    public Choice UninstallChoice =>
        _model.Removal is RemovalShown.Stopped stopped
            ? stopped.Again
            : new Choice(_model.Uninstall.Title, _model.Uninstall.Intent, _model.Uninstall.Enabled && _model.Removal is null);

    public Question? UninstallQuestion => _model.Removal is RemovalShown.Stopped ? null : _model.Uninstall.Confirm;

    public bool Uninstalling => _model.Removal is RemovalShown.Removing or RemovalShown.Done;

    public bool OffersUninstall => !Uninstalling;

    // What it does, then what it is doing, and where it stopped short, why.
    public string UninstallSays => _model.Removal switch
    {
        RemovalShown.Removing removing => removing.Title,
        RemovalShown.Stopped stopped => stopped.Line,
        _ => Words.Settings.UninstallNote,
    };

    public string? UninstallStopped => (_model.Removal as RemovalShown.Stopped)?.Title;

    public bool UpdatesAvailable => _updates.Available;

    public bool ChecksAutomatically => _updates.ChecksAutomatically;

    public string UpdatesState =>
        _updates.Waiting is { } waiting ? PitboardModelMethods.ReadyToInstall(waiting) : PitboardModelMethods.NamedVersion(Version);

    public string[] UpdatesNoteLines => UpdatesNotes(_updates, Version);

    public string Version { get; }

    public static string[] UpdatesNotes(IUpdates updates, string version) =>
        [
            .. updates.Refusal is { } refusal ? new[] { refusal } : [],
            .. Versions.IsBeta(version) && Words.Settings.BetaUpdates is { } beta ? new[] { beta } : [],
        ];

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

    public void Uninstall() => _model.Send(UninstallChoice.Intent);

    private void OnModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (args.PropertyName is nameof(AppModel.Machine) or nameof(AppModel.Uninstall) or nameof(AppModel.Removal))
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
