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

public static class Launching
{
    public const string Unregister = "--unregister";

    public const string UnregisterShare = "--unregister-share";

    public const string Updated = "--updated";

    // What the Run value starts Pitboard with, as Windows signs the person in.
    public const string SignIn = "--sign-in";

    // What COM starts Pitboard.exe with for a click on its notification, as Register writes it.
    public const string Notified = "----AppNotificationActivated:";

    // A link launch reads no flag: Microsoft's guide to URI schemes warns that a link's unencoded quotes or spaces split
    // it into more arguments, which another app can make look like Pitboard's own.
    public static Start Decide(
        IReadOnlyCollection<string> arguments, Identity running, string? fixtureFolder, Func<ReadFailure?> refusal,
        Func<bool> holdsKey)
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

        if (holdsKey())
        {
            return Start.Run;
        }

        // A click's arguments reach only the copy that registered, and nobody asked for the other two.
        return Unasked(given) ? Start.Dropped : Start.Redirect;
    }

    public static bool OpensWindow(IEnumerable<string> arguments) =>
        !Unasked(arguments.ToHashSet(StringComparer.OrdinalIgnoreCase));

    // The MSI's command passes a Pitboard link as its first argument, which the model reads as strictly as any link.
    public static string? Link(IEnumerable<string> arguments, string scheme) =>
        arguments.FirstOrDefault() is { } first && first.StartsWith($"{scheme}:", StringComparison.OrdinalIgnoreCase)
            ? first
            : null;

    // Windows App SDK hands the running copy an unpackaged launch's whole command line, its program first.
    public static string? Handed(string commandLine, Identity running, string? fixtureFolder) =>
        running.OpensLinks(fixtureFolder) ? Link(Arguments(commandLine).Skip(1), running.LinkScheme) : null;

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
