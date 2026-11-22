#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
jar="${ANDROID_HOME:?Set ANDROID_HOME}/platforms/android-35/android.jar"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
unzip -p "$jar" resources.arsc > "$tmp/resources.arsc"
cargo run --quiet --manifest-path "$root/Cargo.toml" -p indroid-apk --bin build-framework-res -- "$tmp/resources.arsc" "$root/crates/apk/framework-res.bin"
