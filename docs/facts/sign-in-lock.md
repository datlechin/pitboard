# One sign-in at a time

The core's one sign-in at a time is a lock on `signin.lock`, taken with `File::try_lock`,
which is `flock`.

- Measured on macOS 27.0 on 5 October 2026, with Rust 1.98.1: a lock taken with
  `File::try_lock` can stay held after its `File` is dropped, though std opens every file so
  that a program started after does not keep it. A process another thread is starting holds
  a copy of the descriptor until it runs its program, or ends. Dropped and taken again at
  once, in 3000 rounds each, the lock was still held 437 to 1042 times, for up to 5.5 ms,
  while another thread started processes the way std forks and execs, which it does for a
  program named bare with `PATH` set; 8 to 15 times, for up to 77 µs, where std uses
  `posix_spawn`; and never with no process started. Through the core's own sign-in, one
  started right after a cancel in the same home was refused as one already waiting 64 times
  in 100 while another thread started a program it could not find, and never in 100 while
  one started `/usr/bin/true` or none started anything.
- On Linux, a thread that starts a process while another holds a program's file open for
  writing gives the child a copy of that descriptor, and running the file then fails with
  ETXTBSY, "Text file busy", until that child has started its own program. CI met it in run
  36457687750, in a test that ran a fake `codex` it had just written.
- Measured on 6 October 2026, with Rust 1.98.1, on macOS 27.0 and on Debian 13 with glibc
  2.41 in Docker, both on arm64, by a program of the measurement's own, run three times on
  each: a lock taken on a file, let go of, and at once taken again on the file opened again,
  in 20,000 rounds 200 µs apart, while one other thread started one process after another
  and waited for each. Let go of by closing the file, the lock was still held:
  - with nothing started, never;
  - while the other thread named its program bare with `PATH` set, which std forks and execs
    for: found nowhere, 10 to 13 times on macOS, for up to 781 µs, and 71 to 97 times on
    Linux, for up to 231 µs; found, 4 to 6 times, for up to 731 µs, and 44 to 55, for up to
    269 µs;
  - while it named its program by its path, which std starts with `posix_spawn`: found, 7 to
    13 times on macOS, for up to 61 µs, and 54 to 60 on Linux, for up to 141 µs; where
    nothing is, 166 to 200 times, for up to 151 µs, and 151 to 171, for up to 109 µs,
    starting more since each fails at once.

  Let go of with `File::unlock` before the file was closed, it was never held, however the
  other thread started its processes, in any run on either system. Named by its path, a
  process holds the copy for less time on macOS and for about as long on Linux, but holds
  it: starting a program by the path it was found at does not close the window, and letting
  go of the lock by name does. So a `SignIn` lets go of its lock with `File::unlock` as it
  is dropped, after clearing its directory, and `WatchedSignIn::cancel` returns once it is
  free. The core already starts every tool it found by the path it found it at. It names a
  tool bare, with `PATH` set, only where it found none, in `provider::command`, and
  `ready_to_sign_in` refuses such a sign-in before it gets there, so that runs only for a
  program taken away between the two. It is left as it is.
- Cancel and then Sign In at once, through the app's model over the real core, measured on
  6 October 2026 on the same two systems with the model's tests. Before the model
  waited for the sign-in cancelled and the lock was let go of by name,
  `a_sign_in_asked_for_at_once_after_a_cancel_starts_once_that_one_has_stopped`, with a
  stand-in for `claude`, failed in 13 of 50 runs alone and in 30 of 40 runs of the whole
  suite on macOS. On Linux it failed in 4 of 100 runs of the whole suite, and in 36 of 200
  with four suites running at once, where the fixture's
  `accounts_are_added_through_each_tools_sign_in` failed 5 times in all as CI met it (run
  37437790666) and `a_sign_in_asked_for_at_once_after_a_cancel_asks_for_the_code` 11. Each
  time the second sign-in was refused as `sign_in_in_progress`. After, none of them failed
  that way: in 50 runs alone and 140 of the whole suite on macOS, and in 100 of the whole
  suite and 200 with four at once on Linux. In 2 of the 140 on macOS the fixture's test
  failed otherwise, in its Codex sign-in, which cancels nothing: a look for changes that
  answered after the sign-in had finished took its enrolment for a change made elsewhere,
  so the read after the sign-in was dropped, and nothing said when the accounts were read.
- A program a tool started can hold the tool's output open once the tool has been stopped
  and waited for, as `codex login` does once npm's `codex` launcher is killed
  (`codex_install_places`). Measured on 6 October 2026 on the same two systems with
  `a_sign_in_after_a_cancel_waits_for_no_output_the_tool_stopped_left_open`, whose stand-in
  for `claude` starts a shell that holds its output open until the test lets it go, then
  Cancel and Sign In. While the model waited for the cancelled sign-in's thread, the second
  sign-in asked for its code only once that output had closed: let go after 3 s, in 3.03 to
  3.07 s on macOS and 3.02 to 3.07 s on Linux; after 6 s, in 6.05 to 6.06 s on macOS; never
  let go, not within 20 s on either. Taking the stop's own word that it had stopped the
  tool, it asked in 6.8 to 9.7 ms on macOS and 1.4 to 1.9 ms on Linux, 20 runs each, with
  that output still open.
