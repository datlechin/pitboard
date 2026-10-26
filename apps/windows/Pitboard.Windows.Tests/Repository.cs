namespace Pitboard.Windows.Tests;

// The checkout these tests were built in and run from, for what they read of the model's and the views' sources.
internal static class Repository
{
    public static string Root { get; } = Find();

    public static string Path(params string[] parts) => System.IO.Path.Combine([Root, .. parts]);

    private static string Find()
    {
        for (var folder = new DirectoryInfo(AppContext.BaseDirectory); folder is not null; folder = folder.Parent)
        {
            if (File.Exists(System.IO.Path.Combine(folder.FullName, "Cargo.toml")) && Directory.Exists(System.IO.Path.Combine(folder.FullName, "crates")))
            {
                return folder.FullName;
            }
        }

        throw new InvalidOperationException($"no checkout above {AppContext.BaseDirectory}");
    }
}
