using System.ComponentModel;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Navigation;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.System;
using Windows.UI.Core;

namespace Pitboard.Views;

public sealed partial class AccountsPage : Page
{
    private readonly CollectionViewSource _grouped = new() { IsSourceGrouped = true };
    private PaneContext? _context;
    private SetupStep? _setup;

    public AccountsPage()
    {
        InitializeComponent();
        // An item's own Enter and Delete come here too, after the list has looked at them.
        AccountList.AddHandler(KeyDownEvent, new KeyEventHandler(OnListKeyDown), handledEventsToo: true);
        AccountList.ContainerContentChanging += NameRow;
        Loaded += (_, _) => ViewModel?.Listen();
        Unloaded += (_, _) => ViewModel?.Stop();
    }

    public AccountsViewModel? ViewModel { get; private set; }

    private PaneContext Context =>
        _context ?? throw new InvalidOperationException("The page was shown before its frame navigated to it.");

    public static string EmptyGlyph(EmptyKind kind) => kind switch
    {
        EmptyKind.NoTool => Glyphs.Terminal,
        EmptyKind.ReadFailed => Glyphs.Problem,
        EmptyKind.NoAccounts => Glyphs.Accounts,
        _ => "",
    };

    public static bool Busy(EmptyKind kind) => kind == EmptyKind.Reading;

    public static string SetupGlyph(Footing footing) => footing switch
    {
        Footing.Unnamed => Glyphs.Name,
        Footing.NotOpenedAtLogin => Glyphs.Power,
        Footing.AtLimitUnchosen => Glyphs.Limits,
        _ => Glyphs.Switch,
    };

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        if (ViewModel is null)
        {
            _context = (PaneContext)e.Parameter;
            ViewModel = new AccountsViewModel(_context.Model, _context.Open);
            ViewModel.PropertyChanged += OnViewModelChanged;
            _grouped.Source = ViewModel.Sections;
            AccountList.ItemsSource = ViewModel.Rows;
            Bindings.Update();
        }

        ViewModel.Listen();
        ViewModel.Shown();
    }

    private void OnViewModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        var all = string.IsNullOrEmpty(args.PropertyName);
        if (all || args.PropertyName == nameof(AccountsViewModel.Grouped))
        {
            AccountList.ItemsSource = ViewModel!.Grouped ? _grouped.View : ViewModel.Rows;
        }

        if (all || args.PropertyName == nameof(AccountsViewModel.Notices))
        {
            ShowNotices();
        }

        if (all || args.PropertyName == nameof(AccountsViewModel.Setup))
        {
            ShowSetupChoices();
        }

        if (all || args.PropertyName == nameof(AccountsViewModel.Sections))
        {
            NameRows();
        }
    }

    private void ShowNotices() =>
        Keyed.Sync<PanelNotice, UIElement>(
            Notices.Children, ViewModel!.Notices, notice => notice.Id, shown => ((NoticeBar)shown).Key,
            notice => new NoticeBar(notice, action => Ask(action.Confirm, action.Intent), Context.Time),
            (shown, notice) => ((NoticeBar)shown).Show(notice));

    // The first choice is the one the step is for.
    private void ShowSetupChoices()
    {
        if (ViewModel!.Setup is not { } step || Structural.Equal(step, _setup))
        {
            return;
        }

        _setup = step;
        SetupChoices.Children.Clear();
        for (var index = 0; index < step.Actions.Length; index++)
        {
            var choice = step.Actions[index];
            var button = new Button { Content = choice.Title, IsEnabled = choice.Enabled };
            if (index == 0)
            {
                button.Style = (Style)Application.Current.Resources["AccentButtonStyle"];
            }

            button.Click += (_, _) => Choose(choice.Intent);
            SetupChoices.Children.Add(button);
        }

        if (step.Link is { } link)
        {
            var opens = new HyperlinkButton { Content = link.Title };
            opens.Click += (_, _) => ViewModel.OpenSetupLink();
            SetupChoices.Children.Add(opens);
        }
    }

    // Starting at sign-in is Windows': the app turns it on, and says what Windows has.
    private void Choose(Intent intent)
    {
        if (intent is Intent.OpensAtLogin opens)
        {
            Context.StartAtSignIn.Set(opens.On);
        }
        else
        {
            ViewModel!.Send(intent);
        }
    }

    // A renamed or newly named account keeps its row and its container, which no ContainerContentChanging names again.
    private void NameRows()
    {
        IEnumerable<ShownAccount> rows = ViewModel!.Grouped ? ViewModel.Sections.SelectMany(section => section) : ViewModel.Rows;
        foreach (var row in rows)
        {
            if (AccountList.ContainerFromItem(row) is ListViewItem container)
            {
                AutomationProperties.SetAutomationId(container, RowId(row));
            }
        }
    }

    private static string RowId(ShownAccount shown) => $"account.{shown.Item.Qualified ?? shown.Item.Id}";

    private static void NameRow(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        if (!args.InRecycleQueue && args.Item is ShownAccount shown)
        {
            AutomationProperties.SetAutomationId(args.ItemContainer, RowId(shown));
        }
    }

    // Right click, the menu key, Shift+F10 and a long press all ask here.
    private void OnContextRequested(UIElement sender, ContextRequestedEventArgs args)
    {
        if (Look.Ancestor<ListViewItem>(args.OriginalSource as DependencyObject) is not { Content: ShownAccount shown } item)
        {
            return;
        }

        var menu = AccountMenu.For(shown.Item, Ask, Context.OpenWindow, ViewModel!.HeldWindows);
        if (args.TryGetPosition(item, out var point))
        {
            menu.ShowAt(item, point);
        }
        else
        {
            menu.ShowAt(item);
        }

        args.Handled = true;
    }

    private void OnDoubleTapped(object sender, DoubleTappedRoutedEventArgs args)
    {
        var source = args.OriginalSource as DependencyObject;
        if (Look.Ancestor<ButtonBase>(source) is null
            && Look.Ancestor<ListViewItem>(source) is { Content: ShownAccount { Item.Action: not null } shown })
        {
            args.Handled = true;
            shown.Act();
        }
    }

    private void OnListKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (args.OriginalSource is not ListViewItem { Content: ShownAccount shown })
        {
            return;
        }

        var control = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down);
        if (AccountKeys.Copied(args.Key, control, shown.Item) is { } email)
        {
            args.Handled = true;
            Context.Copy(email);
        }
        else if (AccountKeys.Pressed(args.Key, shown.Item) is var (confirm, intent))
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

    private void OnRefresh(object sender, RoutedEventArgs args) => ViewModel?.Refresh();

    private void OnAdd(object sender, RoutedEventArgs args) => ViewModel?.Add();

    private void OnEmptyChosen(object sender, RoutedEventArgs args) => ViewModel?.ChooseEmpty();
}
