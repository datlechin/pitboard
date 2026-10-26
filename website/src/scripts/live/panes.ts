// A window's panes (answer 4 a): macOS's sidebar and WinUI's NavigationView, in WAI-ARIA's tabs
// pattern. A page shown `under` another, as Windows' Diagnostics and Activity are under Settings,
// keeps that page's tab selected and goes back to it.

export interface PanesOptions {
  system: "macos" | "windows";
  window: HTMLElement;
  // a pane has shown, the first time or again
  shown: (pane: string) => void;
}

// the window's width in units, by which its height is kept in units as the page scales it
const WIDTH = { macos: 640, windows: 704 };

export class Panes {
  readonly tabs: HTMLElement[];
  readonly first: string;
  current: string;
  // each page shown under another, and that page
  readonly #under = new Map<string, string>();

  constructor(readonly o: PanesOptions) {
    this.tabs = [...o.window.querySelectorAll<HTMLElement>('[role="tab"][data-pane]')];
    for (const panel of o.window.querySelectorAll<HTMLElement>("[data-pane-panel][data-under]"))
      this.#under.set(panel.dataset.panePanel!, panel.dataset.under!);
    const shownPanel = o.window.querySelector<HTMLElement>("[data-pane-panel]:not([hidden])");
    this.current = this.first =
      shownPanel?.dataset.panePanel ??
      this.tabs.find((t) => t.getAttribute("aria-selected") === "true")?.dataset.pane ??
      "";
    this.#rove(this.#tabOf(this.current) ?? this.tabs[0]);
    for (const tab of this.tabs) {
      tab.addEventListener("click", () => this.show(tab.dataset.pane!, true));
      tab.addEventListener("keydown", (e) => this.#key(e, tab));
    }
    const toggle = o.window.querySelector<HTMLButtonElement>('[data-does="sidebar"]');
    toggle?.addEventListener("click", () => {
      const hidden = o.window.dataset.state !== "collapsed";
      if (hidden) o.window.dataset.state = "collapsed";
      else delete o.window.dataset.state;
      toggle.setAttribute("aria-label", hidden ? "Show Sidebar" : "Hide Sidebar");
      const sidebar = document.getElementById(toggle.getAttribute("aria-controls") ?? "");
      if (sidebar) sidebar.inert = hidden;
    });
  }

  #tabOf(pane: string) {
    const tabbed = this.#under.get(pane) ?? pane;
    return this.tabs.find((t) => t.dataset.pane === tabbed);
  }

  // the page the one shown is under, which the title bar's back button and Alt+Left go back to
  back() {
    const above = this.#under.get(this.current);
    if (above) this.show(above);
  }

  #rove(to: HTMLElement | undefined) {
    for (const t of this.tabs) t.tabIndex = t === to ? 0 : -1;
  }

  #key(e: KeyboardEvent, tab: HTMLElement) {
    const i = this.tabs.indexOf(tab);
    const mac = this.o.system === "macos";
    let to: HTMLElement | undefined;
    switch (e.key) {
      case "ArrowDown":
        to = mac
          ? this.tabs[Math.min(i + 1, this.tabs.length - 1)]
          : this.tabs[(i + 1) % this.tabs.length];
        break;
      case "ArrowUp":
        to = mac ? this.tabs[Math.max(i - 1, 0)] : this.tabs.at(i - 1);
        break;
      case "Enter":
      case " ":
        if (mac) return;
        e.preventDefault();
        return this.show(tab.dataset.pane!, true);
      default:
        return;
    }
    e.preventDefault();
    if (!to) return;
    this.#rove(to);
    to.focus({ preventScroll: true });
    if (mac) this.show(to.dataset.pane!, true);
  }

  // Shows `pane`, keeping the window the size it had; the pane it first showed takes it back.
  show(pane: string, focus = false) {
    const tab = this.#tabOf(pane);
    if (!tab) return;
    if (pane !== this.current) {
      const w = this.o.window;
      if (pane === this.first) w.style.removeProperty("--locked");
      else if (!w.style.getPropertyValue("--locked")) {
        const unit = w.getBoundingClientRect().width / WIDTH[this.o.system];
        w.style.setProperty(
          "--locked",
          `calc(var(--spacing) * ${w.getBoundingClientRect().height / unit})`,
        );
      }
      this.current = pane;
      for (const t of this.tabs) t.setAttribute("aria-selected", String(t === tab));
      for (const el of w.querySelectorAll<HTMLElement>("[data-pane-panel], [data-head]"))
        el.hidden = (el.dataset.panePanel ?? el.dataset.head) !== pane;
      const back = w.querySelector<HTMLElement>('[data-does="back"]');
      if (back) back.hidden = !this.#under.has(pane);
      this.o.shown(pane);
    }
    this.#rove(tab);
    if (focus) tab.focus({ preventScroll: true });
  }

  restart() {
    this.show(this.first);
    const toggle = this.o.window.querySelector<HTMLButtonElement>('[data-does="sidebar"]');
    if (this.o.window.dataset.state === "collapsed") toggle?.click();
  }
}
