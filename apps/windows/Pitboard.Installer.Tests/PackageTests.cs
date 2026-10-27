using System.Globalization;
using WixToolset.Dtf.WindowsInstaller;
using WindowsInstaller = WixToolset.Dtf.WindowsInstaller.Installer;

namespace Pitboard.Installer.Tests;

[TestClass]
public sealed class PackageTests
{
    // One for the x64 and the ARM64 MSI, so an install of either replaces the other: CI's two legs each check theirs.
    private const string UpgradeCode = "{9D967FF2-F54D-4D63-9B26-51D7E75F62FC}";

    private const string FullUninstall = "REMOVE = \"ALL\" AND NOT UPGRADINGPRODUCTCODE";

    private const int Deferred = 0x400;
    private const int Rollback = 0x500;
    private const int NoImpersonate = 0x800;
    private const int IgnoreExitCode = 0x40;
    private const int AsyncNoWait = 0xC0;
    private const int ExeFromInstalledFile = 18;

    private static readonly string[] UsersRegistryRoots = ["-1", "1"];

    private static Database Open() => new(Built.Msi, DatabaseOpenMode.ReadOnly);

    private static List<string?[]> Rows(Database database, string query, int columns)
    {
        using var view = database.OpenView(query);
        view.Execute();
        var rows = new List<string?[]>();
        foreach (var record in view)
        {
            using (record)
            {
                rows.Add(Enumerable.Range(1, columns).Select(field => record.IsNull(field) ? null : record.GetString(field)).ToArray());
            }
        }

        return rows;
    }

    private static string? Property(Database database, string name) =>
        Rows(database, $"SELECT `Value` FROM `Property` WHERE `Property` = '{name}'", 1).SingleOrDefault()?[0];

    [TestMethod]
    public void ItIsPitboardsForOneAccountWithOneUpgradeCode()
    {
        using var database = Open();

        Assert.AreEqual("Pitboard", Property(database, "ProductName"));
        Assert.AreEqual("Pitboard", Property(database, "Manufacturer"));
        Assert.AreEqual(UpgradeCode, Property(database, "UpgradeCode"));
        Assert.AreEqual("2", Property(database, "ALLUSERS"));
        Assert.AreEqual("1", Property(database, "MSIINSTALLPERUSER"));
        var conditions = Rows(database, "SELECT `Condition` FROM `LaunchCondition`", 1).Select(row => row[0]).ToList();
        CollectionAssert.Contains(conditions, "NOT ALLUSERS OR ACTION = \"ADMIN\"");
    }

    // A beta and its final upgrade each other, since Windows Installer compares three fields. check-windows-msi.ps1 holds
    // ProductVersion to windows-versions.sh.
    [TestMethod]
    public void ItReplacesTheSameVersion()
    {
        using var database = Open();
        var expected = Property(database, "ProductVersion");

        var upgrades = Rows(database, "SELECT `UpgradeCode`, `VersionMax`, `Attributes` FROM `Upgrade` WHERE `VersionMax` IS NOT NULL", 3);
        var upgrade = upgrades.Single();
        Assert.AreEqual(UpgradeCode, upgrade[0]);
        Assert.AreEqual(expected, upgrade[1]);
        const int versionMaxInclusive = 0x200;
        Assert.AreEqual(versionMaxInclusive, int.Parse(upgrade[2]!, CultureInfo.InvariantCulture) & versionMaxInclusive);
    }

    // Removed before the install, the old product left no bin\pitboard.exe for about 2 s of an upgrade on the session's
    // PC (2026-10-18), and 9 of 416 status line runs found no pitboard. Removed after it, it takes only what the new one
    // no longer has, which holds while every component keeps its GUID from one release to the next.
    [TestMethod]
    public void AnUpgradeReplacesFilesWhereTheyAreAndRemovesTheOldProductAfter()
    {
        using var database = Open();

        Assert.IsGreaterThan(Sequence(database, "InstallExecute"), Sequence(database, "RemoveExistingProducts"));
        Assert.IsLessThan(Sequence(database, "InstallFinalize"), Sequence(database, "RemoveExistingProducts"));
        // The x64 and the ARM64 MSI of one version replace each other's files of an equal version.
        Assert.AreEqual("emus", Property(database, "REINSTALLMODE"));
        var unmanaged = Rows(database, "SELECT `Component` FROM `Component` WHERE `ComponentId` IS NULL", 1);
        Assert.IsEmpty(unmanaged, string.Join(", ", unmanaged.Select(row => row[0])));
        StringAssert.EndsWith(Condition(database, "Unregister"), "AND NOT UPGRADINGPRODUCTCODE");
        if (Built.ShareSigned)
        {
            StringAssert.EndsWith(Condition(database, "UnregisterShare"), "AND NOT UPGRADINGPRODUCTCODE");
        }
    }

    [TestMethod]
    public void RestartManagerIsLeftOn()
    {
        using var database = Open();

        Assert.IsNull(Property(database, "MSIRESTARTMANAGERCONTROL"));
        Assert.IsNull(Property(database, "MSIDISABLERMRESTART"));
    }

    [TestMethod]
    public void TheCommandLineGoesOnTheUserPathAndComesOffItOnUninstall()
    {
        using var database = Open();

        var row = Rows(database, "SELECT `Name`, `Value` FROM `Environment`", 2).Single();
        var name = row[0]!;
        var flags = name[..^name.TrimStart('=', '+', '-', '!', '*').Length];
        Assert.AreEqual("PATH", name[flags.Length..], ignoreCase: true);
        StringAssert.Contains(flags, "=", "it is set on an install");
        StringAssert.Contains(flags, "-", "it goes on the uninstall");
        Assert.IsFalse(flags.Contains('*', StringComparison.Ordinal), "the machine's PATH");
        Assert.AreEqual("[~];[INSTALLFOLDER]bin", row[1]);
    }

    // Beside Pitboard.exe, and never in bin, which the user PATH reaches.
    [TestMethod]
    public void ItLaysOutTheZipsPowerShellCompletions()
    {
        using var database = Open();

        var file = Rows(
            database,
            "SELECT `File`.`FileName`, `Directory`.`DefaultDir`, `Directory`.`Directory_Parent` FROM `File`, `Component`, `Directory` " +
            "WHERE `File`.`Component_` = `Component`.`Component` AND `Component`.`Directory_` = `Directory`.`Directory` AND `File`.`File` = 'PowerShellCompletions'",
            3).Single();
        Assert.AreEqual("pitboard.ps1", file[0]!.Split('|')[^1]);
        Assert.AreEqual("completions", file[1]!.Split('|')[^1]);
        Assert.AreEqual("INSTALLFOLDER", file[2]);
        StringAssert.Contains(File.ReadAllText(Path.Combine(Built.Image, "completions", "pitboard.ps1")), "Register-ArgumentCompleter");
    }

    [TestMethod]
    public void PitboardLinksOpenTheInstalledPitboardExe()
    {
        using var database = Open();
        var rows = Rows(database, "SELECT `Root`, `Key`, `Name`, `Value` FROM `Registry`", 4);

        string? Value(string key, string? name) =>
            rows.Single(row => row[1] == key && row[2] == name)[3];

        Assert.AreEqual("URL:Pitboard", Value(@"Software\Classes\pitboard", null));
        Assert.IsNull(Value(@"Software\Classes\pitboard", "URL Protocol"));
        Assert.AreEqual("\"[INSTALLFOLDER]Pitboard.exe\" \"%1\"", Value(@"Software\Classes\pitboard\shell\open\command", null));
    }

    [TestMethod]
    public void TheStartMenuShortcutCarriesPitboardsAppUserModelId()
    {
        using var database = Open();

        var shortcut = Rows(database, "SELECT `Shortcut`, `Directory_`, `Target`, `Component_` FROM `Shortcut`", 4).Single();
        Assert.AreEqual("ProgramMenuFolder", shortcut[1]);
        Assert.AreEqual("[INSTALLFOLDER]Pitboard.exe", shortcut[2]);
        var property = Rows(database, "SELECT `Shortcut_`, `PropertyKey`, `PropVariantValue` FROM `MsiShortcutProperty`", 3).Single();
        Assert.AreEqual($"{shortcut[0]} System.AppUserModel.ID Pitboard.Pitboard", string.Join(' ', property));
        var keyPath = Rows(
            database,
            $"SELECT `Registry`.`Root` FROM `Component`, `Registry` WHERE `Component`.`Component` = '{shortcut[3]}' AND `Component`.`KeyPath` = `Registry`.`Registry`",
            1).Single();
        Assert.AreEqual("1", keyPath[0], "HKCU, since ICE57 takes a shortcut in ProgramMenuFolder for the user's");
    }

    [TestMethod]
    public void OnlyAFullUninstallUnregistersAndRemovesTheAccountWindowsData()
    {
        using var database = Open();

        var unregister = Rows(database, "SELECT `Type`, `Source`, `Target` FROM `CustomAction` WHERE `Action` = 'Unregister'", 3).Single();
        var type = int.Parse(unregister[0]!, CultureInfo.InvariantCulture);
        Assert.AreEqual(ExeFromInstalledFile, type & 0x3F);
        Assert.AreEqual(Deferred, type & Deferred);
        Assert.AreEqual(0, type & NoImpersonate, "as the person, since the core's gate refuses SYSTEM");
        Assert.AreEqual(IgnoreExitCode, type & IgnoreExitCode);
        Assert.AreEqual("PitboardExe --unregister", string.Join(' ', unregister[1..]));
        Assert.AreEqual(FullUninstall, Condition(database, "Unregister"));

        var removed = Rows(database, "SELECT `Property`, `InstallMode`, `Condition` FROM `Wix4RemoveFolderEx`", 3).Single();
        Assert.AreEqual($"PITBOARDAPPDATA 2 {FullUninstall}", string.Join(' ', removed));
        var folder = Rows(database, "SELECT `Target` FROM `CustomAction` WHERE `Source` = 'PITBOARDAPPDATA'", 1).Single();
        Assert.AreEqual("[LocalAppDataFolder]Pitboard App", folder[0]);
    }

    // Uninstall Pitboard starts the MSI's uninstall only from the copy in this folder.
    [TestMethod]
    public void ItRecordsTheFolderItInstallsPitboardIn()
    {
        using var database = Open();

        var location = Rows(database, "SELECT `Action`, `Target` FROM `CustomAction` WHERE `Source` = 'ARPINSTALLLOCATION'", 2).Single();
        Assert.AreEqual("[INSTALLFOLDER]", location[1]);
        Assert.IsGreaterThan(Sequence(database, "CostFinalize"), Sequence(database, location[0]!));
        Assert.IsGreaterThan(Sequence(database, location[0]!), Sequence(database, "RegisterProduct"), "set after RegisterProduct wrote it");
    }

    [TestMethod]
    public void AnUpdateStartsPitboardAgainOnlyWhenItsInstallAsks()
    {
        using var database = Open();

        var launch = Rows(database, "SELECT `Type`, `Source`, `Target` FROM `CustomAction` WHERE `Action` = 'LaunchAfterInstall'", 3).Single();
        var type = int.Parse(launch[0]!, CultureInfo.InvariantCulture);
        Assert.AreEqual(ExeFromInstalledFile, type & 0x3F);
        Assert.AreEqual(AsyncNoWait, type & AsyncNoWait);
        Assert.AreEqual(0, type & Deferred);
        Assert.AreEqual("PitboardExe --updated", string.Join(' ', launch[1..]));
        Assert.AreEqual("LAUNCHAFTERINSTALL = \"1\" AND NOT REMOVE", Condition(database, "LaunchAfterInstall"));
        Assert.IsGreaterThan(Sequence(database, "InstallFinalize"), Sequence(database, "LaunchAfterInstall"));
    }

    // Plan 4.12: once a first install is done, Pitboard starts as from Start, which shows its welcome, unless the install is
    // quiet. An upgrade, whose MajorUpgrade finds the product it replaces, starts it only where WinSparkle asks.
    [TestMethod]
    public void AFirstInstallStartsPitboardAsStartDoes()
    {
        using var database = Open();

        var start = Rows(database, "SELECT `Type`, `Source`, `Target` FROM `CustomAction` WHERE `Action` = 'StartAfterFirstInstall'", 3).Single();
        var type = int.Parse(start[0]!, CultureInfo.InvariantCulture);
        Assert.AreEqual(ExeFromInstalledFile, type & 0x3F);
        Assert.AreEqual(AsyncNoWait, type & AsyncNoWait);
        Assert.AreEqual(0, type & Deferred);
        Assert.AreEqual("PitboardExe", start[1]);
        Assert.IsNull(start[2], "an argument a launch from Start has none of");
        var upgrading = Rows(database, "SELECT `ActionProperty` FROM `Upgrade` WHERE `VersionMax` IS NOT NULL", 1).Single()[0];
        Assert.AreEqual($"NOT Installed AND NOT {upgrading} AND UILevel <> 2", Condition(database, "StartAfterFirstInstall"));
        Assert.IsGreaterThan(Sequence(database, "InstallFinalize"), Sequence(database, "StartAfterFirstInstall"));
    }

    // MsiOpenPackageEx's restricted session, which cannot change this machine (docs/facts/windows-documented.md).
    [TestMethod]
    [DataRow(InstallUIOptions.Silent, false, DisplayName = "/quiet")]
    [DataRow(InstallUIOptions.Basic, true, DisplayName = "/passive")]
    [DataRow(InstallUIOptions.Full, true, DisplayName = "with its window")]
    public void AQuietFirstInstallStartsNothing(InstallUIOptions ui, bool starts)
    {
        string condition;
        string upgrading;
        using (var database = Open())
        {
            condition = Condition(database, "StartAfterFirstInstall")!;
            upgrading = Rows(database, "SELECT `ActionProperty` FROM `Upgrade` WHERE `VersionMax` IS NOT NULL", 1).Single()[0]!;
        }

        var before = WindowsInstaller.SetInternalUI(ui);
        try
        {
            using var session = WindowsInstaller.OpenPackage(Built.Msi, ignoreMachineState: true);
            Assert.AreEqual(((int)ui).ToString(CultureInfo.InvariantCulture), session["UILevel"]);
            Assert.AreEqual(starts, session.EvaluateCondition(condition));

            session[upgrading] = UpgradeCode;
            Assert.IsFalse(session.EvaluateCondition(condition), "an upgrade");
        }
        finally
        {
            WindowsInstaller.SetInternalUI(before);
        }
    }

    // Each ignored on failure: the install goes on without the share target. Pitboard.exe removes the package, since
    // RemovePackageAsync ends every process of the package it removes, and a rollback after the registration removes it too.
    [TestMethod]
    public void PitboardShareRegistersOnEachInstallAndUnregistersOnEachRemovalAndRollback()
    {
        using var database = Open();
        var actions = Rows(database, "SELECT `Action`, `Type`, `Source`, `Target` FROM `CustomAction`", 4)
            .Where(row => row[0] is "RegisterShare" or "UnregisterShare" or "RegisterShareRollback")
            .ToDictionary(row => row[0]!);

        Assert.AreEqual(Built.ShareSigned ? 3 : 0, actions.Count);
        if (!Built.ShareSigned)
        {
            return;
        }

        foreach (var (action, command, when, condition) in new[]
        {
            ("RegisterShare", "PitboardShareExe --register", Deferred, "NOT REMOVE"),
            ("UnregisterShare", "PitboardExe --unregister-share", Deferred, "REMOVE = \"ALL\" AND NOT UPGRADINGPRODUCTCODE"),
            ("RegisterShareRollback", "PitboardExe --unregister-share", Rollback, "NOT REMOVE"),
        })
        {
            var type = int.Parse(actions[action][1]!, CultureInfo.InvariantCulture);
            Assert.AreEqual(ExeFromInstalledFile | when | IgnoreExitCode, type & (0x3F | Rollback | IgnoreExitCode), action);
            Assert.AreEqual(command, string.Join(' ', actions[action][2..]), action);
            Assert.AreEqual(condition, Condition(database, action), action);
        }

        Assert.IsLessThan(Sequence(database, "RegisterShare"), Sequence(database, "RegisterShareRollback"));
        Assert.IsGreaterThan(Sequence(database, "InstallFiles"), Sequence(database, "RegisterShareRollback"));
    }

    // One feature and no UI, so Modify would only run the install again.
    [TestMethod]
    public void SettingsOffersNoModify()
    {
        using var database = Open();

        Assert.AreEqual("1", Property(database, "ARPNOMODIFY"));
    }

    // A RemoveRegistry row acts on an install, so the Run value goes through --unregister instead.
    [TestMethod]
    public void NoRowWritesTheMachinesRegistryOrRemovesTheRunValue()
    {
        using var database = Open();

        var roots = Rows(database, "SELECT `Root` FROM `Registry`", 1).Select(row => row[0]).Distinct().ToList();
        CollectionAssert.IsSubsetOf(roots, UsersRegistryRoots, "HKMU, which a per-user install writes under HKCU, and HKCU");
        if (database.Tables.Contains("RemoveRegistry"))
        {
            var removed = Rows(database, "SELECT `Root`, `Key` FROM `RemoveRegistry`", 2);
            Assert.IsFalse(removed.Any(row => row[1]!.EndsWith(@"CurrentVersion\Run", StringComparison.OrdinalIgnoreCase)));
            Assert.IsFalse(removed.Any(row => row[0] == "2"));
        }
    }

    private static string? Condition(Database database, string action) =>
        Rows(database, $"SELECT `Condition` FROM `InstallExecuteSequence` WHERE `Action` = '{action}'", 1).Single()[0];

    private static int Sequence(Database database, string action) =>
        int.Parse(
            Rows(database, $"SELECT `Sequence` FROM `InstallExecuteSequence` WHERE `Action` = '{action}'", 1).Single()[0]!,
            CultureInfo.InvariantCulture);
}
