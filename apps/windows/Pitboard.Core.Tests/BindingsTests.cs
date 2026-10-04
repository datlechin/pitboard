namespace Pitboard.Core.Tests;

/// <summary>
/// The bindings against the library they were generated from. The first call checks the
/// library's contract version and every method's checksum, so a call that answers proves the
/// two agree: the same UniFFI made both, and the C# was generated from this library.
/// </summary>
[TestClass]
public sealed class BindingsTests
{
    private static readonly string[] Codes = ["claude", "codex", "desktop"];
    private static readonly string[] Names = ["Claude Code", "Codex", "Claude Desktop"];

    [TestMethod]
    public void TheBindingsLoadTheCoreAndAgreeWithIt()
    {
        var tools = PitboardFfiMethods.Tools();

        CollectionAssert.AreEqual(Codes, tools.Select(tool => tool.Code).ToArray());
        CollectionAssert.AreEqual(Names, tools.Select(tool => tool.Name).ToArray());
    }

    /// <summary>
    /// A sign-in is read by the core in its tool's own words, so the Windows app offers the
    /// address and the code field the macOS app does: the `https` address Codex prints after
    /// its loopback one, and no code, since Codex reads none.
    /// </summary>
    [TestMethod]
    public void ASignInIsReadAsTheCoreReadsIt()
    {
        const string said =
            "Starting local login server on http://localhost:1455.\n"
            + "If your browser did not open, navigate to this URL to authenticate:\n\n"
            + "https://auth.openai.com/oauth/authorize?state=x\n";

        var codex = PitboardFfiMethods.SignInView("codex", said, false);
        var claude = PitboardFfiMethods.SignInView("claude", "Paste code here if prompted > ", false);

        Assert.AreEqual(new SignInView("https://auth.openai.com/oauth/authorize?state=x", false), codex);
        Assert.IsTrue(claude.WantsCode);
    }

    /// <summary>
    /// Where the command line is looked for crosses the bindings in the shape the core gives
    /// it: a search path and places in, one of three answers out, here the one found nowhere.
    /// </summary>
    [TestMethod]
    public void ACommandLineFoundNowhereIsSaidToBeNowhere()
    {
        var found = PitboardFfiMethods.FindCommandLine("/nowhere/at/all", ["/nowhere/else"], null);

        Assert.IsInstanceOfType<FoundCommandLine.Nowhere>(found);
        Assert.AreEqual("/home/x/.cargo/bin", PitboardFfiMethods.CommandLinePlaces("/home/x")[0]);
    }

    /// <summary>
    /// Where the app's home and Pitboard's directory are, and whether a path is a program, are
    /// the core's to say, so the app has no rule of its own for either: the environment goes
    /// in as the app was given it, and the answer comes out as the core reads it.
    /// </summary>
    [TestMethod]
    public void TheAppAsksTheCoreWhereThingsAreAndWhatRuns()
    {
        var environment = new Dictionary<string, string> { ["HOME"] = "/home/x" };

        Assert.AreEqual("/home/x", PitboardFfiMethods.HomeDirectory(environment));
        Assert.AreEqual("/home/x/.pitboard", PitboardFfiMethods.PitboardDirectory(environment));
        Assert.IsFalse(PitboardFfiMethods.CanRun("/nowhere/at/all"));
    }
}
