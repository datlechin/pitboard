// A control's tooltip in WAI-ARIA's tooltip pattern: each owner names its own `role="tooltip"` by
// aria-describedby, hidden until it shows. It shows after the system's delay at the pointer or,
// where the system shows one for keyboard focus, at its owner; the markup places it from there as
// its system does.
// departs from the apps: Tooltips stay while the pointer is over them and Escape closes them

export interface TipOptions {
  // the desk the tooltip is drawn in and kept inside
  desk: HTMLElement;
  // the owners within the desk that have a tooltip
  owners: string;
  // the system's delays: at the pointer, and for keyboard focus where it shows one at all
  delay: { pointer: number; keyboard: number | null };
}

interface Point {
  x: number;
  y: number;
}

// a pointer leaving its owner for the tooltip, or back, crosses the gap between them in this time
const GRACE_MS = 100;

export class Tooltips {
  #shown: { owner: HTMLElement; tip: HTMLElement } | null = null;
  #pending: HTMLElement | null = null;
  #timer = 0;
  #leaving = 0;
  #pointer: Point = { x: 0, y: 0 };

  constructor(readonly o: TipOptions) {
    const { desk } = o;
    desk.addEventListener("pointermove", (e) => {
      if (e.pointerType !== "touch") this.#pointer = this.#within(e.clientX, e.clientY);
    });
    desk.addEventListener("pointerover", (e) => {
      if (e.pointerType === "touch") return;
      this.#pointer = this.#within(e.clientX, e.clientY);
      const owner = this.#owner(e.target);
      if (this.#keeps(e.target as Element)) clearTimeout(this.#leaving);
      if (owner && this.#shown?.owner !== owner && this.#pending !== owner)
        this.#later(owner, o.delay.pointer, () => this.#pointer);
    });
    desk.addEventListener("pointerout", (e) => {
      const from = e.target as Element;
      const to = e.relatedTarget as Element | null;
      if (!this.#owner(from) && !this.#keeps(from)) return;
      if (to && (this.#keeps(to) || this.#owner(to) === this.#owner(from))) return;
      clearTimeout(this.#leaving);
      if (!this.#shown) {
        this.hide();
        return;
      }
      this.#leaving = window.setTimeout(() => this.hide(), GRACE_MS);
    });
    desk.addEventListener("focusin", (e) => {
      const owner = this.#owner(e.target);
      const delay = o.delay.keyboard;
      if (!owner || delay === null || !owner.matches(":focus-visible")) return;
      this.#later(owner, delay, () => this.#at(owner));
    });
    desk.addEventListener("focusout", () => this.hide());
    desk.addEventListener("pointerdown", () => this.hide(), true);
    // the first Escape takes a tooltip away, and only the next reaches the control
    desk.addEventListener(
      "keydown",
      (e) => {
        if (e.key === "Escape" && this.#shown) {
          e.stopPropagation();
          e.preventDefault();
          this.hide();
        }
      },
      true,
    );
  }

  #owner(target: EventTarget | null): HTMLElement | null {
    return target instanceof Element ? target.closest<HTMLElement>(this.o.owners) : null;
  }

  // the tooltip shown, or its owner, which the pointer may cross between
  #keeps(el: Element) {
    const s = this.#shown;
    return !!s && (s.owner.contains(el) || s.tip.contains(el));
  }

  #tip(owner: HTMLElement): HTMLElement | null {
    const id = owner.getAttribute("aria-describedby");
    const tip = id ? document.getElementById(id) : null;
    return tip && tip.textContent?.trim() ? tip : null;
  }

  #within(x: number, y: number): Point {
    const box = this.o.desk.getBoundingClientRect();
    return { x: x - box.left, y: y - box.top };
  }

  // the middle of an owner's top edge
  #at(owner: HTMLElement): Point {
    const r = owner.getBoundingClientRect();
    return this.#within(r.left + r.width / 2, r.top);
  }

  #later(owner: HTMLElement, delay: number, where: () => Point) {
    this.hide();
    this.#pending = owner;
    this.#timer = window.setTimeout(() => {
      this.#pending = null;
      if (owner.getClientRects().length > 0) this.#show(owner, where());
    }, delay);
  }

  #show(owner: HTMLElement, at: Point) {
    const tip = this.#tip(owner);
    // a dimmed control shows none, until measured on each system (plan section 10)
    if (!tip || owner.getAttribute("aria-disabled") === "true") return;
    tip.hidden = false;
    this.#shown = { owner, tip };
    tip.style.setProperty("--x", `${at.x}px`);
    tip.style.setProperty("--y", `${at.y}px`);
    // kept inside the desk, as a system keeps a tooltip on its screen
    const box = this.o.desk.getBoundingClientRect();
    const r = tip.getBoundingClientRect();
    const inset = 4;
    const dx =
      r.right > box.right - inset
        ? box.right - inset - r.right
        : r.left < box.left + inset
          ? box.left + inset - r.left
          : 0;
    if (dx) tip.style.setProperty("--x", `${at.x + dx}px`);
  }

  hide() {
    clearTimeout(this.#timer);
    clearTimeout(this.#leaving);
    this.#pending = null;
    if (!this.#shown) return;
    this.#shown.tip.hidden = true;
    this.#shown = null;
  }

  // a tooltip whose words the state took away goes
  refresh() {
    if (this.#shown && !this.#shown.tip.textContent?.trim()) this.hide();
  }
}
