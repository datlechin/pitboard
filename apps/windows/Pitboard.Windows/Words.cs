using Pitboard.Core;

namespace Pitboard.Windows;

// The model's words for what the views draw on their own, read once: app_words() says the same every time.
public static class Words
{
    private static readonly AppWords All = PitboardModelMethods.AppWords(Identity.Front);

    // What only the Windows app draws.
    public static WindowsNotificationAreaWords Own { get; } = PitboardModelMethods.WindowsNotificationAreaWords();

    public static string AppName => All.AppName;

    public static MenuWords Menu => All.Menu;

    public static WindowWords Window => All.Window;

    public static ActivityWords Activity => All.Activity;

    public static SettingsWords Settings => All.Settings;

    public static SheetWords Sheet => All.Sheet;

    public static AccountWindowWords AccountWindow => All.AccountWindow;
}
