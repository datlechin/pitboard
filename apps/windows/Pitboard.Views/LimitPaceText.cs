using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Core;

namespace Pitboard.Views;

// A nullable pace is passed whole: a binding to Pace.Said can keep the previous words once Pace becomes null.
public sealed partial class LimitPaceText : UserControl
{
    public static readonly DependencyProperty PaceProperty = DependencyProperty.Register(
        nameof(Pace), typeof(object), typeof(LimitPaceText),
        new PropertyMetadata(null, (text, _) => ((LimitPaceText)text).Show()));

    private readonly TextBlock _text = new() { TextWrapping = TextWrapping.Wrap };

    public LimitPaceText()
    {
        Content = _text;
        Show();
    }

    // Object keeps WinUI from generating setters for the binding record's init-only properties.
    public object? Pace
    {
        get => GetValue(PaceProperty);
        set => SetValue(PaceProperty, value);
    }

    private void Show()
    {
        var pace = Pace as LimitPace;
        _text.Text = pace?.Said ?? "";
        ToolTipService.SetToolTip(_text, pace?.Help);
        Visibility = pace is null ? Visibility.Collapsed : Visibility.Visible;
    }
}
