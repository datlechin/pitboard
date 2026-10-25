// A time of day as each system picks it. macOS's NSDatePicker with its stepper: the hour and the
// minute each take focus, the arrows step the one with focus round, digits typed set it, and the
// stepper steps the one last selected. WinUI's TimePicker: a button whose flyout holds a looping
// column each for the hour and the minute, changed only once Accept takes them. Both are
// WAI-ARIA spinbuttons, one for the hour and one for the minute.
import { STEPPER_DELAY, STEPPER_EVERY, holding } from "./spin.ts";

export interface Time {
  set(minutes: number): void;
}

const MAX = [23, 59];
const two = (n: number) => String(n).padStart(2, "0");
// the hour and the minute of a span's end, 24:00 as midnight's 00:00
const parts = (minutes: number) => [Math.floor(minutes / 60) % 24, minutes % 60];
const round = (v: number, i: number) => (v + MAX[i] + 1) % (MAX[i] + 1);

function said(el: HTMLElement, v: number) {
  if (el.getAttribute("aria-valuenow") === String(v)) return;
  el.setAttribute("aria-valuenow", String(v));
  el.setAttribute("aria-valuetext", two(v));
}

// digits typed into one part within this long make one figure
const TYPED_MS = 1000;

export function datePicker(group: HTMLElement, take: (minutes: number) => void): Time {
  const els = [...group.querySelectorAll<HTMLElement>("[data-part]")];
  let now = [0, 0];
  let last = 0;
  let typed = "";
  let typedAt = 0;
  const put = (i: number, v: number) => {
    const next = [...now];
    next[i] = v;
    take(next[0] * 60 + next[1]);
  };

  els.forEach((el, i) => {
    el.addEventListener("focus", () => {
      last = i;
      typed = "";
    });
    el.addEventListener("keydown", (e) => {
      const by = e.key === "ArrowUp" ? 1 : e.key === "ArrowDown" ? -1 : 0;
      if (by) {
        e.preventDefault();
        return put(i, round(now[i] + by, i));
      }
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        e.preventDefault();
        return els[Math.min(1, Math.max(0, i + (e.key === "ArrowLeft" ? -1 : 1)))].focus();
      }
      if (!/^\d$/.test(e.key)) return;
      e.preventDefault();
      const at = performance.now();
      typed = at - typedAt < TYPED_MS && typed.length === 1 ? typed + e.key : e.key;
      typedAt = at;
      if (Number(typed) > MAX[i]) typed = e.key;
      put(i, Number(typed));
      // a part full, or one no further digit could follow, moves on to the next
      if (typed.length === 2 || Number(typed) * 10 > MAX[i]) {
        typed = "";
        if (i === 0) els[1].focus();
      }
    });
  });

  const stepper = group.querySelector<SVGElement>("[data-stepper]")!;
  holding(stepper, STEPPER_DELAY, STEPPER_EVERY, (e) => {
    const box = stepper.getBoundingClientRect();
    els[last].focus({ preventScroll: true });
    put(last, round(now[last] + (e.clientY < box.top + box.height / 2 ? 1 : -1), last));
  });

  return {
    set(minutes) {
      now = parts(minutes);
      els.forEach((el, i) => {
        if (el.textContent !== two(now[i])) el.textContent = two(now[i]);
        said(el, now[i]);
      });
    },
  };
}

// TimePickerFlyoutPresenter: 398 pixels tall, its band's middle 177.5 below the top.
const FLYOUT = 398;
const BAND = 177.5;

export function timePicker(
  button: HTMLButtonElement,
  flyout: HTMLDialogElement,
  take: (minutes: number) => void,
): Time {
  const columns = [...flyout.querySelectorAll<HTMLElement>('[role="spinbutton"]')];
  const label = button.dataset.label!;
  const shown = [...button.querySelectorAll<HTMLElement>("[data-part]")];
  const focusable = () => [...columns, ...flyout.querySelectorAll<HTMLElement>("button")];
  let now = [0, 0];
  let draft = [0, 0];

  const draw = () =>
    columns.forEach((column, i) => {
      said(column, draft[i]);
      for (const row of column.querySelectorAll<HTMLElement>("[data-at]"))
        row.textContent = two(round(draft[i] + Number(row.dataset.at), i));
    });

  // over the button, its band on the button's middle, and kept inside the desk as the screen
  function place() {
    const desk = button.closest<HTMLElement>("[data-output]")!;
    const d = desk.getBoundingClientRect();
    const b = button.getBoundingClientRect();
    const u = d.width / Number(getComputedStyle(desk).getPropertyValue("--units") || 600);
    const top = b.top + b.height / 2 - d.top - BAND * u;
    const most = d.height - (FLYOUT + 8) * u;
    flyout.style.setProperty("--top", `${Math.max(8 * u, Math.min(top, most))}px`);
    flyout.style.setProperty("--left", `${b.left - d.left}px`);
  }

  function open() {
    draft = [...now];
    draw();
    place();
    flyout.show();
    button.setAttribute("aria-expanded", "true");
    columns[0].focus({ preventScroll: true });
    document.addEventListener("pointerdown", outside, true);
  }

  // what the flyout leaves as it closes, however it closes
  flyout.addEventListener("close", () => {
    button.setAttribute("aria-expanded", "false");
    document.removeEventListener("pointerdown", outside, true);
  });

  function close(accept: boolean) {
    flyout.close();
    button.focus({ preventScroll: true });
    if (accept && (draft[0] !== now[0] || draft[1] !== now[1])) take(draft[0] * 60 + draft[1]);
  }

  // a flyout goes on a press anywhere else, taking nothing
  function outside(e: PointerEvent) {
    const t = e.target as Node;
    if (!flyout.contains(t) && !button.contains(t)) close(false);
  }

  const step = (i: number, by: number) => {
    draft[i] = round(draft[i] + by, i);
    draw();
  };

  button.addEventListener("click", () => (flyout.open ? close(false) : open()));
  columns.forEach((column, i) => {
    column.addEventListener("keydown", (e) => {
      const by = { ArrowUp: -1, ArrowDown: 1 }[e.key];
      if (by) {
        e.preventDefault();
        return step(i, by);
      }
      if (e.key === "Home" || e.key === "End") {
        e.preventDefault();
        draft[i] = e.key === "Home" ? 0 : MAX[i];
        draw();
      }
    });
    column.addEventListener("click", (e) => {
      const row = (e.target as Element).closest<HTMLElement>("[data-at]");
      if (row) step(i, Number(row.dataset.at));
      column.focus({ preventScroll: true });
    });
    column.addEventListener(
      "wheel",
      (e) => {
        if (e.deltaY === 0) return;
        e.preventDefault();
        step(i, e.deltaY > 0 ? 1 : -1);
      },
      { passive: false },
    );
  });
  flyout.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      return close(false);
    }
    if (e.key === "Enter" && !(e.target as Element).closest("button")) {
      e.preventDefault();
      return close(true);
    }
    // Tab goes round the flyout, as focus stays in a popup
    if (e.key === "Tab") {
      const all = focusable();
      const at = all.indexOf(document.activeElement as HTMLElement);
      e.preventDefault();
      all[(at + (e.shiftKey ? -1 : 1) + all.length) % all.length].focus();
    }
  });
  flyout.querySelector("[data-accept]")!.addEventListener("click", () => close(true));
  flyout.querySelector("[data-dismiss]")!.addEventListener("click", () => close(false));

  return {
    set(minutes) {
      now = parts(minutes);
      shown.forEach((el, i) => {
        if (el.textContent !== two(now[i])) el.textContent = two(now[i]);
      });
      const name = `${label} ${two(now[0])}:${two(now[1])}`;
      if (button.getAttribute("aria-label") !== name) button.setAttribute("aria-label", name);
    },
  };
}
