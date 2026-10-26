using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using Pitboard.Core;
using Pitboard.Windows;

namespace Pitboard.Views;

public sealed partial class SettingsPage : Page
{
    private PaneContext? _context;

    public SettingsPage()
    {
        InitializeComponent();
        foreach (var link in SettingsViewModel.Links)
        {
            var button = new HyperlinkButton { Content = link.Title };
            button.Click += (_, _) => Context.Open(new Uri(link.Url));
            Links.Children.Add(button);
        }

        Loaded += (_, _) => ViewModel?.Listen();
        Unloaded += (_, _) => ViewModel?.Stop();
    }

    public SettingsViewModel? ViewModel { get; private set; }

    private PaneContext Context =>
        _context ?? throw new InvalidOperationException("The page was shown before its frame navigated to it.");

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        if (ViewModel is null)
        {
            _context = (PaneContext)e.Parameter;
            ViewModel = new SettingsViewModel(
                _context.Model, _context.StartAtSignIn, _context.Updates, _context.Version, _context.Notifications);
            foreach (var title in ViewModel.AtLimitTitles)
            {
                AtLimitChoices.Items.Add(title);
            }

            Bindings.Update();
            // After the bindings, so a failure's words are in place before Narrator is told of them.
            ViewModel.PropertyChanged += OnViewModelChanged;
        }

        ViewModel.Listen();
        ViewModel.Shown();
    }

    // The page keeps where it was scrolled to, and what the model asks for opens it on its section.
    public void Show(SettingsSection section)
    {
        var header = section switch
        {
            SettingsSection.Limits => LimitsHeader,
            SettingsSection.Advanced => AdvancedHeader,
            _ => GeneralHeader,
        };
        if (IsLoaded)
        {
            BringIntoView(header);
            return;
        }

        RoutedEventHandler? once = null;
        once = (_, _) =>
        {
            Loaded -= once;
            BringIntoView(header);
        };
        Loaded += once;
    }

    private static void BringIntoView(TextBlock header) =>
        header.StartBringIntoView(new BringIntoViewOptions { VerticalAlignmentRatio = 0, AnimationDesired = false });

    private void OnShowDiagnostics(object sender, RoutedEventArgs args) => Context.Show(new Place.Checks());

    private void OnShowActivity(object sender, RoutedEventArgs args) => Context.Show(new Place.Activity());

    private void OnViewModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        Announce(StartupFailed, ViewModel!.StartupFailed);
        Announce(ScheduleFailed, ViewModel.ScheduleFailed);
        Announce(UninstallStopped, ViewModel.UninstallStopped);
    }

    // WinUI's live region: Narrator reads the text again only when its peer says it changed.
    private static void Announce(TextBlock text, string? said)
    {
        var changed = said is not null && said != text.Text;
        text.Text = said ?? "";
        text.Visibility = said is null ? Visibility.Collapsed : Visibility.Visible;
        if (changed)
        {
            (FrameworkElementAutomationPeer.FromElement(text) ?? FrameworkElementAutomationPeer.CreatePeerForElement(text))
                .RaiseAutomationEvent(AutomationEvents.LiveRegionChanged);
        }
    }

    private void OnStartAtSignInToggled(object sender, RoutedEventArgs args) =>
        ViewModel?.SetStartsAtSignIn(((ToggleSwitch)sender).IsOn);

    private void OnOpenStartupSettings(object sender, RoutedEventArgs args) =>
        Context.Open(new Uri(SettingsViewModel.StartupSettings));

    private void OnOpenNotificationSettings(object sender, RoutedEventArgs args) =>
        Context.Open(new Uri(SettingsViewModel.NotificationSettings));

    private void OnAtLimitChosen(object sender, SelectionChangedEventArgs args) =>
        ViewModel?.ChooseAtLimit(((ComboBox)sender).SelectedIndex);

    private void OnOpenTerms(object sender, RoutedEventArgs args)
    {
        if (ViewModel is { } shown)
        {
            Context.Open(new Uri(shown.TermsLink));
        }
    }

    // A cleared box shows the share again once its change has run.
    private void OnShareChanged(NumberBox sender, NumberBoxValueChangedEventArgs args)
    {
        if (ViewModel is { } shown && !shown.SetShare(args.NewValue))
        {
            DispatcherQueue.TryEnqueue(Ui.Guarded(() => sender.Value = shown.Share));
        }
    }

    private void OnWarmRangeChanged(TimePicker sender, TimePickerSelectedValueChangedEventArgs args)
    {
        if (args.NewTime is not { } picked)
        {
            return;
        }

        if (sender == WarmFrom)
        {
            ViewModel?.SetWarmFrom(picked);
        }
        else
        {
            ViewModel?.SetWarmTo(picked);
        }
    }

    private void OnWarmToggled(object sender, RoutedEventArgs args)
    {
        if (sender is ToggleSwitch { Tag: string qualified } turned)
        {
            ViewModel?.SetWarm(qualified, turned.IsOn);
        }
    }

    private void OnRenewDailyToggled(object sender, RoutedEventArgs args) => ViewModel?.SetRenewsDaily(((ToggleSwitch)sender).IsOn);

    private void OnAutomaticChecksToggled(object sender, RoutedEventArgs args) =>
        ViewModel?.SetChecksAutomatically(((ToggleSwitch)sender).IsOn);

    private void OnCheckNow(object sender, RoutedEventArgs args) => ViewModel?.CheckNow();

    private async void OnUninstall(object sender, RoutedEventArgs args)
    {
        if (ViewModel is not { } shown || (shown.UninstallQuestion is { } question && !await Context.Dialogs.Confirm(question)))
        {
            return;
        }

        shown.Uninstall();
    }
}
