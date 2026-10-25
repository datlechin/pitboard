using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.ApplicationModel.DataTransfer;

namespace Pitboard.Views;

// An account's context menu, in the macOS order: its offers, its windows, copying its address, then forgetting it.
// Microsoft's menu guidance leaves out what does not apply now, where the model dims it for the macOS menu bar.
public static class AccountMenu
{
    public static MenuFlyout For(
        AccountItem item, Action<Question?, Intent> ask, Action<WindowAccount>? open = null, IReadOnlyCollection<string>? held = null)
    {
        var menu = new MenuFlyout();
        foreach (var offer in item.Offers.Where(offer => offer.Enabled))
        {
            menu.Items.Add(Entry(offer, ask));
        }

        if (open is not null && item.Windows.Length > 0)
        {
            Separate(menu);
            foreach (var offer in item.Windows)
            {
                menu.Items.Add(Window(offer, open, held ?? []));
            }
        }

        if (item.Email.Length > 0)
        {
            Separate(menu);
            var copy = new MenuFlyoutItem { Text = "Copy email address", KeyboardAcceleratorTextOverride = "Ctrl+C" };
            AutomationProperties.SetAutomationId(copy, "account.copyEmail");
            copy.Click += (_, _) =>
            {
                var package = new DataPackage();
                package.SetText(item.Email);
                Clipboard.SetContent(package);
            };
            menu.Items.Add(copy);
        }

        if (item.Forget is { Enabled: true } forget)
        {
            Separate(menu);
            var entry = Entry(forget, ask);
            entry.KeyboardAcceleratorTextOverride = "Del";
            menu.Items.Add(entry);
        }

        return menu;
    }

    private static MenuFlyoutItem Entry(ItemOffer offer, Action<Question?, Intent> ask)
    {
        var entry = new MenuFlyoutItem
        {
            Text = offer.Title,
            KeyboardAcceleratorTextOverride = AccountKeys.Renames(offer) ? "F2" : "",
        };
        AutomationProperties.SetAutomationId(entry, $"account.{Identifiers.Of(offer.Intent)}");
        entry.Click += (_, _) => ask(offer.Confirm, offer.Intent);
        return entry;
    }

    private static MenuFlyoutItem Window(WindowOffer offer, Action<WindowAccount> open, IReadOnlyCollection<string> held)
    {
        var entry = new MenuFlyoutItem { Text = offer.Title };
        AutomationProperties.SetAutomationId(entry, IconMenu.WindowId(offer.Window));
        if (HeldStores.Note(offer.Window, held) is { } note)
        {
            entry.IsEnabled = false;
            ToolTipService.SetToolTip(entry, note);
            AutomationProperties.SetHelpText(entry, note);
        }

        entry.Click += (_, _) => open(offer.Window);
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
