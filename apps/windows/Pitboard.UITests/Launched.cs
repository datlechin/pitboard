using System.Diagnostics;
using System.Drawing;
using System.Text;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Conditions;
using FlaUI.Core.Input;
using FlaUI.Core.Tools;
using FlaUI.Core.WindowsAPI;
using FlaUI.UIA3;

// One Pitboard at a time: they share the fixture's folder, the mouse and the foreground.
[assembly: DoNotParallelize]

namespace Pitboard.UITests;

// A Pitboard.exe started directly, in seven homes of its own, as AGENTS.md has a built one run.
internal sealed class Launched : IDisposable
{
    public static readonly string[] Homes =
        ["USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA", "CODEX_HOME", "CLAUDE_CONFIG_DIR", "PITBOARD_HOME"];

    private static readonly TimeSpan Patience = TimeSpan.FromSeconds(60);

    private static readonly TimeSpan Shortly = TimeSpan.FromSeconds(10);

    private readonly DirectoryInfo _root;
    private readonly string _exe;
    private readonly string? _fixture;
    private readonly IReadOnlyDictionary<string, string> _environment;
    private readonly string[] _arguments;
    private readonly bool _ownsHomes;
    private readonly StringBuilder _said = new();

    private Launched(
        DirectoryInfo root, string exe, string? fixture, IReadOnlyDictionary<string, string> environment, string[] arguments,
        Dictionary<string, string> homes, bool ownsHomes)
    {
        _root = root;
        _exe = exe;
        _fixture = fixture;
        _environment = environment;
        _arguments = arguments;
        _ownsHomes = ownsHomes;
        HomeFolders = homes;
        var start = StartInfo();
        start.RedirectStandardError = true;
        Process = Process.Start(start)!;
        Process.ErrorDataReceived += (_, line) =>
        {
            lock (_said)
            {
                _said.AppendLine(line.Data);
            }
        };
        Process.BeginErrorReadLine();
    }

    public Process Process { get; }

    public Dictionary<string, string> HomeFolders { get; }

    public UIA3Automation Automation { get; } = new();

    // The person's own: a known folder, which no variable moves.
    public static string[] DataFolders { get; } =
    [
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Pitboard App"),
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Pitboard App Debug"),
    ];

    public static Launched Start(string exe, string? fixture = null, params string[] arguments) =>
        Start(exe, fixture, new Dictionary<string, string>(), arguments);

    // With variables a Debug build reads in a fixture, such as PITBOARD_FIXTURE_STARTUP.
    public static Launched Start(string exe, string? fixture, IReadOnlyDictionary<string, string> environment, params string[] arguments)
    {
        var root = Directory.CreateTempSubdirectory("pitboard-ui-");
        var homes = Homes.ToDictionary(home => home, home => root.CreateSubdirectory(home).FullName);
        return new Launched(root, exe, fixture, environment, arguments, homes, ownsHomes: true);
    }

    // The Debug build's Pitboard link for a page, as pitboard_link makes one.
    public static string Link(string page) => $"pitboard-debug://open?url={Uri.EscapeDataString(page)}";

    // Launched again now, in the same homes, as from Start.
    public Launched Again(params string[] arguments) => new(_root, _exe, _fixture, _environment, arguments, HomeFolders, ownsHomes: false);

    // A second launch once Pitboard runs, as from Start, which the running copy answers with its window.
    public Window OpenWindow()
    {
        HandOver();
        return Window("Pitboard");
    }

    // A link given to a second launch in the same homes and fixture, as the MSI's command gives one, which the running
    // copy answers with its Open link window.
    public Window Share(string page)
    {
        HandOver(Link(page));
        return Window("Open link");
    }

    private void HandOver(params string[] arguments)
    {
        Running();
        using var again = Again(arguments);
        if (!again.HandedOver(out var why))
        {
            throw new AssertFailedException($"a second launch did not hand over to the first: {why}");
        }
    }

    // Pitboard makes its hidden window as it starts, before it can open another.
    public void Running()
    {
        if (!Retry.WhileFalse(() => HiddenWindows.Of(Process).Any(), Patience, TimeSpan.FromMilliseconds(250)).Result)
        {
            throw new AssertFailedException($"Pitboard did not start within {Patience}: {State()}");
        }
    }

    public bool HandedOver(out string why)
    {
        var quit = Process.WaitForExit(Patience);
        why = quit ? State() : $"it went on running after {Patience}";
        return quit && Process.ExitCode == 0;
    }

    public Window[] Windows(string title) =>
        FlaUI.Core.Application.Attach(Process).GetAllTopLevelWindows(Automation).Where(window => window.Title == title).ToArray();

    public Window Window(string title) => Window(shown => shown == title, $"titled {title}");

    // Its page's title comes first once it has one: "chatgpt.com stand-in - main".
    public Window AccountWindow(string label) => Window(shown => OfAccount(shown, label), $"of {label}");

    public static bool OfAccount(string title, string label) =>
        title == label || title.EndsWith($" - {label}", StringComparison.Ordinal);

    private Window Window(Func<string, bool> titled, string what)
    {
        var found = Retry.WhileNull(
            () => FlaUI.Core.Application.Attach(Process).GetAllTopLevelWindows(Automation)
                .FirstOrDefault(window => titled(window.Title)),
            Patience, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;
        return found ?? throw new AssertFailedException($"Pitboard showed no window {what} within {Patience}: {State()}");
    }

    // A WinUI app that fails fast exits 0xC000027B and leaves only what it wrote to its standard error.
    public string State()
    {
        string state;
        if (Process.HasExited)
        {
            Process.WaitForExit();
            state = $"it exited 0x{Process.ExitCode:X8}";
        }
        else
        {
            var titles = Titles().Select(title => $"'{title}'").ToArray();
            state = titles.Length == 0 ? "it is running, with no window" : $"it is running, with {string.Join(", ", titles)}";
        }

        lock (_said)
        {
            return $"{state}, saying: {_said.ToString().Trim()}";
        }
    }

    // A window closing as it is read has no title left to give.
    public string[] Titles() =>
        [.. FlaUI.Core.Application.Attach(Process).GetAllTopLevelWindows(Automation)
            .Select(window => window.Properties.Name.ValueOrDefault)
            .OfType<string>()];

    // Looked for in every window of Pitboard's, since a menu or a dialog may draw in a window of its own.
    public AutomationElement Find(Func<ConditionFactory, ConditionBase> condition, string what)
    {
        var found = Retry.WhileNull(() => Look(condition), Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;
        return found ?? throw new AssertFailedException($"Pitboard showed no {what} within {Shortly}: {Shown()}");
    }

    // Each window's title and lines, for a failure to say what was there instead.
    public string Shown()
    {
        try
        {
            var windows = FlaUI.Core.Application.Attach(Process).GetAllTopLevelWindows(Automation);
            return string.Join("; ", windows.Select(window => $"'{window.Title}': {Listed(window)}"));
        }
        catch (Exception unread)
        {
            return $"its windows could not be read: {unread.Message}; {State()}";
        }
    }

    public bool Gone(Func<ConditionFactory, ConditionBase> condition) =>
        Retry.WhileTrue(() => Look(condition) is not null, Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    // A row hides its button while it switches and shows it again on the model's next snapshot.
    public static AutomationElement Within(AutomationElement scope, Func<ConditionFactory, ConditionBase> condition, string what) =>
        Retry.WhileNull(() => scope.FindFirstDescendant(condition), Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result
        ?? throw new AssertFailedException($"no {what} in {scope.Name} within {Shortly}: {Listed(scope)}");

    public AutomationElement Row(string qualified) => Find(found => found.ByAutomationId($"account.{qualified}"), $"row {qualified}");

    public AutomationElement Named(string name) => Find(found => found.ByName(name), $"'{name}'");

    public AutomationElement Element(string automationId) => Find(found => found.ByAutomationId(automationId), automationId);

    // As a person presses it once it can be pressed.
    public void Invoke(string automationId) => PressOnceEnabled(Element(automationId));

    // A dialog's button, which its template names: PrimaryButton, SecondaryButton or CloseButton.
    public void Answer(string dialog, string button) =>
        PressOnceEnabled(Within(Element(dialog), found => found.ByAutomationId(button), button));

    private void PressOnceEnabled(AutomationElement found)
    {
        if (!Retry.WhileFalse(() => found.IsEnabled, Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result)
        {
            throw new AssertFailedException($"{found.AutomationId}, a {found.ControlType} named '{found.Name}', stayed dimmed for {Shortly}: {Shown()}");
        }

        found.Patterns.Invoke.Pattern.Invoke();
    }

    // Through UI Automation, which reaches a field however the foreground stands.
    public static void Type(TextBox field, string text) => field.Patterns.Value.Pattern.SetValue(text);

    // A row's shortcut menu, asked for through UI Automation as the menu key asks. A click went to the point codex/main
    // had before it scrolled into view, below windows-2025's fold, and opened no menu (runs 39015107289, 39016117183).
    public static void OpenMenu(AutomationElement row)
    {
        Reveal(row);
        var native = ((UIA3FrameworkAutomationElement)row.FrameworkAutomationElement).NativeElement;
        ((Interop.UIAutomationClient.IUIAutomationElement3)native).ShowContextMenu();
    }

    // As a person right-clicks it, once UI Automation finds the row itself at the point the click goes to.
    public void RightClick(AutomationElement row)
    {
        Reveal(row);
        var point = Point.Empty;
        if (!Retry.WhileFalse(
                () => row.TryGetClickablePoint(out point) && Holds(row, Automation.FromPoint(point)),
                Shortly, TimeSpan.FromMilliseconds(100), ignoreException: true).Result)
        {
            throw new AssertFailedException($"UI Automation found no {row.AutomationId} at its own point within {Shortly}: {Shown()}");
        }

        Mouse.RightClick(point);
    }

    // Scrolled into view as a person would, and on screen once the list has moved it there.
    private static void Reveal(AutomationElement row)
    {
        row.Patterns.ScrollItem.PatternOrDefault?.ScrollIntoView();
        if (!Retry.WhileTrue(() => row.IsOffscreen, Shortly, TimeSpan.FromMilliseconds(100), ignoreException: true).Result)
        {
            throw new AssertFailedException($"{row.AutomationId} stayed off screen for {Shortly} after scrolling into view");
        }
    }

    private static bool Holds(AutomationElement scope, AutomationElement? found)
    {
        for (var at = found; at is not null; at = at.Parent)
        {
            if (at.Equals(scope))
            {
                return true;
            }
        }

        return false;
    }

    public TextBox Field(string automationId) => Find(found => found.ByAutomationId(automationId), automationId).AsTextBox();

    // A button the model holds back until it can be pressed.
    public static bool Enables(AutomationElement button) =>
        Retry.WhileFalse(() => button.IsEnabled, Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    public void Select(string pane) =>
        Find(found => found.ByAutomationId(pane), pane).Patterns.SelectionItem.Pattern.Select();

    // One of Settings' own pages, from its card as a person opens it: a SettingsCard takes Enter, and gives UI Automation
    // no Invoke.
    public void OpenFromSettings(Window window, string card)
    {
        Select("SettingsItem");
        var found = Element(card);
        Foreground.Require(window);
        found.Focus();
        Keyboard.Type(VirtualKeyShort.ENTER);
    }

    public bool Selected(string pane) =>
        Retry.WhileFalse(
            () => Find(found => found.ByAutomationId(pane), pane).Patterns.SelectionItem.Pattern.IsSelected.Value,
            Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    // A message box's line, or any line, is a Text element named by it.
    public static string Said(AutomationElement scope) =>
        string.Join(" ", Lines(scope));

    public static void Shows(AutomationElement scope, string text, bool exact = true, TimeSpan? within = null)
    {
        var shown = Retry.WhileFalse(
            () => Lines(scope).Any(line => exact ? line == text : line.StartsWith(text, StringComparison.Ordinal)),
            within ?? Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;
        Assert.IsTrue(shown, $"{scope.Name} does not show '{text}': {Listed(scope)}");
    }

    // An account window's first page, which the run's first browser process took more than Shortly to show on
    // windows-2025 (run 38877054096).
    public static void Loads(AutomationElement window, string text, bool exact = true) => Shows(window, text, exact, Patience);

    // What can be pressed in it, by control type and name.
    public static string Pressables(AutomationElement scope)
    {
        try
        {
            var kinds = new[] { FlaUI.Core.Definitions.ControlType.Button, FlaUI.Core.Definitions.ControlType.Hyperlink, FlaUI.Core.Definitions.ControlType.MenuItem, FlaUI.Core.Definitions.ControlType.ListItem };
            return string.Join(
                " | ",
                scope.FindAllDescendants()
                    .Where(found => kinds.Contains(found.Properties.ControlType.ValueOrDefault))
                    .Select(found => $"{found.Properties.ControlType.ValueOrDefault} '{found.Properties.Name.ValueOrDefault}'"));
        }
        catch (Exception unread)
        {
            return $"its controls could not be read: {unread.Message}";
        }
    }

    // Its lines, and the AutomationIds of its list's items.
    private static string Listed(AutomationElement scope)
    {
        try
        {
            var items = scope.FindAllDescendants(found => found.ByControlType(FlaUI.Core.Definitions.ControlType.ListItem))
                .Select(item => item.Properties.AutomationId.ValueOrDefault)
                .Where(id => !string.IsNullOrEmpty(id));
            return $"{string.Join(" | ", Lines(scope))} [{string.Join(", ", items)}]";
        }
        catch (Exception unread)
        {
            return $"its lines could not be read: {unread.Message}";
        }
    }

    // WebView2 gives some of a page's text no Name, which reading Name throws for.
    public static IEnumerable<string> Lines(AutomationElement scope) =>
        scope.FindAllDescendants(found => found.ByControlType(FlaUI.Core.Definitions.ControlType.Text))
            .Select(text => text.Properties.Name.ValueOrDefault)
            .OfType<string>();

    // A button or a page's link in the scope given, once it shows.
    public static AutomationElement? Pressable(AutomationElement scope, string name) =>
        Retry.WhileNull(
            () => scope.FindFirstDescendant(condition => condition.ByName(name)
                .And(condition.ByControlType(FlaUI.Core.Definitions.ControlType.Button)
                    .Or(condition.ByControlType(FlaUI.Core.Definitions.ControlType.Hyperlink)))),
            Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    // As a person presses it.
    public static void Press(AutomationElement scope, string name) =>
        (Pressable(scope, name) ?? throw new AssertFailedException($"no '{name}' in {scope.Name}")).Patterns.Invoke.Pattern.Invoke();

    // OK through UI Automation is a BM_CLICK, which may fail on an inactive box (E_UNEXPECTED on windows-2025).
    public static void Dismiss(Window box) => box.Patterns.Window.Pattern.Close();

    private AutomationElement? Look(Func<ConditionFactory, ConditionBase> condition) =>
        FlaUI.Core.Application.Attach(Process).GetAllTopLevelWindows(Automation)
            .Select(window => window.FindFirstDescendant(condition))
            .FirstOrDefault(found => found is not null);

    private ProcessStartInfo StartInfo()
    {
        var start = new ProcessStartInfo(_exe, _arguments) { UseShellExecute = false, WorkingDirectory = _root.FullName };
        foreach (var (home, folder) in HomeFolders)
        {
            start.Environment[home] = folder;
        }

        start.Environment.Remove("PITBOARD_FIXTURE");
        start.Environment.Remove("PITBOARD_FIXTURE_WEBVIEW2");
        start.Environment.Remove("PITBOARD_FIXTURE_STARTUP");
        if (_fixture is not null)
        {
            start.Environment["PITBOARD_FIXTURE"] = _fixture;
            start.Environment["PITBOARD_FIXTURE_WEBVIEW2"] = Web.Value.FullName;
        }

        foreach (var (name, value) in _environment)
        {
            start.Environment[name] = value;
        }

        return start;
    }

    // One user data folder for the run's windows: WebView2 tries the user's password blank once for each new folder, and
    // a folder per launch locked the user out on both images after ten (run 38874616742).
    private static readonly Lazy<DirectoryInfo> Web = new(() => Directory.CreateTempSubdirectory("pitboard-uitests-webview2-"));

    public static void RemoveWeb()
    {
        if (Web.IsValueCreated)
        {
            Remove(Web.Value.FullName);
        }
    }

    public static void Remove(string folder) => Retry.WhileFalse(() => Emptied(folder), Shortly, TimeSpan.FromMilliseconds(250));

    // Chromium holds it while a browser process runs over the folder.
    private static bool LetGo(string folder)
    {
        try
        {
            File.Delete(Path.Combine(folder, "EBWebView", "lockfile"));
            return true;
        }
        catch (DirectoryNotFoundException)
        {
            return true;
        }
        catch (Exception held) when (held is IOException or UnauthorizedAccessException)
        {
            return false;
        }
    }

    public string[] Written() =>
        HomeFolders.Values.Concat(DataFolders.Where(Directory.Exists))
            .SelectMany(Directory.EnumerateFileSystemEntries)
            .ToArray();

    // Asked first, so Pitboard removes its icon: a killed one leaves it in the notification area until hovered.
    public void Dispose()
    {
        if (!Process.HasExited)
        {
            HiddenWindows.AskToEnd(Process);
            if (!Process.WaitForExit(Shortly))
            {
                Process.Kill(entireProcessTree: true);
                Process.WaitForExit();
            }
        }

        // WebView2's browser process outlives Pitboard for a moment, holding the run's user data folder, which the next
        // launch would join as it goes. A copy launched again leaves it to the first, which may still use it.
        if (_fixture is not null && _ownsHomes)
        {
            Retry.WhileFalse(() => LetGo(Web.Value.FullName), Shortly, TimeSpan.FromMilliseconds(250));
        }

        Process.Dispose();
        Automation.Dispose();
        if (_ownsHomes)
        {
            _root.Delete(recursive: true);
        }
    }

    private static bool Emptied(string folder)
    {
        try
        {
            if (Directory.Exists(folder))
            {
                Directory.Delete(folder, recursive: true);
            }

            return true;
        }
        catch (Exception held) when (held is IOException or UnauthorizedAccessException)
        {
            return false;
        }
    }
}
