// The Limits settings' words on both systems, as present/machine.rs `at_limit` and `warm` give
// them: each choice and order titled as macOS titles a menu item and in sentence case on Windows,
// the controls' names, the warm-up's hours, and the terms. limits.test.ts finds each in its source.
import { DOCS } from "./site.ts";
import type { AtLimit, Order } from "../lib/model.ts";
import { eachFront, titled } from "../lib/words.ts";

export const section = "Before an account runs out";
export const heading = "When a limit reaches";

export const choices: { value: AtLimit; mac: string; win: string }[] = [
  { value: "nothing", mac: "Do Nothing", win: "Do nothing" },
  { value: "notify", mac: "Notify Me", win: "Notify me" },
  { value: "switch", mac: "Switch Automatically", win: "Switch automatically" },
];

// the pop-up's and the ComboBox's name, which says the share
export const atLabel = (share: number) => `${shareLabel} ${share}%`;
// present/words/machine.rs `AT`: the share's field, beside When a limit reaches
export const shareLabel = "5-hour limit";
export const weeklyLabel = "Weekly limits";
export const weeklyToggle = { mac: "Use Weekly Limits", win: "Use weekly limits" };
export const shareRange = "From 50% to 99%";

// present/words/machine.rs `USE_FIRST` and `order_choice`: which account goes first, under At
export const orderLabel = "Use first";
export const orders: { value: Order; mac: string; win: string }[] = [
  { value: "most-left", mac: "Most Left", win: "Most left" },
  { value: "resets-soonest", mac: "Resets Soonest", win: "Resets soonest" },
];

export const terms = {
  sentence: "Anthropic’s terms may cover automatic switching and warm-ups.",
  link: "Is this allowed?",
  href: `${DOCS}/concepts/limits#is-this-allowed`,
};

// A time of the day in minutes, on the world's 24-hour clock.
export const timeOfDay = (minutes: number) =>
  `${String(Math.floor(minutes / 60) % 24).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;

// present/words/machine.rs `warm_hours`: a clock says a day's end as the next day's 00:00, which a
// span ending then does not mean.
export function warmHours(from: number, to: number): string {
  if (from === 0 && to === 24 * 60) return "All day";
  return `${timeOfDay(from)} to ${to === 24 * 60 ? "midnight" : timeOfDay(to)}`;
}

// present/words/machine.rs `warm_up`
export const warm = { title: eachFront((front) => titled(front, "Warm-Up", "Warm-up")) };

export function weeklyNote(on: boolean, order: Order): string {
  const note = on
    ? "The same threshold applies to overall and model-specific weekly limits."
    : "Weekly usage does not trigger notifications or automatic switches. Accounts with no weekly usage left are still unavailable.";
  return order === "resets-soonest"
    ? `${note} Resets Soonest can still choose an account whose week resets earlier.`
    : note;
}
