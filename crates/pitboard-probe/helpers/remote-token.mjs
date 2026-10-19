// Usage: bun remote-token.mjs <dummy>
//
// RF-T7: Claude Code's remote token path names no drive. This resolves it from the working
// folder and reads it as Claude Code 2.1.303's well-known file reader does: opened for
// reading, a regular file alone, trimmed. It prints the drive and whether what it read is the
// dummy it was handed, never what it read.

import { closeSync, fstatSync, openSync, readSync } from "node:fs";
import { resolve } from "node:path";

const dummy = process.argv[2];
if (!dummy) {
  console.error("usage: remote-token.mjs <the dummy the row wrote>");
  process.exit(2);
}
const path = "/home/claude/.claude/remote/.oauth_token";
const drive = (p) => (/^[A-Za-z]:/.test(p) ? p.slice(0, 2).toUpperCase() : null);
const out = {
  runtime: typeof Bun !== "undefined" ? `bun ${Bun.version}` : `node ${process.version}`,
  working_drive: drive(process.cwd()),
  resolved_drive: drive(resolve(path)),
  resolved_below_the_drive: resolve(path).slice(2),
};
try {
  const fd = openSync(path, "r");
  try {
    out.regular_file = fstatSync(fd).isFile();
    const buffer = Buffer.alloc(4096);
    const read = readSync(fd, buffer, 0, buffer.length, null);
    out.bytes = read;
    out.is_the_dummy = buffer.subarray(0, read).toString("utf8").trim() === dummy;
  } finally {
    closeSync(fd);
  }
} catch (e) {
  out.error = e.code ?? String(e);
}
console.log(JSON.stringify(out, null, 2));
