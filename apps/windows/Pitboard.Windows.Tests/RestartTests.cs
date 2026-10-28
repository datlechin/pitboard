using System.Runtime.InteropServices;
using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed partial class RestartTests
{
    private const uint NoCrashHangOrReboot = 1 | 2 | 8;

    // HRESULT_FROM_WIN32(ERROR_NOT_FOUND): this process asked for no restart.
    private const int NotFound = unchecked((int)0x80070490);

    // Restart Manager starts again only a process that asked, so an upgrade the person opens brings Pitboard back.
    [TestMethod]
    public void OnlyAReleaseBuildAsksToStartAgainAfterAnUpdateAndNeverAfterACrashAHangOrAReboot()
    {
        try
        {
            Shell.RestartAfterUpdate(Build.Debug);
            Assert.AreEqual(NotFound, Settings(out _, out _));

            Shell.RestartAfterUpdate(Build.Release);
            Assert.AreEqual(0, Settings(out var command, out var flags));
            Assert.AreEqual("--updated", command);
            Assert.AreEqual(NoCrashHangOrReboot, flags);
        }
        finally
        {
            Marshal.ThrowExceptionForHR(UnregisterApplicationRestart());
        }
    }

    private static unsafe int Settings(out string command, out uint flags)
    {
        var buffer = stackalloc char[1024];
        var size = 1024u;
        var result = GetApplicationRestartSettings(GetCurrentProcess(), buffer, ref size, out flags);
        command = result == 0 ? new string(buffer) : "";
        return result;
    }

    [LibraryImport("kernel32.dll")]
    private static partial nint GetCurrentProcess();

    [LibraryImport("kernel32.dll")]
    private static unsafe partial int GetApplicationRestartSettings(nint process, char* command, ref uint size, out uint flags);

    [LibraryImport("kernel32.dll")]
    private static partial int UnregisterApplicationRestart();
}
