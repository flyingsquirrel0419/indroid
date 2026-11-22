#!/usr/bin/env bash
# Kotlin oracle: tests/kotlin/<Name>.kt is compiled with kotlinc, run on the
# JVM and on indroid (kotlin-stdlib dexed alongside), outputs must match.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/android-sdk}}"
D8="$(ls -d "$SDK"/build-tools/*/ | sort -V | tail -1)d8"
ANDROID_JAR="$(ls -d "$SDK"/platforms/android-*/ | sort -V | tail -1)android.jar"
KOTLINC="${KOTLINC:-$( [ -x "$HOME/tools/kotlinc/bin/kotlinc" ] && echo "$HOME/tools/kotlinc/bin/kotlinc" || command -v kotlinc)}"
KOTLINC="$(readlink -f "$KOTLINC")"
STDLIB="$(dirname "$KOTLINC")/../lib/kotlin-stdlib.jar"
BIN="$ROOT/target/release/indroid"
OUT="$ROOT/tests/out/kotlin"
mkdir -p "$OUT"
cargo build -q --release -p indroid-cli --manifest-path "$ROOT/Cargo.toml" || exit 1
if [ ! -f "$OUT/stdlib/classes.dex" ] || [ "$STDLIB" -nt "$OUT/stdlib/classes.dex" ]; then
  mkdir -p "$OUT/stdlib"
  "$D8" --min-api 24 --lib "$ANDROID_JAR" --output "$OUT/stdlib" "$STDLIB" > "$OUT/stdlib/d8.log" 2>&1 || { echo "d8 of $STDLIB failed"; tail -20 "$OUT/stdlib/d8.log"; exit 1; }
fi
pass=0; fail=0; failed=()
for src in "$ROOT"/tests/kotlin/${1:-*}.kt; do
  name="$(basename "$src" .kt)"
  d="$OUT/$name"; rm -rf "$d"; mkdir -p "$d"
  "$KOTLINC" -jvm-target 1.8 -nowarn "$src" -d "$d/app.jar" 2>"$d/kotlinc.err" || { echo "kotlinc failed: $name"; cat "$d/kotlinc.err"; fail=$((fail+1)); failed+=("$name"); continue; }
  "$D8" --min-api 24 --lib "$ANDROID_JAR" --classpath "$STDLIB" --output "$d" "$d/app.jar" >/dev/null 2>"$d/d8.err" || { echo "d8 failed: $name"; cat "$d/d8.err"; fail=$((fail+1)); failed+=("$name"); continue; }
  main="${name}Kt"
  java -cp "$d/app.jar:$STDLIB" "$main" >"$d/expected.txt" 2>/dev/null
  timeout 60 "$BIN" main "$d/classes.dex" "$OUT"/stdlib/classes*.dex "$main" >"$d/actual.txt" 2>"$d/stderr.txt"
  if cmp -s "$d/expected.txt" "$d/actual.txt"; then
    pass=$((pass+1)); echo "ok   $name"
  else
    fail=$((fail+1)); failed+=("$name"); echo "FAIL $name"
    diff "$d/expected.txt" "$d/actual.txt" | head -20
    head -5 "$d/stderr.txt"
  fi
done
echo "kotlin: $pass passed, $fail failed ${failed[*]:-}"
[ "$fail" -eq 0 ]
