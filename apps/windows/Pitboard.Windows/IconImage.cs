using System.Runtime.InteropServices;

namespace Pitboard.Windows;

// The icon's image at the size the notification area draws it at, which Explorer would stretch from another.
public sealed class IconImage
{
    private readonly Func<int> _size;
    private readonly Func<int, nint> _load;
    private readonly Action<nint> _destroy;
    private int _loaded;

    public IconImage(Func<int> size, Func<int, nint> load, Action<nint> destroy)
    {
        _size = size;
        _load = load;
        _destroy = destroy;
        _loaded = size();
        Handle = load(_loaded);
    }

    public nint Handle { get; private set; }

    // The image before is destroyed only once the new one is shown in its place.
    public void Follow(Action<nint> show)
    {
        int size;
        nint loaded;
        try
        {
            size = _size();
            if (size == _loaded)
            {
                return;
            }

            loaded = _load(size);
        }
        // ThrowOnFailure raises GetDpiForMonitor's E_INVALIDARG as ArgumentException, most other HRESULTs as COMException.
        catch (Exception failed) when (failed is COMException or ArgumentException)
        {
            return;
        }

        var before = Handle;
        Handle = loaded;
        _loaded = size;
        show(Handle);
        _destroy(before);
    }
}
