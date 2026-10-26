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
    posted: p && [p.title, p.subtitle, p.body, p.switchTo?.label ?? null],
  };
};

test("at the page's start the row's moment is the static figure's: 05:53, and Notify Me's heads-up", () => {
  reset();
  assert.deepEqual(said(model.get()), {
    at: menuBarClock(clock.warned),
    item: "personal 95%",
    posted: [headsUp.title, headsUp.tool, headsUp.body, "work"],
  });
});

test("a lower share comes sooner at the same pace, and one already read comes at once, the 5-hour first", () => {
  reset();
  setShare(90);
  assert.deepEqual(said(model.get()), {
    at: "Wed 14 Oct 03:50",
    item: "personal 90%",
    posted: [
      "personal has used 90% of its weekly limit",
      "Claude Code",
      "work has 60% of its own left.",
      "work",
    ],
  });
  setShare(70);
  assert.deepEqual(said(model.get()).posted?.[0], "personal has used 73% of its weekly limit");
  assert.equal(hm(noticeMoment(model.get())!.at), hm(clock.menuBar));
  setShare(55);
  assert.deepEqual(said(model.get()).posted, [
    "personal has used 59% of its 5-hour limit",
    "Claude Code",
    "work has 88% of its own left.",
    "work",
  ]);
});

test("under Switch Automatically the row shows the switch, and under Do Nothing nothing", () => {
  reset();
  setChoice("switch");
  assert.deepEqual(said(model.get()), {
    at: menuBarClock(clock.warned),
    item: "work 40%",
    posted: [
      "Switched Claude Code to work",
      null,
      "personal has used 95% of its weekly limit. Sessions already running follow within 33 seconds.",
      null,
    ],
  });
  setChoice("nothing");
  assert.equal(said(model.get()).posted, null);
});

test("the moment names accounts as the page does, and a quit app posts nothing", () => {
  reset();
  rename("claude", "personal", "<b>me");
  assert.equal(said(model.get()).posted?.[0], "<b>me has used 95% of its weekly limit");
  forget("claude", "work");
  assert.deepEqual(said(model.get()).posted?.slice(2), ["Resets in 5d 8h.", null]);
  reset();
  quit();
  assert.equal(said(model.get()).posted, null);
  reset();
});
