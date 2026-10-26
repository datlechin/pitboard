// A touch held still on a row opens its shortcut menu, as iOS Safari sends no contextmenu: 500 ms
// within 10 px, until measured on a phone (plan section 10).

const HOLD_MS = 500;
const SLOP_PX = 10;

export function hold(
  root: HTMLElement,
  target: string,
  open: (on: HTMLElement, x: number, y: number) => void,
) {
  let timer = 0;
  let from: { x: number; y: number } | null = null;
  let held = false;
  const stop = () => {
    clearTimeout(timer);
    from = null;
  };
  root.addEventListener("pointerdown", (e) => {
    const on = (e.target as Element).closest<HTMLElement>(target);
    if (e.pointerType !== "touch" || !on) return;
    held = false;
    from = { x: e.clientX, y: e.clientY };
    timer = window.setTimeout(() => {
      held = true;
      open(on, from!.x, from!.y);
      from = null;
    }, HOLD_MS);
  });
  root.addEventListener("pointermove", (e) => {
    if (from && Math.hypot(e.clientX - from.x, e.clientY - from.y) > SLOP_PX) stop();
  });
  for (const type of ["pointerup", "pointercancel"]) root.addEventListener(type, stop);
  root.addEventListener("contextmenu", () => clearTimeout(timer), true);
  root.addEventListener(
    "click",
    (e) => {
      if (!held) return;
      held = false;
      e.preventDefault();
      e.stopPropagation();
    },
    true,
  );
}
