using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media.Animation;
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
        Title = Words.AppName;
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
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
            AppWindow.Resize(new SizeInt32((int)(960 * scale), (int)(680 * scale)));
            WindowChrome.Fit(this);
        }

        _dialogs = new DialogHost(() => Navigation.XamlRoot);
        _context = new PaneContext(model, _dialogs, startAtSignIn, updates, version)
        {
            OpenWindow = openWindow,
            Notifications = notifications ?? (() => null),
            Show = Show,
            Back = ShowSettings,
        };
        // A dialog needs the window's XamlRoot, which its content has once loaded.
        Navigation.Loaded += (_, _) =>
        {
            _sheets ??= new WindowSheets(model, _dialogs);
            _alerts ??= new WindowAlerts(model, _dialogs, _context.Open);
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
            ShowSettings();
        };
        Navigation.KeyboardAccelerators.Add(settings);
        Root.AddHandler(UIElement.PointerPressedEvent, new PointerEventHandler(OnPointerPressed), handledEventsToo: true);
        Show(new Place.Accounts());
    }

    // Diagnostics and Activity are pages of Settings, as Windows Settings keeps Troubleshoot's.
    public void Show(Place place)
    {
        switch (place)
        {
            case Place.Settings { Section: var section }:
                ShowSettings();
                ((SettingsPage)Pages.Content).Show(section);
                break;
            case Place.Checks:
                Navigate(typeof(DiagnosticsPage), new DrillInNavigationTransitionInfo());
                break;
            case Place.Activity:
                Navigate(typeof(ActivityPage), new DrillInNavigationTransitionInfo());
                break;
            default:
                Navigate(typeof(AccountsPage));
                break;
        }
    }

    // As it was left, scrolled where it was.
    public void ShowSettings() => Navigate(typeof(SettingsPage));

    // As NavigationView's guidance navigates its frame: only to a page not shown already. Settings' item stays selected
    // over its own pages.
    private void Navigate(Type page, NavigationTransitionInfo? transition = null)
    {
        Navigation.SelectedItem = page == typeof(AccountsPage) ? AccountsItem : Navigation.SettingsItem;
        // Not CurrentSourcePageType, which did not come back as the page just navigated to (run 40413073866).
        if (Pages.Content?.GetType() != page)
        {
            Pages.Navigate(page, _context, transition);
        }
    }

    // Each click, also on the item already selected, which takes a page of Settings back to Settings.
    private void OnItemInvoked(NavigationView sender, NavigationViewItemInvokedEventArgs args)
    {
        if (args.IsSettingsInvoked)
        {
            ShowSettings();
        }
        else
        {
            Show(new Place.Accounts());
        }
    }

    private void OnNavigated(object sender, NavigationEventArgs args) =>
        AppTitleBar.IsBackButtonVisible = args.SourcePageType == typeof(DiagnosticsPage) || args.SourcePageType == typeof(ActivityPage);

    private void OnBackRequested(TitleBar sender, object args) => ShowSettings();

    // The mouse's back button goes back as Alt+Left does, as Microsoft's back navigation guidance has it.
    private void OnPointerPressed(object sender, PointerRoutedEventArgs args)
    {
        if (AppTitleBar.IsBackButtonVisible && args.GetCurrentPoint(Root).Properties.IsXButton1Pressed)
        {
            args.Handled = true;
            ShowSettings();
        }
    }

    private void OnBack(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        if (AppTitleBar.IsBackButtonVisible)
        {
            args.Handled = true;
            ShowSettings();
        }
    }

    // Two regions, the navigation pane and the page, so Shift+F6's reverse cycle is F6's.
    private void OnNextRegion(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        var focused = FocusManager.GetFocusedElement(Root.XamlRoot) as DependencyObject;
        if (Look.Ancestor<Frame>(focused) != Pages)
        {
            (FocusManager.FindFirstFocusableElement(Pages) as UIElement)?.Focus(FocusState.Keyboard);
        }
        else
        {
            (Navigation.SelectedItem as UIElement)?.Focus(FocusState.Keyboard);
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

    // Ctrl+N adds an account from any page; the model then shows the accounts.
    private void OnAddAccount(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        _model.Send(new Intent.PresentSheet(new Sheet.Add(Tool: null)));
    }
}
