# The app model

## A look and the app's own changes

Measured on 6 October 2026 on macOS 27.0 (arm64), with Rust 1.98.1 and `pitboard-ffi`'s
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
  `naming_the_login_in_use_is_read_after_it_whatever_the_poll_finds_meanwhile` and
  `a_sign_in_is_read_after_it_enrols_whatever_the_poll_finds_meanwhile` failed in 10 of 10
  runs, each with nothing read after the change; after, in none of 50. The same again on
  24 October 2026 (macOS 27.0.1, Rust 1.98.1), when the tests moved the hold there from a
  read held at the lock usage readings are written under.
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
