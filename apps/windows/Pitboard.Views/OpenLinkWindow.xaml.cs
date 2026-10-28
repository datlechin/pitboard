using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.Graphics;
using Windows.System;
using WinRT.Interop;

namespace Pitboard.Views;

public sealed partial class OpenLinkWindow : Window
{
    private const double ContentWidth = 440;

    private readonly EmptyList _noLink;
    private readonly Action<Intent> _send;
    private readonly Action<WindowAccount> _open;
    private readonly Action<string> _inBrowser;
    private LinkPicker? _picker;
    private Action? _primary;
    private Action? _secondary;

    public OpenLinkWindow(EmptyList noLink, Action<Intent> send, Action<WindowAccount> open, Action<string> inBrowser)
    {
        _noLink = noLink;
        _send = send;
        _open = open;
        _inBrowser = inBrowser;
        InitializeComponent();
        Title = Words.Window.OpenLink;
        AccountList.ContainerContentChanging += (_, args) => Name(args);
        AppWindow.SetPresenter(OverlappedPresenter.CreateForDialog());
        WindowChrome.Apply(this);
        Closed += (_, _) =>
        {
            if (_picker is { } picker)
            {
                _send(new Intent.DismissLink(picker.Arrival));
            }
        };
        Show(null, fresh: true);
    }

    // The link it shows, which the model names each link by.
    public ulong? Arrival => _picker?.Arrival;

    public void Follow(LinkPicker? picker)
    {
        if (picker is not null && _picker is not null && picker.Arrival == _picker.Arrival
            && Structural.Equal(picker.Shown, _picker.Shown))
        {
            _picker = picker;
            if (picker.Shown is PickerShown.Choose)
            {
                PrimaryButton.IsEnabled = picker.Armed;
            }

            return;
        }

        var fresh = picker?.Arrival != _picker?.Arrival;
        _picker = picker;
        Show(picker, fresh);
        Fit(center: false);
    }

    // Closed once no link waits, so closing dismisses nothing.
    public void Withdraw()
    {
        _picker = null;
        Close();
    }

    private void Show(LinkPicker? picker, bool fresh)
    {
        var shown = picker?.Shown;
        Unavailable.Visibility = Look.Shown(shown is null);
        Waiting.Visibility = Look.Shown(shown is PickerShown.Reading);
        WaitingRing.IsActive = shown is PickerShown.Reading;
        Heading.Visibility = Look.Shown(shown is not (null or PickerShown.Reading));
        if (shown is not null)
        {
            AutomationProperties.SetAutomationId(Heading, $"picker.{Identifiers.Of(shown)}");
        }

        LinkLine.Visibility = Look.Shown(shown is PickerShown.NoWindows or PickerShown.NoAccount or PickerShown.Choose);
        Explanation.Visibility = Look.Shown(
            shown is PickerShown.ReadFailed or PickerShown.Refused or PickerShown.NoWindows or PickerShown.NoAccount);
        AccountList.Visibility = Look.Shown(shown is PickerShown.Choose);
        CancelButton.Visibility = Look.Shown(shown is not (null or PickerShown.Refused));
        Primary(null, null);
        Secondary(null, null);
        switch (shown)
        {
            case null:
                Unavailable.Title = _noLink.Title;
                Unavailable.Detail = _noLink.Detail;
                Primary(Words.Window.Close, Close);
                break;
            case PickerShown.Reading reading:
                WaitingText.Text = reading.Title;
                break;
            case PickerShown.ReadFailed failed:
                Heading.Text = failed.Title;
                Explanation.Text = failed.Detail;
                Primary(failed.Retry.Title, () => _send(failed.Retry.Intent), failed.Retry.Enabled);
                break;
            case PickerShown.Refused refused:
                Heading.Text = refused.Title;
                Explanation.Text = refused.Reason;
                Primary(Words.Window.Ok, Cancel);
                break;
            // Neither shipping app meets it: each has account windows.
            case PickerShown.NoWindows browser:
                Heading.Text = browser.Title;
                ShowLink(browser.LinkText, browser.Link.Url);
                Explanation.Text = browser.Detail;
                Primary(browser.InBrowser, () =>
                {
                    _inBrowser(browser.Link.Url);
                    Cancel();
                });
                break;
            case PickerShown.NoAccount none:
                Heading.Text = none.Title;
                ShowLink(none.LinkText, none.Link.Url);
                Explanation.Text = none.Detail;
                Primary(none.Add.Title, () => _send(none.Add.Intent), none.Add.Enabled);
                Secondary(none.InBrowser, () =>
                {
                    _inBrowser(none.Link.Url);
                    Cancel();
                });
                break;
            case PickerShown.Choose choose:
                Heading.Text = choose.Title;
                ShowLink(choose.LinkText, choose.Link.Url);
                ShowAccounts(choose, fresh);
                Primary(choose.Open, OpenChosen, picker!.Armed);
                break;
        }
    }

    private void Primary(string? title, Action? chosen, bool enabled = true)
    {
        _primary = chosen;
        PrimaryButton.Content = title;
        PrimaryButton.IsEnabled = enabled;
        PrimaryButton.Visibility = Look.Shown(chosen is not null);
    }

    private void Secondary(string? title, Action? chosen)
    {
        _secondary = chosen;
        SecondaryButton.Content = title;
        SecondaryButton.Visibility = Look.Shown(chosen is not null);
    }

    private void ShowLink(string text, string url)
    {
        LinkLine.Text = text;
        ToolTipService.SetToolTip(LinkLine, url);
        AutomationProperties.SetHelpText(LinkLine, url);
    }

    // The account selected stays while the same link waits; a link that arrives again starts from the model's choice.
    private void ShowAccounts(PickerShown.Choose choose, bool fresh)
    {
        var kept = fresh ? null : (AccountList.SelectedItem as PickerAccount)?.Window.Store;
        AccountList.ItemsSource = choose.Accounts;
        AccountList.SelectedItem = Offered(choose, kept) ?? Offered(choose, choose.Chosen) ?? choose.Accounts.FirstOrDefault();
    }

    private static PickerAccount? Offered(PickerShown.Choose choose, string? store) =>
        store is null
            ? null
            : choose.Accounts.FirstOrDefault(account => string.Equals(account.Window.Store, store, StringComparison.OrdinalIgnoreCase));

    // Only once Open answers, and only as an account the window offers, as the model takes it.
    private void OpenChosen()
    {
        if (_picker is not { Armed: true, Shown: PickerShown.Choose choose } picker
            || Offered(choose, (AccountList.SelectedItem as PickerAccount)?.Window.Store) is not { } account)
        {
            return;
        }

        _send(new Intent.OpenLink(picker.Arrival, account.Window.Store));
        _open(account.Window);
    }

    // The model answers with no link waiting, and the window is withdrawn.
    private void Cancel()
    {
        if (_picker is { } picker)
        {
            _send(new Intent.DismissLink(picker.Arrival));
        }
        else
        {
            Close();
        }
    }

    private void OnPrimary(object sender, RoutedEventArgs args) => _primary?.Invoke();

    private void OnSecondary(object sender, RoutedEventArgs args) => _secondary?.Invoke();

    private void OnCancel(object sender, RoutedEventArgs args) => Cancel();

    private void OnEscape(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        Cancel();
    }

    // One element for Narrator, its label and its line read together, found by UI Automation by its label.
    private static void Name(ContainerContentChangingEventArgs args)
    {
        if (!args.InRecycleQueue && args.Item is PickerAccount account)
        {
            AutomationProperties.SetAutomationId(args.ItemContainer, $"picker.account.{account.Window.Label}");
            AutomationProperties.SetName(args.ItemContainer, $"{account.Window.Label}, {account.Detail}");
        }
    }

    private void OnAccountDoubleTapped(object sender, DoubleTappedRoutedEventArgs args) => OpenChosen();

    private void OnAccountKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (args.Key == VirtualKey.Enter)
        {
            args.Handled = true;
            OpenChosen();
        }
    }

    private void OnLoaded(object sender, RoutedEventArgs args) => Fit(center: true);

    // As tall as what it shows, at its screen's scale.
    private void Fit(bool center)
    {
        if (!Root.IsLoaded)
        {
            return;
        }

        Root.Measure(new global::Windows.Foundation.Size(ContentWidth, double.PositiveInfinity));
        var scale = Shell.Scale(WindowNative.GetWindowHandle(this));
        AppWindow.ResizeClient(new SizeInt32((int)Math.Ceiling(ContentWidth * scale), (int)Math.Ceiling(Root.DesiredSize.Height * scale)));
        if (center)
        {
            var area = DisplayArea.GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary).WorkArea;
            AppWindow.Move(new PointInt32(
                area.X + ((area.Width - AppWindow.Size.Width) / 2), area.Y + ((area.Height - AppWindow.Size.Height) / 2)));
        }
    }
}
