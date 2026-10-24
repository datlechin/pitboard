using System.Runtime.InteropServices;
using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;
using WinRT.Interop;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed partial class WindowChromeTests
{
    private const uint GetIcon = 0x007F;
    private const nuint BigIcon = 1;

    // Without both, a secondary window's title bar stayed light in dark mode, and only the main window had the icon.
    [UITestMethod]
    public void EachWindowFollowsTheAppModeAndCarriesTheIcon()
    {
        var web = WebEnvironment.Fixture(Path.GetTempPath());
        var account = new AccountWindow(Items.Window("codex", "main").Store, web, new Downloads(web, _ => { }), _ => { });
        Window[] windows =
        [
            MainWindowTests.Made(Snapshots.Read(1)),
            account,
            new SignInWindow(new SignInWindowSize(480, 640), account, "Sign in"),
            new OpenLinkWindow(Snapshots.Read(1).AccountWindows.NoLink, _ => { }, _ => { }, _ => { }),
        ];
        try
        {
            foreach (var window in windows)
            {
                var name = window.GetType().Name;
                Assert.AreEqual(TitleBarTheme.UseDefaultAppMode, window.AppWindow.TitleBar.PreferredTheme, name);
                Assert.AreNotEqual(0, SendMessage(WindowNative.GetWindowHandle(window), GetIcon, BigIcon, 0), $"{name} has no icon");
            }
        }
        finally
        {
            // The sign-in window first: closing its account window destroys it, and WinUI's Close then fails fast.
            foreach (var window in Enumerable.Reverse(windows))
            {
                window.Close();
            }
        }
    }

    // The WinUI Gallery's TitleBarHelper: over content extended into the title bar, the caption buttons kept black glyphs
    // on dark after a switch of mode while Pitboard ran.
    [UITestMethod]
    public async Task TheMainWindowsCaptionButtonsFollowItsTheme()
    {
        var window = await Views.Activated(MainWindowTests.Made(Snapshots.Read(1)));
        try
        {
            var root = (FrameworkElement)window.Content;
            var bar = window.AppWindow.TitleBar;
            foreach (var (theme, glyph) in new[] { (ElementTheme.Dark, Colors.White), (ElementTheme.Light, Colors.Black), (ElementTheme.Dark, Colors.White) })
            {
                root.RequestedTheme = theme;

                await Views.Until(() => bar.ButtonForegroundColor == glyph, () => $"the caption's glyphs are {bar.ButtonForegroundColor} in {theme}");
                Assert.AreEqual(glyph, bar.ButtonHoverForegroundColor, $"{theme}");
                Assert.AreEqual(theme == ElementTheme.Dark ? (byte)255 : (byte)0, bar.ButtonHoverBackgroundColor?.R, $"{theme}");
            }
        }
        finally
        {
            window.Close();
        }
    }

    // Microsoft's title bar guidance has the caption follow a contrast theme, whose colours black or white glyphs covered.
    [UITestMethod]
    public async Task UnderAContrastThemeTheCaptionButtonsTakeWindowsOwnColours()
    {
        var contrast = new StandInContrast();
        var root = new Grid { RequestedTheme = ElementTheme.Dark };
        var window = new Window { Content = root, ExtendsContentIntoTitleBar = true };
        WindowChrome.FollowTheme(window, root, contrast);
        await Views.Activated(window);
        try
        {
            var bar = window.AppWindow.TitleBar;
            await Views.Until(() => bar.ButtonForegroundColor == Colors.White, () => $"the caption's glyphs are {bar.ButtonForegroundColor}");

            contrast.Turn(on: true);

            await Views.Until(
                () => bar.ButtonForegroundColor is null, () => $"the caption's glyphs are {bar.ButtonForegroundColor} under a contrast theme");
            Assert.IsNull(bar.ButtonHoverForegroundColor);
            Assert.IsNull(bar.ButtonHoverBackgroundColor);

            contrast.Turn(on: false);

            await Views.Until(() => bar.ButtonForegroundColor == Colors.White, () => $"the caption's glyphs are {bar.ButtonForegroundColor}");
        }
        finally
        {
            window.Close();
        }
    }

    private sealed class StandInContrast : IContrastTheme
    {
        public event EventHandler? Changed;

        public bool Shown { get; private set; }

        public void Turn(bool on)
        {
            Shown = on;
            Changed?.Invoke(this, EventArgs.Empty);
        }
    }

    [LibraryImport("user32.dll", EntryPoint = "SendMessageW")]
    private static partial nint SendMessage(nint window, uint message, nuint wParam, nint lParam);
}
