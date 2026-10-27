using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class DialogHostTests
{
    // Microsoft's contract for ContentDialog, which is why the window's dialogs wait their turn.
    [UITestMethod]
    public async Task OnlyOneContentDialogShowsAtATime()
    {
        var content = await Views.Shown(new Grid());
        var first = new ContentDialog { XamlRoot = content.XamlRoot, Title = "first", CloseButtonText = "OK" };
        var second = new ContentDialog { XamlRoot = content.XamlRoot, Title = "second", CloseButtonText = "OK" };
        var shown = first.ShowAsync();
        try
        {
            var refused = await Assert.ThrowsAsync<Exception>(
                () => second.ShowAsync().AsTask().WaitAsync(TimeSpan.FromSeconds(5)));
            Assert.IsNotInstanceOfType<TimeoutException>(refused, "the second dialog waited instead of being refused");
        }
        finally
        {
            second.Hide();
            first.Hide();
            await shown;
        }
    }

    [UITestMethod]
    public async Task TheWindowsDialogsWaitTheirTurn()
    {
        var content = await Views.Shown(new Grid());
        var dialogs = new DialogHost(() => content.XamlRoot);
        var first = new ContentDialog { Title = "first", CloseButtonText = "OK" };
        var second = new ContentDialog { Title = "second", CloseButtonText = "OK" };

        var firstShown = dialogs.Show(first);
        var secondShown = dialogs.Show(second);
        await Views.Until(() => dialogs.Open == first, "the first dialog did not show");
        first.Hide();
        await Views.Until(() => dialogs.Open == second, "the second dialog did not follow the first");
        second.Hide();

        Assert.AreEqual(ContentDialogResult.None, await firstShown);
        Assert.AreEqual(ContentDialogResult.None, await secondShown);
    }

    // As an account window closes: its question goes, and one waiting its turn never shows.
    [UITestMethod]
    public async Task AWithdrawnHostsDialogGoesAndNoneWaitingShows()
    {
        var content = await Views.Shown(new Grid());
        var dialogs = new DialogHost(() => content.XamlRoot);
        var first = new ContentDialog { Title = "first", CloseButtonText = "OK" };
        var second = new ContentDialog { Title = "second", CloseButtonText = "OK" };
        var firstShown = dialogs.Show(first);
        var secondShown = dialogs.Show(second);
        await Views.Until(() => dialogs.Open == first, "the first dialog did not show");

        dialogs.Withdraw();

        Assert.AreEqual(ContentDialogResult.None, await firstShown);
        Assert.IsNull(await secondShown);
    }

    // An account window's question waits for a sheet over the main window, and the other way round.
    [UITestMethod]
    public async Task EachWindowsDialogsWaitForTheOthers()
    {
        var content = await Views.Shown(new Grid());
        var main = new DialogHost(() => content.XamlRoot);
        var account = new DialogHost(() => content.XamlRoot);
        var first = new ContentDialog { Title = "first", CloseButtonText = "OK" };
        var second = new ContentDialog { Title = "second", CloseButtonText = "OK" };

        var firstShown = main.Show(first);
        var secondShown = account.Show(second);
        await Views.Until(() => main.Open == first, "the first dialog did not show");
        Assert.IsNull(account.Open, "the second dialog showed over the first");
        first.Hide();
        await Views.Until(() => account.Open == second, "the second dialog did not follow the first");
        second.Hide();

        Assert.AreEqual(ContentDialogResult.None, await firstShown);
        Assert.AreEqual(ContentDialogResult.None, await secondShown);
    }

    // Microsoft's dialog guidance draws the default button in the accent colour, Enter presses it and the focus opens on it.
    // Uninstall Pitboard's question makes Cancel its default, so it is read first.
    [UITestMethod]
    public async Task AQuestionsVerbIsItsDefaultButtonWhereTheModelSays()
    {
        var content = await Views.Shown(new Grid());
        var dialogs = new DialogHost(() => content.XamlRoot);
        foreach (var (confirmIsDefault, expected) in new[] { (true, ContentDialogButton.Primary), (false, ContentDialogButton.Close) })
        {
            var asked = dialogs.Confirm(new Question(
                "Remove “spare”?", "Pitboard deletes its saved sign-in, so using it again needs a sign-in in your browser.", "Remove",
                Destructive: true, ConfirmIsDefault: confirmIsDefault));
            await Views.Until(() => dialogs.Open is not null, "the question did not show");
            var shown = dialogs.Open!;

            Assert.AreEqual(expected, shown.DefaultButton, $"confirm is default: {confirmIsDefault}");
            Assert.AreEqual("Remove", shown.PrimaryButtonText);
            Assert.AreEqual("Cancel", shown.CloseButtonText);
            shown.Hide();
            Assert.IsFalse(await asked);
        }
    }

    // A switch to dark while Pitboard ran left the Add account sheet light under white text.
    [UITestMethod]
    public async Task ADialogTakesItsWindowsThemeAndFollowsIt()
    {
        var other = Application.Current.RequestedTheme == ApplicationTheme.Dark ? ElementTheme.Light : ElementTheme.Dark;
        var content = await Views.Shown(new Grid { RequestedTheme = other });
        var dialogs = new DialogHost(() => content.XamlRoot);
        var dialog = new ContentDialog { Title = "Add account", Content = "Sign in as the account you’re adding.", CloseButtonText = "Cancel" };
        var shown = dialogs.Show(dialog);
        try
        {
            await Views.Until(() => dialogs.Open == dialog && dialog.IsLoaded, "the dialog did not show");
            Assert.AreEqual(other, dialog.ActualTheme, "the dialog took the app's theme, not its window's");
            Readable(dialog);

            content.RequestedTheme = other == ElementTheme.Dark ? ElementTheme.Light : ElementTheme.Dark;

            await Views.Until(() => dialog.ActualTheme == content.ActualTheme, "the dialog kept the theme it opened in");
            Readable(dialog);
        }
        finally
        {
            dialog.Hide();
            await shown;
        }
    }

    // Light text on a dark ground in the dark theme, and dark on light in the light one.
    private static void Readable(ContentDialog dialog)
    {
        static double Lightness(Brush brush)
        {
            var color = ((SolidColorBrush)brush).Color;
            return (0.2126 * color.R) + (0.7152 * color.G) + (0.0722 * color.B);
        }

        var text = Lightness(dialog.Foreground);
        var ground = Lightness(dialog.Background);
        Assert.AreEqual(
            dialog.ActualTheme == ElementTheme.Dark, text > ground,
            $"{dialog.ActualTheme} dialog: text {text:0}, ground {ground:0}");
    }
}
