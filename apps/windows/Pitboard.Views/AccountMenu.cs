using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Core;
using Pitboard.Windows;

namespace Pitboard.Views;

// An account's context menu, in the macOS order: what it does to the account, its windows, copying its address, then
// removing it. Microsoft's menu guidance leaves out what does not apply now, where the model dims it for the macOS menu bar.
public static class AccountMenu
{
    public static MenuFlyout For(
        AccountItem item, Action<Question?, Intent> ask, Action<string> copy, Action<WindowAccount>? open = null,
        IReadOnlyCollection<string>? held = null)
    {
        var menu = new MenuFlyout();
        foreach (var action in item.Actions.Where(action => action is { Enabled: true, Kind: not (ActionKind.OpenWindow or ActionKind.Remove) }))
        {
            menu.Items.Add(Entry(action, ask));
        }

        var windows = item.Actions.Where(action => action is { Kind: ActionKind.OpenWindow, Window: not null }).ToArray();
        if (open is not null && windows.Length > 0)
        {
            Separate(menu);
            foreach (var action in windows)
            {
                menu.Items.Add(Window(action.Title, action.Window!, open, held ?? []));
            }
        }

        if (item.Email.Length > 0)
        {
            Separate(menu);
            var entry = new MenuFlyoutItem { Text = Words.Menu.CopyEmail, AccessKey = "C", KeyboardAcceleratorTextOverride = "Ctrl+C" };
            AutomationProperties.SetAutomationId(entry, "account.copyEmail");
            entry.Click += (_, _) => copy(item.Email);
            menu.Items.Add(entry);
        }

        if (item.Actions.FirstOrDefault(action => action is { Kind: ActionKind.Remove, Enabled: true }) is { } remove)
        {
            Separate(menu);
            menu.Items.Add(Entry(remove, ask));
        }

        return menu;
    }

    // Each kind's letter apart from the others it can share a menu with; Rename takes Explorer's M.
    private static string AccessKeyOf(ActionKind kind) => kind switch
    {
        ActionKind.Switch => "S",
        ActionKind.SignInAgain => "I",
        ActionKind.Add => "A",
        ActionKind.Rename => "M",
        ActionKind.WarmUpNow => "W",
        ActionKind.OpenWindow => "O",
        ActionKind.Remove => "R",
        _ => "",
    };

    private static MenuFlyoutItem Entry(AccountAction action, Action<Question?, Intent> ask)
    {
        var entry = new MenuFlyoutItem
        {
            Text = action.Title,
            AccessKey = AccessKeyOf(action.Kind),
            KeyboardAcceleratorTextOverride = AccountKeys.Shown(action.Kind),
        };
        if (action.Intent is { } intent)
        {
            AutomationProperties.SetAutomationId(entry, $"account.{Identifiers.Of(intent)}");
            entry.Click += (_, _) => ask(action.Confirm, intent);
        }

        return entry;
    }

    private static MenuFlyoutItem Window(string title, WindowAccount window, Action<WindowAccount> open, IReadOnlyCollection<string> held)
    {
        var entry = new MenuFlyoutItem { Text = title, AccessKey = AccessKeyOf(ActionKind.OpenWindow) };
        AutomationProperties.SetAutomationId(entry, IconMenu.WindowId(window));
        if (HeldStores.Note(window, held) is { } note)
        {
            entry.IsEnabled = false;
            ToolTipService.SetToolTip(entry, note);
            AutomationProperties.SetHelpText(entry, note);
        }

        entry.Click += (_, _) => open(window);
        return entry;
    }

    private static void Separate(MenuFlyout menu)
    {
        if (menu.Items.Count > 0)
        {
            menu.Items.Add(new MenuFlyoutSeparator());
        }
    }
}
