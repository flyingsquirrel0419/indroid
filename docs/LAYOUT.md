# Layout v2: Android measure/layout in Java, host places frames

Status: in progress (v0.3).

## Why

v0.1/v0.2 let the iOS host lay out the tree with `UIStackView`s. That only
works for the handful of layouts we re-implemented. Real apps ship their own
`ViewGroup`s inside the APK (`ConstraintLayout`, `RecyclerView`,
`CoordinatorLayout`, `MaterialCardView`, ...) and those implement
`onMeasure`/`onLayout` themselves. To run them unmodified, the framework must
do what Android does: a real measure/layout pass in Java. The host then only
places native views at the frames Java computed.

## Pass

On every UI dump `ActivityThread` runs, on the decor view:

1. `measure(EXACTLY windowW, EXACTLY windowH)` when a layout was requested
   (`requestLayout()` anywhere sets a flag; first dump always lays out).
2. `layout(0, 0, w, h)`.
3. serialization with frames.

`View.measure` caches by (widthSpec, heightSpec) and the `PFLAG_FORCE_LAYOUT`
bit like Android, so unchanged subtrees are skipped.

## Text metrics

Only the host knows real font metrics. The VM asks through
`Vm::text_measurer` (set by the FFI from a host callback):

```c
typedef void (*indroid_measure_text_fn)(const char *utf8, float size, int32_t flags,
                                       float max_width, float out[2]);
// out = { width, height }; flags bit0 bold, bit1 italic, bit2 monospace,
// bits 8..15 maxLines (0 = unlimited)
void indroid_set_text_measurer(indroid_measure_text_fn fn);
```

Without a callback (CLI, tests) a deterministic approximation is used
(average advance `0.55 * size`, line height `1.17 * size`, word wrap).

## Window size

```c
// Content area in points (safe area already removed). Returns new UI JSON.
char *indroid_vm_set_window_size(IndroidVm *vm, int32_t w, int32_t h);
```

Default 390 x 760 until the host reports.

## JSON

Every node gets `"frame": [left, top, width, height]` in points, relative to
its parent's top-left (scroll offset not applied). Scroll containers add
`"content": [w, h]` and keep native scrolling; the host reports scroll offsets
back with `indroid_vm_scroll(vm, id, x, y)` (optional, for `getScrollY()`).

The existing per-type properties (`text`, `textSize`, `color`, `bg`, `draw`,
...) stay. Layout-only properties (`width`, `height`, `weight`, `margin`,
`layoutGravity`, `orientation`) are no longer needed by the host and may be
dropped later.

## Host

A node renders as a native view (UILabel/UIButton/UITextField/UIImageView/...)
with `frame` applied directly. No Auto Layout, no stack views. Canvas ops and
background drawables are painted behind content.
