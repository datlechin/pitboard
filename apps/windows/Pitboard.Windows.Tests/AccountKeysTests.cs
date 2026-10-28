using Pitboard.Core;
using Windows.System;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AccountKeysTests
{
    private static AccountItem Spare(params AccountAction[] actions) =>
        Items.Account("claude", "spare") with { Actions = [.. actions, Items.RemoveAction("claude", "spare")] };

    [TestMethod]
    public void EnterDoesWhatTheRowsButtonDoes()
    {
        Assert.AreEqual((null, new Intent.SwitchTo(new AccountKey(Tool.Claude, "spare"))), AccountKeys.Pressed(VirtualKey.Enter, Spare()));
    }

    [TestMethod]
    public void DeleteAsksBeforeItRemoves()
    {
        var remove = Items.RemoveAction("claude", "spare");

        Assert.AreEqual((remove.Confirm, remove.Intent!), AccountKeys.Pressed(VirtualKey.Delete, Spare()));
        Assert.IsNull(AccountKeys.Pressed(VirtualKey.Delete, Spare() with { Actions = [remove with { Enabled = false }] }), "the account in use");
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
        var rename = Items.RenameAction("claude", "spare");

        Assert.AreEqual((null, rename.Intent!), AccountKeys.Pressed(VirtualKey.F2, Spare(rename)));
        Assert.IsNull(AccountKeys.Pressed(VirtualKey.F2, Spare(rename with { Enabled = false })));
        Assert.IsNull(AccountKeys.Pressed(VirtualKey.F2, Spare()));
    }

    // What the menu shows beside the items the keys do too.
    [TestMethod]
    public void TheMenuShowsTheKeysOfRenameAndRemove()
    {
        ActionKind[] kinds = [ActionKind.Rename, ActionKind.Remove, ActionKind.Switch];

        Assert.AreEqual("F2, Del, ", string.Join(", ", kinds.Select(AccountKeys.Shown)));
    }
}
