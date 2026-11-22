#!/usr/bin/env bash
# Ubuntu prerequisites: git, build-essential, cmake, ninja-build, meson,
# pkg-config, python3, libegl-dev, libgles-dev, libx11-dev, libvulkan1, mesa-vulkan-drivers. No packages installed.
# Override the default workspace with GFX_DIR; at most two jobs are used.
set -euo pipefail
root=${GFX_DIR:-"$HOME/gfx"}
root=$(mkdir -p "$root" && cd "$root" && pwd)
revisions=(d047a57228332d995d36600792fa9ccc26cf8ae6 3c1ced8a369417db591eb7cd083af5bb2c317975)
names=(gfxstream aemu)
for i in 0 1; do
    name=${names[$i]}; rev=${revisions[$i]}; src="$root/src/$name-$rev"
    if [[ ! -f "$src/.indroid-revision" ]]; then
        mkdir -p "$root/git" "$src"
        if [[ ! -d "$root/git/$name.git" ]]; then
            git clone --bare --depth 1 "https://android.googlesource.com/platform/hardware/google/$name" "$root/git/$name.git"
        fi
        git -C "$root/git/$name.git" fetch --depth 1 origin "$rev"
        git -C "$root/git/$name.git" archive "$rev" | tar -x -C "$src"
        printf '%s\n' "$rev" > "$src/.indroid-revision"
    fi
done
# The pinned Meson build omits its own default graphics-agent injection,
# while stream_renderer_init uses those agents. Supply the upstream factory.
python3 - "$root/src/gfxstream-${revisions[0]}/host/virtio-gpu-gfxstream-renderer.cpp" <<'PY_PATCH'
import pathlib, sys
p = pathlib.Path(sys.argv[1])
s = p.read_text()
old = "#ifndef GFXSTREAM_MESON_BUILD\n    android::emulation::injectGraphicsAgents(android::emulation::GfxStreamGraphicsAgentFactory());\n#endif"
new = "    android::emulation::injectGraphicsAgents(android::emulation::GfxStreamGraphicsAgentFactory());"
if old in s:
    p.write_text(s.replace(old, new, 1))
elif new not in s:
    raise SystemExit("unexpected gfxstream agent initialization source")
PY_PATCH
python3 - "$root/src/gfxstream-${revisions[0]}/host" <<'PY_PATCH'
import pathlib, sys
for name in ("GfxStreamAgents.h", "GfxStreamAgents.cpp"):
    p = pathlib.Path(sys.argv[1]) / name
    s = p.read_text()
    start = s.find("#ifndef GFXSTREAM_MESON_BUILD\n", s.index("namespace emulation {"))
    if start >= 0:
        end = s.index("\n#endif", start)
        s = s[:start] + s[start + len("#ifndef GFXSTREAM_MESON_BUILD\n"):end] + s[end + len("\n#endif"):]
        p.write_text(s)
PY_PATCH
script_dir=$(cd "$(dirname "$0")" && pwd)
python3 "$script_dir/gfxstream-frame-hook.py" "$root/src/gfxstream-${revisions[0]}"
python3 "$script_dir/gfxstream-android13-compat.py" "$root/src/gfxstream-${revisions[0]}"
# GFX_PATCH_ONLY=1: fetch and patch the sources, then stop (used by build-gfxstream-ios.sh).
[[ -z "${GFX_PATCH_ONLY:-}" ]] || exit 0
out="$root/out"
cmake -S "$root/src/aemu-${revisions[1]}" -B "$root/build-aemu" -G Ninja \
    -DCMAKE_INSTALL_PREFIX="$out" -DAEMU_COMMON_GEN_PKGCONFIG=ON \
    -DAEMU_COMMON_BUILD_CONFIG=gfxstream -DENABLE_VKCEREAL_TESTS=OFF -DBUILD_SHARED_LIBS=OFF
cmake --build "$root/build-aemu" -j 2
cmake --install "$root/build-aemu"
export PKG_CONFIG_PATH="$out/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
if [[ ! -f "$root/build-gfxstream/build.ninja" ]]; then
    meson setup "$root/build-gfxstream" "$root/src/gfxstream-${revisions[0]}" \
        --prefix="$out" --libdir=lib -Dgfxstream-build=host -Ddecoders=gles,vulkan,composer
fi
meson compile -C "$root/build-gfxstream" -j 2
meson install -C "$root/build-gfxstream"
