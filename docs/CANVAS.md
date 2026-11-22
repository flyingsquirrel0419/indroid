# Custom views: Canvas, size and touch

Contract between the Java framework shim (`core/src/android`), the Rust FFI
(`crates/ffi`) and the iOS host (`ios/Sources`).

## Units

1 Android px == 1 dp == 1 iOS point (`DisplayMetrics.density == 1`). The host
draws in points; Core Graphics handles Retina scaling.

## Which views are "canvas" views

A view is serialized with `"draw": [...]` when its class overrides
`View.onDraw(Canvas)` (checked by the native
`android.view.VMViews.overridesOnDraw(View)`, which compares the resolved
virtual method's declaring class against `android/view/View`) **or** when it
is a stock widget we draw ourselves later. Its `type` stays whatever
`indroidType()` returns (`"View"` for plain custom views; a `ViewGroup` that
overrides onDraw keeps its group type and the drawing is painted behind its
children).

## Size

The host measures views; Java cannot. After layout the host reports each
canvas view's size:

```c
// Report the laid-out size (points) of a view. Returns new UI JSON.
char *indroid_vm_set_size(IndroidVm *vm, int32_t view_id, int32_t w, int32_t h);
```

Java side: `ActivityThread.setSize(int uid, int w, int h)` stores
`mWidth/mHeight` on the view, calls `onSizeChanged(w, h, oldw, oldh)` when
changed and `getWidth()/getHeight()/getMeasuredWidth()/getMeasuredHeight()`
return them (also `getRight() == w`, `getBottom() == h`). The host calls it
only when the size changed. Until a size is known the view is drawn with
size 0 (so the first JSON may have `"draw": []`).

## Drawing ops

On every UI dump, for each visible canvas view with size > 0, the framework
calls `onDraw(canvas)` on a fresh recording `android.graphics.Canvas` whose
`getWidth()/getHeight()` are the view size. Ops are emitted as a JSON array
of arrays, first element the op name. Colors are `"#aarrggbb"` strings.
`paint` is encoded inline as the trailing fields
`color, style, strokeWidth` where style is `0` fill, `1` stroke,
`2` fill-and-stroke. Numbers are floats (integers allowed).

| op | fields |
|---|---|
| `["save"]` / `["restore"]` | push/pop graphics state (transform + clip) |
| `["translate", dx, dy]` | |
| `["scale", sx, sy]` | |
| `["rotate", degrees]` | around the origin (pivot form is emitted as translate/rotate/translate) |
| `["concat", a, b, c, d, tx, ty]` | concatenate affine transform x′ = a·x + c·y + tx, y′ = b·x + d·y + ty |
| `["clip", l, t, r, b]` | intersect clip with rect |
| `["color", color]` | fill whole clip (`drawColor`, `drawRGB`, `drawARGB`) |
| `["rect", l, t, r, b, color, style, sw]` | `drawRect` |
| `["rrect", l, t, r, b, rx, ry, color, style, sw]` | `drawRoundRect` |
| `["oval", l, t, r, b, color, style, sw]` | `drawOval`; `drawCircle(cx,cy,r)` → oval |
| `["arc", l, t, r, b, start, sweep, useCenter, color, style, sw]` | degrees, clockwise like Android |
| `["line", x0, y0, x1, y1, color, sw, cap]` | `drawLine`/`drawLines`; cap `0` butt `1` round `2` square |
| `["path", "<svg-path>", color, style, sw, fillType]` | path as SVG-like commands `M x y L x y Q x1 y1 x y C x1 y1 x2 y2 x y Z`; fillType `0` winding `1` even-odd |
| `["text", "str", x, y, color, size, align, bold]` | baseline at y; align `0` left `1` center `2` right |
| `["bitmap", resId, l, t, r, b, alpha]` | drawable resource drawn into dst rect (host loads via `indroid_vm_resource_bytes`) |

Anti-aliasing is always on. Unknown ops are ignored by the host.

`Paint.measureText(s)` uses an approximation table (average glyph advance
`0.55 * textSize`, digits `0.56`, space `0.28`, narrow letters `0.3`); good
enough for centering labels.

## Invalidation and animation

`View.invalidate()` / `postInvalidate()` / `postInvalidateOnAnimation()`
mark the view dirty and request a frame. `ActivityThread.dump()` reports
`"next": 16` (or the earlier Handler deadline) whenever any canvas view was
invalidated since the previous dump (including during its own onDraw, which
is how game loops are written), so the host's tick timer redraws at ~60 fps.
`postOnAnimation(r)` is `postDelayed(r, 16)`.

## Touch

```c
// action: 0 down, 1 up, 2 move, 3 cancel; x/y in view-local points.
char *indroid_vm_touch(IndroidVm *vm, int32_t view_id, int32_t action, float x, float y);
```

Java: `ActivityThread.touch(int uid, int action, float x, float y)` builds a
`MotionEvent` and calls `view.dispatchTouchEvent(e)`, which calls the
`OnTouchListener` first and then `onTouchEvent(e)`. The default
`View.onTouchEvent` performs a click on `ACTION_UP` if the view is clickable.

The host forwards touches only for views whose JSON has `"touch": true`,
emitted when the view has an `OnTouchListener` or its class overrides
`onTouchEvent(MotionEvent)` (native `VMViews.overridesOnTouch(View)`). Other
views keep the plain tap → `indroid_vm_click` path.

## Host rendering

`CanvasView: UIView` with `draw(_:)` replaying the ops on the current
`CGContext` (state stack via `saveGState`), `isOpaque = false`, redraw via
`setNeedsDisplay()` when the op array changed. Reports its bounds through
`indroid_vm_set_size` from `layoutSubviews` when they change. If a canvas
view's layout width/height is `wrap`, the host gives it 0 intrinsic size
unless `minWidth`/`minHeight` are set; `match`/weight/dp sizes work normally.
