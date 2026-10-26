// <pb-settings> and <pb-warm>: macOS's Settings window, cut at When a limit reaches or at Warm-Up,
// its tabs as SettingsView.swift's TabView shows them (answer 4 a).
import { combobox, type Combobox } from "./combobox.ts";
import { attr, cloned, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { AtLimitControls, WarmControls } from "./limits.ts";
import { model, setGeneral, setSchedule, type State } from "../../lib/model.ts";
import type { MenuBarShows } from "../../data/settings.ts";

const TABS = { general: "General", limits: "Limits", advanced: "Advanced" } as const;
type Tab = keyof typeof TABS;

class SettingsWindow extends LiveFigure {
  #atLimit: AtLimitControls | null = null;
  #warm: WarmControls | null = null;
  #tabs: HTMLElement[] = [];
  #tab: Tab = "limits";
  #shows: Combobox | null = null;

  connectedCallback() {
    if (this.querySelector(`#${this.id}-at`)) this.#atLimit = new AtLimitControls(this, this.id);
    if (this.querySelector(`#${this.id}-from`)) this.#warm = new WarmControls(this, this.id);
    this.#tabs = [...this.querySelectorAll<HTMLElement>('[role="tab"][data-tab]')];
    this.#rove();
    for (const tab of this.#tabs) {
      tab.addEventListener("click", () => this.#show(tab.dataset.tab as Tab, true));
      tab.addEventListener("keydown", (e) => {
        const i = this.#tabs.indexOf(tab);
        const to =
          e.key === "ArrowRight"
            ? this.#tabs[Math.min(i + 1, this.#tabs.length - 1)]
            : e.key === "ArrowLeft"
              ? this.#tabs[Math.max(i - 1, 0)]
              : e.key === "Home"
                ? this.#tabs[0]
                : e.key === "End"
                  ? this.#tabs.at(-1)
                  : null;
        if (!to) return;
        e.preventDefault();
        this.#show(to.dataset.tab as Tab, true);
      });
    }
    // General's and Advanced's controls, wherever they are cloned
    this.addEventListener("click", (e) => {
      const control = (e.target as Element).closest<HTMLElement>("[data-setting]");
      if (!control || control.getAttribute("aria-disabled") === "true") return;
      const s = model.get();
      const g = s.general;
      switch (control.dataset.setting) {
        case "login":
          return setGeneral({ opensAtLogin: !g.opensAtLogin });
        case "checks":
          return setGeneral({ checksForUpdates: !g.checksForUpdates });
        case "installs":
          return setGeneral({ installsUpdates: !g.installsUpdates });
        case "renew":
          return void setSchedule(!s.schedule, "app");
      }
    });
    super.connectedCallback();
  }

  // Settings has no Refresh, and its window neither minimizes nor zooms
  protected windowMenus() {
    return { account: null, refresh: null, resizes: false };
  }

  protected restart() {
    this.#atLimit?.restart();
    this.#warm?.restart();
    this.#shows?.close();
    if (this.#tabs.length) this.#show("limits");
    this.style.removeProperty("--locked");
  }

  #rove() {
    for (const t of this.#tabs) t.tabIndex = t.dataset.tab === this.#tab ? 0 : -1;
  }

  #show(tab: Tab, focus = false) {
    if (tab !== this.#tab) {
      // the desk keeps the height it has, the window growing or shrinking under its cut
      if (!this.style.getPropertyValue("--locked")) {
        const units = Number(getComputedStyle(this).getPropertyValue("--units") || 600);
        const box = this.getBoundingClientRect();
        this.style.setProperty(
          "--locked",
          `calc(var(--spacing) * ${box.height / (box.width / units)})`,
        );
      }
      this.#tab = tab;
      for (const t of this.#tabs) t.setAttribute("aria-selected", String(t.dataset.tab === tab));
      for (const panel of this.querySelectorAll<HTMLElement>("[data-tab-panel]"))
        panel.hidden = panel.dataset.tabPanel !== tab;
      text(this.querySelector('[data-part="title"]'), TABS[tab]);
      const panel = this.querySelector<HTMLElement>(`[data-tab-panel="${tab}"]`)!;
      if (tab !== "limits" && !panel.firstElementChild) this.#clone(panel, tab);
      this.render(model.get());
    }
    this.#rove();
    if (focus) this.#tabs.find((t) => t.dataset.tab === tab)?.focus({ preventScroll: true });
  }

  // a tab's template, its `tpl-` ids made this window's own
  #clone(panel: HTMLElement, tab: Tab) {
    const el = cloned(
      document.querySelector<HTMLTemplateElement>(
        `[data-drawing="app macos"] template[data-part="${tab}"]`,
      ),
    );
    for (const node of [el, ...el.querySelectorAll("*")])
      for (const name of ["id", "aria-labelledby", "aria-controls"]) {
        const v = node.getAttribute(name);
        if (v?.includes("tpl-")) node.setAttribute(name, v.replace(/\btpl-/g, `${this.id}-`));
      }
    panel.append(el);
    const box = el.querySelector<HTMLElement>(`#${this.id}-shows`);
    const list = el.querySelector<HTMLElement>(`#${this.id}-shows-list`);
    if (box && list)
      this.#shows = combobox(box, list, "popup", (v) =>
        setGeneral({ menuBarShows: v as MenuBarShows }),
      );
  }

  protected render(s: State) {
    this.#atLimit?.render(s);
    this.#warm?.render(s);
    const g = s.general;
    const set = (name: string, on: boolean, enabled = true) => {
      const control = this.querySelector(`[data-setting="${name}"]`);
      attr(control, "aria-checked", String(on));
      attr(control, "aria-disabled", enabled ? null : "true");
      // a switch's label dims with it
      attr(
        document.getElementById(control?.getAttribute("aria-labelledby") ?? ""),
        "aria-disabled",
        enabled ? null : "true",
      );
    };
    set("login", g.opensAtLogin);
    set("checks", g.checksForUpdates);
    set("installs", g.installsUpdates, g.checksForUpdates);
    set("renew", s.schedule);
    this.#shows?.set(g.menuBarShows);
  }
}

customElements.define("pb-settings", SettingsWindow);
customElements.define("pb-warm", class extends SettingsWindow {});
