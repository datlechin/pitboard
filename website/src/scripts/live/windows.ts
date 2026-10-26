// What the menus open that has no figure of its own (answer 3 a): the About panel and an account's
// window, each over the desk it is asked from.
import { cloned, text } from "./draw.ts";
import { closeWindow, model, type State } from "../../lib/model.ts";
import { siteOf } from "../../data/window.ts";
import type { Account } from "../../data/world.ts";

// AccountWindow.xaml's own title until the window reads its account: run 40208954337 saw it 73 ms
const XAML_TITLE_MS = 73;
// how far down and right each window opens from the one before it, as AppKit cascades them
const CASCADE = 22;

type System = "macos" | "windows";

// A template's clone, its `tpl-` ids made the desk's own.
function made(system: System, name: string, desk: HTMLElement, key: string): HTMLElement {
  const el = cloned(
    document.querySelector<HTMLTemplateElement>(
      `[data-drawing="app ${system}"] template[data-part="${name}"]`,
    ),
  );
  const own = (v: string) => v.replace(/\btpl-/g, `${desk.id}-${key}-`);
  for (const node of [el, ...el.querySelectorAll("*")])
    for (const name of ["id", "aria-labelledby", "aria-controls", "aria-describedby"]) {
      const v = node.getAttribute(name);
      if (v?.includes("tpl-")) node.setAttribute(name, own(v));
    }
  return el;
}

// the pixels a unit of `desk` draws at, as the page scales it
const unit = (desk: HTMLElement) =>
  desk.getBoundingClientRect().width /
  Number(getComputedStyle(desk).getPropertyValue("--units") || 600);

// Closing gives focus back to what had it, where that is still shown.
function returnsFocus() {
  const back = document.activeElement;
  return () => {
    if (back instanceof HTMLElement && back.isConnected && back.checkVisibility())
      back.focus({ preventScroll: true });
  };
}

export function showAbout(desk: HTMLElement) {
  let panel = desk.querySelector<HTMLDialogElement>(':scope > dialog[data-part="about"]');
  if (!panel) {
    panel = made("macos", "about", desk, "about") as HTMLDialogElement;
    const about = panel;
    about.querySelector('[data-does="close"]')!.addEventListener("click", () => about.close());
    about.addEventListener("close", () => about.remove());
  }
  // NSPanel's center(): across the middle, and a sixth of the way down what the menu bar leaves
  const u = unit(desk);
  const height = desk.clientHeight / u;
  panel.style.setProperty("--y", `${(24 + (height - 24 - 170) * 0.175) * u}px`);
  if (!panel.open) {
    panel.addEventListener("close", returnsFocus(), { once: true });
    desk.append(panel);
    panel.show();
  }
  panel.focus({ preventScroll: true });
}

export function showAccountWindow(desk: HTMLElement, system: System, a: Account) {
  let w = desk.querySelector<HTMLElement>(
    `:scope > div[data-part="account-window"][data-account="${a.id}"]`,
  );
  if (!w) {
    const open = desk.querySelectorAll(':scope > div[data-part="account-window"]').length;
    w = made(system, "account-window", desk, `window-${a.id}`);
    w.dataset.account = a.id;
    // its default size and place, each kept within what the desk leaves of its screen: under the
    // menu bar on macOS, over the taskbar on Windows
    const u = unit(desk);
    const [deskW, deskH] = [desk.clientWidth / u, desk.clientHeight / u];
    const [top, bottom] = system === "macos" ? [24, 0] : [0, 48];
    const width = Math.min(1100, deskW);
    const height = Math.min(800, deskH - top - bottom);
    const x = Math.min(40 + open * CASCADE, deskW - width);
    const y = Math.min(top + 16 + open * CASCADE, deskH - bottom - height);
    w.style.setProperty("--w", `${width * u}px`);
    w.style.setProperty("--h", `${height * u}px`);
    w.style.setProperty("--x", `${x * u}px`);
    w.style.setProperty("--y", `${y * u}px`);
    const back = returnsFocus();
    w.querySelector('[data-does="close"]')!.addEventListener("click", () => {
      w!.remove();
      closeWindow(a.id);
      back();
    });
    desk.append(w);
    titled(w, system, a, true);
  }
  // in front of the desk's other windows
  desk.append(w);
  w.querySelector<HTMLElement>('[data-does="reload"]')?.focus({ preventScroll: true });
}

function titled(w: HTMLElement, system: System, a: Account, opening = false) {
  const title = w.querySelector('[data-part="title"]');
  if (system === "macos") {
    text(title, a.label);
    text(w.querySelector('[data-part="site"]'), siteOf(a.tool) ?? "");
  } else if (opening) setTimeout(() => text(title, a.label), XAML_TITLE_MS);
  else if (title?.textContent !== "Account") text(title, a.label);
}

// every account window drawn, as the state has its account and its window
function follow(s: State) {
  for (const w of document.querySelectorAll<HTMLElement>(
    'div[data-part="account-window"][data-account]',
  )) {
    const a = s.accounts.find((x) => x.id === w.dataset.account);
    if (!a || !s.windowsOpen.includes(a.id)) w.remove();
    else titled(w, w.closest('[data-drawing="app windows"]') ? "windows" : "macos", a);
  }
}

model.subscribe(follow);
document.addEventListener("figures-reset", () => {
  for (const el of document.querySelectorAll(
    '[data-output] > div[data-part="account-window"], [data-output] > dialog[data-part="about"]',
  ))
    el.remove();
});
