// The page stands for one Mac, PC or Linux computer (plan question 3).
// departs from the apps: Open Pitboard, Settings on Windows, Warm-Up Settings and the tray icon bring the page's figure into view
import { DOCS } from "../../data/site.ts";
import { platformOf, type Platform, type System } from "../../lib/platform.ts";

export type Shown = "accounts" | "settings" | "warm";

// Each app's window, its settings and their warm-up, by the element of the figures that draw them:
// macOS's Settings cut at Warm-Up is a figure of its own, Windows' Settings is a page of its
// window, and the Linux app's Preferences a dialog over its window, drawn in figures of their own.
const figures: Record<Platform, Record<Shown, string>> = {
  macos: { accounts: "pb-mac-accounts", settings: "pb-settings", warm: "pb-warm" },
  windows: { accounts: "pb-win-window", settings: "pb-win-window", warm: "pb-win-window" },
  linux: {
    accounts: "pb-linux-window",
    settings: "pb-linux-preferences",
    warm: "pb-linux-preferences",
  },
};

// A figure that opens a sheet or a page of its own when asked, such as the window's Add Account.
interface Presents {
  present(sheet: string): void;
}

// The figures of `window` on `system`, those shown first: under KDE Plasma the hero draws no
// window, so its tray brings the next one.
function figuresOf(window: Shown, system: System): HTMLElement[] {
  const platform = platformOf(system);
  const all = [
    ...document.querySelectorAll<HTMLElement>(
      `[data-drawing="app ${platform === "linux" ? "linux" : system}"] ${figures[platform][window]}`,
    ),
  ].filter(
    // Windows' Settings is the window that draws its Settings page
    (el) =>
      platform !== "windows" ||
      window === "accounts" ||
      el.querySelector('[data-pane-panel="settings"] > *'),
  );
  return [...all.filter((el) => el.offsetParent !== null), ...all];
}

export function bringForward(window: Shown, system: System, sheet?: string) {
  const [figure] = figuresOf(window, system);
  if (!figure) return;
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  figure.scrollIntoView({ block: "center", behavior: still ? "auto" : "smooth" });
  if (!figure.matches(":defined")) return;
  const presents = figure as HTMLElement & Partial<Presents>;
  if (system === "windows" && window !== "accounts") presents.present?.("settings");
  if (sheet && presents.present) presents.present(sheet);
  else
    (figure.querySelector(':scope > [data-part="window"]') ?? figure)
      .querySelector<HTMLElement>(':is([tabindex="0"], button:not(:disabled))')
      ?.focus({ preventScroll: true });
}

// Pitboard Help: the documentation, in a tab of its own.
export function help() {
  window.open(DOCS, "_blank", "noopener");
}
