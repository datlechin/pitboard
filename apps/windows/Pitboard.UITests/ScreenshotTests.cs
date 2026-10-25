using System.Drawing;
using System.Runtime.InteropServices;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Capturing;
using FlaUI.Core.Definitions;

namespace Pitboard.UITests;

// Pictures for the docs, as the macOS ScreenshotTests takes them. CI keeps windows-2025's as windows-app-pictures.
[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed partial class ScreenshotTests
{
    private const uint ExtendedFrameBounds = 9;

    private static readonly TimeSpan Patience = TimeSpan.FromSeconds(10);

    private static readonly TimeSpan OneFrame = TimeSpan.FromMilliseconds(50);

    public TestContext TestContext { get; set; } = null!;

    [TestMethod]
    public void TheMenuAndTheWindowForTheDocs()
    {
        var pictures = Directory.CreateTempSubdirectory("pitboard-pictures-");
        using var launched = Launched.Start(Builds.Debug, fixture: "twoTools");
        var window = launched.OpenWindow();
        // A picture is of the screen, so it shows whatever is in front.
        if (!Foreground.Takes(window))
        {
            Assert.Inconclusive($"CF1: {Foreground.Holder()} keeps the foreground, so a picture would show it");
        }

        // The pictures show the accounts: starting at sign-in, then what happens at a limit.
        foreach (var tip in new[] { "Start when you sign in to Windows", "Switch before a limit runs out" })
        {
            launched.Named(tip);
            launched.Invoke("Not now");
            Assert.IsTrue(launched.Gone(found => found.ByName(tip)), tip);
        }

        window.Move(0, 0);
        launched.Row("codex/spare");
        Launched.Shows(window, "5-hour limit, 42", exact: false);
        Keep(Drawn(() => Capture.Rectangle(Frame(window)), "the window"), pictures, "window");

        // The menu opens over the window, as the macOS picture shows it.
        launched.Select("pane.machine");
        var frame = Frame(window);
        HiddenWindows.OpenMenu(launched.Process, frame.Left + 40, frame.Top + 60);
        var main = launched.Find(found => found.ByName("main · Pro · 7%").And(found.ByControlType(ControlType.MenuItem)), "the menu's main");
        var menu = Around(main, ControlType.Menu);
        Keep(Drawn(() => Capture.Element(menu), "the menu"), pictures, "menu");
    }

    // A window and a menu animate as they open, and UI Automation gives the bounds they end at before they are drawn
    // there: a picture is kept once the next one, a frame later, is the same.
    private static CaptureImage Drawn(Func<CaptureImage> take, string what)
    {
        var deadline = DateTime.UtcNow + Patience;
        var taken = take();
        while (true)
        {
            Thread.Sleep(OneFrame);
            var next = take();
            var drawn = Pixels(taken).SequenceEqual(Pixels(next));
            taken.Dispose();
            taken = next;
            if (drawn)
            {
                return taken;
            }

            if (DateTime.UtcNow > deadline)
            {
                taken.Dispose();
                throw new AssertFailedException($"{what} was still changing after {Patience}");
            }
        }
    }

    private static byte[] Pixels(CaptureImage picture)
    {
        using var png = new MemoryStream();
        picture.Bitmap.Save(png, System.Drawing.Imaging.ImageFormat.Png);
        return png.ToArray();
    }

    private void Keep(CaptureImage picture, DirectoryInfo pictures, string name)
    {
        using (picture)
        {
            var file = Path.Combine(pictures.FullName, $"{name}.png");
            picture.ToFile(file);
            TestContext.AddResultFile(file);
        }
    }

    private static AutomationElement Around(AutomationElement element, ControlType kind)
    {
        for (var parent = element.Parent; parent is not null; parent = parent.Parent)
        {
            if (parent.Properties.ControlType.ValueOrDefault == kind)
            {
                return parent;
            }
        }

        throw new AssertFailedException($"no {kind} around '{element.Name}'");
    }

    // GetWindowRect takes in the window's drop shadow; the frame DWM draws leaves it out.
    private static Rectangle Frame(Window window)
    {
        var handle = window.Properties.NativeWindowHandle.Value;
        Marshal.ThrowExceptionForHR(DwmGetWindowAttribute(handle, ExtendedFrameBounds, out var frame, (uint)Marshal.SizeOf<Bounds>()));
        return Rectangle.FromLTRB(frame.Left, frame.Top, frame.Right, frame.Bottom);
    }

    [LibraryImport("dwmapi.dll")]
    private static partial int DwmGetWindowAttribute(nint window, uint attribute, out Bounds value, uint size);

    [StructLayout(LayoutKind.Sequential)]
    private struct Bounds
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
