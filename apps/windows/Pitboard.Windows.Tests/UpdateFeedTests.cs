using System.Runtime.InteropServices;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class UpdateFeedTests
{
    // As windows-appcast.py writes one.
    private static string Feed(params (string Version, string Os)[] items) =>
        $"""
        <?xml version="1.0" encoding="utf-8"?>
        <rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
          <channel>
            <title>Pitboard for Windows</title>
        {string.Concat(items.Select(Item))}
          </channel>
        </rss>
        """;

    private static string Item((string Version, string Os) item) =>
        $"""
            <item>
              <title>Pitboard {item.Version}</title>
              <sparkle:version>{item.Version}</sparkle:version>
              <sparkle:shortVersionString>{item.Version}</sparkle:shortVersionString>
              <sparkle:minimumSystemVersion>10.0.26100</sparkle:minimumSystemVersion>
              <enclosure url="https://example.com/{item.Version}-{item.Os}.msi" length="1" type="application/x-msi"
                  sparkle:os="{item.Os}" sparkle:installerArguments="/passive /norestart LAUNCHAFTERINSTALL=1" sparkle:edSignature="c2ln"/>
            </item>

        """;

    // 1.0.0 is offered to a beta, and would be to 1.0.0+<commit> again and again: the app passes the plain version.
    [TestMethod]
    public void ABetaSortsBelowItsFullReleaseAsWinSparkleSortsIt()
    {
        foreach (var (lower, higher) in new[]
        {
            ("1.0.0-beta.1", "1.0.0"),
            ("1.0.0-beta.1", "1.0.0-beta.2"),
            ("1.0.0-beta.9", "1.0.0-beta.10"),
            ("1.0.0+3f2a9c1", "1.0.0"),
            ("1.0.0", "1.0.1"),
            ("1.9.0", "1.10.0"),
            ("1.2rc1", "1.2.0"),
            ("1.5b3", "1.5"),
            ("1.5", "1.5.1"),
        })
        {
            Assert.IsLessThan(0, UpdateFeed.Compare(lower, higher), $"{lower} below {higher}");
            Assert.IsGreaterThan(0, UpdateFeed.Compare(higher, lower), $"{higher} above {lower}");
        }

        Assert.AreEqual(0, UpdateFeed.Compare("1.0.0", "1.0.0"));
    }

    [TestMethod]
    public void TheNewestItemForThisMachineNamesTheVersion()
    {
        var feed = Feed(("1.0.1", "windows-x64"), ("1.0.1", "windows-arm64"));

        Assert.AreEqual("1.0.1", UpdateFeed.Newest(feed, "windows-arm64"));
        Assert.AreEqual("1.0.1", UpdateFeed.Newest(feed, "windows-x64"));
        Assert.IsNull(UpdateFeed.Newest(Feed(("1.0.1", "windows-x64")), "windows-arm64"), "an item for another machine");
        Assert.AreEqual("1.1.0", UpdateFeed.Newest(Feed(("1.0.1", "windows-x64"), ("1.1.0", "windows")), "windows-x64"));
        Assert.AreEqual("1.0.10", UpdateFeed.Newest(Feed(("1.0.10", "windows-x64"), ("1.0.9", "windows-x64")), "windows-x64"));
    }

    [TestMethod]
    public void EachArchitectureReadsItsOwnMarker()
    {
        Assert.AreEqual("windows-arm64", UpdateFeed.Marker(Architecture.Arm64));
        Assert.AreEqual("windows-x64", UpdateFeed.Marker(Architecture.X64));
    }
}
