using System.ComponentModel;
using System.Diagnostics;
using Windows.Win32;
using Windows.Win32.Foundation;

namespace Pitboard.Windows;

// What Uninstall Pitboard leaves to Windows once the model has removed what is Pitboard's: the MSI's full uninstall,
// which takes Pitboard.exe away, so it is started on its own as Pitboard exits.
public sealed class Uninstaller(Build build, Func<string, string?> installed, Action<ProcessStartInfo> start)
{
    // Package.wxs's, one for the x64 and the ARM64 MSI.
    public const string UpgradeCode = "{9D967FF2-F54D-4D63-9B26-51D7E75F62FC}";

    public static Uninstaller For(Build build) => new(build, Installed, msiexec => Process.Start(msiexec)?.Dispose());

    // A Debug build, or a copy no MSI installed, leaves its files where they are.
    public void Run()
    {
        if (build is Build.Debug || installed(UpgradeCode) is not { } product)
        {
            return;
        }

        var msiexec = new ProcessStartInfo(Path.Combine(Environment.SystemDirectory, "msiexec.exe")) { UseShellExecute = false };
        foreach (var argument in new[] { "/x", product, "/passive" })
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

    // The product of this UpgradeCode Windows Installer has installed for the person running Pitboard.
    internal static unsafe string? Installed(string upgradeCode)
    {
        // A product code is a GUID in braces: 38 characters and the terminator.
        var product = stackalloc char[39];
        fixed (char* code = upgradeCode)
        {
            return PInvoke.MsiEnumRelatedProducts(code, 0, 0, product) == (uint)WIN32_ERROR.ERROR_SUCCESS
                ? new string(product)
                : null;
        }
    }
}
