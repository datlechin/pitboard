using System.ComponentModel;
using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Navigation;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.System;
using Windows.UI.Core;

namespace Pitboard.Views;

public sealed partial class DiagnosticsPage : Page
{
    private readonly MenuFlyout _detailMenu;
    private PaneContext? _context;

    public DiagnosticsPage()
    {
        InitializeComponent();
        _detailMenu = (MenuFlyout)Resources["DetailMenu"];
        Loaded += (_, _) => ViewModel?.Listen();
        Unloaded += (_, _) => ViewModel?.Stop();
    }

    public MachineViewModel? ViewModel { get; private set; }

    private PaneContext Context =>
        _context ?? throw new InvalidOperationException("The page was shown before its frame navigated to it.");

    public static InfoBarSeverity Severity(Level level) => level switch
    {
        Level.Fail => InfoBarSeverity.Error,
        Level.Warn => InfoBarSeverity.Warning,
        _ => InfoBarSeverity.Success,
    };

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        if (ViewModel is null)
        {
            _context = (PaneContext)e.Parameter;
            ViewModel = new MachineViewModel(_context.Model);
            ViewModel.PropertyChanged += OnViewModelChanged;
            Bindings.Update();
        }

        ViewModel.Listen();
        ViewModel.Check();
    }

    private static Style Shared(string key) => (Style)Application.Current.Resources[key];

    // Its level as a shape as well as a colour, which Narrator reads as a word.
    private static FontIcon Mark(Level level, string spoken)
    {
        var mark = new FontIcon
        {
            Name = "LevelMark",
            Glyph = level switch
            {
                Level.Ok => Glyphs.Passed,
                Level.Warn => Glyphs.Warning,
                _ => Glyphs.Problem,
            },
            Style = Shared(level switch
            {
                Level.Ok => "PassedMarkStyle",
                Level.Warn => "WarningMarkStyle",
                _ => "FailedMarkStyle",
            }),
            IsTextScaleFactorEnabled = false,
        };
        AutomationProperties.SetName(mark, spoken);
        return mark;
    }

    private void OnViewModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (string.IsNullOrEmpty(args.PropertyName) || args.PropertyName == nameof(MachineViewModel.Groups))
        {
            ShowGroups();
        }
    }

    // A check is a card: what it looked at, what it found, and its fix in the app or where the docs explain it.
    private void ShowGroups()
    {
        Groups.Children.Clear();
        foreach (var group in ViewModel!.Groups)
        {
            Groups.Children.Add(new TextBlock { Text = group.Heading, Style = Shared("SettingsSectionHeaderTextBlockStyle") });
            foreach (var line in group.Lines)
            {
                Groups.Children.Add(Card(line));
            }
        }
    }

    private SettingsCard Card(CheckLine line)
    {
        var detail = new TextBlock
        {
            Text = PathText.Shown(line.Detail),
            TextWrapping = TextWrapping.Wrap,
            IsTextSelectionEnabled = true,
            ContextFlyout = _detailMenu,
            SelectionFlyout = null,
        };
        detail.PreviewKeyDown += OnDetailKeyDown;
        var description = new StackPanel { Spacing = 2 };
        description.Children.Add(detail);
        if (line.Advice is { } advice)
        {
            description.Children.Add(new TextBlock { Text = PathText.Shown(advice), TextWrapping = TextWrapping.Wrap });
        }

        var card = new SettingsCard { Header = line.Name, Description = description, HeaderIcon = Mark(line.Level, line.SpokenLevel) };
        if (line.Action is { } fix)
        {
            var button = new Button { Content = fix.Title, IsEnabled = fix.Enabled };
            AutomationProperties.SetAutomationId(button, $"check.action.{Identifiers.Of(fix.Intent)}");
            button.Click += (_, _) => Ask(fix.Confirm, fix.Intent);
            card.Content = button;
        }
        else if (line.Learn is { } learn)
        {
            var link = new HyperlinkButton { Content = learn.Title };
            link.Click += (_, _) => Context.Open(new Uri(learn.Link));
            card.Content = link;
        }

        AutomationProperties.SetAutomationId(card, $"check.{line.Id}");
        // Narrator reads a check as one line: its level, its name, what it found and what to do.
        AutomationProperties.SetName(
            card, string.Join(", ", new[] { line.SpokenLevel, line.Name, line.Detail, line.Advice }.OfType<string>()));
        return card;
    }

    private async void Ask(Question? question, Intent intent)
    {
        if (question is null || await Context.Dialogs.Confirm(question))
        {
            Context.Model.Send(intent);
        }
    }

    private void OnBack(object sender, RoutedEventArgs args) => Context.Back();

    private void OnCheckAgain(object sender, RoutedEventArgs args) => ViewModel?.Check();

    private void OnCopyReport(object sender, RoutedEventArgs args)
    {
        if (ViewModel?.Report is { } report)
        {
            Context.Copy(report);
        }
    }

    private void OnDetailKeyDown(object sender, KeyRoutedEventArgs args)
    {
        var control = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down);
        if (control && args.Key is (VirtualKey.C or VirtualKey.Insert) && sender is TextBlock { SelectedText.Length: > 0 } detail)
        {
            args.Handled = true;
            Context.Copy(PathText.Copied(detail.SelectedText));
        }
    }

    private void OnDetailMenuOpening(object sender, object args) =>
        _detailMenu.Items[0].IsEnabled = _detailMenu.Target is TextBlock { SelectedText.Length: > 0 };

    private void OnCopyDetail(object sender, RoutedEventArgs args)
    {
        if (_detailMenu.Target is TextBlock detail)
        {
            Context.Copy(PathText.Copied(detail.SelectedText));
        }
    }

    private void OnSelectAllDetail(object sender, RoutedEventArgs args) => (_detailMenu.Target as TextBlock)?.SelectAll();
}
