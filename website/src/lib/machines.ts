// The command line each system's terminal runs, from what crates/pitboard's tests keep in
// src/snapshots: its command tree and help on Windows and elsewhere, doctor on a healthy machine of
// each system, and each shell's completion script, read only when a terminal prints one.
import tree from "../../../crates/pitboard/src/snapshots/pitboard__tests__command_tree.snap?raw";
import windowsTree from "../../../crates/pitboard/src/snapshots/pitboard__tests__command_tree_on_windows.snap?raw";
import help from "../../../crates/pitboard/src/snapshots/pitboard__tests__help_in_80_columns.snap?raw";
import windowsHelp from "../../../crates/pitboard/src/snapshots/pitboard__tests__help_in_80_columns_on_windows.snap?raw";
import doctorMac from "../../../crates/pitboard/src/render/snapshots/pitboard__render__doctor__tests__doctor_on_a_healthy_mac.snap?raw";
import doctorPc from "../../../crates/pitboard/src/render/snapshots/pitboard__render__doctor__tests__doctor_on_a_healthy_pc.snap?raw";
import doctorLinux from "../../../crates/pitboard/src/render/snapshots/pitboard__render__doctor__tests__doctor_on_a_healthy_linux.snap?raw";
import { differing, readHelps, readTree, snapshot, swapped } from "./clap.ts";
import { checks, type Machine } from "./cli.ts";
import { rowsOf } from "./doctor.ts";
import type { Platform } from "./platform.ts";

const kept = {
  bash: () =>
    import("../../../crates/pitboard/src/snapshots/pitboard__tests__completions_bash.snap?raw"),
  zsh: () =>
    import("../../../crates/pitboard/src/snapshots/pitboard__tests__completions_zsh.snap?raw"),
  fish: () =>
    import("../../../crates/pitboard/src/snapshots/pitboard__tests__completions_fish.snap?raw"),
  elvish: () =>
    import("../../../crates/pitboard/src/snapshots/pitboard__tests__completions_elvish.snap?raw"),
  powershell: () =>
    import("../../../crates/pitboard/src/snapshots/pitboard__tests__completions_powershell_on_windows.snap?raw"),
};

export function machine(platform: Platform, version: string): Machine {
  const windows = platform === "windows";
  const off = readTree(tree);
  const on = readTree(windowsTree);
  const pairs = differing(off, on);
  return {
    platform,
    version,
    grammar: { tree: windows ? on : off, helps: readHelps(windows ? windowsHelp : help), checks },
    doctor: rowsOf({ macos: doctorMac, windows: doctorPc, linux: doctorLinux }[platform]),
    async script(shell) {
      const load = kept[shell as keyof typeof kept];
      if (!load) return null;
      const text = snapshot((await load()).default);
      // bash's script names no about, so every system's is the one kept
      if (shell === "bash") return text;
      if (shell === "powershell") return windows ? text : swapped(text, pairs, false);
      // zsh's, fish's and elvish's are kept as macOS and Linux write them
      return windows ? swapped(text, pairs, true) : text;
    },
  };
}
