using Pitboard.Core;

namespace Pitboard.Windows;

// What the icon's commands reach beyond the model and the updater.
public interface IIconTargets
{
    void ShowMenu(IReadOnlyList<IconMenuEntry> entries, IconEvent.Menu at, Action opened, Action<IconCommand> chosen);

    void OpenLink(string url);

    void OpenWindow(WindowAccount window);

    void ShowSettings();

    Task<bool> MayQuit();

    void Quit();
}

// A click on the icon opens the window, a right click its menu, and a choice there does what it says.
public sealed class IconCommands(AppModel model, IUpdates updates, IIconTargets targets)
{
    public void Heard(IconEvent happened)
    {
        switch (happened)
        {
            case IconEvent.Activated:
                model.Send(new Intent.Show(Place: null));
                break;
            case IconEvent.Menu at:
                targets.ShowMenu(
                    IconMenu.Entries(model, updates), at, () => model.Send(new Intent.Glanced()),
                    async command => await Perform(command));
                break;
        }
    }

    public async Task Perform(IconCommand command)
    {
        switch (command)
        {
            case IconCommand.Send send:
                model.Send(send.Intent);
                break;
            case IconCommand.OpenLink link:
                targets.OpenLink(link.Url);
                break;
            case IconCommand.OpenWindow open:
                targets.OpenWindow(open.Window);
                break;
            case IconCommand.ShowSettings:
                targets.ShowSettings();
                break;
            case IconCommand.CheckForUpdates:
                updates.Check();
                break;
            // Quitting stops every download still running, so it asks first, as the macOS app does.
            case IconCommand.Quit:
                if (await targets.MayQuit())
                {
                    targets.Quit();
                }

                break;
        }
    }
}
