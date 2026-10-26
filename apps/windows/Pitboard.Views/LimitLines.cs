using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Core;
using Windows.Foundation;

namespace Pitboard.Views;

// An account's limits, a line each: its name, a bar, how much is used and when it resets. One grid for all of them, so
// the bars and figures line up whatever each limit is called. WinUI shares no column widths across controls, so the
// names' column takes the widest name of any account on the page, as WPF's SharedSizeGroup would.
public sealed partial class LimitLines : UserControl
{
    // Not an array: XAML's type info would then write setters for LimitRow's init-only properties, which do not compile.
    public static readonly DependencyProperty LimitsProperty = DependencyProperty.Register(
        nameof(Limits), typeof(IEnumerable<LimitRow>), typeof(LimitLines),
        new PropertyMetadata(Array.Empty<LimitRow>(), (lines, _) => ((LimitLines)lines).Show()));

    private readonly Grid _grid = new() { ColumnSpacing = 8, RowSpacing = 4 };
    private readonly List<TextBlock> _names = [];
    private double _namesWidth;
    private Page? _page;

    public LimitLines()
    {
        // A bar as long as the mockup draws it, and shorter where the window is narrow.
        GridLength[] widths = [GridLength.Auto, new GridLength(1, GridUnitType.Star), GridLength.Auto, GridLength.Auto];
        foreach (var width in widths)
        {
            _grid.ColumnDefinitions.Add(new ColumnDefinition { Width = width });
        }

        _grid.ColumnDefinitions[1].MinWidth = 80;
        _grid.ColumnDefinitions[1].MaxWidth = 260;
        Content = _grid;
        Unloaded += (_, _) =>
        {
            var page = _page;
            _page = null;
            _namesWidth = 0;
            Align(page);
        };
    }

    public IEnumerable<LimitRow> Limits
    {
        get => (IEnumerable<LimitRow>)GetValue(LimitsProperty);
        set => SetValue(LimitsProperty, value);
    }

    protected override Size MeasureOverride(Size availableSize)
    {
        var measured = base.MeasureOverride(availableSize);
        var widest = _names.Select(name => name.DesiredSize.Width).DefaultIfEmpty(0).Max();
        _page = Look.Ancestor<Page>(this);
        if (widest != _namesWidth)
        {
            _namesWidth = widest;
            Align(_page);
        }

        return measured;
    }

    private static void Align(Page? page)
    {
        if (page is null)
        {
            return;
        }

        var shown = Look.Descendants<LimitLines>(page).ToList();
        var width = shown.Select(lines => lines._namesWidth).DefaultIfEmpty(0).Max();
        foreach (var lines in shown)
        {
            lines._grid.ColumnDefinitions[0].MinWidth = width;
        }
    }

    private static T Unspoken<T>(T element)
        where T : UIElement
    {
        AutomationProperties.SetAccessibilityView(element, AccessibilityView.Raw);
        return element;
    }

    private static TextBlock Column(string text) =>
        new() { Text = text, TextWrapping = TextWrapping.NoWrap, VerticalAlignment = VerticalAlignment.Center };

    // The accent while there is room, as plan 4.9's mockup draws it, then caution and critical.
    private static ProgressBar Bar(LimitRow limit)
    {
        var bar = new ProgressBar
        {
            Minimum = 0,
            Maximum = 100,
            Value = Math.Clamp(limit.Percent, 0, 100),
            VerticalAlignment = VerticalAlignment.Center,
        };
        if (limit.Level is UsageLevel.Low or UsageLevel.Out)
        {
            bar.Style = (Style)Application.Current.Resources[limit.Level == UsageLevel.Low ? "LowLimitBarStyle" : "OutLimitBarStyle"];
        }

        return Unspoken(bar);
    }

    private void Show()
    {
        _grid.Children.Clear();
        _grid.RowDefinitions.Clear();
        _names.Clear();
        var row = 0;
        foreach (var limit in Limits)
        {
            _grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            var name = Column(limit.Short);
            // Narrator reads the limit once, as a sentence, and not the bar and each figure on their own.
            AutomationProperties.SetName(name, limit.Spoken);
            _names.Add(name);
            Place(name, row, 0);
            Place(Bar(limit), row, 1);
            var figure = Column(limit.Figure);
            figure.MinWidth = 36;
            figure.TextAlignment = TextAlignment.Right;
            Place(Unspoken(figure), row, 2);
            Place(Unspoken(Column(limit.Resets)), row, 3);
            row++;
        }

        Visibility = row > 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    private void Place(FrameworkElement element, int row, int column)
    {
        Grid.SetRow(element, row);
        Grid.SetColumn(element, column);
        _grid.Children.Add(element);
    }
}
