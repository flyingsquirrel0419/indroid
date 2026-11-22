#!/usr/bin/env python3
"""Add an optional host-side Vulkan color-buffer capture hook to pinned gfxstream."""
import pathlib
import sys

source = pathlib.Path(sys.argv[1]) / "host/FrameBuffer.cpp"
text = source.read_text()
if "indroid_set_frame_callback" not in text:
    marker = "namespace gfxstream {\n"
    hook = r'''
// Optional indroid display capture. Pixels are borrowed for the callback only.
#include <atomic>
using IndroidFrameCallback = void (*)(uint32_t, uint32_t, const unsigned char*);
static std::atomic<IndroidFrameCallback> sIndroidFrameCallback{nullptr};
extern "C" __attribute__((visibility("default")))
void indroid_set_frame_callback(IndroidFrameCallback callback) {
    sIndroidFrameCallback.store(callback, std::memory_order_release);
}
'''
    if text.count(marker) != 1:
        raise SystemExit("unexpected FrameBuffer.cpp namespace")
    text = text.replace(marker, hook + "\n" + marker, 1)
    marker = "    colorBuffer->touch();\n    if (m_subWin) {"
    hook = r'''    colorBuffer->touch();
    if (auto capture = sIndroidFrameCallback.load(std::memory_order_acquire)) {
        const uint32_t width = colorBuffer->getWidth();
        const uint32_t height = colorBuffer->getHeight();
        if (width && height && width <= 16384 && height <= 16384) {
            std::vector<unsigned char> pixels(static_cast<size_t>(width) * height * 4);
            colorBuffer->readToBytes(0, 0, width, height, GL_RGBA, GL_UNSIGNED_BYTE,
                                     pixels.data(), pixels.size());
            capture(width, height, pixels.data());
        }
    }
    if (m_subWin) {'''
    if text.count(marker) != 1:
        raise SystemExit("unexpected FrameBuffer.cpp post path")
    text = text.replace(marker, hook, 1)
    source.write_text(text)
