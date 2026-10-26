namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class ShownAccountTests
{
    [TestMethod]
    public void TheRowIsToldOnlyWhenItsAccountChanged()
    {
        var shown = new ShownAccount(Items.Account("claude", "work"), _ => { });
        var told = new List<string?>();
        shown.PropertyChanged += (_, changed) => told.Add(changed.PropertyName);

        shown.Show(Items.Account("claude", "work"));
        shown.Show(Items.Account("claude", "work") with { Title = "office" });

        Assert.AreEqual("Item", string.Join(", ", told));
        Assert.AreEqual("office", shown.Item.Title);
    }
}
