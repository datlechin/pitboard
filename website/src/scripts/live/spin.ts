// The share's controls as each system draws them: on macOS a text field with its stepper beside it,
// two controls on one value, and on Windows WinUI's NumberBox, a spinbutton with its spin buttons
// inline. Both step by one within LOWEST to HIGHEST and give back text that is no figure.
import { HIGHEST, LOWEST } from "../../lib/model.ts";

export interface Share {
  set(share: number, enabled: boolean): void;
}

const within = (n: number) => Math.min(HIGHEST, Math.max(LOWEST, n));

// Steps while a press is held: one at once, one at `delay`, then one each `every`.
function holding(el: Element, delay: number, every: number, step: (e: PointerEvent) => void) {
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
const STEPPER_DELAY = 300;
const STEPPER_EVERY = 50;

// LimitsSettings.swift: a percent typed in the field, its "%" left out or not; text that is no
// whole figure goes back to the last value, and a figure out of range to the nearest bound.
// departs from the apps: The macOS share field takes a typed share on Return or as focus leaves
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
