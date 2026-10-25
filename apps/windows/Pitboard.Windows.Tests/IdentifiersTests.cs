using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class IdentifiersTests
{
    // As the macOS app names them, so a UI test of either app finds a control by the same name.
    [TestMethod]
    public void AControlIsFoundByTheModelsCaseItShowsOrSends()
    {
        Assert.AreEqual("switchTo", Identifiers.Of(new Intent.SwitchTo("claude/spare")));
        Assert.AreEqual("abandonStuckSwitch", Identifiers.Of(new Intent.AbandonStuckSwitch()));
        Assert.AreEqual("rename", Identifiers.Of(new Intent.PresentSheet(new Sheet.Rename("claude", "work"))));
        Assert.AreEqual("onlyOne", Identifiers.Of(new Footing.OnlyOne("claude", "work")));
        Assert.AreEqual("notify", Identifiers.Of(AtLimit.Notify));
    }
}
