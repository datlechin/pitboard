// The Limits settings' words on both systems, as present/machine.rs `at_limit` gives them: each
// choice titled as macOS titles a menu item and in sentence case on Windows, the controls' names,
// and the terms under Switch Automatically. limits.test.ts finds each in its source.
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
