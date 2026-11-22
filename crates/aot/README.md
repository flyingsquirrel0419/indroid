# ROM page AOT

Collect a profile with `INDROID_PROFILE=/tmp/indroid.prof` while running the
interpreter. Build selected pages into the CLI with:

```sh
INDROID_AOT_PROFILE=/tmp/indroid.prof INDROID_AOT_ROM="$HOME/aosp/rom13" \
INDROID_AOT_LIMIT=4 cargo build -j2 --profile fast -p indroid-cli
```

The build script emits Rust state machines for the hottest file-backed pages.
The default limit is one; increase it deliberately because each additional page
increases compile time and binary size. With no profile and ROM variables, the
table is empty. Generated units are disabled by default because performance is
workload-dependent; set `INDROID_ENABLE_AOT=1` to use them. `INDROID_DISABLE_AOT=1`
selects the interpreter even when AOT is enabled; `INDROID_AOT_TRACE=1` logs
the first entered unit. `INDROID_AOT_ONLY_OFFSET=0x11a000` restricts dispatch
to one file-page offset when measuring individual units. The executable mapping must be non-writable, and the
runtime checks its contents before dispatch; mapping-generation changes flush
cached translations.

For inspection or integration into another build, generate source directly:

```sh
cargo run -j2 -p indroid-aot --bin generate -- /tmp/indroid.prof "$HOME/aosp/rom13" /tmp/units.rs 4
```
