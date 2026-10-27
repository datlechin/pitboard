using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;
using Windows.Foundation;
using Windows.Graphics;
using Windows.System;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class MainWindowTests
{
    // Each page, as the model asks for it.
    private static readonly Place[] Places =
        [new Place.Accounts(), new Place.Checks(), new Place.Activity(), new Place.Settings(SettingsSection.General)];

    // A window over its own placement file, which goes with it.
    internal static MainWindow Made(Snapshot snapshot, StartAtSignIn? start = null, Func<NotificationsBlocked?>? notifications = null)
    {
        var placement = Path.Combine(Path.GetTempPath(), $"pitboard-placement-{Guid.NewGuid()}.json");
        var window = new MainWindow(
            new AppModel(new StandInModel(snapshot)), start ?? new StartAtSignIn(new FixtureStartupItem()), new NoUpdates(), "0.0.0",
            _ => { }, new WindowPlacement(placement), notifications);
        window.Closed += (_, _) => File.Delete(placement);
        return window;
    }

    // Something on every page: accounts, a change and a check.
    private static Snapshot Filled()
    {
        var read = Snapshots.Read(1);
        return read with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare"))],
            Machine = read.Machine with
            {
                Activity = new ActivityShown(
                    Lines: [new ActivityLine(0, "15/01/2027 09:00", "Switch", "claude/work", "done", Done: true, AskedBy: "pitboard")],
                    Empty: null),
                Checks = new ChecksShown(
                    Groups: [new CheckGroup("claude", "Claude Code", [new CheckLine(0, "Claude Code", Level.Ok, "Passed", "2.1.299", Advice: null, Action: null, Learn: null)])],
                    Summary: "Everything looks right.", Verdict: null, Level: Level.Ok, Checking: false, Checked: "Checked at 09:00",
                    Waiting: null, Report: null),
            },
        };
    }

    private static Frame Pages(MainWindow window) => Views.Descendants(window.Content).OfType<Frame>().Single();

    private static NavigationView Navigation(MainWindow window) => Views.Element<NavigationView>(window.Content, "navigation");

    private static Type PageOf(Place place) => place switch
    {
        Place.Checks => typeof(DiagnosticsPage),
        Place.Activity => typeof(ActivityPage),
        Place.Settings => typeof(SettingsPage),
        _ => typeof(AccountsPage),
    };

    private static async Task<Page> Showing(MainWindow window, Place place)
    {
        window.Show(place);
        var frame = Pages(window);
        Page? page = null;
        await Views.Until(
            () => (page = frame.Content as Page) is { IsLoaded: true } && page.GetType() == PageOf(place), $"{PageOf(place).Name} did not show");
        page!.UpdateLayout();
        return page;
    }

    private static TextBlock Title(Page page) =>
        Views.Visible<TextBlock>(page).Single(text => AutomationProperties.GetHeadingLevel(text) == AutomationHeadingLevel.Level1);

    // What heads the page: its title, or on one of Settings' own pages the way back to Settings before it.
    private static FrameworkElement Heading(Page page) =>
        Views.Visible<SettingsBreadcrumb>(page).FirstOrDefault() as FrameworkElement ?? Title(page);

    // A list's first row, or what the page's scroll viewer holds.
    private static FrameworkElement Content(Page page) =>
        Views.Visible<ListViewItem>(page).FirstOrDefault() as FrameworkElement
        ?? (FrameworkElement)Views.Descendants(page).OfType<ScrollViewer>().First().Content;

    private static FrameworkElement ScrollHost(Page page) =>
        Views.Visible<ListView>(page).FirstOrDefault() as FrameworkElement ?? Views.Descendants(page).OfType<ScrollViewer>().First();

    private static Rect Bounds(FrameworkElement element, UIElement within) =>
        element.TransformToVisual(within).TransformBounds(new Rect(0, 0, element.ActualWidth, element.ActualHeight));

    // F6, Shift+F6 and Ctrl+W showed "F6" wherever the pointer rested over a page.
    [UITestMethod]
    public async Task NoShortcutShowsAsATooltipOverThePages()
    {
        var window = await Views.Activated(Made(Filled()));
        try
        {
            foreach (var place in Places)
            {
                await Showing(window, place);
                Assert.AreEqual("", string.Join(", ", Views.StrayShortcuts(window.Content)), $"over {PageOf(place).Name}");
            }
        }
        finally
        {
            window.Close();
        }
    }

    // The window had no way to the help but the icon's menu.
    [UITestMethod]
    public async Task TheWindowTakesF1ForTheHelp()
    {
        var window = await Views.Activated(Made(Snapshots.Read(1)));
        try
        {
            var root = (UIElement)window.Content;

            Assert.HasCount(
                1, root.KeyboardAccelerators.Where(accelerator => accelerator is { Key: VirtualKey.F1, Modifiers: VirtualKeyModifiers.None }));
        }
        finally
        {
            window.Close();
        }
    }

    // As the WinUI Gallery's main window keeps.
    [UITestMethod]
    public async Task TheWindowIsNeverSmallerThan640By500()
    {
        var window = await Views.Activated(Made(Snapshots.Read(1)));
        try
        {
            var scale = window.Content.XamlRoot.RasterizationScale;
            var presenter = (OverlappedPresenter)window.AppWindow.Presenter;

            Assert.AreEqual((int)Math.Ceiling(640 * scale), presenter.PreferredMinimumWidth);
            Assert.AreEqual((int)Math.Ceiling(500 * scale), presenter.PreferredMinimumHeight);
        }
        finally
        {
            window.Close();
        }
    }

    // NavigationView's header sat 40 epx right of its pane's content. Its guidance gives content 24 epx sides beside a
    // compact pane.
    [UITestMethod]
    public async Task EachPagesTitleLinesUpWithItsContent()
    {
        var window = await Views.Activated(Made(Filled()));
        try
        {
            // Narrower than the settings' 1064 epx column, so nothing is centred.
            var scale = window.Content.XamlRoot.RasterizationScale;
            window.AppWindow.Resize(new SizeInt32((int)(900 * scale), (int)(700 * scale)));
            Assert.AreEqual(NavigationViewDisplayMode.Compact, Navigation(window).DisplayMode);
            foreach (var place in Places)
            {
                var page = await Showing(window, place);
                var frame = Pages(window);
                double Left(FrameworkElement element) => Bounds(element, frame).Left;
                var heading = Heading(page);
                var content = Content(page);

                await Views.Until(
                    () => Math.Abs(Left(heading) - 24) < 0.5 && Math.Abs(Left(content) - 24) < 0.5,
                    () => $"{page.GetType().Name}: the title at {Left(heading)} and its content at {Left(content)}, not 24");
            }
        }
        finally
        {
            window.Close();
        }
    }

    // A breadcrumb's page title sat 3 to 5 epx below where every other page's does, and the command bars moved with it, so
    // the heading jumped from Settings to one of its pages.
    [UITestMethod]
    public async Task EveryPagesTitleAndCommandBarSitWhereTheOthersDo()
    {
        var window = await Views.Activated(Made(Filled()));
        try
        {
            var frame = Pages(window);
            var titles = new List<Rect>();
            var bars = new List<Rect>();
            foreach (var place in Places)
            {
                var page = await Showing(window, place);
                titles.Add(Bounds(Title(page), frame));
                bars.AddRange(Views.Visible<CommandBar>(page).Take(1).Select(bar => Bounds(bar, frame)));
            }

            var said = string.Join(" | ", titles.Select(at => $"{at.Left},{at.Top}"));
            Assert.IsLessThanOrEqualTo(1.0, titles.Max(at => at.Top) - titles.Min(at => at.Top), $"titles at {said}");
            Assert.IsLessThanOrEqualTo(1.0, titles.Max(at => at.Left) - titles.Min(at => at.Left), $"titles at {said}");
            Assert.HasCount(3, bars);
            Assert.IsLessThanOrEqualTo(1.0, bars.Max(at => at.Top) - bars.Min(at => at.Top), string.Join(" | ", bars.Select(at => at.Top)));
        }
        finally
        {
            window.Close();
        }
    }

    [UITestMethod]
    public async Task EachPagesTitleIsALevel1Heading()
    {
        var window = await Views.Activated(Made(Filled()));
        try
        {
            var titles = new List<string>();
            foreach (var place in Places)
            {
                titles.Add(Title(await Showing(window, place)).Text);
            }

            Assert.AreEqual("Accounts, Diagnostics, Activity, Settings", string.Join(", ", titles));
        }
        finally
        {
            window.Close();
        }
    }

    // Scrolled content went under a title that stayed put, cut off with no room between.
    [UITestMethod]
    public async Task ThePageScrollsBelowItsTitle()
    {
        var window = await Views.Activated(Made(Filled()));
        try
        {
            foreach (var place in Places)
            {
                var page = await Showing(window, place);
                var title = Bounds(Heading(page), page);
                var host = Bounds(ScrollHost(page), page);

                Assert.IsGreaterThanOrEqualTo(title.Bottom + 24, host.Top, $"{page.GetType().Name}: the title ends at {title.Bottom}, its scrolling at {host.Top}");
            }
        }
        finally
        {
            window.Close();
        }
    }

    // As the macOS settings read the login item again once the app is active, back from System Settings.
    [UITestMethod]
    public async Task BackFromWindowsSettingsTheSettingsReadWhatWindowsKeepsAgain()
    {
        var key = new StandInRunKey();
        var item = new RunValue(Build.Release, @"C:\Users\Dana Lee\AppData\Local\Programs\Pitboard\Pitboard.exe", key);
        item.Write(starts: true);
        NotificationsBlocked? blocked = null;
        var window = await Views.Activated(Made(Snapshots.Read(1), new StartAtSignIn(item), () => blocked));
        try
        {
            var page = await Showing(window, new Place.Settings(SettingsSection.General));
            var toggle = Views.Element<ToggleSwitch>(page, "settings.startAtSignIn");
            var notifications = Views.Element<SettingsCard>(page, "settings.notifications");
            Assert.IsTrue(toggle.IsOn);
            Assert.AreEqual(Visibility.Collapsed, notifications.Visibility);

            key.Approvals["Pitboard"] = [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            blocked = new(Words.Settings.NotificationsOff, InSettings: true);
            UnitTestApp.Window!.Activate();
            window.Activate();

            // A card collapsed at load lays out its description only once shown.
            await Views.Until(
                () => !toggle.IsOn && !toggle.IsEnabled && notifications.Visibility == Visibility.Visible
                    && Views.Descendants(page).OfType<HyperlinkButton>().Any(link => AutomationProperties.GetAutomationId(link) == "settings.notificationSettings"),
                "the settings still show what Windows held before the window came back");
            Assert.AreEqual(Visibility.Visible, Views.Element<HyperlinkButton>(page, "settings.startupSettings").Visibility);
            Assert.AreEqual(Visibility.Visible, Views.Element<HyperlinkButton>(page, "settings.notificationSettings").Visibility);
        }
        finally
        {
            window.Close();
        }
    }

    // As NavigationView's guidance navigates: only to a page not shown already, with no back stack, and each page kept.
    [UITestMethod]
    public async Task ChangingPagesNavigatesTheFrameOnce()
    {
        var window = await Views.Activated(Made(Filled()));
        try
        {
            var frame = Pages(window);
            var accounts = frame.Content;
            var navigated = new List<string>();
            void Navigated(object sender, NavigationEventArgs args) => navigated.Add(args.SourcePageType.Name);
            frame.Navigated += Navigated;

            await Showing(window, new Place.Activity());
            await Showing(window, new Place.Activity());
            await Showing(window, new Place.Settings(SettingsSection.Limits));
            await Showing(window, new Place.Accounts());
            frame.Navigated -= Navigated;

            Assert.AreEqual("ActivityPage, SettingsPage, AccountsPage", string.Join(", ", navigated));
            Assert.AreSame(accounts, frame.Content, "the accounts page was made again");
            Assert.IsEmpty(frame.BackStack);
        }
        finally
        {
            window.Close();
        }
    }

    // The window had a pane item each for Activity and This PC, which now are pages of Settings, as Windows Settings
    // keeps Troubleshoot's: Settings' item stays selected over them, and the title bar's back button goes to Settings.
    [UITestMethod]
    public async Task DiagnosticsAndActivityArePagesOfSettingsWithAWayBack()
    {
        var window = await Views.Activated(Made(Filled()));
        try
        {
            var navigation = Navigation(window);
            var titleBar = Views.Descendants(window.Content).OfType<TitleBar>().Single();
            Assert.AreEqual(
                "pane.accounts", string.Join(", ", navigation.MenuItems.OfType<NavigationViewItem>().Select(AutomationProperties.GetAutomationId)));
            Assert.IsFalse(titleBar.IsBackButtonVisible, "a way back from the accounts");

            foreach (var place in new Place[] { new Place.Checks(), new Place.Activity() })
            {
                var page = await Showing(window, place);
                Assert.AreSame(navigation.SettingsItem, navigation.SelectedItem, $"over {page.GetType().Name}");
                Assert.IsTrue(titleBar.IsBackButtonVisible, $"no way back from {page.GetType().Name}");

                // The bar holds an item for its ellipsis too, so Settings' is found by what it says.
                var crumb = Views.Visible<TextBlock>(page).Single(text => text.Text == "Settings");
                new BreadcrumbBarItemAutomationPeer(Look.Ancestor<BreadcrumbBarItem>(crumb)!).Invoke();

                await Views.Until(() => Pages(window).Content is SettingsPage { IsLoaded: true }, $"Settings did not show from {page.GetType().Name}");
                Assert.IsFalse(titleBar.IsBackButtonVisible, "a way back from Settings");
            }
        }
        finally
        {
            window.Close();
        }
    }

    // Windows' own default for a window of its kind, which the mockup draws the accounts at.
    [UITestMethod]
    public async Task ANewWindowOpensAt960By680()
    {
        var window = await Views.Activated(Made(Snapshots.Read(1)));
        try
        {
            var scale = window.Content.XamlRoot.RasterizationScale;

            Assert.AreEqual(new SizeInt32((int)(960 * scale), (int)(680 * scale)), window.AppWindow.Size);
        }
        finally
        {
            window.Close();
        }
    }

    // A title bar 32 epx tall, as Windows' standard one: it holds no search box or person picture.
    [UITestMethod]
    public async Task TheTitleBarIsTheStandardHeight()
    {
        var window = await Views.Activated(Made(Snapshots.Read(1)));
        try
        {
            var titleBar = Views.Descendants(window.Content).OfType<TitleBar>().Single();

            Assert.AreEqual(32, titleBar.ActualHeight, 0.5);
        }
        finally
        {
            window.Close();
        }
    }
}
