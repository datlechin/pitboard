using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Windows;

namespace Pitboard.Views;

// The notification-area icon's menu. It opens from a window one pixel wide, so it draws outside it.
public static class IconMenuFlyout
{
    public static MenuFlyout For(IReadOnlyList<IconMenuEntry> entries, Action<IconCommand> chosen)
    {
        var menu = new MenuFlyout { ShouldConstrainToRootBounds = false };
        foreach (var entry in entries)
        {
            menu.Items.Add(Item(entry, chosen));
        }

        return menu;
    }

    // WinUI pads a row narrow when a mouse, pen or keyboard opened its menu, and wider otherwise. Explorer's
    // WM_CONTEXTMENU says nothing of the device, so each row takes the narrow padding whatever opened the menu before.
    private static MenuFlyoutItemBase Item(IconMenuEntry entry, Action<IconCommand> chosen)
    {
        var item = Made(entry, chosen);
        if (item is not MenuFlyoutSeparator
            && Application.Current.Resources.TryGetValue("MenuFlyoutItemThemePaddingNarrow", out var narrow) && narrow is Thickness padding)
        {
            item.Padding = padding;
        }

        return item;
    }

    private static MenuFlyoutItemBase Made(IconMenuEntry entry, Action<IconCommand> chosen)
    {
        switch (entry)
        {
            case IconMenuEntry.Heading heading:
                // A menu has no heading of its own, so a tool's name is an item nobody can choose.
                var line = new MenuFlyoutItem { Text = heading.Text, IsEnabled = false };
                AutomationProperties.SetHeadingLevel(line, AutomationHeadingLevel.Level1);
                return line;
            case IconMenuEntry.Account account:
                var radio = new RadioMenuFlyoutItem
                {
                    Text = account.Text,
                    GroupName = account.Group,
                    IsChecked = account.InUse,
                    IsEnabled = account.Enabled,
                };
                if (account.Id is { } accountId)
                {
                    AutomationProperties.SetAutomationId(radio, accountId);
                }

                if (account.Detail is { } detail)
                {
                    ToolTipService.SetToolTip(radio, detail);
                    AutomationProperties.SetHelpText(radio, detail);
                }

                return Chosen(radio, account.Does, chosen);
            case IconMenuEntry.Command command:
                var item = new MenuFlyoutItem { Text = command.Text };
                if (command.Default)
                {
                    item.FontWeight = FontWeights.SemiBold;
                }

                if (command.Id is { } id)
                {
                    AutomationProperties.SetAutomationId(item, id);
                }

                return Chosen(item, command.Does, chosen);
            case IconMenuEntry.Submenu submenu:
                var sub = new MenuFlyoutSubItem { Text = submenu.Text };
                if (submenu.Id is { } submenuId)
                {
                    AutomationProperties.SetAutomationId(sub, submenuId);
                }

                foreach (var inner in submenu.Entries)
                {
                    sub.Items.Add(Item(inner, chosen));
                }

                return sub;
            case IconMenuEntry.Separator:
                return new MenuFlyoutSeparator();
            default:
                throw new ArgumentOutOfRangeException(nameof(entry), entry, "an entry the menu cannot show");
        }
    }

    private static MenuFlyoutItem Chosen(MenuFlyoutItem item, IconCommand? does, Action<IconCommand> chosen)
    {
        if (does is not null)
        {
            item.Click += (_, _) => chosen(does);
        }

        return item;
    }
}
