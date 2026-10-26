using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Core;
using Pitboard.Windows;

namespace Pitboard.Views;

public sealed partial class AccountRow : UserControl
{
    public static readonly DependencyProperty AccountProperty = DependencyProperty.Register(
        nameof(Account), typeof(ShownAccount), typeof(AccountRow),
        new PropertyMetadata(null, (row, change) => ((AccountRow)row).Follow(change.OldValue as ShownAccount, change.NewValue as ShownAccount)));

    public AccountRow() => InitializeComponent();

    public ShownAccount? Account
    {
        get => (ShownAccount?)GetValue(AccountProperty);
        set => SetValue(AccountProperty, value);
    }

    public static string MarkOf(AccountItem item) =>
        item.Unplaced ? Glyphs.Warning
        : item.NeedsSignIn ? Glyphs.SignIn
        : item.InUse ? Glyphs.InUse
        : Glyphs.Account;

    public Style MarkStyle(bool inUse) => (Style)Resources[inUse ? "InUseMark" : "AccountMark"];

    // Given the item, not its plan: x:Bind skips a function whose argument changes to null, so the tag stayed.
    public static Visibility PlanShown(AccountItem item) =>
        string.IsNullOrEmpty(item.Plan) ? Visibility.Collapsed : Visibility.Visible;

    public static Visibility EmailShown(AccountItem item) =>
        item.EmailLine is null ? Visibility.Collapsed : Visibility.Visible;

    public static string EmailLine(AccountItem item) => item.EmailLine ?? "";

    public static bool Switching(AccountItem item) => item.End is RowEnd.Switching;

    public static Visibility SwitchingShown(AccountItem item) =>
        Switching(item) ? Visibility.Visible : Visibility.Collapsed;

    public static string SwitchingSpoken(AccountItem item) => (item.End as RowEnd.Switching)?.Spoken ?? "";

    public static Visibility InUseShown(AccountItem item) =>
        item.End is RowEnd.InUse ? Visibility.Visible : Visibility.Collapsed;

    public static string InUseTitle(AccountItem item) => (item.End as RowEnd.InUse)?.Title ?? "";

    public static Visibility ActionShown(AccountItem item) =>
        item.End is RowEnd.Button ? Visibility.Visible : Visibility.Collapsed;

    public static string ActionTitle(AccountItem item) => (item.End as RowEnd.Button)?.Action.Title ?? "";

    public static string ActionSpoken(AccountItem item) => (item.End as RowEnd.Button)?.Action.Spoken ?? "";

    private static Style Shared(string key) => (Style)Application.Current.Resources[key];

    private static T Unspoken<T>(T element)
        where T : UIElement
    {
        AutomationProperties.SetAccessibilityView(element, AccessibilityView.Raw);
        return element;
    }

    private void Follow(ShownAccount? before, ShownAccount? now)
    {
        if (before is not null)
        {
            before.PropertyChanged -= OnAccountChanged;
        }

        if (now is not null)
        {
            now.PropertyChanged += OnAccountChanged;
            ShowLists(now.Item);
        }
    }

    private void OnAccountChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (Account is { } account)
        {
            ShowLists(account.Item);
        }
    }

    private void ShowLists(AccountItem item)
    {
        ShowLimits(item.Limits);
        Notes.Children.Clear();
        foreach (var line in item.Lines)
        {
            Notes.Children.Add(
                new TextBlock { Text = line, Style = Shared("SecondaryCaptionTextBlockStyle"), TextWrapping = TextWrapping.Wrap });
        }
    }

    // One grid for every limit, so the bars and figures line up whatever each limit is called. WinUI shares no column
    // widths across a list's items, so the grid is built here.
    private void ShowLimits(LimitRow[] limits)
    {
        Limits.Children.Clear();
        Limits.RowDefinitions.Clear();
        Limits.ColumnDefinitions.Clear();
        Limits.Visibility = limits.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
        GridLength[] widths = [GridLength.Auto, new GridLength(1, GridUnitType.Star), GridLength.Auto, GridLength.Auto];
        foreach (var width in widths)
        {
            Limits.ColumnDefinitions.Add(new ColumnDefinition { Width = width });
        }

        for (var row = 0; row < limits.Length; row++)
        {
            var limit = limits[row];
            Limits.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            var name = Column(limit.Short, "SecondaryCaptionTextBlockStyle");
            // Narrator reads the limit once, as a sentence, and not the bar and each figure on their own.
            AutomationProperties.SetName(name, limit.Spoken);
            Place(name, row, 0);
            Place(Bar(limit), row, 1);
            var figure = Column(limit.Figure, "SecondaryCaptionTextBlockStyle");
            figure.MinWidth = 36;
            figure.TextAlignment = TextAlignment.Right;
            Place(Unspoken(figure), row, 2);
            var resets = Column(limit.Resets, "SecondaryCaptionTextBlockStyle");
            resets.TextAlignment = TextAlignment.Right;
            Place(Unspoken(resets), row, 3);
        }
    }

    private ProgressBar Bar(LimitRow limit) =>
        Unspoken(new ProgressBar
        {
            Minimum = 0,
            Maximum = 100,
            Value = Math.Clamp(limit.Percent, 0, 100),
            MinWidth = 80,
            VerticalAlignment = VerticalAlignment.Center,
            Style = Own(limit.Level switch
            {
                UsageLevel.Low => "Low",
                UsageLevel.Out => "Out",
                _ => "Plenty",
            }),
        });

    private static TextBlock Column(string text, string style) =>
        new() { Text = text, Style = Shared(style), TextWrapping = TextWrapping.NoWrap, VerticalAlignment = VerticalAlignment.Center };

    private void Place(FrameworkElement element, int row, int column)
    {
        Grid.SetRow(element, row);
        Grid.SetColumn(element, column);
        Limits.Children.Add(element);
    }

    private Style Own(string key) => (Style)Resources[key];

    private void OnAction(object sender, RoutedEventArgs args) => Account?.Act();
}
