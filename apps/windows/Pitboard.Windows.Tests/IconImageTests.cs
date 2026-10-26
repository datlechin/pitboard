using System.Runtime.InteropServices;
using Pitboard.Core;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.Graphics.Gdi;
using Windows.Win32.UI.WindowsAndMessaging;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class IconImageTests
{
    private static readonly IconLook Low = new(UsageLevel.Low, Attention: false);

    private static readonly bool[] Taskbars = [true, false];

    // What the notification area draws at 100% to 300%.
    private static readonly int[] Sizes = [16, 20, 24, 32, 40, 48];

    // Microsoft's high DPI guidance loads an image again for a new DPI, and Explorer stretches one of another size.
    [TestMethod]
    public void TheImageIsLoadedAgainOnlyAtANewSize()
    {
        var size = 16;
        var done = new List<string>();
        var image = new IconImage(
            () => new IconPlace(size, LightTaskbar: true),
            (wanted, _) =>
            {
                done.Add($"load {wanted.Size}");
                return wanted.Size;
            },
            before => done.Add($"destroy {before}"));

        image.Follow(shown => done.Add($"show {shown}"));
        size = 24;
        image.Follow(shown => done.Add($"show {shown}"));

        Assert.AreEqual("load 16, load 24, show 24, destroy 16", string.Join(", ", done));
        Assert.AreEqual((nint)24, image.Handle);
    }

    // A change of Windows' mode or of a contrast theme reaches the hidden window as WM_SETTINGCHANGE, which follows.
    [TestMethod]
    public void TheImageIsLoadedAgainOnATaskbarOfTheOtherTheme()
    {
        var light = true;
        var done = new List<string>();
        var image = new IconImage(
            () => new IconPlace(16, light),
            (wanted, _) =>
            {
                done.Add($"load {(wanted.LightTaskbar ? "light" : "dark")}");
                return done.Count;
            },
            before => done.Add($"destroy {before}"));

        image.Follow(shown => done.Add($"show {shown}"));
        light = false;
        image.Follow(shown => done.Add($"show {shown}"));

        Assert.AreEqual("load light, load dark, show 2, destroy 1", string.Join(", ", done));
    }

    // Windows' mode for the taskbar, read as PowerToys reads it, gives way to a contrast theme's text colour.
    [TestMethod]
    public void TheTaskbarsThemeIsWindowsModeOrTheContrastThemes()
    {
        Assert.IsTrue(IconPlace.Light(systemUsesLightTheme: 1, contrastText: null));
        Assert.IsFalse(IconPlace.Light(systemUsesLightTheme: 0, contrastText: null));
        Assert.IsTrue(IconPlace.Light(systemUsesLightTheme: null, contrastText: null), "no mode set");
        // A dark contrast theme's white text, a light one's black and a dark blue, as COLORREFs.
        Assert.IsFalse(IconPlace.Light(systemUsesLightTheme: 1, contrastText: 0x00FFFFFF));
        Assert.IsTrue(IconPlace.Light(systemUsesLightTheme: 0, contrastText: 0x00000000));
        Assert.IsTrue(IconPlace.Light(systemUsesLightTheme: 0, contrastText: 0x00800000));
    }

    // What the menu bar says moves far more often than the level or the attention it shows.
    [TestMethod]
    public void TheImageIsLoadedAgainOnlyForANewLook()
    {
        var done = new List<string>();
        var image = new IconImage(
            () => new IconPlace(16, LightTaskbar: true),
            (wanted, look) =>
            {
                done.Add($"load {look.Level?.ToString() ?? "none"} {look.Attention}");
                return done.Count;
            },
            before => done.Add($"destroy {before}"));

        image.Show(Low, shown => done.Add($"show {shown}"));
        image.Show(Low, shown => done.Add($"show {shown}"));
        image.Show(Low with { Attention = true }, shown => done.Add($"show {shown}"));
        image.Follow(shown => done.Add($"show {shown}"));

        Assert.AreEqual(
            "load none False, load Low False, show 2, destroy 1, load Low True, show 5, destroy 2",
            string.Join(", ", done));
    }

    // It follows from the hidden window's procedure, where an exception ends Pitboard.
    [TestMethod]
    public void AFailedReadOrLoadKeepsTheImageShown()
    {
        Exception? sizeFails = null;
        Exception? loadFails = null;
        var size = 16;
        var done = new List<string>();
        var image = new IconImage(
            () => sizeFails is { } failed ? throw failed : new IconPlace(size, LightTaskbar: true),
            (wanted, _) =>
            {
                if (loadFails is { } failed)
                {
                    throw failed;
                }

                done.Add($"load {wanted.Size}");
                return wanted.Size;
            },
            before => done.Add($"destroy {before}"));

        size = 24;
        // As ThrowOnFailure raises E_INVALIDARG and E_FAIL.
        sizeFails = Marshal.GetExceptionForHR(unchecked((int)0x80070057));
        image.Follow(shown => done.Add($"show {shown}"));
        sizeFails = null;
        loadFails = Marshal.GetExceptionForHR(unchecked((int)0x80004005));
        image.Follow(shown => done.Add($"show {shown}"));
        image.Show(Low, shown => done.Add($"show {shown}"));
        loadFails = null;
        image.Follow(shown => done.Add($"show {shown}"));

        Assert.AreEqual("load 16, load 24, show 24, destroy 16", string.Join(", ", done));
    }

    // Drawn in ../Icon: the notification area takes an image at its own size, 40 and 48 at 250% and 300%, and Explorer would
    // stretch another.
    [TestMethod]
    public void EachLevelIsLoadedAtTheSizeAskedFor()
    {
        foreach (var light in Taskbars)
        {
            foreach (var level in new UsageLevel?[] { null, UsageLevel.Plenty, UsageLevel.Low, UsageLevel.Out })
            {
                foreach (var size in Sizes)
                {
                    var icon = Shell.NotificationIcon(new IconPlace(size, light), new IconLook(level, Attention: false));
                    try
                    {
                        Assert.AreEqual(size, Pixels(icon).Length, $"{level} at {size} on a light taskbar: {light}");
                    }
                    finally
                    {
                        Shell.DestroyIcon(icon);
                    }
                }
            }
        }
    }

    // In Fluent's primary text colour on each taskbar, as Windows' own icons are: black at 89.4% on a light one, white on a
    // dark one. tray-*-16.svg's lit row is at (5, 4).
    [TestMethod]
    public void EachTaskbarHasTheDrawingInItsOwnInk()
    {
        var onLight = Shell.NotificationIcon(new IconPlace(16, LightTaskbar: true), new IconLook(UsageLevel.Plenty, Attention: false));
        var onDark = Shell.NotificationIcon(new IconPlace(16, LightTaskbar: false), new IconLook(UsageLevel.Plenty, Attention: false));
        try
        {
            Assert.AreEqual(0xE4000000u, Pixels(onLight)[4][5]);
            Assert.AreEqual(0xFFFFFFFFu, Pixels(onDark)[4][5]);
        }
        finally
        {
            Shell.DestroyIcon(onLight);
            Shell.DestroyIcon(onDark);
        }
    }

    // With no reading the lit row shows only its track: plenty would claim room nobody measured.
    [TestMethod]
    public void NoReadingLeavesTheRowUnlit()
    {
        foreach (var light in Taskbars)
        {
            var plenty = Shell.NotificationIcon(new IconPlace(16, light), new IconLook(UsageLevel.Plenty, Attention: false));
            var none = Shell.NotificationIcon(new IconPlace(16, light), new IconLook(null, Attention: false));
            try
            {
                var lit = Pixels(plenty)[4][5];
                var unlit = Pixels(none)[4][5];
                Assert.AreEqual(lit & 0xFFFFFF, unlit & 0xFFFFFF, $"one ink on a light taskbar: {light}");
                Assert.IsLessThan(lit >> 24, unlit >> 24, $"a light taskbar: {light}");
            }
            finally
            {
                Shell.DestroyIcon(plenty);
                Shell.DestroyIcon(none);
            }
        }
    }

    // The warning goes over the bottom right, and the drawing beside it is the level's, its partly
    // covered edges at 20 px included.
    [TestMethod]
    public void AttentionPutsTheStandardWarningOverTheBottomRight()
    {
        foreach (var place in Taskbars.SelectMany(light => Sizes.Select(size => new IconPlace(size, light))))
        {
            var size = place.Size;
            var plain = Shell.NotificationIcon(place, Low);
            var flagged = Shell.NotificationIcon(place, Low with { Attention = true });
            try
            {
                var before = Pixels(plain);
                var after = Pixels(flagged);
                Assert.AreEqual(size, after.Length);
                // Not one pixel: the stock icon's edges are smoothed, its corner to 245 of 255 at 16 px.
                Assert.IsTrue(
                    Enumerable.Range(size / 2, size / 2).Any(
                        y => Enumerable.Range(size / 2, size / 2).Any(x => before[y][x] >> 24 == 0 && after[y][x] >> 24 == 0xFF)),
                    $"the warning at {size}");
                for (var y = 0; y < size / 3; y++)
                {
                    for (var x = 0; x < size; x++)
                    {
                        Same(before[y][x], after[y][x], $"({x}, {y}) at {size}");
                    }
                }
            }
            finally
            {
                Shell.DestroyIcon(plain);
                Shell.DestroyIcon(flagged);
            }
        }
    }

    // Drawn and read back, a partly covered pixel's colour may round a step either way.
    private static void Same(uint before, uint after, string at)
    {
        Assert.AreEqual(before >> 24, after >> 24, at);
        for (var shift = 0; shift < 24; shift += 8)
        {
            Assert.IsLessThanOrEqualTo(1, Math.Abs(OverBlack(before, shift) - OverBlack(after, shift)), at);
        }
    }

    private static int OverBlack(uint pixel, int shift) => (int)(((pixel >> shift) & 0xFF) * (pixel >> 24) / 255);

    // An icon's straight BGRA pixels, top row first, at the size its bitmap is.
    private static unsafe uint[][] Pixels(nint icon)
    {
        ICONINFO info;
        Assert.IsTrue(PInvoke.GetIconInfo((HICON)icon, &info));
        var screen = PInvoke.GetDC(HWND.Null);
        try
        {
            var header = new BITMAPINFO { bmiHeader = new BITMAPINFOHEADER { biSize = (uint)sizeof(BITMAPINFOHEADER) } };
            Assert.AreNotEqual(0, PInvoke.GetDIBits(screen, info.hbmColor, 0, 0, null, &header, DIB_USAGE.DIB_RGB_COLORS));
            var size = header.bmiHeader.biWidth;
            header.bmiHeader = new BITMAPINFOHEADER
            {
                biSize = (uint)sizeof(BITMAPINFOHEADER),
                biWidth = size,
                biHeight = -size,
                biPlanes = 1,
                biBitCount = 32,
            };
            var pixels = new uint[size * size];
            fixed (uint* bits = pixels)
            {
                Assert.AreEqual(size, PInvoke.GetDIBits(screen, info.hbmColor, 0, (uint)size, bits, &header, DIB_USAGE.DIB_RGB_COLORS));
            }

            return [.. pixels.Chunk(size)];
        }
        finally
        {
            _ = PInvoke.ReleaseDC(HWND.Null, screen);
            PInvoke.DeleteObject(info.hbmColor);
            PInvoke.DeleteObject(info.hbmMask);
        }
    }
}
