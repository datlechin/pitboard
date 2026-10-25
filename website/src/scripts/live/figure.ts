// A figure that works: the light-DOM custom element a figure component renders around its drawing,
// inert until its live module defines it (plan 2.4). Each draws the one state on every change; once
// defined it speaks for itself, so the caption's description of its drawing goes, and the caption's
// Reset shows while anything on the page has changed.
import { model, reset, run, type State } from "../../lib/model.ts";

export abstract class LiveFigure extends HTMLElement {
  #stop?: () => void;

  connectedCallback() {
    this.inert = false;
    const figure = this.closest("figure");
    // `data-drawing` is "<surface> <platform>", as Figure.astro and Term.astro mark each drawing
    const [surface, platform] = (
      this.closest<HTMLElement>("[data-drawing]")?.dataset.drawing ?? ""
    ).split(" ");
    for (const words of figure?.querySelectorAll<HTMLElement>("[data-describes]") ?? []) {
      const on = words.dataset.platforms?.split(" ") ?? [];
      if (words.dataset.describes === surface && on.includes(platform)) words.hidden = true;
    }
    const again = figure?.querySelector<HTMLButtonElement>("[data-reset]");
    if (again)
      again.onclick = () => {
        reset();
        // the button goes with the change it undid, so focus moves to the figure's first control
        this.querySelector<HTMLElement>('[tabindex="0"], button:not(:disabled)')?.focus();
      };
    this.#stop = model.subscribe((state) => {
      this.render(state);
      if (again) again.hidden = !state.changed;
    });
    run();
  }

  disconnectedCallback() {
    this.#stop?.();
  }

  protected abstract render(state: State): void;
}
