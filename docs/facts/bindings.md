# The bindings

## The model's crate and the libraries

`pitboard-model` holds the model and everything the bindings export, and `pitboard-ffi` is
`pub use pitboard_model::*;` built as the static and the dynamic library the apps link.
Cargo builds a dependency as every crate type it names and has no way to ask for its `lib`
alone (rust-lang/cargo#11232, open when read on 28 October 2026), so the Linux app depends on
the model, which names none.

Measured on 28 October 2026 with Rust 1.98.1, UniFFI 0.31.2 and uniffi-bindgen-cs
v0.11.1+v0.31.0, on the debug libraries built at 6238d370, before the split, and after it:

- `libpitboard_ffi.dylib` and `libpitboard_ffi.a`, on macOS 27.0 (arm64) with `nm -gU`,
  export 126 `uniffi_pitboard_model_` symbols, 53 `ffi_pitboard_model_` and 182
  `UNIFFI_META_`: the 361 names they exported before, with `pitboard_ffi` read as
  `pitboard_model`. So does `pitboard_ffi.dll`, built before and after with cargo-xwin 0.23.0
  for `x86_64-pc-windows-msvc` and read with LLVM 22.1.8's `llvm-readobj --coff-exports`.
  `libpitboard_ffi.so`, built after with cargo-zigbuild 0.24.0 for
  `x86_64-unknown-linux-gnu` and read with LLVM 22.1.8's `llvm-nm -D`, exports the same 361
  as the Mac's libraries. Built in Debian 13 for `aarch64-unknown-linux-gnu`, it exports 126,
  53 and 182 by GNU `nm -D` 2.44.
- The bindings' namespace is the name of the crate that declares the exports,
  `pitboard_model`. The Swift and C# generated after are what was generated before but for
  the names made from it: the C symbols, `uniffiEnsurePitboardModelInitialized`, the C#
  class `PitboardModelMethods` and its file `pitboard_model.cs`. And for the 56 checksums,
  every one different, since a checksum covers the call's module
  ([The C# bindings](#the-c-bindings)).
- The Swift generator, given no `--config`, as `build-xcframework.sh` runs it, reads
  `crates/pitboard-model/uniffi.toml`, the file of the crate that declares the exports: it
  still writes `PitboardFFI.h`, `module.modulemap` and `PitboardBindings.swift`. The C#
  generated with that file is in `Pitboard.Core` and loads `pitboard_ffi`, as its
  `cdylib_name` says.

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
  checksum of every export, the model's constructor and methods and `Listener.changed` among
  them, and hand the library each foreign trait's table of calls.
- A trait an app implements is an interface of the trait's own name, `Listener`,
  `OtherApps`, `Notifier` and `LocalTime`, `ModelListener`, `AppControl` and `Notifications`
  before 29 October 2026. Its error is an exception named for it, `PlatformException`, whose
  variant the app throws. An `Option<String>` it gives back is a `string?`.
- An `Option<Arc<dyn OtherApps>>` the app passes is an `OtherApps?` in C# and an
  `OtherApps?` in Swift, and a `null` or `nil` reaches Rust as `None`. Read in what the
  generators wrote from the debug library on 29 October 2026, and measured by
  `ModelTests.AModelOverNoOtherAppsSwitchesWithoutAskingToQuitChatGpt` and by the Swift tests'
  `fixtureModel`, which passes `nil`.
- Each call the bindings write for a trait catches a `PlatformException` and hands Rust its
  variant, and catches any other exception and hands Rust its message, which
  `From<UnexpectedUniFFICallbackError>` makes `PlatformError::Failed`. So nothing an app's
  code throws goes past Rust. Read in the generated `AppControl` calls.
  `ModelTests.APlatformExceptionFromTheAppsOtherAppsStopsTheSwitch`, then named for
  `AppControl`, measured the first on 13 October 2026, in CI on Linux with the .NET SDK
  10.0.401 and the release library built with the `fixture` feature: `RequestQuit` throwing
  `PlatformException.Failed` stopped the switch as an error from `request_quit` does, and
  `Running` was not asked again.
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
  `BridgingHeaderTemplate.h` naming each field through `var_name`. `OtherApps`' method that
  opens an app again is `reopen` for that reason.
- A type the bindings export that `PitboardApp` also declares is the app's own inside
  `PitboardApp`, and ambiguous in a module that imports both, as the app's tests do: with
  the bindings' `AppControl` beside the app's protocol of that name, `swift test` stopped at
  "'AppControl' is ambiguous for type lookup in this context" in `Fixtures.swift`. The app
  implements the bindings' trait, now `OtherApps`, as `MacApps`, and declares no type of
  that name of its own.
- A type the bindings export is declared in the same Swift package as the app, so making
  one conform to a protocol, as `Sheet` to `Identifiable` for a sheet's binding, takes no
  `@retroactive`: Swift 6.4 refuses it there, "'retroactive' attribute does not apply;
  'Sheet' is declared in the same package". Measured on 6 October 2026.
- `MainActor.assumeIsolated` traps off the main thread, so `ModelListening`'s hop is
  tested by where it runs: with the snapshot queued on a global queue in place of the main
  one, the test of the listener's order stopped with signal 5 rather than failing an
  expectation. Measured on 6 October 2026.
