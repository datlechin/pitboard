using Pitboard.Core;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.Graphics.Gdi;
using Windows.Win32.System.Recovery;
using Windows.Win32.UI.HiDpi;
using Windows.Win32.UI.Shell;
using Windows.Win32.UI.WindowsAndMessaging;

namespace Pitboard.Windows;

public static class Shell
{
    // Before any window, so each one groups under it on the taskbar.
    public static void Name(Identity identity) =>
        PInvoke.SetCurrentProcessExplicitAppUserModelID(identity.Aumid).ThrowOnFailure();

    // An upgrade's Restart Manager starts Pitboard again once it replaced it, and nothing else does.
    public static void RestartAfterUpdate(Build build)
    {
        if (build is Build.Release)
        {
            PInvoke.RegisterApplicationRestart(
                Launching.Updated,
                REGISTER_APPLICATION_RESTART_FLAGS.RESTART_NO_CRASH | REGISTER_APPLICATION_RESTART_FLAGS.RESTART_NO_HANG
                    | REGISTER_APPLICATION_RESTART_FLAGS.RESTART_NO_REBOOT).ThrowOnFailure();
        }
    }

    // Environment.GetFolderPath falls back to LOCALAPPDATA where this fails, and no variable is to move it.
    public static string LocalAppData() => KnownFolder(PInvoke.FOLDERID_LocalAppData);

    public static unsafe string KnownFolder(Guid id)
    {
        PInvoke.SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG.KF_FLAG_DONT_VERIFY, null, out var path).ThrowOnFailure();
        try
        {
            return path.ToString();
        }
        finally
        {
            PInvoke.CoTaskMemFree(path);
        }
    }

    // Where the person's downloads go, wherever they moved the folder.
    public static string Downloads() => KnownFolder(PInvoke.FOLDERID_Downloads);

    // The .NET SDK numbers an ApplicationIcon 32512, IDI_APPLICATION's id.
    public static unsafe nint AppIcon(int size)
    {
        using var module = PInvoke.GetModuleHandle((string?)null);
        HICON icon;
        PInvoke.LoadIconWithScaleDown((HINSTANCE)module.DangerousGetHandle(), (char*)32512, size, size, &icon).ThrowOnFailure();
        return (nint)icon.Value;
    }

    public static void DestroyIcon(nint icon) => PInvoke.DestroyIcon((HICON)icon);

    // Tray holds a drawing for each level, and None for no reading, beside Pitboard.exe, which ../Icon/make-ico.py writes.
    public static unsafe nint NotificationIcon(int size, IconLook look)
    {
        var drawn = Path.Combine(AppContext.BaseDirectory, "Tray", $"{look.Level?.ToString() ?? "None"}.ico");
        HANDLE loaded;
        fixed (char* file = drawn)
        {
            loaded = PInvoke.LoadImage(
                HINSTANCE.Null, file, GDI_IMAGE_TYPE.IMAGE_ICON, size, size, IMAGE_FLAGS.LR_LOADFROMFILE);
        }

        var icon = loaded.IsNull ? AppIcon(size) : (nint)loaded.Value;
        if (!look.Attention || Warned(icon, size) is not { } warned)
        {
            return icon;
        }

        DestroyIcon(icon);
        return warned;
    }

    // Windows' own warning icon over the bottom right, as Microsoft's notification area guidance overlays a status.
    private static unsafe nint? Warned(nint icon, int size)
    {
        var stock = new SHSTOCKICONINFO { cbSize = (uint)sizeof(SHSTOCKICONINFO) };
        if (PInvoke.SHGetStockIconInfo(SHSTOCKICONID.SIID_WARNING, SHGSI_FLAGS.SHGSI_ICONLOCATION, ref stock).Failed)
        {
            return null;
        }

        var side = size * 5 / 8;
        HICON warning;
        HRESULT extracted;
        fixed (char* file = stock.szPath.ToString())
        {
            extracted = PInvoke.SHDefExtractIcon(file, stock.iIcon, 0, &warning, null, (uint)side);
        }

        // S_FALSE where the file has no such icon.
        if (extracted.Value != 0)
        {
            return null;
        }

        var header = new BITMAPINFO
        {
            bmiHeader = new BITMAPINFOHEADER
            {
                biSize = (uint)sizeof(BITMAPINFOHEADER),
                biWidth = size,
                biHeight = -size,
                biPlanes = 1,
                biBitCount = 32,
            },
        };
        void* bits;
        var color = PInvoke.CreateDIBSection(HDC.Null, &header, DIB_USAGE.DIB_RGB_COLORS, &bits, HANDLE.Null, 0);
        var mask = PInvoke.CreateBitmap(size, size, 1, 1, null);
        var drawing = PInvoke.CreateCompatibleDC(HDC.Null);
        try
        {
            // Past GDI's limits, drawing would write through a null pointer, which .NET cannot catch.
            if (color.IsNull || mask.IsNull || drawing.IsNull || bits is null)
            {
                return null;
            }

            var before = PInvoke.SelectObject(drawing, color);
            PInvoke.DrawIconEx(drawing, 0, 0, (HICON)icon, size, size, 0, HBRUSH.Null, DI_FLAGS.DI_NORMAL);
            PInvoke.DrawIconEx(drawing, size - side, size - side, warning, side, side, 0, HBRUSH.Null, DI_FLAGS.DI_NORMAL);
            PInvoke.SelectObject(drawing, before);
            Straighten(new Span<uint>(bits, size * size));
            var made = new ICONINFO { fIcon = true, hbmColor = color, hbmMask = mask };
            var warned = PInvoke.CreateIconIndirect(&made);
            return warned.IsNull ? null : (nint)warned.Value;
        }
        finally
        {
            PInvoke.DeleteDC(drawing);
            PInvoke.DeleteObject(color);
            PInvoke.DeleteObject(mask);
            PInvoke.DestroyIcon(warning);
        }
    }

    // DrawIconEx blends into the bitmap premultiplied, and an icon's pixels are straight.
    private static void Straighten(Span<uint> pixels)
    {
        foreach (ref var pixel in pixels)
        {
            var alpha = pixel >> 24;
            if (alpha is 0 or 255)
            {
                continue;
            }

            var blended = pixel;
            uint Channel(int shift) => Math.Min(255, (((blended >> shift) & 0xFF) * 255 + alpha / 2) / alpha) << shift;
            pixel = (alpha << 24) | Channel(16) | Channel(8) | Channel(0);
        }
    }

    // At the primary display's scale, where the notification area is: GetSystemMetrics answers at the session's first one.
    public static int NotificationIconSize()
    {
        var primary = PInvoke.MonitorFromPoint(default, MONITOR_FROM_FLAGS.MONITOR_DEFAULTTOPRIMARY);
        PInvoke.GetDpiForMonitor(primary, MONITOR_DPI_TYPE.MDT_EFFECTIVE_DPI, out var dpi, out _).ThrowOnFailure();
        return PInvoke.GetSystemMetricsForDpi(SYSTEM_METRICS_INDEX.SM_CXSMICON, dpi);
    }

    // Allowed while the icon's click has handed this process the foreground.
    public static void BringForward(nint window) => PInvoke.SetForegroundWindow((HWND)window);

    // A window's effective pixels to its physical ones, at its monitor's scale.
    public static double Scale(nint window) => PInvoke.GetDpiForWindow((HWND)window) / 96.0;

    public static bool InFront(nint window) => PInvoke.GetForegroundWindow() == (HWND)window;

    // An owned window stays above its owner, hides while it is minimized and goes with it. WinUI has no API for an owner.
    public static void Own(nint window, nint owner) =>
        PInvoke.SetWindowLongPtr((HWND)window, WINDOW_LONG_PTR_INDEX.GWLP_HWNDPARENT, owner);

    public static void Refuse(string message) =>
        PInvoke.MessageBox(HWND.Null, message, "Pitboard", MESSAGEBOX_STYLE.MB_OK | MESSAGEBOX_STYLE.MB_ICONERROR);
}
