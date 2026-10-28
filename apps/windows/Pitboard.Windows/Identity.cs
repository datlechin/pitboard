using Pitboard.Core;

namespace Pitboard.Windows;

public enum Build
{
    Debug,
    Release,
}

public sealed record Identity(Build Build)
{
    // Which app this is to the model, whose words and rules that differ by front follow it.
    public const Front Front = Pitboard.Core.Front.WindowsNotificationArea;

    public string Aumid => Build is Build.Debug ? "Pitboard.Pitboard.Debug" : "Pitboard.Pitboard";

    public string LinkScheme => Build is Build.Debug ? "pitboard-debug" : "pitboard";

    // A Debug build outside a fixture runs on whatever homes its launch has, which no link may reach.
    public bool OpensLinks(string? fixtureFolder) => Build is Build.Release || fixtureFolder is not null;

    public string DataFolderName => Build is Build.Debug ? "Pitboard App Debug" : "Pitboard App";

    // Explorer ties an icon's GUID to the path of the exe that added it, which only the MSI keeps fixed.
    public Guid? IconGuid => Build is Build.Release ? new Guid("786fa81f-ed43-4b15-b0cd-ab1344852e03") : null;

    public string InstanceKey(string? fixtureFolder) =>
        (Build, fixtureFolder) switch
        {
            (Build.Release, _) => "Pitboard",
            (Build.Debug, null) => "Pitboard Debug",
            (Build.Debug, var folder) => $"Pitboard Debug {folder}",
            _ => throw new ArgumentOutOfRangeException(nameof(fixtureFolder)),
        };

    // Beside Pitboard's folder, never in it: neither PITBOARD_HOME nor pitboard uninstall reaches it.
    public string DataFolder(string localAppData, string? fixtureFolder) =>
        fixtureFolder ?? Path.Combine(localAppData, DataFolderName);
}
