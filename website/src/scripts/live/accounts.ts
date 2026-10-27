// The Accounts window on both systems (plan 3.3 to 3.5, app redo 4.3 on macOS); every view of it,
// the cut ones too, follows the one state. macOS's list is a grid with a selection; Windows'
// accounts are SettingsExpanders, which hold none, their keys acting on the account whose toggle
// has focus.
import { attr, cloned, keyed, shown, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { Grid } from "./grid.ts";
import { hold } from "./hold.ts";
import { INSET, Menu, sectioned } from "./menu.ts";
import { offersOf } from "./offers.ts";
import { readLog } from "./activity.ts";
import type { WindowMenus } from "./appmenu.ts";
import { MachinePane } from "./machine.ts";
import { Panes } from "./panes.ts";
import { disclosure } from "./disclosure.ts";
import { AtLimitControls, WarmControls } from "./limits.ts";
import { bringForward } from "./forward.ts";
import { showAccountWindow } from "./windows.ts";
import { closeDialog, openDialog } from "./dialog.ts";
import { say } from "./say.ts";
import { sheets } from "./sheets.ts";
import {
  declineSecond,
  dismissNotice,
  model,
  openWindow,
  quit,
  refreshFromApp,
  setGeneral,
  setSchedule,
  switchBackFromApp,
  switchFromApp,
  warmNote,
  warmUp,
  type State,
} from "../../lib/model.ts";
import {
  READING,
  accountSpoken,
  isInUse,
  limitRow,
  limitTitle,
  listed,
  resetsIn,
  rowNote,
  signInExpired,
  signInSaid,
  staleNote,
  windowsRowNote,
  switchSpoken,
  updated,
} from "../../lib/words.ts";
import {
  SWITCH,
  WARM_UP_OFF,
  keysOf,
  noticeOf,
  openSite,
  setupStepOf,
  signInAsking,
  signInToAgain,
  siteOf,
  used,
  type System,
} from "../../data/window.ts";
import { detail, limitCard, rowAction, signInSaid as windowsSignIn } from "../../data/windows.ts";
import { winAdvanced } from "../../data/settings.ts";
import type { Account, Tool } from "../../data/world.ts";

const part = <T extends Element = HTMLElement>(in_: ParentNode, name: string) =>
  in_.querySelector<T>(`[data-part="${name}"]`);
const accountOf = (s: State, id: string | undefined) => s.accounts.find((a) => a.id === id);

interface Settings {
  atLimit: AtLimitControls | null;
  warm: WarmControls | null;
}

export class AccountsWindow extends LiveFigure {
  system: System = "macos";
  window!: HTMLElement;
  // macOS's list; Windows' accounts hold no selection
  #grid: Grid | null = null;
  // what an account's row is
  #rowSelector = '[role="row"]';
  #menu!: Menu;
  #menuFor: string | null = null;
  // what raised the coming contextmenu: a right or Control press, or the keys that open a menu
  #raised: "pointer" | "keys" | null = null;
  // a Control-click's click, which selects nothing
  #swallow = false;
  #saying: Tool | null = null;
  // the window's panes, which the macOS window has none of
  #panes: Panes | null = null;
  // a window first shown on another pane draws its list's rows once the list shows
  #listed = false;
  #machine: MachinePane | null = null;
  // the row macOS selects as the window opens: the account in use of the first tool shown
  #first: string | null = null;
  #seen?: IntersectionObserver;
  // the listeners this window puts on the document, taken off as it leaves the page
  #off = new AbortController();

  connectedCallback() {
    this.#off = new AbortController();
    const drawing = this.closest<HTMLElement>("[data-drawing]")?.dataset.drawing ?? "";
    this.system = drawing.endsWith("windows") ? "windows" : "macos";
    this.window = this.querySelector<HTMLElement>(':scope > [data-part="window"]')!;
    if (this.window.querySelector('[role="tablist"]'))
      this.#panes = new Panes({
        system: this.system,
        window: this.window,
        shown: (pane) => this.#paneShown(pane),
      });
    this.#listed = this.#pane === "accounts";
    // a Settings page the figure draws works from the start
    if (this.#panel("settings")?.firstElementChild) this.#settingsPage();
    if (this.system === "windows") {
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
        { capture: true, signal: this.#off.signal },
      );
    }
    const machine = this.#panel("machine");
    if (machine) this.#machine = new MachinePane(machine);
    // the Activity pane reads the log as it comes into view
    this.#seen = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting) && this.#pane === "activity")
        readLog(this.#panel("activity")!, this.system, model.get());
    });
    this.#seen.observe(this);
    if (this.system === "windows") this.#rowSelector = "[data-row]";
    // macOS's grid sits inside its window's list
    const grid = this.querySelector<HTMLElement>(
      this.system === "macos" ? '[role="grid"]' : '[data-part="list"]',
    )!;
    if (this.system === "macos") {
      this.#grid = new Grid({
        system: this.system,
        grid,
        rows: () =>
          [...grid.querySelectorAll<HTMLElement>(this.#rowSelector)].filter((r) => !r.hidden),
        act: (row) => this.#act(row),
        forget: (row) => this.#ask("forget", row),
        rename: (row) => this.#ask("rename", row),
        name: (row) => accountOf(model.get(), row.dataset.account)?.label ?? "",
        selected: () => this.#inspector(model.get()),
      });
      this.#first =
        grid.querySelector<HTMLElement>('[role="row"][aria-selected="true"]')?.dataset.account ??
        null;
      if (this.#first) this.#grid.select(this.#rowFor(this.#first), false);
    }
    this.#menu = new Menu(this.querySelector<HTMLElement>(`#${this.id}-menu`)!, {
      system: this.system,
      desk: this,
      choose: (item) => this.#offer(item.dataset.offer!),
      shown: (open, refocus) => {
        const row = this.#rowFor(this.#menuFor);
        // macOS outlines the row a shortcut menu acts on while it is open
        if (this.system === "macos") attr(row, "data-state", open ? "menu" : null);
        if (!open && refocus)
          (row?.querySelector<HTMLElement>('[data-does="expand"]') ?? row)?.focus({
            preventScroll: true,
          });
      },
    });
    this.#listen(grid);
    super.connectedCallback();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.#seen?.disconnect();
    this.#off.abort();
  }

  protected restart() {
    this.#menu.closeAll(false);
    sheets(this.system)?.close();
    this.#grid?.select(this.#rowFor(this.#first), false);
    this.#showInspector(true);
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
    this.#menu.closeAll(false);
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

  // Windows' Settings page: the figure's own, or SettingsPage.astro's from its top, cloned the first
  // time it shows, its ids made this window's own and its flyouts put on the desk
  #settings: Settings | null = null;
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
    if (this.system !== "windows") return;
    const button = this.querySelector('[data-does="navigation"]');
    if (open) this.window.dataset.state = "open";
    else if (this.window.dataset.state === "open") delete this.window.dataset.state;
    attr(button, "aria-expanded", String(open));
    attr(button, "aria-label", open ? "Close Navigation" : "Open Navigation");
  }

  // the window's Add Account sheet, asked for from elsewhere: the menu bar's Add Account…, which
  // brings the window to its Accounts pane, as a sheet is about accounts
  present(sheet: string) {
    if (sheet === "settings") return this.#panes?.show("settings");
    if (sheet !== "add") return;
    this.#panes?.show("accounts");
    sheets(this.system)?.signIn(this, null, null);
  }

  #rowFor(id: string | null) {
    return id
      ? this.querySelector<HTMLElement>(`${this.#rowSelector}[data-account="${id}"]`)
      : null;
  }

  // a Windows row opened on its cards, or closed
  #expand(row: HTMLElement, open: boolean) {
    attr(row.querySelector('[data-does="expand"]'), "aria-expanded", String(open));
    shown(part(row, "details"), open);
  }

  #toggle(row: HTMLElement) {
    this.#expand(row, row.querySelector('[data-does="expand"]')?.ariaExpanded !== "true");
  }

  #listen(grid: HTMLElement) {
    this.addEventListener("click", (e) => {
      const at = e.target as Element;
      // a press on an expander's header opens or closes it, but on a button in it
      const header = this.system === "windows" ? at.closest('[data-part="header"]') : null;
      if (header && !at.closest("button"))
        return this.#toggle(header.closest<HTMLElement>("[data-row]")!);
      const does = at.closest<HTMLElement>("[data-does]");
      if (!does || !this.window.contains(does)) return;
      const row = does.closest<HTMLElement>(this.#rowSelector);
      const account = accountOf(model.get(), row?.dataset.account);
      switch (does.dataset.does) {
        case "use":
          if (row) this.#act(row);
          return;
        case "expand":
          if (row) this.#toggle(row);
          return;
        case "copy-email":
          if (account?.email) void navigator.clipboard?.writeText(account.email).catch(() => {});
          return;
        case "open":
        case "sign-in-again":
        case "rename":
        case "warm":
        case "forget":
          if (account) this.#offer(does.dataset.does, account);
          return;
        case "refresh":
          return refreshFromApp(this.system);
        case "read":
          return readLog(this.#panel("activity")!, this.system, model.get());
        case "inspector":
          return this.#showInspector(this.window.dataset.state === "collapsed");
        // Windows' Settings is a page of the window, macOS's a window of its own
        case "warm-settings":
          return this.#panes ? this.#panes.show("settings") : bringForward("settings", this.system);
        case "navigation":
          return this.#navigation(this.window.dataset.state !== "open");
        case "back":
          return this.#panes?.back();
        case "show":
          return this.#panes?.show(does.dataset.page!);
        case "add":
          return sheets(this.system)?.signIn(
            this,
            does.closest('[data-part="setup"]') ? this.#setupTool : null,
            null,
          );
        case "not-now":
          if (this.#setupTool) declineSecond(this.#setupTool);
          return;
        case "dismiss":
          return dismissNotice(does.closest<HTMLElement>("[data-notice]")!.dataset.notice as Tool);
        case "switch-back":
          return switchBackFromApp(this.system);
      }
    });
    // a right click, a Control-click or a two-finger click opens a row's shortcut menu where it
    // is; on Windows Shift+F10 and the menu key open it above the row, and on macOS no key does
    this.addEventListener(
      "pointerdown",
      (e) => {
        this.#raised = e.button === 2 || (e.button === 0 && e.ctrlKey) ? "pointer" : null;
        this.#swallow = false;
      },
      true,
    );
    this.addEventListener("keydown", () => (this.#raised = null), true);
    // the press that opens one leaves focus where it was, for the menu to take
    grid.addEventListener("mousedown", (e) => {
      if (e.button === 2 || (e.button === 0 && e.ctrlKey)) e.preventDefault();
    });
    // light dismiss: a press anywhere but the menu closes it, while the window is shown
    document.addEventListener(
      "pointerdown",
      (e) => {
        if (this.#menu.open && this.offsetParent && !this.#menu.contains(e.target as Node))
          this.#menu.closeAll(false);
      },
      { capture: true, signal: this.#off.signal },
    );
    grid.addEventListener("keydown", (e) => {
      if (e.key !== "ContextMenu" && !(e.key === "F10" && e.shiftKey)) return;
      e.preventDefault();
      this.#raised = "keys";
      const row = (e.target as Element).closest<HTMLElement>(this.#rowSelector);
      if (row && this.system === "windows") this.#openMenu(row, null, null);
    });
    // an expander's toggle: Enter switches to its account, as its button does, and F2 and Delete
    // rename and remove it (AccountKeys.cs)
    if (this.system === "windows")
      grid.addEventListener("keydown", (e) => {
        const row = (e.target as Element)
          .closest('[data-does="expand"]')
          ?.closest<HTMLElement>(this.#rowSelector);
        if (!row || e.ctrlKey || e.altKey || e.shiftKey) return;
        const does = {
          Enter: () => this.#act(row),
          F2: () => this.#ask("rename", row),
          Delete: () => this.#ask("forget", row),
        }[e.key];
        if (!does) return;
        e.preventDefault();
        does();
      });
    grid.addEventListener(
      "click",
      (e) => {
        if (e.detail > 0 && this.#swallow) e.stopPropagation();
      },
      true,
    );
    grid.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      const by = this.#raised;
      this.#raised = null;
      this.#swallow = by === "pointer";
      const row = (e.target as Element).closest<HTMLElement>(this.#rowSelector);
      // the keys above answered it already, where a browser raises one after them
      if (!row || by === "keys") return;
      // a touch held still opened it already (hold.ts)
      if (this.#menu.open && this.#menuFor === row.dataset.account) return;
      // with no press before it, a screen reader asked, as VoiceOver's VO-Shift-M does
      if (by === "pointer") this.#openMenu(row, e.clientX, e.clientY);
      else this.#openMenu(row, null, null);
    });
    hold(grid, this.#rowSelector, (row, x, y) => this.#openMenu(row, x, y));
    // ⌘⌫, the Account menu's Remove Account…, wherever focus is in the window but a field
    this.addEventListener("keydown", (e) => {
      const at = e.target as Element;
      if (this.system !== "macos" || e.defaultPrevented || !e.metaKey || e.key !== "Backspace")
        return;
      const selected = this.#grid?.selected;
      if (at.closest("dialog, input") || !selected || this.#pane !== "accounts") return;
      e.preventDefault();
      this.#ask("forget", selected);
    });
    // ⌃⌘I, InspectorCommands' Show and Hide Inspector
    this.addEventListener("keydown", (e) => {
      if (this.system !== "macos" || e.defaultPrevented || !this.#inspectorPanel()) return;
      if (!e.metaKey || !e.ctrlKey || e.code !== "KeyI") return;
      e.preventDefault();
      this.#showInspector(this.window.dataset.state === "collapsed");
    });
    // the list draws its selection in the accent while it, or nothing else in the window, has focus
    this.window.addEventListener("focusin", (e) =>
      attr(grid, "data-key", grid.contains(e.target as Node) ? "" : null),
    );
    // the inspector's own commands, on the account selected
    this.#inspectorPanel()?.addEventListener("click", (e) => {
      const button = (e.target as Element).closest<HTMLElement>("[data-offer]");
      if (button)
        this.#offer(
          button.dataset.offer!,
          accountOf(model.get(), this.#grid?.selected?.dataset.account),
        );
    });
    // ⌘C and Ctrl+C copy the account's email, with nothing on screen saying so
    grid.addEventListener("copy", (e) => {
      const row =
        this.system === "macos"
          ? this.#grid?.selected
          : (document.activeElement as Element | null)?.closest<HTMLElement>(this.#rowSelector);
      const email = accountOf(model.get(), row?.dataset.account)?.email;
      if (!email) return;
      e.preventDefault();
      e.clipboardData?.setData("text/plain", email);
    });
  }

  #setupTool: Tool | null = null;

  // Switch, Return, Enter or a double click: the row's button
  #act(row: HTMLElement) {
    const s = model.get();
    const a = accountOf(s, row.dataset.account);
    if (!a || isInUse(a, s) || s.switching) return;
    if (signInExpired(a, s)) return sheets(this.system)?.signIn(this, a.tool, a);
    this.#saying = a.tool;
    switchFromApp(a.tool, a.label, this.system);
  }

  #ask(what: "forget" | "rename", row: HTMLElement) {
    const s = model.get();
    const a = accountOf(s, row.dataset.account);
    if (!a) return;
    if (what === "forget" && !isInUse(a, s)) sheets(this.system)?.forget(this, a);
    if (what === "rename") sheets(this.system)?.rename(this, a);
  }

  #openMenu(row: HTMLElement, x: number | null, y: number | null) {
    const s = model.get();
    const a = accountOf(s, row.dataset.account);
    if (!a) return;
    this.#menu.closeAll(false);
    this.#menuFor = a.id;
    this.#fillMenu(a, s);
    if (this.system === "macos") attr(row, "data-state", "menu");
    const el = this.#menu.el;
    this.#menu.show(
      x === null ? "keyboard" : "pointer",
      this.system === "windows" ? "first" : "menu",
    );
    // one opened from the keyboard sits above the row; each is kept on the desk, opening the
    // other way where it would leave it, as a menu keeps on its screen
    const desk = this.getBoundingClientRect();
    const r = row.getBoundingClientRect();
    const m = el.getBoundingClientRect();
    let left = x ?? r.left + r.width / 2 - m.width / 2;
    let top = y ?? r.top - m.height;
    if (left + m.width > desk.right - INSET) left -= m.width;
    if (top + m.height > desk.bottom - INSET) top -= m.height;
    left = Math.max(desk.left + INSET, left);
    top = Math.max(desk.top + INSET, top);
    const box = (el.offsetParent ?? this).getBoundingClientRect();
    el.style.setProperty("--x", `${left - box.left}px`);
    el.style.setProperty("--y", `${top - box.top}px`);
  }

  // only what applies to the account, as the shortcut menu leaves out the rest (present/accounts.rs
  // `offers`; AccountCommands.swift; AccountMenu.cs)
  #fillMenu(a: Account, s: State) {
    const o = offersOf(a, s);
    const offer = (name: string, applies: boolean, words?: string) => {
      const item = this.#menu.el.querySelector<HTMLElement>(`[data-offer="${name}"]`)!;
      shown(item, applies);
      if (words !== undefined) text(part(item, "title") ?? item, words);
      return applies;
    };
    sectioned(this.#menu.el, {
      offers: [
        offer("use", o.use.enabled, o.use.title),
        offer("sign-in-again", o.signInAgain),
        offer("rename", o.rename),
        offer("warm", !!o.warm?.enabled),
      ].some(Boolean),
      open: offer("open", o.open !== null, o.open ?? ""),
      copy: offer("copy", o.copy),
      forget: offer("forget", o.forget),
    });
  }

  #offer(what: string, a = accountOf(model.get(), this.#menuFor ?? undefined)) {
    if (!a) return;
    switch (what) {
      case "use":
        if (!offersOf(a, model.get()).use.enabled) return;
        this.#saying = a.tool;
        return switchFromApp(a.tool, a.label, this.system);
      case "sign-in-again":
        return sheets(this.system)?.signIn(this, a.tool, a);
      case "rename":
        return sheets(this.system)?.rename(this, a);
      case "warm":
        return void warmUp(a.label);
      case "open":
        openWindow(a.tool, a.label);
        return showAccountWindow(this, this.system, a);
      case "copy":
        return void navigator.clipboard?.writeText(a.email).catch(() => {});
      case "forget":
        return sheets(this.system)?.forget(this, a);
    }
  }

  // The menu bar's Account menu, Edit's Copy and Delete and View's Refresh act on the list's
  // selection.
  protected windowMenus(): WindowMenus {
    const s = model.get();
    const pane = this.#pane;
    // only the Accounts pane puts its account up for the menu bar (AccountsWindow.swift)
    const account =
      pane === "accounts" ? (accountOf(s, this.#grid?.selected?.dataset.account) ?? null) : null;
    const refresh =
      pane === "machine"
        ? { title: "Check Again", enabled: !this.#machine?.checking }
        : { title: "Refresh", enabled: pane === "activity" || s.reading === null };
    return { account, refresh, resizes: true };
  }

  protected fromMenuBar(does: string): boolean {
    const row = this.#pane === "accounts" ? (this.#grid?.selected ?? null) : null;
    const a = accountOf(model.get(), row?.dataset.account);
    if (does.startsWith("offer:")) {
      this.#offer(does.slice("offer:".length), a);
      return true;
    }
    switch (does) {
      case "add":
        this.present("add");
        return true;
      case "refresh":
        if (this.#pane === "activity") readLog(this.#panel("activity")!, this.system, model.get());
        else refreshFromApp(this.system);
        return true;
      case "copy":
        if (a?.email) void navigator.clipboard?.writeText(a.email).catch(() => {});
        return true;
      case "delete":
        if (row) this.#ask("forget", row);
        return true;
    }
    return false;
  }

  // after Remove Account the row has gone and nothing is selected; macOS's list keeps focus, and
  // Windows gives it to the window's title bar
  forgotten() {
    this.#grid?.lost();
    if (this.system !== "windows") return;
    const bar = this.querySelector<HTMLElement>('[data-part="title-bar"]');
    if (!bar) return;
    bar.tabIndex = -1;
    bar.focus({ preventScroll: true });
  }

  protected render(s: State) {
    shown(this.window, s.running);
    if (!s.running) this.#menu.closeAll(false);
    text(part(this, "updated"), s.reading !== null ? READING : updated(s.readAt));
    attr(this.querySelector('[data-does="refresh"]'), "disabled", s.reading !== null ? "" : null);
    sheets(this.system)?.render(s);
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
    this.#notices(s);
    this.#setup(s);
    const above = this.querySelectorAll(
      '[data-notice]:not([hidden]), [data-part="setup"]:not([hidden])',
    );
    attr(part(this, "first"), "data-state", above.length ? null : "top");
    for (const group of this.querySelectorAll<HTMLElement>(
      '[role="rowgroup"][data-tool], [data-part="list"] > [data-tool]',
    ))
      this.#rows(s, group, group.dataset.tool as Tool);
    this.#grid?.refresh();
    this.#inspector(s);
  }

  #notices(s: State) {
    for (const box of this.querySelectorAll<HTMLElement>("[data-notice]")) {
      const tool = box.dataset.notice as Tool;
      const n = noticeOf(s, tool, this.system);
      shown(box, !!n);
      if (!n) continue;
      text(part(box, "title"), n.title);
      box.querySelectorAll('[data-part="line"]').forEach((line, i) => {
        shown(line, i < n.lines.length);
        text(line, n.lines[i] ?? "");
      });
      shown(part(box, "countdown"), !!n.countdown);
      text(part(box, "countdown"), n.countdown ?? "");
      const back = box.querySelector<HTMLButtonElement>('[data-does="switch-back"]');
      shown(back?.closest('[data-part="actions"]') ?? null, !!n.back);
      text(back, n.back ?? "");
      attr(back, "disabled", s.switching ? "" : null);
      for (const icon of box.querySelectorAll<HTMLElement>("[data-severity]"))
        shown(icon, icon.dataset.severity === n.severity);
      // a switch asked for here says the notice it leaves
      if (this.#saying === tool && !s.switching) {
        this.#saying = null;
        say(n.title);
      }
    }
  }

  // Windows holds its tip back while a notice shows, whose message it would cover.
  #setup(s: State) {
    const box = part(this, "setup");
    if (!box) return;
    const noticed =
      this.system === "windows" && !!this.querySelector("[data-notice]:not([hidden])");
    const step = noticed ? null : setupStepOf(s, s.declinedSecond, this.system);
    this.#setupTool = step?.tool ?? null;
    shown(box, !!step);
    if (!step) return;
    text(part(box, "title"), step.title);
    text(part(box, "detail"), step.detail);
  }

  // a tool's accounts, the one in use first; macOS's after its heading, Windows' in their own stack,
  // where a row a visitor adds takes ids of its own
  #rows(s: State, group: HTMLElement, tool: Tool) {
    const want = listed(s, tool);
    const windows = this.system === "windows";
    const rows = keyed(
      windows ? part(group, "rows")! : group,
      want.map((a) => a.id),
      windows ? 0 : 1,
      this.#template("row"),
      (row, id) => {
        const details = part(row, "details");
        if (!details) return;
        details.id = `${this.id}-${id}-details`;
        row.querySelector('[data-does="expand"]')?.setAttribute("aria-controls", details.id);
      },
    );
    want.forEach((a, i) => (windows ? this.#expander(s, rows[i], a) : this.#row(s, rows[i], a)));
  }

  // the templates the window's top view draws for every view of its system
  #template(name: string) {
    return document.querySelector<HTMLTemplateElement>(
      `[data-drawing="app ${this.system}"] pb-accounts template[data-part="${name}"]`,
    );
  }

  #row(s: State, row: HTMLElement, a: Account) {
    const on = isInUse(a, s);
    const switching = s.switching?.id === a.id;
    attr(row, "aria-label", accountSpoken(a, s));
    attr(row, "aria-keyshortcuts", keysOf(a, s, this.system));
    shown(part(row, "in-use-symbol"), on);
    shown(part(row, "parked-symbol"), !on);
    text(part(row, "label"), a.label);
    shown(part(row, "plan"), !!a.plan);
    text(part(row, "plan"), a.plan ?? "");
    shown(part(row, "email"), !!a.email);
    text(part(row, "email"), a.email);
    shown(part(row, "in-use"), on && !switching);
    shown(part(row, "switching"), switching);
    const button = row.querySelector<HTMLButtonElement>('[data-does="use"]')!;
    const again = signInExpired(a, s);
    shown(button, !on && !switching);
    // present/accounts.rs `action_of`: Sign In… where the row's sign-in has to be made again
    text(button, again ? signInAsking(this.system) : SWITCH);
    attr(
      button,
      "aria-label",
      on ? null : again ? `${signInToAgain(this.system, a.label)}…` : switchSpoken(a),
    );
    attr(button, "disabled", s.switching && !switching ? "" : null);
    this.#limits(s, row, a);
    // macOS's row says its most pressing note alone
    const said = rowNote(a, s);
    shown(part(row, "note"), !!said);
    text(part(row, "note"), said ?? "");
  }

  #inspectorPanel() {
    return this.querySelector<HTMLElement>('[data-part="inspector"]');
  }

  // ⌃⌘I and the toolbar's Inspector
  #showInspector(on: boolean) {
    if (!this.#inspectorPanel()) return;
    if (on) delete this.window.dataset.state;
    else this.window.dataset.state = "collapsed";
    attr(this.querySelector('[data-does="inspector"]'), "aria-expanded", String(on));
    this.#inspectorPanel()!.inert = !on;
  }

  // AccountInspector.swift: the account selected, its limits, values and commands, or that none is
  #inspector(s: State) {
    const panel = this.#inspectorPanel();
    if (!panel) return;
    const a = accountOf(s, this.#grid?.selected?.dataset.account);
    shown(part(panel, "inspector-account"), !!a);
    shown(part(panel, "inspector-none"), !a);
    if (!a) return;
    const on = isInUse(a, s);
    shown(part(panel, "ins-in-use-symbol"), on);
    shown(part(panel, "ins-parked-symbol"), !on);
    text(part(panel, "ins-label"), a.label);
    shown(part(panel, "ins-plan"), !!a.plan);
    text(part(panel, "ins-plan"), a.plan ?? "");
    const limits = part(panel, "ins-limits")!;
    shown(limits, a.limits.length > 0);
    let rows = [...limits.querySelectorAll<HTMLElement>('[data-part="ins-limit"]')];
    if (rows.length !== a.limits.length) {
      limits.replaceChildren(...a.limits.map(() => cloned(this.#template("ins-limit"))));
      rows = [...limits.children] as HTMLElement[];
    }
    a.limits.forEach((limit, i) => {
      const r = rows[i];
      const l = limitRow(limit, s);
      attr(r, "aria-label", l.spoken);
      text(part(r, "title"), limitTitle(limit));
      text(part(r, "used"), used(l.figure));
      attr(part(r, "fill"), "width", String(l.percent));
      attr(part(r, "fill"), "data-state", l.level);
      text(part(r, "resets"), l.resets);
      text(part(r, "in"), resetsIn(limit.resetsAt, s.now));
    });
    // only a Claude Desktop login in use is read at a time of its own (present/accounts.rs `read`)
    const read = staleNote(a, s);
    shown(part(panel, "ins-read"), !!read);
    text(part(panel, "ins-read"), read ?? "");
    shown(part(panel, "ins-email-row"), !!a.email);
    text(part(panel, "ins-email"), a.email);
    const signIn = signInSaid(a, s);
    shown(part(panel, "ins-sign-in-row"), !!signIn);
    text(part(panel, "ins-sign-in"), signIn ?? "");
    const claude = a.tool === "claude";
    shown(part(panel, "ins-warm-row"), claude);
    if (claude) text(part(panel, "ins-warm"), warmNote(s, a) ?? WARM_UP_OFF);
    // what its shortcut menu offers, only what applies now
    const o = offersOf(a, s);
    const offer = (name: string, applies: boolean, words?: string) => {
      const button = panel.querySelector<HTMLElement>(`[data-offer="${name}"]`);
      shown(button, applies);
      if (words !== undefined) text(button, words);
    };
    offer("use", o.use.enabled, o.use.title);
    offer("sign-in-again", o.signInAgain);
    offer("rename", o.rename);
    offer("warm", !!o.warm?.enabled);
    offer("open", o.open !== null, o.open ?? "");
    offer("forget", o.forget);
  }

  // Windows' row: its header's toggle named as Narrator hears the account, its one note, its end, and
  // its cards
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
    this.#limits(s, row, a);
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
    const signIn = windowsSignIn(a, s);
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
      box.replaceChildren(...a.limits.map(() => cloned(this.#template("limit-card"))));
    a.limits.forEach((limit, i) => {
      const card = box.children[i] as HTMLElement;
      const c = limitCard(limit, s);
      text(part(card, "name"), c.name);
      text(part(card, "used"), c.used);
      text(part(card, "resets"), c.resets);
      text(part(card, "resets-in"), c.resetsIn);
    });
  }

  #limits(s: State, row: HTMLElement, a: Account) {
    const box = part(row, "limits")!;
    shown(box, a.limits.length > 0);
    let groups = [...box.querySelectorAll<HTMLElement>('[data-part="limit"]')];
    if (groups.length !== a.limits.length) {
      box.replaceChildren(...a.limits.map(() => cloned(this.#template("limit"))));
      groups = [...box.children] as HTMLElement[];
    }
    a.limits.forEach((limit, i) => {
      const g = groups[i];
      const l = limitRow(limit, s);
      text(part(g, "short"), l.short);
      text(part(g, "spoken"), l.spoken);
      const fill = part(g, "fill")!;
      attr(fill, "width", String(l.percent));
      attr(fill, "data-state", l.level);
      text(part(g, "figure"), l.figure);
      text(part(g, "resets"), l.resets);
    });
  }
}

customElements.define("pb-accounts", AccountsWindow);
