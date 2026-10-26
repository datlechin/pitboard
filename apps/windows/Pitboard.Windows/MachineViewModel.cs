using System.ComponentModel;
using Pitboard.Core;

namespace Pitboard.Windows;

public sealed partial class MachineViewModel(AppModel model) : INotifyPropertyChanged
{
    private ChecksShown? _shown;

    public event PropertyChangedEventHandler? PropertyChanged;

    private ChecksShown Checks => model.Machine.Checks;

    public CheckLine[] Lines => Checks.Lines;

    public bool HasSummary => Checks.Summary is not null;

    public string Summary => Checks.Summary ?? "";

    public Level Level => Checks.Level;

    public bool Checking => Checks.Checking;

    public bool CanCheck => !Checks.Checking;

    public string Checked => Checks.Checked ?? "";

    public bool Waiting => Checks.Waiting is not null;

    public string WaitingText => Checks.Waiting ?? "";

    public void Listen()
    {
        model.PropertyChanged -= OnModelChanged;
        model.PropertyChanged += OnModelChanged;
        _shown = null;
        Show();
    }

    public void Stop() => model.PropertyChanged -= OnModelChanged;

    // Every visit, and Check again: a check fixed in a terminal since would otherwise still read as failing.
    public void Check() => model.Send(new Intent.PaneShown(Pane.Checks));

    private void OnModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (args.PropertyName == nameof(AppModel.Machine))
        {
            Show();
        }
    }

    // A list told its lines again goes back to its top, so they are told only when they changed.
    private void Show()
    {
        var checks = Checks;
        var before = _shown;
        if (Structural.Equal(checks, before))
        {
            return;
        }

        _shown = checks;
        if (before is null)
        {
            Tell(string.Empty);
            return;
        }

        if (!Structural.Equal(checks.Lines, before.Lines))
        {
            Tell(nameof(Lines));
        }

        if (checks.Summary != before.Summary)
        {
            Tell(nameof(HasSummary), nameof(Summary));
        }

        if (checks.Level != before.Level)
        {
            Tell(nameof(Level));
        }

        if (checks.Checking != before.Checking)
        {
            Tell(nameof(Checking), nameof(CanCheck));
        }

        if (checks.Checked != before.Checked)
        {
            Tell(nameof(Checked));
        }

        if (checks.Waiting != before.Waiting)
        {
            Tell(nameof(Waiting), nameof(WaitingText));
        }
    }

    private void Tell(params string[] names)
    {
        foreach (var name in names)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }
    }
}
