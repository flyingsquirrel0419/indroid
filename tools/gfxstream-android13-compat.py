#!/usr/bin/env python3
"""Disable a Vulkan feature whose extension Android 13's ranchu ICD hides.

ANGLE on Android 13 queries the Vulkan 1.2 core hostQueryReset feature and,
when true, requests VK_EXT_host_query_reset. The same ROM's ranchu ICD does
not advertise that extension, so Android's Vulkan loader rejects vkCreateDevice.
The feature is optional; reporting it unsupported keeps these APIs consistent.
"""
import pathlib
import sys

source = pathlib.Path(sys.argv[1]) / "host/vulkan/VkDecoderGlobalState.cpp"
text = source.read_text()
marker = "        // Disable a set of Vulkan features if BypassVulkanDeviceFeatureOverrides is NOT enabled.\n"
patch = '''        // indroid Android 13 compatibility: ranchu does not advertise
        // VK_EXT_host_query_reset although ANGLE requests it when this core
        // Vulkan 1.2 feature is true. Report the optional feature as absent.
        if (auto* hostQueryReset = static_cast<VkPhysicalDeviceHostQueryResetFeatures*>(__vk_find_struct(pFeatures, VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_HOST_QUERY_RESET_FEATURES))) {
            hostQueryReset->hostQueryReset = VK_FALSE;
        }

'''
if "indroid Android 13 compatibility" not in text:
    if text.count(marker) != 1:
        raise SystemExit("unexpected Vulkan feature override location")
    source.write_text(text.replace(marker, patch + marker, 1))

# Android 13 goldfish (hasVirtioGpuNext) reads the blob size and host page
# offset back from vkGetMemoryHostAddressInfoGOOGLE; current gfxstream only
# returns the hostmem id, so the guest asks for a zero-sized blob and aborts.
text = source.read_text()
old = '''        uint64_t hostBlobId = sNextHostBlobId++;
        *pHostmemId = hostBlobId;
        return vkGetBlobInternal(boxed_device, memory, hostBlobId);
'''
new = '''        uint64_t hostBlobId = sNextHostBlobId++;
        *pHostmemId = hostBlobId;
        VkResult blobResult = vkGetBlobInternal(boxed_device, memory, hostBlobId);
        // indroid Android 13 compatibility: report size and page offset.
        if (blobResult == VK_SUCCESS) {
            std::lock_guard<std::mutex> lock(mMutex);
            if (auto* info = android::base::find(mMemoryInfo, memory)) {
                if (pSize) *pSize = info->size;
                if (pAddress) *pAddress = (uint64_t)(uintptr_t)info->ptr & ~kPageMaskForBlob;
            }
        }
        return blobResult;
'''
if "report size and page offset" not in text:
    if text.count(old) != 1:
        raise SystemExit("unexpected vkGetMemoryHostAddressInfoGOOGLE body")
    source.write_text(text.replace(old, new, 1))

# Android 13 ranchu HWC asks for an EGL retire fence after rcCompose even when
# the guest renders with ANGLE on Vulkan. A Vulkan-only host has no EGL; hand
# back a null sync (the composition already completed synchronously).
fb = pathlib.Path(sys.argv[1]) / "host/FrameBuffer.cpp"
text = fb.read_text()
old = '''    if (!m_emulationGl) {
        GFXSTREAM_ABORT(FatalError(ABORT_REASON_OTHER)) << "GL/EGL emulation not available.";
    }

    // TODO(b/233939967): move RenderThreadInfoGl usage to EmulationGl.
    RenderThreadInfoGl* const info = RenderThreadInfoGl::get();
    if (!info) {
        GFXSTREAM_ABORT(FatalError(ABORT_REASON_OTHER)) << "RenderThreadInfoGl not available.";
    }
    if (!info->currContext) {'''
new = '''    if (!m_emulationGl) {
        // indroid: Vulkan-only host, no EGL fences.
        if (outSync) *outSync = 0;
        if (outSyncThread) *outSyncThread = 0;
        return;
    }

    // TODO(b/233939967): move RenderThreadInfoGl usage to EmulationGl.
    RenderThreadInfoGl* const info = RenderThreadInfoGl::get();
    if (!info) {
        GFXSTREAM_ABORT(FatalError(ABORT_REASON_OTHER)) << "RenderThreadInfoGl not available.";
    }
    if (!info->currContext) {'''
if "indroid: Vulkan-only host" not in text:
    if text.count(old) != 1:
        raise SystemExit("unexpected createEmulatedEglFenceSync body")
    fb.write_text(text.replace(old, new, 1))

# Android 13 ranchu gralloc uploads through rcUpdateColorBufferDMA with a buffer
# from the goldfish_address_space host-memory allocator, which indroid's kernel
# implements. There is no QEMU DmaMap: resolve guest physical addresses through
# a callback indroid registers (unknown addresses yield null and are skipped).
ring = pathlib.Path(sys.argv[1]) / "host/RingStream.cpp"
text = ring.read_text()
originals = [
    """void* RingStream::getDmaForReading(uint64_t guest_paddr) {
    return emugl::g_emugl_dma_get_host_addr(guest_paddr);
}

void RingStream::unlockDma(uint64_t guest_paddr) { emugl::g_emugl_dma_unlock(guest_paddr); }
""",
    """void* RingStream::getDmaForReading(uint64_t guest_paddr) {
    // indroid: guest physical addresses are host pointers.
    return reinterpret_cast<void*>(static_cast<uintptr_t>(guest_paddr));
}

void RingStream::unlockDma(uint64_t guest_paddr) { (void)guest_paddr; }
""",
]
new = """using IndroidDmaResolver = void* (*)(uint64_t);
static std::atomic<IndroidDmaResolver> sIndroidDmaResolver{nullptr};
extern "C" __attribute__((visibility("default")))
void indroid_set_dma_resolver(IndroidDmaResolver resolver) {
    sIndroidDmaResolver.store(resolver, std::memory_order_release);
}

void* RingStream::getDmaForReading(uint64_t guest_paddr) {
    // indroid: resolved by the embedding kernel; null when unknown.
    if (auto resolve = sIndroidDmaResolver.load(std::memory_order_acquire)) {
        return resolve(guest_paddr);
    }
    return nullptr;
}

void RingStream::unlockDma(uint64_t guest_paddr) { (void)guest_paddr; }
"""
if "indroid_set_dma_resolver" not in text:
    hits = [o for o in originals if text.count(o) == 1]
    if not hits:
        raise SystemExit("unexpected RingStream DMA helpers")
    text = text.replace(hits[0], new, 1)
    if "#include <atomic>" not in text:
        text = "#include <atomic>\n" + text
    ring.write_text(text)

# A DMA update whose buffer could not be resolved must not touch memory.
rc = pathlib.Path(sys.argv[1]) / "host/RenderControl.cpp"
text = rc.read_text()
old = """                                  void* pixels, uint32_t pixels_size)
{
    FrameBuffer *fb = FrameBuffer::getFB();

    if (!fb) {"""
new = """                                  void* pixels, uint32_t pixels_size)
{
    FrameBuffer *fb = FrameBuffer::getFB();

    if (!pixels) {
        // indroid: unresolved DMA buffer.
        fprintf(stderr, "gfxstream: rcUpdateColorBufferDMA: unresolved buffer\\n");
        sGrallocSync()->unlockColorBufferPrepare();
        return -1;
    }
    if (!fb) {"""
if "indroid: unresolved DMA buffer" not in text:
    if text.count(old) != 1:
        raise SystemExit("unexpected rcUpdateColorBufferDMA body")
    rc.write_text(text.replace(old, new, 1))

# Vulkan commands carry a per-guest-process sequence number, checked against a
# counter in ProcessResources, which gfxstream keys by virtio-gpu context id.
# Android 13 guests open a render node (a new context) per connection, so one
# process spans several contexts and later streams spin forever waiting for a
# seqno their fresh counter never reaches. Let indroid map contexts to a
# per-process id; cleanup runs when the process's last context goes away.
rt = pathlib.Path(sys.argv[1]) / "host/RenderThread.cpp"
text = rt.read_text()
if "indroid_set_puid_resolver" not in text:
    marker = "namespace gfxstream {\n"
    hook = r'''#include <atomic>
// indroid: per-guest-process puid for virtio-gpu contexts.
using IndroidPuidResolver = uint64_t (*)(uint32_t, int);
static std::atomic<IndroidPuidResolver> sIndroidPuidResolver{nullptr};
extern "C" __attribute__((visibility("default")))
void indroid_set_puid_resolver(IndroidPuidResolver resolver) {
    sIndroidPuidResolver.store(resolver, std::memory_order_release);
}
// release=0: puid for the context. release=1: the context is going away;
// returns the puid to clean up, or 0 while other contexts still use it.
extern "C" uint64_t indroid_resolve_puid(uint32_t ctx, int release) {
    if (auto resolve = sIndroidPuidResolver.load(std::memory_order_acquire)) {
        return resolve(ctx, release);
    }
    return ctx;
}

'''
    if text.count(marker) < 1:
        raise SystemExit("unexpected RenderThread.cpp namespace")
    text = text.replace(marker, hook + marker, 1)
    old = "                tInfo->m_puid = mContextId;\n"
    if text.count(old) != 1:
        raise SystemExit("unexpected RenderThread puid assignment")
    text = text.replace(old, "                tInfo->m_puid = indroid_resolve_puid(mContextId, 0);\n", 1)
    rt.write_text(text)

vc = pathlib.Path(sys.argv[1]) / "host/VirtioGpuContext.cpp"
text = vc.read_text()
if "indroid_resolve_puid" not in text:
    marker = "namespace gfxstream {\n"
    old = "    android_cleanupProcGLObjects(mId);\n"
    if text.count(marker) < 1 or text.count(old) != 1:
        raise SystemExit("unexpected VirtioGpuContext.cpp")
    text = text.replace(marker, 'extern "C" uint64_t indroid_resolve_puid(uint32_t ctx, int release);\n\n' + marker, 1)
    text = text.replace(old, "    if (uint64_t puid = indroid_resolve_puid(mId, 1)) {\n        android_cleanupProcGLObjects(puid);\n    }\n", 1)
    vc.write_text(text)

# ProcessResources are created per context at context creation; key them by
# the resolved per-process puid as well (one shared entry per guest process).
fbp = pathlib.Path(sys.argv[1]) / "host/FrameBuffer.cpp"
text = fbp.read_text()
if "indroid: per-process ProcessResources" not in text:
    old = """void FrameBuffer::createGraphicsProcessResources(uint64_t puid) {
    bool inserted = false;
    {
        AutoLock mutex(m_procOwnedResourcesLock);
        inserted = m_procOwnedResources.try_emplace(puid, ProcessResources::create()).second;
    }
    if (!inserted) {
        WARN("Failed to create process resource for puid %" PRIu64 ".", puid);
    }
}"""
    new = """void FrameBuffer::createGraphicsProcessResources(uint64_t puid) {
    // indroid: per-process ProcessResources; later contexts of the same guest
    // process reuse the entry.
    if (puid <= 0xffffffffull) {
        puid = indroid_resolve_puid(static_cast<uint32_t>(puid), 0);
    }
    AutoLock mutex(m_procOwnedResourcesLock);
    if (m_procOwnedResources.find(puid) == m_procOwnedResources.end()) {
        m_procOwnedResources.emplace(puid, ProcessResources::create());
    }
}"""
    old2 = """        auto i = m_procOwnedResources.find(puid);
        if (i != m_procOwnedResources.end()) {
            return i->second.get();
        }
    }
    ERR("Failed to find process owned resources for puid %" PRIu64 ".", puid);
    return nullptr;"""
    new2 = """        auto i = m_procOwnedResources.find(puid);
        if (i != m_procOwnedResources.end()) {
            return i->second.get();
        }
        // indroid: create on first use (puid resolved per guest process).
        return m_procOwnedResources.emplace(puid, ProcessResources::create()).first->second.get();
    }"""
    if text.count(old) != 1 or text.count(old2) != 1:
        raise SystemExit("unexpected FrameBuffer ProcessResources code")
    text = text.replace(old, new, 1).replace(old2, new2, 1)
    text = text.replace("namespace gfxstream {\n", 'extern "C" uint64_t indroid_resolve_puid(uint32_t ctx, int release);\n\nnamespace gfxstream {\n', 1)
    fbp.write_text(text)
