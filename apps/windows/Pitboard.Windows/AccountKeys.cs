using Pitboard.Core;
using Windows.System;

namespace Pitboard.Windows;

public static class AccountKeys
{
    public static (Question? Confirm, Intent Intent)? Pressed(VirtualKey key, AccountItem item) => key switch
    {
        VirtualKey.Enter when item.Action is { } action => (null, action.Intent),
        VirtualKey.Delete when Offered(item, ActionKind.Remove) is { Intent: { } intent } remove => (remove.Confirm, intent),
        VirtualKey.F2 when Offered(item, ActionKind.Rename) is { Intent: { } intent } rename => (rename.Confirm, intent),
        _ => null,
    };

    public static string? Copied(VirtualKey key, bool control, AccountItem item) =>
        key == VirtualKey.C && control && item.Email.Length > 0 ? item.Email : null;

    // What a shortcut menu shows beside an item its key does too.
    public static string Shown(ActionKind kind) => kind switch
    {
        ActionKind.Rename => "F2",
        ActionKind.Remove => "Del",
        _ => "",
    };

    private static AccountAction? Offered(AccountItem item, ActionKind kind) =>
        item.Actions.FirstOrDefault(action => action.Kind == kind && action.Enabled);
}
