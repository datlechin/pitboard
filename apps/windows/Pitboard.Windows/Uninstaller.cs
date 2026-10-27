using System.ComponentModel;
using System.Diagnostics;
using Windows.Win32;
using Windows.Win32.Foundation;

namespace Pitboard.Windows;

// A product Windows Installer installed, and the folder Package.wxs records it in.
public sealed record InstalledProduct(string Code, string Folder);

// What Uninstall Pitboard leaves to Windows once the model has removed what is Pitboard's: the MSI's full uninstall,
// which takes Pitboard.exe away, so it is started on its own as Pitboard exits.
public sealed class Uninstaller(Build build, string folder, Func<string, InstalledProduct?> installed, Action<ProcessStartInfo> start)
{
    // Package.wxs's, one for the x64 and the ARM64 MSI.
    public const string UpgradeCode = "{9D967FF2-F54D-4D63-9B26-51D7E75F62FC}";

    // Room for the longest path Windows has.
    private const int LongestPath = 32_768;

    public static Uninstaller For(Build build) =>
        new(build, AppContext.BaseDirectory, InstalledBy, msiexec => Process.Start(msiexec)?.Dispose());

    // A Debug build, or a copy the MSI did not install, such as a zip's beside an installed one, leaves its files where
    // they are.
    public void Run()
    {
        if (build is Build.Debug || installed(UpgradeCode) is not { } product || !Same(product.Folder, folder))
        {
            return;
        }

        var msiexec = new ProcessStartInfo(Path.Combine(Environment.SystemDirectory, "msiexec.exe")) { UseShellExecute = false };
        // Alone, /passive restarts Windows unasked where a file is still in use; WinSparkle's updates pass /norestart too.
        foreach (var argument in new[] { "/x", product.Code, "/passive", "/norestart" })
        {
            msiexec.ArgumentList.Add(argument);
        }

        try
        {
            start(msiexec);
        }
        catch (Win32Exception)
        {
            // Settings > Apps still lists Pitboard, whose uninstall this would have started.
        }
    }

    private static bool Same(string one, string other) =>
        string.Equals(
            Path.TrimEndingDirectorySeparator(Path.GetFullPath(one)), Path.TrimEndingDirectorySeparator(Path.GetFullPath(other)),
            StringComparison.OrdinalIgnoreCase);

    // The product of this UpgradeCode Windows Installer has installed for the person running Pitboard.
    internal static unsafe InstalledProduct? InstalledBy(string upgradeCode)
    {
        // A product code is a GUID in braces: 38 characters and the terminator.
        var product = stackalloc char[39];
        fixed (char* code = upgradeCode)
        {
            if (PInvoke.MsiEnumRelatedProducts(code, 0, 0, product) != (uint)WIN32_ERROR.ERROR_SUCCESS)
            {
                return null;
            }
        }

        var found = new string(product);
        var location = new char[LongestPath];
        var length = (uint)location.Length;
        fixed (char* code = found)
        fixed (char* property = "InstallLocation")
        fixed (char* into = location)
        {
            return PInvoke.MsiGetProductInfo(code, property, into, &length) == (uint)WIN32_ERROR.ERROR_SUCCESS && length > 0
                ? new InstalledProduct(found, new string(into, 0, (int)length))
                : null;
        }
    }
}
