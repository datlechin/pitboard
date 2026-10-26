using System.Text;
using System.Text.RegularExpressions;
using System.Xml.Linq;

namespace Pitboard.Windows.Tests;

// Plan 2: the model owns every word the Windows app draws, through app_words() and the sentences it makes, so no view or
// view model writes one of its own in XAML or C#.
[TestClass]
public sealed partial class OwnWordsTests
{
    private static readonly string[] Projects = ["Pitboard.Views", "Pitboard.Windows", "Pitboard.App"];

    // What a control draws, or Narrator reads, from an attribute or a member set in code.
    private static readonly string[] Drawn =
    [
        "Text", "Content", "Header", "Description", "Title", "Subtitle", "Label", "Message", "PlaceholderText",
        "PrimaryButtonText", "SecondaryButtonText", "CloseButtonText", "ActionButtonContent", "CloseButtonContent",
        "OnContent", "OffContent", "ToolTipService.ToolTip", "ActionIconToolTip", "AutomationProperties.Name",
        "AutomationProperties.HelpText",
    ];

    // What draws the text inside it, and a drawn member set as an element.
    private static readonly string[] Drawing = ["TextBlock", "Run", "Span", "Bold", "Italic", "Underline", "Hyperlink", "String"];

    // Words in a literal that nobody reads in the app: the names Windows registers it under, an HTTP status, and what
    // Windows was asked to do in a failure the core reports after its own sentence, as it reports Windows' own error.
    private static readonly (string File, string Said)[] NotDrawn =
    [
        ("Identity.cs", "Pitboard App Debug"),
        ("Identity.cs", "Pitboard App"),
        ("Identity.cs", "Pitboard Debug"),
        ("Identity.cs", "Pitboard Debug "),
        ("WebSession.cs", "No Content"),
        ("WindowsApps.cs", "start a Restart Manager session"),
        ("WindowsApps.cs", "register process  with Restart Manager"),
    ];

    private static string Apps
    {
        get
        {
            for (var folder = new DirectoryInfo(AppContext.BaseDirectory); folder is not null; folder = folder.Parent)
            {
                if (Directory.Exists(Path.Combine(folder.FullName, "Pitboard.Views")))
                {
                    return folder.FullName;
                }
            }

            throw new InvalidOperationException($"no apps/windows above {AppContext.BaseDirectory}");
        }
    }

    private static IEnumerable<string> Sources(string pattern) =>
        Projects.SelectMany(project => Directory.EnumerateFiles(Path.Combine(Apps, project), pattern));

    [TestMethod]
    public void NoXamlSaysWordsOfItsOwn()
    {
        var own = new List<string>();
        foreach (var file in Sources("*.xaml"))
        {
            foreach (var element in XDocument.Load(file).Descendants())
            {
                own.AddRange(element.Attributes()
                    .Where(attribute => Drawn.Contains(attribute.Name.LocalName) && !attribute.Value.StartsWith('{') && Letters(attribute.Value))
                    .Select(attribute => $"{Path.GetFileName(file)}: {attribute.Name.LocalName}=\"{attribute.Value}\""));
                var local = element.Name.LocalName;
                own.AddRange(element.Nodes().OfType<XText>()
                    .Where(text => (Drawing.Contains(local) || Drawn.Contains(local[(local.IndexOf('.') + 1)..])) && Letters(text.Value))
                    .Select(text => $"{Path.GetFileName(file)}: <{element.Name.LocalName}>{text.Value.Trim()}"));
            }
        }

        Assert.AreEqual("", string.Join(Environment.NewLine, own));
    }

    [TestMethod]
    public void NoCodeSaysWordsOfItsOwn()
    {
        var own = new List<string>();
        var seen = new HashSet<(string, string)>();
        foreach (var file in Sources("*.cs"))
        {
            var name = Path.GetFileName(file);
            foreach (var (said, before, line) in Literals(File.ReadAllText(file)))
            {
                var drawn = Sentence().IsMatch(said) || (Letters(said) && SetsADrawnMember().IsMatch(before));
                if (!drawn || before.Contains("Exception", StringComparison.Ordinal))
                {
                    continue;
                }

                if (NotDrawn.Contains((name, said)))
                {
                    seen.Add((name, said));
                }
                else
                {
                    own.Add($"{name}:{line}: \"{said}\"");
                }
            }
        }

        Assert.AreEqual("", string.Join(Environment.NewLine, own));
        Assert.AreEqual("", string.Join(", ", NotDrawn.Except(seen)), "words no file says any more");
    }

    // The checks read a literal as a person would: with what it is set to say, a sentence and a word for a control.
    [TestMethod]
    public void TheChecksFindWhatAViewWouldSay()
    {
        const string code = """
            // Text = "A comment's words";
            label.Text = "Exit";
            AutomationProperties.SetName(ring, "Looking");
            var note = $"Pitboard {version} is ready to install.";
            var id = $"account.{item.Qualified}";
            throw new InvalidOperationException("The page was shown too soon.");
            """;

        var found = Literals(code).Select(literal => (literal.Said, Sentence().IsMatch(literal.Said), SetsADrawnMember().IsMatch(literal.Before)));

        CollectionAssert.AreEqual(
            new[]
            {
                ("Exit", false, true), ("Looking", false, true), ("Pitboard  is ready to install.", true, false),
                ("account.", false, false), ("The page was shown too soon.", true, false),
            },
            found.ToArray());
    }

    private static bool Letters(string said) => said.Any(char.IsLetter);

    // Each string literal outside comments, its interpolations left out, with the code before it in its statement.
    private static List<(string Said, string Before, int Line)> Literals(string source)
    {
        var found = new List<(string, string, int)>();
        var statement = new StringBuilder();
        var at = 0;
        while (at < source.Length)
        {
            var next = at + 1 < source.Length ? source[at + 1] : '\0';
            if (source[at] == '/' && next == '/')
            {
                at = source.IndexOf('\n', at) is var end and >= 0 ? end : source.Length;
            }
            else if (source[at] == '/' && next == '*')
            {
                at = source.IndexOf("*/", at, StringComparison.Ordinal) + 2;
            }
            else if (source[at] == '\'')
            {
                at = source.IndexOf('\'', at + (next == '\\' ? 3 : 2)) + 1;
            }
            else if (Opening().Match(source, at) is { Success: true } opening)
            {
                var line = 1 + source.AsSpan(0, at).Count('\n');
                var (said, end) = Literal(source, at + opening.Length, opening.Value.Contains('@'), opening.Value.Contains('$'), found);
                found.Add((said, statement.ToString(), line));
                statement.Append("\"\"");
                at = end;
            }
            else
            {
                statement = source[at] is ';' or '{' or '}' ? new StringBuilder() : statement.Append(source[at]);
                at++;
            }
        }

        return found;
    }

    // What a literal says from `at`, past its opening quote, and where it ends; a literal inside an interpolation is found too.
    private static (string Said, int End) Literal(
        string source, int at, bool verbatim, bool interpolated, List<(string, string, int)> found)
    {
        var said = new StringBuilder();
        while (source[at] != '"' || (verbatim && at + 1 < source.Length && source[at + 1] == '"'))
        {
            if (source[at] == '"' || (source[at] is '{' or '}' && interpolated && source[at + 1] == source[at]))
            {
                said.Append(source[at]);
                at += 2;
            }
            else if (source[at] == '{' && interpolated)
            {
                at = Hole(source, at + 1, found);
            }
            else if (source[at] == '\\' && !verbatim)
            {
                said.Append(source[at + 1]);
                at += 2;
            }
            else
            {
                said.Append(source[at]);
                at++;
            }
        }

        return (said.ToString(), at + 1);
    }

    // Past an interpolation's closing brace.
    private static int Hole(string source, int at, List<(string, string, int)> found)
    {
        for (var depth = 1; depth > 0; at++)
        {
            if (Opening().Match(source, at) is { Success: true } opening)
            {
                var (said, end) = Literal(source, at + opening.Length, opening.Value.Contains('@'), opening.Value.Contains('$'), found);
                found.Add((said, "", 1 + source.AsSpan(0, at).Count('\n')));
                at = end - 1;
            }
            else
            {
                depth += source[at] switch { '{' => 1, '}' => -1, _ => 0 };
            }
        }

        return at;
    }

    [GeneratedRegex("""\G(\$@|@\$|\$|@)?"(?!"")""")]
    private static partial Regex Opening();

    // Two words in a row.
    [GeneratedRegex(@"\p{L}{2,}\s+\p{L}")]
    private static partial Regex Sentence();

    [GeneratedRegex(@"\b(Text|Content|Header|Description|Title|Subtitle|Label|Message|PlaceholderText|PrimaryButtonText|SecondaryButtonText|CloseButtonText)\s*=\s*$|\.Set(Name|HelpText|ToolTip)\([^,]*,\s*$")]
    private static partial Regex SetsADrawnMember();
}
