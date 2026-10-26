using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace Pitboard.Views;

internal static class Look
{
    public static Visibility Shown(bool shown) => shown ? Visibility.Visible : Visibility.Collapsed;

    public static T? Ancestor<T>(DependencyObject? from)
        where T : DependencyObject
    {
        for (var at = from; at is not null; at = VisualTreeHelper.GetParent(at))
        {
            if (at is T found)
            {
                return found;
            }
        }

        return null;
    }

    public static IEnumerable<T> Descendants<T>(DependencyObject from)
        where T : DependencyObject
    {
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(from); i++)
        {
            var child = VisualTreeHelper.GetChild(from, i);
            if (child is T found)
            {
                yield return found;
            }

            foreach (var below in Descendants<T>(child))
            {
                yield return below;
            }
        }
    }
}
