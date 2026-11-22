#!/usr/bin/env bash
# Oracle test: every tests/java/<Name>.java is compiled once, run on the host
# JVM and on indroid (via d8), and the outputs must be identical.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/android-sdk}}"
D8="$(ls -d "$SDK"/build-tools/*/ | sort -V | tail -1)d8"
ANDROID_JAR="$(ls -d "$SDK"/platforms/android-*/ | sort -V | tail -1)android.jar"
BIN="$ROOT/target/release/indroid"
OUT="$ROOT/tests/out"
mkdir -p "$OUT"
cargo build -q --release -p indroid-cli --manifest-path "$ROOT/Cargo.toml" || exit 1
pass=0; fail=0; failed=()
for src in "$ROOT"/tests/java/${1:-*}.java; do
  name="$(basename "$src" .java)"
  d="$OUT/$name"; rm -rf "$d"; mkdir -p "$d/classes"
  javac --release 8 -nowarn -encoding UTF-8 -d "$d/classes" "$src" 2>"$d/javac.err" || { echo "javac failed: $name"; cat "$d/javac.err"; fail=$((fail+1)); failed+=("$name"); continue; }
  api="$(sed -n 's|^// min-api: *\([0-9]*\).*|\1|p' "$src" | head -1)"
  api="${api:-${ORACLE_MIN_API:-21}}"
  flags="$(sed -n 's|^// d8-flags: *||p' "$src" | head -1)"
  (cd "$d/classes" && "$D8" $flags --min-api "$api" --lib "$ANDROID_JAR" --output "$d" $(find . -name '*.class')) 2>"$d/d8.err" || { echo "d8 failed: $name"; cat "$d/d8.err"; fail=$((fail+1)); failed+=("$name"); continue; }
  java -cp "$d/classes" "$name" >"$d/expected.txt" 2>/dev/null
  timeout 60 "$BIN" main "$d/classes.dex" "$name" >"$d/actual.txt" 2>"$d/stderr.txt"
  if cmp -s "$d/expected.txt" "$d/actual.txt"; then
    pass=$((pass+1)); echo "ok   $name"
  else
    fail=$((fail+1)); failed+=("$name"); echo "FAIL $name"
    diff "$d/expected.txt" "$d/actual.txt" | head -20
    head -5 "$d/stderr.txt"
  fi
done
echo "oracle: $pass passed, $fail failed ${failed[*]:-}"
[ "$fail" -eq 0 ]
