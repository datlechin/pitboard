// <pb-notice>: the notification row, in its own moment (lib/moment.ts). Pitboard sets no alert style
// or duration, so each system's banner goes after the system's time (plan 3.6, 3.7; answer 2 a).
// departs from the apps: The Mac's clock opens nothing
import { attr, shown, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { bringForward } from "./forward.ts";
import { say } from "./say.ts";
import { noticeMoment, type Moment } from "../../lib/moment.ts";
import { model, type Posted, type State } from "../../lib/model.ts";
import {
  hm,
  menuBarClock,
  menuBarLevel,
  menuBarSpoken,
  menuBarShown,
  shortDate,
  type System,
} from "../../lib/words.ts";

// how long a banner stays, until measured on macOS 27 and Windows 11: Windows' default, which
// Settings > Accessibility > Visual effects sets
const SHOWN_MS = 5000;
const IN_VIEW = 0.6;

const said = (el: Element | null, text: string) => {
  if (el && el.textContent!.trim() !== text) el.textContent = text;
};

class Notice extends LiveFigure {
  #system: System = "macos";
  #banner!: HTMLElement;
  #moment: Moment | null = null;
  #inputs = "";
  // the notification last shown, which comes again only once the moment posts another
  #shown: string | null = null;
  #inView = false;
  #timer?: ReturnType<typeof setTimeout>;
  #pointer = false;
  #focus = false;
  #seen?: IntersectionObserver;
  // what Notification Center holds of the moment's notification, and the clock that opens it
  #kept: Posted | null = null;
  #center: HTMLDialogElement | null = null;
  #clock: HTMLElement | null = null;

  connectedCallback() {
    const drawing = this.closest<HTMLElement>("[data-drawing]")?.dataset.drawing ?? "";
    this.#system = drawing.endsWith("windows") ? "windows" : "macos";
    this.#banner = this.querySelector("[data-banner]")!;
    const b = this.#banner;
    b.addEventListener("pointerenter", () => this.#hold("pointer", true));
    b.addEventListener("pointerleave", () => this.#hold("pointer", false));
    b.addEventListener("focusin", () => this.#hold("focus", true));
    b.addEventListener("focusout", (e) => {
      if (!b.contains(e.relatedTarget as Node | null)) this.#hold("focus", false);
    });
    this.querySelector("[data-close]")?.addEventListener("click", () => this.#leave());
    this.querySelector("[data-switch]")?.addEventListener("click", () => this.#switch());
    this.#center = this.querySelector<HTMLDialogElement>('[data-part="center"]');
    this.#clock = this.querySelector('[data-does="center"]');
    if (this.#center && this.#clock) this.#listenCenter(this.#center, this.#clock);
    this.#seen = new IntersectionObserver(
      ([e]) => {
        this.#inView = e.isIntersecting && e.intersectionRatio >= IN_VIEW;
        this.#arrive();
      },
      { threshold: [0, IN_VIEW] },
    );
    this.#seen.observe(this);
    super.connectedCallback();
  }

  disconnectedCallback() {
    this.#seen?.disconnect();
    clearTimeout(this.#timer);
    super.disconnectedCallback();
  }

  protected render(s: State) {
    const inputs = JSON.stringify([
      s.preferences,
      s.running,
      s.accounts,
      s.inUse,
      s.watchService,
      s.general.menuBarShows,
    ]);
    if (inputs === this.#inputs) return;
    this.#inputs = inputs;
    this.#moment = noticeMoment(s);
    this.#draw();
    this.#arrive();
  }

  // Reset takes the row back to before its notification came
  protected restart() {
    this.#shown = null;
    this.#kept = null;
    this.#openCenter(false);
    this.#drawCenter();
    this.#arrive();
  }

  #listenCenter(center: HTMLDialogElement, clock: HTMLElement) {
    clock.addEventListener("click", () => this.#openCenter(!center.open));
    center.addEventListener("close", () => attr(clock, "aria-expanded", "false"));
    center.addEventListener("keydown", (e) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      this.#openCenter(false);
      clock.focus({ preventScroll: true });
    });
    center.addEventListener("click", (e) => {
      const does = (e.target as Element).closest<HTMLElement>("[data-does]")?.dataset.does;
      if (does === "dismiss" || does === "clear") return this.#forget();
      if (does === "switch") return this.#switch();
      // a click on the notification itself brings the app's window forward, as its toast's does
      if ((e.target as Element).closest('[data-part="note"]')) {
        this.#forget();
        this.#openCenter(false);
        bringForward("accounts", "windows");
      }
    });
    document.addEventListener(
      "pointerdown",
      (e) => {
        const at = e.target as Node;
        if (center.open && !center.contains(at) && !clock.contains(at)) this.#openCenter(false);
      },
      true,
    );
  }

  #openCenter(open: boolean) {
    const center = this.#center;
    if (!center || open === center.open) return;
    if (!open) return center.close();
    center.show();
    attr(this.#clock, "aria-expanded", "true");
    this.#leave();
    center.focus({ preventScroll: true });
  }

  #forget() {
    this.#kept = null;
    this.#drawCenter();
  }

  // the notification Notification Center holds, or none
  #drawCenter() {
    const c = this.#center;
    if (!c) return;
    const p = this.#kept;
    shown(c.querySelector('[data-part="none"]'), !p);
    shown(c.querySelector('[data-part="group"]'), !!p);
    shown(c.querySelector('[data-does="clear"]'), !!p);
    if (!p) return;
    text(c.querySelector(`[data-part="note-title"]`), p.title[this.#system]);
    const subtitle = c.querySelector<HTMLElement>("[data-subtitle]");
    shown(subtitle, p.subtitle !== null);
    if (p.subtitle) text(subtitle, p.subtitle);
    text(c.querySelector("[data-body]"), p.body);
    shown(c.querySelector("[data-switch-row]"), p.button !== null);
    if (p.button) text(c.querySelector('[data-does="switch"]'), p.button.title[this.#system]);
  }

  // The desk at the moment: its clock, and Pitboard's figure in the menu bar or the tray.
  #draw() {
    const m = this.#moment;
    if (!m) return;
    said(this.querySelector("[data-clock]"), menuBarClock(m.at));
    said(this.querySelector("[data-time]"), hm(m.at));
    said(this.querySelector("[data-date]"), shortDate(m.at));
    const title = this.querySelector("[data-title]");
    if (title) {
      said(title, menuBarShown(m.state, model.get().general.menuBarShows));
      title.parentElement!.setAttribute("aria-label", menuBarSpoken(m.state));
    }
    const level = menuBarLevel(m.state);
    for (const svg of this.querySelectorAll<SVGElement>("[data-level]"))
      svg.toggleAttribute("hidden", svg.dataset.level !== level);
  }

  #fill(p: Posted) {
    const b = this.#banner;
    said(b.querySelector(`#${this.id}-title`), p.title[this.#system]);
    const subtitle = b.querySelector<HTMLElement>("[data-subtitle]")!;
    subtitle.hidden = p.subtitle === null;
    if (p.subtitle) said(subtitle, p.subtitle);
    said(b.querySelector("[data-body]"), p.body);
    const button =
      b.querySelector<HTMLElement>("[data-switch-row]") ?? b.querySelector("[data-switch]")!;
    button.hidden = p.button === null;
    if (p.button) said(b.querySelector("[data-switch]"), p.button.title[this.#system]);
  }

  #arrive() {
    const p = this.#moment?.posted;
    if (!p) {
      this.#forget();
      return this.#leave();
    }
    const key = JSON.stringify([p.title, p.subtitle, p.body, p.button, this.#moment!.at]);
    if (!this.#inView || key === this.#shown) return;
    this.#shown = key;
    this.#fill(p);
    this.#kept = p;
    this.#drawCenter();
    this.#banner.dataset.state = this.#held() ? "held" : "in";
    say([p.title[this.#system], p.subtitle, p.body].filter(Boolean).join(", "));
    this.#wait();
  }

  #held = () => this.#pointer || this.#focus;

  #hold(by: "pointer" | "focus", on: boolean) {
    if (by === "pointer") this.#pointer = on;
    else this.#focus = on;
    if (!this.#banner.dataset.state) return;
    this.#banner.dataset.state = this.#held() ? "held" : "in";
    if (!this.#held()) this.#wait();
  }

  #wait() {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => {
      if (!this.#held()) this.#leave();
    }, SHOWN_MS);
  }

  #leave() {
    clearTimeout(this.#timer);
    if (!this.#banner.dataset.state) return;
    // focus inside the banner stays on the desk as the banner goes
    if (this.#banner.contains(document.activeElement)) {
      this.tabIndex = -1;
      this.focus({ preventScroll: true });
    }
    delete this.#banner.dataset.state;
    this.#pointer = this.#focus = false;
  }

  // Switch and Switch Back act in the moment: Pitboard switches to the account offered, and the desk
  // shows it.
  #switch() {
    const to = this.#moment?.posted?.button;
    if (!to) return;
    this.#moment!.state.inUse[to.tool] = to.label;
    this.#draw();
    this.#leave();
    // acted on, it leaves Notification Center too
    this.#forget();
    this.#openCenter(false);
  }
}

customElements.define("pb-notice", Notice);
