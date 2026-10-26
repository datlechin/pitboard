using Microsoft.Win32;
using Pitboard.Core;

namespace Pitboard.Windows;

// The values of the person's own Run key, each a command Windows starts as they sign in, and what Settings > Apps >
// Startup keeps of each in StartupApproved, which only it writes.
public interface IRunKey
{
    string? Value(string name);

    byte[]? Approval(string name);

    void Write(string name, string command);

    void Remove(string name);
}

public sealed class RegistryRunKey : IRunKey
{
    public const string Path = @"Software\Microsoft\Windows\CurrentVersion\Run";

    public const string ApprovedPath = @"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    public string? Value(string name)
    {
        using var key = Registry.CurrentUser.OpenSubKey(Path);
        return key?.GetValue(name, null, RegistryValueOptions.DoNotExpandEnvironmentNames) as string;
    }

    public byte[]? Approval(string name)
    {
        using var key = Registry.CurrentUser.OpenSubKey(ApprovedPath);
        return key?.GetValue(name) as byte[];
    }

    public void Write(string name, string command)
    {
        using var key = Registry.CurrentUser.CreateSubKey(Path, writable: true);
        key.SetValue(name, command, RegistryValueKind.String);
    }

    public void Remove(string name)
    {
        using var key = Registry.CurrentUser.OpenSubKey(Path, writable: true);
        key?.DeleteValue(name, throwOnMissingValue: false);
    }
}

// Start at sign-in as an installed Release build keeps it: the Run value Pitboard, naming this Pitboard.exe and nothing else.
public sealed class RunValue(Build build, string exe, IRunKey key) : IStartupItem
{
    public const string Name = "Pitboard";

    // What the Run key's documentation allows a command line.
    public const int Longest = 260;

    public string? Refusal => build is Build.Debug ? Words.Settings.DebugNeverStarts : null;

    public static string Command(string exe) => $"\"{exe}\" {Launching.SignIn}";

    // Whatever its arguments, as an uninstall finds a value this Pitboard.exe wrote.
    public static bool Names(string? command, string exe) =>
        command is not null
            && Launching.Arguments(command).FirstOrDefault() is { } program
            && string.Equals(program, exe, StringComparison.OrdinalIgnoreCase);

    public bool Read() => Written() && !OffInSettings();

    // Windows starts no Run value Settings turned off, and lists none that is not there to turn on again.
    public bool TurnedOff() => Written() && OffInSettings();

    public void Write(bool starts)
    {
        if (Refusal is { } refusal)
        {
            throw new InvalidOperationException(refusal);
        }

        if (starts && TurnedOff())
        {
            throw new InvalidOperationException(Words.Settings.StartupTurnedOff);
        }

        if (!starts)
        {
            if (Read())
            {
                key.Remove(Name);
            }

            return;
        }

        var command = Command(exe);
        if (command.Length > Longest)
        {
            throw new InvalidOperationException(PitboardFfiMethods.PathTooLong((uint)(Longest - Command("").Length)));
        }

        key.Write(Name, command);
    }

    // Paths are compared as Windows compares them, without case.
    private bool Written() =>
        Refusal is null && string.Equals(key.Value(Name), Command(exe), StringComparison.OrdinalIgnoreCase);

    // Odd is off: Settings wrote 01 there on the session's PC (build 26300, 2026-10-18).
    private bool OffInSettings() => key.Approval(Name) is [var first, ..] && (first & 1) == 1;
}
