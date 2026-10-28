using System.Collections;
using System.Text.RegularExpressions;
using Pitboard.Core;

namespace Pitboard.Windows.Tests;

// PR 316 left the stand-ins with sentences of their own. Each one they say is the model's, as its sources hold it,
// mostly as present/words/ pins it for the same labels, so a view test reads what the app would show.
[TestClass]
public sealed partial class StandInsTests
{
    [TestMethod]
    public void EachSentenceAStandInSaysIsTheModels()
    {
        object[] standIns =
        [
            Snapshots.Read(1), Snapshots.Terms, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare"),
            Items.Limit(42), Items.RenameAction("claude", "work"), Items.RemoveAction("claude", "spare"),
            Items.WindowAction(Items.Window("claude", "work")), Items.Notice("switched", Items.Dismiss("codex")), Items.LearnMore,
            Items.Tool("codex", "Codex"), Items.CodexCliMissing(), Items.AddAccount(), Items.Rename("work"),
            Items.SigningIn("Codex"), Items.Stow(),
        ];
        var model = Model();

        var theirOwn = standIns.SelectMany(Strings).Where(Said).Distinct().Where(said => !model.Contains($"\"{Escaped(said)}\"", StringComparison.Ordinal));

        Assert.AreEqual("", string.Join(Environment.NewLine, theirOwn));
    }

    // The string literals of the model's sources, its comments left out, each written on one line.
    private static string Model()
    {
        var sources = Directory.EnumerateFiles(Repository.Path("crates", "pitboard-model", "src"), "*.rs", SearchOption.AllDirectories)
            .Append(Repository.Path("crates", "pitboard-core", "src", "words.rs"))
            .Select(File.ReadAllText);
        return Continued().Replace(Comment().Replace(string.Join('\n', sources), ""), "");
    }

    private static IEnumerable<string> Strings(object? value) => value switch
    {
        string said => [said],
        IEnumerable items => items.Cast<object?>().SelectMany(Strings),
        not null when value.GetType().Namespace == typeof(Snapshot).Namespace && !value.GetType().IsEnum =>
            value.GetType().GetProperties()
                .Where(property => property.GetIndexParameters().Length == 0)
                .SelectMany(property => Strings(property.GetValue(value))),
        _ => [],
    };

    // A sentence or a control's word, and not a label, an id, a host, an address or a link.
    private static bool Said(string said) =>
        said.Any(char.IsLetter) && !said.Contains("://", StringComparison.Ordinal) && !said.Contains('@')
        && (said.Any(char.IsWhiteSpace) || char.IsUpper(said[0]));

    private static string Escaped(string said) => said.Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\n", "\\n");

    [GeneratedRegex(@"^\s*//.*$", RegexOptions.Multiline)]
    private static partial Regex Comment();

    [GeneratedRegex(@"\\\n\s*")]
    private static partial Regex Continued();
}
