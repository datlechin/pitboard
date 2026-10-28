using Pitboard.Core;
using Windows.Win32;
using Windows.Win32.Foundation;

namespace Pitboard.Windows;

public enum Start
{
    Run,
    Redirect,
    Dropped,
    Refused,
    LinkRefused,
    Unregister,
    UnregisterShare,
}

// What a second launch hands the running copy: its Pitboard link, its Jump List task, or neither, which brings the
// window forward.
public abstract record Handover
{
    private Handover()
    {
    }

    public sealed record Window : Handover;

    public sealed record Link(string Text) : Handover;

    public sealed record Jump(JumpAsk Ask) : Handover;
}

public static class Launching
{
    public const string Unregister = "--unregister";

    public const string UnregisterShare = "--unregister-share";

    public const string Updated = "--updated";

    // What the Run value starts Pitboard with, as Windows signs the person in.
    public const string SignIn = "--sign-in";

    // What COM starts Pitboard.exe with for a click on its notification, as Register writes it.
    public const string Notified = "----AppNotificationActivated:";

    // What a Jump List task starts Pitboard.exe with, each id after its flag.
    public const string SwitchTo = "--switch-to";

    public const string AddAccount = "--add-account";

    public const string OpenWindow = "--open-window";

    // How many times a launch asks for the copy holding the key.
    public const int Asks = 20;

    // A link launch reads no flag: Microsoft's guide to URI schemes warns that a link's unencoded quotes or spaces split
    // it into more arguments, which another app can make look like Pitboard's own. holdsKey answers null where no copy
    // could be found holding the key.
    public static Start Decide(
        IReadOnlyCollection<string> arguments, Identity running, string? fixtureFolder, Func<ReadFailure?> refusal,
        Func<bool?> holdsKey)
    {
        var link = Link(arguments, running.LinkScheme);
        var given = link is null ? arguments.ToHashSet(StringComparer.OrdinalIgnoreCase) : [];
        if (given.Contains(Unregister))
        {
            return Start.Unregister;
        }

        if (given.Contains(UnregisterShare))
        {
            return Start.UnregisterShare;
        }

        if (refusal() is not null)
        {
            return Start.Refused;
        }

        if (link is not null && !running.OpensLinks(fixtureFolder))
        {
            return Start.LinkRefused;
        }

        switch (holdsKey())
        {
            case true:
                return Start.Run;
            case null:
                return Start.Dropped;
        }

        // A click's arguments reach only the copy that registered, and nobody asked for the other two.
        return Unasked(given) ? Start.Dropped : Start.Redirect;
    }

    // Windows App SDK's FindOrRegisterForKey answers null where another copy holds the key's mutex but none it lists
    // has the key, as for one of two launches at once (docs/facts/windows-app.md). Asked again, it finds that copy, or
    // takes the key once that copy has quit.
    public static T? Holder<T>(Func<T?> findOrRegister, Action pause)
        where T : class
    {
        for (var asked = 1; asked < Asks; asked++)
        {
            if (findOrRegister() is { } holder)
            {
                return holder;
            }

            pause();
        }

        return findOrRegister();
    }

    // A Jump List task opens what it asks for, and no window of its own.
    public static bool OpensWindow(IReadOnlyList<string> arguments) =>
        !Unasked(arguments.ToHashSet(StringComparer.OrdinalIgnoreCase)) && Asked(arguments) is null;

    // The MSI's command passes a Pitboard link as its first argument, which the model reads as strictly as any link.
    public static string? Link(IEnumerable<string> arguments, string scheme) =>
        arguments.FirstOrDefault() is { } first && first.StartsWith($"{scheme}:", StringComparison.OrdinalIgnoreCase)
            ? first
            : null;

    public static string[] Of(JumpAsk ask) => ask switch
    {
        JumpAsk.Switch switching => [SwitchTo, switching.Account],
        JumpAsk.AddAccount => [AddAccount],
        JumpAsk.OpenWindow window => [OpenWindow, window.Store],
        _ => throw new ArgumentOutOfRangeException(nameof(ask)),
    };

    // A task's flag alone, and its id a GUID, as the model gives every one: anything else is no task.
    public static JumpAsk? Asked(IReadOnlyList<string> arguments) => arguments switch
    {
        [var flag] when Named(flag, AddAccount) => new JumpAsk.AddAccount(),
        [var flag, var id] when Named(flag, SwitchTo) && Guid.TryParse(id, out var account) =>
            new JumpAsk.Switch(account.ToString()),
        [var flag, var id] when Named(flag, OpenWindow) && Guid.TryParse(id, out var store) =>
            new JumpAsk.OpenWindow(store.ToString()),
        _ => null,
    };

    // Windows App SDK hands the running copy an unpackaged launch's whole command line, its program first. A Debug build
    // outside a fixture runs on whatever homes its launch has, so it takes neither a link nor a task.
    public static Handover Handed(string commandLine, Identity running, string? fixtureFolder)
    {
        var arguments = Arguments(commandLine).Skip(1).ToArray();
        if (!running.OpensLinks(fixtureFolder))
        {
            return new Handover.Window();
        }

        if (Link(arguments, running.LinkScheme) is { } link)
        {
            return new Handover.Link(link);
        }

        return Asked(arguments) is { } ask ? new Handover.Jump(ask) : new Handover.Window();
    }

    private static bool Named(string given, string flag) => string.Equals(given, flag, StringComparison.OrdinalIgnoreCase);

    private static bool Unasked(HashSet<string> given) =>
        given.Contains(SignIn) || given.Contains(Updated) || given.Contains(Notified);

    internal static unsafe string[] Arguments(string commandLine)
    {
        var split = PInvoke.CommandLineToArgv(commandLine, out var count);
        if (split is null)
        {
            return [];
        }

        try
        {
            var arguments = new string[count];
            for (var index = 0; index < count; index++)
            {
                arguments[index] = split[index].ToString();
            }

            return arguments;
        }
        finally
        {
            PInvoke.LocalFree((HLOCAL)split);
        }
    }
}
