// <pb-win-window>: the Pitboard window on Windows (plan 3.3 to 3.5): its NavigationView's pages,
// Accounts with each account a SettingsExpander, which holds no selection, its keys acting on the
// account whose toggle has focus, Settings, Diagnostics and Activity.
import { RowsFigure, accountOf, part } from "./accounts.ts";
import { readLog } from "./activity.ts";
import { closeDialog, openDialog } from "./dialog.ts";
import { disclosure } from "./disclosure.ts";
import { attr, cloned, keyed, shown, text } from "./draw.ts";
import { AtLimitControls, WarmControls } from "./limits.ts";
import { MachinePane } from "./machine.ts";
import { offersOf } from "./offers.ts";
import { Panes } from "./panes.ts";
import { model, quit, setGeneral, setSchedule, warmNote, type State } from "../../lib/model.ts";
import {
  READING,
  accountSpoken,
  isInUse,
  listed,
  signInExpired,
  updated,
  windowsRowNote,
} from "../../lib/words.ts";
import { SWITCH, keysOf, openSite, siteOf } from "../../data/window.ts";
import { detail, limitCard, rowAction, signInSaid } from "../../data/windows.ts";
import { winAdvanced } from "../../data/settings.ts";
import type { Account, Tool } from "../../data/world.ts";

interface Settings {
  atLimit: AtLimitControls | null;
  warm: WarmControls | null;
}

class WinWindow extends RowsFigure {
  protected readonly rowSelector = "[data-row]";
  #panes: Panes | null = null;
  // a window first shown on another pane draws its list's rows once the list shows
  #listed = false;
  #machine: MachinePane | null = null;
  #seen?: IntersectionObserver;
  #settings: Settings | null = null;

  protected list() {
    return this.querySelector<HTMLElement>('[data-part="list"]')!;
  }

  protected connect() {
    if (this.window.querySelector('[role="tablist"]'))
      this.#panes = new Panes({
        system: "windows",
        window: this.window,
        shown: (pane) => this.#paneShown(pane),
      });
    this.#listed = this.#pane === "accounts";
    // a Settings page the figure draws works from the start
    if (this.#panel("settings")?.firstElementChild) this.#settingsPage();
    const nav = this.querySelector<HTMLElement>('[role="tablist"]');
    nav?.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && this.window.dataset.state === "open") this.#navigation(false);
    });
    document.addEventListener(
      "pointerdown",
      (e) => {
        const at = e.target as Node;
        const button = this.querySelector('[data-does="navigation"]');
        if (this.window.dataset.state === "open" && !nav?.contains(at) && !button?.contains(at))
          this.#navigation(false);
      },
      { capture: true, signal: this.off.signal },
    );
    const machine = this.#panel("machine");
    if (machine) this.#machine = new MachinePane(machine);
    // the Activity pane reads the log as it comes into view
    this.#seen = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting) && this.#pane === "activity")
        readLog(this.#panel("activity")!, this.system, model.get());
    });
    this.#seen.observe(this);
    // a press on an expander's header opens or closes it, but on a button in it
    this.addEventListener("click", (e) => {
      const at = e.target as Element;
      const header = at.closest('[data-part="header"]');
      if (header && !at.closest("button")) this.#toggle(header.closest<HTMLElement>("[data-row]")!);
    });
    super.connect();
    // an expander's toggle: Enter switches to its account, as its button does, and F2 and Delete
    // rename and remove it (AccountKeys.cs)
    this.list().addEventListener("keydown", (e) => {
      const row = (e.target as Element)
        .closest('[data-does="expand"]')
        ?.closest<HTMLElement>(this.rowSelector);
      const a = accountOf(model.get(), row?.dataset.account);
      if (!a || e.ctrlKey || e.altKey || e.shiftKey) return;
      const does = {
        Enter: () => this.act(a),
        F2: () => this.ask("rename", a),
        Delete: () => this.ask("forget", a),
      }[e.key];
      if (!does) return;
      e.preventDefault();
      does();
    });
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.#seen?.disconnect();
  }

  protected copied() {
    return (
      (document.activeElement as Element | null)?.closest<HTMLElement>(this.rowSelector) ?? null
    );
  }

  protected does(does: string, el: HTMLElement, row: HTMLElement | null): boolean {
    switch (does) {
      case "expand":
        if (row) this.#toggle(row);
        return true;
      case "read":
        readLog(this.#panel("activity")!, this.system, model.get());
        return true;
      case "warm-settings":
        this.#panes?.show("settings");
        return true;
      case "navigation":
        this.#navigation(this.window.dataset.state !== "open");
        return true;
      case "back":
        this.#panes?.back();
        return true;
      case "show":
        this.#panes?.show(el.dataset.page!);
        return true;
    }
    return false;
  }

  protected restart() {
    super.restart();
    for (const row of this.querySelectorAll<HTMLElement>("[data-row]")) this.#expand(row, false);
    this.#settings?.atLimit?.restart();
    this.#settings?.warm?.restart();
    this.#navigation(false);
    this.#panes?.restart();
  }

  get #pane() {
    return this.#panes?.current ?? "accounts";
  }

  #panel(pane: string) {
    return this.window.querySelector<HTMLElement>(`[data-pane-panel="${pane}"]`);
  }

  #paneShown(pane: string) {
    this.menu.closeAll(false);
    this.#navigation(false);
    const s = model.get();
    if (pane === "accounts" && !this.#listed) {
      this.#listed = true;
      this.render(s);
    }
    if (pane === "activity") readLog(this.#panel("activity")!, this.system, s);
    if (pane === "machine") this.#machine?.show();
    if (pane === "settings" && !this.#settings) this.#settingsPage();
    this.render(s);
  }

  // the Settings page: the figure's own, or SettingsPage.astro's from its top, cloned the first time
  // it shows, its ids made this window's own and its flyouts put on the desk
  #settingsPage() {
    const panel = this.#panel("settings");
    if (!panel) return;
    if (!panel.firstElementChild) {
      const template = document.querySelector<HTMLTemplateElement>(
        '[data-drawing="app windows"] template[data-part="settings"]',
      );
      const parts = [...template!.content.children].map((el) => el.cloneNode(true) as HTMLElement);
      for (const part of parts)
        for (const node of [part, ...part.querySelectorAll("*")])
          for (const name of ["id", "aria-labelledby", "aria-controls", "aria-describedby"]) {
            const v = node.getAttribute(name);
            if (v?.includes("tpl-")) node.setAttribute(name, v.replace(/\btpl-/g, `${this.id}-`));
          }
      for (const part of parts)
        if (part.hasAttribute("data-desk")) this.append(...part.children);
        else panel.append(part);
      const about = panel.querySelector<HTMLButtonElement>(`#${this.id}-about-expand`);
      if (about) disclosure(about, about.closest<HTMLElement>("[data-header]")!);
    }
    const section = (name: string) => panel.querySelector<HTMLElement>(`[data-section="${name}"]`);
    const at = section("at-limit");
    const warm = section("warm");
    this.#settings = {
      atLimit: at ? new AtLimitControls(at, this.id) : null,
      warm: warm ? new WarmControls(warm, this.id) : null,
    };
    panel.addEventListener("click", (e) => {
      const control = (e.target as Element).closest<HTMLElement>("[data-setting]");
      if (!control || control.getAttribute("aria-disabled") === "true") return;
      const now = model.get();
      if (control.dataset.setting === "login")
        setGeneral({ opensAtLogin: !now.general.opensAtLogin });
      if (control.dataset.setting === "updates")
        setGeneral({ checksForUpdates: !now.general.checksForUpdates });
      if (control.dataset.setting === "renew") setSchedule(!now.schedule, "app");
      if (control.dataset.setting === "uninstall") this.#uninstall();
    });
  }

  // Uninstall Pitboard asks first, its answer no default, as SettingsPage.xaml.cs does
  // departs from the apps: Uninstall Pitboard on Windows quits Pitboard until Reset
  #uninstall() {
    const d = document.querySelector<HTMLDialogElement>("#win-accounts-uninstall");
    if (!d) return;
    text(part(d, "title"), winAdvanced.uninstall.question);
    text(part(d, "message"), winAdvanced.uninstall.says);
    openDialog(d, {
      system: this.system,
      window: this.window,
      desk: this,
      answer: (value) => {
        if (value !== "uninstall") return;
        closeDialog(d);
        quit();
      },
      cancel: () => true,
      focus: d.querySelector<HTMLElement>('[data-does="cancel"]'),
    });
  }

  // TitleBar's pane button: the navigation over the page with its labels, until a pane is chosen,
  // Escape, or a press elsewhere
  #navigation(open: boolean) {
    const button = this.querySelector('[data-does="navigation"]');
    if (open) this.window.dataset.state = "open";
    else if (this.window.dataset.state === "open") delete this.window.dataset.state;
    attr(button, "aria-expanded", String(open));
    attr(button, "aria-label", open ? "Close Navigation" : "Open Navigation");
  }

  // the window's Add Account sheet or its Settings page, asked for from elsewhere: the tray's
  // Add account…, which brings the window to its Accounts pane, as a sheet is about accounts
  present(sheet: string) {
    if (sheet === "settings") return this.#panes?.show("settings");
    if (sheet !== "add") return;
    this.#panes?.show("accounts");
    super.present(sheet);
  }

  // a row opened on its cards, or closed
  #expand(row: HTMLElement, open: boolean) {
    attr(row.querySelector('[data-does="expand"]'), "aria-expanded", String(open));
    shown(part(row, "details"), open);
  }

  #toggle(row: HTMLElement) {
    this.#expand(row, row.querySelector('[data-does="expand"]')?.ariaExpanded !== "true");
  }

  // after Remove Account the row has gone, and Windows gives focus to the window's title bar
  forgotten() {
    const bar = this.querySelector<HTMLElement>('[data-part="title-bar"]');
    if (!bar) return;
    bar.tabIndex = -1;
    bar.focus({ preventScroll: true });
  }

  protected render(s: State) {
    super.render(s);
    text(part(this, "updated"), s.reading !== null ? READING : updated(s.readAt));
    attr(this.querySelector('[data-does="refresh"]'), "disabled", s.reading !== null ? "" : null);
    this.#settings?.atLimit?.render(s);
    this.#settings?.warm?.render(s);
    const settings = this.#panel("settings");
    if (settings) {
      for (const [name, on] of [
        ["login", s.general.opensAtLogin],
        ["updates", s.general.checksForUpdates],
        ["renew", s.schedule],
      ] as const) {
        const toggle = settings.querySelector(`[data-setting="${name}"]`);
        attr(toggle, "aria-checked", String(on));
        text(toggle?.parentElement?.querySelector("[data-said]"), on ? "On" : "Off");
      }
    }
    if (!this.#listed) return;
    this.notices(s);
    // the tip waits while a notice shows, whose message it would cover
    this.setup(s, !!this.querySelector("[data-notice]:not([hidden])"));
    const above = this.querySelectorAll(
      '[data-notice]:not([hidden]), [data-part="setup"]:not([hidden])',
    );
    attr(part(this, "first"), "data-state", above.length ? null : "top");
    for (const group of this.querySelectorAll<HTMLElement>('[data-part="list"] > [data-tool]'))
      this.#rows(s, group, group.dataset.tool as Tool);
  }

  // a tool's accounts, the one in use first, in their own stack, where a row a visitor adds takes
  // ids of its own
  #rows(s: State, group: HTMLElement, tool: Tool) {
    const want = listed(s, tool);
    const rows = keyed(
      part(group, "rows")!,
      want.map((a) => a.id),
      0,
      this.template("row"),
      (row, id) => {
        const details = part(row, "details");
        if (!details) return;
        details.id = `${this.id}-${id}-details`;
        row.querySelector('[data-does="expand"]')?.setAttribute("aria-controls", details.id);
      },
    );
    want.forEach((a, i) => this.#expander(s, rows[i], a));
  }

  // a row: its header's toggle named as Narrator hears the account, its one note, its end, and its
  // cards
  #expander(s: State, row: HTMLElement, a: Account) {
    const on = isInUse(a, s);
    const switching = s.switching?.id === a.id;
    const toggle = row.querySelector('[data-does="expand"]');
    attr(toggle, "aria-label", accountSpoken(a, s));
    attr(toggle, "aria-keyshortcuts", keysOf(a, s, this.system));
    shown(part(row, "in-use-symbol"), on);
    shown(part(row, "parked-symbol"), !on);
    text(part(row, "label"), a.label);
    shown(part(row, "plan"), !!a.plan);
    text(part(row, "plan"), a.plan ?? "");
    this.limits(s, row, a);
    const note = windowsRowNote(a, s);
    shown(part(row, "note"), !!note);
    text(part(row, "note"), note ?? "");
    const action = rowAction(a, s);
    shown(part(row, "action"), !!action && !switching);
    shown(part(row, "in-use"), on && !switching);
    shown(part(row, "switching"), switching);
    const button = row.querySelector('[data-does="use"]');
    text(button, action?.title ?? SWITCH);
    attr(button, "aria-label", action?.spoken ?? null);
    attr(button, "disabled", s.switching && !switching ? "" : null);
    shown(part(row, "email-card"), !!a.email);
    text(part(row, "email"), a.email);
    const signIn = signInSaid(a, s);
    shown(part(row, "sign-in-card"), !!signIn);
    text(part(row, "sign-in"), signIn ?? "");
    shown(part(row, "warm-card"), a.tool === "claude");
    text(part(row, "warm"), warmNote(s, a) ?? detail.off);
    this.#cards(s, row, a);
    const site = siteOf(a.tool);
    shown(part(row, "open-command"), !!site);
    text(row.querySelector('[data-does="open"]'), site ? openSite(site) : "");
    // what the shortcut menu offers past the row's own button
    const o = offersOf(a, s);
    shown(part(row, "sign-in-again-command"), o.signInAgain && !signInExpired(a, s));
    shown(part(row, "warm-command"), !!o.warm?.enabled);
    attr(row.querySelector('[data-does="forget"]'), "disabled", on ? "" : null);
  }

  // an opened row's card a limit, its countdown to the reset moving with the clock
  #cards(s: State, row: HTMLElement, a: Account) {
    const box = part(row, "limit-cards")!;
    if (box.children.length !== a.limits.length)
      box.replaceChildren(...a.limits.map(() => cloned(this.template("limit-card"))));
    a.limits.forEach((limit, i) => {
      const card = box.children[i] as HTMLElement;
      const c = limitCard(limit, s);
      text(part(card, "name"), c.name);
      text(part(card, "used"), c.used);
      text(part(card, "resets"), c.resets);
      text(part(card, "resets-in"), c.resetsIn);
    });
  }
}

customElements.define("pb-win-window", WinWindow);
