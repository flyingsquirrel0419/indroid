#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${SIMULATOR:-0}" == "1" ]]; then
  rust_target=aarch64-apple-ios-sim
  sdk=iphonesimulator
  swift_target=arm64-apple-ios15.0-simulator
  app=build/sim/indroid.app
else
  rust_target=aarch64-apple-ios
  sdk=iphoneos
  swift_target=arm64-apple-ios15.0
  app=build/Payload/indroid.app
  rm -rf build/Payload
fi

cargo build --profile release-ffi -p indroid-ffi --target "$rust_target"
rm -rf "$app"
mkdir -p "$app"
xcrun -sdk "$sdk" swiftc -sdk "$(xcrun --sdk "$sdk" --show-sdk-path)" \
  -target "$swift_target" -O -import-objc-header ios/Bridging-Header.h \
  -L "target/$rust_target/release-ffi" -lindroid_ffi \
  -framework UIKit -framework Security -framework SystemConfiguration -lc++ \
  ios/Sources/*.swift -o "$app/indroid"
cp ios/Info.plist "$app/Info.plist"
# indroid neo: our patches layered over the Android ROM.
cp -Rp rom-overlay "$app/rom-overlay"
printf 'APPL????' > "$app/PkgInfo"
if [[ "${SIMULATOR:-0}" != "1" ]]; then
  rm -f build/indroid.ipa
  (cd build && zip -qry indroid.ipa Payload)
fi
