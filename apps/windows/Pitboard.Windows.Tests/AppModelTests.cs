using System.Reflection;
using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AppModelTests
{
    [TestMethod]
    public void ASnapshotNoNewerThanTheOneShownIsDropped()
    {
        var model = new AppModel(Snapshots.Read(2));
        var told = new List<string?>();
        model.PropertyChanged += (_, changed) => told.Add(changed.PropertyName);

        Assert.IsEmpty(model.Apply(Snapshots.Read(1) with { Reading = true }));
        Assert.IsEmpty(model.Apply(Snapshots.Read(2) with { Reading = true }));

        Assert.AreEqual(2UL, model.Revision);
        Assert.IsFalse(model.Reading);
        Assert.IsEmpty(told);
    }

    // Each call makes its arrays anew, as the bindings do for every snapshot told.
    private static Snapshot Listing(ulong revision) =>
        Snapshots.Read(revision) with { Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))] };

    // Every list in the newer snapshot is a new array holding what the shown one held.
    [TestMethod]
    public void OnlyThePartsThatChangedAreAssignedAndTold()
    {
        var model = new AppModel(Listing(1));
        var sections = model.Sections;
        var told = new List<string?>();
        model.PropertyChanged += (_, changed) => told.Add(changed.PropertyName);

        var assigned = model.Apply(Listing(3) with { Reading = true });

        Assert.AreEqual("Revision, Reading", string.Join(", ", assigned));
        Assert.AreEqual("Revision, Reading", string.Join(", ", told));
        Assert.AreSame(sections, model.Sections);
        Assert.IsTrue(model.Reading);
    }

    [TestMethod]
    public void AChangeInsideAListIsAChangeOfItsPart()
    {
        var model = new AppModel(Listing(1));
        var read = Listing(2);
        var section = read.Sections[0];
        var renamed = section with { Accounts = [section.Accounts[0] with { Title = "office" }] };

        var assigned = model.Apply(read with { Sections = [renamed] });

        Assert.AreEqual("Revision, Sections", string.Join(", ", assigned));
        Assert.AreEqual("office", model.Sections[0].Accounts[0].Title);
    }

    [TestMethod]
    public void TheModelShowsEveryPartOfASnapshot()
    {
        var snapshot = Snapshots.Read(1);
        var model = new AppModel(snapshot);

        foreach (var part in typeof(Snapshot).GetProperties(BindingFlags.Public | BindingFlags.Instance))
        {
            var shown = typeof(AppModel).GetProperty(part.Name);
            Assert.IsNotNull(shown, part.Name);
            Assert.AreEqual(part.PropertyType, shown.PropertyType, part.Name);
            Assert.AreEqual(part.GetValue(snapshot), shown.GetValue(model), part.Name);
        }
    }
}
