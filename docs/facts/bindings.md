# The bindings

## The C# bindings

Read in the C# that uniffi-bindgen-cs v0.11.0+v0.31.0 generated from `pitboard-ffi`, and
measured by `apps/windows/Pitboard.Core.Tests` against the debug library, on 5 October 2026.
Read again on 15 October 2026 in what v0.11.1+v0.31.0 generates, which differs only in how a
string crosses: it is read and written in the Rust buffer's own memory, with no `byte[]`
between.

- An exported object's class frees its Rust object from a finalizer: `~PitboardModel()`
  calls `Destroy()`. So .NET's finalizer thread can be the one that drops a
  `PitboardModel`, and its `Drop` waits on nothing.
- The first call loads the library, and before it answers, the bindings compare the
  checksum of every export, the model's constructor and methods and
  `ModelListener.changed` among them, and hand the library each foreign trait's table of
  calls.
- A trait an app implements is an interface of the trait's own name, `ModelListener`,
  `AppControl`, `Notifications` and `LocalTime`. Its error is an exception named for it,
  `PlatformException`, whose variant the app throws. An `Option<String>` it gives back is a
  `string?`.
- Each call the bindings write for a trait catches a `PlatformException` and hands Rust its
  variant, and catches any other exception and hands Rust its message, which
  `From<UnexpectedUniFFICallbackError>` makes `PlatformError::Failed`. So nothing an app's
  code throws goes past Rust. Read in the generated `AppControl` calls.
  `ModelTests.APlatformExceptionFromTheAppsAppControlStopsTheSwitch` measured the first on
  13 October 2026, in CI on Linux with the .NET SDK 10.0.401 and the release library built
  with the `fixture` feature: `RequestQuit` throwing `PlatformException.Failed` stopped the
  switch as an error from `request_quit` does, and `Running` was not asked again.
- A record holds a list as an array, and a C# record compares arrays by reference, so two
  snapshots read alike are not equal. `ModelTests.ASnapshotCarriesWhatWasRead` measures it.
  Two records C# makes with an empty collection expression, `[]`, for each list do compare
  equal: the compiler gives them one empty array, so an app's own test of that kind proves
  nothing about lists. `ModelTests.ASnapshotSaysWhatIsKnownOfTheMachine` measures it, and
  measured it with the .NET SDK 10.0.401 on 6 October 2026.
- A call's checksum is taken over what UniFFI records of it: its module, object and name,
  its arguments, the types it takes, gives and throws, and its doc comment. A record type is
  recorded by its module and name alone, so no checksum covers a record's fields. Read in
  `uniffi_macros` 0.31.2, `fnsig.rs` and `record.rs`.

On Windows, CI's `csharp-windows` job measured on windows-2025 and windows-11-arm, in run
38683164949 on 15 October 2026, where both passed:

- The C# generated from the release `pitboard_ffi.dll`, built as a 0.x release is and
  opened with its fixtures, is the Linux job's file byte for byte on both images. The DLL
  keeps through the release profile's strip the exports the generator reads, where Linux's
  shared library does not.
- A test program published self-contained for the image's machine loads either DLL as a
  fresh standard user, and its calls answer. A model over the gated DLL reads
  `windows_not_released` and leaves each home it was given empty.

## The Swift bindings

Measured with `build-xcframework.sh` and `swift test --package-path apps/macos`, with the
Swift UniFFI 0.31.2 generates and the Swift 6.4 of Xcode 27.1, on 5 October 2026.

- A method of a trait an app implements may not have a name on the generator's list of
  Swift keywords, which has `open` though Swift takes `open` as a method's name, as
  `PitboardApp`'s own `AppControl.open` was. The generator puts such a name in backticks, and
  writes the C header's table of calls through the same filter, so the header has
  `` UniffiCallbackInterfaceAppControlMethod2 _Nonnull `open`; ``, which clang refuses
  ("expected member name or ';' after declaration specifiers"), and the bindings do not
  build. Read in `uniffi_bindgen` 0.31.2: the list in `bindings/swift/gen_swift/mod.rs`, and
  `BridgingHeaderTemplate.h` naming each field through `var_name`. `AppControl`'s method that
  opens an app again is `reopen` for that reason.
- A type the bindings export that `PitboardApp` also declares is the app's own inside
  `PitboardApp`, and ambiguous in a module that imports both, as the app's tests do: with
  the bindings' `AppControl` beside the app's protocol of that name, `swift test` stopped at
  "'AppControl' is ambiguous for type lookup in this context" in `Fixtures.swift`. The app
  has implemented the bindings' trait, as `MacAppControl`, since it runs on the model, and
  declares no `AppControl` of its own.
- A type the bindings export is declared in the same Swift package as the app, so making
  one conform to a protocol, as `Sheet` to `Identifiable` for a sheet's binding, takes no
  `@retroactive`: Swift 6.4 refuses it there, "'retroactive' attribute does not apply;
  'Sheet' is declared in the same package". Measured on 6 October 2026.
- `MainActor.assumeIsolated` traps off the main thread, so `ModelListening`'s hop is
  tested by where it runs: with the snapshot queued on a global queue in place of the main
  one, the test of the listener's order stopped with signal 5 rather than failing an
  expectation. Measured on 6 October 2026.
