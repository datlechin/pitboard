using System.ComponentModel;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.Graphics;
using WinRT.Interop;

namespace Pitboard.Views;

public sealed partial class WelcomeWindow : Window
{
    private const double ContentWidth = 600;

    private readonly AppModel _model;
    private readonly StartAtSignIn _start;
    private readonly Action<Uri> _open;

    public WelcomeWindow(AppModel model, StartAtSignIn start, Action<Uri> open)
    {
        _model = model;
        _start = start;
        _open = open;
        InitializeComponent();
        AppWindow.SetPresenter(OverlappedPresenter.CreateForDialog());
        WindowChrome.Apply(this);
        model.PropertyChanged += OnModelChanged;
        start.Changed += OnStartChanged;
        Closed += (_, _) =>
        {
            model.PropertyChanged -= OnModelChanged;
            start.Changed -= OnStartChanged;
        };
        Show();
    }

    private WelcomeShown Welcome => _model.Welcome;

    private void OnModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (args.PropertyName == nameof(AppModel.Welcome))
        {
            Show();
        }
    }

    private void OnStartChanged(object? sender, EventArgs args) => ShowLogin();

    private void Show()
    {
        var welcome = Welcome;
        Title = welcome.Title;
        Heading.Text = welcome.Title;
        Says.Text = welcome.Lines.ElementAtOrDefault(0) ?? "";
        IconCard.Header = welcome.Lines.ElementAtOrDefault(1) ?? "";
        IconLink.Content = welcome.IconLink?.Title;
        IconLink.Visibility = Look.Shown(welcome.IconLink is not null);
        ToolsHeading.Text = welcome.ToolsHeading ?? "";
        ToolsHeading.Visibility = Look.Shown(welcome.ToolsHeading is not null && welcome.Tools.Length > 0);
        if (!Structural.Equal(Tools.ItemsSource as WelcomeTool[], welcome.Tools))
        {
            Tools.ItemsSource = welcome.Tools;
        }

        Done.Content = welcome.Done.Title;
        Done.IsEnabled = welcome.Done.Enabled;
        ShowLogin();
    }

    private void ShowLogin()
    {
        var login = Welcome.OpenAtLogin;
        LoginCard.Header = login.Title;
        LoginCard.Description = _start.Note(login.Note);
        AutomationProperties.SetName(StartsAtSignIn, login.Title);
        StartsAtSignIn.IsOn = _start.On;
        StartsAtSignIn.IsEnabled = _start.CanTurnOn;
    }

    public static string ToolId(string code) => $"welcome.tool.{code}";

    public static Visibility AddedShown(WelcomeStep step) => Look.Shown(step is WelcomeStep.Added);

    public static string AddedSaid(WelcomeStep step) => (step as WelcomeStep.Added)?.Said ?? "";

    public static Visibility AddShown(WelcomeStep step) => Look.Shown(step is WelcomeStep.Add);

    public static string AddTitle(WelcomeStep step) => (step as WelcomeStep.Add)?.Choice.Title ?? "";

    public static bool AddEnabled(WelcomeStep step) => step is WelcomeStep.Add { Choice.Enabled: true };

    public static Visibility InstallShown(WelcomeStep step) => Look.Shown(step is WelcomeStep.Install);

    public static string InstallTitle(WelcomeStep step) => (step as WelcomeStep.Install)?.Link.Title ?? "";

    // The click is what lets Pitboard read the tool's login.
    private void OnAdd(object sender, RoutedEventArgs args)
    {
        if ((sender as FrameworkElement)?.Tag is WelcomeTool { Step: WelcomeStep.Add { Choice: var choice } })
        {
            _model.Send(choice.Intent);
        }
    }

    private void OnInstall(object sender, RoutedEventArgs args)
    {
        if ((sender as FrameworkElement)?.Tag is WelcomeTool { Step: WelcomeStep.Install { Link: var link } })
        {
            _open(new Uri(link.Link));
        }
    }

    private void OnIconLink(object sender, RoutedEventArgs args)
    {
        if (Welcome.IconLink is { } link)
        {
            _open(new Uri(link.Link));
        }
    }

    private void OnStartToggled(object sender, RoutedEventArgs args)
    {
        if (StartsAtSignIn.IsOn != _start.On)
        {
            _start.Set(StartsAtSignIn.IsOn);
        }
    }

    private void OnDone(object sender, RoutedEventArgs args)
    {
        _model.Send(Welcome.Done.Intent);
        Close();
    }

    private void OnEscape(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        Close();
    }

    // As tall as what it shows, within its screen, centred on it. A window has no default button, so the focus starts
    // on Get started, the accent one, for Enter to press.
    private void OnLoaded(object sender, RoutedEventArgs args)
    {
        Root.Measure(new global::Windows.Foundation.Size(ContentWidth, double.PositiveInfinity));
        var scale = Shell.Scale(WindowNative.GetWindowHandle(this));
        AppWindow.ResizeClient(new SizeInt32((int)Math.Ceiling(ContentWidth * scale), (int)Math.Ceiling(Root.DesiredSize.Height * scale)));
        var area = DisplayArea.GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary).WorkArea;
        AppWindow.Move(new PointInt32(
            area.X + ((area.Width - AppWindow.Size.Width) / 2), area.Y + ((area.Height - AppWindow.Size.Height) / 2)));
        WindowChrome.Fit(this);
        Done.Focus(FocusState.Programmatic);
    }
}
