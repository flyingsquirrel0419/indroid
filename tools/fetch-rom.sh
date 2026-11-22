#!/usr/bin/env bash
# Downloads the AOSP arm64 emulator system image (Android 13, API 33) and
# unpacks it into a plain directory tree usable as indroid neo's guest rootfs:
#   <out>/            system partition (system-as-root: /system, /bin -> ...)
#   <out>/system_ext, <out>/product, <out>/vendor
#   <out>/apex/<name> activated APEX payloads (what apexd would mount)
# The ROM is not part of this repository; run this locally.
# Needs: curl, unzip, 7z, python3, debugfs (e2fsprogs).
set -euo pipefail
TOOLS="$(cd "$(dirname "$0")" && pwd)"
# ROM_VARIANT=emu13 (default): Google's arm64 emulator image (Android SDK
#   license; for local development only).
# ROM_VARIANT=cf13: AOSP cuttlefish arm64 phone, android13-gsi branch, from
#   ci.android.com (AOSP licenses; redistributable; virtio-gpu/gfxstream).
VARIANT="${ROM_VARIANT:-emu13}"
case "$VARIANT" in
  emu13)
    OUT="${1:-$HOME/aosp/rom13}"
    WORK="${WORK:-$HOME/aosp/work13}"
    URL="https://dl.google.com/android/repository/sys-img/android/arm64-v8a-33_r02.zip"
    ;;
  cf13)
    OUT="${1:-$HOME/aosp/romcf}"
    WORK="${WORK:-$HOME/aosp/cf13}"
    BUILD="${CF_BUILD:-16833088}"
    URL="https://ci.android.com/builds/submitted/$BUILD/aosp_cf_arm64_phone-userdebug/latest/raw/aosp_cf_arm64_phone-img-$BUILD.zip"
    ;;
  *) echo "unknown ROM_VARIANT $VARIANT" >&2; exit 2 ;;
esac
mkdir -p "$WORK" "$OUT"
cd "$WORK"
[ -f img.zip ] || curl -fL -o img.zip "$URL"
if [ ! -f system.ext4 ]; then
  if [ "$VARIANT" = cf13 ]; then
    unzip -o -q img.zip super.img && mv super.img 1.super.img
  else
    unzip -o -q img.zip 'arm64-v8a/system.img' 'arm64-v8a/build.prop'
    7z x -y arm64-v8a/system.img 1.super.img -o. >/dev/null
    rm arm64-v8a/system.img
  fi
  # Locate the ext4 partitions inside super.img by superblock magic (each is
  # one contiguous extent in emulator images).
  python3 - <<'PY'
import os, struct, subprocess
f = open('1.super.img', 'rb'); size = os.path.getsize('1.super.img'); off = 0
names = {b'/': 'system'}
while off < size:
    f.seek(off + 1024 + 0x38)
    if f.read(2) == b'\x53\xef':
        f.seek(off + 1024); sb = f.read(256)
        blocks = struct.unpack_from('<I', sb, 4)[0]; lbs = struct.unpack_from('<I', sb, 24)[0]
        mnt = sb[0x78:0x88].rstrip(b'\0'); name = names.get(mnt, mnt.decode())
        n = blocks * (1024 << lbs)
        if mnt == b'system':  # system_other (odex), not needed
            off += (n + (1 << 20) - 1) & ~((1 << 20) - 1)
            continue
        subprocess.check_call(['dd', 'if=1.super.img', f'of={name}.ext4', 'bs=1M',
            f'skip={off >> 20}', f'count={(n + (1 << 20) - 1) >> 20}', 'status=none'])
        off += (n + (1 << 20) - 1) & ~((1 << 20) - 1)
        continue
    off += 1 << 20
PY
  rm 1.super.img
fi
rdump() { debugfs -R "rdump / $2" "$1" >/dev/null 2>&1; }
rm -rf "$OUT.tmp" && mkdir -p "$OUT.tmp"
rdump system.ext4 "$OUT.tmp"
for p in system_ext product vendor odm; do
  [ -f $p.ext4 ] || continue
  rm -rf "$OUT.tmp/$p"; mkdir -p "$OUT.tmp/$p"; rdump $p.ext4 "$OUT.tmp/$p"
done
# Activate APEXes: each .apex/.capex holds apex_payload.img (ext4).
mkdir -p "$OUT.tmp/apex"
for a in "$OUT.tmp"/system/apex/*.apex "$OUT.tmp"/system/apex/*.capex "$OUT.tmp"/vendor/apex/*.apex; do
  [ -f "$a" ] || continue
  t=$(mktemp -d)
  if [[ "$a" == *.capex ]]; then unzip -q -o "$a" original_apex -d "$t"; unzip -q -o "$t/original_apex" apex_payload.img apex_manifest.pb -d "$t"
  else unzip -q -o "$a" apex_payload.img apex_manifest.pb -d "$t"; fi
  name=$(python3 -c "import sys;b=open(sys.argv[1],'rb').read();n=b[2:2+b[1]].decode();print(n)" "$t/apex_manifest.pb")
  mkdir -p "$OUT.tmp/apex/$name"; rdump "$t/apex_payload.img" "$OUT.tmp/apex/$name"
  rm -rf "$t"
done
python3 "$TOOLS/apex-info-list.py" "$OUT.tmp"
python3 "$TOOLS/apex-labels.py" "$OUT.tmp"
rm -rf "$OUT" && mv "$OUT.tmp" "$OUT"
du -sh "$OUT"; ls "$OUT/apex"
