using Pitboard.Core;
using Windows.System;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AccountKeysTests
{
    private static readonly ItemOffer Rename = new(
        Title: "Rename…", Intent: new Intent.PresentSheet(new Sheet.Rename("claude", "spare")), Enabled: true, Confirm: null);

    private static AccountItem Spare(params ItemOffer[] offers) =>
        Items.Account("claude", "spare") with { Offers = offers, Forget = Items.Forget("claude/spare") };

    [TestMethod]
    public void EnterDoesWhatTheRowsButtonDoes()
    {
        Assert.AreEqual((null, new Intent.SwitchTo("claude/spare")), AccountKeys.Pressed(VirtualKey.Enter, Spare()));
    }

    [TestMethod]
    public void DeleteAsksBeforeItForgets()
    {
        var forget = Items.Forget("claude/spare");

        Assert.AreEqual((forget.Confirm, forget.Intent), AccountKeys.Pressed(VirtualKey.Delete, Spare()));
    }

    [TestMethod]
    public void CtrlCCopiesTheEmailAddressAsItsMenuDoes()
    {
        Assert.AreEqual("spare@example.com", AccountKeys.Copied(VirtualKey.C, control: true, Spare()));
        Assert.IsNull(AccountKeys.Copied(VirtualKey.C, control: false, Spare()));
        Assert.IsNull(AccountKeys.Copied(VirtualKey.C, control: true, Spare() with { Email = "" }));
    }

    // Windows' rename key, for the Rename… the row's menu offers.
    [TestMethod]
    public void F2RenamesTheAccountAsItsMenuDoes()
    {
        Assert.AreEqual((null, Rename.Intent), AccountKeys.Pressed(VirtualKey.F2, Spare(Rename)));
        Assert.IsNull(AccountKeys.Pressed(VirtualKey.F2, Spare(Rename with { Enabled = false })));
        Assert.IsNull(AccountKeys.Pressed(VirtualKey.F2, Spare()));
    }
}
