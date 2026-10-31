// <pb-gnome-desk>: the hero's desk on stock GNOME (plan 3.3 "Hero"), the top bar's clock and its
// date menu, Ptyxis's status line, and the Pitboard window, its own figure. Its demo presses work's
// Switch, and the window switches as the app does, its spinner turning until the switch is made.
import { center } from "./center.ts";
import { handOver, type Switches } from "./demo.ts";
import { html, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { model, switchFromApp, type State } from "../../lib/model.ts";
import { statusLine } from "../../lib/words.ts";
import { topBarClock } from "../../data/gnome.ts";

class GnomeDesk extends LiveFigure implements Switches {
  connectedCallback() {
    const clock = this.querySelector<HTMLElement>('[data-does="center"]');
    const menu = this.querySelector<HTMLDialogElement>('[data-part="center"]');
    if (clock && menu) center(clock, menu);
    handOver(() => {
      // the visitor's own pointer takes over from the one the demo draws
      this.querySelector('[data-part="pointer"]')?.setAttribute("data-state", "gone");
    });
    super.connectedCallback();
  }

  // the demo's switch to work, made from the window's Switch, done once the window has made it
  switches(done: () => void) {
    switchFromApp("claude", "work", "gnome");
    const stop = model.subscribe((s) => {
      if (s.switching || s.inUse.claude !== "work") return;
      queueMicrotask(() => stop());
      done();
    });
  }

  protected render(s: State) {
    const clock = topBarClock(s.now);
    const button = this.querySelector('[data-does="center"]');
    text(button?.querySelector('[data-part="date"]'), clock.date);
    text(button?.querySelector('[data-part="time"]'), clock.time);
    html(this.querySelector('[data-part="status"]'), statusLine(s));
  }
}

customElements.define("pb-gnome-desk", GnomeDesk);
