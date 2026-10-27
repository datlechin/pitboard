using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Windows;

namespace Pitboard.Views;

public sealed record Crumb(string Title, AutomationHeadingLevel Level);

// What heads one of Settings' own pages, as Windows Settings heads Troubleshoot's: Settings, the way back, then the page.
public sealed partial class SettingsBreadcrumb : UserControl
{
    public static readonly DependencyProperty TitleProperty = DependencyProperty.Register(
        nameof(Title), typeof(string), typeof(SettingsBreadcrumb),
        new PropertyMetadata("", (shown, _) => ((SettingsBreadcrumb)shown).ShowTitle()));

    public SettingsBreadcrumb()
    {
        InitializeComponent();
        ShowTitle();
    }

    public event RoutedEventHandler? Back;

    public string Title
    {
        get => (string)GetValue(TitleProperty);
        set => SetValue(TitleProperty, value);
    }

    private void ShowTitle() =>
        Crumbs.ItemsSource = new[] { new Crumb(Words.Settings.Title, AutomationHeadingLevel.None), new Crumb(Title, AutomationHeadingLevel.Level1) };

    private void OnItemClicked(BreadcrumbBar sender, BreadcrumbBarItemClickedEventArgs args)
    {
        if (args.Index == 0)
        {
            Back?.Invoke(this, new RoutedEventArgs());
        }
    }
}
