using System.Globalization;
using System.Reflection;
using System.Text.RegularExpressions;

namespace Pitboard.UITests;

internal static partial class Builds
{
    public static string Debug => Built("PitboardAppDebug");

    // As the MSI lays it out: CI passes the Pitboard.exe of its administrative image.
    public static string Release => Built("PitboardAppRelease");

    public static int ProductMajor() => int.Parse(ProductVersion().Split('.')[0], CultureInfo.InvariantCulture);

    public static bool ProductBeta() => ProductVersion().Contains('-', StringComparison.Ordinal);

    // The version the release gate reads, from Cargo.toml rather than the build under test.
    private static string ProductVersion()
    {
        for (var folder = new DirectoryInfo(AppContext.BaseDirectory); folder is not null; folder = folder.Parent)
        {
            var manifest = Path.Combine(folder.FullName, "Cargo.toml");
            var version = File.Exists(manifest) ? WorkspaceVersion().Match(File.ReadAllText(manifest)) : Match.Empty;
            if (version.Success)
            {
                return version.Groups[1].Value;
            }
        }

        throw new InvalidOperationException($"no workspace Cargo.toml above {AppContext.BaseDirectory}");
    }

    private static string Built(string key)
    {
        var path = typeof(Builds).Assembly.GetCustomAttributes<AssemblyMetadataAttribute>()
            .FirstOrDefault(metadata => metadata.Key == key)?.Value;
        return string.IsNullOrEmpty(path) || !File.Exists(path)
            ? throw new InvalidOperationException($"no Pitboard.exe at '{path}': publish the tests with -p:{key}=<its path>")
            : path;
    }

    [GeneratedRegex(@"^\[workspace\.package\]$[^\[]*?^version = ""([^""]+)""", RegexOptions.Multiline)]
    private static partial Regex WorkspaceVersion();
}
