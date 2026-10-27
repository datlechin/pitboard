using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.Foundation;
using Windows.UI.ViewManagement;

namespace Pitboard.Views;

public sealed partial class ActivityPage : Page
{
    private const int AccountColumn = 2;

    private readonly UISettings _settings = new();
    private double[] _widths = [];
    private PaneContext? _context;

    public ActivityPage()
    {
        InitializeComponent();
        Lines.ContainerContentChanging += ShowLine;
        Loaded += (_, _) =>
        {
            ViewModel?.Listen();
            _settings.TextScaleFactorChanged += OnTextScaleFactorChanged;
            Fit();
        };
        Unloaded += (_, _) =>
        {
            ViewModel?.Stop();
            _settings.TextScaleFactorChanged -= OnTextScaleFactorChanged;
        };
    }

    public ActivityViewModel? ViewModel { get; private set; }

    private PaneContext Context =>
        _context ?? throw new InvalidOperationException("The page was shown before its frame navigated to it.");

    // A change that went as asked is said quietly, and one that did not stands out.
    public static Style ResultStyle(bool done) =>
        (Style)Application.Current.Resources[done ? "SecondaryTextBlockStyle" : "BodyTextBlockStyle"];

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        if (ViewModel is null)
        {
            _context = (PaneContext)e.Parameter;
            ViewModel = new ActivityViewModel(_context.Model);
            ViewModel.PropertyChanged += (_, _) => Fit();
            Bindings.Update();
        }

        ViewModel.Listen();
        ViewModel.Read();
    }

    // Each cell as the table's template shows it, in the columns' order.
    private static (string Text, Style? Style)[] Cells(ActivityLine line) =>
    [
        (line.Date, null), (line.Change, null), (line.Account, null), (line.Result, ResultStyle(line.Done)),
        (line.AskedBy, (Style)Application.Current.Resources["SecondaryTextBlockStyle"]),
    ];

    // Narrator reads a change as one line, in the columns' order.
    private void ShowLine(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        if (!args.InRecycleQueue && args.Item is ActivityLine line)
        {
            AutomationProperties.SetName(
                args.ItemContainer, string.Join(", ", line.Date, line.Change, line.Account, line.Result, line.AskedBy));
            if (args.ItemContainer.ContentTemplateRoot is Grid row)
            {
                Apply(row);
            }
        }
    }

    // Each column as wide as its widest cell or its header, with the account taking the rest, so a change and its result
    // read on one line in any language and at any text size. Where the page is narrower than all of them, each column
    // gives up room as its width weighs, and wraps.
    private void Fit()
    {
        if (!IsLoaded || ViewModel is null)
        {
            return;
        }

        var measured = new Dictionary<(string, Style?), double>();
        double WidthOf(string text, Style? style)
        {
            if (!measured.TryGetValue((text, style), out var width))
            {
                if (style is null)
                {
                    Ruler.ClearValue(StyleProperty);
                }
                else
                {
                    Ruler.Style = style;
                }

                Ruler.Text = text;
                Ruler.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
                width = measured[(text, style)] = Ruler.DesiredSize.Width;
            }

            return width;
        }

        var widths = Columns.Children.OfType<TextBlock>()
            .OrderBy(title => Grid.GetColumn(title))
            .Select(title => WidthOf(title.Text, title.Style))
            .ToArray();
        foreach (var cells in ViewModel.Lines.Select(Cells))
        {
            for (var column = 0; column < widths.Length; column++)
            {
                widths[column] = Math.Max(widths[column], WidthOf(cells[column].Text, cells[column].Style));
            }
        }

        Ruler.Text = "";
        // Layout rounding at 125% and 150% can take back part of a pixel, which would wrap the widest cell.
        _widths = [.. widths.Select(width => Math.Ceiling(width) + 1)];
        Apply(Columns);
        foreach (var row in Lines.ItemsPanelRoot?.Children.OfType<ListViewItem>().Select(item => item.ContentTemplateRoot).OfType<Grid>() ?? [])
        {
            Apply(row);
        }
    }

    // A stacked line has two columns, and keeps them.
    private void Apply(Grid table)
    {
        if (table.ColumnDefinitions.Count != _widths.Length)
        {
            return;
        }

        for (var column = 0; column < _widths.Length; column++)
        {
            var definition = table.ColumnDefinitions[column];
            definition.Width = new GridLength(_widths[column], GridUnitType.Star);
            definition.MaxWidth = column == AccountColumn ? double.PositiveInfinity : _widths[column];
        }
    }

    // Settings > Accessibility > Text size, raised on a thread of its own.
    private void OnTextScaleFactorChanged(UISettings sender, object args) => DispatcherQueue.TryEnqueue(Ui.Guarded(Fit));

    private void OnBack(object sender, RoutedEventArgs args) => Context.Back();

    private void OnRefresh(object sender, RoutedEventArgs args) => ViewModel?.Read();
}
