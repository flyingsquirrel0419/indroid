<div align="center">

# indroid

**Real Android 13 on a stock iPhone. No JIT, no jailbreak.**

Open the app and Android boots. indroid is an AArch64 interpreter plus a
Linux-ABI kernel in Rust that runs the unmodified Android 13 system image.
It also contains a smaller Dalvik engine that runs single APKs on a computer
or as native UIKit views.

[![CI](https://github.com/flyingsquirrel0419/indroid/actions/workflows/ci.yml/badge.svg)](https://github.com/flyingsquirrel0419/indroid/actions/workflows/ci.yml)
[![iOS IPA](https://github.com/flyingsquirrel0419/indroid/actions/workflows/ios.yml/badge.svg)](https://github.com/flyingsquirrel0419/indroid/actions/workflows/ios.yml)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)

</div>

---

## Real Android 13 (neo)

neo runs the unmodified Android 13 arm64 system image (Google's emulator
image, downloaded from the public Android SDK repository) in user space:
an AArch64 interpreter with a decoded block cache (`crates/a64`, ~55 M
instructions/s) and a kernel that implements the Linux ABI Android needs
(`crates/kernel`: processes, threads, futexes, signals, mmap, binder, ashmem,
epoll, netlink, bpf, virtio-gpu, ...). `init` starts ~120 services, zygote
forks `system_server`, and SurfaceFlinger, SystemUI and Launcher3 render with
ANGLE on Vulkan through gfxstream:

<p align="center">
  <img src="docs/img/neo-bootanim.png" width="240" alt="Android 13 boot animation on indroid neo">
  <img src="docs/img/neo-launcher.png" width="240" alt="Android 13 launcher home screen on indroid neo">
</p>

No code is generated at run time, so it fits iOS's rules. Status: the home
screen above is from a Linux host (gfxstream on lavapipe). On iPhone the app
downloads, installs and boots the image with console and logcat output; the
on-device display (gfxstream on MoltenVK) and touch input are the next
milestones, then speed (build-time AOT of hot ROM code). Boot takes ~160 s on
the interpreter today. Details: [docs/NEO.md](docs/NEO.md).

## The Dalvik engine: run a single APK

```text
$ indroid run examples/todo.apk
[indroid] loaded dev.indroid.todo in 1.0ms
== Todo (Android) ==
#1 FrameLayout
  #2 LinearLayout
    #3 TextView "Todo (Android)"
    #4 TextView "2 open · running for 0s"
    #5 LinearLayout
      #6 EditText "" hint="What needs doing?"
      #7 Button "Add"
    #8 LinearLayout
      #9 Spinner
      #10 CheckBox "Urgent only" checked=false
    #12 ScrollView
        #17 TextView "🔥 Try indroid on an iPhone" [clickable]
        #18 TextView "• Star the repo" [clickable]
    #13 Button "About"
[indroid] launch 7.6ms, 143 classes, 1002 objects, heap 61 KB
```

That is an **unmodified APK** built with the stock Android SDK (`aapt2` + `d8`):
its `classes.dex` is interpreted, its `resources.arsc` and binary layout XML are
parsed, and its `Activity` goes through the real lifecycle. On iOS the same view
tree becomes native `UILabel`s, `UIButton`s and `UITextField`s placed at the
frames computed by Android's own measure/layout algorithm.

**New in 0.2: real AndroidX apps.** The Android Studio *Empty Views Activity*
template, built by Gradle with R8 (`AppCompatActivity`, Material 3 theme,
`MaterialButton`, `ConstraintLayout`), runs unmodified: the AndroidX and
Material Components code inside the APK is executed as-is, themes and styles
resolve through `resources.arsc`, and `ConstraintLayout` from the APK lays the
screen out. Kotlin apps work too (the Kotlin stdlib is just more bytecode).

## Why this is different

iOS does not let ordinary apps create executable memory. Every Android-on-iOS
attempt so far either emulates a whole ARM machine and needs JIT (StikDebug,
debugger tricks, iOS-version roulette) or is not open source.

indroid takes the other road:

The APK runtime takes a different road from emulators:

| | Full-system emulators | **indroid APK runtime** |
|---|---|---|
| Needs JIT / debugger attach | usually | **never**: only signed code runs; bytecode is data |
| Boots Android | yes (seconds to minutes, GBs) | **no**: an app starts in milliseconds |
| Memory | hundreds of MB+ | **~12 MB RSS** for the VM + app |
| UI | a framebuffer in a window | **native UIKit views** |
| Runs native `.so` libraries | yes | no (see [limits](#what-works-today)) |

## How it works

```
APK ──zip──▶ classes*.dex (zero-copy)        AndroidManifest.xml / resources.arsc / res/layout/*.xml
                   │                                         │ binary XML (AXML) + resource table
                   ▼                                         ▼
     indroid-dex: zero-copy DEX parser          indroid-apk: launcher activity, strings, layouts
                   │
                   ▼
     indroid-vm:  non-recursive interpreter (all ~220 Dalvik opcodes)
                 · dense jump-table dispatch, monomorphic inline caches
                 · load-time bytecode verifier → unchecked register access
                 · 32-bit object handles, compact Latin-1/UTF-16 strings
                 · precise-heap / conservative-frame mark-sweep GC
                 · lazy class linking, lazy method decoding
                   │  + core.dex: our own java.lang / java.util / android.* (Java, ~360 KB)
                   ▼
     Android measure/layout in Java (real LinearLayout, RelativeLayout, ConstraintLayout from the APK, ...)
                   │
     UI tree + frames (JSON) ──▶ indroid-ffi (C ABI) ──▶ Swift/UIKit host on iPhone
                    └─▶ indroid-cli (headless; used by tests)
```

* **No AOSP framework.** Booting `framework.jar` would drag in thousands of JNI
  natives and system services. Instead `core/src/android` is a compact
  re-implementation of the parts apps actually touch (Activity lifecycle,
  Views, layouts, adapters, dialogs, Handler/Looper, SharedPreferences, …)
  written in plain Java and compiled to `core.dex`, which is embedded in the
  binary.
* **Correctness by differential testing.** Every program in `tests/java` is run
  on the host JVM and on indroid, and the outputs must match byte for byte
  (`tests/oracle.sh`). Real APKs are driven end to end in
  `crates/ffi/tests/apps.rs`.

## What works today

**Runtime:** the whole Dalvik instruction set except `invoke-polymorphic`
(`invoke-custom` lambdas via `LambdaMetafactory` are supported), reflection
(`Constructor`/`Method`/`Field`), Kotlin and its stdlib, exceptions with stack
traces and line numbers, static initializers, interfaces with default methods,
inner/anonymous classes, enums, string switch, multi-dimensional arrays,
`String.format`, a regex subset (`split`, `matches`, `replaceAll`), boxing,
`java.util` collections with JDK-identical iteration order, `java.util.concurrent`
basics, cooperative `Thread`s, `Handler.postDelayed` timers.

**AndroidX:** `AppCompatActivity`, `AppCompatDelegate` (theme checks, sub-decor,
view inflation factory), Material 3 themes and `MaterialButton` (shape
backgrounds drawn through `Canvas`), `ConstraintLayout`, `RecyclerView`
(scrolling and recycling through Android's own touch dispatch),
`MaterialToolbar` with menus, `TextInputLayout`, `Chip`, `MaterialSwitch`,
`MaterialCardView`, `CoordinatorLayout`, `Snackbar`.

**Framework:** Android's measure/layout pass, themes and styles
(`obtainStyledAttributes`, `TypedArray`, `?attr`, `textAppearance`,
configuration-qualified resources, framework resources), drawable XML
(shape, selector, layer-list, inset, ripple, vector), `Canvas` custom views
with touch, real touch dispatch (`onInterceptTouchEvent`), `ValueAnimator`,
`java.math`, `java.text`.

**Android:** `Activity` (lifecycle, `startActivity` with extras,
`startActivityForResult`, back stack), layout XML inflation (`<include>`,
`<merge>`, `android:onClick`), `LinearLayout`, `FrameLayout`, `RelativeLayout`
(approximate), `ScrollView`, `TextView`, `Button`, `EditText`, `ImageView`,
`CheckBox`, `Switch`, `RadioButton`/`RadioGroup`, `ProgressBar`, `SeekBar`,
`Spinner`, `ListView`/`GridView` + `ArrayAdapter`/`BaseAdapter`, `AlertDialog`,
`Toast`, `Log`, string/color/dimen resources, `SharedPreferences` (in memory).

**Not yet:** native `.so` libraries (JNI), Jetpack Compose, networking,
files, databases, sound, sensors, real threads (threads run cooperatively),
WebView. Apps that need these will start and then hit a clear error. See the
[roadmap](#roadmap). `indroid missing app.apk` lists every framework API an app
references that indroid does not provide yet.

Native libraries are the hard wall: they are ARM64 ELF code, and iOS will only
execute code signed into the app. Future options are build-time static
translation into the IPA, or an ARM64 interpreter. Neither is a weekend project.

## Install on iPhone

1. Download `indroid.ipa` from the latest
   [release](https://github.com/flyingsquirrel0419/indroid/releases) (or from the
   artifacts of the latest [iOS workflow run](https://github.com/flyingsquirrel0419/indroid/actions/workflows/ios.yml)).
2. Sideload it with SideStore, AltStore, Sideloadly or TrollStore. It is
   unsigned; your tool signs it with your Apple ID. No JIT enabler is needed.
3. Open indroid. Android 13 starts right away: the first launch downloads
   Google's Android 13 system image (~670 MB) and installs it, every launch
   after that boots straight into it. The app shows the console and logcat
   while Android comes up (no display on the phone yet; see the roadmap).

To run a single APK without booting Android, use the command-line Dalvik
runtime on a computer: `indroid run app.apk` (see [Build](#build)).

## Build

Requirements: Rust (stable), JDK 17+, Android SDK build-tools (for `d8` and
`aapt2`; only needed to rebuild `core.dex` or test APKs).

```sh
cargo build --release                                   # VM, CLI, C library
./target/release/indroid run examples/todo.apk           # interactive: click <id>, type <id> <text>, back, log
./target/release/indroid main app.dex com.example.Main   # run a plain main()

tools/build-core.sh                                     # rebuild crates/vm/core.dex from core/src
tools/build-apk.sh examples/todo examples/todo.apk      # build an example APK with the stock SDK
tests/oracle.sh                                         # JVM vs indroid differential tests
cargo test --workspace

ios/build-ipa.sh                                        # on macOS: build/indroid.ipa (unsigned)
```

## Performance

Micro-benchmarks (`tests/bench/run.sh`, x86-64 Linux). The comparison is
HotSpot's `-Xint` mode, which is a template interpreter generated as machine
code at startup, a technique iOS forbids:

| benchmark | indroid | JVM `-Xint` |
|---|---:|---:|
| `fib(27)` | 35 ms | 15 ms |
| 10M-iteration arithmetic loop | 301 ms | 150 ms |
| sieve, 2M | 194 ms | 110 ms |
| 1M `new int[8]` | 96 ms | 76 ms |
| 5M interface calls | 478 ms | 183 ms |
| **peak RSS** | **12 MB** | 66 MB |

App launch (load APK, link classes, run `onCreate`, produce the UI) is 3–8 ms
for the examples.

## Roadmap

- [x] Canvas / custom views, touch dispatch, vector drawables
- [x] Kotlin apps, `invoke-custom` lambdas, reflection
- [x] Android measure/layout, themes/styles, AndroidX AppCompat + Material 3 + ConstraintLayout
- [x] RecyclerView, MaterialToolbar menus, TextInputLayout, CoordinatorLayout
- [ ] Fragments, Navigation, ViewPager2, Jetpack Compose
- [ ] Faster interpreter: pre-decoded handlers, field-offset quickening, string fast paths
- [ ] Green threads (the non-recursive interpreter is already built for it)
- [ ] Networking (`HttpURLConnection`/OkHttp) over `URLSession`, files, SQLite
- [ ] Research: static ARM64 → signed-code translation for JNI libraries
- [x] neo: boot unmodified Android 13 to the launcher (AArch64 interpreter + Linux-ABI kernel + gfxstream)
- [ ] neo: display and touch on iPhone (gfxstream on MoltenVK)
- [ ] neo: speed: build-time AOT of profile-selected ROM code

## Project layout

| path | what |
|---|---|
| `crates/dex` | DEX parser |
| `crates/vm` | interpreter, heap/GC, class loader, natives |
| `crates/apk` | APK, binary XML, resources.arsc |
| `crates/ffi` | `App` driver + C ABI (`include/indroid.h`) |
| `crates/cli` | `indroid` command-line runner (`indroid neo` boots Android) |
| `crates/a64` | neo: AArch64 interpreter |
| `crates/kernel` | neo: Linux ABI (processes, binder, virtio-gpu, ...) |
| `crates/gpu` | neo: gfxstream renderer binding |
| `crates/rom` | neo: system image installer (sparse, super, ext4, APEX) |
| `crates/aot` | neo: build-time AOT of hot guest code (experimental) |
| `rom-overlay/` | neo: our patches layered over the Android ROM |
| `core/src` | Java class library + Android framework shim → `core.dex` |
| `ios/` | Swift UIKit host and IPA build script |
| `tests/java` | differential tests, `tests/bench` benchmarks |
| `examples/` | demo apps (plain Android SDK projects) |

## Contributing

The fastest way to help is to **try an APK and open an issue with the log**
(the Log button in the app, or `indroid run app.apk` then `log`). Missing
framework APIs are usually small, self-contained Java additions in
`core/src/android`.

## Credits

Built by a human with Claude Code and OpenAI Codex (`gpt-6.1-terra`) working as
pair programmers. indroid re-implements Android APIs from their public
documentation. The only AOSP-derived data is a trimmed copy of the framework
resource table (`crates/apk/framework-res.bin`, Apache-2.0), used to resolve
`android:` themes and attributes. neo boots Google's Android system image, which
the user downloads from Google; it is not redistributed. Not affiliated with Google or Apple. Android is a trademark
of Google LLC.

## License

Apache-2.0
