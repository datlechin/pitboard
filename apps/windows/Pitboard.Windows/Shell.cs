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
