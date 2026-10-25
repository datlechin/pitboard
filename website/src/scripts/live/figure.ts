// A figure that works: the light-DOM custom element a figure component renders around its drawing,
// inert until its live module defines it (plan 2.4). Each draws the one state on every change; once
// defined it speaks for itself, so the caption's description of its drawing goes, and the caption's
// Reset shows while anything on the page has changed. Reset in any caption brings every figure's
// own controls back to their start too.
import { model, reset, run, type State } from "../../lib/model.ts";

const RESET = "figures-reset";

export abstract class LiveFigure extends HTMLElement {
  #stop?: () => void;
  #restart = () => this.restart();

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
        document.dispatchEvent(new Event(RESET));
        // the button goes with the change it undid, so focus moves to the first control of the
        // drawing shown, where a figure draws one for each system
        const shown = [...figure!.querySelectorAll<HTMLElement>("[data-output]")].find(
          (root) => root.offsetParent !== null && customElements.get(root.localName),
        );
        shown?.querySelector<HTMLElement>('[tabindex="0"], button:not(:disabled)')?.focus();
      };
    this.#stop = model.subscribe((state) => {
      this.render(state);
      if (again) again.hidden = !state.changed;
    });
    document.addEventListener(RESET, this.#restart);
    run();
  }

  disconnectedCallback() {
    this.#stop?.();
    document.removeEventListener(RESET, this.#restart);
  }

  protected abstract render(state: State): void;

  // the figure's own controls as they start: menus, selection, dialogs
  protected restart() {}
}
