using System.Security;

namespace Pitboard.Windows;

// What starts Pitboard as the person signs in, which a fixture replaces so nothing is registered with Windows.
public interface IStartupItem
{
    // Why this copy never starts at sign-in, or null where it may.
    string? Refusal { get; }

    bool Read();

    // Settings > Apps > Startup turned it off, so Windows will not start it, and only Settings turns it on again.
    bool TurnedOff();

    void Write(bool starts);
}

// The settings' start at sign-in, as the macOS app's OpenAtLogin: what Windows has, and why a change did not happen.
public sealed class StartAtSignIn(IStartupItem item)
{
    public event EventHandler? Changed;

    // A Debug build's own writer refuses, so only a fixture's stand-in turns it on there.
    public static StartAtSignIn For(AppStart start, string exe)
    {
#if DEBUG
        if (start.Fixture is not null)
        {
            return new(new FixtureStartupItem(start.Environment.GetValueOrDefault(FixtureStartupItem.Variable) == "off"));
        }
#endif
        return new(new RunValue(start.Identity.Build, exe, new RegistryRunKey()));
    }

    public bool On { get; private set; }

    public bool TurnedOff { get; private set; }

    public string? Failed { get; private set; }

    public string? Refusal => item.Refusal;

    public bool CanTurnOn => Refusal is null && !TurnedOff;

    // Why it cannot be turned on here, as the settings and the welcome say it, or what it is for.
    public string Note(string usually) => Refusal ?? (TurnedOff ? Words.Own.StartupTurnedOff : null) ?? usually;

    // Read again each time the settings show it, since another program can change the Run key.
    public void Read()
    {
        try
        {
            On = item.Read();
            TurnedOff = item.TurnedOff();
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or SecurityException)
        {
            On = false;
            TurnedOff = false;
            Failed = error.Message;
        }

        Changed?.Invoke(this, EventArgs.Empty);
    }

    public void Set(bool on)
    {
        Failed = null;
        try
        {
            item.Write(on);
        }
        catch (Exception error) when (error is InvalidOperationException or IOException or UnauthorizedAccessException or SecurityException)
        {
            Failed = error.Message;
        }

        Read();
    }
}

#if DEBUG
// Starts nothing. Off in Settings, it stands for a Run value Settings > Apps > Startup turned off.
public sealed class FixtureStartupItem(bool offInSettings = false) : IStartupItem
{
    public const string Variable = "PITBOARD_FIXTURE_STARTUP";

    private readonly bool _offInSettings = offInSettings;
    private bool _written = offInSettings;

    public string? Refusal => null;

    public bool Read() => _written && !_offInSettings;

    public bool TurnedOff() => _written && _offInSettings;

    public void Write(bool starts)
    {
        if (starts && TurnedOff())
        {
            throw new InvalidOperationException(Words.Own.StartupTurnedOff);
        }

        if (!TurnedOff())
        {
            _written = starts;
        }
    }
}
#endif
