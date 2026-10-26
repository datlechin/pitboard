using System.Globalization;
using Pitboard.Core;

namespace Pitboard.Windows;

// Read at each call, so a change to the regional format or the time zone reaches the next snapshot.
public sealed class WindowsLocalTime(CultureInfo? culture = null, TimeZoneInfo? zone = null) : LocalTime
{
    private CultureInfo Culture => culture ?? CultureInfo.CurrentCulture;

    private DateTimeOffset At(long epoch) =>
        TimeZoneInfo.ConvertTime(DateTimeOffset.FromUnixTimeSeconds(epoch), zone ?? TimeZoneInfo.Local);

    public string Clock(long epoch, bool withWeekday)
    {
        var at = At(epoch);
        var time = at.ToString("t", Culture);
        return withWeekday ? $"{at.ToString("ddd", Culture)} {time}" : time;
    }

    public bool SameDay(long first, long second) => At(first).Date == At(second).Date;

    public string DateAndTime(long epoch) => At(epoch).ToString("g", Culture);

    public string TimeOfDay(ushort minutes) => DateTime.MinValue.AddMinutes(minutes).ToString("t", Culture);

    // The culture's own order of day and month, its month abbreviated: "Nov 20", "20 Nov".
    public string Date(long epoch) =>
        At(epoch).ToString(Culture.DateTimeFormat.MonthDayPattern.Replace("MMMM", "MMM", StringComparison.Ordinal), Culture);
}
