using System.ComponentModel;
using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Navigation;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.System;
using Windows.UI.Core;

namespace Pitboard.Views;

public sealed partial class AccountsPage : Page
{
    private PaneContext? _context;
    private SetupStep? _setup;
    private bool _putDown;

    public AccountsPage()
    {
        InitializeComponent();
        // A tip's popup stays over the window, so it goes with the page and comes back with it.
        Loaded += (_, _) =>
        {
            ViewModel?.Listen();
            if (_context is { } context)
            {
                context.Dialogs.OpenChanged -= OnDialogChanged;
                context.Dialogs.OpenChanged += OnDialogChanged;
            }

            _putDown = false;
            ShowTip();
        };
        Unloaded += (_, _) =>
        {
            ViewModel?.Stop();
            if (_context is { } context)
            {
                context.Dialogs.OpenChanged -= OnDialogChanged;
            }

            Tip.IsOpen = false;
        };
        // WinUI's TeachingTip undoes an IsOpen change made while it animates, so a tip that should be down is closed again.
        Tip.RegisterPropertyChangedCallback(TeachingTip.IsOpenProperty, (_, _) =>
        {
            if (Tip.IsOpen && !TipWanted)
            {
                Tip.IsOpen = false;
            }
        });
    }

    public AccountsViewModel? ViewModel { get; private set; }

    private PaneContext Context =>
        _context ?? throw new InvalidOperationException("The page was shown before its frame navigated to it.");

    // Its popup draws over a dialog's smoke and a notice's message, so it waits for both to go, and it stays down for the
    // visit once its choice is made.
    private bool TipWanted =>
        _setup is not null && IsLoaded && !_putDown && Notices.Children.Count == 0 && _context?.Dialogs.Open is null;

    public static string EmptyGlyph(EmptyKind kind) => kind switch
    {
        EmptyKind.NoTool => Glyphs.Terminal,
        EmptyKind.ReadFailed => Glyphs.Problem,
        EmptyKind.NoAccounts => Glyphs.Accounts,
        _ => "",
    };

    public static bool Busy(EmptyKind kind) => kind == EmptyKind.Reading;

    public static string SetupId(Footing footing) => $"setup.{Identifiers.Of(footing)}";

    public static string SetupGlyph(Footing footing) => footing switch
    {
        Footing.AtLimitUnchosen => Glyphs.Limits,
        _ => Glyphs.Switch,
    };

    // Each function below is given the whole item: x:Bind calls a function only for an argument that is not null, so a
    // part an account lacks would keep what it showed for the one before.
    public static string RowId(AccountItem item) => $"account.{item.Qualified ?? item.Id}";

    public static string MarkOf(AccountItem item) =>
        item.Unplaced ? Glyphs.Warning
        : item.InUse ? Glyphs.InUse
        : Glyphs.Account;

    public static Style MarkStyle(AccountItem item) => Shared(item.InUse ? "InUseMarkStyle" : "AccountMarkStyle");

    public static string PlanOf(AccountItem item) => item.Plan ?? "";

    // Its most pressing note, the one line a row has room for.
    public static string NoteOf(AccountItem item) => item.Notes.FirstOrDefault()?.Text ?? "";

    public static Visibility NoteShown(AccountItem item) => Look.Shown(item.Notes.Length > 0);

    public static string NoteLearnTitle(AccountItem item) => item.Notes.FirstOrDefault()?.Learn?.Title ?? "";

    public static Visibility NoteLearnShown(AccountItem item) => Look.Shown(item.Notes.FirstOrDefault()?.Learn is not null);

    public static bool Switching(AccountItem item) => item.End is RowEnd.Switching;

    public static Visibility SwitchingShown(AccountItem item) => Look.Shown(Switching(item));

    public static string SwitchingSpoken(AccountItem item) => (item.End as RowEnd.Switching)?.Spoken ?? "";

    public static Visibility InUseShown(AccountItem item) => Look.Shown(item.End is RowEnd.InUse);

    public static string InUseTitle(AccountItem item) => (item.End as RowEnd.InUse)?.Title ?? "";

    public static Visibility ActionShown(AccountItem item) => Look.Shown(item.End is RowEnd.Button);

    public static string ActionTitle(AccountItem item) => (item.End as RowEnd.Button)?.Action.Title ?? "";

    public static string ActionSpoken(AccountItem item) => (item.End as RowEnd.Button)?.Action.Spoken ?? "";

    public static string EmailOf(AccountItem item) => item.Detail.Email ?? "";

    public static Visibility EmailShown(AccountItem item) => Look.Shown(item.Detail.Email is not null);

    public static string SignInOf(AccountItem item) => item.Detail.SignIn ?? "";

    public static Visibility SignInShown(AccountItem item) => Look.Shown(item.Detail.SignIn is not null);

    public static string WarmOf(AccountItem item) => item.Detail.Warm?.Said ?? "";

    public static Visibility WarmShown(AccountItem item) => Look.Shown(item.Detail.Warm is not null);

    public static string WarmSettingsTitle(AccountItem item) => item.Detail.Warm?.Settings.Title ?? "";

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        if (ViewModel is null)
        {
            _context = (PaneContext)e.Parameter;
            ViewModel = new AccountsViewModel(_context.Model, _context.Open);
            ViewModel.PropertyChanged += OnViewModelChanged;
            Bindings.Update();
        }

        ViewModel.Listen();
        ViewModel.Shown();
    }

    private static Style Shared(string key) => (Style)Application.Current.Resources[key];

    // The account whose row holds what was clicked, pressed or asked for a menu.
    private static ShownAccount? AccountAt(object? source) =>
        Look.Ancestor<SettingsExpander>(source as DependencyObject)?.DataContext as ShownAccount;

    private void OnViewModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        var all = string.IsNullOrEmpty(args.PropertyName);
        if (all || args.PropertyName == nameof(AccountsViewModel.Notices))
        {
            ShowNotices();
        }

        if (all || args.PropertyName == nameof(AccountsViewModel.Setup))
        {
            ShowSetup();
        }
    }

    // None takes no room, where an empty panel would keep the gap above the accounts.
    private void ShowNotices()
    {
        Keyed.Sync<PanelNotice, UIElement>(
            Notices.Children, ViewModel!.Notices, notice => notice.Id, shown => ((NoticeBar)shown).Key,
            notice => new NoticeBar(notice, action => Ask(action.Confirm, action.Intent), Context.Open, Context.Time),
            (shown, notice) => ((NoticeBar)shown).Show(notice));
        Notices.Visibility = Look.Shown(Notices.Children.Count > 0);
        ShowTip();
    }

    // The first choice is the one the step is for, and the second turns it down. Left of Add account where the step is to
    // add one, clear of each row's button below it; about the settings, it points at nothing on this page.
    private void ShowSetup()
    {
        var step = ViewModel!.Setup;
        if (Structural.Equal(step, _setup))
        {
            return;
        }

        _setup = step;
        if (step is not null)
        {
            Tip.Title = step.Title;
            Tip.Subtitle = step.Detail;
            Tip.ActionButtonContent = step.Actions.ElementAtOrDefault(0)?.Title;
            Tip.CloseButtonContent = step.Actions.ElementAtOrDefault(1)?.Title;
            Tip.Target = ViewModel.SetupFooting is Footing.AtLimitUnchosen ? null : AddAccount;
            Tip.PreferredPlacement = Tip.Target is null ? TeachingTipPlacementMode.Auto : TeachingTipPlacementMode.LeftBottom;
        }

        _putDown = false;
        ShowTip();
    }

    private void ShowTip() => Tip.IsOpen = TipWanted;

    private void OnDialogChanged(object? sender, EventArgs args) => ShowTip();

    private void OnTipChosen(TeachingTip sender, object args)
    {
        if (_setup?.Actions.ElementAtOrDefault(0) is { } choice)
        {
            _putDown = true;
            sender.IsOpen = false;
            ViewModel!.Send(choice.Intent);
        }
    }

    private void OnTipDeclined(TeachingTip sender, object args)
    {
        if (_setup?.Actions.ElementAtOrDefault(1) is { } choice)
        {
            ViewModel!.Send(choice.Intent);
        }
    }

    // Right click, the menu key, Shift+F10 and a long press all ask here.
    private void OnContextRequested(UIElement sender, ContextRequestedEventArgs args)
    {
        if (Look.Ancestor<SettingsExpander>(args.OriginalSource as DependencyObject) is not { DataContext: ShownAccount shown } row)
        {
            return;
        }

        var menu = AccountMenu.For(shown.Item, Ask, Context.Copy, Context.OpenWindow, ViewModel!.HeldWindows);
        if (args.TryGetPosition(row, out var point))
        {
            menu.ShowAt(row, point);
        }
        else
        {
            menu.ShowAt(row);
        }

        args.Handled = true;
    }

    // Enter on a row's own toggle switches to it, where Space opens it; a button in the row takes Enter for itself.
    private void OnAccountKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (AccountAt(args.OriginalSource) is not { } shown)
        {
            return;
        }

        var control = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down);
        if (AccountKeys.Copied(args.Key, control, shown.Item) is { } email)
        {
            args.Handled = true;
            Context.Copy(email);
        }
        else if ((args.Key != VirtualKey.Enter || args.OriginalSource is ToggleButton)
            && AccountKeys.Pressed(args.Key, shown.Item) is var (confirm, intent))
        {
            args.Handled = true;
            Ask(confirm, intent);
        }
    }

    private async void Ask(Question? question, Intent intent)
    {
        if (question is null || await Context.Dialogs.Confirm(question))
        {
            ViewModel!.Send(intent);
        }
    }

    private void OnAction(object sender, RoutedEventArgs args) => ((sender as FrameworkElement)?.Tag as ShownAccount)?.Act();

    private void OnCopyEmail(object sender, RoutedEventArgs args)
    {
        if ((sender as FrameworkElement)?.Tag is ShownAccount { Item.Detail.Email: { } email })
        {
            Context.Copy(email);
        }
    }

    private void OnWarmUpSettings(object sender, RoutedEventArgs args)
    {
        if ((sender as FrameworkElement)?.Tag is ShownAccount { Item.Detail.Warm.Settings: { } settings })
        {
            ViewModel!.Send(settings.Intent);
        }
    }

    private void OnNoteLearn(object sender, RoutedEventArgs args)
    {
        if ((sender as FrameworkElement)?.Tag is ShownAccount { Item.Notes: [{ Learn: { } learn }, ..] })
        {
            Context.Open(new Uri(learn.Link));
        }
    }

    private void OnMoreNoteLearn(object sender, RoutedEventArgs args)
    {
        if ((sender as FrameworkElement)?.Tag is ShownNote { Learn: { } learn })
        {
            Context.Open(new Uri(learn.Link));
        }
    }

    // An account's window opens here; everything else is the model's, after any question it asks first.
    private void OnCommand(object sender, RoutedEventArgs args)
    {
        if ((sender as FrameworkElement)?.DataContext is not ShownCommand { Action: var action })
        {
            return;
        }

        if (action.Window is { } window)
        {
            Context.OpenWindow?.Invoke(window);
        }
        else if (action.Intent is { } intent)
        {
            Ask(action.Confirm, intent);
        }
    }

    private void OnRefresh(object sender, RoutedEventArgs args) => ViewModel?.Refresh();

    private void OnAdd(object sender, RoutedEventArgs args) => ViewModel?.Add();

    private void OnEmptyChosen(object sender, RoutedEventArgs args) => ViewModel?.ChooseEmpty();
}
