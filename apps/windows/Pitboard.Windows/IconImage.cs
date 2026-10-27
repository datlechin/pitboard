using System.Runtime.InteropServices;
using Pitboard.Core;

namespace Pitboard.Windows;

// The step of the limit the menu bar names, and whether something needs attention.
public readonly record struct IconLook(UsageLevel? Level, bool Attention);

// Where the notification area draws the icon: at its size, and on a light taskbar or a dark one.
public readonly record struct IconPlace(int Size, bool LightTaskbar)
{
    // Windows' mode colours the taskbar, light where it says nothing as PowerToys reads it, and under a contrast theme the
    // icon takes the theme's text colour, as EarTrumpet's does: dark text is a light taskbar's.
    public static bool Light(int? systemUsesLightTheme, uint? contrastText) =>
        contrastText is { } text ? Luminance(text) < 128 : systemUsesLightTheme is not 0;

    // A COLORREF is 0x00BBGGRR.
    private static int Luminance(uint color) =>
        (int)(((color & 0xFF) * 2126) + (((color >> 8) & 0xFF) * 7152) + (((color >> 16) & 0xFF) * 722)) / 10000;
}

// The icon's image at the size the notification area draws it at, which Explorer would stretch from another, in the ink
// of its taskbar's theme.
public sealed class IconImage
{
    private readonly Func<IconPlace> _place;
    private readonly Func<IconPlace, IconLook, nint> _load;
    private readonly Action<nint> _destroy;
    private IconPlace _loaded;
    private IconLook _look;

    public IconImage(Func<IconPlace> place, Func<IconPlace, IconLook, nint> load, Action<nint> destroy)
    {
        _place = place;
        _load = load;
        _destroy = destroy;
        _loaded = place();
        Handle = load(_loaded, _look);
    }

    public nint Handle { get; private set; }

    public void Follow(Action<nint> show) => Change(_look, show);

    public void Show(IconLook look, Action<nint> show) => Change(look, show);

    // The image before is destroyed only once the new one is shown in its place.
    private void Change(IconLook look, Action<nint> show)
    {
        IconPlace place;
        nint loaded;
        try
        {
            place = _place();
            if (place == _loaded && look == _look)
            {
                return;
            }

            loaded = _load(place, look);
        }
        // ThrowOnFailure raises GetDpiForMonitor's E_INVALIDARG as ArgumentException, most other HRESULTs as COMException.
        catch (Exception failed) when (failed is COMException or ArgumentException)
        {
            return;
        }

        var before = Handle;
        Handle = loaded;
        _loaded = place;
        _look = look;
        show(Handle);
        _destroy(before);
    }
}
