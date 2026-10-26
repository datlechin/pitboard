// The terminals: each window a shell of its own, which starts with the session it draws and its
// history holding that command. A line runs `pitboard` against the one state, through
// src/lib/cli.ts, and the shell's built-ins through src/lib/shell.ts; output goes into the log
// above the prompt line, which grows with it to the terminal's rows, then scrolls.
import { LiveFigure } from "./figure.ts";
import { Prompt } from "./prompt.ts";
import { toolbar } from "./toolbar.ts";
import type { Machine, Running, Step } from "../../lib/cli.ts";
import type { State } from "../../lib/model.ts";
import type { Platform } from "../../lib/platform.ts";
import { escaped } from "../../lib/words.ts";
import {
  builtin,
  cd,
  continuation,
  echo,
  history,
  homes,
  interrupted,
  isPitboard,
  notFound,
  pwd,
  read,
  shellOn,
  stopped,
  tab,
  type Shell,
  type Simple,
} from "../../lib/shell.ts";

// The command line and what it reads load when a terminal is first used, not with the page.
type Loaded = typeof import("../../lib/cli.ts") & typeof import("../../lib/machines.ts");
let loading: Promise<Loaded> | undefined;
const load = () =>
  (loading ??= Promise.all([import("../../lib/cli.ts"), import("../../lib/machines.ts")]).then(
    ([cli, machines]) => ({ ...cli, ...machines }),
  ));

class Term extends LiveFigure {
  private shell!: Shell;
  private platform!: Platform;
  private log!: HTMLElement;
  private line!: HTMLElement;
  private prompt!: Prompt;
  private drawn: Node[] = [];
  private cwd = "";
  private running: Running | null = null;
  private asking: ((reply: string) => Step) | null = null;
  // a line that left a quote open, and the quote
  private open: { text: string; quote: "'" | '"' } | null = null;
  // what is left of a line's commands while one asks or runs
  private queue: (() => Promise<void>)[] = [];
  private machine?: Promise<Machine>;
  private row: HTMLElement | null = null;
  // the keys that act on the line, each in turn as the shell takes them
  private keys: Promise<void> = Promise.resolve();

  connectedCallback() {
    const drawing = this.closest<HTMLElement>("[data-drawing]")?.dataset.drawing ?? "";
    this.platform = drawing.split(" ")[1] as Platform;
    this.shell = shellOn[this.platform];
    this.cwd = homes[this.shell];
    this.line = this.querySelector<HTMLElement>("[data-prompt]")!;
    this.log = this.line.parentElement!;
    this.drawn = [...this.log.childNodes]
      .filter((n) => n !== this.line)
      .map((n) => n.cloneNode(true));
    this.prompt = new Prompt(this.line, this.shell, {
      enter: (text) => this.inTurn(() => this.enter(text)),
      tab: () => this.inTurn(() => this.complete()),
      interrupt: (text) => this.inTurn(async () => this.interrupt(text)),
      clear: () => this.clear(),
    });
    this.prompt.history = [this.dataset.command ?? ""];
    this.prompt.reset();
    // a tap on a command puts it in at the caret and runs the line, as a paste and Return would
    this.row = this.querySelector<HTMLElement>("[data-commands]");
    if (this.row)
      toolbar(this.row, (button) => {
        const { value, caret } = this.prompt.value;
        const text = value.slice(0, caret) + (button.textContent ?? "").trim() + value.slice(caret);
        this.prompt.reset();
        this.inTurn(() => this.enter(text));
      });
    // a click in the window gives the prompt focus, as a terminal's does, and a drag selects
    this.log.addEventListener("click", (e) => {
      if ((e as PointerEvent).pointerType === "touch" || e.target === this.prompt.field) return;
      if (getSelection()?.isCollapsed !== false) this.prompt.field.focus({ preventScroll: true });
    });
    super.connectedCallback();
  }

  protected render(state: State) {
    if (this.running) this.print(this.running.next(state));
    // the row's switch is to the Claude Code account not in use
    const swap = [...(this.row?.querySelectorAll("button") ?? [])].find((b) =>
      b.textContent?.trim().startsWith("pitboard use "),
    );
    if (swap) {
      const other = state.accounts.find(
        (a) => a.tool === "claude" && a.label !== state.inUse.claude,
      );
      swap.hidden = !other;
      if (other && swap.textContent?.trim() !== `pitboard use ${other.label}`)
        swap.textContent = `pitboard use ${other.label}`;
    }
  }

  private inTurn(act: () => Promise<void>) {
    this.keys = this.keys.then(act, act);
  }

  private machineFor(): Promise<Machine> {
    return (this.machine ??= load().then((m) => m.machine(this.platform, this.dataset.version!)));
  }

  // PowerShell's prompt names the folder it is in; zsh's and bash's, as drawn, do not.
  private ps(): string {
    const dim = {
      zsh: "text-mac-term-dim",
      bash: "text-linux-term-dim",
      powershell: "text-win-term-dim",
    }[this.shell];
    const said = this.shell === "powershell" ? `PS ${this.cwd}>` : this.shell === "zsh" ? "%" : "$";
    return `<span class="${dim}">${escaped(said)}</span> `;
  }

  // Reset puts every terminal back to its session
  protected restart() {
    this.running?.stop();
    this.running = null;
    this.asking = null;
    this.open = null;
    this.queue = [];
    this.cwd = homes[this.shell];
    this.log.replaceChildren(...this.drawn.map((n) => n.cloneNode(true)), this.line);
    this.log.style.removeProperty("--min");
    this.prompt.history = [this.dataset.command ?? ""];
    this.prompt.reset();
    this.ready();
  }

  // Lines of output, above the prompt line; the window then shows the newest.
  private print(lines: string[]) {
    for (const text of lines) {
      const div = document.createElement("div");
      div.innerHTML = text || " ";
      this.log.insertBefore(div, this.line);
    }
    if (lines.length) this.log.scrollTop = this.log.scrollHeight;
  }

  // The line as typed after what preceded it, kept above as the terminal keeps it.
  private echo(text: string, after = "") {
    const div = document.createElement("div");
    div.innerHTML = this.prompt.ps.innerHTML;
    div.append(text);
    if (after) div.insertAdjacentHTML("beforeend", after);
    this.log.insertBefore(div, this.line);
    this.below(null);
    this.log.scrollTop = this.log.scrollHeight;
  }

  // zsh's list below the prompt line, until the line is run.
  private below(lines: string[] | null) {
    this.log.querySelector(":scope > [data-below]")?.remove();
    if (!lines) return;
    const box = document.createElement("div");
    box.dataset.below = "";
    box.innerHTML = lines.map((l) => `<div>${l || " "}</div>`).join("");
    this.log.append(box);
    this.log.scrollTop = this.log.scrollHeight;
  }

  // A fresh prompt, or the shell's own for a line that left a quote open.
  private ready() {
    this.prompt.setPrompt(
      this.open ? escaped(continuation[this.shell](this.open.quote)) : this.ps(),
    );
  }

  private async enter(text: string) {
    if (this.asking) {
      const answer = this.asking;
      this.asking = null;
      this.echo(text);
      this.prompt.setPrompt("");
      await this.resume(answer(text));
      return;
    }
    // a program that runs reads nothing typed, which the terminal echoes
    if (this.running) {
      this.echo(text);
      return;
    }
    this.echo(text);
    const whole = this.open ? `${this.open.text}\n${text}` : text;
    if (!whole.trim()) {
      this.open = null;
      return this.ready();
    }
    const parsed = read(this.shell, whole, this.cwd);
    if (parsed.kind === "open") {
      this.open = { text: whole, quote: parsed.quote };
      return this.ready();
    }
    this.open = null;
    this.prompt.record(whole);
    this.prompt.setPrompt("");
    let status = 0;
    this.queue = parsed.list.map(({ pipeline }, i) => async () => {
      const op = i > 0 ? parsed.list[i - 1].after : null;
      if ((op === "&&" && status !== 0) || (op === "||" && status === 0)) return;
      status = await this.pipeline(pipeline, whole);
    });
    await this.drain();
  }

  // Runs what is left of the line, until a command asks or runs.
  private async drain() {
    while (this.queue.length && !this.asking && !this.running) await this.queue.shift()!();
    if (!this.asking && !this.running) this.ready();
  }

  private async resume(step: Step) {
    this.take(step);
    await this.drain();
  }

  // What a step leaves: its lines, then a question on the prompt line or a program running.
  private take(step: Step) {
    this.print(step.lines);
    if (step.ask) {
      this.asking = step.ask.answer;
      this.prompt.setPrompt(escaped(step.ask.question));
    } else if (step.runs) {
      this.running = step.runs;
      this.prompt.setPrompt("");
    }
  }

  // A pipeline prints what its last command prints; output sent to a file is not drawn.
  private async pipeline(stages: Simple[], line: string): Promise<number> {
    let status = 0;
    for (const [i, simple] of stages.entries()) {
      status = await this.simple(simple, line, i === stages.length - 1 && !simple.redirected);
      if (this.asking || this.running) break;
    }
    return status;
  }

  private async simple(s: Simple, line: string, shown: boolean): Promise<number> {
    const [name, ...args] = s.words;
    if (name === undefined) return 0;
    const show = (lines: string[]) => {
      if (shown) this.print(lines);
    };
    if (isPitboard(this.shell, name)) {
      const { run } = await load();
      const step = await run(await this.machineFor(), args);
      if (shown || step.ask || step.runs) this.take(step);
      return step.failed ? 1 : 0;
    }
    switch (builtin(this.shell, name)) {
      case "clear":
        this.clear();
        return 0;
      case "history":
        // bash lists the line that asked; zsh and PowerShell the lines before it
        show(
          history(this.shell, this.prompt.history.slice(0, this.shell === "bash" ? undefined : -1)),
        );
        return 0;
      case "echo":
        show(echo(this.shell, args).lines);
        return 0;
      case "pwd":
        show(pwd(this.shell, this.cwd));
        return 0;
      case "cd": {
        const moved = cd(this.shell, this.cwd, args, line, s.at[0] ?? 0);
        if ("cwd" in moved) {
          this.cwd = moved.cwd;
          return 0;
        }
        this.print(moved.lines);
        return 1;
      }
      // the window stays, where a terminal would close it
      case "exit":
        return 0;
      default:
        this.print(notFound(this.shell, name, line, s.at[0] ?? 0));
        return 127;
    }
  }

  private interrupt(text: string) {
    if (this.asking || this.running) {
      const echoed = stopped(this.shell);
      this.running?.stop();
      this.running = null;
      this.asking = null;
      this.queue = [];
      if (echoed !== null || text) this.echo(text, echoed ?? "");
      return this.ready();
    }
    this.echo(text, interrupted(this.shell));
    this.open = null;
    this.ready();
  }

  // Ctrl+L, `clear` and `cls`: the window keeps its height, as a terminal's window does.
  private clear() {
    this.log.style.setProperty("--min", `${this.log.getBoundingClientRect().height}px`);
    for (const node of [...this.log.childNodes]) if (node !== this.line) node.remove();
    this.log.scrollTop = 0;
  }

  private async complete() {
    const machine = await this.machineFor();
    const line = this.prompt.value;
    const done = tab(this.shell, machine.grammar.tree, line, this.prompt.tabState);
    if (!done) return;
    // bash lists above the line it draws again; zsh below it, until the line is run
    if (done.list && done.listAbove) {
      this.echo(line.value);
      this.print(done.list);
    } else if (done.list) this.below(done.list);
    this.prompt.set(done.line);
    this.prompt.tabState = done.state;
  }
}

customElements.define("pb-term", Term);
