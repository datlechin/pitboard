using Pitboard.Core;
using Windows.System;

namespace Pitboard.Windows;

public static class AccountKeys
{
    public static (Question? Confirm, Intent Intent)? Pressed(VirtualKey key, AccountItem item) => key switch
    {
        VirtualKey.Enter when item.Action is { } action => (null, action.Intent),
        VirtualKey.Delete when item.Forget is { Enabled: true } forget => (forget.Confirm, forget.Intent),
        VirtualKey.F2 when item.Offers.FirstOrDefault(Renames) is { } rename => (rename.Confirm, rename.Intent),
        _ => null,
    };

    public static string? Copied(VirtualKey key, bool control, AccountItem item) =>
        key == VirtualKey.C && control && item.Email.Length > 0 ? item.Email : null;

    public static bool Renames(ItemOffer offer) =>
        offer is { Enabled: true, Intent: Intent.PresentSheet { Sheet: Sheet.Rename } };
}
