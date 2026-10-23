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

    // Microsoft's high DPI guidance loads an image again for a new DPI, and Explorer stretches one of another size.
    [TestMethod]
    public void TheImageIsLoadedAgainOnlyAtANewSize()
    {
        var size = 16;
        var done = new List<string>();
        var image = new IconImage(
            () => size,
            (wanted, _) =>
            {
                done.Add($"load {wanted}");
                return wanted;
            },
            before => done.Add($"destroy {before}"));

        image.Follow(shown => done.Add($"show {shown}"));
        size = 24;
        image.Follow(shown => done.Add($"show {shown}"));

        Assert.AreEqual("load 16, load 24, show 24, destroy 16", string.Join(", ", done));
        Assert.AreEqual((nint)24, image.Handle);
    }

    // What the menu bar says moves far more often than the level or the attention it shows.
    [TestMethod]
    public void TheImageIsLoadedAgainOnlyForANewLook()
    {
        var done = new List<string>();
        var image = new IconImage(
            () => 16,
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
            () => sizeFails is { } failed ? throw failed : size,
            (wanted, _) =>
            {
                if (loadFails is { } failed)
                {
                    throw failed;
                }

                done.Add($"load {wanted}");
                return wanted;
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

    // Drawn in ../Icon: the notification area takes an image at its own size, and Explorer would stretch another.
    [TestMethod]
    public void EachLevelIsLoadedAtTheSizeAskedFor()
    {
        foreach (var level in new UsageLevel?[] { null, UsageLevel.Plenty, UsageLevel.Low, UsageLevel.Out })
        {
            foreach (var size in new[] { 16, 20, 24, 32 })
            {
                var icon = Shell.NotificationIcon(size, new IconLook(level, Attention: false));
                try
                {
                    Assert.AreEqual(size, Pixels(icon).Length, $"{level} at {size}");
                }
                finally
                {
                    Shell.DestroyIcon(icon);
                }
            }
        }
    }

    // The warning goes over the bottom right, and the drawing beside it is the level's, its partly
    // covered edges at 20 px included.
    [TestMethod]
    public void AttentionPutsTheStandardWarningOverTheBottomRight()
    {
        foreach (var size in new[] { 16, 20, 24, 32 })
        {
            var plain = Shell.NotificationIcon(size, Low);
            var flagged = Shell.NotificationIcon(size, Low with { Attention = true });
            try
            {
                var before = Pixels(plain);
                var after = Pixels(flagged);
                Assert.AreEqual(size, after.Length);
                Assert.AreEqual(0xFFu, after[size - 2][size - 2] >> 24, $"the warning at {size}");
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
