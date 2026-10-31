// A figure that works: the custom element around its drawing, inert until its module defines it
// (plan 2.4).
import { appMenus, type AppMenus, type WindowMenus } from "./appmenu.ts";
import { shown, text } from "./draw.ts";
import { bringForward, help } from "./forward.ts";
import { showAbout, showAccountWindow } from "./windows.ts";
import { model, openWindow, quit, reset, run, type State } from "../../lib/model.ts";
import { menuBarClock } from "../../lib/words.ts";

const RESET = "figures-reset";

export abstract class LiveFigure extends HTMLElement {
  #stop?: () => void;
  #restart = () => {
    this.menuBar?.close();
    this.restart();
  };
  // a figure another system's or surface's choice hides draws nothing until it shows again
  // (plan 3.5), but for one that keeps every state, as a terminal's running program does
  protected drawsHidden = false;
  #behind = false;
  #shown = () => {
    if (this.#behind && this.offsetParent !== null) this.#draw(model.get());
  };
  protected menuBar: AppMenus | null = null;

  connectedCallback() {
    this.inert = false;
    this.menuBar = appMenus(this, {
      menus: () => this.windowMenus(),
      choose: (does, item) => {
        if (!this.fromMenuBar(does, item)) fromMenuBar(does, item, this);
      },
    });
    const figure = this.closest("figure");
    // Figure.astro and Term.astro mark each drawing "<surface> <system>" and the systems it shows
    // to, and each caption the systems whose drawing it describes
    const drawing = this.closest<HTMLElement>("[data-drawing]");
    const surface = drawing?.dataset.drawing?.split(" ")[0];
    const mine = drawing?.dataset.on?.split(" ") ?? [];
    for (const words of figure?.querySelectorAll<HTMLElement>("[data-describes]") ?? []) {
      const on = words.closest<HTMLElement>("[data-on]")?.dataset.on?.split(" ") ?? [];
      if (words.dataset.describes === surface && on.some((s) => mine.includes(s)))
        words.hidden = true;
    }
    const again = figure?.querySelector<HTMLButtonElement>("[data-reset]");
    if (again)
      again.onclick = () => {
        reset();
        document.dispatchEvent(new Event(RESET));
        // the button goes with the change it undid, so focus moves to the first control in the
        // Tab order of the drawing shown, where a figure draws one for each system, past what a
        // dialog over it puts out of reach
        const shown = [...figure!.querySelectorAll<HTMLElement>("[data-output]")].find(
          (root) => root.offsetParent !== null && customElements.get(root.localName),
        );
        [...(shown?.querySelectorAll<HTMLElement>('[tabindex="0"], button:not(:disabled)') ?? [])]
          .find((el) => el.tabIndex >= 0 && !el.closest("[inert]"))
          ?.focus();
      };
    this.#stop = model.subscribe((state) => {
      if (again) again.hidden = !state.changed;
      this.#behind = !this.drawsHidden && this.offsetParent === null;
      if (!this.#behind) this.#draw(state);
    });
    document.addEventListener(RESET, this.#restart);
    document.addEventListener("choice", this.#shown);
    run();
  }

  #draw(state: State) {
    this.#behind = false;
    this.render(state);
    if (this.menuBar) {
      // the app's menus leave the menu bar once it has quit
      shown(this.menuBar.bar, state.running);
      if (!state.running) this.menuBar.close();
      this.menuBar.render(state);
      text(this.menuBar.bar.parentElement!.querySelector("[data-clock]"), menuBarClock(state.now));
    }
  }

  disconnectedCallback() {
    this.#stop?.();
    document.removeEventListener(RESET, this.#restart);
    document.removeEventListener("choice", this.#shown);
  }

  protected abstract render(state: State): void;

  // the figure's own controls as they start: menus, selection, dialogs
  protected restart() {}

  // what the window's menu bar acts on: its account selected and its Refresh
  protected windowMenus(): WindowMenus {
    return { account: null, refresh: null, resizes: true };
  }

  // an item of the menu bar the window answers itself, where it does
  protected fromMenuBar(_does: string, _item: HTMLElement): boolean {
    return false;
  }
}

// What the app does for an item of macOS's menu bar, the one system whose figures draw one,
// whichever of its windows is in front, on `desk`.
function fromMenuBar(does: string, item: HTMLElement, desk: HTMLElement) {
  const s = model.get();
  switch (does) {
    case "settings":
      return bringForward("settings", "macos");
    case "add":
      return bringForward("accounts", "macos", "add");
    case "about":
      return showAbout(desk);
    case "open": {
      const a = s.accounts.find((x) => x.id === item.dataset.account);
      if (!a) return;
      openWindow(a.tool, a.label);
      return showAccountWindow(desk, "macos", a);
    }
    case "help":
      return help();
    case "quit":
      return quit();
  }
}
