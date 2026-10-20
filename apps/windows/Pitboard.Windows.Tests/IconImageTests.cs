using System.Runtime.InteropServices;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class IconImageTests
{
    // Microsoft's high DPI guidance loads an image again for a new DPI, and Explorer stretches one of another size.
    [TestMethod]
    public void TheImageIsLoadedAgainOnlyAtANewSize()
    {
        var size = 16;
        var done = new List<string>();
        var image = new IconImage(
            () => size,
            wanted =>
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
            wanted =>
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
        loadFails = null;
        image.Follow(shown => done.Add($"show {shown}"));

        Assert.AreEqual("load 16, load 24, show 24, destroy 16", string.Join(", ", done));
    }
}
