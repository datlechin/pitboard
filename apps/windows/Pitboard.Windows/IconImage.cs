using System.Runtime.InteropServices;
using Pitboard.Core;

namespace Pitboard.Windows;

// The step of the limit the menu bar names, and whether its menu opens on something to look at.
public readonly record struct IconLook(UsageLevel? Level, bool Attention);

// The icon's image at the size the notification area draws it at, which Explorer would stretch from another.
public sealed class IconImage
{
    private readonly Func<int> _size;
    private readonly Func<int, IconLook, nint> _load;
    private readonly Action<nint> _destroy;
    private int _loaded;
    private IconLook _look;

    public IconImage(Func<int> size, Func<int, IconLook, nint> load, Action<nint> destroy)
    {
        _size = size;
        _load = load;
        _destroy = destroy;
        _loaded = size();
        Handle = load(_loaded, _look);
    }

    public nint Handle { get; private set; }

    public void Follow(Action<nint> show) => Change(_look, show);

    public void Show(IconLook look, Action<nint> show) => Change(look, show);

    // The image before is destroyed only once the new one is shown in its place.
    private void Change(IconLook look, Action<nint> show)
    {
        int size;
        nint loaded;
        try
        {
            size = _size();
            if (size == _loaded && look == _look)
            {
                return;
            }

            loaded = _load(size, look);
        }
        // ThrowOnFailure raises GetDpiForMonitor's E_INVALIDARG as ArgumentException, most other HRESULTs as COMException.
        catch (Exception failed) when (failed is COMException or ArgumentException)
        {
            return;
        }

        var before = Handle;
        Handle = loaded;
        _loaded = size;
        _look = look;
        show(Handle);
        _destroy(before);
    }
}
