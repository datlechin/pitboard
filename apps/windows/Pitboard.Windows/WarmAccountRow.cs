using System.ComponentModel;
using Pitboard.Core;

namespace Pitboard.Windows;

// One Claude Code account under the settings' warm-up, kept while the model lists it so its card keeps its focus.
public sealed partial class WarmAccountRow(WarmAccountShown account, bool enabled) : INotifyPropertyChanged
{
    private WarmAccountShown _account = account;

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Qualified { get; } = account.Qualified;

    public string AutomationId => $"settings.warm.{Qualified}";

    public string Title => _account.Title;

    public string Standing => _account.Standing ?? "";

    public bool On => _account.On;

    public bool Enabled { get; private set; } = enabled;

    public void Show(WarmAccountShown account, bool enabled)
    {
        if (Structural.Equal(account, _account) && enabled == Enabled)
        {
            return;
        }

        _account = account;
        Enabled = enabled;
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(string.Empty));
    }
}
