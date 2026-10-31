// <pb-mac-accounts>: macOS's Accounts window (app redo 4.3), its list a grid with a selection, the
// inspector beside it, and the menu bar's menus acting on the account selected.
import { RowsFigure, accountOf, part } from "./accounts.ts";
import type { WindowMenus } from "./appmenu.ts";
import { attr, cloned, keyed, shown, text } from "./draw.ts";
import { Grid } from "./grid.ts";
import { offersOf } from "./offers.ts";
import { model, refreshFromApp, warmNote, type State } from "../../lib/model.ts";
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
  switchSpoken,
  updated,
} from "../../lib/words.ts";
import {
  SWITCH,
  WARM_UP_OFF,
  keysOf,
  signInAsking,
  signInToAgain,
  used,
} from "../../data/window.ts";
import type { Account, Tool } from "../../data/world.ts";

class MacAccounts extends RowsFigure {
  protected readonly rowSelector = '[role="row"]';
  #grid!: Grid;
  // the row selected as the window opens: the account in use of the first tool shown
  #first: string | null = null;

  protected list() {
    return this.querySelector<HTMLElement>('[role="grid"]')!;
  }

  protected connect() {
    super.connect();
    const grid = this.list();
    this.#grid = new Grid({
      system: "macos",
      grid,
      rows: () =>
        [...grid.querySelectorAll<HTMLElement>(this.rowSelector)].filter((r) => !r.hidden),
      act: (row) => this.#account(row, (a) => this.act(a)),
      forget: (row) => this.#account(row, (a) => this.ask("forget", a)),
      rename: (row) => this.#account(row, (a) => this.ask("rename", a)),
      name: (row) => accountOf(model.get(), row.dataset.account)?.label ?? "",
      selected: () => this.#inspector(model.get()),
    });
    this.#first =
      grid.querySelector<HTMLElement>('[role="row"][aria-selected="true"]')?.dataset.account ??
      null;
    if (this.#first) this.#grid.select(this.rowFor(this.#first), false);
    // ⌘⌫, the Account menu's Remove Account…, wherever focus is in the window but a field
    this.addEventListener("keydown", (e) => {
      const at = e.target as Element;
      if (e.defaultPrevented || !e.metaKey || e.key !== "Backspace") return;
      const selected = this.#grid.selected;
      if (at.closest("dialog, input") || !selected) return;
      e.preventDefault();
      this.#account(selected, (a) => this.ask("forget", a));
    });
    // ⌃⌘I, InspectorCommands' Show and Hide Inspector
    this.addEventListener("keydown", (e) => {
      if (e.defaultPrevented || !this.#inspectorPanel()) return;
      if (!e.metaKey || !e.ctrlKey || e.code !== "KeyI") return;
      e.preventDefault();
      this.#showInspector(this.window.dataset.state === "collapsed");
    });
    // the inspector's own commands, on the account selected
    this.#inspectorPanel()?.addEventListener("click", (e) => {
      const button = (e.target as Element).closest<HTMLElement>("[data-offer]");
      if (button)
        this.offer(
          button.dataset.offer!,
          accountOf(model.get(), this.#grid.selected?.dataset.account),
        );
    });
  }

  #account(row: HTMLElement, does: (a: Account) => void) {
    const a = accountOf(model.get(), row.dataset.account);
    if (a) does(a);
  }

  protected copied() {
    return this.#grid.selected;
  }

  // macOS outlines the row a shortcut menu acts on while it is open
  protected menuShown(row: HTMLElement | null, open: boolean) {
    attr(row, "data-state", open ? "menu" : null);
  }

  protected does(does: string): boolean {
    if (does !== "inspector") return false;
    this.#showInspector(this.window.dataset.state === "collapsed");
    return true;
  }

  protected restart() {
    super.restart();
    this.#grid.select(this.rowFor(this.#first), false);
    this.#showInspector(true);
  }

  // after Remove Account the row has gone and nothing is selected; the list keeps focus
  forgotten() {
    this.#grid.lost();
  }

  // The menu bar's Account menu, Edit's Copy and Delete and View's Refresh act on the list's
  // selection.
  protected windowMenus(): WindowMenus {
    const s = model.get();
    const account = accountOf(s, this.#grid.selected?.dataset.account) ?? null;
    return { account, refresh: { title: "Refresh", enabled: s.reading === null }, resizes: true };
  }

  protected fromMenuBar(does: string): boolean {
    const row = this.#grid.selected;
    const a = accountOf(model.get(), row?.dataset.account);
    if (does.startsWith("offer:")) {
      this.offer(does.slice("offer:".length), a);
      return true;
    }
    switch (does) {
      case "add":
        this.present("add");
        return true;
      case "refresh":
        refreshFromApp(this.system);
        return true;
      case "copy":
        this.offer("copy", a);
        return true;
      case "delete":
        if (a) this.ask("forget", a);
        return true;
    }
    return false;
  }

  protected render(s: State) {
    super.render(s);
    text(part(this, "updated"), s.reading !== null ? READING : updated(s.readAt));
    attr(this.querySelector('[data-does="refresh"]'), "disabled", s.reading !== null ? "" : null);
    this.notices(s);
    this.setup(s);
    const above = this.querySelectorAll(
      '[data-notice]:not([hidden]), [data-part="setup"]:not([hidden])',
    );
    attr(part(this, "first"), "data-state", above.length ? null : "top");
    for (const group of this.querySelectorAll<HTMLElement>('[role="rowgroup"][data-tool]'))
      this.#rows(s, group, group.dataset.tool as Tool);
    this.#grid.refresh();
    this.#inspector(s);
  }

  // a tool's accounts after its heading, the one in use first
  #rows(s: State, group: HTMLElement, tool: Tool) {
    const want = listed(s, tool);
    const rows = keyed(
      group,
      want.map((a) => a.id),
      1,
      this.template("row"),
    );
    want.forEach((a, i) => this.#row(s, rows[i], a));
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
    text(button, again ? signInAsking(this.front) : SWITCH);
    attr(
      button,
      "aria-label",
      on ? null : again ? `${signInToAgain(this.front, a.label)}…` : switchSpoken(a),
    );
    attr(button, "disabled", s.switching && !switching ? "" : null);
    this.limits(s, row, a);
    // the row says its most pressing note alone
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
    const a = accountOf(s, this.#grid.selected?.dataset.account);
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
      limits.replaceChildren(...a.limits.map(() => cloned(this.template("ins-limit"))));
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
}

customElements.define("pb-mac-accounts", MacAccounts);
