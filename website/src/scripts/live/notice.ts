// <pb-notice>: the notification row, in its own moment (lib/moment.ts). Pitboard sets no alert style
// or duration, so each system's banner goes after the system's time (plan 3.6, 3.7; answer 2 a):
// macOS's banner, Windows' toast and Notification Center, GNOME Shell's banner and date menu, and
// KDE Plasma's popup over its tray.
// departs from the apps: The Mac's clock opens nothing
import { center, type Center } from "./center.ts";
import { attr, shown, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { bringForward } from "./forward.ts";
import { say } from "./say.ts";
import { noticeMoment, type Moment } from "../../lib/moment.ts";
import { model, showAppWindow, type Posted, type State } from "../../lib/model.ts";
import {
  hm,
  menuBarClock,
  menuBarLevel,
  menuBarSpoken,
  menuBarShown,
  shortDate,
} from "../../lib/words.ts";
import { frontOf, systemOf, type System } from "../../lib/platform.ts";
import { topBarClock } from "../../data/gnome.ts";
import { trayAttention } from "../../data/linux.ts";

// how long a banner stays: macOS's, until measured on macOS 27; Windows' default, which Settings >
// Accessibility > Visual effects sets; GNOME Shell's NOTIFICATION_TIMEOUT (messageTray.js); and
// Plasma's PopupTimeout (libnotificationmanager's notificationsettings.kcfg)
const SHOWN_MS: Record<System, number> = { macos: 5000, windows: 5000, gnome: 4000, kde: 5000 };
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
  // what Notification Center or the date menu holds of the moment's notification, and the clock
  // that opens it
  #kept: Posted | null = null;
  #center: HTMLDialogElement | null = null;
  #opens: Center | null = null;

  connectedCallback() {
    this.#system = systemOf(this);
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
    // on Linux a click on the notification itself opens the window (notices.rs `posted`)
    if (this.#system === "gnome" || this.#system === "kde")
      b.addEventListener("click", (e) => {
        if (!(e.target as Element).closest("button")) this.#opened();
      });
    this.#center = this.querySelector<HTMLDialogElement>('[data-part="center"]');
    const clock = this.querySelector<HTMLElement>('[data-does="center"]');
    if (this.#center && clock) this.#listenCenter(this.#center, clock);
    // nothing is kept, and Plasma's bell is off, until the notification comes
    this.#drawCenter();
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
    this.#opens?.open(false);
    this.#drawCenter();
    this.#arrive();
  }

  #listenCenter(dialog: HTMLDialogElement, clock: HTMLElement) {
    this.#opens = center(clock, dialog, { opened: () => this.#leave() });
    dialog.addEventListener("click", (e) => {
      const at = e.target as Element;
      const does = at.closest<HTMLElement>("[data-does]")?.dataset.does;
      if (does === "dismiss" || does === "clear") return this.#forget();
      if (does === "switch") return this.#switch();
      // GNOME's Expand shows the message's actions in the list
      if (does === "expand") {
        const note = at.closest<HTMLElement>('[data-part="note"]')!;
        const open = !note.hasAttribute("data-expanded");
        note.toggleAttribute("data-expanded", open);
        attr(at.closest('[data-does="expand"]'), "aria-expanded", String(open));
        return;
      }
      // a click on the notification itself brings the app's window forward, as its toast's does
      if (at.closest('[data-part="note"]')) this.#opened();
    });
  }

  // the notification activated: it goes, and the app's window comes into view
  #opened() {
    this.#leave();
    this.#forget();
    this.#opens?.open(false);
    if (this.#system === "kde") showAppWindow();
    bringForward("accounts", this.#system);
  }

  #forget() {
    this.#kept = null;
    this.#drawCenter();
  }

  // the notification Notification Center or the date menu holds, or none; Plasma's bell shows
  // while it is unread
  #drawCenter() {
    shown(this.querySelector('[data-part="bell"]'), !!this.#kept);
    const c = this.#center;
    if (!c) return;
    const p = this.#kept;
    shown(c.querySelector('[data-part="none"]'), !p);
    shown(c.querySelector('[data-part="group"]'), !!p);
    shown(c.querySelector('[data-does="clear"]'), !!p);
    if (!p) {
      const note = c.querySelector('[data-part="note"]');
      note?.removeAttribute("data-expanded");
      attr(note?.querySelector('[data-does="expand"]'), "aria-expanded", "false");
      return;
    }
    text(c.querySelector(`[data-part="note-title"]`), p.title[frontOf(this.#system)]);
    const subtitle = c.querySelector<HTMLElement>("[data-subtitle]");
    shown(subtitle, p.subtitle !== null);
    if (p.subtitle) text(subtitle, p.subtitle);
    text(c.querySelector("[data-body]"), p.body);
    shown(c.querySelector("[data-switch-row]"), p.button !== null);
    if (p.button)
      text(c.querySelector('[data-does="switch"]'), p.button.title[frontOf(this.#system)]);
  }

  // The desk at the moment: its clock, and Pitboard's figure in the menu bar or the tray.
  #draw() {
    const m = this.#moment;
    if (!m) return;
    if (this.#system === "gnome") {
      const clock = topBarClock(m.at);
      const button = this.querySelector('[data-does="center"]');
      said(button?.querySelector('[data-part="date"]') ?? null, clock.date);
      said(button?.querySelector('[data-part="time"]') ?? null, clock.time);
    }
    said(this.querySelector("[data-clock]"), menuBarClock(m.at));
    said(this.querySelector("[data-time]"), hm(m.at));
    said(this.querySelector("[data-date]"), shortDate(m.at));
    const title = this.querySelector("[data-title]");
    if (title) {
      said(title, menuBarShown(m.state, model.get().general.menuBarShows));
      title.parentElement!.setAttribute("aria-label", menuBarSpoken(m.state));
    }
    // the icon at its level, and on KDE Plasma its attention twin while the menu needs attention
    const level = menuBarLevel(m.state);
    const attention = String(this.#system === "kde" && trayAttention(m.state));
    for (const svg of this.querySelectorAll<SVGElement>("[data-level]"))
      svg.toggleAttribute(
        "hidden",
        svg.dataset.level !== level || (svg.dataset.attention ?? attention) !== attention,
      );
    attr(this.querySelector("button[data-attention]"), "data-attention", attention);
  }

  #fill(p: Posted) {
    const b = this.#banner;
    said(b.querySelector(`#${this.id}-title`), p.title[frontOf(this.#system)]);
    const subtitle = b.querySelector<HTMLElement>("[data-subtitle]")!;
    subtitle.hidden = p.subtitle === null;
    if (p.subtitle) said(subtitle, p.subtitle);
    said(b.querySelector("[data-body]"), p.body);
    const button =
      b.querySelector<HTMLElement>("[data-switch-row]") ?? b.querySelector("[data-switch]")!;
    button.hidden = p.button === null;
    if (p.button) said(b.querySelector("[data-switch]"), p.button.title[frontOf(this.#system)]);
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
    say([p.title[frontOf(this.#system)], p.subtitle, p.body].filter(Boolean).join(", "));
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
    }, SHOWN_MS[this.#system]);
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
    this.#moment!.state.advice = [];
    this.#draw();
    this.#leave();
    // acted on, it leaves Notification Center too
    this.#forget();
    this.#opens?.open(false);
  }
}

customElements.define("pb-notice", Notice);
