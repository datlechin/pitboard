// <pb-linux-preferences>: the Linux app's Preferences (preferences/, plan 3.3 "Automatic switch,
// and Warm-up"), an AdwPreferencesDialog over the dimmed window, its view switcher a tablist. It
// says what the desktop's host says: where no status item keeps Pitboard running, as on stock
// GNOME, that closing the window quits it, with Watch in the Background; under KDE Plasma's tray
// neither (surfaces.status_item, present/machine.rs).
import { combobox, type Combobox } from "./combobox.ts";
import { closeDialog, openDialog } from "./dialog.ts";
import { attr, keyed, shown, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { gtkSpin, type Spin } from "./spin.ts";
import {
  HIGHEST,
  LOWEST,
  installWatch,
  model,
  nextWarmUp,
  quit,
  setChoice,
  setGeneral,
  setSchedule,
  setShare,
  setWarmRange,
  surfaces,
  turnWarmUp,
  uninstallWatch,
  warmNote,
  type AtLimit,
  type State,
} from "../../lib/model.ts";
import { systemOf } from "../../lib/platform.ts";
import { autoStandingSaid } from "../../lib/words.ts";
import { atLimitNote, prefs } from "../../data/linux-window.ts";

const DAY = 24 * 60;
const two = (n: number) => String(n).padStart(2, "0");
// a time of the day on the 24-hour clock the page's locale gives GNOME (local_time.rs)
const clock = (minutes: number) => `${two(Math.floor(minutes / 60) % 24)}:${two(minutes % 60)}`;
// a popover's arrow, between it and its button
const ARROW = 8;
type Page = "general" | "limits" | "advanced";

class LinuxPreferences extends LiveFigure {
  #tabs: HTMLButtonElement[] = [];
  #page: Page = "limits";
  #first: Page = "limits";
  #scrolled = 0;
  #choice!: Combobox;
  #share!: Spin;
  #from!: Spin;
  #to!: Spin;

  #part<T extends Element = HTMLElement>(name: string) {
    return this.querySelector<T>(`[data-part="${name}"]`)!;
  }

  #byId<T extends HTMLElement = HTMLElement>(name: string) {
    return document.getElementById(`${this.#prefix}-${name}`) as T;
  }

  get #prefix() {
    return this.id.replace(/-preferences$/, "");
  }

  // the window it floats over, which dims behind it, and that window's desk
  get #window() {
    return this.closest<HTMLElement & { desk: HTMLElement }>("pb-linux-window")!;
  }

  get #statusItem() {
    return surfaces(systemOf(this)).statusItem;
  }

  connectedCallback() {
    this.#tabs = [...this.querySelectorAll<HTMLButtonElement>('[role="tab"][data-page]')];
    this.#page = this.#first =
      (this.#tabs.find((t) => t.getAttribute("aria-selected") === "true")?.dataset.page as Page) ??
      "limits";
    for (const tab of this.#tabs) {
      tab.addEventListener("click", () => this.#show(tab.dataset.page as Page, true));
      tab.addEventListener("keydown", (e) => this.#tabKey(e, tab));
    }
    const box = this.#part<HTMLButtonElement>("choice");
    const list = this.#byId("at-limit-choices");
    // the list opens under the row's button, wherever the page is scrolled
    for (const type of ["pointerdown", "keydown"])
      box.addEventListener(type, () => this.#placeList(box, list), true);
    this.#choice = combobox(box, list, "dropdown", (v) => setChoice(v as AtLimit));
    const spin = (row: string) => {
      const el = this.#part(row);
      return [
        el.querySelector<HTMLInputElement>('[role="spinbutton"]')!,
        el.querySelector<HTMLElement>("[data-spin=down]")!,
        el.querySelector<HTMLElement>("[data-spin=up]")!,
      ] as const;
    };
    // preferences.ui: the share a percent a step and five a page, typed with its sign or without;
    // the hours a quarter of an hour a step and an hour a page, wrapping, not typed
    this.#share = gtkSpin(...spin("share"), {
      min: LOWEST,
      max: HIGHEST,
      step: 1,
      page: 5,
      wrap: false,
      said: (n) => `${n}%`,
      read: (typed) => {
        const n = Number(typed.trim().replace(/%$/, "").trim());
        return typed.trim() === "" || Number.isNaN(n) ? null : n;
      },
      take: setShare,
    });
    const hour = { min: 0, max: DAY - 1, step: 15, page: 60, wrap: true, said: clock };
    this.#from = gtkSpin(...spin("from"), {
      ...hour,
      take: (m) => setWarmRange(m, model.get().warm.to),
    });
    this.#to = gtkSpin(...spin("to"), {
      ...hour,
      take: (m) => setWarmRange(model.get().warm.from, m),
    });
    const scroller = this.#part("scroller");
    scroller.addEventListener("scroll", () =>
      shown(this.#part("undershoot"), scroller.scrollTop > 0),
    );
    this.addEventListener("click", (e) => this.#clicked(e));
    this.addEventListener("keydown", (e) => {
      if (
        e.key !== "Escape" ||
        e.defaultPrevented ||
        (e.target as Element).closest("[role=listbox], [aria-expanded=true]")
      )
        return;
      e.preventDefault();
      this.#close();
    });
    super.connectedCallback();
    // drawn at Warm-Up, the page as far down as it goes
    this.#scrolled = this.#part("undershoot").hidden ? 0 : scroller.scrollHeight;
    scroller.scrollTop = this.#scrolled;
  }

  #placeList(box: HTMLElement, list: HTMLElement) {
    const root = this.getBoundingClientRect();
    const u = root.width / 540;
    list.style.setProperty("--y", `${box.getBoundingClientRect().bottom + ARROW * u - root.top}px`);
  }

  // AdwViewSwitcher's buttons as a tablist: the arrows and Home and End move and choose
  #tabKey(e: KeyboardEvent, tab: HTMLButtonElement) {
    const i = this.#tabs.indexOf(tab);
    const to = {
      ArrowRight: this.#tabs[(i + 1) % this.#tabs.length],
      ArrowLeft: this.#tabs.at(i - 1),
      Home: this.#tabs[0],
      End: this.#tabs.at(-1),
    }[e.key];
    if (!to) return;
    e.preventDefault();
    this.#show(to.dataset.page as Page, true);
  }

  #show(page: Page, focus = false) {
    if (page !== this.#page) {
      this.#page = page;
      this.#choice.close();
      for (const t of this.#tabs) {
        const on = t.dataset.page === page;
        attr(t, "aria-selected", String(on));
        attr(t, "tabindex", on ? null : "-1");
      }
      for (const panel of this.querySelectorAll<HTMLElement>('[role="tabpanel"]'))
        panel.hidden = panel.id !== `${this.#prefix}-${page}`;
      this.#part("scroller").scrollTop = 0;
      shown(this.#part("undershoot"), false);
    }
    if (focus) this.#tabs.find((t) => t.dataset.page === page)?.focus({ preventScroll: true });
  }

  // Preferences asked for from the window's main menu, Ctrl+Comma, the tray or Warm-Up
  // Preferences: open again where it was closed, on General, or on Limits with From focused
  present(page: string) {
    this.#open(true);
    if (page === "warm") {
      this.#show("limits");
      const from = this.#part("from");
      from.scrollIntoView({ block: "nearest" });
      from.querySelector<HTMLElement>('[role="spinbutton"]')?.focus({ preventScroll: true });
      return;
    }
    this.#show(page === "general" || page === "advanced" ? page : "limits", true);
  }

  #open(on: boolean) {
    this.hidden = !on;
    shown(this.#window.querySelector('[data-part="dimming"]'), on);
    if (on) this.render(model.get());
  }

  // Close and Escape put Preferences away, and focus goes back to the window
  #close() {
    this.#choice.close();
    this.#open(false);
    this.#window
      .querySelector<HTMLElement>('[data-page="accounts"] [data-does="main-menu"]')
      ?.focus({ preventScroll: true });
  }

  #clicked(e: MouseEvent) {
    const at = e.target as Element;
    const control = at.closest<HTMLElement>("[data-part], [data-does], [data-account]");
    if (!control) return;
    const s = model.get();
    const toggle = at.closest('[role="switch"]');
    if (control.dataset.does === "cancel") return this.#close();
    if (control.dataset.does === "uninstall") return this.#uninstall();
    if (!toggle) return;
    const row = toggle.closest<HTMLElement>("[data-account]");
    const a = s.accounts.find((x) => x.id === row?.dataset.account);
    if (a) return turnWarmUp(a.label, toggle.getAttribute("aria-checked") !== "true");
    switch (toggle.getAttribute("data-part")) {
      case "start-at-login":
        return setGeneral({ opensAtLogin: !s.general.opensAtLogin });
      case "renew":
        return void setSchedule(!s.schedule, "app");
      case "watch":
        if (s.watchService) uninstallWatch("app");
        else installWatch(s.preferences.share, "app");
    }
  }

  // Uninstall Pitboard… asks first, Cancel the default; once Pitboard's data is gone Remove the
  // App names the package manager's command, and any answer to it quits (preferences/advanced.rs
  // `take_back`), which the page does until Reset
  #uninstall() {
    const question = this.#byId<HTMLDialogElement>("uninstall-question");
    const removal = this.#byId<HTMLDialogElement>("remove-the-app");
    const window = this.#window;
    const over = { system: systemOf(this), window, desk: window.desk };
    openDialog(question, {
      ...over,
      answer: (value) => {
        if (value !== "uninstall") return;
        closeDialog(question);
        openDialog(removal, {
          ...over,
          answer: (value) => {
            if (value === "copy-command")
              return void navigator.clipboard
                ?.writeText(prefs.removeTheApp.command)
                .catch(() => {});
            if (value !== "quit") return;
            closeDialog(removal);
            quit();
          },
          cancel: () => {
            quit();
            return true;
          },
        });
      },
      cancel: () => true,
    });
  }

  protected restart() {
    this.#choice.close();
    this.#open(true);
    this.#show(this.#first);
    this.#part("scroller").scrollTop = this.#scrolled;
    shown(this.#part("undershoot"), this.#scrolled > 0);
  }

  protected render(s: State) {
    const p = s.preferences;
    const statusItem = this.#statusItem;
    // with no status item, once the window is closed: the watch service, or a watch in a terminal
    const watched = s.watching !== null || !!s.watchService;
    const set = (name: string, on: boolean, enabled = true) => {
      const toggle = this.querySelector(`[role="switch"][data-part="${name}"]`);
      attr(toggle, "aria-checked", String(on));
      attr(toggle, "disabled", enabled ? null : "");
    };
    set("start-at-login", s.general.opensAtLogin);
    set("renew", s.schedule);
    set("watch", !!s.watchService, s.watching === null);
    text(
      this.#part("watch-row").querySelector('[data-part="subtitle"]'),
      prefs.advanced.watchAbout(p.atLimit === "switch", s.watchService?.at ?? p.share),
    );
    // Limits: the choice and its standing, the share where something happens at it, and the note
    this.#choice.set(p.atLimit);
    const standing = p.atLimit === "switch" ? s.autoStanding : null;
    const said = this.#part("at-limit-row").querySelector<HTMLElement>('[data-part="subtitle"]');
    shown(said, !!standing);
    text(said, standing ? autoStandingSaid(standing) : "");
    this.#share.set(p.share, true);
    shown(this.#part("share"), p.atLimit !== "nothing");
    const note = atLimitNote(p, true);
    const whole = atLimitNote(p, statusItem, watched);
    const description = this.#part("note").closest<HTMLElement>('[data-part="description"]');
    shown(description, !!whole);
    shown(this.#part("note"), !!note);
    text(this.#part("note"), note ?? "");
    const closed = whole && note ? whole.slice(note.length).trim() : (whole ?? "");
    const once = this.#part("once-closed");
    shown(once, !!closed);
    text(once, closed);
    // Warm-Up: its hours, each Claude Code account's switch and standing, and the next warm-up
    this.#from.set(s.warm.from % DAY, true);
    this.#to.set(s.warm.to % DAY, true);
    const warm = this.#byId("warm-description");
    const [gnome, kde] = warm?.querySelectorAll<HTMLElement>("[data-on]") ?? [];
    text(gnome, prefs.limits.warm.note(false, watched));
    text(kde, prefs.limits.warm.note(true));
    // a row a Claude Code account, after From and To
    const claude = s.accounts.filter((a) => a.tool === "claude");
    const rows = keyed(
      this.#part("from").parentElement!,
      claude.map((a) => a.id),
      2,
      this.querySelector<HTMLTemplateElement>('template[data-part="warm-row"]'),
      (row, id) => {
        const title = `${this.#prefix}-warm-${id}`;
        row.querySelector('[data-part="title"]')!.id = title;
        row.querySelector('[role="switch"]')!.setAttribute("aria-labelledby", title);
      },
    );
    claude.forEach((a, i) => {
      const row = rows[i];
      text(row.querySelector('[data-part="title"]'), a.label);
      attr(row.querySelector('[role="switch"]'), "aria-checked", String(s.warm.on.includes(a.id)));
      const line = warmNote(s, a);
      const subtitle = row.querySelector<HTMLElement>('[data-part="subtitle"]');
      shown(subtitle, !!line);
      text(subtitle, line ?? "");
    });
    const next = nextWarmUp(s);
    shown(this.#part("warm-standing"), !!next);
    text(this.#part("warm-standing"), next ?? "");
  }
}

customElements.define("pb-linux-preferences", LinuxPreferences);
