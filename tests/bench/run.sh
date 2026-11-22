#!/usr/bin/env bash
# Interpreter micro-benchmarks: indroid vs the host JVM in interpreter mode (-Xint).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SDK="${ANDROID_HOME:-$HOME/android-sdk}"
D8="$(ls -d "$SDK"/build-tools/*/ | sort -V | tail -1)d8"
JAR="$(ls -d "$SDK"/platforms/android-*/ | sort -V | tail -1)android.jar"
OUT="$ROOT/tests/out/bench"; rm -rf "$OUT"; mkdir -p "$OUT/c"
javac --release 8 -nowarn -d "$OUT/c" "$ROOT/tests/bench/Bench.java"
(cd "$OUT/c" && "$D8" --min-api 21 --lib "$JAR" --output "$OUT" $(find . -name '*.class'))
cargo build -q --release -p indroid-cli --manifest-path "$ROOT/Cargo.toml"
echo "== indroid"; /usr/bin/time -f "  wall %es, max RSS %M KB" "$ROOT/target/release/indroid" main "$OUT/classes.dex" Bench
echo "== JVM -Xint"; /usr/bin/time -f "  wall %es, max RSS %M KB" java -Xint -cp "$OUT/c" Bench
