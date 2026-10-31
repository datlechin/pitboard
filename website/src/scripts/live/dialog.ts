// A sheet, an alert, a ContentDialog or an AdwDialog as HTML's modal dialog (answer 15), placed and
// dimmed as docs/facts/site.md reads each system's.
// departs from the apps: A sheet, an alert or a ContentDialog blocks the whole page
import type { System } from "../../lib/platform.ts";

export type Cancelled = "key" | "button";

export interface DialogOptions {
  system: System;
  // the window it opens over, and that window's desk
  window: HTMLElement;
  desk: HTMLElement;
  // what a button or Return asks for, by the button's value; the dialog stays up until closed
  answer: (value: string) => void;
  // Escape, ⌘. or the Cancel button: false where the dialog cannot be put away that way now
  cancel: (by: Cancelled) => boolean;
  // what to focus as it opens, where not its first control
  focus?: HTMLElement | null;
}

const wired = new WeakSet<HTMLDialogElement>();
const open = new WeakMap<HTMLDialogElement, DialogOptions>();

// What a dialog opens over: a sheet hangs from its window's title bar on macOS, a ContentDialog
// centres on the desk, and an AdwDialog centres on its window where the desk shows all of it, or
// on the desk that cuts it.
function overWindow(o: DialogOptions) {
  if (o.system === "macos") return true;
  if (o.system === "windows") return false;
  const w = o.window.getBoundingClientRect();
  const d = o.desk.getBoundingClientRect();
  return w.top >= d.top && w.bottom <= d.bottom && w.left >= d.left && w.right <= d.right;
}

// Each window and desk names itself after its desk, or the window after itself where its desk has
// no id, so no two share an anchor.
function anchor(o: DialogOptions, dialog: HTMLDialogElement) {
  const key = o.desk.id || o.window.id;
  o.window.style.setProperty("--window", `--window-${key}`);
  o.desk.style.setProperty("--desk", `--desk-${key}`);
  dialog.style.setProperty("--over", overWindow(o) ? `--window-${key}` : `--desk-${key}`);
}

function wire(dialog: HTMLDialogElement) {
  if (wired.has(dialog)) return;
  wired.add(dialog);
  const form = dialog.querySelector("form")!;
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const value = (e.submitter as HTMLButtonElement | null)?.value ?? "";
    open.get(dialog)?.answer(value);
  });
  dialog.addEventListener("cancel", (e) => {
    e.preventDefault();
    const o = open.get(dialog);
    if (o?.cancel("key")) closeDialog(dialog);
  });
  // A browser may close a modal dialog itself: Chromium does on a second Escape with no press
  // between. One that cannot be put away comes back up.
  dialog.addEventListener("close", () => {
    const o = open.get(dialog);
    if (!o) return;
    if (o.cancel("key")) release(dialog, o);
    else dialog.showModal();
  });
  dialog.addEventListener("click", (e) => {
    const does = (e.target as Element).closest<HTMLElement>("[data-does]")?.dataset.does;
    const o = open.get(dialog);
    if (!o || !does) return;
    if (does === "cancel") {
      if (o.cancel("button")) closeDialog(dialog);
    } else o.answer(does);
  });
  dialog.addEventListener("keydown", (e) => {
    const o = open.get(dialog);
    if (o?.system !== "macos" || e.isComposing) return;
    if (e.key === "." && e.metaKey) {
      e.preventDefault();
      if (o.cancel("key")) closeDialog(dialog);
      return;
    }
    if (e.key !== "Enter") return;
    const target = e.target as HTMLElement;
    // Return in a field submits the form by itself
    if (target instanceof HTMLInputElement) return;
    e.preventDefault();
    // Return presses the default button wherever focus is, and nothing where there is none
    const main = form.querySelector<HTMLButtonElement>('button[type="submit"]:not([hidden])');
    if (main && !main.disabled) form.requestSubmit(main);
  });
}

export function openDialog(dialog: HTMLDialogElement, o: DialogOptions) {
  wire(dialog);
  open.set(dialog, o);
  anchor(o, dialog);
  dim(o, true);
  dialog.showModal();
  reveal(dialog);
  if (o.focus) o.focus.focus({ preventScroll: true });
}

// A modal dialog stands still as the page scrolls, over its window: the page scrolls the least that
// shows all of it.
function reveal(dialog: HTMLDialogElement) {
  const r = dialog.getBoundingClientRect();
  const below = r.bottom - innerHeight;
  if (below > 0) scrollBy({ top: Math.min(below, r.top), behavior: "instant" });
  else if (r.top < 0) scrollBy({ top: r.top, behavior: "instant" });
}

export function closeDialog(dialog: HTMLDialogElement) {
  const o = open.get(dialog);
  if (o) release(dialog, o);
  if (dialog.open) dialog.close();
}

function release(dialog: HTMLDialogElement, o: DialogOptions) {
  open.delete(dialog);
  dim(o, false);
}

export const isOpen = (dialog: HTMLDialogElement | null) => !!dialog && open.has(dialog);

// the window's smoke, or libadwaita's dimming, which a window drawn dimmed keeps
const drawnDim = new WeakMap<HTMLElement, boolean>();
function dim(o: DialogOptions, on: boolean) {
  o.window.toggleAttribute("data-modal", on);
  const smoke = o.window.querySelector<HTMLElement>(
    ':scope > :is([data-part="smoke"], [data-part="dimming"])',
  );
  if (!smoke) return;
  if (!drawnDim.has(smoke)) drawnDim.set(smoke, !smoke.hidden);
  smoke.hidden = !on && !drawnDim.get(smoke);
}
