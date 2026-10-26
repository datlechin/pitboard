using System.Reflection;

namespace Pitboard.Windows;

// Pitboard's own version, as the settings' About and Updates and the updater read it.
public static class Versions
{
    // The .NET SDK adds the commit to an assembly's informational version after a plus.
    public static string Of(Assembly assembly) =>
        Of(assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion);

    public static string Of(string? informational) => (informational ?? "").Split('+')[0];

    public static bool IsBeta(string version) => version.Contains('-', StringComparison.Ordinal);
}
