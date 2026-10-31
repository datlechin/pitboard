// What the Accounts window of every system shares over the one state (plan 3.3 to 3.5, app redo 4.3
// on macOS): an account's switch, its sign-in and its sheets, the notices above the accounts and
// the tip, each read from the model. Each system's window is a module of its own over it:
// mac-accounts.ts, win-window.ts and linux-window.ts.
import { attr, cloned, shown, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { bringForward } from "./forward.ts";
import { hold } from "./hold.ts";
import { INSET, Menu, sectioned } from "./menu.ts";
import { offersOf } from "./offers.ts";
import { say } from "./say.ts";
import { sheets, type Sheets } from "./sheets.ts";
import { showAccountWindow } from "./windows.ts";
import {
  declineSecond,
  dismissNotice,
  model,
  openWindow,
  refreshFromApp,
  switchBackFromApp,
  switchFromApp,
  warmUp,
  type State,
} from "../../lib/model.ts";
import { isInUse, limitRow, signInExpired } from "../../lib/words.ts";
import { noticeOf, setupStepOf } from "../../data/window.ts";
import { frontOf, systemOf, type Front, type System } from "../../lib/platform.ts";
import type { Account, Tool } from "../../data/world.ts";

export const part = <T extends Element = HTMLElement>(in_: ParentNode, name: string) =>
  in_.querySelector<T>(`[data-part="${name}"]`);
export const accountOf = (s: State, id: string | undefined) => s.accounts.find((a) => a.id === id);

export type Offer = "use" | "sign-in-again" | "rename" | "warm" | "open" | "copy" | "forget";

export abstract class AccountsFigure extends LiveFigure {
  window!: HTMLElement;
  // the tool whose switch was asked for here, whose notice is said once it is made
  #saying: Tool | null = null;
  // the tool the tip asks about
  #setupTool: Tool | null = null;
  // the listeners this window puts on the document, taken off as it leaves the page
  protected off = new AbortController();
  // what an account's row or group is
  protected abstract readonly rowSelector: string;

  // the system it draws: its own, or the desktop shown for the Linux window both draw
  get system(): System {
    return systemOf(this);
  }

  get front(): Front {
    return frontOf(this.system);
  }

  // the desk a sheet keeps to: the figure's own element, which draws the desk around the window
  get desk(): HTMLElement {
    return this;
  }

  protected get sheets(): Sheets | null {
    return sheets(this.system);
  }

  connectedCallback() {
    this.off = new AbortController();
    this.window = this.windowOf();
    this.connect();
    this.addEventListener("click", (e) => this.#clicked(e));
    super.connectedCallback();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.off.abort();
  }

  // the window the figure draws, which a sheet opens over
  protected windowOf(): HTMLElement {
    return this.querySelector<HTMLElement>(':scope > [data-part="window"]')!;
  }

  // the system's own controls, wired once
  protected abstract connect(): void;

  // a press on one of the window's own controls, before the ones every window has
  protected does(_does: string, _el: HTMLElement, _row: HTMLElement | null): boolean {
    return false;
  }

  protected rowFor(id: string | null) {
    return id ? this.querySelector<HTMLElement>(`${this.rowSelector}[data-account="${id}"]`) : null;
  }

  #clicked(e: MouseEvent) {
    const does = (e.target as Element).closest<HTMLElement>("[data-does]");
    if (!does || !this.window.contains(does)) return;
    const row = does.closest<HTMLElement>(this.rowSelector);
    if (this.does(does.dataset.does!, does, row)) return;
    const account = accountOf(model.get(), row?.dataset.account);
    switch (does.dataset.does) {
      case "use":
        if (account) this.act(account);
        return;
      case "copy-email":
        if (account) this.offer("copy", account);
        return;
      case "open":
      case "sign-in-again":
      case "rename":
      case "warm":
      case "forget":
        if (account) this.offer(does.dataset.does, account);
        return;
      case "refresh":
        return refreshFromApp(this.system);
      // SettingsSection.WarmUp, which macOS's Settings opens on its Limits pane
      case "warm-settings":
        return bringForward("warm", this.system);
      case "add":
        return this.sheets?.signIn(
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
  }

  // Switch, its key or a double click: the account's button, or its sign-in where that has to be
  // made again (present/accounts.rs `action_of`)
  protected act(a: Account) {
    const s = model.get();
    if (isInUse(a, s) || s.switching) return;
    if (signInExpired(a, s)) return this.sheets?.signIn(this, a.tool, a);
    this.#saying = a.tool;
    switchFromApp(a.tool, a.label, this.system);
  }

  protected ask(what: "forget" | "rename", a: Account) {
    if (what === "forget" && !isInUse(a, model.get())) this.sheets?.forget(this, a);
    if (what === "rename") this.sheets?.rename(this, a);
  }

  // what an account's menu or its inspector offers, done to it
  protected offer(what: string, a: Account | undefined) {
    if (!a) return;
    switch (what as Offer) {
      case "use":
        if (!offersOf(a, model.get()).use.enabled) return;
        this.#saying = a.tool;
        return switchFromApp(a.tool, a.label, this.system);
      case "sign-in-again":
        return this.sheets?.signIn(this, a.tool, a);
      case "rename":
        return this.sheets?.rename(this, a);
      case "warm":
        return void warmUp(a.label);
      case "open":
        openWindow(a.tool, a.label);
        return showAccountWindow(this, this.system, a);
      case "copy":
        if (a.email) void navigator.clipboard?.writeText(a.email).catch(() => {});
        return;
      case "forget":
        return this.sheets?.forget(this, a);
    }
  }

  // the window's Add Account sheet, asked for from elsewhere, as the menu bar's or the tray's
  // Add Account… asks
  present(sheet: string) {
    if (sheet === "add") this.sheets?.signIn(this, null, null);
  }

  // after Remove Account the account's row has gone
  forgotten() {}

  // the templates the window's top view draws for every view of its system
  protected template(name: string) {
    const drawn = this.closest<HTMLElement>("[data-drawing]")!.dataset.drawing;
    return document.querySelector<HTMLTemplateElement>(
      `[data-drawing="${drawn}"] ${this.localName} template[data-part="${name}"]`,
    );
  }

  // a notice a tool, the model's for it (present/notices.rs), and said once where its switch was
  // asked for here
  protected notices(s: State) {
    for (const box of this.querySelectorAll<HTMLElement>("[data-notice]")) {
      const tool = box.dataset.notice as Tool;
      const n = noticeOf(s, tool, this.front);
      shown(box, !!n);
      if (!n) continue;
      text(part(box, "title"), n.title);
      const lines = box.querySelectorAll('[data-part="line"]');
      lines.forEach((line, i) => {
        shown(line, i < n.lines.length);
        text(line, n.lines[i] ?? "");
      });
      // a row that says its lines as one subtitle, newlines between them
      const said = part(box, "lines");
      shown(said, n.lines.length > 0);
      text(said, n.lines.join("\n"));
      shown(part(box, "countdown"), !!n.countdown);
      text(part(box, "countdown"), n.countdown ?? "");
      const back = box.querySelector<HTMLButtonElement>('[data-does="switch-back"]');
      shown(back?.closest('[data-part="actions"]') ?? null, !!n.back);
      if (n.back) text(back, n.back);
      attr(back, "disabled", s.switching ? "" : null);
      shown(part(box, "learn"), !!n.learn);
      for (const icon of box.querySelectorAll<HTMLElement>("[data-severity]"))
        shown(icon, icon.dataset.severity === n.severity);
      if (this.#saying === tool && !s.switching) {
        this.#saying = null;
        say(n.title);
      }
    }
  }

  // the tip, the model's next step for setting up, where the window draws one
  protected setup(s: State, held = false) {
    const box = part(this, "setup");
    if (!box) return;
    const step = held ? null : setupStepOf(s, s.declinedSecond, this.front);
    this.#setupTool = step?.tool ?? null;
    shown(box, !!step);
    if (!step) return;
    text(part(box, "title"), step.title);
    text(part(box, "detail"), step.detail);
  }

  // an account's limits as macOS's and Windows' rows draw them: a group a limit, its bar and its
  // words
  protected limits(s: State, row: HTMLElement, a: Account) {
    const box = part(row, "limits")!;
    shown(box, a.limits.length > 0);
    let groups = [...box.querySelectorAll<HTMLElement>('[data-part="limit"]')];
    if (groups.length !== a.limits.length) {
      box.replaceChildren(...a.limits.map(() => cloned(this.template("limit"))));
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

// A row's shortcut menu on macOS and Windows: a right click, a Control-click or a two-finger
// click opens it where it is, a touch held still too, and on Windows Shift+F10 and the menu key
// open it above the row; on macOS no key does. A press anywhere but the menu closes it.
export abstract class RowsFigure extends AccountsFigure {
  protected menu!: Menu;
  protected menuFor: string | null = null;
  // what raised the coming contextmenu: a right or Control press, or the keys that open a menu
  #raised: "pointer" | "keys" | null = null;
  // a Control-click's click, which selects nothing
  #swallow = false;

  // the list the rows are in
  protected abstract list(): HTMLElement;

  // the menu has opened or closed over `row`
  protected menuShown(_row: HTMLElement | null, _open: boolean) {}

  protected connect() {
    this.menu = new Menu(this.querySelector<HTMLElement>(`#${this.id}-menu`)!, {
      toolkit: this.system === "macos" ? "appkit" : "winui",
      desk: this,
      choose: (item) =>
        this.offer(item.dataset.offer!, accountOf(model.get(), this.menuFor ?? undefined)),
      shown: (open, refocus) => {
        const row = this.rowFor(this.menuFor);
        this.menuShown(row, open);
        if (!open && refocus)
          (row?.querySelector<HTMLElement>('[data-does="expand"]') ?? row)?.focus({
            preventScroll: true,
          });
      },
    });
    const grid = this.list();
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
    document.addEventListener(
      "pointerdown",
      (e) => {
        if (this.menu.open && this.offsetParent && !this.menu.contains(e.target as Node))
          this.menu.closeAll(false);
      },
      { capture: true, signal: this.off.signal },
    );
    grid.addEventListener("keydown", (e) => {
      if (e.key !== "ContextMenu" && !(e.key === "F10" && e.shiftKey)) return;
      e.preventDefault();
      this.#raised = "keys";
      const row = (e.target as Element).closest<HTMLElement>(this.rowSelector);
      if (row && this.system === "windows") this.openMenu(row, null, null);
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
      const row = (e.target as Element).closest<HTMLElement>(this.rowSelector);
      // the keys above answered it already, where a browser raises one after them
      if (!row || by === "keys") return;
      // a touch held still opened it already (hold.ts)
      if (this.menu.open && this.menuFor === row.dataset.account) return;
      // with no press before it, a screen reader asked, as VoiceOver's VO-Shift-M does
      if (by === "pointer") this.openMenu(row, e.clientX, e.clientY);
      else this.openMenu(row, null, null);
    });
    hold(grid, this.rowSelector, (row, x, y) => this.openMenu(row, x, y));
    // the list draws its selection in the accent while it, or nothing else in the window, has focus
    this.window.addEventListener("focusin", (e) =>
      attr(grid, "data-key", grid.contains(e.target as Node) ? "" : null),
    );
    // ⌘C and Ctrl+C copy the account's email, with nothing on screen saying so
    grid.addEventListener("copy", (e) => {
      const email = accountOf(model.get(), this.copied()?.dataset.account)?.email;
      if (!email) return;
      e.preventDefault();
      e.clipboardData?.setData("text/plain", email);
    });
  }

  // the row ⌘C or Ctrl+C copies the email of
  protected abstract copied(): HTMLElement | null;

  protected restart() {
    this.menu.closeAll(false);
    this.sheets?.close();
  }

  protected openMenu(row: HTMLElement, x: number | null, y: number | null) {
    const s = model.get();
    const a = accountOf(s, row.dataset.account);
    if (!a) return;
    this.menu.closeAll(false);
    this.menuFor = a.id;
    this.#fillMenu(a, s);
    this.menuShown(row, true);
    const el = this.menu.el;
    this.menu.show(
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
      const item = this.menu.el.querySelector<HTMLElement>(`[data-offer="${name}"]`)!;
      shown(item, applies);
      if (words !== undefined) text(part(item, "title") ?? item, words);
      return applies;
    };
    sectioned(this.menu.el, {
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

  protected render(s: State) {
    shown(this.window, s.running);
    if (!s.running) this.menu.closeAll(false);
    this.sheets?.render(s);
  }
}
