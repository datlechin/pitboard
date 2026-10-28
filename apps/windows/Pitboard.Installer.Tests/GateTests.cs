using System.Runtime.InteropServices;
using Pitboard.Core;

namespace Pitboard.Installer.Tests;

// The MSI image's own pitboard_ffi.dll, loaded here: an opened Release build of Pitboard.exe registers with Windows as it starts.
[TestClass]
public sealed class GateTests
{
    [ClassInitialize]
    public static void LoadTheImagesLibrary(TestContext context)
    {
        var library = Path.Combine(Built.Image, "pitboard_ffi.dll");
        NativeLibrary.SetDllImportResolver(
            typeof(PitboardModelMethods).Assembly,
            (name, _, _) => name == "pitboard_ffi" ? NativeLibrary.Load(library) : IntPtr.Zero);
    }

    [TestMethod]
    public void TheImagesCoreRefusesUntilPitboardForWindowsIsReleased()
    {
        var root = Directory.CreateTempSubdirectory("pitboard-installer-gate-").FullName;
        try
        {
            string[] homes = ["USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA", "CODEX_HOME", "CLAUDE_CONFIG_DIR", "PITBOARD_HOME"];
            var environment = homes.ToDictionary(home => home, home => Directory.CreateDirectory(Path.Combine(root, home)).FullName);

            var refused = PitboardModelMethods.LaunchRefusal(environment);

            if (Built.Major(Built.CargoVersion()) == 0)
            {
                Assert.AreEqual("windows_not_released", refused?.Code);
                foreach (var home in environment.Values)
                {
                    Assert.IsEmpty(Directory.EnumerateFileSystemEntries(home), home);
                }
            }
            else
            {
                Assert.IsNull(refused, refused?.Message);
            }
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }
}
