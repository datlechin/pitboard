// The share's controls: on macOS a field with a stepper beside it, on Windows WinUI's NumberBox.
import { HIGHEST, LOWEST } from "../../lib/model.ts";

export interface Share {
  set(share: number, enabled: boolean): void;
}

const within = (n: number) => Math.min(HIGHEST, Math.max(LOWEST, n));

// Steps while a press is held: one at once, one at `delay`, then one each `every`.
export function holding(
  el: Element,
  delay: number,
  every: number,
  step: (e: PointerEvent) => void,
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const stop = () => {
    clearTimeout(timer);
    clearInterval(timer);
    const held = el as HTMLElement | SVGElement;
    if (held.dataset.state === "up" || held.dataset.state === "down") delete held.dataset.state;
  };
  el.addEventListener("pointerdown", (e) => {
    const p = e as PointerEvent;
    if (p.button !== 0) return;
    p.preventDefault();
    el.setPointerCapture(p.pointerId);
    step(p);
    timer = setTimeout(() => {
      step(p);
      timer = setInterval(() => step(p), every);
    }, delay);
  });
  for (const name of ["pointerup", "pointercancel", "lostpointercapture"])
    el.addEventListener(name, stop);
}

// The share's stepper held: a step at the press, then 20 a second from 0.3 s on, which gives the 6,
// 16 to 17 and 36 steps of holds of 0.5, 1 and 2 s that run 40252330837 read (docs/facts/site.md).
export const STEPPER_DELAY = 300;
export const STEPPER_EVERY = 50;

// LimitsSettings.swift: a percent typed in the field, its "%" left out or not; text that is no
// whole figure goes back to the last value, and a figure out of range to the nearest bound.
export function macShare(
  field: HTMLInputElement,
  stepper: SVGElement,
  setShare: (share: number) => void,
): Share {
  let share = Number.parseInt(field.value, 10);
  let enabled = true;
  // typed and not yet taken, which a change from the stepper leaves alone
  let typing = false;
  const said = (n: number) => `${n}%`;

  function commit() {
    typing = false;
    const typed = /^\s*(\d{1,3})\s*%?\s*$/.exec(field.value);
    const to = typed ? within(Number(typed[1])) : share;
    field.value = said(to);
    if (to !== share) setShare(to);
  }
  field.addEventListener("input", () => (typing = true));
  field.addEventListener("keydown", (e) => {
    if (e.key === "Enter") commit();
  });
  field.addEventListener("change", commit);

  const step = (by: number) => {
    if (!enabled) return;
    const to = within(share + by);
    if (to !== share) setShare(to);
  };
  holding(stepper, STEPPER_DELAY, STEPPER_EVERY, (e) => {
    if (!enabled) return;
    const box = stepper.getBoundingClientRect();
    const up = e.clientY < box.top + box.height / 2;
    stepper.dataset.state = up ? "up" : "down";
    step(up ? 1 : -1);
  });
  stepper.addEventListener("keydown", (e) => {
    const by =
      (e as KeyboardEvent).key === "ArrowUp"
        ? 1
        : (e as KeyboardEvent).key === "ArrowDown"
          ? -1
          : 0;
    if (!by) return;
    e.preventDefault();
    step(by);
  });

  return {
    set(to, on) {
      if (to !== share) {
        share = to;
        if (!typing) field.value = said(to);
        stepper.setAttribute("aria-valuenow", String(to));
      }
      if (on !== enabled) {
        enabled = on;
        field.disabled = !on;
        stepper.setAttribute("aria-disabled", String(!on));
        stepper.setAttribute("tabindex", on ? "0" : "-1");
      }
    },
  };
}

// RepeatButton's Delay and Interval, which NumberBox's spin buttons keep.
const SPIN_DELAY = 250;
const SPIN_EVERY = 250;
// NumberBox's SmallChange and LargeChange in SettingsPage.xaml
const SMALL = 1;
const LARGE = 5;

// .NET's Math.Round, which SettingsViewModel.ShareOf takes: a half goes to the even figure.
const toEven = (n: number) => {
  const whole = Math.floor(n);
  const part = n - whole;
  return part > 0.5 || (part === 0.5 && whole % 2 === 1) ? whole + 1 : whole;
};

// NumberBox.cpp: typed text counts on Enter or as focus leaves, Escape puts the value back, a
// number out of range is clamped, other text is overwritten, and a cleared box shows the share
// again (SettingsPage.xaml.cs OnShareChanged).
export function numberBox(
  input: HTMLInputElement,
  up: HTMLElement,
  down: HTMLElement,
  setShare: (share: number) => void,
): Share {
  let share = Number(input.value);
  let enabled = true;

  function validate() {
    const text = input.value.trim();
    const n = Number(text);
    const to = text === "" || Number.isNaN(n) ? share : within(toEven(n));
    input.value = String(to);
    if (to !== share) setShare(to);
    return to;
  }
  const step = (by: number) => {
    if (!enabled) return;
    const to = within(validate() + by);
    if (to !== share) setShare(to);
  };

  input.addEventListener("keydown", (e) => {
    const by = { ArrowUp: SMALL, ArrowDown: -SMALL, PageUp: LARGE, PageDown: -LARGE }[e.key];
    if (by) {
      e.preventDefault();
      return step(by);
    }
    if (e.key === "Enter") validate();
    if (e.key === "Escape") input.value = String(share);
  });
  input.addEventListener("change", validate);
  // the wheel steps only while the box has focus
  input.addEventListener(
    "wheel",
    (e) => {
      if (document.activeElement !== input || e.deltaY === 0) return;
      e.preventDefault();
      step(e.deltaY < 0 ? SMALL : -SMALL);
    },
    { passive: false },
  );
  for (const [button, by] of [
    [up, SMALL],
    [down, -SMALL],
  ] as const) {
    holding(button, SPIN_DELAY, SPIN_EVERY, () => {
      if (!enabled || button.dataset.state === "off") return;
      input.focus({ preventScroll: true });
      step(by);
    });
  }

  return {
    set(to, on) {
      if (to !== share) {
        share = to;
        input.value = String(to);
      }
      input.setAttribute("aria-valuenow", String(to));
      // the spin buttons turn off at the bounds
      if (to >= HIGHEST) up.dataset.state = "off";
      else if (up.dataset.state === "off") delete up.dataset.state;
      if (to <= LOWEST) down.dataset.state = "off";
      else if (down.dataset.state === "off") delete down.dataset.state;
      if (on !== enabled) {
        enabled = on;
        input.disabled = !on;
      }
    },
  };
}

// GtkSpinButton as an AdwSpinRow holds it (gtkspinbutton.c): the arrows step it, Page Up and Page
// Down a page, Ctrl+Home and Ctrl+End go to its ends, and a press on its - or + steps at once, then
// after TIMEOUT_INITIAL every TIMEOUT_REPEAT; wrapping, a step from an end goes to the other and
// one past an end stops on it, and otherwise a button turns off at its end; typed, Enter or leaving
// the field reads what was typed, and what reads as no number puts the value back.
export interface Spin {
  set(value: number, enabled: boolean): void;
}
export interface SpinOptions {
  min: number;
  max: number;
  step: number;
  page: number;
  wrap: boolean;
  said: (value: number) => string;
  // what was typed, read as a number, or null where it is none; a field nothing is typed in has
  // none of its own
  read?: (text: string) => number | null;
  take: (value: number) => void;
}
const GTK_SPIN_DELAY = 500;
const GTK_SPIN_EVERY = 50;

export function gtkSpin(
  input: HTMLInputElement,
  down: HTMLElement,
  up: HTMLElement,
  o: SpinOptions,
): Spin {
  let value = Number(input.getAttribute("aria-valuenow"));
  let enabled = true;
  // typed and not yet read, which a step puts back
  let typing = false;
  const to = (by: number) => {
    const next = value + by;
    if (by > 0) return o.wrap && value === o.max ? o.min : Math.min(next, o.max);
    return o.wrap && value === o.min ? o.max : Math.max(next, o.min);
  };
  const go = (n: number) => {
    typing = false;
    if (!enabled) return;
    if (n === value) input.value = o.said(value);
    else o.take(n);
  };
  function read() {
    if (!o.read || !typing) return;
    const n = o.read(input.value);
    go(n === null ? value : Math.min(o.max, Math.max(o.min, Math.round(n))));
  }
  input.addEventListener("input", () => (typing = true));
  // one the app takes no typing in: nothing typed, pasted or dropped lands
  if (!o.read) input.addEventListener("beforeinput", (e) => e.preventDefault());
  input.addEventListener("keydown", (e) => {
    const ends: Record<string, number> = {
      Home: o.min,
      End: o.max,
      PageUp: o.max,
      PageDown: o.min,
    };
    const steps: Record<string, number> = {
      ArrowUp: o.step,
      ArrowDown: -o.step,
      PageUp: o.page,
      PageDown: -o.page,
    };
    const n = e.ctrlKey ? ends[e.key] : steps[e.key] === undefined ? undefined : to(steps[e.key]);
    if (n !== undefined) {
      e.preventDefault();
      return go(n);
    }
    if (e.key === "Enter") read();
  });
  input.addEventListener("change", read);
  for (const [button, by] of [
    [up, o.step],
    [down, -o.step],
  ] as const)
    holding(button, GTK_SPIN_DELAY, GTK_SPIN_EVERY, () => {
      if (!enabled || button.dataset.state === "off") return;
      go(to(by));
    });

  return {
    set(n, on) {
      value = n;
      const said = o.said(n);
      if (!typing) input.value = said;
      input.setAttribute("aria-valuenow", String(n));
      input.setAttribute("aria-valuetext", said);
      for (const [button, end] of [
        [up, o.max],
        [down, o.min],
      ] as const)
        if (!o.wrap && n === end) button.dataset.state = "off";
        else if (button.dataset.state === "off") delete button.dataset.state;
      if (on !== enabled) {
        enabled = on;
        input.disabled = !on;
      }
    },
  };
}
