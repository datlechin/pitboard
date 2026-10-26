using Microsoft.UI.Xaml.Controls;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class NoticeBarTests
{
    private const long At = 1_800_000_000;

    // Between snapshots too, as the macOS app's timer text does.
    [UITestMethod]
    public async Task ASwitchNoticeCountsDownToWhenOpenSessionsFollow()
    {
        var clock = new Clock(DateTimeOffset.FromUnixTimeSeconds(At));
        var notice = Items.Notice("switch/codex", Items.Dismiss("codex")) with
        {
            Until = At + 125,
            UntilLabel = "Sessions already open follow in",
        };
        var bar = await Views.Shown(new NoticeBar(notice, _ => { }, clock));

        Assert.Contains("Sessions already open follow in 2:05", Views.Texts(bar));
        clock.Now = DateTimeOffset.FromUnixTimeSeconds(At + 200);

        await Views.Until(
            () => Views.Texts(bar).Contains("Sessions already open follow in 0:00"), "the countdown did not reach nought");
    }

    [UITestMethod]
    public async Task ItsButtonsAreUnderItsMessageAndItsDismissalIsTheCloseButton()
    {
        var performed = new List<NoticeAction>();
        var giveUp = new NoticeAction(
            "Give up…", new Intent.AbandonStuckSwitch(), Dismisses: false, Switches: false, Enabled: true,
            Confirm: new Question("Give up on the interrupted switch?", "Both logins are kept.", "Give up", Destructive: false, ConfirmIsDefault: true));
        var dismiss = Items.Dismiss("codex");
        var bar = await Views.Shown(new NoticeBar(Items.Notice("stuck", giveUp, dismiss), performed.Add, TimeProvider.System));

        Views.Press(Views.Button(bar, "Give up…"));
        Assert.IsFalse(Views.Visible<Button>(bar).Any(button => button.Content as string == "Dismiss"), "Dismiss as a button");
        var close = Views.Descendants(bar).OfType<Button>().Single(button => button.Name == "CloseButton");
        Views.Press(close);

        Assert.AreEqual("Give up…, Dismiss", string.Join(", ", performed.Select(action => action.Title)));
        Assert.IsTrue(Views.Element<InfoBar>(bar, "notice.stuck").IsOpen, "the notice went before the model took it away");
    }

    [UITestMethod]
    public async Task ANoticeWithNothingToDismissHasNoCloseButton()
    {
        var bar = await Views.Shown(new NoticeBar(Items.Notice("read"), _ => { }, TimeProvider.System));

        Assert.IsFalse(Views.Element<InfoBar>(bar, "notice.read").IsClosable);
    }
}
