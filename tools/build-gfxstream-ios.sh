#!/usr/bin/env bash
# Cross-build gfxstream's host renderer (libgfxstream_backend.dylib) for iOS arm64
# and fetch MoltenVK. Needs a macOS host with Xcode, cmake, ninja, meson, pkg-config.
# Output: $GFX_DIR/out/lib/libgfxstream_backend.dylib and
#         $GFX_DIR/out/Frameworks/MoltenVK.framework (default GFX_DIR: build/gfx-ios).
# Same pinned sources and compat patches as tools/build-gfxstream.sh (Linux).
# Vulkan only: the iOS host has no EGL/GL, so decoders=vulkan,composer.
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD
export GFX_DIR=${GFX_DIR:-"$repo/build/gfx-ios"}
mkdir -p "$GFX_DIR"
root=$(cd "$GFX_DIR" && pwd)
out="$root/out"
export GFX_DIR=$root

mvk_version=v1.4.2
mvk_sha256=b5d947b1660e6e9fed40b9cd2387e160aaab9e80b775c0cef7e14059405178c1

if [[ -f "$out/lib/libgfxstream_backend.dylib" && -d "$out/Frameworks/MoltenVK.framework" ]]; then
    echo "gfxstream for iOS already built in $out"
    exit 0
fi

# 1. MoltenVK (pinned prebuilt dynamic framework, iOS arm64).
mkdir -p "$out/Frameworks" "$root/dl"
if [[ ! -d "$out/Frameworks/MoltenVK.framework" ]]; then
    tar_path="$root/dl/MoltenVK-ios.tar"
    curl -fsSL -o "$tar_path" \
        "https://github.com/KhronosGroup/MoltenVK/releases/download/$mvk_version/MoltenVK-ios.tar"
    echo "$mvk_sha256  $tar_path" | shasum -a 256 -c -
    rm -rf "$root/dl/mvk" && mkdir -p "$root/dl/mvk"
    tar -xf "$tar_path" -C "$root/dl/mvk"
    cp -R "$root/dl/mvk/MoltenVK/MoltenVK/dynamic/MoltenVK.xcframework/ios-arm64/MoltenVK.framework" \
        "$out/Frameworks/"
fi

# 2. Fetch and patch the pinned sources (shared with the Linux script).
GFX_PATCH_ONLY=1 tools/build-gfxstream.sh
gfx_src=$(echo "$root"/src/gfxstream-*)
aemu_src=$(echo "$root"/src/aemu-*)

# iOS SDK compatibility patches for the (macOS-oriented) pinned sources. Idempotent.
python3 - "$aemu_src" <<'PY_PATCH'
import pathlib, sys
root = pathlib.Path(sys.argv[1])
p = root / "base/System.cpp"
s = p.read_text()
# libproc.h is not in the iOS SDK; the executable path comes from dyld instead.
s = s.replace("#include <libproc.h>\n", "#include <mach-o/dyld.h>\n")
s = s.replace("    proc_pidpath(pid, s, sizeof(s));\n",
              "    (void)pid;\n    { uint32_t n = sizeof(s); if (_NSGetExecutablePath(s, &n) != 0) s[0] = '\\0'; }\n")
p.write_text(s)

# system-native-mac.mm uses IOKit/AppKit (macOS only): keep the CPU-time helper, stub the rest.
p = root / "base/system-native-mac.mm"
s = p.read_text()
if "indroid iOS" not in s:
    start = s.index("#include <CoreFoundation/CoreFoundation.h>")
    s = s[:start] + """// indroid iOS: IOKit/AppKit are unavailable; disk kind and dock icon are stubbed.
#include <CoreFoundation/CoreFoundation.h>
#import <Foundation/Foundation.h>
""" + s[s.index("#include <mach/mach_init.h>"):]
    a = s.index("Optional<DiskKind> nativeDiskKind")
    b = s.index("}  // namespace base")
    s = s[:a] + """Optional<DiskKind> nativeDiskKind(int) { return {}; }

void hideDockIcon_macImpl(void) {}

""" + s[b:]
    # disableAppNap_macImpl uses NSProcessInfo, which exists on iOS.
    p.write_text(s)

p = root / "base/CMakeLists.txt"
s = p.read_text()
s = s.replace('"-framework Foundation" "-framework AppKit" "-framework IOKit"', '"-framework Foundation"')
p.write_text(s)
PY_PATCH

python3 - "$gfx_src" <<'PY_PATCH'
import pathlib, sys
root = pathlib.Path(sys.argv[1]) / "host/gl/gl-host-common"
# macTouchOpenGL.m / IOKit GPU probing are macOS only; Vulkan-only iOS needs neither.
stub = root / "opengl/NativeGpuInfo_ios.cpp"
stub.write_text("""// indroid iOS: no GL stack to probe; MoltenVK is the only GPU path.
#include "host-common/opengl/NativeGpuInfo.h"

void getGpuInfoListNative(GpuInfoList*) {}

bool isVulkanSafeToUseNative() { return true; }
""")
m = root / "meson.build"
s = m.read_text()
if "NativeGpuInfo_ios.cpp" not in s:
    s = s.replace("if host_machine.system() == 'darwin'\n  files_lib_gl_host_common += files_gl_host_common_darwin",
                  "if host_machine.system() == 'darwin' and host_machine.subsystem() == 'ios'\n  files_lib_gl_host_common += files('opengl/NativeGpuInfo_ios.cpp')\nelif host_machine.system() == 'darwin'\n  files_lib_gl_host_common += files_gl_host_common_darwin")
    m.write_text(s)

# host_backend has no sources; Apple's ar refuses to create an empty archive.
b = pathlib.Path(sys.argv[1]) / "host/backend"
(b / "indroid_empty.cpp").write_text("namespace indroid_empty { int placeholder = 0; }\n")
m = b / "meson.build"
t = m.read_text()
if "indroid_empty.cpp" not in t:
    m.write_text(t.replace("files_lib_host_backend = files(\n)", "files_lib_host_backend = files('indroid_empty.cpp')"))

# FrameBuffer.h wraps the Vulkan accessors in the GLES-only block; hoist them out.
h = pathlib.Path(sys.argv[1]) / "host/FrameBuffer.h"
t = h.read_text()
vk = "    vk::VkEmulation& getEmulationVk();\n    bool hasEmulationVk() const { return m_emulationVk != nullptr; }\n"
if vk in t and "indroid: Vulkan accessors" not in t:
    t = t.replace(vk, "#endif  // indroid: Vulkan accessors are not GLES-only\n" + vk + "#if GFXSTREAM_ENABLE_HOST_GLES\n", 1)
    h.write_text(t)

# GLES-less compat stub: native window handles are pointers (FBNativeWindowType), and
# FrameBuffer asks the (absent) GL emulation about formats.
g = pathlib.Path(sys.argv[1]) / "host/GlesCompat.h"
t = g.read_text()
t = t.replace("typedef unsigned int EGLNativeWindowType;", "typedef void* EGLNativeWindowType;")
if "isFormatSupported" not in t:
    t = t.replace("    ~EmulationGl() {}\n", "    ~EmulationGl() {}\n    bool isFormatSupported(GLenum) { return false; }\n")
g.write_text(t)

# VK_MVK_macos_surface does not exist on iOS; the portable Metal surface extension does.
for name in ("VkCommonOperations.cpp", "VkDecoderGlobalState.cpp"):
    f = pathlib.Path(sys.argv[1]) / "host/vulkan" / name
    t = f.read_text()
    f.write_text(t.replace("VK_MVK_MACOS_SURFACE_EXTENSION_NAME", '"VK_EXT_metal_surface"'))
PY_PATCH

sdk=$(xcrun --sdk iphoneos --show-sdk-path)
arch_flags="-arch arm64 -isysroot $sdk -miphoneos-version-min=15.0"

# 3. aemu (static) for iOS.
cmake -S "$aemu_src" -B "$root/build-aemu" -G Ninja \
    -DCMAKE_SYSTEM_NAME=iOS -DCMAKE_OSX_SYSROOT=iphoneos \
    -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=15.0 \
    -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY \
    -DCMAKE_INSTALL_PREFIX="$out" -DAEMU_COMMON_GEN_PKGCONFIG=ON \
    -DAEMU_COMMON_BUILD_CONFIG=gfxstream -DENABLE_VKCEREAL_TESTS=OFF -DBUILD_SHARED_LIBS=OFF
cmake --build "$root/build-aemu"
cmake --install "$root/build-aemu"

# 4. gfxstream host renderer via a Meson cross file.
cat > "$root/ios-cross.ini" <<CROSS
[binaries]
c = ['xcrun', '-sdk', 'iphoneos', 'clang']
cpp = ['xcrun', '-sdk', 'iphoneos', 'clang++']
objc = ['xcrun', '-sdk', 'iphoneos', 'clang']
objcpp = ['xcrun', '-sdk', 'iphoneos', 'clang++']
ar = ['xcrun', '-sdk', 'iphoneos', 'ar']
strip = ['xcrun', '-sdk', 'iphoneos', 'strip']
pkg-config = 'pkg-config'

[built-in options]
c_args = ['-arch', 'arm64', '-isysroot', '$sdk', '-miphoneos-version-min=15.0']
# Khronos EGL/GLES headers ship in the tree; no GL is linked, but the decoders include them.
cpp_args = ['-arch', 'arm64', '-isysroot', '$sdk', '-miphoneos-version-min=15.0', '-I$gfx_src/common/opengl/include', '-DVK_USE_PLATFORM_METAL_EXT']
c_link_args = ['-arch', 'arm64', '-isysroot', '$sdk', '-miphoneos-version-min=15.0']
cpp_link_args = ['-arch', 'arm64', '-isysroot', '$sdk', '-miphoneos-version-min=15.0']
pkg_config_path = ['$out/lib/pkgconfig']

[host_machine]
system = 'darwin'
subsystem = 'ios'
kernel = 'xnu'
cpu_family = 'aarch64'
cpu = 'arm64'
endian = 'little'

[properties]
needs_exe_wrapper = true
CROSS
if [[ ! -f "$root/build-gfxstream/build.ninja" ]]; then
    meson setup "$root/build-gfxstream" "$gfx_src" --cross-file "$root/ios-cross.ini" \
        --prefix="$out" --libdir=lib -Dgfxstream-build=host -Ddecoders=vulkan,composer
fi
meson compile -C "$root/build-gfxstream" --ninja-args=-k0
meson install -C "$root/build-gfxstream"

# 5. Plain file name + @rpath install name; the app dlopens it from Frameworks/.
cp -L "$out/lib/libgfxstream_backend.dylib" "$out/lib/libgfxstream_backend.tmp"
rm -f "$out"/lib/libgfxstream_backend*.dylib
mv "$out/lib/libgfxstream_backend.tmp" "$out/lib/libgfxstream_backend.dylib"
install_name_tool -id @rpath/libgfxstream_backend.dylib "$out/lib/libgfxstream_backend.dylib"
echo "built $out/lib/libgfxstream_backend.dylib"
