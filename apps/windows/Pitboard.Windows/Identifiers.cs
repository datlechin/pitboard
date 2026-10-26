using Pitboard.Core;

namespace Pitboard.Windows;

// What UI Automation finds a control by: the model's case it shows or sends, "switchTo", as the macOS app names it.
public static class Identifiers
{
    // An intent that puts up a sheet is named by the sheet: "rename", not "presentSheet".
    public static string Of(object shown)
    {
        if (shown is Intent.PresentSheet put)
        {
            return Of(put.Sheet);
        }

        var name = shown is Enum ? shown.ToString()! : shown.GetType().Name;
        return char.ToLowerInvariant(name[0]) + name[1..];
    }
}
