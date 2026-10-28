# The app model

## A look and the app's own changes

Measured on 6 October 2026 on macOS 27.0 (arm64), with Rust 1.98.1 and the model's
tests, and read in the macOS app's Swift at 0.7.0 (277539b).

- A look reads when the account index was last written on the lane of reads, while a change
  the app makes writes it on the lane of changes, or a sign-in's enrolment on that sign-in's
  own thread, and their answers reach the model in the order they are sent. A look that read
  the index after a change wrote it, and whose answer came after the change's, was compared
  once the change had taken the ticket of the read it asked for: it counted a change made
  elsewhere, and that read was dropped. The fixture's
  `accounts_are_added_through_each_tools_sign_in`, which signs a Codex account in and
  cancels nothing, met it in 2 of 140 runs of the whole suite, with nothing said of when the
  accounts were read.
- With looks back to back, each asked for as the last one answered, a rename, a forget,
  naming the login in use and a sign-in met it in none of 10 runs: the core writes its log
  of changes after the index and before the change answers, so the first look to read the
  change's write answered first. Held on the lane of reads instead, before it reads
  anything, until the change has answered (`lanes::NextLook`), a look reads the index after
  the change and answers after it every time. Against the model before it held the index for
  every change of its own, `a_rename_is_read_after_it_whatever_the_poll_finds_meanwhile`,
  `adding_the_login_in_use_is_read_after_it_whatever_the_poll_finds_meanwhile` and
  `a_sign_in_is_read_after_it_enrols_whatever_the_poll_finds_meanwhile` failed in 10 of 10
  runs, each with nothing read after the change; after, in none of 50. The same again on
  24 October 2026 (macOS 27.0.1, Rust 1.98.1), when the tests moved the hold there from a
  read held at the lock usage readings are written under.
- Where the watch on Pitboard's directory asks the look, as both apps take it, the same holds
  for a look a write to the index asked before the change, which
  `a_rename_is_read_after_it_whatever_the_watchs_look_finds_meanwhile`,
  `adding_the_login_in_use_is_read_after_it_whatever_the_watchs_look_finds_meanwhile` and
  `a_sign_in_is_read_after_it_enrols_whatever_the_watchs_look_finds_meanwhile` hold. On
  29 October 2026 (macOS 27.0.1, Rust 1.98.1), with `State`'s check that a change of the app's
  own holds the index taken out of a look's answer, all six failed, each waiting 30 seconds
  for a read after the change that never came; with it, all six passed in 5 of 5 runs.
- The Swift model at 0.7.0 had the same race. PitboardService.swift asked `changedAt` on its
  queue of reads, and enrolled, renamed, forgot, renewed and gave up on an interrupted switch
  on its queue of changes. AppModel.swift's `enrol`, `rename`, `forget`,
  `abandonStuckSwitch` and a sign-in's `watch` counted the change in `changesSeen` and then
  read, the read keeping the count it started with, and `noticeOtherChanges` counted any move
  of the index's time as a change made elsewhere unless `switching` was set. So a look whose
  `changedAt` came back after the change's write, and whose comparison ran once the read had
  taken its count, dropped the read and left `updatedAt` nil; one whose read of what is known
  landed before a rename's own code ran put away what the switch to that account had said.

## The model's timers

Read on 5 October 2026 in Rust 1.98.1's standard library and in the macOS SDK of Xcode 27.1,
with Swift 6.4.

- The model's timers run on `pitboard_core::host::awake`, the time the machine has been
  awake. On macOS and Linux that is `std::time::Instant`, which reads `CLOCK_UPTIME_RAW` on
  Apple's systems, which macOS's `clock_gettime(3)` says does not increment while the system
  is asleep, and `CLOCK_MONOTONIC` on Linux, which "does not count time that the system is
  suspended" (Linux man-pages 6.19, `clock_gettime(2)`, read 15 October 2026).
- The Swift model's loops slept with `Task.sleep(for:)`, whose clock is `.continuous` unless
  it is given another, and `ContinuousClock` does not stop while the system is asleep. After
  a long sleep its five-minute read came as the machine woke, where the model's comes up to
  five minutes of waking time later. An app sends `Intent::Woke` as the machine wakes, which
  reads at once: the macOS app sends it on `NSWorkspace.didWakeNotification`, where the
  Swift model read.
- The 30 seconds an app is given to quit run on time awake too, where the Swift model's ran
  on `ContinuousClock`: a machine put to sleep while an app is asked to quit gives it its
  30 seconds of waking time, where the Swift gave it none once the machine woke.
- The look whether to switch Claude Code by itself every 30 seconds, and the wait after a
  refused switch, run on time awake too, so they count waking time. After a sleep the read
  that `Intent::Woke` asks for is followed by a look at once where it lands, and one that
  fails, as before the network is back, leaves the next look to its timer. A refusal's
  wait goes on where it stopped.
- On Windows the standard library's `Instant` reads `QueryPerformanceCounter` (Rust 1.98.1's
  `library/std/src/sys/time/windows.rs`), which Microsoft documents as counting "the time
  when the machine was in a sleep state such as standby, hibernate, or connected standby"
  ([Acquiring high-resolution time stamps](https://learn.microsoft.com/windows/win32/sysinfo/acquiring-high-resolution-time-stamps)).
  There the timers read `QueryUnbiasedInterruptTimePrecise`, whose count "does not include
  time the system spends in sleep or hibernation", in units of 100 nanoseconds
  ([QueryUnbiasedInterruptTimePrecise](https://learn.microsoft.com/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryunbiasedinterrupttimeprecise)),
  read 15 October 2026. With `Instant` there, an app asked to quit just before a sleep would
  have had its 30 seconds used up by the time the machine woke. Microsoft's contract: no
  machine has been put to sleep during a quit yet.

## Watching Pitboard's directory

Measured on 29 October 2026 with notify 8.2.1, watching a folder alone, not what is below
it, through the core's own `atomic::write` and `home::remove_all`, in a test run once and not
kept. Read the same day in notify 8.2.1's source.

- On macOS 27.0.1 (26A434, arm64), on APFS, FSEvents named each file by its path with every
  link resolved: `/private/var/folders/…` for a folder watched as `/var/folders/…`. A replace
  of `state.json` came as six events for the temporary, in two batches, and five for
  `state.json`, the flags of each split into `Create`, `Modify(Name)`, `Modify(Data)` and
  `Modify(Metadata)`, 11 to 24 ms after the write began. A replace of `usage.json` came 12 ms
  after. A read of `state.json` came as nothing. The folder removed whole came as three events
  for the folder itself, `Create`, `Remove` and `Modify(Metadata)`, 12 ms after, and none for
  the files in it; the folder had been made 300 ms before the watch began.
- In Docker Desktop 29.8.2 (Linux 7.0.14-linuxkit, aarch64), in `pitboard-linux-refactor:trixie`
  (sha256:0e0944571e15…, from `rust:1.98.1-slim-trixie@sha256:4cd8294…`, Debian 13.7), alike
  on the named volume's ext4, on a tmpfs and on the container's overlayfs, inotify told a
  replace of `state.json` as `Create`, `Modify(Data)` and `Access(Close(Write))` of the
  temporary, then `Modify(Name(From))` of it, `Modify(Name(To))` of `state.json` and
  `Modify(Name(Both))` of the two, 0.7 to 12 ms after the write began, the flush included. Each
  open came as `Access(Open)`: of the folder, by the write's sweep of temporaries and its flush
  of the folder, and of `state.json`, by a read. The folder removed whole came as
  `Remove(File)` of each file, then `Remove(Folder)` of the folder itself, within 1 ms.
- So a watch takes a file by its name alone, takes no `Access` for a change, and ends at a
  `Remove` or a `Modify(Name)` of the folder itself (`host/watch.rs`,
  `a_named_file_is_heard_by_its_name_alone`, `a_read_is_not_a_write`,
  `the_folder_going_ends_the_watch`).
- notify's Windows backend tells no event as a watch ends: where the folder is gone, it and
  every other error ReadDirectoryChangesW answers with stop the watch in silence
  (`src/windows.rs:339-368`). Its server thread sends `MetaEvent::SingleWatchComplete` once it
  has closed the folder's handle, to the channel `ReadDirectoryChangesWatcher::create` is
  given, which `host/windows/watch.rs` reads as the end. It reads the entries of a buffer
  without looking at the length the system gave (`_bytes_written`, `:333`, `:381-385`), so
  what an overflow comes to is not known
  ([Not measured](not-measured.md#watching-pitboards-directory)).
