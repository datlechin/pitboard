using Pitboard.Core;

namespace Pitboard.Windows;

public interface IScheduledTasks
{
    void Remove(string exe);
}

// The core alone names and writes the watch and renewal tasks, and takes each away only where it runs the command line
// beside exe.
public sealed class CoreScheduledTasks(Dictionary<string, string> environment) : IScheduledTasks
{
    public void Remove(string exe) => _ = PitboardModelMethods.UnscheduleApp(environment, exe);
}
