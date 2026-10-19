# Probe helpers

Scripts `.github/scripts/runner-facts.ps1` runs beside `pitboard-probe` on the Windows
runners, as the job's user and as the throwaway users it makes. They touch only the scratch
folder they are given, and `bun-secrets.mjs` only its `pitboard-citest-bun` items, which it
writes and the row deletes. They load no website, sign in to nothing, and print counts,
error codes and redacted paths, never what they read.

The runner gets Bun from its release zip, checked against the SHA-256 `windows-facts.yml`
pins, and `proper-lockfile` from `bun.lock` with `bun install --frozen-lockfile
--ignore-scripts`.

- `reader-loop.mjs <scratch> [seconds] [hold-ms]`: the Bun or Node reader of the replace
  loop (C4). It waits for `pitboard-probe replace-loop`'s target, then opens it by path,
  reads it whole, checks the record and closes it, until the loop ends, and counts whole,
  torn, empty and missing reads.
- `proper-lockfile-loop.mjs <scratch> <hold|check> [seconds] [write|refresh]`:
  proper-lockfile's lock with the options Claude Code passes for its write lock or its
  refresh lock (C5). Run `hold` against `pitboard-probe lock --mode check`, then `check`
  against `pitboard-probe lock --mode hold`, with the same timing.
- `os-homedir.mjs <profile>`: `os.homedir()`, `os.userInfo().homedir` and the home-shaped
  variables, with the account's profile folder, given by the script, shown as `<profile>` and
  the account name as `<user>` (B1, B2).
- `bun-mode.mjs <scratch>`: a file made as Claude Code makes `.credentials.json`, with mode
  0o600 and `chmod`, for `pitboard-probe acl --describe` to read (C1).
- `bun-secrets.mjs <set|delete>`: `Bun.secrets` items as Claude Code writes its login and its
  pieces, under the `pitboard-citest-bun` service, for `pitboard-probe credman-item` to read
  and delete (G3). It refuses outside a GitHub-hosted runner.
- `fd3-read.mjs`: reads descriptor 3 as Claude Code 2.1.300 reads its OAuth token descriptor
  (socket gate: a direct `readSync` only when `fstat` calls it a socket, otherwise
  `/proc/self/fd/<fd>`, with the direct read as a fallback only on EACCES or EPERM there), and
  prints which branch it took, what fstat saw and whether it got the bytes. `pitboard-probe
  fd-handover` hands it a dummy on descriptor 3 through the C runtime's inheritance table, as a
  pipe or a socket (RF-WW1); it reads nothing of its own.
- `remote-token.mjs <dummy>`: resolves Claude Code's remote token path,
  `/home/claude/.claude/remote/.oauth_token`, from the working folder and reads it as Claude
  Code 2.1.303 does, then prints the drives involved and whether it read the dummy the row
  wrote there (RF-T7).
