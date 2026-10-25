import { clock, logged, toolOn, tools, warmUp, type Logged, type Moment } from "./world.ts";
import { platforms, type Platform } from "../lib/platform.ts";
import { statusHeading, statusLines } from "../lib/words.ts";

export interface Line {
  html: string;
  // where not every system the session runs on prints it
  on?: Platform[];
}

export interface Session {
  title: string;
  command: string;
  output: Line[];
  // where it does not run on every system
  on?: Platform[];
}

const line = (html: string, on?: Platform[]): Line =>
  on && on.length < platforms.length ? { html, on } : { html };

// A line as each system prints it: one where they all print it alike, else one for each wording.
function said(html: (platform: Platform) => string | null): Line[] {
  const by = new Map<string, Platform[]>();
  for (const p of platforms) {
    const h = html(p);
    if (h !== null) by.set(h, [...(by.get(h) ?? []), p]);
  }
  return [...by].map(([h, on]) => line(h, on));
}

export function printed(session: Session, platform: Platform): string[] {
  if (session.on && !session.on.includes(platform))
    throw new Error(`${session.command} does not run on ${platform}`);
  return session.output.filter((l) => !l.on || l.on.includes(platform)).map((l) => l.html);
}

function status(moment: Moment): Session {
  return {
    title: "pitboard",
    command: "pitboard",
    output: tools.flatMap(([tool], i) =>
      [...(i > 0 ? [""] : []), statusHeading(tool), ...statusLines(tool, moment)].map((html) =>
        line(html, toolOn[tool]),
      ),
    ),
  };
}

// watch.rs `warms`: nothing where no account is turned on.
function warms(): string {
  const on = warmUp.accounts.map((label) => `<b>${label}</b>`);
  if (on.length === 0) return "";
  const named = on.length > 1 ? `${on.slice(0, -1).join(", ")} and ${on.at(-1)}` : on[0];
  return `, and warms up ${named} from ${warmUp.from} to ${warmUp.to}`;
}

// main.rs `log`: each verb padded to the longest of the entries shown.
function log(entries: Logged[]): Session {
  const shown = (p: Platform) => entries.filter((e) => !e.on || e.on.includes(p));
  return {
    title: "pitboard log",
    command: `pitboard log -n ${entries.length}`,
    output: entries.flatMap((e) =>
      said((p) => {
        if (!shown(p).includes(e)) return null;
        const width = Math.max(...shown(p).map((s) => s.verb.length));
        return `<span class="dim">${e.day}T${e.time}+07:00</span>  ${e.verb.padEnd(width)}  ${e.account}  <span class="dim">ok</span>`;
      }),
    ),
  };
}

export const sessions = {
  status: status("after"),
  statusBefore: status("before"),
  use: {
    title: "pitboard use work",
    command: "pitboard use work",
    output: [
      line("Switched to <b>work</b>; <b>personal</b> is parked."),
      line("Claude Code sessions already running follow within 33 seconds."),
    ],
  },
  useDesktop: {
    title: "pitboard use desktop/home",
    command: "pitboard use desktop/home",
    on: toolOn.desktop,
    output: [
      line("Switched to <b>desktop/home</b>; <b>desktop/office</b> is parked."),
      line("Claude opens signed in to it."),
    ],
  },
  watch: {
    title: "pitboard watch",
    command: "pitboard watch",
    output: [
      line(
        `Watching Claude Code. Pitboard switches to another of your accounts once a limit of the one in use reaches 95%${warms()}. Stop with Ctrl-C.`,
      ),
      line(
        `<span class="dim">${clock.read}:14</span> Watching <b>work</b>: 40% of its weekly limit, as of ${clock.read}.`,
      ),
    ],
  },
  warm: {
    title: "pitboard warm",
    command: "pitboard warm",
    output: [
      // warm.rs `warmers`: Linux has no app.
      ...said(
        (p) =>
          `Pitboard warms up these accounts from ${warmUp.from} to ${warmUp.to}, while ${p === "linux" ? "" : "the app or "}\`pitboard watch\` runs.`,
      ),
      line(`  <b>personal</b>  on   warms up again at ${clock.warmsAgain}`),
      line(`  <b>work    </b>  <span class="dim">off</span>`),
    ],
  },
  log: log(logged),
} satisfies Record<string, Session>;
