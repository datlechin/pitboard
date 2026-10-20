using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Pitboard.Windows;
using Windows.Graphics;
using WinRT.Interop;

namespace Pitboard.Views;

// What each of Pitboard's windows carries: the app's icon, which WinUI's window class has none of, and a title bar in
// the person's app mode, where it is otherwise light whatever the mode.
internal static class WindowChrome
{
    public static void Apply(Window window)
    {
        window.AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, WindowsNotifications.IconFile));
        window.AppWindow.TitleBar.PreferredTheme = TitleBarTheme.UseDefaultAppMode;
        if (window.ExtendsContentIntoTitleBar && window.Content is FrameworkElement root)
        {
            FollowTheme(window, root, new SystemContrast(window.AppWindow.Id));
        }
    }

    // The WinUI Gallery's TitleBarHelper: over content extended into the title bar, the caption buttons keep the colours
    // of the mode they were first drawn in.
    internal static void FollowTheme(Window window, FrameworkElement root, IContrastTheme contrast)
    {
        var closed = false;
        void Show() => Caption(window.AppWindow.TitleBar, root.ActualTheme, contrast.Shown);
        void Changed(object? sender, EventArgs args) => window.DispatcherQueue.TryEnqueue(Ui.Guarded(() =>
        {
            if (!closed)
            {
                Show();
            }
        }));
        root.Loaded += (_, _) => Show();
        root.ActualThemeChanged += (_, _) => Show();
        contrast.Changed += Changed;
        window.Closed += (_, _) =>
        {
            closed = true;
            contrast.Changed -= Changed;
        };
    }

    // Under a contrast theme the system's own caption colours are the theme's.
    private static void Caption(AppWindowTitleBar bar, ElementTheme theme, bool contrast)
    {
        if (contrast)
        {
            bar.ButtonForegroundColor = null;
            bar.ButtonHoverForegroundColor = null;
            bar.ButtonHoverBackgroundColor = null;
            return;
        }

        var dark = theme == ElementTheme.Dark;
        var glyph = dark ? Colors.White : Colors.Black;
        bar.ButtonForegroundColor = glyph;
        bar.ButtonHoverForegroundColor = glyph;
        bar.ButtonHoverBackgroundColor = dark ? ColorHelper.FromArgb(24, 255, 255, 255) : ColorHelper.FromArgb(24, 0, 0, 0);
    }

    // In effective pixels, at the scale of the screen the window is on, and again when it moves to one of another.
    public static void MinimumSize(Window window, double width, double height)
    {
        var shown = 0.0;
        void At(double scale)
        {
            if (scale != shown && window.AppWindow.Presenter is OverlappedPresenter presenter)
            {
                shown = scale;
                presenter.PreferredMinimumWidth = (int)Math.Ceiling(width * scale);
                presenter.PreferredMinimumHeight = (int)Math.Ceiling(height * scale);
            }
        }

        At(Shell.Scale(WindowNative.GetWindowHandle(window)));
        if (window.Content is FrameworkElement root)
        {
            root.Loaded += (_, _) =>
            {
                At(root.XamlRoot.RasterizationScale);
                root.XamlRoot.Changed += (changed, _) => At(changed.RasterizationScale);
            };
        }
    }

    // Whole within the work area of the screen it is nearest, moved and if need be made smaller, as Microsoft's window
    // management guidance asks of a window's first place.
    public static void Fit(Window window)
    {
        var area = DisplayArea.GetFromWindowId(window.AppWindow.Id, DisplayAreaFallback.Nearest).WorkArea;
        var at = window.AppWindow.Position;
        var size = window.AppWindow.Size;
        var width = Math.Min(size.Width, area.Width);
        var height = Math.Min(size.Height, area.Height);
        var fitted = new RectInt32(
            Math.Clamp(at.X, area.X, area.X + area.Width - width), Math.Clamp(at.Y, area.Y, area.Y + area.Height - height),
            width, height);
        if (fitted.X != at.X || fitted.Y != at.Y || width != size.Width || height != size.Height)
        {
            window.AppWindow.MoveAndResize(fitted);
        }
    }
}
