using System.Runtime.InteropServices;
using Pitboard.Core;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.Storage.FileSystem;
using Windows.Win32.System.Variant;
using Windows.Win32.UI.Shell;
using Windows.Win32.UI.Shell.PropertiesSystem;

namespace Pitboard.Windows.Tests;

// The links a Jump List is made of, made and read back in memory: no test writes a list, which Windows keeps in the
// person's own profile whatever the homes. On an STA thread, as Pitboard makes them on its UI thread and in Main.
[STATestClass]
public sealed class JumpListTests
{
    private const string Personal = "35c0d1c9-6b25-5cdc-b543-64c0c0d8de46";

    private static readonly string Exe = Environment.ProcessPath!;

    [TestMethod]
    public void TheSwitchesAddAccountAndTheWindowsAreSetApart()
    {
        JumpTask[] tasks =
        [
            new("Switch to personal", new JumpAsk.Switch(Personal)), new("Switch to spare (Codex)", new JumpAsk.Switch(Personal)),
            new("Add account…", new JumpAsk.AddAccount()), new("Open claude.ai as work", new JumpAsk.OpenWindow(Personal)),
        ];

        CollectionAssert.AreEqual(
            new JumpTask?[] { tasks[0], tasks[1], null, tasks[2], null, tasks[3] }, SystemJumpList.Grouped(tasks).ToArray());
    }

    [TestMethod]
    public void ATaskIsALinkToPitboardExeWithItsArgumentsIconAndTitle()
    {
        var link = new SystemJumpList(new Identity(Build.Release), Exe).Link(new JumpTask("Switch to personal", new JumpAsk.Switch(Personal)));

        var path = new char[1024];
        var found = default(WIN32_FIND_DATAW);
        link.GetPath(path, ref found, (uint)SLGP_FLAGS.SLGP_RAWPATH).ThrowOnFailure();
        var arguments = new char[1024];
        link.GetArguments(arguments);
        var icon = new char[1024];
        link.GetIconLocation(icon, out var index);
        ((IPropertyStore)link).GetValue(PInvoke.PKEY_Title, out var title);
        string said;
        unsafe
        {
            said = title.pwszVal.ToString();
            Marshal.FreeCoTaskMem((nint)title.pwszVal.Value);
        }

        Assert.AreEqual(Exe, Read(path));
        Assert.AreEqual($"--switch-to {Personal}", Read(arguments));
        Assert.AreEqual((Exe, 0), (Read(icon), index));
        Assert.AreEqual(VARENUM.VT_LPWSTR, title.vt);
        Assert.AreEqual("Switch to personal", said);
    }

    [TestMethod]
    public void ASeparatorStandsBetweenTwoGroups()
    {
        JumpTask[] tasks = [new("Switch to personal", new JumpAsk.Switch(Personal)), new("Add account…", new JumpAsk.AddAccount())];

        var links = new SystemJumpList(new Identity(Build.Release), Exe).Links(tasks);
        links.GetCount(out var count);
        links.GetAt(1, out IPropertyStore separator);
        separator.GetValue(PInvoke.PKEY_AppUserModel_IsDestListSeparator, out var value);

        Assert.AreEqual(3u, count);
        Assert.AreEqual((VARENUM.VT_BOOL, VARIANT_BOOL.VARIANT_TRUE), (value.vt, value.boolVal));
    }

    private static string Read(char[] text) => new(text, 0, Array.IndexOf(text, '\0'));
}
