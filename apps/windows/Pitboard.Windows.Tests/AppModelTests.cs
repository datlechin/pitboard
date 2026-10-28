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

    // Which parts differ is the model's to say: a part the snapshot does not name stays as it was, whatever it holds.
    [TestMethod]
    public void ASnapshotBasedOnTheOneShownAssignsOnlyItsParts()
    {
        var model = new AppModel(Listing(1));
        var sections = model.Sections;
        var told = new List<string?>();
        model.PropertyChanged += (_, changed) => told.Add(changed.PropertyName);

        var reading = Listing(2) with { Changed = new(1, [Part.Reading]), Reading = true, UpdatedMenu = "Updated 12:05" };

        var assigned = model.Apply(reading);

        Assert.AreEqual("Revision, Reading", string.Join(", ", assigned));
        Assert.AreEqual("Revision, Reading", string.Join(", ", told));
        Assert.IsTrue(model.Reading);
        Assert.AreSame(sections, model.Sections);
        Assert.AreEqual("Updated 12:00", model.UpdatedMenu, "a part it does not name");
    }

    // Snapshot() can give one the listener was never told, so a snapshot changed since another is shown whole.
    [TestMethod]
    public void ASnapshotBasedOnAnotherAssignsEveryPart()
    {
        var model = new AppModel(Listing(1));
        string[] every = [nameof(AppModel.Revision), .. Enum.GetValues<Part>().Select(part => part.ToString())];

        var assigned = model.Apply(Listing(3) with { Changed = new(2, [Part.Reading]), UpdatedMenu = "Updated 12:05" });

        CollectionAssert.AreEqual(every, assigned.ToArray());
        Assert.AreEqual("Updated 12:05", model.UpdatedMenu);
        Assert.AreEqual(3UL, model.Revision);
    }

    [TestMethod]
    public void TheModelShowsEveryPartOfASnapshot()
    {
        var snapshot = Snapshots.Read(1);
        var model = new AppModel(snapshot);

        Assert.AreEqual(snapshot.Revision, model.Revision);
        foreach (var part in Enum.GetValues<Part>())
        {
            var told = typeof(Snapshot).GetProperty(part.ToString());
            var shown = typeof(AppModel).GetProperty(part.ToString());
            Assert.IsNotNull(told, part.ToString());
            Assert.IsNotNull(shown, part.ToString());
            Assert.AreEqual(told.PropertyType, shown.PropertyType, part.ToString());
            Assert.AreEqual(told.GetValue(snapshot), shown.GetValue(model), part.ToString());
        }
    }
}
