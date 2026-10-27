namespace Pitboard.UITests;

// A Debug build writes no Jump List, so its tasks are started as Explorer starts one: Pitboard.exe with the task's
// arguments, handed to the copy that runs or starting one.
[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed class JumpListTests
{
    [TestMethod]
    public void AddAccountReachesTheCopyThatRuns()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.Window("Pitboard");

        launched.Jump("--add-account");

        launched.Answer("sheet", "CloseButton");
    }

    [TestMethod]
    public void AddAccountStartsPitboardOnItsDialog()
    {
        using var launched = Launched.Start(Builds.Debug, "oneTool", "--add-account");

        launched.Window("Pitboard");
        launched.Answer("sheet", "CloseButton");
    }
}
