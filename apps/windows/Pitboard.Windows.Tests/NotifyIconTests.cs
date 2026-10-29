using Pitboard.Core;
using Windows.Win32.Foundation;
using Windows.Win32.UI.Shell;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class NotifyIconTests
{
    private const nint Window = 0x1234;
    private const nint Image = 0x5678;

    private static readonly Guid? Release = new Identity(Build.Release).IconGuid;
    private static readonly Guid? Debug = new Identity(Build.Debug).IconGuid;

    private static string Said(StandInArea area) => string.Join(", ", area.Sent.Select(sent => sent.Message));

    [TestMethod]
    public void TheReleaseBuildAddsItsIconByItsGuid()
    {
        var area = new StandInArea();
        var icon = new NotifyIcon(area, Window, Image, Release);

        icon.Show("Pitboard, work 42%");

        Assert.AreEqual("Add, SetVersion", Said(area));
        Assert.AreEqual(new IconData(Window, 1, Release, NotifyIcon.CallbackMessage, Image, "Pitboard, work 42%"), area.Sent[0].Icon);
        Assert.IsNotNull(Release);
    }

    // A GUID is tied to the path of the exe that added it, and a Debug build is run from any folder.
    [TestMethod]
    public void TheDebugBuildAddsItsIconByItsWindowAndId()
    {
        var area = new StandInArea();
        var icon = new NotifyIcon(area, Window, Image, Debug);

        icon.Show("Pitboard");

        Assert.IsNull(Debug);
        Assert.AreEqual(new IconData(Window, 1, null, NotifyIcon.CallbackMessage, Image, "Pitboard"), area.Sent[0].Icon);
    }

    [TestMethod]
    public void OnlyTheReleaseBuildAsksForItsGuid()
    {
        var release = ShellNotificationArea.Native(new IconData(Window, 1, Release, NotifyIcon.CallbackMessage, Image, "Pitboard"));
        var debug = ShellNotificationArea.Native(new IconData(Window, 1, null, NotifyIcon.CallbackMessage, Image, "Pitboard"));

        Assert.IsTrue(release.uFlags.HasFlag(NOTIFY_ICON_DATA_FLAGS.NIF_GUID));
        Assert.AreEqual(Release, release.guidItem);
        Assert.IsFalse(debug.uFlags.HasFlag(NOTIFY_ICON_DATA_FLAGS.NIF_GUID));
        Assert.AreEqual(Guid.Empty, debug.guidItem);
        Assert.AreEqual(1U, debug.uID);
        Assert.AreEqual((HWND)Window, debug.hWnd);
    }

    // NOTIFYICON_VERSION_4 drops the standard tooltip unless NIF_SHOWTIP asks for it.
    [TestMethod]
    public void TheIconShowsTheStandardTooltip()
    {
        var data = ShellNotificationArea.Native(new IconData(Window, 1, null, NotifyIcon.CallbackMessage, Image, "Pitboard, work 42%"));

        Assert.AreEqual(
            NOTIFY_ICON_DATA_FLAGS.NIF_MESSAGE | NOTIFY_ICON_DATA_FLAGS.NIF_ICON | NOTIFY_ICON_DATA_FLAGS.NIF_TIP | NOTIFY_ICON_DATA_FLAGS.NIF_SHOWTIP,
            data.uFlags);
        Assert.AreEqual("Pitboard, work 42%", data.szTip.ToString());
        Assert.AreEqual(NotifyIcon.CallbackMessage, data.uCallbackMessage);
    }

    [TestMethod]
    public void ANewTipModifiesTheIconAddedAlready()
    {
        var area = new StandInArea();
        var icon = new NotifyIcon(area, Window, Image, Debug);

        icon.Show("Pitboard");
        icon.Show("Pitboard, work 42%");

        Assert.AreEqual("Add, SetVersion, Modify", Said(area));
        Assert.AreEqual("Pitboard, work 42%", area.Sent[^1].Icon.Tip);
    }

    // As the display's scale changes, the image the notification area draws at its new size.
    [TestMethod]
    public void ANewImageModifiesTheIconAddedAlready()
    {
        var area = new StandInArea();
        var icon = new NotifyIcon(area, Window, Image, Debug);
        icon.Redraw(0x9abc);
        Assert.AreEqual("", Said(area), "nothing to modify before it is added");

        icon.Show("Pitboard");
        icon.Redraw(0xdef0);

        Assert.AreEqual("Add, SetVersion, Modify", Said(area));
        Assert.AreEqual((nint)0x9abc, area.Sent[0].Icon.Image);
        Assert.AreEqual((nint)0xdef0, area.Sent[^1].Icon.Image);
    }

    // Narrator reads the tooltip of an icon in the notification area, every line of it.
    [TestMethod]
    public void AShortTipIsKeptWhole()
    {
        Assert.AreEqual("Pitboard\nwork: 5-hour 42%\njob (Codex): 5-hour 71%", NotifyIcon.Tip("Pitboard\nwork: 5-hour 42%\njob (Codex): 5-hour 71%"));
    }

    [TestMethod]
    public void ALongTipIsCutTo127Units()
    {
        var spoken = "Pitboard, " + new string('w', 200);

        Assert.AreEqual(spoken[..127], NotifyIcon.Tip(spoken));
    }

    // An emoji in an account's name is two UTF-16 units, never cut between them.
    [TestMethod]
    public void ACutNeverSplitsASurrogatePair()
    {
        var spoken = new string('w', 126) + "🏁 work";

        Assert.AreEqual(new string('w', 126), NotifyIcon.Tip(spoken));
        Assert.AreEqual(127, NotifyIcon.Tip(new string('w', 125) + "🏁 work").Length);
    }

    // Explorer started again, with the old icon gone.
    [TestMethod]
    public void TheIconIsAddedAgainWithItsTip()
    {
        var area = new StandInArea();
        var icon = new NotifyIcon(area, Window, Image, Release);
        icon.Show("Pitboard, work 42%");

        icon.Readd();

        Assert.AreEqual("Add, SetVersion, Add, SetVersion", Said(area));
        Assert.AreEqual("Pitboard, work 42%", area.Sent[2].Icon.Tip);
    }

    [TestMethod]
    public void AnIconRefusedItsGuidIsAddedByItsWindow()
    {
        var area = new StandInArea { Refuses = icon => icon.IconGuid is not null };
        var icon = new NotifyIcon(area, Window, Image, Release);

        icon.Show("Pitboard");
        icon.Show("Pitboard, work 42%");

        Assert.AreEqual("Add, Add, SetVersion, Modify", Said(area));
        Assert.IsNull(area.Sent[1].Icon.IconGuid);
        Assert.IsNull(area.Sent[^1].Icon.IconGuid);
    }

    // At sign-in Explorer may not be there yet; TaskbarCreated or the next tip adds it.
    [TestMethod]
    public void AnIconNotAddedIsTriedAgainWithTheNextTip()
    {
        var area = new StandInArea { Refuses = _ => true };
        var icon = new NotifyIcon(area, Window, Image, Debug);
        icon.Show("Pitboard");
        area.Refuses = _ => false;

        icon.Show("Pitboard, work 42%");

        Assert.AreEqual("Add, Add, SetVersion", Said(area));
    }

    [TestMethod]
    public void TheIconGoesWithPitboard()
    {
        var area = new StandInArea();
        var icon = new NotifyIcon(area, Window, Image, Release);
        icon.Show("Pitboard");

        icon.Remove();
        icon.Remove();

        Assert.AreEqual("Add, SetVersion, Delete", Said(area));
        Assert.AreEqual(Release, area.Sent[^1].Icon.IconGuid);
    }

    [TestMethod]
    public void FocusGoesBackToTheIconOnlyOnceItIsThere()
    {
        var area = new StandInArea { Refuses = _ => true };
        var icon = new NotifyIcon(area, Window, Image, Debug);
        icon.Show("Pitboard");
        icon.ReturnFocus();
        area.Refuses = _ => false;
        icon.Readd();

        icon.ReturnFocus();

        Assert.AreEqual("Add, Add, SetVersion, SetFocus", Said(area));
    }

    internal sealed class StandInArea : INotificationArea
    {
        public List<(IconMessage Message, IconData Icon)> Sent { get; } = [];

        public Func<IconData, bool> Refuses { get; set; } = _ => false;

        public bool Send(IconMessage message, IconData icon)
        {
            Sent.Add((message, icon));
            return message is not IconMessage.Add || !Refuses(icon);
        }
    }
}
