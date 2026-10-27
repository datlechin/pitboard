using System.Globalization;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class WindowsLocalTimeTests
{
    // Friday 15 January 2027, 08:00 UTC.
    private const long At = 1_800_000_000;

    private static WindowsLocalTime In(string culture) =>
        new(new CultureInfo(culture, useUserOverride: false), TimeZoneInfo.Utc);

    // Newer ICU data puts a narrow no-break space before AM, which \s takes too.
    [TestMethod]
    public void AnAmericanReadsTwelveHours()
    {
        var time = In("en-US");

        StringAssert.Matches(time.Clock(At, withWeekday: false), new(@"^8:00\sAM$"));
        StringAssert.Matches(time.Clock(At, withWeekday: true), new(@"^Fri 8:00\sAM$"));
        StringAssert.Matches(time.DateAndTime(At), new(@"^1/15/2027 8:00\sAM$"));
    }

    [TestMethod]
    public void ABritishReaderReadsTwentyFourHours()
    {
        var time = In("en-GB");

        Assert.AreEqual("08:00", time.Clock(At, withWeekday: false));
        Assert.AreEqual("Fri 08:00", time.Clock(At, withWeekday: true));
        Assert.AreEqual("15/01/2027 08:00", time.DateAndTime(At));
    }

    // As Windows' regional format lets a person set their own short time.
    [TestMethod]
    public void APersonsOwnShortTimeIsTheClocks()
    {
        var culture = (CultureInfo)new CultureInfo("en-US", useUserOverride: false).Clone();
        culture.DateTimeFormat.ShortTimePattern = "HH:mm";
        var time = new WindowsLocalTime(culture, TimeZoneInfo.Utc);

        Assert.AreEqual("08:00", time.Clock(At, withWeekday: false));
        Assert.AreEqual("Fri 08:00", time.Clock(At, withWeekday: true));
    }

    // The warm-up hours' summary sat in 24 hours beside time pickers in AM and PM.
    [TestMethod]
    public void ATimeOfDayIsTheClocks()
    {
        var culture = (CultureInfo)new CultureInfo("en-US", useUserOverride: false).Clone();
        culture.DateTimeFormat.ShortTimePattern = "HH:mm";

        StringAssert.Matches(In("en-US").TimeOfDay(6 * 60), new(@"^6:00\sAM$"));
        StringAssert.Matches(In("en-US").TimeOfDay((22 * 60) + 30), new(@"^10:30\sPM$"));
        StringAssert.Matches(In("en-US").TimeOfDay(0), new(@"^12:00\sAM$"));
        Assert.AreEqual("06:00", In("en-GB").TimeOfDay(6 * 60));
        Assert.AreEqual("22:30", new WindowsLocalTime(culture, TimeZoneInfo.Utc).TimeOfDay((22 * 60) + 30));
    }

    // "Saved sign-in valid until Nov 20": the culture orders the day and its month.
    [TestMethod]
    public void ADateIsItsDayAndShortMonth()
    {
        Assert.AreEqual("Jan 15", In("en-US").Date(At));
        Assert.AreEqual("15 Jan", In("en-GB").Date(At));
    }

    [TestMethod]
    public void ADayIsTheOneInThePersonsTimeZone()
    {
        var east = TimeZoneInfo.CreateCustomTimeZone("UTC+02", TimeSpan.FromHours(2), "UTC+02", "UTC+02");
        var before = At + (13 * 3600) + (30 * 60);
        var after = before + 3600;
        var culture = new CultureInfo("en-GB", useUserOverride: false);

        Assert.IsTrue(new WindowsLocalTime(culture, TimeZoneInfo.Utc).SameDay(before, after));
        Assert.IsFalse(new WindowsLocalTime(culture, east).SameDay(before, after));
        Assert.AreEqual("Sat 00:30", new WindowsLocalTime(culture, east).Clock(after, withWeekday: true));
    }
}
