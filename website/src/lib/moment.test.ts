import assert from "node:assert/strict";
import { test } from "node:test";
import { noticeMoment } from "./moment.ts";
import { forget, model, quit, rename, reset, setChoice, setShare, type State } from "./model.ts";
import { headsUp, hm, menuBarClock, menuBarTitle } from "./words.ts";
import { clock } from "../data/world.ts";

const said = (s: State) => {
  const m = noticeMoment(s)!;
  const p = m.posted;
  return {
    at: menuBarClock(m.at),
    item: menuBarTitle(m.state),
    posted: p && [p.title.macos, p.subtitle, p.body, p.button?.label ?? null],
  };
};

test("at the page's start the row's moment is the static figure's: 05:53, and Notify Me's heads-up", () => {
  reset();
  assert.deepEqual(said(model.get()), {
    at: menuBarClock(clock.warned),
    item: "personal 95%",
    posted: [headsUp.title.macos, headsUp.tool, headsUp.body, "work"],
  });
});

test("a lower share comes sooner at the same pace, and one already read comes at once, the 5-hour first", () => {
  reset();
  setShare(90);
  assert.deepEqual(said(model.get()), {
    at: "Wed 14 Oct 03:50",
    item: "personal 90%",
    posted: [
      "Weekly Limit Almost Reached",
      "Claude Code",
      "personal has used 90%. work has used 40%.",
      "work",
    ],
  });
  setShare(70);
  assert.deepEqual(said(model.get()).posted?.slice(0, 3), [
    "Weekly Limit Almost Reached",
    "Claude Code",
    "personal has used 73%. work has used 40%.",
  ]);
  assert.equal(hm(noticeMoment(model.get())!.at), hm(clock.menuBar));
  setShare(55);
  assert.deepEqual(said(model.get()).posted, [
    "5-Hour Limit Almost Reached",
    "Claude Code",
    "personal has used 59%. work has used 12%.",
    "work",
  ]);
});

test("under Switch Automatically the row shows the switch, with Switch Back, and under Do Nothing nothing", () => {
  reset();
  setChoice("switch");
  assert.deepEqual(said(model.get()), {
    at: menuBarClock(clock.warned),
    item: "work 40%",
    posted: [
      "Switched to work",
      "Claude Code",
      "personal has used 95% of its weekly limit. Sessions already running follow within 33 seconds.",
      "personal",
    ],
  });
  setChoice("nothing");
  assert.equal(said(model.get()).posted, null);
});

test("the moment names accounts as the page does, and a quit app posts nothing", () => {
  reset();
  rename("claude", "personal", "<b>me");
  assert.equal(said(model.get()).posted?.[2], "<b>me has used 95%. work has used 40%.");
  forget("claude", "work");
  assert.deepEqual(said(model.get()).posted?.slice(2), [
    "<b>me has used 95%. Resets in 5d 8h.",
    null,
  ]);
  reset();
  quit();
  assert.equal(said(model.get()).posted, null);
  reset();
});
