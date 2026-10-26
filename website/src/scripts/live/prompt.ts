// A shell's prompt line: the native field Term.astro draws, under the cursor each terminal draws.
import { edit, freshTab, keys, type Line, type Shell, type TabState } from "../../lib/shell.ts";

// Enter and Ctrl+C take the line as typed, and leave the field to what is typed next; Tab reads
// the line when its turn comes, after the keys before it.
export interface Keys {
  enter: (text: string) => void;
  tab: () => void;
  interrupt: (text: string) => void;
  clear: () => void;
}

export class Prompt {
  readonly field: HTMLInputElement;
  readonly ps: HTMLElement;
  private readonly line: HTMLElement;
  private readonly end: HTMLElement;
  private readonly cursor: HTMLElement;
  private readonly shell: Shell;
  private readonly on: Keys;
  history: string[] = [];
  // where Up and Down are in the history, and the line being typed before they moved
  private at = 0;
  private draft = "";
  private kill = "";
  private undone: Line[] = [];
  private redone: Line[] = [];
  // Escape was the last key: the start of a Meta key in zsh and bash
  private meta = false;
  tabState: TabState = freshTab();

  constructor(line: HTMLElement, shell: Shell, on: Keys) {
    this.line = line;
    this.field = line.querySelector<HTMLInputElement>("[data-line]")!;
    this.ps = line.querySelector<HTMLElement>("[data-ps]")!;
    this.end = line.querySelector<HTMLElement>("[data-end]")!;
    this.cursor = line.querySelector<HTMLElement>("[data-cursor]")!;
    this.shell = shell;
    this.on = on;
    const f = this.field;
    f.addEventListener("keydown", (e) => this.key(e));
    f.addEventListener("beforeinput", () => this.remember());
    for (const type of ["input", "focus", "blur", "scroll", "select"])
      f.addEventListener(type, () => this.draw());
    document.addEventListener("selectionchange", () => {
      if (document.activeElement === f) this.draw();
    });
    new ResizeObserver(() => this.place()).observe(line);
    void document.fonts?.ready.then(() => this.place());
  }

  get value(): Line {
    const f = this.field;
    const caret = f.selectionDirection === "backward" ? f.selectionStart : f.selectionEnd;
    return { value: f.value, caret: caret ?? f.value.length };
  }

  set(line: Line) {
    this.field.value = line.value;
    this.field.setSelectionRange(line.caret, line.caret);
    this.draw();
  }

  // A fresh line, as Enter and Ctrl+C leave it.
  reset() {
    this.set({ value: "", caret: 0 });
    this.undone = [];
    this.redone = [];
    this.tabState = freshTab();
    this.at = this.history.length;
    this.draft = "";
  }

  // A line run, which Up then recalls first.
  record(line: string) {
    this.history.push(line);
    this.at = this.history.length;
    this.draft = "";
  }

  private take(): string {
    const text = this.field.value;
    this.reset();
    return text;
  }

  // What precedes the field on its line: the prompt, a question, or nothing while a program runs.
  setPrompt(html: string) {
    this.ps.innerHTML = html;
    this.place();
  }

  // The field and cursor sit where the prompt's text ends, in whole rows of the terminal's lines.
  place() {
    const box = this.line.getBoundingClientRect();
    const end = this.end.getBoundingClientRect();
    if (box.width === 0) return;
    const lh = parseFloat(getComputedStyle(this.line).lineHeight);
    const row = Math.floor((end.top - box.top) / lh + 0.5);
    this.line.style.setProperty("--row", String(Math.max(0, row)));
    this.line.style.setProperty("--x", String(end.left - box.left));
    this.draw();
  }

  draw() {
    const { value, caret } = this.value;
    this.line.style.setProperty("--at", String(caret));
    this.line.style.setProperty("--scroll", String(this.field.scrollLeft));
    this.cursor.textContent = value[caret] ?? " ";
  }

  private remember() {
    this.undone.push(this.value);
    this.redone = [];
  }

  private change(line: Line) {
    this.remember();
    this.set(line);
  }

  private key(e: KeyboardEvent) {
    if (e.isComposing) return;
    const meta = this.meta;
    this.meta = false;
    if (e.key !== "Tab") this.tabState = freshTab();
    if (e.key === "Tab") {
      // departs from the apps: Shift+Tab leaves the PowerShell prompt
      // departs from the apps: Escape then Tab leaves every prompt
      if (e.shiftKey || e.ctrlKey || e.altKey || e.metaKey || meta) return;
      e.preventDefault();
      this.on.tab();
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      this.meta = true;
      // PSReadLine's RevertLine
      if (this.shell === "powershell" && this.field.value) this.change({ value: "", caret: 0 });
      return;
    }
    if (e.key === "Enter" && !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      this.on.enter(this.take());
      return;
    }
    if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !e.shiftKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      this.recall(e.key === "ArrowUp" ? -1 : 1);
      return;
    }
    if (!e.ctrlKey || e.altKey || e.metaKey) return;
    const name = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (name === "c") {
      // PSReadLine's CopyOrCancelLine copies a selection as the browser does
      const f = this.field;
      if (this.shell === "powershell" && f.selectionStart !== f.selectionEnd) return;
      e.preventDefault();
      this.on.interrupt(this.take());
      return;
    }
    const what = keys[this.shell][name];
    if (!what) return;
    e.preventDefault();
    if (what === "clear-screen") return this.on.clear();
    if (what === "undo") {
      const back = this.undone.pop();
      if (back) {
        this.redone.push(this.value);
        this.set(back);
      }
      return;
    }
    if (what === "redo") {
      const again = this.redone.pop();
      if (again) {
        this.undone.push(this.value);
        this.set(again);
      }
      return;
    }
    const done = edit(this.shell, what, this.value, this.kill);
    if (!done) return;
    if (done.killed) this.kill = done.killed;
    if (done.line.value === this.field.value) this.set(done.line);
    else this.change(done.line);
  }

  // up-line-or-history, previous-history, PreviousHistory: the line before, whatever is typed.
  private recall(step: -1 | 1) {
    const to = this.at + step;
    if (to < 0 || to > this.history.length) return;
    if (this.at === this.history.length) this.draft = this.field.value;
    this.at = to;
    const value = to === this.history.length ? this.draft : this.history[to];
    this.set({ value, caret: value.length });
  }
}
