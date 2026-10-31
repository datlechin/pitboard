// A clock that opens the notifications it keeps, as the taskbar's opens Notification Center on
// Windows and the top bar's the date menu on GNOME: a button with aria-expanded over a
// non-modal dialog, which Escape and a press elsewhere close, Escape giving focus back to the clock.
import { attr } from "./draw.ts";

export interface Center {
  open(on: boolean): void;
}

export function center(
  clock: HTMLElement,
  dialog: HTMLDialogElement,
  o: { opened?: () => void } = {},
): Center {
  const open = (on: boolean) => {
    if (on === dialog.open) return;
    if (!on) return dialog.close();
    dialog.show();
    attr(clock, "aria-expanded", "true");
    o.opened?.();
    dialog.focus({ preventScroll: true });
  };
  clock.addEventListener("click", () => open(!dialog.open));
  dialog.addEventListener("close", () => attr(clock, "aria-expanded", "false"));
  dialog.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    e.preventDefault();
    open(false);
    clock.focus({ preventScroll: true });
  });
  document.addEventListener(
    "pointerdown",
    (e) => {
      const at = e.target as Node;
      if (dialog.open && !dialog.contains(at) && !clock.contains(at)) open(false);
    },
    true,
  );
  return { open };
}
