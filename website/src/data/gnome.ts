// What GNOME Shell 50.6 says in the page's Fedora 44, whose en_GB locale gives the 24-hour clock
// (docs/facts/linux-session.md): its own words, and the dates its top bar and date menu show, as
// gnome-shell's and gnome-desktop's po/en_GB.po translate js/ui/dateMenu.js, js/ui/calendar.js
// and gnome-wall-clock.c.
import { ZONE } from "./world.ts";
import { dayAndMonth, hm } from "../lib/words.ts";

export const shell = {
  clear: "Clear",
  clearAll: "Clear all notifications",
  noNotifications: "No Notifications",
  today: "Today",
  noEvents: "No Events",
  // Fedora's Workstation installs Clocks and Weather (comps-f44 gnome-desktop), so the date menu
  // shows both, set up with nothing
  addWorldClocks: "Add World Clocks…",
  selectWeatherLocation: "Select Weather Location…",
  // dateUtils.js formatTimeSpan, under five minutes
  justNow: "Just now",
  // a message's close and expand buttons, which St names by nothing but their icons, window-close
  // and notification-expand
  close: "Close",
  expand: "Expand",
};

const local = (epoch: number) => new Date((epoch + ZONE) * 1000);
const weekdays = "Sunday Monday Tuesday Wednesday Thursday Friday Saturday".split(" ");
const months =
  "January February March April May June July August September October November December".split(
    " ",
  );

// gnome-wall-clock.c with clock-show-date on and the weekday off, en_GB's "%-e %b_%R": the date,
// then the time, the underscore an en space the top bar draws as a gap
export const topBarClock = (epoch: number) => ({ date: dayAndMonth(epoch), time: hm(epoch) });

// The date menu's TodayButton: "%A" over en_GB's "%-d %B %Y".
export function todayButton(epoch: number) {
  const d = local(epoch);
  return {
    day: weekdays[d.getUTCDay()],
    date: `${d.getUTCDate()} ${months[d.getUTCMonth()]} ${d.getUTCFullYear()}`,
  };
}

export interface Day {
  // "%d", as the grid numbers a day
  label: string;
  today: boolean;
  // en_GB's calendar-no-work "06": Sunday and Saturday
  weekend: boolean;
  otherMonth: boolean;
}

// en_GB weeks start on Monday (Shell.util_get_week_start)
const WEEK_START = 1;
const letters = "S M T W T F S".split(" ");

// Calendar._rebuildCalendar: six weeks from the week start on or before the month's first day, a
// week more before a month that begins on the week start, so the menu's height never changes. The
// month is named "%OB" in its own year.
export function month(epoch: number) {
  const now = local(epoch);
  const [y, m, today] = [now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate()];
  const first = new Date(Date.UTC(y, m, 1));
  const toStart = (7 + first.getUTCDay() - WEEK_START) % 7;
  const begin = Date.UTC(y, m, 1 - toStart - (toStart === 0 ? 7 : 0));
  const weeks: Day[][] = [];
  for (let w = 0; w < 6; w++) {
    const week: Day[] = [];
    for (let i = 0; i < 7; i++) {
      const d = new Date(begin + (w * 7 + i) * 86_400_000);
      week.push({
        label: String(d.getUTCDate()).padStart(2, "0"),
        today: d.getUTCMonth() === m && d.getUTCDate() === today,
        weekend: d.getUTCDay() === 0 || d.getUTCDay() === 6,
        otherMonth: d.getUTCMonth() !== m,
      });
    }
    weeks.push(week);
  }
  const headings = Array.from({ length: 7 }, (_, i) => {
    const day = (WEEK_START + i) % 7;
    return { letter: letters[day], name: weekdays[day] };
  });
  return { label: months[m], headings, weeks };
}
