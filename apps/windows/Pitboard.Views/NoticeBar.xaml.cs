using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Core;
using Pitboard.Windows;

namespace Pitboard.Views;

public sealed partial class NoticeBar : UserControl
{
    private readonly Action<NoticeAction> _perform;
    private readonly Action<Uri> _open;
    private readonly TimeProvider _time;
    private readonly DispatcherQueueTimer _timer;
    private PanelNotice _notice;
    private bool _shown;

    public NoticeBar(PanelNotice notice, Action<NoticeAction> perform, Action<Uri> open, TimeProvider time)
    {
        _notice = notice;
        _perform = perform;
        _open = open;
        _time = time;
        InitializeComponent();
        _timer = DispatcherQueue.CreateTimer();
        _timer.Interval = TimeSpan.FromSeconds(1);
        _timer.Tick += (_, _) => Count();
        Loaded += (_, _) =>
        {
            _shown = true;
            Count();
        };
        Unloaded += (_, _) =>
        {
            _shown = false;
            _timer.Stop();
        };
        Fill(notice, buttonsChanged: true);
    }

    public string Key => _notice.Id;

    public static string ActionId(Intent intent) => $"notice.action.{Identifiers.Of(intent)}";

    public void Show(PanelNotice notice)
    {
        if (!Structural.Equal(notice, _notice))
        {
            Fill(notice, !Structural.Equal(notice.Actions, _notice.Actions));
        }
    }

    // Its buttons are made again only when they change, so one with focus keeps it.
    private void Fill(PanelNotice notice, bool buttonsChanged)
    {
        _notice = notice;
        AutomationProperties.SetAutomationId(Bar, $"notice.{notice.Id}");
        Bar.Severity = notice.Severity switch
        {
            Severity.Warning => InfoBarSeverity.Warning,
            Severity.Error => InfoBarSeverity.Error,
            _ => InfoBarSeverity.Informational,
        };
        Bar.Title = notice.Title;
        Bar.Message = string.Join("\n\n", notice.Lines);
        Bar.IsClosable = notice.Actions.Any(action => action.Dismisses);
        var buttons = notice.Actions.Where(action => !action.Dismisses).ToArray();
        if (buttonsChanged)
        {
            Actions.ItemsSource = buttons;
        }

        Actions.Visibility = Look.Shown(buttons.Length > 0);
        LearnLink.Content = notice.Learn?.Title;
        LearnLink.Visibility = Look.Shown(notice.Learn is not null);
        Count();
        Below.Visibility = Look.Shown(buttons.Length > 0 || notice.Learn is not null || notice.Until is not null);
    }

    // Counts down by itself between snapshots, and stops at nought.
    private void Count()
    {
        if (_notice is not { Until: { } until, UntilLabel: { } label })
        {
            CountdownText.Visibility = Visibility.Collapsed;
            _timer.Stop();
            return;
        }

        var now = _time.GetUtcNow();
        CountdownText.Text = $"{label} {Countdown.Left(until, now)}";
        CountdownText.Visibility = Visibility.Visible;
        if (Countdown.Ended(until, now) || !_shown)
        {
            _timer.Stop();
        }
        else if (!_timer.IsRunning)
        {
            _timer.Start();
        }
    }

    private void OnAction(object sender, RoutedEventArgs args)
    {
        if (sender is FrameworkElement { DataContext: NoticeAction action })
        {
            _perform(action);
        }
    }

    private void OnLearn(object sender, RoutedEventArgs args)
    {
        if (_notice.Learn is { } learn)
        {
            _open(new Uri(learn.Link));
        }
    }

    // The model takes the notice away once it is dismissed.
    private void OnClosing(InfoBar sender, InfoBarClosingEventArgs args)
    {
        if (args.Reason != InfoBarCloseReason.CloseButton)
        {
            return;
        }

        args.Cancel = true;
        if (_notice.Actions.FirstOrDefault(action => action.Dismisses) is { } dismiss)
        {
            _perform(dismiss);
        }
    }
}
