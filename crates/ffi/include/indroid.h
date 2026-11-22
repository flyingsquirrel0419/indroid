#ifndef INDROID_H
#define INDROID_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct IndroidVm IndroidVm;
// Callback may be called from any thread running a VM. NULL restores the fallback.
// out receives width and height; flags: bold=1, italic=2, monospace=4,
// max lines in bits 8..15 (0 means unlimited). utf8 is valid during the call.
typedef void (*indroid_measure_text_fn)(const char *utf8, float size, int32_t flags,
                                       float max_width, float out[2]);
void indroid_set_text_measurer(indroid_measure_text_fn fn);
IndroidVm *indroid_vm_new(const uint8_t *apk, size_t len, char **err);
// Directory for app-private data (SharedPreferences); call before launch.
// Data goes to <dir>/<package>/. Returns "{}".
char *indroid_vm_set_data_dir(IndroidVm *vm, const char *dir);
char *indroid_vm_launch(IndroidVm *vm);
char *indroid_vm_set_window_size(IndroidVm *vm, int32_t w, int32_t h);
char *indroid_vm_set_size(IndroidVm *vm, int32_t view_id, int32_t w, int32_t h);
char *indroid_vm_touch(IndroidVm *vm, int32_t view_id, int32_t action, float x, float y);
// Touch in window (content area) coordinates, dispatched through the view tree
// like Android (onInterceptTouchEvent, touch targets). action as above.
char *indroid_vm_touch_window(IndroidVm *vm, int32_t action, float x, float y);
// A native scroll view (ScrollView node) scrolled to (x, y): keeps Java's scroll
// offset in sync for hit testing. Returns "{}" (no UI change).
char *indroid_vm_scroll(IndroidVm *vm, int32_t view_id, int32_t x, int32_t y);
char *indroid_vm_click(IndroidVm *vm, int32_t view_id);
char *indroid_vm_set_text(IndroidVm *vm, int32_t view_id, const char *utf8);
char *indroid_vm_back(IndroidVm *vm);
// Long press on a view.
char *indroid_vm_long_click(IndroidVm *vm, int32_t view_id);
// Return/Done key in an EditText.
char *indroid_vm_editor_action(IndroidVm *vm, int32_t view_id);
// User moved a SeekBar.
char *indroid_vm_set_progress(IndroidVm *vm, int32_t view_id, int32_t progress);
// Timer callback: the UI JSON's "next" field (ms, -1 = none) says when to call.
char *indroid_vm_tick(IndroidVm *vm);
char *indroid_vm_take_log(IndroidVm *vm);
char *indroid_vm_info(IndroidVm *vm);
// PNG/WebP/JPEG bytes of a drawable resource (the UI JSON's "src"); NULL if none.
uint8_t *indroid_vm_resource_bytes(IndroidVm *vm, int32_t id, size_t *len);
void indroid_bytes_free(uint8_t *ptr, size_t len);
void indroid_string_free(char *s);
void indroid_vm_free(IndroidVm *vm);

/* ---- indroid neo: real Android (crates/ffi/src/neo.rs) ---- */
typedef void (*IndroidNeoTextFn)(void *ctx, const uint8_t *text, size_t len);
/* Installs an Android arm64 system image zip into `out` (blocking). 0 = ok. */
int32_t indroid_neo_install(const char *zip, const char *out, const char *work, IndroidNeoTextFn progress, void *ctx);
/* Boots /system/bin/init from `rom`; `overlays` is ':'-separated; `cmdline`
   may be NULL. Console + logcat lines are passed to `log`. 0 = started. */
int32_t indroid_neo_boot(const char *rom, const char *state, const char *overlays, const char *cmdline, IndroidNeoTextFn log, void *ctx);
void indroid_neo_touch(int32_t slot, int32_t id, int32_t x, int32_t y);
uint64_t indroid_neo_instructions(void);

#ifdef __cplusplus
}
#endif
#endif
