using System.Diagnostics;
using System.Runtime.InteropServices;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Tools;

namespace Pitboard.UITests;

// Real input reaches Pitboard only while its window is in front. On windows-11-arm a full-screen window of the job user's
// keeps the foreground from every program the tests start (docs/facts/windows-runners.md).
internal static partial class Foreground
{
    public static nint Window() => GetForegroundWindow();

    public static bool Takes(Window window)
    {
        var handle = window.Properties.NativeWindowHandle.Value;
        window.SetForeground();
        return Retry.WhileFalse(() => GetForegroundWindow() == handle, TimeSpan.FromSeconds(5), TimeSpan.FromMilliseconds(100)).Result;
    }

    // For a test of real input itself, which nothing else stands in for.
    public static void Require(Window window)
    {
        if (!Takes(window))
        {
            Assert.Inconclusive($"CF1: {Holder()} keeps the foreground, so no key or click reaches Pitboard");
        }
    }

    public static unsafe string Holder()
    {
        var window = GetForegroundWindow();
        if (window == 0)
        {
            return "no window";
        }

        var name = stackalloc char[256];
        var length = GetClassName(window, name, 256);
        var title = stackalloc char[256];
        var titled = GetWindowText(window, title, 256);
        _ = GetWindowRect(window, out var place);
        _ = GetWindowThreadProcessId(window, out var owner);
        string process;
        try
        {
            process = Process.GetProcessById((int)owner).ProcessName;
        }
        catch (ArgumentException)
        {
            process = $"process {owner}";
        }

        return $"{new string(name, 0, length)} '{new string(title, 0, titled)}' of {process} at {place.Left},{place.Top} to {place.Right},{place.Bottom}";
    }

    [LibraryImport("user32.dll")]
    private static partial nint GetForegroundWindow();

    [LibraryImport("user32.dll", EntryPoint = "GetClassNameW")]
    private static unsafe partial int GetClassName(nint window, char* name, int capacity);

    [LibraryImport("user32.dll")]
    private static partial uint GetWindowThreadProcessId(nint window, out uint process);

    [LibraryImport("user32.dll", EntryPoint = "GetWindowTextW")]
    private static unsafe partial int GetWindowText(nint window, char* text, int capacity);

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool GetWindowRect(nint window, out Rect place);

    [StructLayout(LayoutKind.Sequential)]
    private struct Rect
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
