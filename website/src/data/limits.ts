// The Limits settings' words on both systems, as present/machine.rs `at_limit` and `warm` give
// them: each choice titled as macOS titles a menu item and in sentence case on Windows, the
// controls' names, the warm-up's hours, and the terms. limits.test.ts finds each in its source.
import { DOCS } from "./site.ts";
import type { AtLimit } from "../lib/model.ts";

export const section = "Before an account runs out";
export const heading = "When a limit reaches";

export const choices: { value: AtLimit; mac: string; win: string }[] = [
  { value: "nothing", mac: "Do Nothing", win: "Do nothing" },
  { value: "notify", mac: "Notify Me", win: "Notify me" },
  { value: "switch", mac: "Switch Automatically", win: "Switch automatically" },
];

// the pop-up's and the ComboBox's name, which says the share
export const atLabel = (share: number) => `${heading} ${share}%`;
export const shareLabel = "Share of a limit";
export const shareRange = "From 50% to 99%";

export const terms = {
  sentence: "Anthropic’s terms may cover automatic switching and warm-ups.",
  link: "Is this allowed?",
  href: `${DOCS}/concepts/limits#is-this-allowed`,
};

// A time of the day in minutes, on the world's 24-hour clock.
export const timeOfDay = (minutes: number) =>
  `${String(Math.floor(minutes / 60) % 24).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;

// words.rs `warm_hours`: a clock says a day's end as the next day's 00:00, which a span ending
// then does not mean.
export function warmHours(from: number, to: number): string {
  if (from === 0 && to === 24 * 60) return "All day";
  return `${timeOfDay(from)} to ${to === 24 * 60 ? "midnight" : timeOfDay(to)}`;
}

export const warm = { title: "Warm up limits" };
