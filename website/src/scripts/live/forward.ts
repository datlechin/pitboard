// The page stands for one Mac or PC (plan question 3).
// departs from the apps: Open Pitboard, Settings on Windows, Warm-Up Settings and the tray icon bring the page's figure into view
import { DOCS } from "../../data/site.ts";
import type { System } from "../../lib/platform.ts";

export type Shown = "accounts" | "settings";
const elements: Record<Shown, string> = { accounts: "pb-accounts", settings: "pb-settings" };

// A figure that opens a sheet of its own when asked, such as the window's Add Account.
interface Presents {
  present(sheet: string): void;
}

export function bringForward(window: Shown, system: System, sheet?: string) {
  // Windows' Settings is a page of the Pitboard window: the figure that draws it
  const figure =
    system === "windows" && window === "settings"
      ? [
          ...document.querySelectorAll<HTMLElement>('[data-drawing="app windows"] pb-accounts'),
        ].find((el) => el.querySelector('[data-pane-panel="settings"] > *'))
      : [...document.querySelectorAll<HTMLElement>(elements[window])].find((el) =>
          el.closest(`[data-drawing="app ${system}"]`),
        );
  if (!figure) return;
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  figure.scrollIntoView({ block: "center", behavior: still ? "auto" : "smooth" });
  if (!figure.matches(":defined")) return;
  const presents = figure as HTMLElement & Partial<Presents>;
  if (system === "windows" && window === "settings") presents.present?.("settings");
  if (sheet && presents.present) presents.present(sheet);
  else
    figure
      .querySelector<HTMLElement>(
        ':scope > [data-part="window"] :is([tabindex="0"], button:not(:disabled))',
      )
      ?.focus({ preventScroll: true });
}

// Pitboard Help: the documentation, in a tab of its own.
export function help() {
  window.open(DOCS, "_blank", "noopener");
}
