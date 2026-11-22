#!/usr/bin/env bash
# Sanity-check the Mach-O files in an iOS app bundle: platform, exported
# symbols the indroid GPU loader needs, and no host-specific library paths.
set -euo pipefail
app=${1:?usage: verify-ios-bundle.sh path/to/indroid.app}
fail=0
check() { echo "FAIL: $*"; fail=1; }

for f in "$app/Frameworks/libgfxstream_backend.dylib" "$app/Frameworks/MoltenVK.framework/MoltenVK"; do
    [[ -f "$f" ]] || { check "missing $f"; continue; }
    echo "== $f"
    xcrun vtool -show-build "$f" | sed 's/^/   /'
    xcrun vtool -show-build "$f" | grep -q 'platform IOS$' || check "$f is not built for the iOS platform"
    lipo -archs "$f" | grep -qx arm64 || check "$f is not arm64"
    if otool -L "$f" | tail -n +2 | grep -Eq '/usr/local|/opt/homebrew|/Users/|/Applications/'; then
        check "$f links against a host path"
    fi
done

gfx="$app/Frameworks/libgfxstream_backend.dylib"
if [[ -f "$gfx" ]]; then
    syms=$(nm -gU "$gfx")
    # Keep in sync with the sym!() list in crates/gpu/src/lib.rs.
    for s in init teardown resource_create resource_unref context_create context_destroy \
             ctx_attach_resource ctx_detach_resource submit_cmd create_blob export_blob \
             resource_map resource_unmap transfer_read_iov transfer_write_iov get_cap_set \
             fill_caps create_fence flush; do
        grep -q " _stream_renderer_$s\$" <<<"$syms" || check "missing symbol stream_renderer_$s"
    done
    for s in set_frame_callback set_dma_resolver set_puid_resolver; do
        grep -q " _indroid_$s\$" <<<"$syms" || check "missing symbol indroid_$s"
    done
fi
[[ $fail == 0 ]] && echo "bundle OK" || exit 1
