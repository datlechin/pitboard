using System.Diagnostics;

namespace Pitboard.Windows.Tests;

// No test runs msiexec: each hands the uninstall a stand-in for what it would start.
[TestClass]
public sealed class UninstallerTests
{
    private const string Product = "{5B1F07C4-3A0E-4C9B-9D51-2F7A2C1E8B60}";

    [TestMethod]
    public void ItStartsTheFullUninstallOfTheMsiThatInstalledPitboard()
    {
        var started = new List<ProcessStartInfo>();
        var asked = new List<string>();

        new Uninstaller(Build.Release, upgrade => { asked.Add(upgrade); return Product; }, started.Add).Run();

        Assert.AreEqual("{9D967FF2-F54D-4D63-9B26-51D7E75F62FC}", asked.Single(), "Package.wxs's UpgradeCode");
        var msiexec = started.Single();
        Assert.AreEqual(Path.Combine(Environment.SystemDirectory, "msiexec.exe"), msiexec.FileName);
        CollectionAssert.AreEqual(new[] { "/x", Product, "/passive" }, msiexec.ArgumentList.ToArray());
        Assert.IsFalse(msiexec.UseShellExecute);
    }

    [TestMethod]
    public void NothingIsStartedWhereNoMsiInstalledPitboard()
    {
        var started = new List<ProcessStartInfo>();

        new Uninstaller(Build.Release, _ => null, started.Add).Run();

        Assert.IsEmpty(started);
    }

    // A Debug build is never one an MSI installed, whatever this account has installed.
    [TestMethod]
    public void ADebugBuildStartsNoUninstall()
    {
        var started = new List<ProcessStartInfo>();

        new Uninstaller(Build.Debug, _ => Product, started.Add).Run();

        Assert.IsEmpty(started);
    }
}
