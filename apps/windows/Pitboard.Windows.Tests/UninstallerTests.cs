using System.Diagnostics;
using System.Xml.Linq;

namespace Pitboard.Windows.Tests;

// No test runs msiexec: each hands the uninstall a stand-in for what it would start.
[TestClass]
public sealed class UninstallerTests
{
    private const string Product = "{5B1F07C4-3A0E-4C9B-9D51-2F7A2C1E8B60}";

    private const string Installed = @"C:\Users\dana\AppData\Local\Programs\Pitboard\";

    private static List<ProcessStartInfo> Run(string folder, InstalledProduct? installed, Build build = Build.Release)
    {
        var started = new List<ProcessStartInfo>();
        new Uninstaller(build, folder, _ => installed, started.Add).Run();
        return started;
    }

    [TestMethod]
    public void ItStartsTheFullUninstallOfTheMsiThatInstalledPitboard()
    {
        var started = new List<ProcessStartInfo>();
        var asked = new List<string>();

        new Uninstaller(Build.Release, Installed, upgrade => { asked.Add(upgrade); return new(Product, Installed); }, started.Add).Run();

        Assert.AreEqual(Uninstaller.UpgradeCode, asked.Single());
        var msiexec = started.Single();
        Assert.AreEqual(Path.Combine(Environment.SystemDirectory, "msiexec.exe"), msiexec.FileName);
        CollectionAssert.AreEqual(new[] { "/x", Product, "/passive", "/norestart" }, msiexec.ArgumentList.ToArray());
        Assert.IsFalse(msiexec.UseShellExecute);
    }

    // A change to Package.wxs alone would leave the uninstall finding nothing.
    [TestMethod]
    public void ItAsksForTheUpgradeCodeOfPackageWxs()
    {
        var package = XDocument.Load(Repository.Path("apps", "windows", "Pitboard.Installer", "Package.wxs"))
            .Descendants().Single(element => element.Name.LocalName == "Package");

        Assert.AreEqual(Uninstaller.UpgradeCode, $"{{{package.Attribute("UpgradeCode")?.Value}}}");
    }

    // InstallLocation ends with a backslash, and a folder's name is the same in any case.
    [TestMethod]
    public void TheCopyInTheFolderItInstalledIsTheOneItUninstalls()
    {
        Assert.HasCount(1, Run(@"c:\users\dana\appdata\local\programs\pitboard", new(Product, Installed)));
    }

    [TestMethod]
    public void NothingIsStartedWhereNoMsiInstalledPitboard()
    {
        Assert.IsEmpty(Run(Installed, null));
    }

    // A zip's copy, say Scoop's, beside an installed one, would otherwise uninstall that one and stay.
    [TestMethod]
    public void NothingIsStartedForACopyTheMsiDidNotInstall()
    {
        Assert.IsEmpty(Run(@"C:\Users\dana\scoop\apps\pitboard\current\", new(Product, Installed)));
    }

    // A Debug build is never one an MSI installed, whatever this account has installed.
    [TestMethod]
    public void ADebugBuildStartsNoUninstall()
    {
        Assert.IsEmpty(Run(Installed, new(Product, Installed), Build.Debug));
    }
}
