import {
  clock,
  logged,
  moments,
  share,
  toolOn,
  tools,
  warmUp,
  type Logged,
  type World,
} from "./world.ts";
import { platforms, type Platform } from "../lib/platform.ts";
import { escaped, hm, stamp, statusHeading, statusLines } from "../lib/words.ts";

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

function status(w: World): Session {
  return {
    title: "pitboard",
    command: "pitboard",
    output: tools.flatMap(([tool], i) =>
      [...(i > 0 ? [""] : []), statusHeading(tool), ...statusLines(w, tool)].map((html) =>
        line(html, toolOn[tool]),
      ),
    ),
  };
}

// Where each system keeps the watch service: launchd.rs's label, systemd.rs's unit and task.rs's
// task, with the SID of task.rs's test.
const watchService: Record<Platform, string> = {
  macos: "/Users/me/Library/LaunchAgents/com.datlechin.pitboard.watch.plist",
  linux: "/home/me/.config/systemd/user/pitboard-watch.service",
  windows: "\\Pitboard\\Watch S-1-5-21-1004-1005-1006-1001",
};

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
        return `<span class="dim">${stamp(e.at)}</span>  ${e.verb.padEnd(width)}  ${escaped(e.subject)}  <span class="dim">${e.outcome}</span>`;
      }),
    ),
  };
}

export const sessions = {
  status: status(moments.after),
  statusBefore: status(moments.before),
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
  watchInstall: {
    title: "pitboard watch install",
    command: "pitboard watch install",
    output: [
      line(
        `Pitboard watches Claude Code in the background from now on, and at each sign-in. It switches once a limit of the account in use reaches ${share}%, and warms up the accounts \`pitboard warm on\` turned on.`,
      ),
      line(
        "Your user's systemd runs it while you are signed in. To keep it running when you are not, run `loginctl enable-linger`.",
        ["linux"],
      ),
      ...said((p) => watchService[p]),
      line(""),
      line(
        "Anthropic's terms may cover automatic switching and warm-ups: https://docs.usepitboard.com/concepts/limits#is-this-allowed",
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
      line(`  <b>personal</b>  on   warms up again at ${hm(clock.warmsAgain)}`),
      line(`  <b>work    </b>  <span class="dim">off</span>`),
    ],
  },
  log: log(logged),
} satisfies Record<string, Session>;
