using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class IdentityTests
{
    private static readonly Identity Debug = new(Build.Debug);
    private static readonly Identity Release = new(Build.Release);

    [TestMethod]
    public void ADebugBuildKeepsApartFromARelease()
    {
        Assert.AreEqual("Pitboard.Pitboard", Release.Aumid);
        Assert.AreEqual("Pitboard.Pitboard.Debug", Debug.Aumid);
        Assert.AreEqual("pitboard", Release.LinkScheme);
        Assert.AreEqual("pitboard-debug", Debug.LinkScheme);
        Assert.AreEqual("Pitboard", Release.InstanceKey(null));
        Assert.AreEqual("Pitboard Debug", Debug.InstanceKey(null));
        Assert.AreEqual(@"Pitboard Debug C:\Temp\pitboard-fixture", Debug.InstanceKey(@"C:\Temp\pitboard-fixture"));
    }

    [TestMethod]
    public void OnlyAReleaseOrAFixtureOpensLinks()
    {
        Assert.IsTrue(Release.OpensLinks(null));
        Assert.IsTrue(Debug.OpensLinks(@"C:\Temp\pitboard-fixture"));
        Assert.IsFalse(Debug.OpensLinks(null));
    }

    [TestMethod]
    public void EachBuildKeepsItsDataInAFolderOfItsOwn()
    {
        var local = Path.Combine(Path.GetTempPath(), "Local");
        var fixture = Path.Combine(Path.GetTempPath(), "pitboard-fixture");

        Assert.AreEqual(Path.Combine(local, "Pitboard App"), Release.DataFolder(local, null));
        Assert.AreEqual(Path.Combine(local, "Pitboard App Debug"), Debug.DataFolder(local, null));
        Assert.AreEqual(fixture, Debug.DataFolder(local, fixture));
    }
}
