using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Views;
using Windows.Graphics;
using WinRT.Interop;

namespace Pitboard.Windows.Tests;

// The icon's menu in the window it opens from, as the notification-area icon opens it.
[TestClass]
public sealed class MenuHostTests
{
    private static readonly IconMenuEntry[] Entries =
    [
        new IconMenuEntry.Command("Settings", new IconCommand.ShowSettings()) { Id = "menu.settings" },
        new IconMenuEntry.Separator(),
        new IconMenuEntry.Command("Exit", new IconCommand.Quit()) { Id = "menu.exit" },
    ];

    [UITestMethod]
    public async Task OpeningTheMenuIsSaidOnce()
    {
        var up = Up.Over();
        try
        {
            await up.Opened();

            Assert.AreEqual(1, up.Glanced);
        }
        finally
        {
            up.Host.Close();
        }
    }

    // As TrackPopupMenu's caller does: the choice runs once the menu is gone, so a window it opens is not under it.
    [UITestMethod]
    public async Task AChoiceRunsAfterTheMenuCloses()
    {
        var up = Up.Over();
        try
        {
            await up.Opened();
            var menu = up.Host.Menu!;
            var exit = menu.Items.OfType<MenuFlyoutItem>().Single(item => AutomationProperties.GetAutomationId(item) == "menu.exit");

            new MenuFlyoutItemAutomationPeer(exit).Invoke();

            await Views.Until(() => up.Chosen.Count > 0, "nothing was chosen");
            Assert.AreEqual("Quit after the menu closed", string.Join(", ", up.Chosen));
            Assert.IsFalse(up.Cancelled, "a choice cancelled");
        }
        finally
        {
            up.Host.Close();
        }
    }

    // Esc leaves the menu's window in front, so focus goes back where it was; a click elsewhere has moved it already.
    [UITestMethod]
    public async Task AMenuClosedWithNoChoiceGivesFocusBack()
    {
        var up = Up.Over();
        try
        {
            await up.Opened();
            if (!Shell.InFront(WindowNative.GetWindowHandle(up.Host)))
            {
                Assert.Inconclusive("the menu's window could not come to the front here");
            }

            up.Host.Menu!.Hide();

            await Views.Until(() => up.Cancelled, "focus was not given back");
            Assert.IsEmpty(up.Chosen);
        }
        finally
        {
            up.Host.Close();
        }
    }

    // The menu closes after its window has: its handler ran on a window with no AppWindow and ended the process.
    [UITestMethod]
    public async Task ClosingTheWindowUnderAnOpenMenuRunsNothing()
    {
        var up = Up.Over();
        await up.Opened();
        var menu = up.Host.Menu!;

        up.Host.Close();
        await Views.Until(() => !menu.IsOpen, "the menu stayed open");
        await Views.Drained();

        Assert.IsEmpty(up.Chosen);
        Assert.IsFalse(up.Cancelled, "a closed window gave focus back");
    }

    private sealed class Up
    {
        private Up(MenuHost host) => Host = host;

        public MenuHost Host { get; }

        public int Glanced { get; private set; }

        public List<string> Chosen { get; } = [];

        public bool Cancelled { get; private set; }

        public static Up Over()
        {
            var up = new Up(new MenuHost());
            up.Host.Show(
                Entries, new PointInt32(200, 200), () => up.Glanced++,
                command => up.Chosen.Add($"{command.GetType().Name} after the menu {(up.Host.Menu is null ? "closed" : "was open")}"),
                () => up.Cancelled = true);
            return up;
        }

        public Task Opened() => Views.Until(() => Glanced > 0 && Host.Menu is { IsOpen: true }, "the menu did not open");
    }
}
