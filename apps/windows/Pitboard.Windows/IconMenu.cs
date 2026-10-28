using Pitboard.Core;

namespace Pitboard.Windows;

public abstract record IconCommand
{
    private IconCommand()
    {
    }

    public sealed record Send(Intent Intent) : IconCommand;

    public sealed record OpenLink(string Url) : IconCommand;

    public sealed record OpenWindow(WindowAccount Window) : IconCommand;

    public sealed record ShowSettings : IconCommand;

    public sealed record CheckForUpdates : IconCommand;

    public sealed record Quit : IconCommand;
}

public abstract record IconMenuEntry
{
    private IconMenuEntry()
    {
    }

    public sealed record Command(string Text, IconCommand Does) : IconMenuEntry
    {
        // What a click on the icon does, which a Windows context menu shows in bold.
        public bool Default { get; init; }

        // What UI Automation finds it by.
        public string? Id { get; init; }
    }

    // Choosing it does what its row's button does; the one in use stays in use until another is chosen. Detail is its
    // tooltip and its help text.
    public sealed record Account(string Text, string Group, bool InUse, IconCommand? Does, string? Detail) : IconMenuEntry
    {
        public bool Enabled => InUse || Does is not null;

        public string? Id { get; init; }
    }

    // A tool's name over its accounts.
    public sealed record Heading(string Text) : IconMenuEntry;

    public sealed record Submenu(string Text, IReadOnlyList<IconMenuEntry> Entries) : IconMenuEntry
    {
        public string? Id { get; init; }
    }

    public sealed record Separator : IconMenuEntry;
}

// The notification-area icon's menu, in the order Microsoft gives one: Open first, Exit last, and what does not apply
// left out rather than dimmed. What needs attention is in the window, which the icon's warning points to.
public static class IconMenu
{
    public const string HelpLink = "https://docs.usepitboard.com";

    public static IReadOnlyList<IconMenuEntry> Entries(AppModel model, IUpdates updates)
    {
        var words = Words.Menu;
        List<IconMenuEntry>[] groups =
        [
            [
                new IconMenuEntry.Command(words.OpenPitboard, new IconCommand.Send(new Intent.Show(Place: null)))
                {
                    Default = true,
                    Id = "menu.openPitboard",
                },
            ],
            Accounts(model.Sections),
            [
                new IconMenuEntry.Command(words.AddAccount, new IconCommand.Send(new Intent.PresentSheet(new Sheet.Add(Tool: null))))
                {
                    Id = "menu.addAccount",
                },
            ],
            Sites(model.AccountWindows),
            [
                new IconMenuEntry.Command(words.Settings, new IconCommand.ShowSettings()) { Id = "menu.settings" },
                .. Updates(updates),
                new IconMenuEntry.Command(words.Help, new IconCommand.OpenLink(HelpLink)) { Id = "menu.help" },
            ],
            [new IconMenuEntry.Command(words.Quit, new IconCommand.Quit()) { Id = "menu.exit" }],
        ];

        var entries = new List<IconMenuEntry>();
        foreach (var group in groups.Where(group => group.Count > 0))
        {
            if (entries.Count > 0)
            {
                entries.Add(new IconMenuEntry.Separator());
            }

            entries.AddRange(group);
        }

        return entries;
    }

    // Each tool's accounts under its name, and one tool's unheaded, as the window shows them.
    private static List<IconMenuEntry> Accounts(AccountSection[] sections)
    {
        var entries = new List<IconMenuEntry>();
        foreach (var section in sections)
        {
            if (section.Heading is { } heading)
            {
                entries.Add(new IconMenuEntry.Heading(heading));
            }

            entries.AddRange(section.Accounts.Select(item => new IconMenuEntry.Account(
                item.MenuTitle,
                section.Id,
                item.InUse,
                item.Action is { } action ? new IconCommand.Send(action.Intent) : null,
                Lines(item.Summary, item.Help))
            {
                Id = $"menu.account.{item.Qualified ?? item.Id}",
            }));
        }

        return entries;
    }

    // An update found is offered by its version, on a beta too, which finds no later beta and so offers no check.
    private static List<IconMenuEntry> Updates(IUpdates updates)
    {
        if (!updates.Available)
        {
            return [];
        }

        if (updates.Waiting is { } waiting)
        {
            return [new IconMenuEntry.Command(PitboardModelMethods.InstallUpdate(waiting), new IconCommand.CheckForUpdates()) { Id = "menu.update" }];
        }

        return updates.Beta
            ? []
            : [new IconMenuEntry.Command(Words.Menu.CheckForUpdates, new IconCommand.CheckForUpdates()) { Id = "menu.checkForUpdates" }];
    }

    // An account's window entry: its title is the account's label, as the account's own entry is.
    public static string WindowId(WindowAccount window) => $"window.{window.Site.Host}/{window.Label}";

    // A site with one account's window is an item, and one with several a submenu of their titles. A window whose store
    // is still held opens nothing until it is let go, so it is left out until then.
    private static List<IconMenuEntry> Sites(AccountWindowsShown windows)
    {
        var entries = new List<IconMenuEntry>();
        foreach (var menu in windows.Menus)
        {
            switch (menu)
            {
                case SiteMenu.One one when Opens(one.Window):
                    entries.Add(Window(one.Title, one.Window));
                    break;
                case SiteMenu.Several several when several.Windows.Where(Opens).ToArray() is { Length: > 0 } opening:
                    entries.Add(new IconMenuEntry.Submenu(several.Title, [.. opening.Select(window => Window(window.Title, window))])
                    {
                        Id = $"menu.site.{several.Site.Host}",
                    });
                    break;
            }
        }

        return entries;

        bool Opens(WindowAccount window) => HeldStores.Note(window, windows.Held) is null;
    }

    private static IconMenuEntry.Command Window(string text, WindowAccount window) =>
        new(text, new IconCommand.OpenWindow(window)) { Id = WindowId(window) };

    private static string? Lines(params string?[] lines)
    {
        var said = lines.Where(line => !string.IsNullOrEmpty(line)).ToArray();
        return said.Length > 0 ? string.Join("\n", said) : null;
    }
}
