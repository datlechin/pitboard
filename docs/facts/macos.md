# macOS

Measured on macOS 26 and written down on 22 September 2026. Before changing code that
depends on the `security -i` limit or the Security framework's cost, measure them again on
a scratch item.

- `security -i` reads at most 4097 bytes of command from standard input, with no line
  continuation. Its interactive `-w` prompt takes 128 bytes.
- A keychain item written in process, through the Security framework, stays slow to read.
  Every later read of it by `security` takes about a second instead of 0.01 seconds. On a
  scratch item, reads went from 0.01 seconds to 20.55, then settled around 0.8.
- Claude Code reads its login at every miss of its cache (`credential_cache`), so writing its
  item through the framework would slow Claude Code for good. Pitboard writes a large login on the argument line
  instead, where `ps` can see it for the length of one call.
- On throwaway items, a read in process by a program the item does not trust adds that
  program to the item's access list, and a write in process replaces its partition list.
  Every read by `/usr/bin/security` after that takes 1 to 3 seconds instead of 0.02, or waits
  on a keychain password dialog in a desktop session. A write through `security -U` changes
  only the item's modification time.
- Undoing the framework's change needs `security set-key-partition-list`, which asks for the
  keychain password.
- APFS keeps a directory's mtime in nanoseconds, but not exactly: setting one and reading it
  straight back gives a value 18 to 60 nanoseconds away. A lock that remembered the value it
  asked for would abandon every switch, so `lock.rs` keeps the value read back.
- Pitboard lists its parked items with `security dump-keychain` without `-d`. It never
  prompts, and emits attributes only, no secret of any item. It exits 0 in 0.06 seconds
  against a keychain of 362 items.
- Reads after a `dump-keychain` take the usual 0.016 seconds, so listing has none of the
  access-list cost of an in-process read. Each service name is on a line of the form
  `"svce"<blob>="<name>"`.
- A lock taken with `flock` outlives the file it was taken on while another thread is
  starting a process, here as on Linux: [One sign-in at a time](sign-in-lock.md).
- `ps` gives what a process was started as: a `codex` started from a shell by its bare name
  lists as `codex`, and one started by its path lists the path (1 October 2026). So Pitboard
  tells the kinds of Codex apart by the folders in that path, and takes a bare name for a
  `codex` session.
