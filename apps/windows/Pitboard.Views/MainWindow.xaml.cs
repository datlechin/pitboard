using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Navigation;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.Graphics;
using Windows.System;
using WinRT.Interop;

namespace Pitboard.Views;

public sealed partial class MainWindow : Window
{
    private readonly AppModel _model;
    private readonly DialogHost _dialogs;
    private readonly PaneContext _context;
    private WindowAlerts? _alerts;
    private WindowSheets? _sheets;

    public MainWindow(
        AppModel model, StartAtSignIn startAtSignIn, IUpdates updates, string version, Action<WindowAccount> openWindow,
        WindowPlacement placement, Func<NotificationsBlocked?>? notifications = null)
    {
        _model = model;
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        AppWindow.TitleBar.PreferredHeightOption = TitleBarHeightOption.Tall;
        WindowChrome.Apply(this);
        WindowChrome.MinimumSize(this, 640, 500);
        // Where it was closed, partly under the taskbar too, or at Windows' own place for a new application window.
        var handle = WindowNative.GetWindowHandle(this);
        var saved = placement.Saved();
        if (saved is not null)
        {
            WindowPlacement.Restore(handle, saved);
        }
        else
        {
            var scale = Shell.Scale(handle);
            AppWindow.Resize(new SizeInt32((int)(1100 * scale), (int)(720 * scale)));
            WindowChrome.Fit(this);
        }

        _dialogs = new DialogHost(() => Navigation.XamlRoot);
        _context = new PaneContext(model, _dialogs, startAtSignIn, updates, version)
        {
            OpenWindow = openWindow,
            Notifications = notifications ?? (() => null),
            ShowSettings = () => Select(null),
        };
        // A dialog needs the window's XamlRoot, which its content has once loaded.
        Navigation.Loaded += (_, _) =>
        {
            _sheets ??= new WindowSheets(model, _dialogs);
            _alerts ??= new WindowAlerts(model, _dialogs);
        };
        Closed += (_, _) =>
        {
            if (WindowPlacement.Of(handle) is { } closed)
            {
                placement.Save(closed);
            }

            _sheets?.Dispose();
            _alerts?.Dispose();
        };
        // Back from installing a tool the sheet said Pitboard could not find, or from a setting Windows keeps.
        Activated += (_, args) =>
        {
            if (args.WindowActivationState == WindowActivationState.Deactivated)
            {
                return;
            }

            if (_model.Sheet is not null)
            {
                _model.Send(new Intent.LookForTools());
            }

            if (Pages.Content is SettingsPage { ViewModel: { } settings })
            {
                settings.ReadWindowsSettings();
            }
        };
        // VK_OEM_COMMA, which VirtualKey has no name for.
        var settings = new KeyboardAccelerator { Key = (VirtualKey)0xBC, Modifiers = VirtualKeyModifiers.Control };
        settings.Invoked += (_, args) =>
        {
            args.Handled = true;
            Select(null);
        };
        Navigation.KeyboardAccelerators.Add(settings);
        Select(Pane.Accounts);
    }

    // Null while the settings page is shown.
    public Pane? Selected =>
        Navigation.SelectedItem is NavigationViewItem { Tag: string tag } && Enum.TryParse<Pane>(tag, out var pane)
            ? pane
            : null;

    public void Select(Pane? pane) =>
        Navigation.SelectedItem = pane is { } shown
            ? Navigation.MenuItems.OfType<NavigationViewItem>().First(item => item.Tag as string == shown.ToString())
            : Navigation.SettingsItem;

    public void ShowLimits()
    {
        if (Pages.Content is SettingsPage page)
        {
            page.ShowLimits();
        }
    }

    // As NavigationView's own guidance navigates its frame: once, and only to a page not shown already.
    private void OnSelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        var page = Selected switch
        {
            Pane.Accounts => typeof(AccountsPage),
            Pane.Activity => typeof(ActivityPage),
            Pane.Checks => typeof(MachinePage),
            _ => typeof(SettingsPage),
        };
        if (Pages.CurrentSourcePageType != page)
        {
            Pages.Navigate(page, _context, args.RecommendedNavigationTransitionInfo);
        }
    }

    private void OnNavigated(object sender, NavigationEventArgs args) => ShowSides(Navigation.DisplayMode);

    private void OnDisplayModeChanged(NavigationView sender, NavigationViewDisplayModeChangedEventArgs args) =>
        ShowSides(args.DisplayMode);

    // Each page's sides: NavigationView's guidance gives its content 12 epx in Minimal and 24 otherwise.
    private void ShowSides(NavigationViewDisplayMode mode)
    {
        if (Pages.Content is Page page)
        {
            VisualStateManager.GoToState(page, mode == NavigationViewDisplayMode.Minimal ? "Minimal" : "Wide", useTransitions: false);
        }
    }

    private void OnPaneToggleRequested(TitleBar sender, object args) => Navigation.IsPaneOpen = !Navigation.IsPaneOpen;

    // Two regions, the navigation pane and the page, so Shift+F6's reverse cycle is F6's. A pane Minimal keeps closed is
    // reached through the title bar's toggle.
    private void OnNextRegion(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        var focused = FocusManager.GetFocusedElement(Root.XamlRoot) as DependencyObject;
        if (Look.Ancestor<Frame>(focused) != Pages)
        {
            (FocusManager.FindFirstFocusableElement(Pages) as UIElement)?.Focus(FocusState.Keyboard);
        }
        else if (Navigation.SelectedItem is not UIElement selected || !selected.Focus(FocusState.Keyboard))
        {
            (FocusManager.FindFirstFocusableElement(AppTitleBar) as UIElement)?.Focus(FocusState.Keyboard);
        }
    }

    // As the caption's close does: Pitboard goes on running.
    private void OnClose(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        Close();
    }

    private void OnHelp(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        _context.Open(new Uri(IconMenu.HelpLink));
    }

    // Ctrl+N adds an account from any pane; the model then shows the accounts.
    private void OnAddAccount(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        _model.Send(new Intent.PresentSheet(new Sheet.Add(Provider: null)));
    }
}
