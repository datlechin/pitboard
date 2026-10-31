// A list as SwiftUI's List(selection:) and WinUI's ListView run it, with focus on the row: the
// account list in WAI-ARIA's grid pattern (plan 3.3, 3.4 and section 6), and the Open Link
// window's in its listbox pattern.
import type { System } from "../../lib/platform.ts";

export interface GridOptions {
  system: System;
  grid: HTMLElement;
  // what a row is, the grid pattern's row unless said
  row?: string;
  rows: () => HTMLElement[];
  act: (row: HTMLElement) => void;
  // what Delete and F2 do to a row, where the list does anything
  forget?: (row: HTMLElement) => void;
  rename?: (row: HTMLElement) => void;
  // what a row is called, for typing its name
  name: (row: HTMLElement) => string;
  // the selection changed
  selected?: (row: HTMLElement | null) => void;
}

// how long apart typed letters still make one name, until measured (plan section 10)
const TYPED_MS = 1000;

export class Grid {
  #selected: HTMLElement | null = null;
  #current: HTMLElement | null = null;
  #typed = "";
  #typedAt = 0;

  constructor(readonly o: GridOptions) {
    const { grid } = o;
    // the list keeps focus when the row with it goes
    grid.tabIndex = -1;
    grid.addEventListener("click", (e) => {
      const row = this.#rowOf(e.target);
      if (!row || this.#onControl(e.target)) return;
      this.select(row, true);
    });
    grid.addEventListener("dblclick", (e) => {
      const row = this.#rowOf(e.target);
      if (row && !this.#onControl(e.target)) o.act(row);
    });
    grid.addEventListener("keydown", (e) => this.#key(e));
    grid.addEventListener("focusin", (e) => {
      const row = this.#rowOf(e.target);
      if (row) this.#make(row);
    });
  }

  get selected() {
    return this.#selected;
  }

  #rowOf(target: EventTarget | null): HTMLElement | null {
    const row =
      target instanceof Element ? target.closest<HTMLElement>(this.o.row ?? '[role="row"]') : null;
    return row && this.o.grid.contains(row) ? row : null;
  }

  #onControl(target: EventTarget | null) {
    return target instanceof Element && !!target.closest("button, input, a");
  }

  // the row Tab comes back to, and its button the one after it
  #make(row: HTMLElement | null) {
    this.#current = row;
    for (const r of this.o.rows()) {
      r.tabIndex = r === row ? 0 : -1;
      for (const b of r.querySelectorAll<HTMLButtonElement>("button"))
        b.tabIndex = r === row ? 0 : -1;
    }
  }

  // the rows drawn again: the current row stays the Tab stop, or the first does
  refresh() {
    const rows = this.o.rows();
    if (this.#selected && !rows.includes(this.#selected)) this.select(null, false);
    if (!this.#current || !rows.includes(this.#current))
      this.#make(this.#selected ?? rows[0] ?? null);
    else this.#make(this.#current);
  }

  select(row: HTMLElement | null, focus: boolean) {
    if (this.#selected !== row) {
      this.#selected?.setAttribute("aria-selected", "false");
      row?.setAttribute("aria-selected", "true");
      this.#selected = row;
      this.o.selected?.(row);
    }
    if (row) this.#make(row);
    if (row && focus) row.focus({ preventScroll: true });
  }

  // after a row goes: nothing selected, and the list keeps focus
  lost() {
    this.select(null, false);
    this.#make(this.o.rows()[0] ?? null);
    this.o.grid.focus({ preventScroll: true });
  }

  #key(e: KeyboardEvent) {
    const { system } = this.o;
    const row = this.#rowOf(e.target);
    const onGrid = e.target === this.o.grid;
    if ((!row && !onGrid) || (row && e.target !== row)) return;
    const rows = this.o.rows();
    const at = row ? rows.indexOf(row) : -1;
    const to = (i: number, select = true) => {
      e.preventDefault();
      const next = rows[Math.max(0, Math.min(rows.length - 1, i))];
      if (!next) return;
      if (select) this.select(next, true);
      else {
        this.#make(next);
        next.focus({ preventScroll: true });
      }
    };
    const mac = system === "macos";
    const focusOnly = !mac && e.ctrlKey;
    switch (e.key) {
      case "ArrowDown":
        return to(at < 0 ? 0 : at + 1, !focusOnly);
      case "ArrowUp":
        return to(at < 0 ? rows.length - 1 : at - 1, !focusOnly);
      case "Home":
      case "PageUp":
        if (mac) return;
        return to(0, !focusOnly);
      case "End":
      case "PageDown":
        if (mac) return;
        return to(rows.length - 1, !focusOnly);
      case "Enter":
        if (!row) return;
        e.preventDefault();
        // the list's own primary gesture selects the row on Windows
        if (!mac) this.select(row, false);
        return this.o.act(row);
      case "Backspace":
      case "Delete": {
        // macOS's Delete and Forward Delete, Windows' Delete, on the selection
        if (!mac && e.key === "Backspace") return;
        const on = mac ? this.#selected : row;
        if (!on || !this.o.forget) return;
        e.preventDefault();
        return this.o.forget(on);
      }
      case "F2":
        if (mac || !row || !this.o.rename) return;
        e.preventDefault();
        return this.o.rename(row);
      case " ":
        if (!mac && row && e.ctrlKey) {
          e.preventDefault();
          this.select(this.#selected === row ? null : row, true);
        }
        return;
    }
    if (mac && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      this.#typeSelect(e.key, rows);
    }
  }

  // typing an account's name selects the first row it begins
  #typeSelect(key: string, rows: HTMLElement[]) {
    const now = performance.now();
    this.#typed = now - this.#typedAt > TYPED_MS ? key : this.#typed + key;
    this.#typedAt = now;
    const typed = this.#typed.toLocaleLowerCase();
    const found = rows.find((r) => this.o.name(r).toLocaleLowerCase().startsWith(typed));
    if (found) this.select(found, true);
  }
}
