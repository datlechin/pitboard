// The hero's opening demo (plan section 4): it plays once, makes its switch in the state, and stops
// at a touch on a working figure.
import { settle, use } from "../../lib/model.ts";

export type Frame = "start" | "move" | "hl" | "pick" | "shut" | "reach" | null;

const steps: [number, Frame][] = [
  [500, "move"],
  [1400, "hl"],
  [1900, "pick"],
  [2100, "shut"],
  [2900, "reach"],
  [3800, null],
];
const root = document.documentElement;
const takers: ((frame: Frame) => void)[] = [];
let phase: "waiting" | "playing" | "over" = "waiting";
let frame: Frame = "start";
let timers: number[] = [];
let switched = false;
let seen: IntersectionObserver | null = null;

// A hero desk takes over once the demo hands over, in the frame it was at: null for its last.
export function handOver(take: (frame: Frame) => void) {
  if (phase === "over") take(null);
  else takers.push(take);
}

function pick() {
  if (switched) return;
  switched = true;
  use("claude", "work", "app");
  // the page rests here: Reset comes back to it
  settle();
}

function end(at: Frame) {
  if (phase === "over") return;
  phase = "over";
  for (const t of timers) clearTimeout(t);
  timers = [];
  seen?.disconnect();
  root.removeAttribute("data-demo");
  for (const take of takers.splice(0)) take(at);
}

function now() {
  pick();
  end(null);
}

function play() {
  phase = "playing";
  for (const [at, step] of steps)
    timers.push(
      window.setTimeout(() => {
        frame = step;
        if (step === "pick") pick();
        if (step) root.setAttribute("data-demo", step);
        else end(null);
      }, at),
    );
}

function watch() {
  const desks = [...document.querySelectorAll<HTMLElement>("[data-demo-desk]")];
  const figure = desks[0]?.closest("figure");
  if (!figure) return;
  const ratios = new Map<Element, number>();
  const check = () => {
    if (phase !== "waiting") return;
    const desk = root.dataset.surface !== "cli" && desks.find((d) => d.offsetParent !== null);
    const shown = desk || figure;
    if ((ratios.get(shown) ?? 0) < 0.6 - 1e-3) return;
    if (desk && root.getAttribute("data-demo") === "start") play();
    else now();
  };
  seen = new IntersectionObserver(
    (entries) => {
      for (const e of entries) ratios.set(e.target, e.isIntersecting ? e.intersectionRatio : 0);
      check();
    },
    { threshold: [0, 0.6] },
  );
  for (const el of [...desks, figure]) seen.observe(el);
  document.addEventListener("choice", check);
}

// a figure works once its element is defined
const works = (target: EventTarget | null) => {
  const figure = target instanceof Element ? target.closest("[data-output]") : null;
  return !!figure && customElements.get(figure.localName) !== undefined;
};

for (const type of ["pointerdown", "keydown", "focusin"])
  document.addEventListener(
    type,
    (e) => {
      if (phase === "over" || !works(e.target)) return;
      if (phase === "waiting") now();
      else end(frame);
    },
    true,
  );

// head.js starts the demo on its first frame unless motion is reduced
if (root.hasAttribute("data-demo")) watch();
else now();
